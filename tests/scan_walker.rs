//! Walker behavior over `FakeFs` (T006), plus `RealFs` on a temp dir.

#[path = "support/fake_fs.rs"]
mod fake_fs;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::{Duration, Instant};

use fake_fs::{DEV, FakeFs};
use storage_analyzer::scan::{
    Errno, FsAccess, RawEntry, RealFs, ScanControl, ScanHandle, ScanMsg, ScanOptions, start,
};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op};

const WAIT: Duration = Duration::from_secs(10);

struct Run<F: FsAccess + 'static> {
    rx: Receiver<ScanMsg>,
    handle: ScanHandle,
    control: Arc<ScanControl>,
    fs: Arc<F>,
    next_id: u32,
}

fn opts(threads: usize, one_filesystem: bool) -> ScanOptions {
    ScanOptions {
        threads,
        one_filesystem,
    }
}

fn begin_with<F: FsAccess + 'static>(
    fs: Arc<F>,
    root: &str,
    root_dev: u64,
    opts: ScanOptions,
    control: Arc<ScanControl>,
    capacity: usize,
) -> Run<F> {
    let (tx, rx) = sync_channel(capacity);
    let handle = start(
        Arc::clone(&fs),
        PathBuf::from(root),
        root_dev,
        opts,
        Arc::clone(&control),
        tx,
        1,
    );
    Run {
        rx,
        handle,
        control,
        fs,
        next_id: 1,
    }
}

fn begin(fs: FakeFs, root: &str, threads: usize, one_filesystem: bool) -> Run<FakeFs> {
    begin_with(
        Arc::new(fs),
        root,
        DEV,
        opts(threads, one_filesystem),
        Arc::new(ScanControl::new()),
        1024,
    )
}

/// A big tree and a channel nobody drains, so workers block on a full channel.
fn blocked_run(threads: usize) -> Run<FakeFs> {
    let run = begin_with(
        Arc::new(FakeFs::generate("/r", 6, 4, 1)),
        "/r",
        DEV,
        opts(threads, true),
        Arc::new(ScanControl::new()),
        1,
    );
    std::thread::sleep(Duration::from_millis(200));
    run
}

impl<F: FsAccess + 'static> Run<F> {
    fn next(&mut self) -> ScanMsg {
        let msg = self.rx.recv_timeout(WAIT).expect("scan stalled");
        if let ScanMsg::Listing {
            first_id, entries, ..
        } = &msg
        {
            self.next_id = self.next_id.max(first_id.0 + entries.len() as u32);
        }
        msg
    }

    /// Messages up to and including the next `Done`.
    fn until_done(&mut self) -> Vec<ScanMsg> {
        let mut msgs = Vec::new();
        loop {
            let msg = self.next();
            let done = matches!(msg, ScanMsg::Done { .. });
            msgs.push(msg);
            if done {
                return msgs;
            }
        }
    }
}

/// Follows ids to paths as listings arrive.
struct Paths {
    map: HashMap<NodeId, PathBuf>,
}

impl Paths {
    fn new(root: &str) -> Paths {
        Paths {
            map: HashMap::from([(NodeId::ROOT, PathBuf::from(root))]),
        }
    }

    fn feed(&mut self, msg: &ScanMsg) {
        if let ScanMsg::Listing {
            dir,
            first_id,
            entries,
        } = msg
        {
            let base = self.map[dir].clone();
            for (i, e) in entries.iter().enumerate() {
                let id = NodeId(first_id.0 + i as u32);
                self.map.insert(id, base.join(OsStr::from_bytes(&e.name)));
            }
        }
    }

    fn of(&self, id: NodeId) -> &Path {
        &self.map[&id]
    }
}

fn listings(msgs: &[ScanMsg]) -> Vec<(NodeId, &Vec<RawEntry>)> {
    msgs.iter()
        .filter_map(|m| match m {
            ScanMsg::Listing { dir, entries, .. } => Some((*dir, entries)),
            _ => None,
        })
        .collect()
}

