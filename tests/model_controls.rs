//! Skip, rescan, identity, focus and rescan planning in the model (T012).

#[path = "support/fake_fs.rs"]
mod fake_fs;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::Duration;

use fake_fs::{DEV, FakeFs};
use storage_analyzer::model::{Flags, Model, ModelOptions, RescanPlan, ext_of};
use storage_analyzer::scan::{RawEntry, ScanControl, ScanHandle, ScanMsg, ScanOptions, start};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op, ScanState, SizeMode};

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

fn link(name: &str, ino: u64, disk: u64) -> RawEntry {
    RawEntry {
        ino,
        nlink: 2,
        ..file(name, disk, 100)
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
        last: true,
    }
}

fn id(n: u32) -> NodeId {
    NodeId(n)
}

/// root(0): a(1) b(2) f.txt(3); a: a1(4) x.mp4(5); a1: y.txt(6); b: z.zip(7).
fn built() -> Model {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![dir("a"), dir("b"), file("f.txt", 8192, 5000)],
    ));
    m.apply(listing(
        1,
        4,
        vec![dir("a1"), file("x.mp4", 100_000, 90_000)],
    ));
    m.apply(listing(4, 6, vec![file("y.txt", 4096, 10)]));
    m.apply(listing(2, 7, vec![file("z.zip", 20_000, 19_000)]));
    m.apply(ScanMsg::Done { cancelled: false });
    m
}

fn subtree_done(m: &Model, n: NodeId) -> bool {
    m.flags(n).contains(Flags::DONE)
        && m.children(n)
            .iter()
            .all(|&c| m.kind(c) != Kind::Dir || subtree_done(m, c))
}

/// The aggregates, counts and completeness equal a recount over the nodes that exist.
fn check_exact(m: &Model, max_id: u32) {
    let live: Vec<NodeId> = (0..max_id).map(NodeId).filter(|&n| m.exists(n)).collect();
    let is_dir = |n: NodeId| m.kind(n) == Kind::Dir && !m.flags(n).contains(Flags::OTHER_FS);
    let dirs = live.iter().filter(|&&n| is_dir(n)).count() as u64;
    let others: Vec<NodeId> = live
        .iter()
        .copied()
        .filter(|&n| m.kind(n) != Kind::Dir)
        .collect();
    let disk: u64 = 4096 * dirs + others.iter().map(|&n| m.size(n, D)).sum::<u64>();
    let apparent: u64 = others.iter().map(|&n| m.size(n, A)).sum();
    assert_eq!(m.size(NodeId::ROOT, D), disk, "disk");
    assert_eq!(m.size(NodeId::ROOT, A), apparent, "apparent");
    assert_eq!(
        m.counts(NodeId::ROOT),
        (others.len() as u64, dirs - 1),
        "counts"
    );
    for &n in &live {
        for &c in m.children(n) {
            assert!(m.exists(c), "{n:?} lists the removed child {c:?}");
            assert_eq!(m.parent(c), Some(n));
        }
        if m.kind(n) == Kind::Dir {
            assert_eq!(
                m.is_complete(n),
                subtree_done(m, n),
                "completeness of {n:?}"
            );
        }
    }
    let mut ext_count = 0;
    for (_, t) in m.ext_totals() {
        ext_count += t.count;
    }
    let counted = others
        .iter()
        .filter(|&&n| {
            let f = m.flags(n);
            !f.contains(Flags::HARDLINK_DUP) && !f.contains(Flags::ERROR)
        })
        .count() as u64;
    assert_eq!(ext_count, counted, "ext index count");
}

// ---- skip ----------------------------------------------------------------

#[test]
fn mark_skipped_lists_the_skip_and_the_cancel_completes_the_dir_inexactly() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b"), file("f", 1, 1)]));
    assert!(m.mark_skipped(id(1)));
    assert_eq!(m.issues().len(), 1);
    assert_eq!(
        (m.issues()[0].node, m.issues()[0].kind),
        (id(1), IssueKind::SkippedByUser)
    );
    assert!(m.issue(id(1)).is_some());
    assert!(!m.mark_skipped(id(3)), "files cannot be skipped");

    m.apply(ScanMsg::Cancelled { dir: id(1) });
    m.apply(listing(2, 4, vec![]));
    assert!(m.is_complete(id(1)) && !m.is_exact(id(1)));
    assert!(m.is_complete(NodeId::ROOT) && !m.is_exact(NodeId::ROOT));
    assert!(m.is_exact(id(2)));
    assert!(!m.mark_skipped(id(2)), "a complete dir is a no-op");
    assert_eq!(m.issues().len(), 1);
}

