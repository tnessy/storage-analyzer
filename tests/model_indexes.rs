//! Extension, category and top-N indexes (T005): live correctness against brute force.

use std::collections::HashMap;
use std::path::PathBuf;

use storage_analyzer::model::{Category, ExtKey, ExtTotals, Model, ModelOptions, ext_of};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{Kind, NodeId, SizeMode};

const D: SizeMode = SizeMode::Disk;
const A: SizeMode = SizeMode::Apparent;

fn ext(s: &str) -> ExtKey {
    ExtKey::Ext(s.as_bytes().into())
}

#[test]
fn ext_of_follows_the_rules() {
    let cases: &[(&[u8], ExtKey)] = &[
        (b"photo.jpg", ext("jpg")),
        (b"photo.JPG", ext("jpg")),
        (b"Mixed.TaR.Gz", ext("gz")),
        (b"a.tar.gz", ext("gz")),
        (b".bashrc", ExtKey::NoExt),
        (b".config.json", ext("json")),
        (b"Makefile", ExtKey::NoExt),
        (b"trailing.", ExtKey::NoExt),
        (b"", ExtKey::NoExt),
        (b".", ExtKey::NoExt),
        (b"x.0123456789abcdef", ext("0123456789abcdef")),
        (b"x.0123456789abcdefg", ExtKey::Other),
        (b"bad.\xff\xfe", ExtKey::Ext(b"\xff\xfe".as_slice().into())),
        (b"bad\xff.TXT", ext("txt")),
    ];
    for (name, expected) in cases {
        assert_eq!(
            &ext_of(name),
            expected,
            "{:?}",
            String::from_utf8_lossy(name)
        );
    }
}

#[test]
fn category_table_covers_the_documented_groups() {
    use Category::*;
    let cases = [
        ("mp4", Video),
        ("mkv", Video),
        ("jpg", Image),
        ("heic", Image),
        ("zip", Archive),
        ("zst", Archive),
        ("rs", CodeText),
        ("md", CodeText),
        ("so", Binary),
        ("wasm", Binary),
        ("pdf", Documents),
        ("xlsx", Documents),
        ("mp3", AudioOther),
        ("flac", AudioOther),
        ("zzzunknown", AudioOther),
    ];
    for (e, cat) in cases {
        assert_eq!(Model::category_of(&ext(e)), cat, "{e}");
    }
    assert_eq!(Model::category_of(&ExtKey::NoExt), Binary);
    assert_eq!(Model::category_of(&ExtKey::Other), AudioOther);
}

