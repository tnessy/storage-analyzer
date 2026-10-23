//! Delete executor safety and behavior (T010). Everything runs inside temp dirs.

use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use storage_analyzer::delete::{
    AbortReason, DeleteEvent, DeleteFailKind, DeleteHandle, DeleteRequest, Outcome, spawn,
};
use storage_analyzer::types::{Kind, NodeId};

/// Tests share the process fd table and must not overlap.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

struct Harness {
    handle: DeleteHandle,
    rx: Receiver<DeleteEvent>,
    root: tempfile::TempDir,
}

impl Harness {
    fn new() -> Harness {
        let root = tempfile::tempdir().unwrap();
        let fd = OwnedFd::from(fs::File::open(root.path()).unwrap());
        let (tx, rx) = sync_channel(1024);
        Harness {
            handle: spawn(fd, tx),
            rx,
            root,
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    fn request(&self, rel: &str) -> DeleteRequest {
        let meta = fs::symlink_metadata(self.path(rel)).unwrap();
        let ft = meta.file_type();
        let kind = if ft.is_dir() {
            Kind::Dir
        } else if ft.is_symlink() {
            Kind::Symlink
        } else if ft.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        DeleteRequest {
            node: NodeId(1),
            rel_path: rel.into(),
            expect_dev: meta.dev(),
            expect_ino: meta.ino(),
            kind,
            total_files: None,
            total_dirs: None,
        }
    }

    /// Submits and waits for `Finished`; returns the progress events seen on the way.
    fn run(&self, req: DeleteRequest) -> (Vec<DeleteEvent>, Outcome) {
        self.handle.submit(req).unwrap();
        self.collect()
    }

    fn collect(&self) -> (Vec<DeleteEvent>, Outcome) {
        let mut progress = Vec::new();
        loop {
            match self
                .rx
                .recv_timeout(Duration::from_secs(60))
                .expect("no Finished event")
            {
                DeleteEvent::Finished(outcome) => return (progress, outcome),
                ev => progress.push(ev),
            }
        }
    }

    fn delete(&self, rel: &str) -> Outcome {
        self.run(self.request(rel)).1
    }
}

fn write(path: impl AsRef<Path>, bytes: usize) {
    fs::create_dir_all(path.as_ref().parent().unwrap()).unwrap();
    fs::write(path, vec![b'x'; bytes]).unwrap();
}

fn running_as_root() -> bool {
    rustix::process::geteuid().is_root()
}

fn open_fd_count() -> usize {
    fs::read_dir("/proc/self/fd").unwrap().count()
}

#[test]
fn removes_a_file() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("f.bin"), 10_000);
    write(h.path("other"), 1);
    let Outcome::Removed(c) = h.delete("f.bin") else {
        panic!("expected Removed")
    };
    assert_eq!((c.files, c.dirs), (1, 0));
    assert!(c.bytes >= 10_000);
    assert!(!h.path("f.bin").exists());
    assert!(h.path("other").exists());
}

#[test]
fn removes_a_directory_tree_with_exact_counts() {
    let _g = serial();
    let h = Harness::new();
    let (mut files, mut dirs) = (0, 1);
    for a in 0..5 {
        dirs += 1;
        for f in 0..12 {
            write(h.path(&format!("t/a{a}/f{f}")), 1);
            files += 1;
        }
        for b in 0..4 {
            dirs += 1;
            for f in 0..22 {
                write(h.path(&format!("t/a{a}/b{b}/f{f}")), 1);
                files += 1;
            }
        }
    }
    assert_eq!(files, 500);
    write(h.path("keep/me"), 1);
    let Outcome::Removed(c) = h.delete("t") else {
        panic!("expected Removed")
    };
    assert_eq!((c.files, c.dirs), (files, dirs));
    assert!(c.bytes > 0);
    assert!(!h.path("t").exists());
    assert!(h.path("keep/me").exists());
}