#[test]
fn skipping_a_listed_but_unfinished_dir_works_through_its_cancelled_children() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a")]));
    m.apply(listing(1, 2, vec![dir("a1")]));
    assert!(!m.is_complete(id(1)));
    assert!(m.mark_skipped(id(1)));
    m.apply(ScanMsg::Cancelled { dir: id(2) });
    assert!(m.is_complete(id(1)) && !m.is_exact(id(1)));
    assert!(m.is_complete(NodeId::ROOT) && !m.is_exact(NodeId::ROOT));
}

// ---- begin_rescan --------------------------------------------------------

#[test]
fn begin_rescan_removes_the_subtree_and_a_fresh_listing_is_applied() {
    let mut m = built();
    let (path, depth) = m.begin_rescan(id(1));
    assert_eq!((path, depth), (PathBuf::from("/r/a"), 1));
    for gone in [4, 5, 6] {
        assert!(!m.exists(id(gone)), "node {gone}");
    }
    assert!(m.children(id(1)).is_empty());
    assert!(!m.flags(id(1)).contains(Flags::DONE));
    assert!(m.flags(id(1)).contains(Flags::PENDING));
    assert!(!m.is_complete(id(1)) && !m.is_complete(NodeId::ROOT));
    assert_eq!(m.totals().pending_dirs, 1);
    assert_eq!(m.counts(NodeId::ROOT), (2, 2));
    assert_eq!(m.size(NodeId::ROOT, D), 3 * 4096 + 8192 + 20_000);
    check_exact(&m, 8);

    let keys: Vec<_> = m
        .ext_totals()
        .into_iter()
        .map(|(k, t)| (k, t.count))
        .collect();
    assert_eq!(keys.len(), 2, "only txt (f.txt) and zip remain: {keys:?}");
    assert_eq!(m.top_files(D), vec![id(7), id(3)]);

    m.apply(listing(
        1,
        8,
        vec![dir("a1"), file("x.mp4", 100_000, 90_000)],
    ));
    assert_eq!(m.children(id(1)), &[id(8), id(9)]);
    m.apply(listing(8, 10, vec![file("y.txt", 4096, 10)]));
    check_exact(&m, 11);
    assert!(m.is_exact(NodeId::ROOT));
    assert_eq!(
        m.size(NodeId::ROOT, D),
        3 * 4096 + 4096 + 8192 + 100_000 + 4096 + 20_000
    );
    assert_eq!(m.counts(NodeId::ROOT), (4, 3));
}

#[test]
fn begin_rescan_clears_inner_skips_and_keeps_other_issue_indexes_right() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![dir("a1"), dir("a2")]));
    assert!(m.mark_skipped(id(3)));
    m.apply(ScanMsg::DirIssue {
        dir: id(2),
        op: Op::Open,
        kind: IssueKind::PermissionDenied,
    });
    m.apply(ScanMsg::Cancelled { dir: id(3) });
    m.apply(listing(4, 5, vec![]));
    assert!(m.is_complete(id(1)) && !m.is_exact(id(1)));
    assert_eq!(m.issues().len(), 2);

    m.begin_rescan(id(1));
    assert_eq!(m.issues().len(), 1, "the skip under a is dropped");
    assert_eq!(
        m.issue(id(2)).map(|i| i.kind),
        Some(IssueKind::PermissionDenied)
    );
    assert!(
        m.issues()
            .iter()
            .all(|i| i.kind != IssueKind::SkippedByUser)
    );

    m.apply(listing(1, 6, vec![dir("a1")]));
    m.apply(listing(6, 7, vec![]));
    assert!(m.is_exact(id(1)) && m.is_complete(NodeId::ROOT));
    check_exact(&m, 8);
}

#[test]
fn removing_issues_keeps_the_list_in_insertion_order() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b"), dir("c")]));
    for d in [1, 2, 3] {
        m.apply(ScanMsg::DirIssue {
            dir: id(d),
            op: Op::Open,
            kind: IssueKind::PermissionDenied,
        });
    }
    let nodes = |m: &Model| m.issues().iter().map(|i| i.node).collect::<Vec<_>>();
    assert_eq!(nodes(&m), vec![id(1), id(2), id(3)]);

    m.begin_rescan(id(1));
    assert_eq!(nodes(&m), vec![id(2), id(3)]);
    assert_eq!(m.issue(id(3)).map(|i| i.node), Some(id(3)));
    assert_eq!(m.issue(id(2)).map(|i| i.node), Some(id(2)));
    assert!(m.issue(id(1)).is_none());
}