fn count_dirs(depth: u32, fanout: u32) -> u32 {
    (0..=depth).map(|d| fanout.pow(d)).sum()
}

#[test]
fn listings_follow_the_ordering_contract_and_cover_the_tree() {
    let mut run = begin(FakeFs::generate("/r", 3, 3, 2), "/r", 4, true);
    let msgs = run.until_done();
    let mut known = vec![NodeId::ROOT];
    let mut seen_dirs = 0;
    for m in &msgs {
        if let ScanMsg::Listing {
            dir,
            first_id,
            entries,
        } = m
        {
            assert!(
                known.contains(dir),
                "listing for {dir:?} before its parent's"
            );
            seen_dirs += 1;
            for (i, e) in entries.iter().enumerate() {
                if e.kind == Kind::Dir {
                    known.push(NodeId(first_id.0 + i as u32));
                }
            }
        }
    }
    assert_eq!(seen_dirs, count_dirs(3, 3));
    assert_eq!(msgs.last(), Some(&ScanMsg::Done { cancelled: false }));
    assert_eq!(
        msgs.iter()
            .filter(|m| matches!(m, ScanMsg::Done { .. }))
            .count(),
        1
    );
}

#[test]
fn ids_are_contiguous_and_unique_across_threads() {
    let mut run = begin(FakeFs::generate("/r", 3, 3, 2), "/r", 4, true);
    let msgs = run.until_done();
    let mut ids: Vec<u32> = Vec::new();
    for m in &msgs {
        if let ScanMsg::Listing {
            first_id, entries, ..
        } = m
        {
            ids.extend(first_id.0..first_id.0 + entries.len() as u32);
        }
    }
    ids.sort_unstable();
    let expected: Vec<u32> = (1..=ids.len() as u32).collect();
    assert_eq!(ids, expected);
}

#[test]
fn single_thread_lists_shallow_directories_first() {
    let mut run = begin(FakeFs::generate("/r", 3, 3, 1), "/r", 1, true);
    let msgs = run.until_done();
    let mut depth_of = HashMap::from([(NodeId::ROOT, 0u32)]);
    let mut depths = Vec::new();
    for m in &msgs {
        if let ScanMsg::Listing {
            dir,
            first_id,
            entries,
        } = m
        {
            let d = depth_of[dir];
            depths.push(d);
            for i in 0..entries.len() {
                depth_of.insert(NodeId(first_id.0 + i as u32), d + 1);
            }
        }
    }
    assert!(depths.windows(2).all(|w| w[0] <= w[1]), "{depths:?}");
}

#[test]
fn focus_lists_the_focused_subtree_before_unfocused_siblings() {
    let fs = FakeFs::generate("/r", 3, 2, 1);
    let control = Arc::new(ScanControl::new());
    control.set_focus(Some("/r/d1".into()));
    let mut run = begin_with(Arc::new(fs), "/r", DEV, opts(1, true), control, 1024);
    let msgs = run.until_done();
    let mut paths = Paths::new("/r");
    let mut order = Vec::new();
    for m in &msgs {
        paths.feed(m);
        if let ScanMsg::Listing { dir, .. } = m {
            order.push(paths.of(*dir).to_path_buf());
        }
    }
    let first_unfocused = order.iter().position(|p| p == Path::new("/r/d0")).unwrap();
    let last_focused = order.iter().rposition(|p| p.starts_with("/r/d1")).unwrap();
    assert!(last_focused < first_unfocused, "{order:?}");
}