#[test]
fn missing_target_is_not_found() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("f"), 1);
    let req = h.request("f");
    fs::remove_file(h.path("f")).unwrap();
    assert_eq!(
        h.run(req).1,
        Outcome::Aborted {
            reason: AbortReason::NotFound
        }
    );

    let mut req = h.request(".");
    req.rel_path = "no/such/parent/x".into();
    assert_eq!(
        h.run(req).1,
        Outcome::Aborted {
            reason: AbortReason::NotFound
        }
    );
}

#[test]
fn rel_paths_that_could_escape_or_name_the_root_are_refused() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("f"), 1);
    for bad in ["", ".", "..", "../f", "/etc/passwd", "a/../f"] {
        let mut req = h.request("f");
        req.rel_path = bad.into();
        let Outcome::Aborted {
            reason: AbortReason::OpenFailed(_),
        } = h.run(req).1
        else {
            panic!("{bad:?} was not refused");
        };
    }
    assert!(h.path("f").exists());
}

#[test]
fn target_swapped_for_a_symlink_is_not_touched() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("victim/a"), 1);
    write(h.path("outside/precious"), 100);
    let req = h.request("victim");
    fs::rename(h.path("victim"), h.path("victim.old")).unwrap();
    symlink(h.path("outside"), h.path("victim")).unwrap();

    assert_eq!(
        h.run(req).1,
        Outcome::Aborted {
            reason: AbortReason::ChangedSinceScan
        }
    );
    assert!(h.path("outside/precious").exists());
    assert!(h.path("victim.old/a").exists());
    assert!(
        fs::symlink_metadata(h.path("victim"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn target_replaced_by_a_different_object_is_changed_since_scan() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("x"), 1);
    let mut req = h.request("x");
    req.kind = Kind::Dir;
    assert_eq!(
        h.run(req).1,
        Outcome::Aborted {
            reason: AbortReason::ChangedSinceScan
        }
    );
    let mut req = h.request("x");
    req.expect_ino += 1;
    assert_eq!(
        h.run(req).1,
        Outcome::Aborted {
            reason: AbortReason::ChangedSinceScan
        }
    );
    assert!(h.path("x").exists());
}

#[test]
fn parent_component_swapped_for_a_symlink_aborts() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("a/b/target"), 1);
    write(h.path("elsewhere/b/target"), 1);
    let req = h.request("a/b/target");
    fs::rename(h.path("a"), h.path("a.old")).unwrap();
    // Relative, so it stays beneath the root and only NO_SYMLINKS can refuse it.
    symlink("elsewhere", h.path("a")).unwrap();

    let Outcome::Aborted {
        reason: AbortReason::OpenFailed(_),
    } = h.run(req).1
    else {
        panic!("expected OpenFailed");
    };
    assert!(h.path("elsewhere/b/target").exists());
    assert!(h.path("a.old/b/target").exists());
}

#[test]
fn symlinks_inside_the_subtree_are_removed_not_followed() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("outside/dir/precious"), 100);
    write(h.path("outside/file"), 100);
    write(h.path("sub/own"), 1);
    symlink(h.path("outside/dir"), h.path("sub/to_dir")).unwrap();
    symlink(h.path("outside/file"), h.path("sub/to_file")).unwrap();
    symlink("does/not/exist", h.path("sub/dangling")).unwrap();
    symlink("..", h.path("sub/up")).unwrap();

    let Outcome::Removed(c) = h.delete("sub") else {
        panic!("expected Removed")
    };
    assert_eq!((c.files, c.dirs), (5, 1));
    assert!(!h.path("sub").exists());
    assert!(h.path("outside/dir/precious").exists());
    assert!(h.path("outside/file").exists());
}

#[test]
fn hard_link_outside_the_subtree_keeps_its_data() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("outside/keep"), 5000);
    fs::create_dir(h.path("sub")).unwrap();
    fs::hard_link(h.path("outside/keep"), h.path("sub/link")).unwrap();

    let Outcome::Removed(c) = h.delete("sub") else {
        panic!("expected Removed")
    };
    assert_eq!(c.files, 1);
    assert_eq!(fs::read(h.path("outside/keep")).unwrap().len(), 5000);
}

