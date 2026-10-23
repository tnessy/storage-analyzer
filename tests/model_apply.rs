//! `Model::apply` behavior: aggregation, completeness, hard links, issues (T003).

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use proptest::prelude::*;
use storage_analyzer::model::{Flags, Model, ModelOptions};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op, ScanState, SizeMode};

const D: SizeMode = SizeMode::Disk;
const A: SizeMode = SizeMode::Apparent;

fn raw(name: &[u8], kind: Kind, disk: u64, apparent: u64) -> RawEntry {
    RawEntry {
        name: name.into(),
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
    raw(name.as_bytes(), Kind::Dir, 4096, 4096)
}

fn file(name: &str, disk: u64, apparent: u64) -> RawEntry {
    raw(name.as_bytes(), Kind::File, disk, apparent)
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

#[test]
fn new_model_has_pending_root() {
    let m = model();
    assert_eq!(m.root(), NodeId::ROOT);
    assert!(m.exists(NodeId::ROOT));
    assert_eq!(m.state(), ScanState::Starting);
    assert_eq!(m.totals().pending_dirs, 1);
    assert!(!m.is_complete(NodeId::ROOT));
    assert_eq!(m.size(NodeId::ROOT, D), 4096);
    assert_eq!(m.size(NodeId::ROOT, A), 0, "dir apparent size is 0");
    assert_eq!(m.parent(NodeId::ROOT), None);
    assert_eq!(m.depth(NodeId::ROOT), 0);
    assert!(!m.exists(id(1)));
}

#[test]
fn three_level_stream_keeps_live_totals_and_completeness() {
    let mut m = model();

    // r: a(1) b(2) f1(3)
    m.apply(listing(0, 1, vec![dir("a"), dir("b"), file("f1", 4096, 5)]));
    assert_eq!(m.state(), ScanState::Scanning);
    assert_eq!(m.size(NodeId::ROOT, D), 4 * 4096);
    assert_eq!(m.size(NodeId::ROOT, A), 5);
    assert_eq!(m.counts(NodeId::ROOT), (1, 2));
    assert_eq!(m.totals().pending_dirs, 2);
    assert!(!m.is_complete(NodeId::ROOT));
    assert!(m.is_complete(id(3)), "files are always complete");
    assert!(!m.is_complete(id(1)));
    assert_eq!(m.size(id(1), D), 4096, "unlisted dir has only its own size");
    assert_eq!(m.children(NodeId::ROOT), &[id(1), id(2), id(3)]);
    assert_eq!(m.parent(id(2)), Some(NodeId::ROOT));
    assert_eq!(m.depth(id(2)), 1);

    // a: a1(4) f2(5)
    m.apply(listing(1, 4, vec![dir("a1"), file("f2", 8192, 7000)]));
    assert_eq!(m.size(id(1), D), 4096 + 4096 + 8192);
    assert_eq!(m.size(id(1), A), 7000);
    assert_eq!(m.counts(id(1)), (1, 1));
    assert_eq!(m.size(NodeId::ROOT, D), 4 * 4096 + 4096 + 8192);
    assert_eq!(m.counts(NodeId::ROOT), (2, 3));
    assert_eq!(m.totals().pending_dirs, 2, "a done, a1 discovered");
    assert!(!m.is_complete(id(1)), "a1 still pending");

    // b: empty
    m.apply(listing(2, 6, vec![]));
    assert!(m.is_complete(id(2)));
    assert!(!m.is_complete(NodeId::ROOT));

    // a1: f3(6)
    m.apply(listing(4, 6, vec![file("f3", 4096, 1)]));
    assert!(m.is_complete(id(4)));
    assert!(
        m.is_complete(id(1)),
        "a completes when its last descendant does"
    );
    assert!(m.is_complete(NodeId::ROOT));
    assert_eq!(m.totals().pending_dirs, 0);
    assert_eq!(m.size(NodeId::ROOT, D), 4 * 4096 + 4096 + 8192 + 4096);
    assert_eq!(m.size(NodeId::ROOT, A), 5 + 7000 + 1);
    assert_eq!(m.counts(NodeId::ROOT), (3, 3));
    assert_eq!(m.totals().entries_seen, 6);
    assert_eq!(m.totals().files, 3);
    assert_eq!(m.totals().dirs, 3);
    assert!(m.is_exact(NodeId::ROOT));

    m.apply(ScanMsg::Done { cancelled: false });
    assert_eq!(m.state(), ScanState::Done);
}

#[test]
fn listing_with_out_of_order_id_blocks_is_applied() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    // b's block (5..6) arrives before a's block (3..5).
    m.apply(listing(2, 5, vec![file("fb", 4096, 1)]));
    assert!(!m.exists(id(3)), "gap ids do not exist yet");
    m.apply(listing(
        1,
        3,
        vec![file("fa", 4096, 1), file("fa2", 4096, 1)],
    ));
    assert_eq!(m.children(id(1)), &[id(3), id(4)]);
    assert_eq!(m.children(id(2)), &[id(5)]);
    assert!(m.is_complete(NodeId::ROOT));
    assert_eq!(m.counts(NodeId::ROOT), (3, 2));
    assert_eq!(m.name(id(4)), b"fa2");
}

#[test]
fn hard_link_is_charged_once() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![link("l1", 7, 2)]));
    m.apply(listing(2, 4, vec![link("l2", 7, 2)]));
    assert_eq!(m.size(NodeId::ROOT, D), 3 * 4096 + 4096);
    assert_eq!(m.size(NodeId::ROOT, A), 100);
    assert_eq!(m.size(id(2), D), 4096, "dup adds nothing to b");
    assert!(!m.flags(id(3)).contains(Flags::HARDLINK_DUP));
    assert!(m.flags(id(4)).contains(Flags::HARDLINK_DUP));
    assert_eq!(m.counts(NodeId::ROOT), (2, 2), "dups still count as files");
    assert_eq!(m.nlink(id(4)), 2);
    assert_eq!(m.identity(id(4)), (1, 7));
}

