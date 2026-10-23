//! Thin rustix wrappers: every call is fd-relative and never follows symlinks.

use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

use rustix::fs::{
    AtFlags, Dir, FileType, Mode, OFlags, ResolveFlags, Statx, StatxFlags, makedev, openat,
    openat2, statx, unlinkat,
};

pub use rustix::io::Errno;

use crate::types::Kind;

/// What the executor needs to know about an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub dev: u64,
    pub ino: u64,
    pub kind: Kind,
    pub disk_bytes: u64,
    pub nlink: u32,
    /// Mount id, if the kernel reports it. Tells bind mounts from plain subdirectories.
    pub mnt_id: Option<u64>,
}

impl Meta {
    /// Same device and, when the kernel reports mount ids, the same mount.
    pub fn same_mount_as(&self, other: &Meta) -> bool {
        self.dev == other.dev
            && match (self.mnt_id, other.mnt_id) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }

    fn from_statx(sx: &Statx) -> Meta {
        let kind = match FileType::from_raw_mode(u32::from(sx.stx_mode)) {
            FileType::RegularFile => Kind::File,
            FileType::Directory => Kind::Dir,
            FileType::Symlink => Kind::Symlink,
            _ => Kind::Other,
        };
        Meta {
            dev: makedev(sx.stx_dev_major, sx.stx_dev_minor),
            ino: sx.stx_ino,
            kind,
            disk_bytes: sx.stx_blocks * 512,
            nlink: sx.stx_nlink,
            mnt_id: (sx.stx_mask & StatxFlags::MNT_ID.bits() != 0).then_some(sx.stx_mnt_id),
        }
    }
}

const STAT_MASK: StatxFlags = StatxFlags::BASIC_STATS.union(StatxFlags::MNT_ID);

const DIR_FLAGS: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);

/// `statx(dir, name, AT_SYMLINK_NOFOLLOW)`.
pub fn stat_at(dir: BorrowedFd<'_>, name: &[u8]) -> Result<Meta, Errno> {
    statx(dir, name, AtFlags::SYMLINK_NOFOLLOW, STAT_MASK).map(|sx| Meta::from_statx(&sx))
}

/// Stats the open directory itself.
pub fn stat_fd(fd: BorrowedFd<'_>) -> Result<Meta, Errno> {
    statx(fd, "", AtFlags::EMPTY_PATH, STAT_MASK).map(|sx| Meta::from_statx(&sx))
}

/// Opens `rel` as a directory beneath `root`, refusing symlinks, magic links and escapes.
pub fn open_dir_beneath(root: BorrowedFd<'_>, rel: &[u8]) -> Result<OwnedFd, Errno> {
    let resolve = ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS;
    openat2(root, rel, DIR_FLAGS, Mode::empty(), resolve)
}

/// Opens the directory `name` inside `dir` without following a symlink.
pub fn open_dir_at(dir: BorrowedFd<'_>, name: &[u8]) -> Result<OwnedFd, Errno> {
    openat(dir, name, DIR_FLAGS, Mode::empty())
}

/// Entry names of an open directory, without `.` and `..`.
pub fn list_names(dir: BorrowedFd<'_>) -> Result<Vec<Box<[u8]>>, Errno> {
    let mut names = Vec::new();
    for entry in Dir::read_from(dir.as_fd())? {
        let entry = entry?;
        let name = entry.file_name().to_bytes();
        if name != b"." && name != b".." {
            names.push(name.into());
        }
    }
    Ok(names)
}

pub fn unlink_at(dir: BorrowedFd<'_>, name: &[u8]) -> Result<(), Errno> {
    unlinkat(dir, name, AtFlags::empty())
}

pub fn remove_dir_at(dir: BorrowedFd<'_>, name: &[u8]) -> Result<(), Errno> {
    unlinkat(dir, name, AtFlags::REMOVEDIR)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::fd::AsFd;
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn opening_a_symlink_to_a_directory_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("real")).unwrap();
        symlink("real", tmp.path().join("link")).unwrap();
        let dir = fs::File::open(tmp.path()).unwrap();

        assert!(open_dir_at(dir.as_fd(), b"real").is_ok());
        assert_eq!(
            open_dir_at(dir.as_fd(), b"link").unwrap_err(),
            Errno::NOTDIR
        );
        let rel = open_dir_beneath(dir.as_fd(), b"link");
        assert!(rel.is_err(), "openat2 with NO_SYMLINKS must refuse it too");
    }
}
