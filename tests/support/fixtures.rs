//! Real-filesystem fixtures: tree builders, a live scan driver and `du` (T011; T007 extends).
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::Duration;

use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::{
    RawEntry, RealFs, ScanControl, ScanHandle, ScanMsg, ScanOptions, start,
};
use storage_analyzer::types::{Kind, NodeId, SizeMode};

/// Creates a file of `len` bytes (parents included).
pub fn write(path: impl AsRef<Path>, len: usize) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; len]).unwrap();
}

/// `du -s -B1` (disk) or `du -s -B1 --apparent-size` of `path`.
pub fn du(path: &Path, apparent: bool) -> u64 {
    let mut cmd = Command::new("du");
    cmd.args(["-s", "-B1"]);
    if apparent {
        cmd.arg("--apparent-size");
    }
    let out = cmd.arg(path).output().expect("du");
    assert!(
        out.status.success(),
        "du failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace().next().unwrap().parse().unwrap()
}

pub fn root_entry(path: &Path) -> RawEntry {
    let meta = fs::symlink_metadata(path).unwrap();
    RawEntry {
        name: path
            .file_name()
            .map_or(&b"root"[..], |n| n.as_encoded_bytes())
            .into(),
        kind: Kind::Dir,
        disk_bytes: meta.blocks() * 512,
        apparent_bytes: 0,
        ino: meta.ino(),
        dev: meta.dev(),
        nlink: meta.nlink() as u32,
        issue: None,
    }
}

pub fn new_model(root: &Path, top_n: usize) -> Model {
    Model::new(root.to_path_buf(), root_entry(root), ModelOptions { top_n })
}

/// The model's root totals equal `du` on the tree as it is now.
pub fn assert_matches_du(model: &Model, root: &Path) {
    assert_eq!(
        model.size(NodeId::ROOT, SizeMode::Disk),
        du(root, false),
        "disk bytes vs du"
    );
    assert_eq!(
        model.size(NodeId::ROOT, SizeMode::Apparent),
        du(root, true),
        "apparent vs du"
    );
}

/// A running real scan whose messages the test applies one at a time.
pub struct Scan {
    pub model: Model,
    pub rx: Receiver<ScanMsg>,
    pub handle: ScanHandle,
    pub control: Arc<ScanControl>,
    pub root: PathBuf,
}

impl Scan {
    /// `capacity` is the scan channel size; a small one keeps the walker close behind the test.
    pub fn start(root: &Path, threads: usize, capacity: usize) -> Scan {
        let control = Arc::new(ScanControl::new());
        let (tx, rx) = sync_channel(capacity);
        let root_dev = fs::metadata(root).unwrap().dev();
        let handle = start(
            Arc::new(RealFs),
            root.to_path_buf(),
            root_dev,
            ScanOptions {
                threads,
                one_filesystem: true,
            },
            Arc::clone(&control),
            tx,
            1,
        );
        Scan {
            model: new_model(root, 100),
            rx,
            handle,
            control,
            root: root.to_path_buf(),
        }
    }

    /// Applies the next scan message and returns it.
    pub fn step(&mut self) -> ScanMsg {
        let msg = self
            .rx
            .recv_timeout(Duration::from_secs(30))
            .expect("scan stalled");
        self.model.apply(msg.clone());
        msg
    }

    pub fn until_done(&mut self) {
        while !matches!(self.step(), ScanMsg::Done { .. }) {}
    }

    /// The node at `rel` (slash separated, below the root), if it is in the model.
    pub fn find(&self, rel: &str) -> Option<NodeId> {
        let mut cur = NodeId::ROOT;
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            cur = *self
                .model
                .children(cur)
                .iter()
                .find(|&&c| self.model.exists(c) && self.model.name(c) == part.as_bytes())?;
        }
        Some(cur)
    }

    /// The app-side sequence after an aborted or partial delete, or to resume a skip.
    pub fn rescan(&mut self, node: NodeId) {
        let path = self.model.path(node);
        self.control.remove_prefixes_under(&path);
        let (path, depth) = self.model.begin_rescan(node);
        self.handle.rescan(node, path, depth);
    }
}

/// Recounts the model's aggregates from the nodes that exist (every directory must own
/// 4096 disk bytes, as in hand-built streams).
pub fn check_exact(m: &Model, max_id: u32) {
    use storage_analyzer::model::Flags;
    let live: Vec<NodeId> = (0..max_id).map(NodeId).filter(|&n| m.exists(n)).collect();
    let is_dir = |n: NodeId| m.kind(n) == Kind::Dir && !m.flags(n).contains(Flags::OTHER_FS);
    let dirs = live.iter().filter(|&&n| is_dir(n)).count() as u64;
    let leaves: Vec<NodeId> = live
        .iter()
        .copied()
        .filter(|&n| m.kind(n) != Kind::Dir)
        .collect();
    let disk: u64 = 4096 * dirs
        + leaves
            .iter()
            .map(|&n| m.size(n, SizeMode::Disk))
            .sum::<u64>();
    let apparent: u64 = leaves.iter().map(|&n| m.size(n, SizeMode::Apparent)).sum();
    assert_eq!(m.size(NodeId::ROOT, SizeMode::Disk), disk, "disk");
    assert_eq!(
        m.size(NodeId::ROOT, SizeMode::Apparent),
        apparent,
        "apparent"
    );
    assert_eq!(
        m.counts(NodeId::ROOT),
        (leaves.len() as u64, dirs - 1),
        "counts"
    );
    for &n in &live {
        for &c in m.children(n) {
            assert!(m.exists(c), "{n:?} lists the removed child {c:?}");
        }
    }
}
