//! Model side of delete: refusals, the confirmation numbers, tombstones, reconciliation (T011).

#[path = "support/fixtures.rs"]
mod fixtures;

use std::fs;
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use fixtures::{Scan, assert_matches_du, check_exact, write};
use storage_analyzer::delete::{
    AbortReason, DeleteEvent, DeleteFailKind, DeleteFailure, DeleteHandle, Outcome, spawn,
};
use storage_analyzer::model::{Flags, Model, ModelOptions, Reconcile, Refusal};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{Counts, IssueKind, Kind, NodeId, Op, SizeMode};

const D: SizeMode = SizeMode::Disk;
const A: SizeMode = SizeMode::Apparent;

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

fn dir(name: &str) -> RawEntry {
    raw(name, Kind::Dir, 4096, 0)
}

fn file(name: &str, disk: u64, apparent: u64) -> RawEntry {
    raw(name, Kind::File, disk, apparent)
}

fn link(name: &str, ino: u64, nlink: u32) -> RawEntry {
    RawEntry {
        ino,
        nlink,
        ..file(name, 4096, 100)
    }
}

fn model() -> Model {
    Model::new(PathBuf::from("/r"), dir("r"), ModelOptions { top_n: 10 })
}

fn listing(dir: u32, first_id: u32, entries: Vec<RawEntry>) -> ScanMsg {
    ScanMsg::Listing {
        dir: NodeId(dir),
        first_id: NodeId(first_id),
        entries,
    }
}

fn id(n: u32) -> NodeId {
    NodeId(n)
}

fn finished(o: Outcome) -> DeleteEvent {
    DeleteEvent::Finished(o)
}

/// root(0): v(1) k(2) f.txt(3); v: v1(4) a.bin(5) h(6, link ino 7); v1: b.bin(7); k: h2(8, same link).
fn built() -> Model {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![dir("v"), dir("k"), file("f.txt", 8192, 5000)],
    ));
    m.apply(listing(
        1,
        4,
        vec![dir("v1"), file("a.bin", 10_000, 9000), link("h", 7, 2)],
    ));
    m.apply(listing(4, 7, vec![file("b.bin", 4096, 10)]));
    m.apply(listing(2, 8, vec![link("h2", 7, 2)]));
    m.apply(ScanMsg::Done { cancelled: false });
    m
}

// ---- refusals --------------------------------------------------------------

#[test]
fn refusals_cover_the_root_other_filesystems_and_running_deletes() {
    let mut m = model();
    let mut mount = dir("mnt");
    mount.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
    m.apply(listing(0, 1, vec![dir("v"), dir("k"), mount]));
    m.apply(listing(1, 4, vec![dir("inner")]));
    assert_eq!(m.delete_refusal(NodeId::ROOT), Some(Refusal::ScanRoot));
    assert_eq!(m.delete_refusal(id(3)), Some(Refusal::OtherFilesystem));
    assert_eq!(m.delete_refusal(id(1)), None);

    let request = m.begin_delete(id(1)).unwrap();
    assert_eq!(request.node, id(1));
    assert_eq!(m.delete_refusal(id(1)), Some(Refusal::AlreadyDeleting));
    assert_eq!(
        m.delete_refusal(id(4)),
        Some(Refusal::AlreadyDeleting),
        "inside the target"
    );
    assert_eq!(m.delete_refusal(id(2)), Some(Refusal::DeleteRunning(id(1))));
    assert_eq!(
        m.begin_delete(id(2)).unwrap_err(),
        Refusal::DeleteRunning(id(1))
    );
    assert_eq!(m.begin_delete(NodeId::ROOT).unwrap_err(), Refusal::ScanRoot);
    assert!(m.flags(id(1)).contains(Flags::DELETING) && m.flags(id(1)).contains(Flags::TOMBSTONE));
    assert!(!m.flags(id(2)).contains(Flags::DELETING));
}