#[test]
fn distinct_inodes_and_single_links_are_not_deduplicated() {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![
            link("x", 7, 2),
            link("y", 8, 2),
            link("z", 9, 1),
            link("w", 9, 1),
        ],
    ));
    assert_eq!(m.size(NodeId::ROOT, D), 4096 * 5);
    for n in 1..=4 {
        assert!(!m.flags(id(n)).contains(Flags::HARDLINK_DUP));
    }
}

#[test]
fn dir_issue_completes_dir_with_known_so_far_size() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a")]));
    m.apply(ScanMsg::DirIssue {
        dir: id(1),
        op: Op::Open,
        kind: IssueKind::PermissionDenied,
    });
    assert!(m.is_complete(id(1)));
    assert!(m.is_complete(NodeId::ROOT));
    assert!(m.flags(id(1)).contains(Flags::ERROR));
    assert_eq!(m.issues().len(), 1);
    let issue = m.issue(id(1)).unwrap();
    assert_eq!(
        (issue.node, issue.op, issue.kind),
        (id(1), Op::Open, IssueKind::PermissionDenied)
    );
    assert_eq!(m.size(NodeId::ROOT, D), 2 * 4096);
    assert!(m.issue(NodeId::ROOT).is_none());
}

#[test]
fn entry_issue_is_recorded_on_the_entry() {
    let mut m = model();
    let mut bad = file("f", 0, 0);
    bad.issue = Some((Op::Stat, IssueKind::Vanished));
    m.apply(listing(0, 1, vec![bad]));
    let issue = m.issue(id(1)).unwrap();
    assert_eq!(
        (issue.node, issue.op, issue.kind),
        (id(1), Op::Stat, IssueKind::Vanished)
    );
    assert_eq!(m.issues().len(), 1);
    assert!(m.flags(id(1)).contains(Flags::ERROR));
}

#[test]
fn other_filesystem_dir_is_not_counted_or_pending() {
    let mut m = model();
    let mut mount = dir("mnt");
    mount.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
    m.apply(listing(0, 1, vec![mount, file("f", 4096, 1)]));
    assert!(m.flags(id(1)).contains(Flags::OTHER_FS));
    assert!(
        m.is_complete(NodeId::ROOT),
        "parent completes without the mount"
    );
    assert!(m.is_complete(id(1)));
    assert!(m.is_exact(NodeId::ROOT));
    assert_eq!(m.size(NodeId::ROOT, D), 2 * 4096, "root + f only");
    assert_eq!(m.counts(NodeId::ROOT), (1, 0));
    assert_eq!(m.issues().len(), 1);
}

