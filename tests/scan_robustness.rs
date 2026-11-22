//! M10: the scan survives files that vanish while it runs (T007). Writes only inside its temp dir.

#[path = "support/fixtures.rs"]
mod fixtures;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use fixtures::{Scan, new_model};
use storage_analyzer::scan::{
    Errno, FsAccess, RealDir, RealFs, ScanControl, ScanMsg, ScanOptions, StatInfo, start,
};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op};

/// `RealFs` that deletes the files named in `victims` right after a directory is listed,
/// so `stat` is guaranteed to find them gone.
struct RacyFs {
    inner: RealFs,
    victims: Vec<&'static [u8]>,
}

struct RacyDir {
    dir: RealDir,
    path: PathBuf,
}

impl FsAccess for RacyFs {
    type Dir = RacyDir;

    fn open_dir(&self, path: &Path) -> Result<RacyDir, Errno> {
        Ok(RacyDir {
            dir: self.inner.open_dir(path)?,
            path: path.to_path_buf(),
        })
    }

    fn read_entries(&self, dir: &mut RacyDir) -> Result<Vec<Box<[u8]>>, Errno> {
        let names = self.inner.read_entries(&mut dir.dir)?;
        for name in &names {
            if self.victims.contains(&&**name) {
                let path = dir
                    .path
                    .join(<std::ffi::OsStr as std::os::unix::ffi::OsStrExt>::from_bytes(name));
                fs::remove_file(path).unwrap();
            }
        }
        Ok(names)
    }

    fn stat_at(&self, dir: &RacyDir, name: &[u8]) -> Result<StatInfo, Errno> {
        self.inner.stat_at(&dir.dir, name)
    }
}

#[test]
fn files_that_vanish_between_listing_and_stat_become_vanished_issues() {
    let tmp = tempfile::tempdir().unwrap();
    for i in 0..20 {
        fixtures::write(tmp.path().join(format!("d/keep{i}")), 100);
    }
    fixtures::write(tmp.path().join("d/gone1"), 100);
    fixtures::write(tmp.path().join("d/gone2"), 100);
    fixtures::write(tmp.path().join("gone3"), 100);

    let fs = Arc::new(RacyFs {
        inner: RealFs,
        victims: vec![b"gone1", b"gone2", b"gone3"],
    });
    let (tx, rx) = sync_channel(64);
    let control = Arc::new(ScanControl::new());
    let dev = std::os::unix::fs::MetadataExt::dev(&fs::metadata(tmp.path()).unwrap());
    let handle = start(
        fs,
        tmp.path().to_path_buf(),
        dev,
        ScanOptions {
            threads: 3,
            one_filesystem: true,
            skip_pseudo: false,
        },
        control,
        tx,
        1,
    );
    let mut model = new_model(tmp.path(), 10);
    loop {
        let msg = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("scan stalled");
        let done = matches!(msg, ScanMsg::Done { .. });
        model.apply(msg);
        if done {
            break;
        }
    }
    drop(handle);

    assert!(model.is_complete(NodeId::ROOT));
    assert_eq!(model.issues().len(), 3, "{:?}", model.issues());
    let mut vanished: Vec<PathBuf> = Vec::new();
    for issue in model.issues() {
        assert_eq!((issue.op, issue.kind), (Op::Stat, IssueKind::Vanished));
        assert_eq!(model.kind(issue.node), Kind::Other);
        vanished.push(model.rel_path(issue.node));
    }
    vanished.sort();
    assert_eq!(
        vanished,
        vec![
            PathBuf::from("d/gone1"),
            PathBuf::from("d/gone2"),
            PathBuf::from("gone3")
        ]
    );
    assert_eq!(
        model.counts(NodeId::ROOT).0,
        23,
        "vanished entries still count as entries"
    );
}

#[test]
fn a_scan_beside_a_thread_that_deletes_and_recreates_files_still_finishes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    for d in 0..50 {
        for f in 0..1000 {
            fixtures::write_empty(root.join(format!("d{d}/f{f}")));
        }
    }
    let stop = Arc::new(AtomicBool::new(false));
    let racer = {
        let (root, stop) = (root.clone(), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut i: u64 = 0;
            while !stop.load(Ordering::Relaxed) {
                let path = root.join(format!("d{}/f{}", i % 50, (i / 50) % 1000));
                let _ = fs::remove_file(&path);
                if i.is_multiple_of(3) {
                    let _ = fs::write(&path, b"x");
                }
                i += 1;
            }
        })
    };

    let mut scan = Scan::start(&root, 4, 64);
    scan.until_done();
    stop.store(true, Ordering::Relaxed);
    racer.join().expect("the racing thread panicked");

    let model = &scan.model;
    assert!(model.is_complete(NodeId::ROOT));
    for issue in model.issues() {
        assert_eq!(issue.kind, IssueKind::Vanished, "{issue:?}");
    }
    println!("vanished issues observed: {}", model.issues().len());
    assert!(model.counts(NodeId::ROOT).1 == 50);
}