#[test]
fn the_request_carries_the_identity_and_totals_only_when_exact() {
    let mut m = built();
    let req = m.begin_delete(id(1)).unwrap();
    assert_eq!(req.rel_path, PathBuf::from("v"));
    assert_eq!((req.expect_dev, req.expect_ino), m.identity(id(1)));
    assert_eq!(req.kind, Kind::Dir);
    // v holds a.bin, h, b.bin (3 files) and v1 (1 folder), plus v itself.
    assert_eq!((req.total_files, req.total_dirs), (Some(3), Some(2)));

    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v"), file("f", 1, 1)]));
    m.apply(listing(1, 3, vec![dir("pending"), file("g", 1, 1)]));
    let req = m.begin_delete(id(1)).unwrap();
    assert_eq!((req.total_files, req.total_dirs), (None, None));

    let mut m = built();
    let req = m.begin_delete(id(3)).unwrap();
    assert_eq!(
        (req.kind, req.total_files, req.total_dirs),
        (Kind::File, Some(1), Some(0))
    );
    assert_eq!(req.rel_path, PathBuf::from("f.txt"));
}

// ---- delete_check ----------------------------------------------------------

#[test]
fn outside_links_counts_files_with_links_beyond_the_subtree() {
    let m = built();
    let check = m.delete_check(id(1), D);
    assert_eq!(check.outside_links, 1, "h has a second link in k");
    assert_eq!(
        check.freed_bytes_upper,
        m.size(id(1), D) - 4096,
        "h's bytes do not count"
    );
    let other = m.delete_check(id(2), D);
    assert_eq!(other.outside_links, 1);
    assert_eq!(
        other.freed_bytes_upper,
        m.size(id(2), D),
        "the duplicate adds nothing here"
    );

    let mut m = model();
    m.apply(listing(0, 1, vec![dir("d"), dir("e")]));
    m.apply(listing(
        1,
        3,
        vec![link("l1", 9, 2), link("l2", 9, 2), link("t1", 5, 3)],
    ));
    m.apply(listing(2, 6, vec![link("t2", 5, 3)]));
    let check = m.delete_check(id(1), D);
    assert_eq!(
        check.outside_links, 1,
        "both links of ino 9 are inside; ino 5 has one outside"
    );
}

#[test]
fn delete_check_numbers_follow_the_scan() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v")]));
    m.apply(listing(
        1,
        2,
        vec![
            file("f1", 4096, 100),
            dir("sub"),
            file("big", 40_000, 30_000),
        ],
    ));
    let mid = m.delete_check(id(1), D);
    assert!(!mid.exact);
    assert_eq!((mid.files, mid.dirs), (2, 1));
    assert_eq!(mid.size_bytes, m.size(id(1), D));
    assert_eq!(m.delete_check(id(1), A).size_bytes, 30_100);
    assert_eq!(mid.largest_known, [Some(id(4)), Some(id(2)), Some(id(3))]);

    m.apply(listing(
        3,
        5,
        vec![file("f2", 4096, 1), file("f3", 4096, 1)],
    ));
    m.apply(ScanMsg::Done { cancelled: false });
    let end = m.delete_check(id(1), D);
    assert!(end.exact);
    assert_eq!((end.files, end.dirs), (4, 1));
    assert!(
        end.files >= mid.files && end.dirs >= mid.dirs,
        "the early numbers are lower bounds"
    );
}

#[test]
fn a_skipped_folder_inside_makes_the_check_inexact() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v")]));
    m.apply(listing(1, 2, vec![dir("s")]));
    assert!(m.mark_skipped(id(2)));
    m.apply(ScanMsg::Cancelled { dir: id(2) });
    assert!(m.is_complete(id(1)));
    assert!(!m.delete_check(id(1), D).exact);
}

#[test]
fn opening_the_check_changes_nothing() {
    let m = built();
    let snapshot = |m: &Model| {
        (0..9)
            .map(NodeId)
            .map(|n| {
                (
                    m.exists(n),
                    m.size(n, D),
                    m.size(n, A),
                    m.flags(n),
                    m.counts(n),
                )
            })
            .collect::<Vec<_>>()
    };
    let before = snapshot(&m);
    let _ = (
        m.delete_check(id(1), D),
        m.delete_check(id(1), A),
        m.delete_refusal(id(1)),
    );
    assert_eq!(snapshot(&m), before);
}

// ---- tombstone -------------------------------------------------------------