#[test]
fn removing_a_subtree_with_issues_keeps_the_rest_in_order() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![dir("a1"), dir("a2")]));
    m.apply(ScanMsg::DirIssue {
        dir: id(2),
        op: Op::Open,
        kind: IssueKind::PermissionDenied,
    });
    m.apply(ScanMsg::DirIssue {
        dir: id(3),
        op: Op::Open,
        kind: IssueKind::Vanished,
    });
    m.apply(ScanMsg::DirIssue {
        dir: id(4),
        op: Op::Read,
        kind: IssueKind::Other(5),
    });
    let before: Vec<_> = m.issues().iter().map(|i| i.node).collect();
    assert_eq!(before, vec![id(2), id(3), id(4)]);

    m.begin_rescan(id(1));
    let after: Vec<_> = m.issues().iter().map(|i| (i.node, i.kind)).collect();
    assert_eq!(after, vec![(id(2), IssueKind::PermissionDenied)]);
    assert_eq!(m.issue(id(2)).map(|i| i.node), Some(id(2)));
}

#[test]
fn begin_rescan_recharges_a_hard_link_whose_counted_occurrence_was_removed() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![link("l1", 7, 4096)]));
    m.apply(listing(2, 4, vec![link("l2", 7, 4096)]));
    let total = 3 * 4096 + 4096;
    assert_eq!(m.size(NodeId::ROOT, D), total);
    assert!(m.flags(id(4)).contains(Flags::HARDLINK_DUP));

    m.begin_rescan(id(1));
    assert_eq!(m.size(NodeId::ROOT, D), total, "b's link is charged now");
    assert!(!m.flags(id(4)).contains(Flags::HARDLINK_DUP));
    assert_eq!(m.size(id(4), D), 4096);
    assert_eq!(m.top_files(D), vec![id(4)]);
    check_exact(&m, 5);

    m.apply(listing(1, 5, vec![link("l1", 7, 4096)]));
    assert_eq!(m.size(NodeId::ROOT, D), total);
    assert!(
        m.flags(id(5)).contains(Flags::HARDLINK_DUP),
        "the new link is the duplicate"
    );
    check_exact(&m, 6);
    let ext_total: u64 = m.ext_totals().iter().map(|(_, t)| t.count).sum();
    assert_eq!(ext_total, 1);
}

#[test]
fn late_messages_for_removed_nodes_are_ignored() {
    let mut m = built();
    m.begin_rescan(id(1));
    let before = (m.totals(), m.issues().len());
    m.apply(listing(4, 20, vec![file("late", 4096, 1)]));
    m.apply(ScanMsg::DirIssue {
        dir: id(4),
        op: Op::Read,
        kind: IssueKind::Vanished,
    });
    m.apply(ScanMsg::Cancelled { dir: id(4) });
    m.apply(listing(99, 21, vec![]));
    assert_eq!((m.totals(), m.issues().len()), before);
    assert!(!m.exists(id(20)));
    check_exact(&m, 8);
}

// ---- identity ------------------------------------------------------------

fn ident(name: &str, kind: Kind, disk: u64, apparent: u64, ino: u64) -> RawEntry {
    RawEntry {
        ino,
        ..raw(name, kind, disk, apparent)
    }
}

#[test]
fn a_dir_that_became_a_file_turns_into_a_leaf_with_exact_totals() {
    let mut m = built();
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("a", Kind::File, 8192, 6500, 77),
    });
    assert_eq!(m.kind(id(1)), Kind::File);
    assert_eq!((m.size(id(1), D), m.size(id(1), A)), (8192, 6500));
    assert_eq!(m.identity(id(1)), (1, 77));
    assert!(m.is_complete(id(1)) && m.is_exact(id(1)));
    assert!(m.is_complete(NodeId::ROOT));
    assert_eq!(m.totals().pending_dirs, 0);
    assert_eq!(m.counts(NodeId::ROOT), (3, 1));
    assert_eq!(m.top_level_progress(), (1, 1));
    assert_eq!(m.top_files(D), vec![id(7), id(1), id(3)]);
    check_exact(&m, 8);
}

#[test]
fn a_dir_that_became_a_symlink_has_no_children_and_is_complete() {
    let mut m = built();
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("a", Kind::Symlink, 0, 8, 5),
    });
    assert_eq!(m.kind(id(1)), Kind::Symlink);
    assert!(m.children(id(1)).is_empty());
    assert_eq!(m.size(id(1), A), 8);
    assert!(m.is_complete(NodeId::ROOT));
    check_exact(&m, 8);
}

#[test]
fn a_replaced_dir_takes_the_new_inode_and_is_listed_again() {
    let mut m = built();
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("a", Kind::Dir, 4096, 0, 99),
    });
    assert_eq!(m.identity(id(1)), (1, 99));
    assert_eq!(m.kind(id(1)), Kind::Dir);
    m.apply(listing(1, 8, vec![file("new.txt", 4096, 3)]));
    assert!(m.is_exact(NodeId::ROOT));
    check_exact(&m, 9);
}

