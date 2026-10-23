//! Delete executor; with `history`, the only module allowed to write (T010).

pub mod msg;

use std::os::fd::OwnedFd;
use std::sync::mpsc::SyncSender;
use std::time::Duration;

pub use msg::{AbortReason, DeleteEvent, DeleteFailKind, DeleteFailure, DeleteRequest, Outcome};

/// Returned by `submit` while another delete is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

/// Owns the executor thread, its request channel and the stop flag (T010).
pub struct DeleteHandle {
    _private: (),
}

/// `root_fd` is the scan root opened with `O_DIRECTORY|O_NOFOLLOW`.
pub fn spawn(_root_fd: OwnedFd, _tx: SyncSender<DeleteEvent>) -> DeleteHandle {
    todo!("T010")
}

impl DeleteHandle {
    pub fn submit(&self, _req: DeleteRequest) -> Result<(), Busy> {
        todo!("T010")
    }

    pub fn stop(&self) {
        todo!("T010")
    }

    /// Waits up to `max` for the running delete to end; `true` if idle.
    pub fn wait_idle(&self, _max: Duration) -> bool {
        todo!("T010")
    }
}