#[test]
fn unremovable_entries_are_reported_and_the_rest_is_removed() {
    if running_as_root() {
        println!("SKIP: permissions are not enforced for root");
        return;
    }
    let _g = serial();
    let h = Harness::new();
    write(h.path("top/ok.txt"), 1);
    write(h.path("top/ro/a"), 1);
    write(h.path("top/ro/b"), 1);
    write(h.path("top/locked/c"), 1);
    fs::set_permissions(h.path("top/ro"), fs::Permissions::from_mode(0o555)).unwrap();
    fs::set_permissions(h.path("top/locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let outcome = h.delete("top");
    for d in ["top/ro", "top/locked"] {
        fs::set_permissions(h.path(d), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let Outcome::Partial { removed, failures } = outcome else {
        panic!("expected Partial")
    };
    let mut failed: Vec<_> = failures
        .iter()
        .map(|f| (f.rel_path.clone(), f.kind))
        .collect();
    failed.sort_by(|a, b| a.0.cmp(&b.0));
    let denied = DeleteFailKind::PermissionDenied;
    assert_eq!(
        failed,
        vec![
            ("top/locked".into(), denied),
            ("top/ro/a".into(), denied),
            ("top/ro/b".into(), denied),
        ],
        "leftover non-empty dirs are not extra failures"
    );
    assert_eq!((removed.files, removed.dirs), (1, 0));
    assert!(!h.path("top/ok.txt").exists());
    assert!(h.path("top/ro/a").exists() && h.path("top/locked/c").exists());
}

#[test]
fn very_deep_tree_is_removed_with_bounded_open_fds() {
    let _g = serial();
    let h = Harness::new();
    let mut rel = String::from("deep");
    for level in 0..400 {
        rel.push_str("/d");
        if level % 50 == 0 {
            fs::create_dir_all(h.path(&rel)).unwrap();
            write(h.path(&format!("{rel}/f")), 1);
        }
    }
    fs::create_dir_all(h.path(&rel)).unwrap();
    write(h.path(&format!("{rel}/leaf")), 1);

    let baseline = open_fd_count();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let peak = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut peak = 0;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                peak = peak.max(open_fd_count());
            }
            peak
        })
    };
    let outcome = h.delete("deep");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let peak = peak.join().unwrap();

    let Outcome::Removed(c) = outcome else {
        panic!("expected Removed: {outcome:?}")
    };
    assert_eq!((c.files, c.dirs), (9, 401));
    assert!(!h.path("deep").exists());
    println!("peak fds above baseline: {}", peak.saturating_sub(baseline));
    assert!(
        peak <= baseline + 64 + 8,
        "peak {peak}, baseline {baseline}"
    );
}

#[test]
fn progress_events_are_rate_limited() {
    let _g = serial();
    let h = Harness::new();
    fs::create_dir(h.path("big")).unwrap();
    for i in 0..50_000 {
        fs::File::create(h.path(&format!("big/f{i}"))).unwrap();
    }
    let started = Instant::now();
    let (progress, outcome) = h.run(h.request("big"));
    let elapsed_ms = started.elapsed().as_millis();
    let Outcome::Removed(c) = outcome else {
        panic!("expected Removed")
    };
    assert_eq!(c.files, 50_000);
    assert!(
        progress.len() as u128 <= elapsed_ms / 100 + 2,
        "{} events in {elapsed_ms} ms",
        progress.len()
    );
    let mut last = 0;
    for ev in &progress {
        let DeleteEvent::Progress { removed_files, .. } = ev else {
            panic!()
        };
        assert!(*removed_files >= last);
        last = *removed_files;
    }
}