#[test]
fn a_rescan_root_that_vanished_is_removed_and_not_counted() {
    let mut m = built();
    m.begin_rescan(id(1));
    m.apply(ScanMsg::DirIssue {
        dir: id(1),
        op: Op::Stat,
        kind: IssueKind::Vanished,
    });
    assert!(!m.exists(id(1)));
    assert!(!m.exists(id(4)));
    assert!(
        m.issues().is_empty(),
        "nothing is left to attach an issue to"
    );
    assert_eq!(m.counts(NodeId::ROOT), (2, 1));
    assert!(m.is_complete(NodeId::ROOT));
    assert_eq!(m.top_level_progress(), (1, 1));
    assert!(!m.children(NodeId::ROOT).contains(&id(1)));
    check_exact(&m, 8);
}

#[test]
fn vanished_below_a_rescan_root_is_an_ordinary_issue() {
    let mut m = built();
    m.begin_rescan(id(1));
    m.apply(listing(1, 8, vec![dir("a1")]));
    m.apply(ScanMsg::DirIssue {
        dir: id(8),
        op: Op::Open,
        kind: IssueKind::Vanished,
    });
    assert!(m.exists(id(8)));
    assert_eq!(m.issue(id(8)).map(|i| i.kind), Some(IssueKind::Vanished));
    assert!(m.is_complete(id(1)));
    check_exact(&m, 9);
}

#[test]
fn an_inode_change_on_a_hard_linked_file_recharges_the_other_link() {
    let mut m = model();
    m.apply(listing(0, 1, vec![link("f", 5, 4096), link("g", 5, 4096)]));
    assert!(m.flags(id(2)).contains(Flags::HARDLINK_DUP));
    assert_eq!(m.size(NodeId::ROOT, D), 4096 + 4096);

    let (path, depth) = m.begin_rescan(id(1));
    assert_eq!((path, depth), (PathBuf::from("/r/f"), 1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("f", Kind::File, 8192, 200, 9),
    });
    assert_eq!(m.identity(id(1)), (1, 9));
    assert_eq!(m.size(id(1), D), 8192);
    assert!(
        !m.flags(id(2)).contains(Flags::HARDLINK_DUP),
        "g is the counted link now"
    );
    assert_eq!(m.size(id(2), D), 4096);
    assert_eq!(m.size(NodeId::ROOT, D), 4096 + 8192 + 4096);
    assert_eq!(m.top_files(D), vec![id(1), id(2)]);
    check_exact(&m, 3);
}

#[test]
fn a_changed_file_size_updates_totals_ext_and_top_list() {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![file("big.bin", 100_000, 90_000), file("mid.bin", 50_000, 1)],
    ));
    assert_eq!(m.top_files(D), vec![id(1), id(2)]);
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("big.bin", Kind::File, 10_000, 9, 3),
    });
    assert_eq!(m.top_files(D), vec![id(2), id(1)]);
    let (_, bin) = m
        .ext_totals()
        .into_iter()
        .find(|(k, _)| *k == ext_of(b"x.bin"))
        .unwrap();
    assert_eq!((bin.disk_bytes, bin.count), (60_000, 2));
    check_exact(&m, 3);
}

// ---- progress, focus, stop -------------------------------------------------

#[test]
fn top_level_progress_follows_rescans_and_skips() {
    let mut m = built();
    assert_eq!(m.top_level_progress(), (2, 2));
    m.begin_rescan(id(4));
    assert_eq!(
        m.top_level_progress(),
        (1, 2),
        "a rescan inside a top-level dir reopens it"
    );
    m.apply(listing(4, 8, vec![]));
    assert_eq!(m.top_level_progress(), (2, 2));

    m.begin_rescan(id(1));
    assert_eq!(m.top_level_progress(), (1, 2));
    m.apply(listing(1, 9, vec![]));
    assert_eq!(m.top_level_progress(), (2, 2));

    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(2, 3, vec![]));
    assert_eq!(m.top_level_progress(), (1, 2));
    assert!(m.mark_skipped(id(1)));
    m.apply(ScanMsg::Cancelled { dir: id(1) });
    assert_eq!(
        m.top_level_progress(),
        (2, 2),
        "a skipped folder counts as done"
    );
}