#[test]
fn cancelled_dir_completes_parent_but_not_exactness() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(ScanMsg::Cancelled { dir: id(1) });
    m.apply(listing(2, 3, vec![]));
    assert!(m.flags(id(1)).contains(Flags::INCOMPLETE));
    assert!(m.is_complete(id(1)));
    assert!(!m.is_exact(id(1)));
    assert!(m.is_complete(NodeId::ROOT));
    assert!(!m.is_exact(NodeId::ROOT));
    assert!(m.is_exact(id(2)));
}

#[test]
fn stopped_scan_leaves_unread_dirs_and_ancestors_incomplete() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![dir("a1")]));
    m.apply(listing(2, 4, vec![]));
    m.apply(ScanMsg::Done { cancelled: true });
    assert_eq!(m.state(), ScanState::Stopped);
    for n in [0, 1, 3] {
        assert!(!m.is_complete(id(n)), "node {n}");
        assert!(!m.is_exact(id(n)));
    }
    assert!(m.flags(id(3)).contains(Flags::INCOMPLETE));
    assert!(m.is_complete(id(2)));
    assert_eq!(m.totals().pending_dirs, 1);
}

#[test]
fn messages_for_unknown_or_finished_dirs_are_ignored() {
    let mut m = model();
    m.apply(listing(0, 1, vec![file("f", 4096, 1)]));
    let before = m.totals();
    m.apply(listing(0, 5, vec![file("again", 4096, 1)]));
    m.apply(listing(99, 6, vec![file("x", 4096, 1)]));
    m.apply(ScanMsg::DirIssue {
        dir: id(77),
        op: Op::Read,
        kind: IssueKind::Vanished,
    });
    m.apply(ScanMsg::Cancelled { dir: id(1) });
    assert_eq!(m.totals(), before);
    assert!(m.issues().is_empty());
}

#[test]
fn children_sorted_is_size_desc_then_name_asc_and_stable() {
    let mut m = model();
    m.apply(listing(
        0,
        1,
        vec![
            file("b", 100, 1),
            file("a", 100, 1),
            file("big", 900, 1),
            file("c", 100, 500),
        ],
    ));
    let disk: Vec<_> = m.children_sorted(NodeId::ROOT, D);
    assert_eq!(disk, vec![id(3), id(2), id(1), id(4)]);
    assert_eq!(m.children_sorted(NodeId::ROOT, D), disk);
    let apparent = m.children_sorted(NodeId::ROOT, A);
    assert_eq!(apparent, vec![id(4), id(2), id(1), id(3)]);
}

#[test]
fn path_round_trips_raw_bytes() {
    let mut m = Model::new(
        PathBuf::from("/data"),
        dir("data"),
        ModelOptions { top_n: 1 },
    );
    let weird: &[u8] = b"bad\xff\xfe\nname";
    m.apply(listing(0, 1, vec![raw(weird, Kind::Dir, 4096, 0)]));
    m.apply(listing(1, 2, vec![file("leaf", 1, 1)]));
    assert_eq!(m.name(id(1)), weird);
    let mut expected = PathBuf::from("/data");
    expected.push(OsStr::from_bytes(weird));
    expected.push("leaf");
    assert_eq!(m.path(id(2)), expected);
    assert_eq!(m.path(NodeId::ROOT), PathBuf::from("/data"));
    let mut rel = PathBuf::from(OsStr::from_bytes(weird));
    rel.push("leaf");
    assert_eq!(m.rel_path(id(2)), rel);
    assert_eq!(m.rel_path(NodeId::ROOT), PathBuf::new());
    assert_eq!(m.depth(id(2)), 2);
}

#[test]
fn top_level_progress_counts_each_completed_top_dir_once() {
    let mut m = model();
    assert_eq!(m.top_level_progress(), (0, 0));
    let mut mount = dir("mnt");
    mount.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
    // a=1 b=2 c=3 mnt=4 f=5
    m.apply(listing(
        0,
        1,
        vec![dir("a"), dir("b"), dir("c"), mount, file("f", 1, 1)],
    ));
    assert_eq!(
        m.top_level_progress(),
        (0, 3),
        "files and OTHER_FS are not counted"
    );

    m.apply(listing(1, 6, vec![dir("a1")]));
    assert_eq!(m.top_level_progress(), (0, 3), "a still has a1 pending");
    m.apply(listing(2, 7, vec![]));
    assert_eq!(m.top_level_progress(), (1, 3));
    m.apply(listing(6, 7, vec![file("x", 1, 1)]));
    assert_eq!(
        m.top_level_progress(),
        (2, 3),
        "a completes with its last descendant"
    );
    m.apply(ScanMsg::Cancelled { dir: id(3) });
    assert_eq!(
        m.top_level_progress(),
        (3, 3),
        "a skipped dir has no work left"
    );
}