#[test]
fn stop_ends_the_delete_early_and_goes_idle() {
    let _g = serial();
    let h = Harness::new();
    let total = 100_000;
    for d in 0..100 {
        fs::create_dir_all(h.path(&format!("big/d{d}"))).unwrap();
        for i in 0..total / 100 {
            fs::File::create(h.path(&format!("big/d{d}/f{i}"))).unwrap();
        }
    }
    h.handle.submit(h.request("big")).unwrap();
    h.handle.stop();
    assert!(h.handle.wait_idle(Duration::from_secs(10)));
    let (_, outcome) = h.collect();
    match outcome {
        Outcome::Partial { removed, failures } => {
            assert!(removed.files < total as u64);
            assert!(failures.is_empty());
            assert!(h.path("big").exists());
        }
        Outcome::Removed(c) => assert_eq!(c.files, total as u64),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn second_submit_while_running_is_busy_and_a_later_one_works() {
    let _g = serial();
    let h = Harness::new();
    for i in 0..20_000 {
        write(h.path(&format!("big/f{i}")), 0);
    }
    write(h.path("small"), 1);
    let big = h.request("big");
    let small = h.request("small");
    h.handle.submit(big).unwrap();
    assert!(h.handle.submit(small.clone()).is_err());
    let (_, outcome) = h.collect();
    assert!(matches!(outcome, Outcome::Removed(_)));
    assert!(h.handle.wait_idle(Duration::from_secs(5)));
    let Outcome::Removed(c) = h.run(small).1 else {
        panic!("expected Removed")
    };
    assert_eq!(c.files, 1);
}

/// Runs `test_name` again under `unshare -rm` so it can mount. Returns true inside the
/// namespace, where the caller runs the real body. Prints SKIP and returns false if
/// user namespaces are unavailable.
fn in_mount_namespace(test_name: &str) -> bool {
    if std::env::var_os("DELETE_TEST_IN_NS").is_some() {
        return true;
    }
    let probe = std::process::Command::new("unshare")
        .args(["-rm", "true"])
        .status();
    if !probe.is_ok_and(|s| s.success()) {
        println!("SKIP: unshare unavailable");
        return false;
    }
    let status = std::process::Command::new("unshare")
        .args(["-rm", "env", "DELETE_TEST_IN_NS=1"])
        .arg(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", test_name, "--nocapture"])
        .status()
        .unwrap();
    assert!(status.success(), "{test_name} failed inside the namespace");
    false
}

fn mount(args: &[&str], target: &Path) {
    let status = std::process::Command::new("mount")
        .args(args)
        .arg(target)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "mount {args:?} failed inside the namespace"
    );
}

/// Needs user namespaces: `cargo test --test delete_executor -- --ignored mount`.
#[test]
#[ignore = "needs unshare -rm"]
fn mount_inside_target_is_not_descended() {
    if !in_mount_namespace("mount_inside_target_is_not_descended") {
        return;
    }
    let _g = serial();
    let h = Harness::new();
    write(h.path("tgt/f.txt"), 1);
    fs::create_dir_all(h.path("tgt/mnt")).unwrap();
    mount(&["-t", "tmpfs", "tmpfs"], &h.path("tgt/mnt"));
    write(h.path("tgt/mnt/keep"), 1);

    let Outcome::Partial { removed, failures } = h.delete("tgt") else {
        panic!("expected Partial")
    };
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].rel_path, PathBuf::from("tgt/mnt"));
    assert_eq!(failures[0].kind, DeleteFailKind::OtherFilesystem);
    assert_eq!(removed.files, 1);
    assert!(!h.path("tgt/f.txt").exists());
    assert!(h.path("tgt/mnt/keep").exists());
}

#[test]
#[ignore = "needs unshare -rm"]
fn mount_same_device_bind_inside_target_is_not_descended() {
    if !in_mount_namespace("mount_same_device_bind_inside_target_is_not_descended") {
        return;
    }
    let _g = serial();
    let h = Harness::new();
    write(h.path("other/keep"), 1);
    write(h.path("tgt/f.txt"), 1);
    fs::create_dir_all(h.path("tgt/bound")).unwrap();
    mount(
        &["--bind", h.path("other").to_str().unwrap()],
        &h.path("tgt/bound"),
    );
    let same_dev = fs::metadata(h.path("tgt/bound")).unwrap().dev()
        == fs::metadata(h.path("tgt")).unwrap().dev();
    assert!(same_dev, "the test needs a same-device bind mount");

    let Outcome::Partial { failures, .. } = h.delete("tgt") else {
        panic!("expected Partial")
    };
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].rel_path, PathBuf::from("tgt/bound"));
    assert_eq!(failures[0].kind, DeleteFailKind::OtherFilesystem);
    assert!(h.path("other/keep").exists());
    assert!(!h.path("tgt/f.txt").exists());
}