#[test]
fn skipped_subtree_is_cancelled_and_never_listed() {
    let control = Arc::new(ScanControl::new());
    control.cancel_prefix("/r/d0".into());
    let mut run = begin_with(
        Arc::new(FakeFs::generate("/r", 2, 2, 1)),
        "/r",
        DEV,
        opts(2, true),
        control,
        1024,
    );
    let msgs = run.until_done();
    let mut paths = Paths::new("/r");
    let mut cancelled = Vec::new();
    for m in &msgs {
        paths.feed(m);
        match m {
            ScanMsg::Listing { dir, .. } => {
                assert!(
                    !paths.of(*dir).starts_with("/r/d0"),
                    "listed {:?}",
                    paths.of(*dir)
                );
            }
            ScanMsg::Cancelled { dir } => cancelled.push(paths.of(*dir).to_path_buf()),
            _ => {}
        }
    }
    assert_eq!(cancelled, vec![PathBuf::from("/r/d0")]);
    assert_eq!(msgs.last(), Some(&ScanMsg::Done { cancelled: false }));
}

#[test]
fn errors_are_reported_and_the_scan_still_completes() {
    let fs = FakeFs::generate("/r", 1, 3, 2);
    fs.fail_open("/r/d1", Errno::ACCESS);
    fs.fail_read("/r/d2", Errno::IO);
    fs.fail_stat("/r/f0", Errno::NOENT);
    fs.fail_stat("/r/f1", Errno::IO);
    let mut run = begin(fs, "/r", 2, true);
    let msgs = run.until_done();
    let mut paths = Paths::new("/r");
    let mut issues = Vec::new();
    for m in &msgs {
        paths.feed(m);
        if let ScanMsg::DirIssue { dir, op, kind } = m {
            issues.push((paths.of(*dir).to_path_buf(), *op, *kind));
        }
    }
    issues.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        issues,
        vec![
            ("/r/d1".into(), Op::Open, IssueKind::PermissionDenied),
            (
                "/r/d2".into(),
                Op::Read,
                IssueKind::Other(Errno::IO.raw_os_error())
            ),
        ]
    );
    let root = listings(&msgs)[0].1;
    let by_name = |n: &str| root.iter().find(|e| &*e.name == n.as_bytes()).unwrap();
    for (name, kind) in [("f0", IssueKind::Vanished), ("f1", IssueKind::Other(5))] {
        let e = by_name(name);
        assert_eq!(e.issue, Some((Op::Stat, kind)));
        assert_eq!(
            (e.kind, e.disk_bytes, e.apparent_bytes),
            (Kind::Other, 0, 0)
        );
    }
    assert_eq!(msgs.last(), Some(&ScanMsg::Done { cancelled: false }));
}

#[test]
fn other_filesystems_are_flagged_and_not_descended_unless_allowed() {
    let make = || {
        let fs = FakeFs::generate("/r", 1, 2, 1);
        fs.set_dev("/r/d1", 2);
        fs
    };
    let mut run = begin(make(), "/r", 2, true);
    let msgs = run.until_done();
    let root = listings(&msgs)[0].1;
    let mnt = root.iter().find(|e| &*e.name == b"d1").unwrap();
    assert_eq!(mnt.issue, Some((Op::Stat, IssueKind::OtherFilesystem)));
    assert_eq!(listings(&msgs).len(), 2, "root and d0 only");
    assert!(!run.fs.calls().iter().any(|c| c == "open /r/d1"));

    let mut run = begin(make(), "/r", 2, false);
    let msgs = run.until_done();
    assert_eq!(listings(&msgs).len(), 3);
    assert!(listings(&msgs)[0].1.iter().all(|e| e.issue.is_none()));
}

#[test]
fn cancel_all_ends_with_done_cancelled_and_stops_listing() {
    let total = count_dirs(7, 4) as usize;
    let mut run = begin_with(
        Arc::new(FakeFs::generate("/r", 7, 4, 0)),
        "/r",
        DEV,
        opts(2, true),
        Arc::new(ScanControl::new()),
        2,
    );
    let _first = run.next();
    run.control.cancel_all();
    let mut listings_after = 0;
    loop {
        match run.next() {
            ScanMsg::Done { cancelled } => {
                assert!(cancelled);
                break;
            }
            ScanMsg::Listing { .. } => listings_after += 1,
            _ => {}
        }
    }
    assert!(listings_after < total / 2, "{listings_after} of {total}");
    assert!(
        run.rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "nothing follows Done"
    );
}