#[test]
fn issues_under_a_tombstone_are_discarded_and_cancels_do_not_make_it_inexact() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v"), dir("k")]));
    m.apply(listing(1, 3, vec![dir("s1"), dir("s2"), dir("s3")]));
    m.begin_delete(id(1)).unwrap();

    m.apply(ScanMsg::DirIssue {
        dir: id(3),
        op: Op::Open,
        kind: IssueKind::PermissionDenied,
    });
    let mut bad = file("bad", 0, 0);
    bad.issue = Some((Op::Stat, IssueKind::Vanished));
    m.apply(listing(4, 6, vec![bad, file("ok", 4096, 5)]));
    assert!(m.issues().is_empty(), "{:?}", m.issues());
    assert_eq!(
        m.size(id(4), D),
        4096 + 4096,
        "listings under it are still applied"
    );

    m.apply(ScanMsg::Cancelled { dir: id(5) });
    assert!(m.is_complete(id(1)));
    assert!(m.is_exact(id(1)), "a delete's cancels are not skips");
}

#[test]
fn issues_elsewhere_are_still_recorded_during_a_delete() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v"), dir("k")]));
    m.begin_delete(id(1)).unwrap();
    m.apply(ScanMsg::DirIssue {
        dir: id(2),
        op: Op::Open,
        kind: IssueKind::PermissionDenied,
    });
    assert_eq!(m.issues().len(), 1);
}

// ---- reconciliation (synthetic events) ---------------------------------------

#[test]
fn removed_takes_the_subtree_out_and_recharges_the_surviving_link() {
    let mut m = built();
    assert!(m.flags(id(8)).contains(Flags::HARDLINK_DUP));
    m.begin_delete(id(1)).unwrap();
    let counts = Counts {
        files: 3,
        dirs: 2,
        bytes: 0,
    };
    assert_eq!(
        m.apply_delete(&finished(Outcome::Removed(counts))),
        Reconcile::None
    );

    for gone in [1, 4, 5, 6, 7] {
        assert!(!m.exists(id(gone)), "node {gone}");
    }
    assert!(!m.children(NodeId::ROOT).contains(&id(1)));
    assert!(
        !m.flags(id(8)).contains(Flags::HARDLINK_DUP),
        "h2 is the charged link now"
    );
    assert_eq!(m.size(id(8), D), 4096);
    assert_eq!(m.size(NodeId::ROOT, D), 4096 + 4096 + 8192 + 4096);
    assert_eq!(m.counts(NodeId::ROOT), (2, 1));
    assert_eq!(m.top_files(D), vec![id(3), id(8)]);
    let ext_count: u64 = m.ext_totals().iter().map(|(_, t)| t.count).sum();
    assert_eq!(ext_count, 2, "f.txt and h2");
    check_exact(&m, 9);
    // The tombstone is gone and a new delete can start.
    assert_eq!(m.delete_refusal(id(2)), None);
}

#[test]
fn deleting_a_top_level_dir_reduces_the_progress_total() {
    let mut m = built();
    assert_eq!(m.top_level_progress(), (2, 2));
    m.begin_delete(id(1)).unwrap();
    m.apply_delete(&finished(Outcome::Removed(Counts::default())));
    assert_eq!(m.top_level_progress(), (1, 1));

    let mut m = model();
    m.apply(listing(0, 1, vec![dir("v"), dir("k")]));
    m.apply(listing(2, 3, vec![]));
    assert_eq!(m.top_level_progress(), (1, 2));
    m.begin_delete(id(1)).unwrap();
    m.apply_delete(&finished(Outcome::Removed(Counts::default())));
    assert_eq!(
        m.top_level_progress(),
        (1, 1),
        "an unfinished one leaves done alone"
    );
}

#[test]
fn late_listings_for_removed_directories_are_ignored() {
    let mut m = built();
    m.begin_delete(id(1)).unwrap();
    m.apply_delete(&finished(Outcome::Removed(Counts::default())));
    let totals = m.totals();
    m.apply(listing(4, 30, vec![file("late", 4096, 1)]));
    m.apply(ScanMsg::Cancelled { dir: id(4) });
    m.apply(ScanMsg::DirIssue {
        dir: id(1),
        op: Op::Read,
        kind: IssueKind::Vanished,
    });
    assert_eq!(m.totals(), totals);
    assert!(!m.exists(id(30)));
    check_exact(&m, 9);
}

fn assert_nothing_deleting(m: &Model, max: u32) {
    for n in (0..max).map(NodeId).filter(|&n| m.exists(n)) {
        let f = m.flags(n);
        assert!(
            !f.contains(Flags::DELETING) && !f.contains(Flags::TOMBSTONE),
            "{n:?}"
        );
    }
}

