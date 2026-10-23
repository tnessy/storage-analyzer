//! Fd-relative, identity-checked removal of one target (design §3.8, ADR-007).
//!
//! The walk is iterative and post-order. Each open directory is a `Frame`; the
//! deepest `limit` frames keep their fd and the rest are reopened on demand from
//! the root of the walk, with the same no-follow and identity checks.

use std::ffi::OsStr;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::msg::{AbortReason, DeleteFailKind, DeleteFailure, DeleteRequest, Outcome};
use super::sys::{self, Errno, Meta};
use crate::types::{Counts, Kind};

/// Most directory fds held at once (not counting the target's parent).
pub const MAX_OPEN_DIRFDS: usize = 64;
/// Frames reopened per rebuild, so climbing out of a deep tree is amortised.
const MAX_REOPEN_BLOCK: usize = 32;

/// Called with the cumulative counts after every removed entry.
pub type OnRemoved<'a> = &'a mut dyn FnMut(Counts);

pub fn run(
    root: BorrowedFd<'_>,
    req: &DeleteRequest,
    stop: &AtomicBool,
    on_removed: OnRemoved<'_>,
) -> Outcome {
    run_with_limit(root, req, stop, MAX_OPEN_DIRFDS, on_removed)
}

pub fn run_with_limit(
    root: BorrowedFd<'_>,
    req: &DeleteRequest,
    stop: &AtomicBool,
    limit: usize,
    on_removed: OnRemoved<'_>,
) -> Outcome {
    let aborted = |reason| Outcome::Aborted { reason };
    let Some((parent_rel, name)) = split_target(&req.rel_path) else {
        return aborted(AbortReason::OpenFailed(Errno::INVAL.raw_os_error()));
    };

    let parent_fd = match sys::open_dir_beneath(root, &parent_rel) {
        Ok(fd) => fd,
        Err(e) => return aborted(open_failure(e)),
    };
    let meta = match sys::stat_at(parent_fd.as_fd(), &name) {
        Ok(m) => m,
        Err(e) => return aborted(open_failure(e)),
    };
    if (meta.dev, meta.ino, meta.kind) != (req.expect_dev, req.expect_ino, req.kind) {
        return aborted(AbortReason::ChangedSinceScan);
    }

    if meta.kind != Kind::Dir {
        return remove_leaf(parent_fd.as_fd(), &name, &meta, &req.rel_path, on_removed);
    }
    // The target may itself be a mount point (cross-filesystem scan, or a bind mount).
    match sys::stat_fd(parent_fd.as_fd()) {
        Ok(parent) if meta.same_mount_as(&parent) => {}
        Ok(_) => return aborted(AbortReason::MountPoint),
        Err(e) => return aborted(open_failure(e)),
    }
    let mut walk = Walk::new(
        parent_fd,
        req.rel_path.clone(),
        name,
        meta,
        stop,
        limit,
        on_removed,
    );
    match walk.start() {
        Ok(()) => walk.finish_run(),
        Err(e) => aborted(open_failure(e)),
    }
}

fn open_failure(e: Errno) -> AbortReason {
    if e == Errno::NOENT {
        AbortReason::NotFound
    } else {
        AbortReason::OpenFailed(e.raw_os_error())
    }
}

/// Splits `rel` into the parent path (`.` for a top-level target) and the target's name.
/// Only plain components are accepted, so the path cannot name the root or climb out.
fn split_target(rel: &Path) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            Component::Normal(n) => parts.push(n.as_bytes()),
            _ => return None,
        }
    }
    let (name, parents) = parts.split_last()?;
    let parent = if parents.is_empty() {
        b".".to_vec()
    } else {
        parents.join(&b'/')
    };
    Some((parent, name.to_vec()))
}

fn classify(e: Errno) -> DeleteFailKind {
    match e {
        Errno::ACCESS | Errno::PERM => DeleteFailKind::PermissionDenied,
        Errno::BUSY | Errno::TXTBSY => DeleteFailKind::Busy,
        Errno::ROFS => DeleteFailKind::ReadOnlyFs,
        other => DeleteFailKind::Other(other.raw_os_error()),
    }
}

fn leaf_counts(meta: &Meta) -> Counts {
    // A surviving hard link frees nothing, so only single-link files add bytes.
    let bytes = if meta.nlink == 1 { meta.disk_bytes } else { 0 };
    Counts {
        files: 1,
        dirs: 0,
        bytes,
    }
}