#[test]
fn the_focus_completion_is_reported_once_at_the_right_message() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![dir("a1")]));
    m.set_focus(Some(id(1)));
    assert_eq!(m.take_focus_completed(), None);
    m.apply(listing(2, 4, vec![]));
    assert_eq!(m.take_focus_completed(), None, "a is still scanning");
    m.apply(listing(3, 5, vec![]));
    assert_eq!(m.take_focus_completed(), Some(id(1)));
    assert_eq!(m.take_focus_completed(), None);

    m.set_focus(Some(id(2)));
    assert_eq!(
        m.take_focus_completed(),
        None,
        "already complete: nothing to report"
    );
    m.set_focus(None);
    assert_eq!(m.take_focus_completed(), None);
}

#[test]
fn a_stopped_scan_can_be_rescanned_from_the_root() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b"), file("f", 4096, 1)]));
    m.apply(listing(1, 4, vec![dir("a1")]));
    m.apply(ScanMsg::Done { cancelled: true });
    assert_eq!(m.state(), ScanState::Stopped);
    assert!(!m.is_complete(NodeId::ROOT));

    let (path, depth) = m.begin_rescan(NodeId::ROOT);
    assert_eq!((path, depth), (PathBuf::from("/r"), 0));
    assert_eq!(m.counts(NodeId::ROOT), (0, 0));
    assert_eq!(m.top_level_progress(), (0, 0));
    assert!(m.children(NodeId::ROOT).is_empty());
    m.apply(listing(0, 10, vec![dir("a"), dir("b"), file("f", 4096, 1)]));
    m.apply(listing(10, 13, vec![]));
    m.apply(listing(11, 14, vec![]));
    m.apply(ScanMsg::Done { cancelled: false });
    assert_eq!(m.state(), ScanState::Done);
    assert!(m.is_exact(NodeId::ROOT));
    assert_eq!(m.top_level_progress(), (2, 2));
    check_exact(&m, 15);
}

// ---- rescan_plan -----------------------------------------------------------

#[test]
fn a_complete_folder_gets_a_full_rescan() {
    let m = built();
    assert_eq!(m.rescan_plan(id(1)), RescanPlan::Full);
    assert_eq!(m.rescan_plan(NodeId::ROOT), RescanPlan::Full, "after Done");
}

#[test]
fn an_unfinished_folder_is_prioritised_and_the_root_does_nothing_while_scanning() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    assert_eq!(
        m.rescan_plan(id(1)),
        RescanPlan::PrioritiseAndResume { resume: vec![] }
    );
    assert_eq!(m.rescan_plan(NodeId::ROOT), RescanPlan::NoneWhileScanning);
}

#[test]
fn only_outermost_skips_inside_the_folder_are_resumed() {
    let mut m = model();
    // root: a(1) c(2); a: s1(3) s2(4); s1: inner(5)
    m.apply(listing(0, 1, vec![dir("a"), dir("c")]));
    m.apply(listing(1, 3, vec![dir("s1"), dir("s2")]));
    m.apply(listing(3, 5, vec![dir("inner")]));
    assert!(m.mark_skipped(id(3)));
    assert!(m.mark_skipped(id(5)));
    assert!(m.mark_skipped(id(2)), "outside the folder");
    assert_eq!(
        m.rescan_plan(id(1)),
        RescanPlan::PrioritiseAndResume {
            resume: vec![id(3)]
        }
    );
    assert_eq!(
        m.rescan_plan(id(3)),
        RescanPlan::PrioritiseAndResume {
            resume: vec![id(3)]
        }
    );
    assert_eq!(
        m.rescan_plan(id(5)),
        RescanPlan::PrioritiseAndResume {
            resume: vec![id(5)]
        }
    );
}

#[test]
fn after_a_stop_an_unfinished_folder_gets_a_full_rescan() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![dir("a1")]));
    m.apply(ScanMsg::Done { cancelled: true });
    assert!(!m.is_complete(id(1)));
    assert_eq!(m.rescan_plan(id(1)), RescanPlan::Full);
    assert_eq!(m.rescan_plan(NodeId::ROOT), RescanPlan::Full);
}

// ---- with the real walker --------------------------------------------------

struct Live {
    rx: Receiver<ScanMsg>,
    handle: ScanHandle,
    control: Arc<ScanControl>,
    model: Model,
    listed: HashMap<PathBuf, usize>,
}

fn live(fs: FakeFs, control: Arc<ScanControl>) -> Live {
    let (tx, rx) = sync_channel(1);
    let handle = start(
        Arc::new(fs),
        PathBuf::from("/r"),
        DEV,
        ScanOptions {
            threads: 1,
            one_filesystem: true,
            skip_pseudo: false,
        },
        Arc::clone(&control),
        tx,
        1,
    );
    Live {
        rx,
        handle,
        control,
        model: model(),
        listed: HashMap::new(),
    }
}

