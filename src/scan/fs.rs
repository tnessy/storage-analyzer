//! Fault-injection seam over the directory-walking syscalls (design §3.5).

use std::path::Path;

pub use rustix::io::Errno;

use crate::types::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatInfo {
    pub kind: Kind,
    pub disk_bytes: u64,
    pub apparent_bytes: u64,
    pub ino: u64,
    pub dev: u64,
    pub nlink: u32,
}

pub trait FsAccess: Send + Sync {
    type Dir: Send;

    /// `O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC` (+`O_NOATIME`).
    fn open_dir(&self, path: &Path) -> Result<Self::Dir, Errno>;
    /// Entry names, without `.` and `..`.
    fn read_entries(&self, dir: &mut Self::Dir) -> Result<Vec<Box<[u8]>>, Errno>;
    /// `AT_SYMLINK_NOFOLLOW`.
    fn stat_at(&self, dir: &Self::Dir, name: &[u8]) -> Result<StatInfo, Errno>;
}

/// The real syscall-backed implementation (T006).
pub struct RealFs;