#[test]
fn rescan_after_done_relists_with_fresh_ids_and_reports_done_again() {
    let mut run = begin(FakeFs::generate("/r", 1, 2, 1), "/r", 2, true);
    let first = run.until_done();
    let before_max = run.next_id;
    assert_eq!(listings(&first).len(), 3);

    run.handle.rescan(NodeId(1), "/r/d0".into(), 1);
    let again = run.until_done();
    assert!(matches!(
        again[0],
        ScanMsg::Identity {
            node: NodeId(1),
            ..
        }
    ));
    let ScanMsg::Listing {
        dir,
        first_id,
        entries,
    } = &again[1]
    else {
        panic!("{again:?}")
    };
    assert_eq!((*dir, entries.len()), (NodeId(1), 1));
    assert!(first_id.0 >= before_max, "ids are never reused");
    assert_eq!(again.last(), Some(&ScanMsg::Done { cancelled: false }));
    assert_eq!(again.len(), 3);
}

#[test]
fn rescan_works_after_a_cancelled_scan() {
    let mut run = begin_with(
        Arc::new(FakeFs::generate("/r", 6, 4, 0)),
        "/r",
        DEV,
        opts(2, true),
        Arc::new(ScanControl::new()),
        2,
    );
    run.next();
    run.control.cancel_all();
    while !matches!(run.next(), ScanMsg::Done { cancelled: true }) {}

    run.fs.dir("/"); // the root rescan re-stats "/r" through its parent
    run.handle.rescan(NodeId::ROOT, "/r".into(), 0);
    assert!(!run.control.is_cancel_all(), "rescan must clear cancel_all");
    let mut got_root_listing = false;
    loop {
        match run.next() {
            ScanMsg::Listing { dir, .. } if dir == NodeId::ROOT => got_root_listing = true,
            ScanMsg::Done { cancelled: false } => break,
            _ => {}
        }
    }
    assert!(got_root_listing);
}

#[test]
fn rescan_of_a_replaced_dir_sends_the_new_identity_then_a_listing() {
    let mut run = begin(FakeFs::generate("/r", 1, 1, 1), "/r", 1, true);
    run.until_done();
    let old = run.fs.stat_info("/r/d0");
    run.fs
        .remove("/r/d0")
        .dir("/r/d0")
        .file("/r/d0/new", 4096, 7);
    run.handle.rescan(NodeId(1), "/r/d0".into(), 1);
    let msgs = run.until_done();

    let ScanMsg::Identity { node, entry } = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert_eq!(
        (*node, entry.kind, &*entry.name),
        (NodeId(1), Kind::Dir, &b"d0"[..])
    );
    assert_ne!(entry.ino, old.ino);
    assert_eq!(entry.dev, DEV);
    let ScanMsg::Listing { dir, entries, .. } = &msgs[1] else {
        panic!("{msgs:?}")
    };
    assert_eq!((*dir, &*entries[0].name), (NodeId(1), &b"new"[..]));
}

#[test]
fn rescan_of_a_dir_now_a_file_or_symlink_sends_identity_only() {
    for replace in ["file", "symlink"] {
        let mut run = begin(FakeFs::generate("/r", 1, 1, 1), "/r", 1, true);
        run.until_done();
        run.fs.remove("/r/d0");
        if replace == "file" {
            run.fs.file("/r/d0", 8192, 123);
        } else {
            run.fs.symlink("/r/d0");
        }
        let calls_before = run.fs.calls().len();
        run.handle.rescan(NodeId(1), "/r/d0".into(), 1);
        let msgs = run.until_done();
        assert_eq!(msgs.len(), 2, "{replace}: {msgs:?}");
        let ScanMsg::Identity { entry, .. } = &msgs[0] else {
            panic!("{msgs:?}")
        };
        if replace == "file" {
            assert_eq!(
                (entry.kind, entry.disk_bytes, entry.apparent_bytes),
                (Kind::File, 8192, 123)
            );
        } else {
            assert_eq!(entry.kind, Kind::Symlink);
        }
        let new_calls = &run.fs.calls()[calls_before..];
        assert!(
            !new_calls
                .iter()
                .any(|c| c == "open /r/d0" || c == "read /r/d0"),
            "{replace}: the target must never be opened: {new_calls:?}"
        );
    }
}