impl Live {
    fn step(&mut self) -> ScanMsg {
        let msg = self
            .rx
            .recv_timeout(Duration::from_secs(10))
            .expect("scan stalled");
        if let ScanMsg::Listing { dir, .. } = &msg {
            *self.listed.entry(self.model.path(*dir)).or_default() += 1;
        }
        self.model.apply(msg.clone());
        msg
    }

    fn until_done(&mut self) {
        while !matches!(self.step(), ScanMsg::Done { .. }) {}
    }

    fn find(&self, path: &str) -> Option<NodeId> {
        let mut cur = NodeId::ROOT;
        for part in Path::new(path).strip_prefix("/r").unwrap().components() {
            let name = part.as_os_str().as_encoded_bytes();
            cur = *self
                .model
                .children(cur)
                .iter()
                .find(|&&c| self.model.name(c) == name)?;
        }
        Some(cur)
    }

    /// Runs the app-side steps for resuming `node` after a skip.
    fn resume(&mut self, node: NodeId) {
        let path = self.model.path(node);
        self.control.remove_prefixes_under(&path);
        let (path, depth) = self.model.begin_rescan(node);
        self.handle.rescan(node, path, depth);
    }
}

fn reference(make: impl Fn() -> FakeFs) -> Model {
    let mut run = live(make(), Arc::new(ScanControl::new()));
    run.until_done();
    run.model
}

fn same_content(a: &Model, b: &Model) {
    for mode in [D, A] {
        assert_eq!(
            a.size(NodeId::ROOT, mode),
            b.size(NodeId::ROOT, mode),
            "{mode:?}"
        );
    }
    assert_eq!(a.counts(NodeId::ROOT), b.counts(NodeId::ROOT));
    assert_eq!(a.ext_totals(), b.ext_totals());
    assert!(a.is_exact(NodeId::ROOT), "the root is exact again");
    assert!(
        a.issues()
            .iter()
            .all(|i| i.kind != IssueKind::SkippedByUser)
    );
}

fn tree() -> FakeFs {
    FakeFs::generate("/r", 2, 3, 1)
}

#[test]
fn skipping_and_then_resuming_gives_the_unskipped_totals() {
    let expected = reference(tree);
    let control = Arc::new(ScanControl::new());
    control.cancel_prefix("/r/d0".into());
    let mut run = live(tree(), control);
    while run.find("/r/d0").is_none() {
        run.step();
    }
    let d0 = run.find("/r/d0").unwrap();
    assert!(run.model.mark_skipped(d0));
    run.until_done();

    assert!(run.model.is_complete(NodeId::ROOT) && !run.model.is_exact(NodeId::ROOT));
    assert!(run.model.is_complete(d0) && !run.model.is_exact(d0));
    assert!(run.model.size(NodeId::ROOT, D) < expected.size(NodeId::ROOT, D));
    assert_eq!(
        run.model.issue(d0).map(|i| i.kind),
        Some(IssueKind::SkippedByUser)
    );

    run.resume(d0);
    run.until_done();
    same_content(&run.model, &expected);
}

#[test]
fn rescanning_a_folder_clears_the_skips_inside_it() {
    let expected = reference(tree);
    let control = Arc::new(ScanControl::new());
    control.cancel_prefix("/r/d0/d1".into());
    let mut run = live(tree(), control);
    while run.find("/r/d0/d1").is_none() {
        run.step();
    }
    let inner = run.find("/r/d0/d1").unwrap();
    assert!(run.model.mark_skipped(inner));
    run.until_done();
    assert!(!run.model.is_exact(NodeId::ROOT));

    let d0 = run.find("/r/d0").unwrap();
    run.resume(d0);
    run.until_done();
    same_content(&run.model, &expected);
}

#[test]
fn rescan_plan_for_an_unfinished_folder_lists_nothing_twice_and_ends_exact() {
    let expected = reference(tree);
    let control = Arc::new(ScanControl::new());
    control.cancel_prefix("/r/d1/d0".into());
    let mut run = live(tree(), control);

    // Run until the skipped folder S is cancelled while its parent F is still unfinished.
    loop {
        let msg = run.step();
        if let ScanMsg::Listing { dir, .. } = &msg
            && run.model.path(*dir) == Path::new("/r/d1")
        {
            let s = run.find("/r/d1/d0").unwrap();
            assert!(run.model.mark_skipped(s));
        }
        if let ScanMsg::Cancelled { dir } = &msg
            && run.model.path(*dir) == Path::new("/r/d1/d0")
        {
            break;
        }
    }
    let (f, s) = (run.find("/r/d1").unwrap(), run.find("/r/d1/d0").unwrap());
    assert!(!run.model.is_complete(f));
    let plan = run.model.rescan_plan(f);
    assert_eq!(plan, RescanPlan::PrioritiseAndResume { resume: vec![s] });

    let RescanPlan::PrioritiseAndResume { resume } = plan else {
        unreachable!()
    };
    run.control.set_focus(Some(run.model.path(f)));
    run.model.set_focus(Some(f));
    for node in resume {
        run.resume(node);
    }
    run.until_done();

    same_content(&run.model, &expected);
    for (path, n) in &run.listed {
        assert_eq!(*n, 1, "{} was listed {n} times", path.display());
    }
    assert!(
        run.listed.contains_key(Path::new("/r/d1/d0")),
        "S is listed after the resume"
    );
    assert_eq!(run.listed.len(), 13, "every directory exactly once");
    assert_eq!(run.model.take_focus_completed(), Some(f));
}