#[test]
fn partial_clears_the_tombstone_and_asks_for_a_rescan() {
    let mut m = built();
    m.begin_delete(id(1)).unwrap();
    let failures = vec![
        DeleteFailure {
            rel_path: "v/a.bin".into(),
            kind: DeleteFailKind::PermissionDenied,
        },
        DeleteFailure {
            rel_path: "v/v1/b.bin".into(),
            kind: DeleteFailKind::Busy,
        },
        DeleteFailure {
            rel_path: "v/v1".into(),
            kind: DeleteFailKind::OtherFilesystem,
        },
    ];
    let partial = Outcome::Partial {
        removed: Counts::default(),
        failures,
    };
    assert_eq!(m.apply_delete(&finished(partial)), Reconcile::Rescan(id(1)));
    assert_nothing_deleting(&m, 9);
    assert!(
        m.issues().is_empty(),
        "delete failures live in the app, not in the scan issues"
    );
    assert_eq!(m.delete_refusal(id(2)), None);
}

#[test]
fn aborted_clears_the_tombstone_and_a_rescan_makes_an_unfinished_folder_exact_again() {
    for reason in [
        AbortReason::ChangedSinceScan,
        AbortReason::MountPoint,
        AbortReason::NotFound,
    ] {
        let mut m = model();
        m.apply(listing(0, 1, vec![dir("v")]));
        m.apply(listing(
            1,
            2,
            vec![dir("s1"), dir("s2"), file("f", 4096, 9)],
        ));
        let before = (
            m.size(NodeId::ROOT, D),
            m.size(NodeId::ROOT, A),
            m.counts(NodeId::ROOT),
        );
        m.begin_delete(id(1)).unwrap();
        // The app cancelled the subtree's jobs when the delete was confirmed.
        m.apply(ScanMsg::Cancelled { dir: id(2) });
        m.apply(ScanMsg::Cancelled { dir: id(3) });
        assert!(
            m.is_complete(id(1)),
            "looks finished, but only because of the cancels"
        );

        assert_eq!(
            m.apply_delete(&finished(Outcome::Aborted { reason })),
            Reconcile::Rescan(id(1))
        );
        assert_nothing_deleting(&m, 5);

        let (path, depth) = m.begin_rescan(id(1));
        assert_eq!((path, depth), (PathBuf::from("/r/v"), 1));
        m.apply(listing(
            1,
            10,
            vec![dir("s1"), dir("s2"), file("f", 4096, 9)],
        ));
        m.apply(listing(10, 13, vec![]));
        m.apply(listing(11, 13, vec![]));
        assert!(m.is_complete(id(1)) && m.is_exact(id(1)));
        assert_eq!(
            (
                m.size(NodeId::ROOT, D),
                m.size(NodeId::ROOT, A),
                m.counts(NodeId::ROOT)
            ),
            before
        );
        check_exact(&m, 14);
    }
}

#[test]
fn a_target_that_left_the_model_mid_delete_needs_no_rescan() {
    for outcome in [
        Outcome::Partial {
            removed: Counts::default(),
            failures: vec![],
        },
        Outcome::Aborted {
            reason: AbortReason::ChangedSinceScan,
        },
        Outcome::Removed(Counts::default()),
    ] {
        let mut m = built();
        m.begin_delete(id(1)).unwrap();
        m.begin_rescan(NodeId::ROOT); // e.g. `r` on the root while the delete runs
        assert!(!m.exists(id(1)));
        assert_eq!(m.apply_delete(&finished(outcome)), Reconcile::None);
        assert_eq!(m.delete_refusal(NodeId::ROOT), Some(Refusal::ScanRoot));
        m.apply(listing(0, 20, vec![dir("x")]));
        assert!(
            m.begin_delete(id(20)).is_ok(),
            "the delete slot is free again"
        );
    }
}

