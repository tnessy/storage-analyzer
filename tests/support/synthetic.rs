//! Synthetic `ScanMsg` streams and fixtures shared by the app tests.

#![allow(dead_code)]

use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::{RawEntry, ScanMsg};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op};

pub const ROOT_PATH: &str = "/synthetic/root";

pub fn entry(name: &str, kind: Kind, bytes: u64) -> RawEntry {
    RawEntry {
        name: name.as_bytes().into(),
        kind,
        disk_bytes: bytes,
        apparent_bytes: bytes,
        ino: 1,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

pub fn root_entry() -> RawEntry {
    entry(ROOT_PATH, Kind::Dir, 4096)
}

pub fn model() -> Model {
    Model::new(ROOT_PATH.into(), root_entry(), ModelOptions { top_n: 100 })
}

pub fn listing(dir: NodeId, first_id: u32, entries: Vec<RawEntry>) -> ScanMsg {
    ScanMsg::Listing {
        dir,
        first_id: NodeId(first_id),
        entries,
        last: true,
    }
}

pub fn done(cancelled: bool) -> ScanMsg {
    ScanMsg::Done { cancelled }
}

/// A small finished scan: root with a directory and a file.
pub fn small_tree() -> Vec<ScanMsg> {
    vec![
        listing(
            NodeId::ROOT,
            1,
            vec![
                entry("docs", Kind::Dir, 4096),
                entry("a.bin", Kind::File, 10_000),
            ],
        ),
        listing(NodeId(1), 3, vec![entry("b.txt", Kind::File, 500)]),
    ]
}

/// `total_dirs + 1000 + 1` small messages (`total_dirs` must be a multiple of 1,000): the root lists
/// 1,000 directories, each of those lists `total_dirs / 1000` directories, and each of those lists
/// one file. No single message is large.
pub fn flood(total_dirs: u32) -> impl Iterator<Item = ScanMsg> {
    const TOP: u32 = 1000;
    let per_top = total_dirs / TOP;
    let top = (0..TOP)
        .map(|i| entry(&format!("t{i}"), Kind::Dir, 4096))
        .collect();
    let level2 = (0..TOP).map(move |t| {
        let dirs = (0..per_top)
            .map(|i| entry(&format!("d{i}"), Kind::Dir, 4096))
            .collect();
        listing(NodeId(1 + t), 1 + TOP + t * per_top, dirs)
    });
    let files = (0..total_dirs).map(move |d| {
        listing(
            NodeId(1 + TOP + d),
            1 + TOP + total_dirs + d,
            vec![entry("f", Kind::File, 100)],
        )
    });
    std::iter::once(listing(NodeId::ROOT, 1, top))
        .chain(level2)
        .chain(files)
}

/// An endless stream of cheap messages (re-listing an already listed directory), for sustained load.
pub fn endless() -> impl Iterator<Item = ScanMsg> {
    std::iter::repeat_with(|| listing(NodeId::ROOT, 1, Vec::new()))
}

/// One directory entry of a synthetic listing.
pub struct Item {
    name: String,
    kind: Kind,
    bytes: u64,
    issue: Option<(Op, IssueKind)>,
}

pub fn file(name: &str, bytes: u64) -> Item {
    Item {
        name: name.into(),
        kind: Kind::File,
        bytes,
        issue: None,
    }
}

/// A directory entry with no size of its own; its contents arrive in a later listing.
pub fn folder(name: &str) -> Item {
    Item {
        name: name.into(),
        kind: Kind::Dir,
        bytes: 0,
        issue: None,
    }
}

impl Item {
    pub fn bytes(mut self, bytes: u64) -> Self {
        self.bytes = bytes;
        self
    }

    pub fn issue(mut self, kind: IssueKind) -> Self {
        self.issue = Some((Op::Open, kind));
        self
    }
}

/// A model plus a node-id allocator, so tests can write listings without bookkeeping.
pub struct Scan {
    pub model: Model,
    next: u32,
}

impl Scan {
    /// A scan whose root directory has no size of its own, so child percentages are round.
    pub fn new() -> Self {
        let root = entry(ROOT_PATH, Kind::Dir, 0);
        Self {
            model: Model::new(ROOT_PATH.into(), root, ModelOptions { top_n: 100 }),
            next: 1,
        }
    }

    /// Like `new`, keeping only the `top_n` largest files (`--top N`).
    pub fn with_top(top_n: usize) -> Self {
        let root = entry(ROOT_PATH, Kind::Dir, 0);
        Self {
            model: Model::new(ROOT_PATH.into(), root, ModelOptions { top_n }),
            next: 1,
        }
    }

    /// Applies a listing of `dir` and returns the new ids in order.
    pub fn list(&mut self, dir: NodeId, items: Vec<Item>) -> Vec<NodeId> {
        let first = self.next;
        self.next += items.len() as u32;
        let entries = items
            .into_iter()
            .map(|i| {
                let mut e = entry(&i.name, i.kind, i.bytes);
                e.issue = i.issue;
                e
            })
            .collect::<Vec<_>>();
        let ids = (first..self.next).map(NodeId).collect();
        self.model.apply(listing(dir, first, entries));
        ids
    }
}