// ---- T027: top-level progress and other filesystems ------------------------

fn top_dir_with_entry_issue(name: &str, kind: IssueKind) -> RawEntry {
    let mut e = dir(name);
    e.issue = Some((Op::Stat, kind));
    e
}

#[test]
fn rescanning_a_top_level_dir_whose_entry_had_an_issue_keeps_progress_consistent() {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![
            top_dir_with_entry_issue("a", IssueKind::PermissionDenied),
            dir("b"),
        ],
    ));
    m.apply(listing(2, 3, vec![]));
    assert_eq!(
        m.top_level_progress(),
        (1, 1),
        "the unreadable entry was never descended"
    );

    m.begin_rescan(id(1));
    assert_eq!(
        m.top_level_progress(),
        (1, 2),
        "it is a pending top-level dir now"
    );
    m.apply(listing(1, 4, vec![file("x", 4096, 1)]));
    assert_eq!(m.top_level_progress(), (2, 2));
    assert!(m.issues().is_empty());

    let mut clean = model();
    clean.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    clean.apply(listing(2, 3, vec![]));
    clean.apply(listing(1, 4, vec![file("x", 4096, 1)]));
    assert_eq!(clean.top_level_progress(), m.top_level_progress());
    check_exact(&m, 5);
}

#[test]
fn a_counted_top_level_dir_that_turns_into_a_leaf_leaves_the_total() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(2, 3, vec![]));
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("a", Kind::File, 4096, 1, 5),
    });
    assert_eq!(m.top_level_progress(), (1, 1));
    // And back: a file that became a directory counts from now on.
    m.begin_rescan(id(1));
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("a", Kind::Dir, 4096, 0, 6),
    });
    assert_eq!(m.top_level_progress(), (1, 2));
    m.apply(listing(1, 4, vec![]));
    assert_eq!(m.top_level_progress(), (2, 2));
}

fn with_mount() -> Model {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![
            top_dir_with_entry_issue("mnt", IssueKind::OtherFilesystem),
            dir("b"),
        ],
    ));
    m.apply(listing(2, 3, vec![]));
    m
}

#[test]
fn another_filesystem_is_never_rescanned() {
    let mut m = with_mount();
    assert_eq!(m.rescan_plan(id(1)), RescanPlan::OtherFilesystem);
    m.apply(ScanMsg::Done { cancelled: false });
    assert_eq!(
        m.rescan_plan(id(1)),
        RescanPlan::OtherFilesystem,
        "also after Done"
    );

    let before = (
        m.totals(),
        m.top_level_progress(),
        m.flags(id(1)),
        m.issues().len(),
    );
    let (path, depth) = m.begin_rescan(id(1));
    assert_eq!((path, depth), (PathBuf::from("/r/mnt"), 1));
    assert_eq!(
        (
            m.totals(),
            m.top_level_progress(),
            m.flags(id(1)),
            m.issues().len()
        ),
        before
    );
    m.apply(ScanMsg::Identity {
        node: id(1),
        entry: ident("mnt", Kind::Dir, 4096, 0, 9),
    });
    assert_eq!(m.totals(), before.0, "nothing changed");
    assert!(m.flags(id(1)).contains(Flags::OTHER_FS));
}

