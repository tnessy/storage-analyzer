//! Small types shared by every module.

/// Index into the model arena. Never reused while the node exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

impl Kind {
    /// Kind from the `st_mode` / `stx_mode` file-type bits; never follows anything.
    pub fn from_raw_mode(mode: u32) -> Kind {
        const S_IFMT: u32 = 0o170_000;
        match mode & S_IFMT {
            0o100_000 => Kind::File,
            0o040_000 => Kind::Dir,
            0o120_000 => Kind::Symlink,
            _ => Kind::Other,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_from_raw_mode_reads_the_file_type_bits_only() {
        assert_eq!(Kind::from_raw_mode(0o100644), Kind::File);
        assert_eq!(Kind::from_raw_mode(0o040755), Kind::Dir);
        assert_eq!(Kind::from_raw_mode(0o120777), Kind::Symlink);
        for other in [0o010644, 0o020644, 0o060644, 0o140755] {
            assert_eq!(Kind::from_raw_mode(other), Kind::Other, "{other:o}");
        }
        assert_eq!(Kind::from_raw_mode(0o104755), Kind::File, "setuid bit");
        assert_eq!(Kind::from_raw_mode(0o041777), Kind::Dir, "sticky bit");
        assert_eq!(Kind::from_raw_mode(0), Kind::Other);
    }
}