#[test]
#[ignore = "needs unshare -rm"]
fn mount_point_as_the_target_is_refused() {
    if !in_mount_namespace("mount_point_as_the_target_is_refused") {
        return;
    }
    let _g = serial();
    let h = Harness::new();
    // A tmpfs (other device) and a bind mount (same device, other mount id).
    fs::create_dir_all(h.path("tmp")).unwrap();
    mount(&["-t", "tmpfs", "tmpfs"], &h.path("tmp"));
    write(h.path("tmp/keep"), 1);
    write(h.path("src/keep"), 1);
    fs::create_dir_all(h.path("bind")).unwrap();
    mount(
        &["--bind", h.path("src").to_str().unwrap()],
        &h.path("bind"),
    );

    for rel in ["tmp", "bind"] {
        assert_eq!(
            h.delete(rel),
            Outcome::Aborted {
                reason: AbortReason::MountPoint
            },
            "{rel}"
        );
    }
    assert!(h.path("tmp/keep").exists() && h.path("bind/keep").exists());
}

#[test]
fn bytes_for_a_file_with_all_its_links_inside_the_target_count_once() {
    let _g = serial();
    let h = Harness::new();
    fs::create_dir(h.path("t")).unwrap();
    write(h.path("t/a"), 20_000);
    fs::hard_link(h.path("t/a"), h.path("t/b")).unwrap();
    fs::hard_link(h.path("t/a"), h.path("t/c")).unwrap();
    let file_bytes = fs::metadata(h.path("t/a")).unwrap().blocks() * 512;
    let dir_bytes = fs::metadata(h.path("t")).unwrap().blocks() * 512;

    let Outcome::Removed(c) = h.delete("t") else {
        panic!("expected Removed")
    };
    assert_eq!((c.files, c.dirs), (3, 1));
    assert_eq!(c.bytes, file_bytes + dir_bytes);
}

#[test]
fn bytes_are_the_blocks_of_single_link_files_and_directories() {
    let _g = serial();
    let h = Harness::new();
    write(h.path("outside/shared"), 9000);
    write(h.path("t/one"), 5000);
    write(h.path("t/sub/two"), 30_000);
    fs::hard_link(h.path("outside/shared"), h.path("t/sub/link")).unwrap();
    let blocks = |rel: &str| fs::metadata(h.path(rel)).unwrap().blocks() * 512;
    let expected = blocks("t/one") + blocks("t/sub/two") + blocks("t") + blocks("t/sub");

    let Outcome::Removed(c) = h.delete("t") else {
        panic!("expected Removed")
    };
    assert_eq!((c.files, c.dirs), (3, 2));
    assert_eq!(c.bytes, expected, "a link whose data survives adds nothing");
}

#[test]
fn dropping_the_handle_returns_even_if_nobody_reads_the_events() {
    let _g = serial();
    let root = tempfile::tempdir().unwrap();
    for name in ["a", "b"] {
        write(root.path().join(name), 1);
    }
    let fd = OwnedFd::from(fs::File::open(root.path()).unwrap());
    let (tx, rx) = sync_channel(1);
    let handle = spawn(fd, tx);
    let mut idle = true;
    for name in ["a", "b"] {
        let meta = fs::symlink_metadata(root.path().join(name)).unwrap();
        let req = DeleteRequest {
            node: NodeId(1),
            rel_path: name.into(),
            expect_dev: meta.dev(),
            expect_ino: meta.ino(),
            kind: Kind::File,
            total_files: None,
            total_dirs: None,
        };
        handle.submit(req).unwrap();
        // The first `Finished` fills the channel; the second cannot be sent.
        idle &= handle.wait_idle(Duration::from_secs(5));
    }
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(handle);
        let _ = done_tx.send(());
    });
    let returned = done_rx.recv_timeout(Duration::from_secs(5)).is_ok();
    drop(rx);
    assert!(idle, "executor never went idle");
    assert!(returned, "Drop hung on a full channel");
}
