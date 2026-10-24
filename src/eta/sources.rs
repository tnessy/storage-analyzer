//! Where the expected total of entries comes from (design §3.6, ADR-010).

use std::path::Path;

use super::EtaSource;

/// Used inodes of the filesystem holding a scan root, when that tells the scan's size.
pub trait InodeSource: Send + Sync {
    fn used_inodes(&self, root: &Path) -> Option<u64>;
}

/// The root must be the mount root and the filesystem must count inodes (btrfs reports 0).
pub fn inode_total(f_files: u64, f_ffree: u64, is_mount_root: bool) -> Option<u64> {
    (is_mount_root && f_files > 0).then(|| f_files.saturating_sub(f_ffree))
}

/// The real thing: read-only `statvfs` and `stat`.
pub struct StatvfsInodes;

impl InodeSource for StatvfsInodes {
    fn used_inodes(&self, root: &Path) -> Option<u64> {
        let dev = rustix::fs::stat(root).ok()?.st_dev;
        let is_mount_root = match root.parent() {
            None => true,
            Some(parent) => rustix::fs::stat(parent).ok()?.st_dev != dev,
        };
        let vfs = rustix::fs::statvfs(root).ok()?;
        inode_total(vfs.f_files, vfs.f_ffree, is_mount_root)
    }
}

/// The totals each source offers; the best available one is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EtaSources {
    pub history: Option<u64>,
    pub session: Option<u64>,
    pub inodes: Option<u64>,
}

impl EtaSources {
    /// History, then this session's previous scan, then the filesystem's inode count.
    pub fn best(&self) -> Option<(u64, EtaSource)> {
        self.history
            .map(|t| (t, EtaSource::History))
            .or(self.session.map(|t| (t, EtaSource::SessionPrevious)))
            .or(self.inodes.map(|t| (t, EtaSource::FsInodes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_source_follows_the_priority_order() {
        let all = EtaSources {
            history: Some(1),
            session: Some(2),
            inodes: Some(3),
        };
        assert_eq!(all.best(), Some((1, EtaSource::History)));
        let no_history = EtaSources {
            history: None,
            ..all
        };
        assert_eq!(no_history.best(), Some((2, EtaSource::SessionPrevious)));
        let inodes_only = EtaSources {
            history: None,
            session: None,
            inodes: Some(3),
        };
        assert_eq!(inodes_only.best(), Some((3, EtaSource::FsInodes)));
        assert_eq!(EtaSources::default().best(), None);
    }

    #[test]
    fn inode_counts_need_the_mount_root_and_a_filesystem_that_counts() {
        assert_eq!(inode_total(1_000, 400, true), Some(600));
        assert_eq!(
            inode_total(0, 0, true),
            None,
            "btrfs reports no inode counts"
        );
        assert_eq!(
            inode_total(1_000, 400, false),
            None,
            "a subdirectory is not the whole filesystem"
        );
        assert_eq!(
            inode_total(100, 150, true),
            Some(0),
            "free can not exceed total, but never underflow"
        );
    }
}