#[test]
fn a_rescan_root_found_on_another_device_becomes_an_other_filesystem_node() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![file("x", 4096, 1)]));
    m.apply(listing(2, 4, vec![]));
    m.begin_rescan(id(1));
    let mut entry = ident("a", Kind::Dir, 4096, 0, 9);
    entry.dev = 2;
    entry.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
    m.apply(ScanMsg::Identity { node: id(1), entry });

    let expected = with_mount_named("a");
    assert!(m.flags(id(1)).contains(Flags::OTHER_FS));
    assert_eq!(
        m.issue(id(1)).map(|i| i.kind),
        Some(IssueKind::OtherFilesystem)
    );
    assert!(m.is_complete(id(1)) && m.is_complete(NodeId::ROOT));
    assert_eq!(m.size(NodeId::ROOT, D), expected.size(NodeId::ROOT, D));
    assert_eq!(m.counts(NodeId::ROOT), expected.counts(NodeId::ROOT));
    assert_eq!(m.top_level_progress(), (1, 1));
    assert_eq!(m.rescan_plan(id(1)), RescanPlan::OtherFilesystem);
    check_exact(&m, 5);
}

fn with_mount_named(name: &str) -> Model {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![
            top_dir_with_entry_issue(name, IssueKind::OtherFilesystem),
            dir("b"),
        ],
    ));
    m.apply(listing(2, 3, vec![]));
    m
}

mod progress_property {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashSet;

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    fn random_entry(rng: &mut Rng, n: u64) -> RawEntry {
        let name = format!("e{n}");
        match rng.below(6) {
            0 => file(&name, 4096, 1),
            1 => top_dir_with_entry_issue(&name, IssueKind::PermissionDenied),
            2 => top_dir_with_entry_issue(&name, IssueKind::OtherFilesystem),
            _ => dir(&name),
        }
    }

    proptest! {
        #[test]
        fn top_level_progress_matches_a_recount_through_random_operations(seed in any::<u64>()) {
            let mut rng = Rng(seed | 1);
            let mut m = model();
            let mut next_id = 1u32;
            let mut counted: HashSet<NodeId> = HashSet::new();
            let mut root_listed = false;
            for step in 0..60u64 {
                let pending: Vec<NodeId> = (1..next_id)
                    .map(NodeId)
                    .filter(|&n| m.exists(n) && m.kind(n) == Kind::Dir)
                    .filter(|&n| !m.flags(n).contains(Flags::DONE))
                    .collect();
                if !root_listed {
                    let entries: Vec<RawEntry> =
                        (0..2 + rng.below(4)).map(|i| random_entry(&mut rng, i)).collect();
                    for (i, e) in entries.iter().enumerate() {
                        if e.kind == Kind::Dir && e.issue.is_none() {
                            counted.insert(NodeId(next_id + i as u32));
                        }
                    }
                    let n = entries.len() as u32;
                    m.apply(listing(0, next_id, entries));
                    next_id += n;
                    root_listed = true;
                } else {
                    match rng.below(6) {
                        0 | 1 if !pending.is_empty() => {
                            let d = pending[rng.below(pending.len() as u64) as usize];
                            let entries: Vec<RawEntry> = (0..rng.below(4))
                                .map(|i| random_entry(&mut rng, step * 10 + i))
                                .collect();
                            let n = entries.len() as u32;
                            m.apply(listing(d.0, next_id, entries));
                            next_id += n;
                        }
                        2 if !pending.is_empty() => {
                            let d = pending[rng.below(pending.len() as u64) as usize];
                            m.apply(ScanMsg::DirIssue {
                                dir: d,
                                op: Op::Open,
                                kind: IssueKind::PermissionDenied,
                            });
                        }
                        3 if !pending.is_empty() => {
                            let d = pending[rng.below(pending.len() as u64) as usize];
                            m.apply(ScanMsg::Cancelled { dir: d });
                        }
                        4 => {
                            let all: Vec<NodeId> =
                                (1..next_id).map(NodeId).filter(|&n| m.exists(n)).collect();
                            if all.is_empty() {
                                continue;
                            }
                            let n = all[rng.below(all.len() as u64) as usize];
                            if m.rescan_plan(n) == RescanPlan::OtherFilesystem {
                                continue;
                            }
                            m.begin_rescan(n);
                            if m.parent(n) == Some(NodeId::ROOT) && m.kind(n) == Kind::Dir {
                                counted.insert(n);
                            }
                            if rng.below(3) == 0 {
                                // The rescan finds a leaf instead.
                                let e = ident("e", Kind::File, 4096, 1, 3);
                                m.apply(ScanMsg::Identity { node: n, entry: e });
                                counted.remove(&n);
                            }
                        }
                        _ => {}
                    }
                }
                let live: Vec<NodeId> = counted
                    .iter()
                    .copied()
                    .filter(|&n| m.exists(n) && m.kind(n) == Kind::Dir)
                    .collect();
                let done = live.iter().filter(|&&n| m.is_complete(n)).count() as u32;
                let (got_done, got_total) = m.top_level_progress();
                prop_assert!(got_done <= got_total);
                prop_assert_eq!((got_done, got_total), (done, live.len() as u32));
            }
        }
    }
}
