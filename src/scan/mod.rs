//! Parallel directory walker (T004, T006).

pub mod control;
pub mod fs;
pub mod msg;
pub mod queue;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

pub use control::ScanControl;
pub use fs::{Errno, FsAccess, RealFs, StatInfo};
pub use msg::{RawEntry, ScanMsg};

use crate::types::NodeId;

const MAX_DEFAULT_THREADS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanOptions {
    pub threads: usize,
    pub one_filesystem: bool,
}

impl ScanOptions {
    /// `min(available_parallelism, 8)`; a tunable target, not a gate.
    pub fn default_threads() -> usize {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(MAX_DEFAULT_THREADS)
    }
}

/// Owns the worker threads and the job queue (T006).
pub struct ScanHandle {
    _private: (),
}

/// Starts the workers. `first_id` is the first `NodeId` the scan may allocate.
pub fn start<F: FsAccess + 'static>(
    _fs: Arc<F>,
    _root: PathBuf,
    _root_dev: u64,
    _opts: ScanOptions,
    _control: Arc<ScanControl>,
    _tx: SyncSender<ScanMsg>,
    _first_id: u32,
) -> ScanHandle {
    todo!("T006")
}

impl ScanHandle {
    /// Works at any time, also after `Done` / `Stopped`.
    pub fn rescan(&self, _dir: NodeId, _path: PathBuf, _depth: u32) {
        todo!("T006")
    }

    /// Joins the workers.
    pub fn shutdown(self) {
        todo!("T006")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_threads_is_between_one_and_eight() {
        assert!((1..=8).contains(&ScanOptions::default_threads()));
    }
}