#[test]
fn progress_events_and_events_without_a_delete_change_nothing() {
    let mut m = built();
    let totals = m.totals();
    let progress = DeleteEvent::Progress {
        removed_files: 5,
        removed_dirs: 1,
        removed_bytes: 9,
    };
    assert_eq!(m.apply_delete(&progress), Reconcile::None);
    let removed = finished(Outcome::Removed(Counts::default()));
    assert_eq!(
        m.apply_delete(&removed),
        Reconcile::None,
        "nothing was begun"
    );
    assert_eq!(m.totals(), totals);
    m.begin_delete(id(1)).unwrap();
    assert_eq!(m.apply_delete(&progress), Reconcile::None);
    assert!(m.flags(id(1)).contains(Flags::TOMBSTONE));
}

// ---- real filesystem ---------------------------------------------------------

struct Deleter {
    handle: DeleteHandle,
    rx: Receiver<DeleteEvent>,
}

fn deleter(root: &Path) -> Deleter {
    let fd = OwnedFd::from(fs::File::open(root).unwrap());
    let (tx, rx) = sync_channel(256);
    Deleter {
        handle: spawn(fd, tx),
        rx,
    }
}

impl Deleter {
    /// Waits for the outcome, ignoring progress.
    fn outcome(&self) -> Outcome {
        loop {
            match self
                .rx
                .recv_timeout(Duration::from_secs(60))
                .expect("no Finished")
            {
                DeleteEvent::Finished(o) => return o,
                DeleteEvent::Progress { .. } => {}
            }
        }
    }
}

fn basic_fixture(root: &Path) {
    write(root.join("keep/a.dat"), 5000);
    write(root.join("keep/b/c.dat"), 3000);
    write(root.join("victim/x.dat"), 9000);
    write(root.join("victim/sub/y.dat"), 100);
    write(root.join("victim/sub/deeper/z.dat"), 20_000);
}

fn scanned(root: &Path) -> Scan {
    let mut scan = Scan::start(root, 2, 64);
    scan.until_done();
    scan
}

#[test]
fn confirming_a_delete_leaves_totals_equal_to_du() {
    let tmp = tempfile::tempdir().unwrap();
    basic_fixture(tmp.path());
    let mut scan = scanned(tmp.path());
    assert_matches_du(&scan.model, tmp.path());
    let victim = scan.find("victim").unwrap();

    let del = deleter(tmp.path());
    let req = scan.model.begin_delete(victim).unwrap();
    del.handle.submit(req).unwrap();
    let outcome = del.outcome();
    assert!(matches!(outcome, Outcome::Removed(_)), "{outcome:?}");
    assert_eq!(scan.model.apply_delete(&finished(outcome)), Reconcile::None);

    assert!(!tmp.path().join("victim").exists());
    assert_matches_du(&scan.model, tmp.path());
    assert!(!scan.model.exists(victim));
}

/// Two directories hold links to one file; deletes the one that holds the charged link
/// (`charged == true`) or the one that holds the duplicate.
fn delete_one_of_two_linked_dirs(charged: bool) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root.join("a/h1"), 7000);
    fs::create_dir_all(root.join("b")).unwrap();
    fs::hard_link(root.join("a/h1"), root.join("b/h2")).unwrap();
    write(root.join("a/other"), 100);
    write(root.join("b/other"), 100);
    let mut scan = scanned(root);
    assert_matches_du(&scan.model, root);

    let holds_charged = |scan: &Scan, dir: &str| {
        let dir = scan.find(dir).unwrap();
        scan.model.children(dir).iter().any(|&c| {
            scan.model.nlink(c) == 2 && !scan.model.flags(c).contains(Flags::HARDLINK_DUP)
        })
    };
    let target = ["a", "b"]
        .into_iter()
        .find(|d| holds_charged(&scan, d) == charged)
        .unwrap();
    let target = scan.find(target).unwrap();

    let del = deleter(root);
    del.handle
        .submit(scan.model.begin_delete(target).unwrap())
        .unwrap();
    let outcome = del.outcome();
    scan.model.apply_delete(&finished(outcome));
    assert_matches_du(&scan.model, root);
}

#[test]
fn hard_links_across_the_deleted_subtree_still_match_du() {
    delete_one_of_two_linked_dirs(true);
    delete_one_of_two_linked_dirs(false);
}

