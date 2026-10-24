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

/// Creates an empty file (parents included).
pub fn write_empty(path: impl AsRef<Path>) {
    write(path, 0);
}

/// `LC_ALL=C du -s -B1 [--apparent-size] [-x]` of `path`. du's exit status is ignored (it
/// is non-zero when a directory cannot be read, but it still prints the total).
pub fn du_with(path: &Path, apparent: bool, one_filesystem: bool) -> u64 {
    let mut cmd = Command::new("du");
    cmd.env("LC_ALL", "C").args(["-s", "-B1"]);
    if apparent {
        cmd.arg("--apparent-size");
    }
    if one_filesystem {
        cmd.arg("-x");
    }
    let out = cmd.arg(path).output().expect("du");
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "no du total: {text:?} {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
}

/// `du -s -B1` (disk) or `du -s -B1 --apparent-size` of `path`.
pub fn du(path: &Path, apparent: bool) -> u64 {
    du_with(path, apparent, false)
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
        Scan::start_with(root, threads, capacity, true)
    }

    pub fn start_with(root: &Path, threads: usize, capacity: usize, one_filesystem: bool) -> Scan {
        let control = Arc::new(ScanControl::new());
        let (tx, rx) = sync_channel(capacity);
        let root_dev = fs::metadata(root).unwrap().dev();
        let handle = start(
            Arc::new(RealFs),
            root.to_path_buf(),
            root_dev,
            ScanOptions {
                threads,
                one_filesystem,
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

/// Scans `root` to `Done` and returns the model.
pub fn scan_to_done(root: &Path, threads: usize, one_filesystem: bool) -> Model {
    let mut scan = Scan::start_with(root, threads, 256, one_filesystem);
    scan.until_done();
    scan.model
}

/// The tree of design §4 Testing, built inside a temp dir on the chosen filesystem.
pub struct M8Tree {
    pub root: PathBuf,
    pub locked: Option<PathBuf>,
    /// Extra hard links (the number of `HARDLINK_DUP` nodes a scan must produce).
    pub extra_links: usize,
    _tmp: tempfile::TempDir,
    _outside: tempfile::TempDir,
}

impl Drop for M8Tree {
    fn drop(&mut self) {
        if let Some(locked) = &self.locked {
            let _ =
                fs::set_permissions(locked, std::os::unix::fs::PermissionsExt::from_mode(0o755));
        }
    }
}

/// Flushes `path`'s filesystem. btrfs settles `st_blocks` of fresh files only after
/// write-back, so a scan and `du` could otherwise see different block counts.
pub fn sync_fs(path: &Path) {
    let status = Command::new("sync").arg("-f").arg(path).status();
    assert!(
        status.is_ok_and(|s| s.success()),
        "sync -f {} failed",
        path.display()
    );
}

/// `SA_TEST_BTRFS_DIR`, if set: a writable directory on btrfs.
pub fn btrfs_dir() -> Option<PathBuf> {
    std::env::var_os("SA_TEST_BTRFS_DIR").map(PathBuf::from)
}

/// Where the parity suite runs: the default temp dir, and btrfs when configured.
pub fn locations() -> Vec<(&'static str, Option<PathBuf>)> {
    let mut all = vec![("tmp", None)];
    match btrfs_dir() {
        Some(dir) => all.push(("btrfs", Some(dir))),
        None => println!("SKIP: SA_TEST_BTRFS_DIR is not set, btrfs case not run"),
    }
    all
}

pub fn m8_tree(base: Option<&Path>) -> M8Tree {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{PermissionsExt, symlink};
    let tmp = match base {
        Some(dir) => tempfile::tempdir_in(dir).unwrap(),
        None => tempfile::tempdir().unwrap(),
    };
    let outside = tempfile::tempdir().unwrap();
    write(outside.path().join("big.dat"), 3_000_000);
    let root = tmp.path().join("root");
    fs::create_dir(&root).unwrap();

    // Plain files, zero-byte files, empty directories.
    write(root.join("plain/a.txt"), 1);
    write(root.join("plain/b.bin"), 123_456);
    write(root.join("plain/empty.dat"), 0);
    fs::create_dir_all(root.join("plain/empty_dir/also_empty")).unwrap();

    // Hard links: within one directory and across directories.
    write(root.join("links/a/h1"), 8000);
    fs::hard_link(root.join("links/a/h1"), root.join("links/a/h1b")).unwrap();
    fs::create_dir_all(root.join("links/b")).unwrap();
    fs::create_dir_all(root.join("links/c")).unwrap();
    fs::hard_link(root.join("links/a/h1"), root.join("links/b/h2")).unwrap();
    fs::hard_link(root.join("links/a/h1"), root.join("links/c/h3")).unwrap();
    write(root.join("links/a/p"), 20_000);
    fs::hard_link(root.join("links/a/p"), root.join("links/a/q")).unwrap();

    // Symlinks: dangling, loops, outside the root, to a file and to a directory.
    fs::create_dir_all(root.join("sym")).unwrap();
    symlink("nowhere", root.join("sym/dangling")).unwrap();
    symlink(".", root.join("sym/loop")).unwrap();
    symlink("loop_b", root.join("sym/loop_a")).unwrap();
    symlink("loop_a", root.join("sym/loop_b")).unwrap();
    symlink(outside.path(), root.join("sym/outside_dir")).unwrap();
    symlink(
        outside.path().join("big.dat"),
        root.join("sym/outside_file"),
    )
    .unwrap();
    symlink("../plain/b.bin", root.join("sym/inside_file")).unwrap();
    symlink("../plain", root.join("sym/inside_dir")).unwrap();

    // A sparse file: 20 MiB long, a few real bytes.
    let sparse = fs::File::create(root.join("sparse.dat")).unwrap();
    sparse.set_len(20 * 1024 * 1024).unwrap();
    std::os::unix::fs::FileExt::write_all_at(&sparse, b"data", 10 * 1024 * 1024).unwrap();

    // Names with invalid UTF-8 and with a newline.
    let weird = root.join("weird");
    fs::create_dir(&weird).unwrap();
    fs::write(
        weird.join(std::ffi::OsStr::from_bytes(b"bad\xff\xfename")),
        vec![b'x'; 5000],
    )
    .unwrap();
    fs::write(weird.join("line\nbreak"), vec![b'x'; 70]).unwrap();

    // 200 levels of nesting.
    let mut deep = root.join("deep");
    for _ in 0..200 {
        deep.push("d");
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("bottom.txt"), vec![b'x'; 300]).unwrap();

    // A directory nobody can read (not for root, where permissions do not apply).
    let locked = (!rustix::process::geteuid().is_root()).then(|| {
        write(root.join("locked/secret.dat"), 10_000);
        fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();
        root.join("locked")
    });
    sync_fs(&root);
    M8Tree {
        root,
        locked,
        extra_links: 4,
        _tmp: tmp,
        _outside: outside,
    }
}