#[test]
fn top_level_progress_stays_below_total_after_a_stop() {
    let mut m = model();
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    m.apply(listing(1, 3, vec![]));
    m.apply(ScanMsg::Done { cancelled: true });
    assert_eq!(m.top_level_progress(), (1, 2));
}

#[derive(Debug, Clone)]
struct Spec {
    parent_seed: u32,
    is_dir: bool,
    disk: u64,
    apparent: u64,
}

fn spec() -> impl Strategy<Value = Spec> {
    (any::<u32>(), any::<bool>(), 0u64..100_000, 0u64..100_000).prop_map(
        |(parent_seed, is_dir, disk, apparent)| Spec {
            parent_seed,
            is_dir,
            disk,
            apparent,
        },
    )
}

proptest! {
    #[test]
    fn random_streams_converge_to_the_sum_of_all_entries(
        specs in prop::collection::vec(spec(), 0..60),
        picks in prop::collection::vec(any::<usize>(), 200),
    ) {
        // Node 0 is the root dir; node i+1 is specs[i], child of an earlier dir.
        let mut dirs = vec![0usize];
        let mut parent_of = vec![0usize];
        let mut children: Vec<Vec<usize>> = vec![vec![]];
        for (i, s) in specs.iter().enumerate() {
            let idx = i + 1;
            let p = dirs[s.parent_seed as usize % dirs.len()];
            parent_of.push(p);
            children.push(vec![]);
            children[p].push(idx);
            if s.is_dir {
                dirs.push(idx);
            }
        }
        let entry = |idx: usize| {
            let s = &specs[idx - 1];
            let kind = if s.is_dir { Kind::Dir } else { Kind::File };
            raw(format!("n{idx}").as_bytes(), kind, s.disk, s.apparent)
        };

        let mut m = Model::new(PathBuf::from("/r"), raw(b"r", Kind::Dir, 4096, 0), ModelOptions { top_n: 5 });
        let mut node_id = vec![NodeId::ROOT];
        node_id.resize(specs.len() + 1, NodeId(u32::MAX));
        let mut next_id = 1u32;
        let mut frontier = vec![0usize];
        let mut step = 0;
        while !frontier.is_empty() {
            let pick = picks[step % picks.len()] % frontier.len();
            step += 1;
            let d = frontier.swap_remove(pick);
            let kids = &children[d];
            for (k, &c) in kids.iter().enumerate() {
                node_id[c] = NodeId(next_id + k as u32);
            }
            m.apply(ScanMsg::Listing {
                dir: node_id[d],
                first_id: NodeId(next_id),
                entries: kids.iter().map(|&c| entry(c)).collect(),
            });
            next_id += kids.len() as u32;
            frontier.extend(kids.iter().copied().filter(|&c| specs[c - 1].is_dir));
        }
        m.apply(ScanMsg::Done { cancelled: false });

        let disk: u64 = 4096 + specs.iter().map(|s| s.disk).sum::<u64>();
        let apparent: u64 = specs.iter().filter(|s| !s.is_dir).map(|s| s.apparent).sum();
        let files = specs.iter().filter(|s| !s.is_dir).count() as u64;
        let dir_count = specs.iter().filter(|s| s.is_dir).count() as u64;
        prop_assert_eq!(m.size(NodeId::ROOT, D), disk);
        prop_assert_eq!(m.size(NodeId::ROOT, A), apparent);
        prop_assert_eq!(m.counts(NodeId::ROOT), (files, dir_count));
        prop_assert_eq!(m.totals().entries_seen, specs.len() as u64);
        prop_assert_eq!(m.totals().pending_dirs, 0);
        for &d in &dirs {
            prop_assert!(m.is_complete(node_id[d]));
            prop_assert!(m.is_exact(node_id[d]));
        }
        for (idx, &p) in parent_of.iter().enumerate().skip(1) {
            prop_assert_eq!(m.parent(node_id[idx]), Some(node_id[p]));
        }
    }
}
