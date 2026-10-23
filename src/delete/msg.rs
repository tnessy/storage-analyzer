//! Messages between the main thread and the delete executor.

use std::path::PathBuf;

use crate::types::{Counts, Kind, NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteRequest {
    pub node: NodeId,
    pub rel_path: PathBuf,
    pub expect_dev: u64,
    pub expect_ino: u64,
    pub kind: Kind,
    /// `None` when the target is not exact.
    pub total_files: Option<u64>,
    pub total_dirs: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeleteEvent {
    Progress {
        removed_files: u64,
        removed_dirs: u64,
        removed_bytes: u64,
    },
    Finished(Outcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Removed(Counts),
    Partial {
        removed: Counts,
        failures: Vec<DeleteFailure>,
    },
    Aborted {
        reason: AbortReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteFailure {
    pub rel_path: PathBuf,
    pub kind: DeleteFailKind,
}

/// `ENOENT` is not a failure: it counts as removed (plan D8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteFailKind {
    PermissionDenied,
    Busy,
    ReadOnlyFs,
    OtherFilesystem,
    Other(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbortReason {
    ChangedSinceScan,
    NotFound,
    /// The target is itself a mount point (another filesystem or a bind mount).
    MountPoint,
    OpenFailed(i32),
}
