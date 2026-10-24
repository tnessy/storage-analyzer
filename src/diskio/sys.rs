//! The part of `/proc`, `/sys` and `/dev` the disk-rate source looks at, behind a trait so
//! tests can use a fake tree. Read-only.

use std::ffi::OsString;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

pub trait SysView: Send + Sync {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn read_link(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<OsString>>;
    /// `st_rdev` of `path` if it is a block device (symlinks followed), `None` if it exists
    /// but is not one.
    fn stat_rdev(&self, path: &Path) -> io::Result<Option<u64>>;
}

/// The real thing: absolute paths are looked up below `root` (`/` in production).
pub struct FsSys {
    root: PathBuf,
}

impl FsSys {
    pub fn new(root: impl Into<PathBuf>) -> FsSys {
        FsSys { root: root.into() }
    }

    pub fn real() -> FsSys {
        FsSys::new("/")
    }

    fn at(&self, path: &Path) -> PathBuf {
        self.root.join(path.strip_prefix("/").unwrap_or(path))
    }
}

impl SysView for FsSys {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(self.at(path))
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::read_link(self.at(path))
    }

    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<OsString>> {
        std::fs::read_dir(self.at(path))?
            .map(|e| e.map(|e| e.file_name()))
            .collect()
    }

    fn stat_rdev(&self, path: &Path) -> io::Result<Option<u64>> {
        let meta = std::fs::metadata(self.at(path))?;
        Ok(meta.file_type().is_block_device().then(|| meta.rdev()))
    }
}