#[test]
fn rescan_of_a_removed_target_reports_vanished() {
    let mut run = begin(FakeFs::generate("/r", 1, 1, 1), "/r", 1, true);
    run.until_done();
    run.fs.remove("/r/d0");
    run.handle.rescan(NodeId(1), "/r/d0".into(), 1);
    let msgs = run.until_done();
    assert_eq!(
        msgs[0],
        ScanMsg::DirIssue {
            dir: NodeId(1),
            op: Op::Stat,
            kind: IssueKind::Vanished
        }
    );
    assert_eq!(msgs.len(), 2);
}

#[test]
fn rescan_of_a_file_node_sends_identity_only() {
    let mut run = begin(FakeFs::generate("/r", 1, 1, 1), "/r", 1, true);
    let first = run.until_done();
    let root = listings(&first)[0].1.clone();
    let file_id = 1 + root.iter().position(|e| e.kind == Kind::File).unwrap() as u32;
    run.handle.rescan(NodeId(file_id), "/r/f0".into(), 1);
    let msgs = run.until_done();
    assert!(matches!(msgs[0], ScanMsg::Identity { .. }));
    assert_eq!(msgs.len(), 2);
}

#[test]
fn rescan_with_an_unreadable_parent_reports_the_error() {
    let mut run = begin(FakeFs::generate("/r", 1, 1, 1), "/r", 1, true);
    run.until_done();
    run.fs.fail_open("/r", Errno::ACCESS);
    run.handle.rescan(NodeId(1), "/r/d0".into(), 1);
    let msgs = run.until_done();
    assert_eq!(
        msgs[0],
        ScanMsg::DirIssue {
            dir: NodeId(1),
            op: Op::Open,
            kind: IssueKind::PermissionDenied
        }
    );
}

#[test]
fn workers_are_named_scan_n() {
    let mut run = begin(FakeFs::generate("/r", 2, 3, 1), "/r", 3, true);
    run.until_done();
    let names = run.fs.thread_names();
    assert!(!names.is_empty());
    for n in names {
        let n = n.expect("unnamed worker");
        assert!(n.starts_with("scan-"), "{n}");
    }
}

#[test]
fn closing_the_handle_returns_promptly_with_a_full_channel_and_a_live_receiver() {
    let run = blocked_run(4);
    let started = Instant::now();
    let rx = run.rx;
    run.handle.shutdown();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    drop(rx);
}

#[test]
fn workers_exit_when_the_receiver_is_dropped() {
    let run = blocked_run(4);
    drop(run.rx);
    let started = Instant::now();
    run.handle.shutdown();
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn cancel_all_while_workers_block_on_a_full_channel_still_ends_the_scan() {
    let run = blocked_run(4);
    run.control.cancel_all();
    let deadline = Instant::now() + WAIT;
    let mut saw_done = false;
    while Instant::now() < deadline && !saw_done {
        saw_done = matches!(
            run.rx.recv_timeout(Duration::from_millis(100)),
            Ok(ScanMsg::Done { cancelled: true })
        );
    }
    assert!(saw_done);
}

#[test]
fn real_fs_stats_match_std() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f"), vec![1u8; 10_000]).unwrap();
    std::fs::create_dir(tmp.path().join("d")).unwrap();
    symlink("f", tmp.path().join("l")).unwrap();
    symlink("d", tmp.path().join("ld")).unwrap();
    std::fs::hard_link(tmp.path().join("f"), tmp.path().join("h")).unwrap();

    let fs = RealFs;
    let dir = fs.open_dir(tmp.path()).unwrap();
    let mut dir_for_read = fs.open_dir(tmp.path()).unwrap();
    let mut names = fs.read_entries(&mut dir_for_read).unwrap();
    names.sort();
    let names: Vec<_> = names
        .iter()
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect();
    assert_eq!(names, ["d", "f", "h", "l", "ld"]);

    let cases = [
        ("f", Kind::File),
        ("d", Kind::Dir),
        ("l", Kind::Symlink),
        ("ld", Kind::Symlink),
    ];
    for (name, kind) in cases {
        let st = fs.stat_at(&dir, name.as_bytes()).unwrap();
        let meta = std::fs::symlink_metadata(tmp.path().join(name)).unwrap();
        assert_eq!(st.kind, kind, "{name}");
        assert_eq!(st.disk_bytes, meta.blocks() * 512, "{name}");
        assert_eq!(st.apparent_bytes, meta.len(), "{name}");
        assert_eq!((st.ino, st.dev), (meta.ino(), meta.dev()), "{name}");
        assert_eq!(u64::from(st.nlink), meta.nlink(), "{name}");
    }
    assert_eq!(fs.stat_at(&dir, b"f").unwrap().nlink, 2);
    assert_eq!(fs.stat_at(&dir, b"missing").unwrap_err(), Errno::NOENT);
}

