//! Spike R1: a synthetic scan stream with a realistic shape, to measure the model's memory.

use std::path::Path;

use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{Kind, NodeId};

/// Shape of the synthetic tree.
#[derive(Clone, Copy, Debug)]
pub struct Shape {
    /// Fraction of entries that are directories.
    pub dir_fraction: f64,
    /// Fraction of files that are one of two links to the same inode.
    pub hardlink_fraction: f64,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            dir_fraction: 0.10,
            hardlink_fraction: 0.005,
        }
    }
}

/// Distribution of name lengths, as a cumulative table over lengths 1..=255.
pub struct NameHist {
    cumulative: Vec<u64>,
}

impl NameHist {
    fn from_weights(weights: &[u64]) -> NameHist {
        let mut sum = 0;
        let cumulative = weights
            .iter()
            .map(|w| {
                sum += w;
                sum
            })
            .collect();
        NameHist { cumulative }
    }

    /// A log-normal-looking shape: most names 5-20 bytes, a tail up to 255.
    pub fn builtin() -> NameHist {
        let weights: Vec<u64> = (1..=255u32)
            .map(|len| {
                let x = (f64::from(len).ln() - 12f64.ln()) / 0.6;
                (1_000_000.0 * (-x * x / 2.0).exp() / f64::from(len)).max(1.0) as u64
            })
            .collect();
        NameHist::from_weights(&weights)
    }

    /// Reads names (no `stat`, no file contents) below `root`, breadth first, up to
    /// `limit` entries, and keeps only the length histogram.
    pub fn from_dir(root: &Path, limit: usize) -> NameHist {
        let mut weights = vec![0u64; 255];
        let mut seen = 0;
        let mut queue = std::collections::VecDeque::from([root.to_path_buf()]);
        while let Some(dir) = queue.pop_front() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let len =
                    std::os::unix::ffi::OsStrExt::as_bytes(entry.file_name().as_os_str()).len();
                weights[len.clamp(1, 255) - 1] += 1;
                seen += 1;
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    queue.push_back(entry.path());
                }
                if seen >= limit {
                    return NameHist::from_weights(&weights);
                }
            }
        }
        if seen == 0 {
            return NameHist::builtin();
        }
        NameHist::from_weights(&weights)
    }

    /// One weight per length 1..=255, whitespace separated.
    pub fn to_text(&self) -> String {
        let mut prev = 0;
        let weights: Vec<String> = self
            .cumulative
            .iter()
            .map(|&c| {
                let w = c - prev;
                prev = c;
                w.to_string()
            })
            .collect();
        weights.join(" ")
    }

    pub fn from_text(text: &str) -> Option<NameHist> {
        let weights: Option<Vec<u64>> = text.split_whitespace().map(|w| w.parse().ok()).collect();
        let weights = weights.filter(|w| w.len() == 255 && w.iter().sum::<u64>() > 0)?;
        Some(NameHist::from_weights(&weights))
    }

    pub fn mean_len(&self) -> f64 {
        let mut prev = 0;
        let mut sum = 0.0;
        for (i, &c) in self.cumulative.iter().enumerate() {
            sum += (i + 1) as f64 * (c - prev) as f64;
            prev = c;
        }
        sum / prev as f64
    }

    fn sample(&self, rng: &mut Rng) -> usize {
        let total = *self.cumulative.last().unwrap_or(&1);
        let x = rng.below(total.max(1));
        self.cumulative.partition_point(|&c| c <= x) + 1
    }
}

/// splitmix64: small, fast, deterministic.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Log-uniform in `lo..=hi`.
    fn log_uniform(&mut self, lo: f64, hi: f64) -> u64 {
        (lo * (hi / lo).powf(self.unit())).round().clamp(lo, hi) as u64
    }
}

/// What the stream produced, for the report.
#[derive(Default)]
pub struct Totals {
    pub entries: u64,
    pub dirs: u64,
    pub name_bytes: u64,
    pub hardlink_pairs: u64,
}

/// The messages of a tree of `entries` entries (the root excluded), parent before child,
/// then `Done`. Pending directories are picked at random.
pub struct Stream {
    remaining: u64,
    shape: Shape,
    hist: NameHist,
    rng: Rng,
    stack: Vec<NodeId>,
    next_id: u32,
    link_candidates: u64,
    totals: Totals,
    finished: bool,
}