fn remove_leaf(
    parent: BorrowedFd<'_>,
    name: &[u8],
    meta: &Meta,
    rel_path: &Path,
    on_removed: OnRemoved<'_>,
) -> Outcome {
    match sys::unlink_at(parent, name) {
        Ok(()) => {
            let removed = leaf_counts(meta);
            on_removed(removed);
            Outcome::Removed(removed)
        }
        Err(Errno::NOENT) => Outcome::Removed(Counts::default()),
        Err(e) => Outcome::Partial {
            removed: Counts::default(),
            failures: vec![DeleteFailure {
                rel_path: rel_path.to_path_buf(),
                kind: classify(e),
            }],
        },
    }
}

struct Frame {
    name: Box<[u8]>,
    /// `None` once closed to stay under the fd limit.
    fd: Option<OwnedFd>,
    dev: u64,
    ino: u64,
    disk_bytes: u64,
    /// Entries not yet handled; popped from the end.
    pending: Vec<Box<[u8]>>,
    /// Something inside could not be removed, so this directory will not be empty.
    blocked: bool,
}

struct Walk<'a> {
    parent_fd: OwnedFd,
    target_rel: PathBuf,
    target_name: Vec<u8>,
    target: Meta,
    /// Frames `lowest_open..` hold an fd; all shallower ones are closed.
    stack: Vec<Frame>,
    lowest_open: usize,
    limit: usize,
    stop: &'a AtomicBool,
    removed: Counts,
    failures: Vec<DeleteFailure>,
    on_removed: OnRemoved<'a>,
}

impl<'a> Walk<'a> {
    fn new(
        parent_fd: OwnedFd,
        target_rel: PathBuf,
        target_name: Vec<u8>,
        target: Meta,
        stop: &'a AtomicBool,
        limit: usize,
        on_removed: OnRemoved<'a>,
    ) -> Self {
        Walk {
            parent_fd,
            target_rel,
            target_name,
            target,
            stack: Vec::new(),
            lowest_open: 0,
            limit: limit.max(1),
            stop,
            removed: Counts::default(),
            failures: Vec::new(),
            on_removed,
        }
    }

    /// Opens the target directory; a failure here aborts before anything is removed.
    fn start(&mut self) -> Result<(), Errno> {
        let name = self.target_name.clone();
        let target = self.target;
        self.push_dir(None, &name, &target)
    }

    fn finish_run(mut self) -> Outcome {
        let completed = self.walk();
        if completed && self.failures.is_empty() {
            Outcome::Removed(self.removed)
        } else {
            Outcome::Partial {
                removed: self.removed,
                failures: self.failures,
            }
        }
    }

    /// Returns true if the whole target was processed (not stopped, no fatal error).
    fn walk(&mut self) -> bool {
        while !self.stack.is_empty() {
            if self.stop.load(Ordering::Relaxed) {
                return false;
            }
            if let Err(e) = self.ensure_top_open() {
                let path = self.path_to(self.stack.len() - 1);
                self.fail(path, e);
                return false;
            }
            let step = match self.stack.last_mut().and_then(|f| f.pending.pop()) {
                Some(name) => self.entry(&name),
                None => self.finish_dir(),
            };
            if let Err(e) = step {
                let path = self.path_to(self.stack.len().saturating_sub(1));
                self.fail(path, e);
                return false;
            }
        }
        true
    }