#[test]
fn real_fs_refuses_symlinks_and_reports_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("d")).unwrap();
    symlink("d", tmp.path().join("ld")).unwrap();
    let fs = RealFs;
    assert!(fs.open_dir(&tmp.path().join("ld")).is_err(), "NOFOLLOW");
    assert_eq!(
        fs.open_dir(&tmp.path().join("nope")).unwrap_err(),
        Errno::NOENT
    );

    if rustix::process::geteuid().is_root() {
        println!("SKIP: permissions are not enforced for root");
        return;
    }
    std::fs::set_permissions(tmp.path().join("d"), std::fs::Permissions::from_mode(0o000)).unwrap();
    let denied = fs.open_dir(&tmp.path().join("d")).unwrap_err();
    std::fs::set_permissions(tmp.path().join("d"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(denied, Errno::ACCESS);
}

#[test]
fn real_scan_of_a_temp_tree_lists_everything_and_flags_unreadable_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join("a/b")).unwrap();
    std::fs::write(root.join("a/b/f"), b"hello").unwrap();
    std::fs::create_dir(root.join("locked")).unwrap();
    let root_dev = std::fs::metadata(root).unwrap().dev();
    let locked = !rustix::process::geteuid().is_root();
    if locked {
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o000))
            .unwrap();
    }
    let mut run = begin_with(
        Arc::new(RealFs),
        root.to_str().unwrap(),
        root_dev,
        opts(2, true),
        Arc::new(ScanControl::new()),
        64,
    );
    let msgs = run.until_done();
    if locked {
        std::fs::set_permissions(root.join("locked"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    assert_eq!(
        listings(&msgs).len(),
        if locked { 3 } else { 4 },
        "{msgs:?}"
    );
    let denied = msgs.iter().any(|m| {
        matches!(
            m,
            ScanMsg::DirIssue {
                op: Op::Open,
                kind: IssueKind::PermissionDenied,
                ..
            }
        )
    });
    assert_eq!(denied, locked);
    assert_eq!(msgs.last(), Some(&ScanMsg::Done { cancelled: false }));
}

#[test]
fn a_rescan_root_now_on_another_device_is_reported_and_not_listed() {
    let mut run = begin(FakeFs::generate("/r", 1, 2, 1), "/r", 1, true);
    run.until_done();
    run.fs.set_dev("/r/d0", 2);
    let calls_before = run.fs.calls().len();
    run.handle.rescan(NodeId(2), "/r/d0".into(), 1);
    let msgs = run.until_done();
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    let ScanMsg::Identity { node, entry } = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert_eq!(*node, NodeId(2));
    assert_eq!((entry.kind, entry.dev), (Kind::Dir, 2));
    assert_eq!(entry.issue, Some((Op::Stat, IssueKind::OtherFilesystem)));
    let new_calls = &run.fs.calls()[calls_before..];
    assert!(
        !new_calls.iter().any(|c| c == "open /r/d0"),
        "{new_calls:?}"
    );
}

#[test]
fn a_rescan_root_on_another_device_is_listed_when_crossing_is_allowed() {
    let mut run = begin(FakeFs::generate("/r", 1, 2, 1), "/r", 1, false);
    run.until_done();
    run.fs.set_dev("/r/d0", 2);
    run.handle.rescan(NodeId(2), "/r/d0".into(), 1);
    let msgs = run.until_done();
    let ScanMsg::Identity { entry, .. } = &msgs[0] else {
        panic!("{msgs:?}")
    };
    assert_eq!(entry.issue, None);
    assert!(matches!(msgs[1], ScanMsg::Listing { .. }));
}

#[test]
fn a_rescan_right_after_a_stop_is_never_lost() {
    for round in 0..200 {
        let run = begin_with(
            Arc::new(FakeFs::generate("/r", 3, 3, 1)),
            "/r",
            DEV,
            opts(4, true),
            Arc::new(ScanControl::new()),
            2,
        );
        run.fs.dir("/");
        let recv = |run: &Run<FakeFs>| {
            run.rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap_or_else(|_| panic!("round {round}: the scan stalled (rescan lost?)"))
        };
        let _first = recv(&run);
        run.control.cancel_all();
        while !matches!(recv(&run), ScanMsg::Done { cancelled: true }) {}

        // The stop's Done is out; the last worker is on its way back to the queue.
        run.handle.rescan(NodeId::ROOT, "/r".into(), 0);
        let mut saw_root_listing = false;
        loop {
            match recv(&run) {
                ScanMsg::Listing { dir, .. } if dir == NodeId::ROOT => saw_root_listing = true,
                ScanMsg::Done { cancelled: false } => break,
                ScanMsg::Done { cancelled: true } => panic!("round {round}: stale stop Done"),
                _ => {}
            }
        }
        assert!(
            saw_root_listing,
            "round {round}: the rescan never listed the root"
        );
        run.handle.shutdown();
    }
}

#[test]
fn rescan_many_gives_one_done_after_all_the_rescans() {
    for _ in 0..50 {
        let mut run = begin(FakeFs::generate("/r", 1, 3, 1), "/r", 4, true);
        let first = run.until_done();
        let root = listings(&first)[0].1.clone();
        let targets: Vec<(NodeId, PathBuf, u32)> = root
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == Kind::Dir)
            .map(|(i, e)| {
                let name = String::from_utf8_lossy(&e.name).into_owned();
                (NodeId(1 + i as u32), PathBuf::from("/r").join(name), 1)
            })
            .collect();
        assert_eq!(targets.len(), 3);

        run.handle.rescan_many(&targets);
        let msgs = run.until_done();
        let dones = msgs
            .iter()
            .filter(|m| matches!(m, ScanMsg::Done { .. }))
            .count();
        let relisted = listings(&msgs).len();
        assert_eq!((dones, relisted), (1, 3), "{msgs:?}");
        assert_eq!(msgs.last(), Some(&ScanMsg::Done { cancelled: false }));
        assert!(
            run.rx.recv_timeout(Duration::from_millis(10)).is_err(),
            "nothing after Done"
        );
    }
}

#[test]
fn rescan_many_clears_a_stop_like_rescan_does() {
    // Big enough that the scan is still running when it is stopped.
    let mut run = begin(FakeFs::generate("/r", 6, 4, 0), "/r", 2, true);
    run.next();
    run.control.cancel_all();
    while !matches!(run.next(), ScanMsg::Done { cancelled: true }) {}
    run.fs.dir("/");
    run.handle.rescan_many(&[(NodeId::ROOT, "/r".into(), 0)]);
    assert!(!run.control.is_cancel_all());
    loop {
        if matches!(run.next(), ScanMsg::Done { cancelled: false }) {
            break;
        }
    }
}
