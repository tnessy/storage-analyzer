//! Parallel directory walker (T004, T006).

pub mod control;
pub mod fs;
pub mod mounts;
pub mod msg;
pub mod queue;
mod walker;

pub use control::ScanControl;
pub use fs::{Errno, FsAccess, RealDir, RealFs, StatInfo};
pub use msg::{RawEntry, ScanMsg};
pub use walker::{ScanHandle, start};

const MAX_DEFAULT_THREADS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanOptions {
    pub threads: usize,
    /// Never leave the root's filesystem (`-x`).
    pub one_filesystem: bool,
    /// Do not enter pseudo (proc, sys, dev…) or network filesystems.
    pub skip_pseudo: bool,
}

impl ScanOptions {
    /// `min(available_parallelism, 8)`; a tunable target, not a gate.
    pub fn default_threads() -> usize {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(MAX_DEFAULT_THREADS)
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