    /// The top frame's fd. `BADF` means the frame bookkeeping is broken; the walk stops.
    fn top_fd(&self) -> Result<BorrowedFd<'_>, Errno> {
        self.frame_fd(self.stack.len().checked_sub(1))
    }

    fn frame_fd(&self, index: Option<usize>) -> Result<BorrowedFd<'_>, Errno> {
        match index {
            None => Ok(self.parent_fd.as_fd()),
            Some(i) => self
                .stack
                .get(i)
                .and_then(|f| f.fd.as_ref())
                .map(AsFd::as_fd)
                .ok_or(Errno::BADF),
        }
    }

    /// `rel_path` of the frame at `index` (0 is the target).
    fn path_to(&self, index: usize) -> PathBuf {
        let mut path = self.target_rel.clone();
        for frame in self.stack.iter().take(index + 1).skip(1) {
            path.push(OsStr::from_bytes(&frame.name));
        }
        path
    }

    fn fail(&mut self, rel_path: PathBuf, e: Errno) {
        self.failures.push(DeleteFailure {
            rel_path,
            kind: classify(e),
        });
    }

    fn block_top(&mut self) {
        if let Some(top) = self.stack.last_mut() {
            top.blocked = true;
        }
    }

    fn same_filesystem(&self, m: &Meta) -> bool {
        m.same_mount_as(&self.target)
    }

    /// Handles one entry of the top directory. `Err` is fatal for the walk.
    fn entry(&mut self, name: &[u8]) -> Result<(), Errno> {
        let meta = match sys::stat_at(self.top_fd()?, name) {
            Ok(m) => m,
            Err(Errno::NOENT) => return Ok(()),
            Err(e) => {
                self.entry_failed(name, e);
                return Ok(());
            }
        };
        if meta.kind == Kind::Dir {
            if !self.same_filesystem(&meta) {
                let path = self.child_path(name);
                self.failures.push(DeleteFailure {
                    rel_path: path,
                    kind: DeleteFailKind::OtherFilesystem,
                });
                self.block_top();
                return Ok(());
            }
            let parent = self.stack.len() - 1;
            if let Err(e) = self.push_dir(Some(parent), name, &meta)
                && e != Errno::NOENT
            {
                self.entry_failed(name, e);
            }
            return Ok(());
        }
        // Residual race: Linux cannot unlink by fd, so an object swapped in under `name`
        // between the `statx` above and this call would be removed instead. It is confined
        // to one name inside a directory we verified, and needs write access to it.
        match sys::unlink_at(self.top_fd()?, name) {
            Ok(()) => self.count(leaf_counts(&meta)),
            Err(Errno::NOENT) => {}
            Err(e) => self.entry_failed(name, e),
        }
        Ok(())
    }

    fn entry_failed(&mut self, name: &[u8], e: Errno) {
        let path = self.child_path(name);
        self.fail(path, e);
        self.block_top();
    }

    fn child_path(&self, name: &[u8]) -> PathBuf {
        let mut path = self.path_to(self.stack.len() - 1);
        path.push(OsStr::from_bytes(name));
        path
    }

    fn count(&mut self, delta: Counts) {
        self.removed.files += delta.files;
        self.removed.dirs += delta.dirs;
        self.removed.bytes += delta.bytes;
        (self.on_removed)(self.removed);
    }

    /// Opens directory `name` in the frame at `parent` (or beside the target for `None`),
    /// verifies it is the directory that was just stat'ed, and pushes a frame for it.
    fn push_dir(&mut self, parent: Option<usize>, name: &[u8], expect: &Meta) -> Result<(), Errno> {
        let fd = sys::open_dir_at(self.frame_fd(parent)?, name)?;
        let opened = sys::stat_fd(fd.as_fd())?;
        if (opened.dev, opened.ino, opened.kind) != (expect.dev, expect.ino, Kind::Dir) {
            return Err(Errno::STALE);
        }
        let pending = sys::list_names(fd.as_fd())?;
        while self.stack.len() - self.lowest_open >= self.limit {
            self.stack[self.lowest_open].fd = None;
            self.lowest_open += 1;
        }
        self.stack.push(Frame {
            name: name.into(),
            fd: Some(fd),
            dev: opened.dev,
            ino: opened.ino,
            disk_bytes: opened.disk_bytes,
            pending,
            blocked: false,
        });
        Ok(())
    }

    /// Removes the finished top directory from its parent.
    fn finish_dir(&mut self) -> Result<(), Errno> {
        let path = self.path_to(self.stack.len() - 1);
        let Some(frame) = self.stack.pop() else {
            return Ok(());
        };
        drop(frame.fd);
        self.lowest_open = self.lowest_open.min(self.stack.len());
        self.ensure_top_open()?;
        let parent = self.frame_fd(self.stack.len().checked_sub(1))?;
        match sys::remove_dir_at(parent, &frame.name) {
            Ok(()) => self.count(Counts {
                files: 0,
                dirs: 1,
                bytes: frame.disk_bytes,
            }),
            Err(Errno::NOENT) => {}
            Err(e) => {
                let leftover = frame.blocked && matches!(e, Errno::NOTEMPTY | Errno::EXIST);
                if !leftover {
                    self.fail(path, e);
                }
                self.block_top();
            }
        }
        Ok(())
    }

    /// Makes sure the top frame has an fd, reopening a block of frames from the walk's root.
    fn ensure_top_open(&mut self) -> Result<(), Errno> {
        let Some(top) = self.stack.len().checked_sub(1) else {
            return Ok(());
        };
        if top >= self.lowest_open {
            return Ok(());
        }
        let keep = (self.limit / 2).clamp(1, MAX_REOPEN_BLOCK);
        let keep_from = (top + 1).saturating_sub(keep);
        let mut previous: Option<OwnedFd> = None;
        for i in 0..=top {
            let base = match (i, &previous) {
                (0, _) => self.parent_fd.as_fd(),
                (_, Some(fd)) => fd.as_fd(),
                _ => self.frame_fd(Some(i - 1))?,
            };
            let fd = sys::open_dir_at(base, &self.stack[i].name)?;
            let opened = sys::stat_fd(fd.as_fd())?;
            if (opened.dev, opened.ino) != (self.stack[i].dev, self.stack[i].ino) {
                return Err(Errno::STALE);
            }
            if i >= keep_from {
                self.stack[i].fd = Some(fd);
                previous = None;
            } else {
                previous = Some(fd);
            }
        }
        self.lowest_open = keep_from;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    use super::*;

    fn request(path: &Path, rel: &str) -> DeleteRequest {
        let meta = fs::symlink_metadata(path.join(rel)).unwrap();
        DeleteRequest {
            node: crate::types::NodeId(1),
            rel_path: rel.into(),
            expect_dev: meta.dev(),
            expect_ino: meta.ino(),
            kind: Kind::Dir,
            total_files: None,
            total_dirs: None,
        }
    }

    /// `depth` nested dirs, each holding a file and a side dir with a file.
    /// Returns (files, dirs) including the top directory.
    fn build(root: &Path, depth: usize) -> (u64, u64) {
        let mut dir = root.join("top");
        fs::create_dir(&dir).unwrap();
        let (mut files, mut dirs) = (0, 1);
        for _ in 0..depth {
            fs::write(dir.join("f"), b"x").unwrap();
            fs::create_dir(dir.join("side")).unwrap();
            fs::write(dir.join("side/g"), b"x").unwrap();
            dir = dir.join("next");
            fs::create_dir(&dir).unwrap();
            files += 2;
            dirs += 2;
        }
        (files, dirs)
    }

    fn run_in(root: &Path, limit: usize, stop: &AtomicBool) -> (Outcome, u64) {
        let root_fd = OwnedFd::from(fs::File::open(root).unwrap());
        let mut calls = 0;
        let outcome = run_with_limit(
            root_fd.as_fd(),
            &request(root, "top"),
            stop,
            limit,
            &mut |_| calls += 1,
        );
        (outcome, calls)
    }

    #[test]
    fn tiny_fd_limit_still_removes_everything_with_exact_counts() {
        for limit in [1, 2, 3, 5] {
            let tmp = tempfile::tempdir().unwrap();
            let (files, dirs) = build(tmp.path(), 40);
            let (outcome, calls) = run_in(tmp.path(), limit, &AtomicBool::new(false));
            let Outcome::Removed(c) = outcome else {
                panic!("limit {limit}: {outcome:?}")
            };
            assert_eq!((c.files, c.dirs), (files, dirs), "limit {limit}");
            assert_eq!(calls, files + dirs);
            assert!(!tmp.path().join("top").exists());
        }
    }

    #[test]
    fn stop_between_entries_gives_partial_without_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let (files, _) = build(tmp.path(), 10);
        let stop = AtomicBool::new(false);
        let root_fd = OwnedFd::from(fs::File::open(tmp.path()).unwrap());
        let mut seen = 0;
        let outcome = run_with_limit(
            root_fd.as_fd(),
            &request(tmp.path(), "top"),
            &stop,
            3,
            &mut |_| {
                seen += 1;
                if seen == 5 {
                    stop.store(true, Ordering::Relaxed);
                }
            },
        );
        let Outcome::Partial { removed, failures } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(
            removed.files + removed.dirs,
            5,
            "stops right after the current entry"
        );
        assert!(removed.files < files);
        assert!(failures.is_empty());
        assert!(tmp.path().join("top").exists());
    }

    #[test]
    fn directory_replaced_while_the_walk_is_below_it_is_detected_on_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let chain = tmp.path().join("top/next/next/next/next/next");
        fs::create_dir_all(&chain).unwrap();
        fs::write(chain.join("leaf"), b"x").unwrap();
        let victim = tmp.path().join("top/next/next");
        let moved = tmp.path().join("moved");
        let stop = AtomicBool::new(false);
        let root_fd = OwnedFd::from(fs::File::open(tmp.path()).unwrap());
        let mut swapped = false;
        // The first removal is the leaf, after every directory was opened and listed.
        let outcome = run_with_limit(
            root_fd.as_fd(),
            &request(tmp.path(), "top"),
            &stop,
            1,
            &mut |_| {
                if !swapped {
                    swapped = true;
                    fs::rename(&victim, &moved).unwrap();
                    // Same names below, different inodes: only the identity check can tell.
                    fs::create_dir_all(victim.join("next/next/next")).unwrap();
                }
            },
        );
        let Outcome::Partial { removed, failures } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!((removed.files, removed.dirs), (1, 0));
        assert_eq!(failures.len(), 1);
        assert!(
            moved.join("next/next/next").is_dir(),
            "the swapped-out tree is left alone"
        );
        assert!(
            victim.join("next/next/next").is_dir(),
            "the look-alike is not removed"
        );
    }
}
