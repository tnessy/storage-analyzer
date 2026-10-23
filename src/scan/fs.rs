//! Fault-injection seam over the directory-walking syscalls (design §3.5).

use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::{AtFlags, CWD, Mode, OFlags, RawDir, StatxFlags, makedev, openat, statx};
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

/// The real syscall-backed implementation.
pub struct RealFs;

/// An open directory; closed on drop.
#[derive(Debug)]
pub struct RealDir {
    fd: OwnedFd,
}

const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const STAT_MASK: StatxFlags = StatxFlags::TYPE
    .union(StatxFlags::MODE)
    .union(StatxFlags::NLINK)
    .union(StatxFlags::INO)
    .union(StatxFlags::SIZE)
    .union(StatxFlags::BLOCKS);
const DIR_BUF_BYTES: usize = 32 * 1024;

fn retry<T>(mut f: impl FnMut() -> Result<T, Errno>) -> Result<T, Errno> {
    loop {
        match f() {
            Err(Errno::INTR) => continue,
            other => return other,
        }
    }
}

impl FsAccess for RealFs {
    type Dir = RealDir;

    fn open_dir(&self, path: &Path) -> Result<RealDir, Errno> {
        // O_NOATIME keeps the scan from writing access times; only the owner may use it.
        let open = |flags| retry(|| openat(CWD, path, flags, Mode::empty()));
        match open(DIR_FLAGS | OFlags::NOATIME) {
            Err(Errno::PERM) => open(DIR_FLAGS),
            other => other,
        }
        .map(|fd| RealDir { fd })
    }

    fn read_entries(&self, dir: &mut RealDir) -> Result<Vec<Box<[u8]>>, Errno> {
        let mut buf = Box::<[u8]>::new_uninit_slice(DIR_BUF_BYTES);
        let mut names = Vec::new();
        let mut raw = RawDir::new(&dir.fd, &mut buf);
        while let Some(entry) = raw.next() {
            let entry = entry?;
            let name = entry.file_name().to_bytes();
            if name != b"." && name != b".." {
                names.push(name.into());
            }
        }
        Ok(names)
    }

    fn stat_at(&self, dir: &RealDir, name: &[u8]) -> Result<StatInfo, Errno> {
        let sx = retry(|| statx(&dir.fd, name, AtFlags::SYMLINK_NOFOLLOW, STAT_MASK))?;
        let kind = Kind::from_raw_mode(u32::from(sx.stx_mode));
        Ok(StatInfo {
            kind,
            disk_bytes: sx.stx_blocks * 512,
            apparent_bytes: sx.stx_size,
            ino: sx.stx_ino,
            // Same encoding as `st_dev`, and as the delete executor's statx.
            dev: makedev(sx.stx_dev_major, sx.stx_dev_minor),
            nlink: sx.stx_nlink,
        })
    }
}