#[test]
fn declining_changes_neither_the_model_nor_the_files() {
    let tmp = tempfile::tempdir().unwrap();
    basic_fixture(tmp.path());
    let scan = scanned(tmp.path());
    let victim = scan.find("victim").unwrap();
    let listing_of = |root: &Path| {
        let mut all = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(p) = stack.pop() {
            for e in fs::read_dir(&p).unwrap() {
                let e = e.unwrap();
                all.push((e.path(), e.metadata().unwrap().len()));
                if e.file_type().unwrap().is_dir() {
                    stack.push(e.path());
                }
            }
        }
        all.sort();
        all
    };
    let files_before = listing_of(tmp.path());
    let snapshot = |m: &Model| {
        (0..m.totals().entries_seen as u32 + 1)
            .map(NodeId)
            .map(|n| (m.size(n, D), m.size(n, A), m.flags(n)))
            .collect::<Vec<_>>()
    };
    let before = snapshot(&scan.model);
    let _ = scan.model.delete_check(victim, D);
    let _ = scan.model.delete_check(victim, A);
    assert_eq!(snapshot(&scan.model), before);
    assert_eq!(listing_of(tmp.path()), files_before);
}

/// 20 directories of 1000 files each; `d7` is deleted while the scan is still running.
fn big_fixture(root: &Path) {
    for d in 0..20 {
        fs::create_dir_all(root.join(format!("d{d}"))).unwrap();
        for f in 0..1000 {
            fs::write(root.join(format!("d{d}/f{f}")), b"x").unwrap();
        }
    }
}

