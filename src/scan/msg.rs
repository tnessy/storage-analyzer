//! Messages from scan workers to the main thread.

use crate::types::{IssueKind, Kind, NodeId, Op};

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
    Listing {
        dir: NodeId,
        first_id: NodeId,
        entries: Vec<RawEntry>,
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