fn raw(name: &str, kind: Kind, disk: u64, apparent: u64) -> RawEntry {
    RawEntry {
        name: name.as_bytes().into(),
        kind,
        disk_bytes: disk,
        apparent_bytes: apparent,
        ino: 0,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

fn file(name: &str, disk: u64, apparent: u64) -> RawEntry {
    raw(name, Kind::File, disk, apparent)
}

fn dir(name: &str) -> RawEntry {
    raw(name, Kind::Dir, 4096, 0)
}

fn model(top_n: usize) -> Model {
    Model::new(PathBuf::from("/r"), dir("r"), ModelOptions { top_n })
}

fn listing(dir: u32, first_id: u32, entries: Vec<RawEntry>) -> ScanMsg {
    ScanMsg::Listing {
        dir: NodeId(dir),
        first_id: NodeId(first_id),
        entries,
        last: true,
    }
}

/// Deterministic pseudo-random numbers (xorshift) so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const NAMES: &[&str] = &[
    "a.mp4",
    "b.JPG",
    "c.zip",
    "d.rs",
    "e.so",
    "f.pdf",
    "g.mp3",
    "h",
    ".hidden",
    "i.tar.gz",
    "j.verylongextensionname",
    "k.txt",
    "l.xyz",
];

/// A random tree streamed in breadth-first order, with some hard links.
fn random_stream(seed: u64, dirs: usize) -> Vec<ScanMsg> {
    let mut rng = Rng(seed);
    let mut msgs = Vec::new();
    let mut queue = vec![0u32];
    let mut next_id = 1u32;
    let mut made = 0;
    while let Some(d) = queue.pop() {
        let n = 2 + rng.below(6);
        let mut entries = Vec::new();
        for i in 0..n {
            if made < dirs && rng.below(3) == 0 {
                made += 1;
                queue.push(next_id + i as u32);
                entries.push(dir(&format!("dir{made}")));
            } else {
                let name = NAMES[rng.below(NAMES.len() as u64) as usize];
                let mut e = file(
                    &format!("{i}{name}"),
                    rng.below(50) * 4096,
                    rng.below(200_000),
                );
                if rng.below(5) == 0 {
                    e.ino = 1 + rng.below(4);
                    e.nlink = 3;
                }
                entries.push(e);
            }
        }
        msgs.push(listing(d, next_id, entries.clone()));
        next_id += entries.len() as u32;
    }
    msgs.push(ScanMsg::Done { cancelled: false });
    msgs
}

fn max_id(msgs: &[ScanMsg]) -> u32 {
    msgs.iter()
        .filter_map(|m| match m {
            ScanMsg::Listing {
                first_id, entries, ..
            } => Some(first_id.0 + entries.len() as u32),
            _ => None,
        })
        .max()
        .unwrap_or(1)
}

/// Every counted file: not a directory, not a hard-link duplicate, no issue.
fn counted_files(m: &Model, up_to: u32) -> Vec<(NodeId, Vec<u8>, u64, u64)> {
    (1..up_to)
        .map(NodeId)
        .filter(|&id| m.exists(id) && m.kind(id) != Kind::Dir)
        .filter(|&id| {
            let f = m.flags(id);
            !f.contains(storage_analyzer::model::Flags::HARDLINK_DUP)
                && !f.contains(storage_analyzer::model::Flags::ERROR)
                && !f.contains(storage_analyzer::model::Flags::OTHER_FS)
        })
        .map(|id| (id, m.name(id).to_vec(), m.size(id, D), m.size(id, A)))
        .collect()
}

fn brute_ext(files: &[(NodeId, Vec<u8>, u64, u64)]) -> HashMap<ExtKey, ExtTotals> {
    let mut out: HashMap<ExtKey, ExtTotals> = HashMap::new();
    for (_, name, disk, apparent) in files {
        let t = out.entry(ext_of(name)).or_default();
        t.disk_bytes += disk;
        t.apparent_bytes += apparent;
        t.count += 1;
    }
    out
}

fn brute_top(files: &[(NodeId, Vec<u8>, u64, u64)], mode: SizeMode, n: usize) -> Vec<NodeId> {
    let mut v: Vec<_> = files
        .iter()
        .map(|(id, _, d, a)| (*id, if mode == D { *d } else { *a }))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v.into_iter().map(|(id, _)| id).collect()
}

fn check_all(m: &Model, up_to: u32, n: usize) {
    let files = counted_files(m, up_to);

    let expected = brute_ext(&files);
    let got = m.ext_totals();
    assert_eq!(got.len(), expected.len());
    for (key, totals) in &got {
        assert_eq!(Some(totals), expected.get(key), "{key:?}");
    }
    assert!(
        got.windows(2)
            .all(|w| (w[0].1.disk_bytes, std::cmp::Reverse(&w[0].0))
                >= (w[1].1.disk_bytes, std::cmp::Reverse(&w[1].0))),
        "sorted by disk bytes desc, then key asc"
    );

    let mut by_cat: HashMap<Category, ExtTotals> = HashMap::new();
    for (key, t) in &expected {
        let c = by_cat.entry(Model::category_of(key)).or_default();
        c.disk_bytes += t.disk_bytes;
        c.apparent_bytes += t.apparent_bytes;
        c.count += t.count;
    }
    let cats = m.category_totals();
    assert_eq!(cats.len(), by_cat.len());
    for (cat, t) in &cats {
        assert_eq!(Some(t), by_cat.get(cat), "{cat:?}");
    }
    assert!(
        cats.windows(2)
            .all(|w| w[0].1.disk_bytes >= w[1].1.disk_bytes)
    );

    for mode in [D, A] {
        assert_eq!(
            m.top_files(mode),
            brute_top(&files, mode, n),
            "{mode:?} n={n}"
        );
    }
}

#[test]
fn totals_and_top_lists_are_correct_after_every_message() {
    for n in [1, 3, 100] {
        for seed in [7, 99, 12345] {
            let msgs = random_stream(seed, 25);
            let up_to = max_id(&msgs);
            let mut m = model(n);
            check_all(&m, up_to, n);
            for msg in msgs {
                m.apply(msg);
                check_all(&m, up_to, n);
            }
        }
    }
}

#[test]
fn recompute_top_matches_the_incremental_lists() {
    let msgs = random_stream(42, 30);
    let mut m = model(5);
    for msg in msgs {
        m.apply(msg);
    }
    let (disk, apparent) = (m.top_files(D), m.top_files(A));
    m.recompute_top();
    assert_eq!(m.top_files(D), disk);
    assert_eq!(m.top_files(A), apparent);
}

#[test]
fn top_n_is_honored() {
    let mut m = model(5);
    m.apply(listing(
        0,
        1,
        (0..3)
            .map(|i| file(&format!("f{i}"), 4096 * (i + 1), 1))
            .collect(),
    ));
    assert_eq!(
        m.top_files(D).len(),
        3,
        "fewer files than N returns all of them"
    );
    m.apply(ScanMsg::Done { cancelled: false });

    let mut m = model(5);
    m.apply(listing(
        0,
        1,
        (0..9)
            .map(|i| file(&format!("f{i}"), 4096 * (i + 1), 1))
            .collect(),
    ));
    let top = m.top_files(D);
    assert_eq!(
        top,
        vec![NodeId(9), NodeId(8), NodeId(7), NodeId(6), NodeId(5)]
    );
}

#[test]
fn ties_break_by_node_id() {
    let mut m = model(3);
    m.apply(listing(
        0,
        1,
        (0..6).map(|i| file(&format!("f{i}"), 8192, 1)).collect(),
    ));
    assert_eq!(m.top_files(D), vec![NodeId(1), NodeId(2), NodeId(3)]);
}

#[test]
fn hard_link_duplicates_and_other_filesystem_entries_are_not_counted() {
    let mut m = model(10);
    let mut a = file("a.bin", 4096, 100);
    a.ino = 5;
    a.nlink = 2;
    let mut b = file("b.bin", 4096, 100);
    b.ino = 5;
    b.nlink = 2;
    let mut mount = dir("mnt");
    mount.issue = Some((
        storage_analyzer::types::Op::Stat,
        storage_analyzer::types::IssueKind::OtherFilesystem,
    ));
    m.apply(listing(0, 1, vec![a, b, mount, file("c.bin", 4096, 1)]));
    let totals = m.ext_totals();
    assert_eq!(totals.len(), 1);
    assert_eq!(totals[0].0, ext("bin"));
    assert_eq!(totals[0].1.count, 2, "the second link is a duplicate");
    assert_eq!(m.top_files(D), vec![NodeId(1), NodeId(4)]);
}

#[test]
fn files_take_their_category_from_the_extension() {
    let mut m = model(10);
    m.apply(listing(
        0,
        1,
        vec![file("clip.mp4", 100, 1), file("notes", 100, 1)],
    ));
    assert_eq!(m.category_of_node(NodeId(1)), Category::Video);
    assert_eq!(m.category_of_node(NodeId(2)), Category::Binary);
}

// ---- resumable top-N rebuild (T030) ------------------------------------------

use std::time::Duration;

fn big_model(files: u32) -> Model {
    let mut m = model(20);
    m.apply(listing(0, 1, vec![dir("d")]));
    let entries: Vec<RawEntry> = (0..files)
        .map(|i| {
            file(
                &format!("f{i}.dat"),
                4096 * u64::from(1 + (i * 7919) % 1000),
                u64::from(i),
            )
        })
        .collect();
    for (k, chunk) in entries.chunks(4000).enumerate() {
        m.apply(ScanMsg::Listing {
            dir: NodeId(1),
            first_id: NodeId(2 + (k as u32) * 4000),
            entries: chunk.to_vec(),
            last: false,
        });
    }
    m.apply(ScanMsg::Listing {
        dir: NodeId(1),
        first_id: NodeId(2 + files),
        entries: vec![],
        last: true,
    });
    m
}

#[test]
fn a_stepped_rebuild_gives_the_same_lists_in_bounded_steps() {
    let mut m = big_model(50_000);
    let (disk, apparent) = (m.top_files(D), m.top_files(A));
    assert!(!m.top_updating());
    let epoch = m.top_epoch();

    m.start_top_rebuild();
    assert!(m.top_updating());
    assert_eq!(m.top_files(D), disk, "the old list stays visible meanwhile");
    let mut steps = 0;
    while !m.step_top_rebuild(Duration::ZERO) {
        steps += 1;
        assert!(steps < 1000, "never finishes");
    }
    assert!(
        steps >= 40,
        "work is spread over many small steps, got {steps}"
    );
    assert!(!m.top_updating());
    assert_ne!(m.top_epoch(), epoch);
    assert_eq!((m.top_files(D), m.top_files(A)), (disk, apparent));
}

#[test]
fn nodes_removed_during_a_rebuild_never_show_up() {
    let mut m = big_model(20_000);
    m.set_incremental_top(true);
    m.start_top_rebuild();
    assert!(!m.step_top_rebuild(Duration::ZERO));
    let biggest = m.top_files(D)[0];
    m.begin_rescan(NodeId(1)); // removes every file; restarts the rebuild
    assert!(!m.exists(biggest));
    assert!(
        m.top_files(D).is_empty(),
        "the old list is filtered, not shown"
    );
    while !m.step_top_rebuild(Duration::from_millis(5)) {}
    assert!(m.top_files(D).is_empty());
}

#[test]
fn a_file_listed_while_a_rebuild_runs_makes_the_new_lists() {
    let mut m = model(5);
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(
        1,
        3,
        (0..3000).map(|i| file(&format!("f{i}"), 4096, 1)).collect(),
    ));
    m.start_top_rebuild();
    assert!(!m.step_top_rebuild(Duration::ZERO));
    m.apply(listing(
        2,
        3003,
        vec![file("giant.bin", 900_000_000, 800_000_000)],
    ));
    while !m.step_top_rebuild(Duration::ZERO) {}
    assert_eq!(m.top_files(D)[0], NodeId(3003));
    assert_eq!(m.top_files(A)[0], NodeId(3003));
    assert_eq!(m.top_files(D).len(), 5);
}

#[test]
fn deletes_and_rescans_rebuild_in_steps_only_when_asked() {
    let build = || {
        let mut m = model(10);
        m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
        m.apply(listing(
            1,
            3,
            (0..100)
                .map(|i| file(&format!("f{i}"), 4096 * (i + 1), 1))
                .collect(),
        ));
        m.apply(listing(
            2,
            103,
            (0..100)
                .map(|i| file(&format!("g{i}"), 4096 * (i + 2), 1))
                .collect(),
        ));
        m
    };
    let mut blocking = build();
    blocking.begin_rescan(NodeId(1));
    assert!(
        !blocking.top_updating(),
        "by default the lists are rebuilt at once"
    );
    assert!(blocking.top_files(D).iter().all(|&n| n.0 >= 103));

    let mut stepped = build();
    stepped.set_incremental_top(true);
    stepped.begin_rescan(NodeId(1));
    assert!(stepped.top_updating());
    assert!(
        stepped.top_files(D).iter().all(|&n| stepped.exists(n)),
        "no removed node is listed"
    );
    while !stepped.step_top_rebuild(Duration::from_millis(5)) {}
    assert_eq!(stepped.top_files(D), blocking.top_files(D));
}
