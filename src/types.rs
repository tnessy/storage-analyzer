//! Small types shared by every module.

/// Index into the model arena. Never reused while the node exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    Other,
}

/// The filesystem operation that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Open,
    Read,
    Stat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeMode {
    Disk,
    Apparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    PermissionDenied,
    Vanished,
    OtherFilesystem,
    SkippedByUser,
    Other(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanIssue {
    pub node: NodeId,
    pub op: Op,
    pub kind: IssueKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanState {
    Starting,
    Scanning,
    Done,
    Stopped,
}
