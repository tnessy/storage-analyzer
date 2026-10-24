//! Messages from scan workers to the main thread.

use crate::types::{IssueKind, Kind, NodeId, Op};

/// Most entries in one `Listing` part; larger directories are split so that no single
/// apply step on the main thread takes more than a few milliseconds.
pub const LISTING_PART_ENTRIES: usize = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawEntry {
    pub name: Box<[u8]>,
    pub kind: Kind,
    pub disk_bytes: u64,
    pub apparent_bytes: u64,
    pub ino: u64,
    pub dev: u64,
    pub nlink: u32,
    pub issue: Option<(Op, IssueKind)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanMsg {
    /// The entries' ids are `first_id .. first_id + entries.len()`.
    ///
    /// A directory with more than `LISTING_PART_ENTRIES` entries arrives in several parts
    /// with contiguous ids; only the `last` part completes the directory, and its child
    /// jobs are queued after it (design §4).
    Listing {
        dir: NodeId,
        first_id: NodeId,
        entries: Vec<RawEntry>,
        last: bool,
    },
    /// Counts as "listed" for `pending_dirs`.
    DirIssue {
        dir: NodeId,
        op: Op,
        kind: IssueKind,
    },
    /// Job dropped by `ScanControl` (skip or delete).
    Cancelled {
        dir: NodeId,
    },
    /// A rescan root, re-stat'ed without following symlinks, sent before its `Listing`.
    /// If the target is no longer a directory, no `Listing` follows.
    Identity {
        node: NodeId,
        entry: RawEntry,
    },
    Done {
        cancelled: bool,
    },
}