impl Stream {
    pub fn new(entries: u64, shape: Shape, hist: NameHist, seed: u64) -> Stream {
        Stream {
            remaining: entries,
            shape,
            hist,
            rng: Rng(seed),
            stack: vec![NodeId::ROOT],
            next_id: 1,
            link_candidates: 0,
            totals: Totals::default(),
            finished: false,
        }
    }

    pub fn totals(&self) -> &Totals {
        &self.totals
    }

    /// A random pending directory, so the tree stays shallow like a real one (depth about
    /// log N); always taking the newest would build chains thousands of levels deep.
    fn pick_pending(&mut self) -> Option<NodeId> {
        if self.stack.is_empty() {
            return None;
        }
        let at = self.rng.below(self.stack.len() as u64) as usize;
        Some(self.stack.swap_remove(at))
    }

    /// Skewed fan-out: many empty or tiny directories, a few huge ones.
    fn fanout(&mut self) -> u64 {
        match self.rng.below(1000) {
            0..=399 => 0,
            400..=749 => 1 + self.rng.below(5),
            750..=949 => self.rng.log_uniform(6.0, 40.0),
            950..=994 => self.rng.log_uniform(41.0, 500.0),
            _ => self.rng.log_uniform(501.0, 5000.0),
        }
    }

    fn name(&mut self) -> Box<[u8]> {
        let len = self.hist.sample(&mut self.rng);
        const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789_-";
        let mut name: Vec<u8> = (0..len)
            .map(|_| CHARS[self.rng.below(CHARS.len() as u64) as usize])
            .collect();
        if len > 5 && self.rng.below(2) == 0 {
            let exts: [&[u8]; 5] = [b".rs", b".txt", b".png", b".log", b".mp4"];
            let ext = exts[self.rng.below(5) as usize];
            name[len - ext.len()..].copy_from_slice(ext);
        }
        self.totals.name_bytes += name.len() as u64;
        name.into()
    }

    fn entry(&mut self, is_dir: bool) -> RawEntry {
        let name = self.name();
        if is_dir {
            return RawEntry {
                name,
                kind: Kind::Dir,
                disk_bytes: 4096,
                apparent_bytes: 0,
                ino: 0,
                dev: 1,
                nlink: 2,
                issue: None,
            };
        }
        let apparent = self.rng.log_uniform(1.0, 4_000_000.0);
        let (mut ino, mut nlink) = (2_000_000_000 + self.totals.entries, 1);
        if self.rng.unit() < self.shape.hardlink_fraction {
            self.link_candidates += 1;
            ino = 10_000 + self.link_candidates / 2;
            nlink = 2;
            self.totals.hardlink_pairs += self.link_candidates % 2;
        }
        RawEntry {
            name,
            kind: Kind::File,
            disk_bytes: apparent.div_ceil(4096) * 4096,
            apparent_bytes: apparent,
            ino,
            dev: 1,
            nlink,
            issue: None,
        }
    }
}

impl Iterator for Stream {
    type Item = ScanMsg;

    fn next(&mut self) -> Option<ScanMsg> {
        if self.finished {
            return None;
        }
        let Some(dir) = self.pick_pending() else {
            self.finished = true;
            return Some(ScanMsg::Done { cancelled: false });
        };
        let last_pending = self.stack.is_empty();
        let mut count = self.fanout().min(self.remaining);
        if last_pending && self.remaining > 0 {
            count = count.max(1);
        }
        let first_id = self.next_id;
        let mut entries = Vec::with_capacity(count as usize);
        let mut any_dir = false;
        for i in 0..count {
            let must_continue =
                last_pending && !any_dir && i + 1 == count && self.remaining > count;
            let is_dir = must_continue || self.rng.unit() < self.shape.dir_fraction;
            any_dir |= is_dir;
            if is_dir {
                self.stack.push(NodeId(first_id + i as u32));
                self.totals.dirs += 1;
            }
            entries.push(self.entry(is_dir));
            self.totals.entries += 1;
        }
        self.next_id += count as u32;
        self.remaining -= count;
        Some(ScanMsg::Listing {
            dir,
            first_id: NodeId(first_id),
            entries,
        })
    }
}

