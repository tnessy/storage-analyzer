//! The largest-files lists keep a reserve, so deletes rarely need a rebuild (T030 part 4).

use std::path::PathBuf;
use std::time::Duration;

use proptest::prelude::*;
use storage_analyzer::delete::{DeleteEvent, Outcome};
use storage_analyzer::model::{Flags, Model, ModelOptions};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{Counts, Kind, NodeId, SizeMode};

const D: SizeMode = SizeMode::Disk;
const A: SizeMode = SizeMode::Apparent;

fn file(name: &str, disk: u64, apparent: u64) -> RawEntry {
    RawEntry {
        name: name.as_bytes().into(),
        kind: Kind::File,
        disk_bytes: disk,
        apparent_bytes: apparent,
        ino: 0,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

fn dir(name: &str) -> RawEntry {
    RawEntry {
        kind: Kind::Dir,
        disk_bytes: 4096,
        apparent_bytes: 0,
        ..file(name, 0, 0)
    }
}

fn model(top_n: usize) -> Model {
    let mut m = Model::new(PathBuf::from("/r"), dir("r"), ModelOptions { top_n });
    m.set_incremental_top(true);
    m
}

fn listing(dir: u32, first_id: u32, entries: Vec<RawEntry>) -> ScanMsg {
    ScanMsg::Listing {
        dir: NodeId(dir),
        first_id: NodeId(first_id),
        entries,
        last: true,
    }
}

fn files(prefix: &str, sizes: impl IntoIterator<Item = u64>) -> Vec<RawEntry> {
    sizes
        .into_iter()
        .map(|s| file(&format!("{prefix}{s}"), s * 4096, s))
        .collect()
}

fn delete(m: &mut Model, id: u32) {
    m.begin_delete(NodeId(id)).unwrap();
    m.apply_delete(&DeleteEvent::Finished(Outcome::Removed(Counts::default())));
}

fn finish_rebuild(m: &mut Model) {
    while !m.step_top_rebuild(Duration::from_millis(5)) {}
}

/// The lists as they must be: every counted file, biggest first, ties by smaller id.
fn expected(m: &Model, n_nodes: u32, mode: SizeMode, n: usize) -> Vec<NodeId> {
    let mut all: Vec<(u64, NodeId)> = (1..n_nodes)
        .map(NodeId)
        .filter(|&id| m.exists(id) && m.kind(id) == Kind::File)
        .filter(|&id| {
            let f = m.flags(id);
            !f.contains(Flags::HARDLINK_DUP)
                && !f.contains(Flags::ERROR)
                && !f.contains(Flags::OTHER_FS)
        })
        .map(|id| (m.size(id, mode), id))
        .collect();
    all.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    all.truncate(n);
    all.into_iter().map(|(_, id)| id).collect()
}

/// root: a(1) b(2); a holds `a_sizes`, b holds `b_sizes`.
fn two_folders(top_n: usize, a_sizes: Vec<u64>, b_sizes: Vec<u64>) -> (Model, u32) {
    let mut m = model(top_n);
    m.apply(listing(0, 1, vec![dir("a"), dir("b")]));
    let (na, nb) = (a_sizes.len() as u32, b_sizes.len() as u32);
    m.apply(listing(1, 3, files("a", a_sizes)));
    m.apply(listing(2, 3 + na, files("b", b_sizes)));
    (m, 3 + na + nb)
}

#[test]
fn deleting_a_folder_with_no_listed_file_does_not_rebuild() {
    // The 5 largest are all in b.
    let (mut m, end) = two_folders(5, (1..=40).collect(), (1000..1040).collect());
    delete(&mut m, 1);
    assert!(!m.top_updating());
    for mode in [D, A] {
        assert_eq!(m.top_files(mode), expected(&m, end, mode, 5));
    }
}

#[test]
fn the_reserve_absorbs_a_delete_that_takes_some_of_the_top() {
    // 3 of the top 5 are in a; the reserve of the next 20 covers the loss.
    let (mut m, end) = two_folders(5, vec![1000, 999, 998, 3, 2, 1], (100..160).collect());
    delete(&mut m, 1);
    assert!(!m.top_updating(), "the reserve was enough");
    for mode in [D, A] {
        assert_eq!(m.top_files(mode), expected(&m, end, mode, 5));
    }
}

#[test]
fn a_delete_that_empties_the_reserve_rebuilds_and_ends_correct() {
    let (mut m, end) = two_folders(5, (1000..1040).collect(), (1..100).collect());
    delete(&mut m, 1);
    assert!(m.top_updating(), "more than the reserve was lost");
    finish_rebuild(&mut m);
    for mode in [D, A] {
        assert_eq!(m.top_files(mode), expected(&m, end, mode, 5));
    }
}

#[test]
fn after_a_rebuild_the_reserve_is_back_and_later_deletes_within_it_do_not_rebuild() {
    // N = 5, so a list holds 25. a: the 40 biggest (more than the reserve), b: 5 next,
    // c: 20 next, d: small ones.
    let mut m = model(5);
    m.apply(listing(0, 1, vec![dir("a"), dir("b"), dir("c"), dir("d")]));
    m.apply(listing(1, 5, files("a", 1000..1040)));
    m.apply(listing(2, 45, files("b", 900..905)));
    m.apply(listing(3, 50, files("c", 800..820)));
    m.apply(listing(4, 70, files("d", 1..30)));
    delete(&mut m, 1);
    assert!(
        m.top_updating(),
        "losing more than the reserve needs a rebuild"
    );
    finish_rebuild(&mut m);
    assert_eq!(m.top_files(D), expected(&m, 99, D, 5));

    // The lists hold b (5) and c (20) again. Removing b takes 5, within the reserve of 20.
    delete(&mut m, 2);
    assert!(!m.top_updating(), "five entries are within the reserve");
    assert_eq!(m.top_files(D), expected(&m, 99, D, 5));
    // Removing c as well uses up the rest of the reserve.
    delete(&mut m, 3);
    assert!(m.top_updating());
    finish_rebuild(&mut m);
    assert_eq!(m.top_files(D), expected(&m, 99, D, 5));
}

#[test]
fn a_small_file_never_fills_a_slot_while_bigger_ones_are_unknown() {
    // 20 files, the list keeps the top N + reserve = 10. Deleting the 9 biggest leaves
    // one exact entry (11) while 10 and below sit outside the list.
    let mut m = model(2);
    m.apply(listing(0, 1, vec![dir("big"), dir("rest")]));
    m.apply(listing(1, 3, files("big", 12..=20)));
    m.apply(ScanMsg::Listing {
        dir: NodeId(2),
        first_id: NodeId(12),
        entries: files("rest", 1..=11),
        last: false,
    });
    delete(&mut m, 1);
    assert!(m.top_updating());
    // A tiny file arrives in the still open folder before the rebuild is done.
    m.apply(ScanMsg::Listing {
        dir: NodeId(2),
        first_id: NodeId(100),
        entries: files("tiny", [0]),
        last: true,
    });
    assert!(
        !m.top_files(D).contains(&NodeId(100)),
        "a tiny file must not take the free slot: {:?}",
        m.top_files(D)
    );
    finish_rebuild(&mut m);
    assert_eq!(m.top_files(D), expected(&m, 101, D, 2));
    assert_eq!(m.top_files(D).len(), 2);
}

#[test]
fn rescanning_the_root_clears_the_lists_without_a_rebuild() {
    let (mut m, _) = two_folders(5, (1..50).collect(), (100..150).collect());
    m.begin_rescan(NodeId::ROOT);
    assert!(!m.top_updating());
    assert!(m.top_files(D).is_empty());
    m.apply(listing(0, 500, vec![dir("n")]));
    m.apply(listing(500, 501, files("n", [7, 9, 8])));
    assert_eq!(m.top_files(D), vec![NodeId(502), NodeId(503), NodeId(501)]);
}

#[test]
fn a_root_rescan_during_a_rebuild_cancels_it() {
    let (mut m, _) = two_folders(5, (1000..1100).collect(), (1..60).collect());
    delete(&mut m, 1);
    assert!(m.top_updating());
    m.begin_rescan(NodeId::ROOT);
    assert!(!m.top_updating());
    assert!(m.top_files(D).is_empty());
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// Listings out of id order, deletes, folder and root rescans in the middle of a
    /// rebuild: whenever no rebuild runs the lists are exact, and after it finishes they
    /// equal a recount, in both size modes.
    #[test]
    fn the_lists_always_match_a_recount(seed in any::<u64>(), top_n in 1usize..8) {
        let mut rng = Rng(seed | 1);
        let mut m = model(top_n);
        let folders = 3 + rng.below(4) as u32;
        m.apply(listing(0, 1, (0..folders).map(|i| dir(&format!("d{i}"))).collect()));
        // Each folder's listing, with ids already assigned, applied in random order.
        let mut next_id = 1 + folders;
        let mut pending: Vec<(u32, u32, Vec<RawEntry>)> = Vec::new();
        for f in 0..folders {
            let count = rng.below(50) as u32;
            let sizes: Vec<u64> = (0..count).map(|_| 1 + rng.below(40)).collect();
            let entries = files(&format!("f{f}-"), sizes)
                .into_iter()
                .enumerate()
                .map(|(i, mut e)| { e.name = format!("f{f}-{i}").into_bytes().into(); e })
                .collect::<Vec<_>>();
            pending.push((1 + f, next_id, entries));
            next_id += count;
        }
        for _ in 0..40 {
            match rng.below(6) {
                0 | 1 if !pending.is_empty() => {
                    let (dir_id, first, entries) = pending.swap_remove(rng.below(pending.len() as u64) as usize);
                    m.apply(listing(dir_id, first, entries));
                }
                2 => { m.step_top_rebuild(Duration::ZERO); }
                3 => {
                    let candidates: Vec<u32> = (1..=folders)
                        .filter(|&f| m.exists(NodeId(f)) && m.delete_refusal(NodeId(f)).is_none())
                        .collect();
                    if let Some(&f) = candidates.get(rng.below(candidates.len().max(1) as u64) as usize) {
                        delete(&mut m, f);
                    }
                }
                4 => {
                    let f = 1 + rng.below(u64::from(folders)) as u32;
                    if m.exists(NodeId(f)) {
                        m.begin_rescan(NodeId(f));
                        let count = rng.below(30) as u32;
                        let sizes: Vec<u64> = (0..count).map(|_| 1 + rng.below(40)).collect();
                        pending.retain(|p| p.0 != f);
                        pending.push((f, next_id, files(&format!("r{f}-"), sizes)));
                        next_id += count;
                    }
                }
                5 if rng.below(8) == 0 => {
                    m.begin_rescan(NodeId::ROOT);
                    pending.clear();
                    let count = 2 + rng.below(40) as u32;
                    let sizes: Vec<u64> = (0..count).map(|_| 1 + rng.below(40)).collect();
                    m.apply(listing(0, next_id, files("root-", sizes)));
                    next_id += count;
                }
                _ => {}
            }
            if !m.top_updating() {
                for mode in [D, A] {
                    prop_assert_eq!(m.top_files(mode), expected(&m, next_id, mode, top_n));
                }
            }
        }
        for (dir_id, first, entries) in pending.drain(..) {
            m.apply(listing(dir_id, first, entries));
        }
        finish_rebuild(&mut m);
        for mode in [D, A] {
            prop_assert_eq!(m.top_files(mode), expected(&m, next_id, mode, top_n));
        }
    }
}
