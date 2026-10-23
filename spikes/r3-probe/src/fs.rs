use crate::tree::{Child, Tree};
use rustix::fs::{fstat, openat, statat, AtFlags, Dir, FileType, Mode, OFlags, CWD};
use rustix::io::Errno;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

/// Read-only view of a directory tree: no symlink following, one filesystem only.
pub struct FsTree {
    root: PathBuf,
    dev: u64,
}

impl FsTree {
    pub fn new(root: PathBuf) -> Result<Self, Errno> {
        let fd = open_dir(&root)?;
        Ok(Self {
            dev: fstat(&fd)?.st_dev,
            root,
        })
    }
}

fn open_dir(path: &std::path::Path) -> Result<OwnedFd, Errno> {
    let base = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    match openat(CWD, path, base | OFlags::NOATIME, Mode::empty()) {
        Err(Errno::PERM) => openat(CWD, path, base, Mode::empty()),
        r => r,
    }
}

impl Tree for FsTree {
    type Node = PathBuf;

    fn root(&self) -> PathBuf {
        self.root.clone()
    }

    fn children(&self, path: &PathBuf) -> Vec<Child<PathBuf>> {
        let Ok(fd) = open_dir(path) else {
            return vec![];
        };
        let Ok(dir) = Dir::read_from(&fd) else {
            return vec![];
        };
        let mut out = Vec::new();
        for entry in dir.flatten() {
            let name = entry.file_name().to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            let is_dir = match entry.file_type() {
                FileType::Unknown => statat(&fd, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW)
                    .map(|s| FileType::from_raw_mode(s.st_mode) == FileType::Directory)
                    .unwrap_or(false),
                t => t == FileType::Directory,
            };
            let node = path.join(std::ffi::OsStr::from_bytes(name));
            let descend = is_dir
                && statat(&fd, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW)
                    .map(|s| s.st_dev == self.dev)
                    .unwrap_or(false);
            out.push(Child { node, descend });
        }
        out
    }
}