pub fn root_entry() -> RawEntry {
    RawEntry {
        name: b"root".as_slice().into(),
        kind: Kind::Dir,
        disk_bytes: 4096,
        apparent_bytes: 0,
        ino: 1,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use storage_analyzer::model::{Model, ModelOptions};
    use storage_analyzer::types::{Kind, NodeId};

    use super::*;

    fn model() -> Model {
        Model::new(
            PathBuf::from("/synthetic"),
            root_entry(),
            ModelOptions { top_n: 100 },
        )
    }

    #[test]
    fn a_small_stream_is_valid_and_has_the_requested_shape() {
        const N: u64 = 10_000;
        let mut stream = Stream::new(N, Shape::default(), NameHist::builtin(), 42);
        let mut known: HashSet<NodeId> = HashSet::from([NodeId::ROOT]);
        let mut m = model();
        let mut entries = 0u64;
        let mut dirs = 0u64;
        for msg in stream.by_ref() {
            if let ScanMsg::Listing {
                dir,
                first_id,
                entries: es,
            } = &msg
            {
                assert!(known.contains(dir), "{dir:?} listed before its parent");
                for (i, e) in es.iter().enumerate() {
                    entries += 1;
                    if e.kind == Kind::Dir {
                        dirs += 1;
                        known.insert(NodeId(first_id.0 + i as u32));
                    }
                }
            }
            m.apply(msg);
        }
        assert_eq!(entries, N);
        assert_eq!(m.totals().entries_seen, N);
        assert!(m.is_complete(NodeId::ROOT), "every directory was listed");
        assert_eq!(m.totals().pending_dirs, 0);
        let fraction = dirs as f64 / N as f64;
        assert!(
            (0.07..0.13).contains(&fraction),
            "directory fraction {fraction}"
        );
        assert_eq!(stream.totals().entries, N);
        assert_eq!(stream.totals().dirs, dirs);
        let (files, ndirs) = m.counts(NodeId::ROOT);
        assert_eq!((files + ndirs, ndirs), (N, dirs));
    }

    #[test]
    fn the_tree_is_shallow_like_a_real_one() {
        let mut m = model();
        let mut deepest = 0;
        for msg in Stream::new(200_000, Shape::default(), NameHist::builtin(), 9) {
            if let ScanMsg::Listing { dir, .. } = &msg {
                deepest = deepest.max(m.depth(*dir));
            }
            m.apply(msg);
        }
        assert!(deepest < 80, "deepest directory at depth {deepest}");
    }

    #[test]
    fn the_stream_is_deterministic_for_a_seed() {
        let make = |seed| {
            Stream::new(2000, Shape::default(), NameHist::builtin(), seed)
                .map(|m| match m {
                    ScanMsg::Listing { entries, .. } => {
                        entries.iter().map(|e| e.name.to_vec()).collect::<Vec<_>>()
                    }
                    _ => vec![],
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(make(1), make(1));
        assert_ne!(make(1), make(2));
    }

    #[test]
    fn name_lengths_follow_the_histogram() {
        let hist = NameHist::builtin();
        let mean = hist.mean_len();
        assert!((6.0..40.0).contains(&mean), "mean name length {mean}");
        let stream = Stream::new(5000, Shape::default(), NameHist::builtin(), 3);
        let mut total = 0usize;
        let mut count = 0usize;
        for m in stream {
            if let ScanMsg::Listing { entries, .. } = m {
                for e in entries {
                    assert!(!e.name.is_empty() && e.name.len() <= 255);
                    total += e.name.len();
                    count += 1;
                }
            }
        }
        let got = total as f64 / count as f64;
        assert!((got - mean).abs() < mean * 0.25, "{got} vs {mean}");
    }

    #[test]
    fn a_histogram_survives_a_round_trip_through_text() {
        let hist = NameHist::builtin();
        let back = NameHist::from_text(&hist.to_text()).unwrap();
        assert_eq!(back.to_text(), hist.to_text());
        assert!(NameHist::from_text("1 2 3").is_none());
    }

    #[test]
    fn a_histogram_can_be_read_from_a_real_directory() {
        let dir = std::env::temp_dir().join(format!("r1-hist-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..50 {
            std::fs::write(dir.join(format!("file_with_a_longish_name_{i}.txt")), b"").unwrap();
        }
        let hist = NameHist::from_dir(&dir, 1000);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(hist.mean_len() > 20.0, "{}", hist.mean_len());
    }
}