/// Applies scan and delete messages until the delete has finished, the scan has reported
/// `Done` and `settled` holds (a rescan started by `on_delete` may need more messages).
fn drive(
    scan: &mut Scan,
    del: &Deleter,
    mut on_delete: impl FnMut(&mut Scan, Outcome),
    settled: impl Fn(&Scan) -> bool,
) {
    let (mut seen_done, mut delete_done) = (false, false);
    let deadline = Instant::now() + Duration::from_secs(60);
    while !(delete_done && seen_done && settled(scan)) {
        assert!(Instant::now() < deadline, "stalled");
        loop {
            match del.rx.try_recv() {
                Ok(DeleteEvent::Finished(o)) => {
                    delete_done = true;
                    on_delete(scan, o);
                }
                Ok(DeleteEvent::Progress { .. }) => {}
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if seen_done && settled(scan) {
            std::thread::sleep(Duration::from_millis(5));
        } else if matches!(scan.step(), ScanMsg::Done { .. }) {
            seen_done = true;
        }
    }
}

#[test]
fn an_unfinished_folder_can_be_deleted_while_the_scan_runs() {
    let tmp = tempfile::tempdir().unwrap();
    big_fixture(tmp.path());
    let mut scan = Scan::start(tmp.path(), 1, 4);
    scan.step();
    let victim = scan.find("d7").unwrap();
    assert!(!scan.model.is_complete(victim));
    let req = scan.model.begin_delete(victim).unwrap();
    assert_eq!((req.total_files, req.total_dirs), (None, None));
    scan.control.cancel_prefix(scan.model.path(victim));

    let del = deleter(tmp.path());
    del.handle.submit(req).unwrap();
    drive(
        &mut scan,
        &del,
        |scan, outcome| {
            assert!(matches!(outcome, Outcome::Removed(_)), "{outcome:?}");
            assert_eq!(scan.model.apply_delete(&finished(outcome)), Reconcile::None);
        },
        |_| true,
    );

    assert!(!tmp.path().join("d7").exists());
    assert_matches_du(&scan.model, tmp.path());
    assert!(scan.model.is_exact(NodeId::ROOT));
    assert_nothing_deleting(&scan.model, 21_100);
}

#[test]
fn an_aborted_delete_of_an_unfinished_folder_is_repaired_by_a_rescan() {
    let tmp = tempfile::tempdir().unwrap();
    big_fixture(tmp.path());
    let mut scan = Scan::start(tmp.path(), 1, 4);
    scan.step();
    let victim = scan.find("d7").unwrap();
    let mut req = scan.model.begin_delete(victim).unwrap();
    req.expect_ino += 1; // the identity check fails: the folder is not what was scanned
    scan.control.cancel_prefix(scan.model.path(victim));

    let del = deleter(tmp.path());
    del.handle.submit(req).unwrap();
    drive(
        &mut scan,
        &del,
        |scan, outcome| {
            assert_eq!(
                outcome,
                Outcome::Aborted {
                    reason: AbortReason::ChangedSinceScan
                }
            );
            let plan = scan.model.apply_delete(&finished(outcome));
            assert_eq!(plan, Reconcile::Rescan(victim));
            scan.rescan(victim);
        },
        |scan| scan.model.is_complete(victim) && scan.model.is_exact(victim),
    );

    assert_nothing_deleting(&scan.model, 21_100);
    assert!(scan.model.is_complete(victim) && scan.model.is_exact(victim));
    assert!(tmp.path().join("d7").exists());
    assert_matches_du(&scan.model, tmp.path());
}

#[test]
fn a_partial_delete_asks_for_a_rescan_that_matches_du() {
    if rustix::process::geteuid().is_root() {
        println!("SKIP: permissions are not enforced for root");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root.join("victim/ok.txt"), 100);
    write(root.join("victim/ro/a"), 100);
    write(root.join("victim/ro/b"), 100);
    write(root.join("keep"), 100);
    fs::set_permissions(root.join("victim/ro"), fs::Permissions::from_mode(0o555)).unwrap();
    let mut scan = scanned(root);
    let victim = scan.find("victim").unwrap();

    let del = deleter(root);
    del.handle
        .submit(scan.model.begin_delete(victim).unwrap())
        .unwrap();
    let outcome = del.outcome();
    let Outcome::Partial { failures, .. } = &outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(failures.len(), 2);
    assert_eq!(
        scan.model.apply_delete(&finished(outcome.clone())),
        Reconcile::Rescan(victim)
    );
    assert!(
        scan.model.issues().is_empty(),
        "failures are reported from the outcome"
    );

    scan.rescan(victim);
    scan.until_done();
    fs::set_permissions(root.join("victim/ro"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_matches_du(&scan.model, root);
    assert_nothing_deleting(&scan.model, 20);
}

#[test]
fn a_folder_replaced_by_a_file_can_be_rescanned_and_then_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(root.join("victim/x.dat"), 100);
    write(root.join("other"), 100);
    let mut scan = scanned(root);
    let victim = scan.find("victim").unwrap();

    fs::remove_dir_all(root.join("victim")).unwrap();
    write(root.join("victim"), 500);
    scan.rescan(victim);
    scan.until_done();
    assert_eq!(scan.model.kind(victim), Kind::File);
    assert_matches_du(&scan.model, root);

    let req = scan.model.begin_delete(victim).unwrap();
    assert_eq!(req.kind, Kind::File);
    let meta = fs::symlink_metadata(root.join("victim")).unwrap();
    assert_eq!(req.expect_ino, std::os::unix::fs::MetadataExt::ino(&meta));
    let del = deleter(root);
    del.handle.submit(req).unwrap();
    let outcome = del.outcome();
    assert!(matches!(outcome, Outcome::Removed(_)), "{outcome:?}");
    assert_eq!(scan.model.apply_delete(&finished(outcome)), Reconcile::None);
    assert!(!root.join("victim").exists());
    assert_matches_du(&scan.model, root);
}

#[test]
fn delete_check_timing() {
    let nodes: u32 = if cfg!(debug_assertions) {
        100_000
    } else {
        1_000_000
    };
    let per_dir = 1000;
    let dirs = nodes / per_dir;
    let mut m = model();
    m.apply(listing(
        0,
        1,
        (0..dirs).map(|d| dir(&format!("d{d}"))).collect(),
    ));
    let mut next = dirs + 1;
    for d in 0..dirs {
        let entries = (0..per_dir)
            .map(|i| {
                let mut e = file(&format!("f{i}.dat"), 4096, 10);
                if i % 50 == 0 {
                    e.ino = u64::from(d) * 1000 + u64::from(i) + 1;
                    e.nlink = 2;
                }
                e
            })
            .collect();
        m.apply(listing(d + 1, next, entries));
        next += per_dir;
    }
    m.apply(ScanMsg::Done { cancelled: false });

    // One check per directory: together they cover every node once.
    let started = Instant::now();
    let mut outside = 0;
    for d in 0..dirs {
        outside += m.delete_check(id(d + 1), D).outside_links;
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let per_million = ms / f64::from(nodes) * 1_000_000.0;
    println!("TARGET: delete_check_ms_per_1M={per_million:.1} (target 50)");
    assert_eq!(outside, u64::from(nodes / 50));
}
