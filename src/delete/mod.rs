//! Delete executor; with `history`, the only module allowed to write (T010).

mod executor;
pub mod msg;
mod sys;

use std::cell::Cell;
use std::os::fd::{AsFd, OwnedFd};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use msg::{AbortReason, DeleteEvent, DeleteFailKind, DeleteFailure, DeleteRequest, Outcome};

use crate::types::Counts;

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// How long `Drop` waits for the executor, and how often a blocked `Finished` send retries.
const EXIT_WAIT: Duration = Duration::from_secs(2);
const SEND_RETRY: Duration = Duration::from_millis(5);

/// Returned by `submit` while another delete is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Busy;

#[derive(Default)]
struct State {
    /// A request is queued or running.
    busy: bool,
    /// The executor thread has ended.
    exited: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        f(&mut self.lock());
        self.changed.notify_all();
    }

    /// Waits until `done(state)` holds or `max` passes; returns whether it holds.
    fn wait(&self, max: Duration, done: impl Fn(&State) -> bool) -> bool {
        let state = self.lock();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, max, |s| !done(s))
            .unwrap_or_else(PoisonError::into_inner);
        done(&state)
    }
}

/// Marks the executor as exited when its thread ends, also on a panic.
struct ExitGuard(Arc<Shared>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.update(|s| s.exited = true);
    }
}

/// Owns the executor thread, its request channel and the stop flag.
pub struct DeleteHandle {
    requests: Option<Sender<DeleteRequest>>,
    stop: Arc<AtomicBool>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

/// `root_fd` is the scan root opened with `O_DIRECTORY|O_NOFOLLOW`.
pub fn spawn(root_fd: OwnedFd, tx: SyncSender<DeleteEvent>) -> DeleteHandle {
    spawn_with(tx, move |req, stop, on_removed| {
        executor::run(root_fd.as_fd(), req, stop, on_removed)
    })
}

/// The executor thread around any `run` function; tests inject failures or delays through it.
#[doc(hidden)]
pub fn spawn_with<R>(tx: SyncSender<DeleteEvent>, run: R) -> DeleteHandle
where
    R: Fn(&DeleteRequest, &AtomicBool, &mut dyn FnMut(Counts)) -> Outcome + Send + 'static,
{
    let (requests, inbox) = mpsc::channel::<DeleteRequest>();
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared::default());
    let worker = {
        let (stop, shared) = (Arc::clone(&stop), Arc::clone(&shared));
        move || {
            let _exit = ExitGuard(Arc::clone(&shared));
            for req in inbox {
                let mut progress = progress_sender(&tx);
                let outcome = guarded(&req, &mut progress, |on_removed| {
                    run(&req, &stop, on_removed)
                });
                // Cleared before `Finished` is sent so the receiver can submit again at once.
                shared.update(|s| s.busy = false);
                if !send_finished(&tx, outcome, &stop) {
                    break;
                }
            }
        }
    };
    // If the thread cannot start, the request channel is dropped and `submit` reports `Busy`.
    let thread = std::thread::Builder::new()
        .name("delete".into())
        .spawn(worker)
        .ok();
    if thread.is_none() {
        shared.update(|s| s.exited = true);
    }
    DeleteHandle {
        requests: thread.is_some().then_some(requests),
        stop,
        shared,
        thread,
    }
}

/// At most one `Progress` per `PROGRESS_INTERVAL`; a full channel just skips an update.
fn progress_sender(tx: &SyncSender<DeleteEvent>) -> impl FnMut(Counts) + '_ {
    let mut last_sent = Instant::now();
    move |c: Counts| {
        if last_sent.elapsed() < PROGRESS_INTERVAL {
            return;
        }
        let progress = DeleteEvent::Progress {
            removed_files: c.files,
            removed_dirs: c.dirs,
            removed_bytes: c.bytes,
        };
        if !matches!(tx.try_send(progress), Err(TrySendError::Full(_))) {
            last_sent = Instant::now();
        }
    }
}

/// Runs the executor and turns a panic into a `Partial` outcome, so the receiver always
/// gets a `Finished` and the busy flag is cleared. The failure entry points at the target.
fn guarded(
    req: &DeleteRequest,
    progress: &mut dyn FnMut(Counts),
    run: impl FnOnce(&mut dyn FnMut(Counts)) -> Outcome,
) -> Outcome {
    let last = Cell::new(Counts::default());
    let mut on_removed = |c: Counts| {
        last.set(c);
        progress(c);
    };
    catch_unwind(AssertUnwindSafe(|| run(&mut on_removed))).unwrap_or_else(|_| Outcome::Partial {
        removed: last.get(),
        failures: vec![DeleteFailure {
            rel_path: req.rel_path.clone(),
            kind: DeleteFailKind::Other(rustix::io::Errno::IO.raw_os_error()),
        }],
    })
}

/// Sends `Finished` without ever blocking for good: once `stop` is set and the channel is
/// still full, the event is dropped. Returns false if the executor should exit.
fn send_finished(tx: &SyncSender<DeleteEvent>, outcome: Outcome, stop: &AtomicBool) -> bool {
    let mut event = DeleteEvent::Finished(outcome);
    loop {
        match tx.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(back)) => {
                if stop.load(Ordering::Relaxed) {
                    return false;
                }
                event = back;
                std::thread::sleep(SEND_RETRY);
            }
        }
    }
}

impl DeleteHandle {
    /// Starts a delete, or returns `Busy` if one is queued or running.
    pub fn submit(&self, req: DeleteRequest) -> Result<(), Busy> {
        let requests = self.requests.as_ref().ok_or(Busy)?;
        let mut state = self.shared.lock();
        if state.busy {
            return Err(Busy);
        }
        self.stop.store(false, Ordering::Relaxed);
        requests.send(req).map_err(|_| Busy)?;
        state.busy = true;
        Ok(())
    }

    /// Gives up on the executor thread without waiting for it (quitting, after the caller has
    /// already waited): a thread stuck in a syscall ends with the process.
    pub fn detach(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.requests = None;
        self.thread = None;
    }

    /// Asks the running delete to stop after its current entry.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Waits up to `max` for the executor to finish its current delete; `true` if idle.
    /// `Finished` may still be in flight to the receiver.
    pub fn wait_idle(&self, max: Duration) -> bool {
        self.shared.wait(max, |s| !s.busy)
    }
}

impl Drop for DeleteHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.requests = None;
        // Never wait for good: a thread stuck in a syscall is detached and ends with the process.
        if self.thread.is_some()
            && self.shared.wait(EXIT_WAIT, |s| s.exited)
            && let Some(thread) = self.thread.take()
        {
            // A panicked executor has nothing left to report.
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Kind, NodeId};

    fn request() -> DeleteRequest {
        DeleteRequest {
            node: NodeId(1),
            rel_path: "a/b".into(),
            expect_dev: 0,
            expect_ino: 0,
            kind: Kind::Dir,
            total_files: None,
            total_dirs: None,
        }
    }

    #[test]
    fn a_panic_becomes_a_partial_outcome_with_the_last_counts() {
        let counts = Counts {
            files: 3,
            dirs: 1,
            bytes: 9,
        };
        let mut seen = Vec::new();
        let outcome = guarded(&request(), &mut |c| seen.push(c), |on_removed| {
            on_removed(counts);
            panic!("executor bug");
        });
        assert_eq!(seen, vec![counts]);
        let Outcome::Partial { removed, failures } = outcome else {
            panic!("{outcome:?}")
        };
        assert_eq!(removed, counts);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].rel_path, std::path::PathBuf::from("a/b"));
        assert_eq!(failures[0].kind, DeleteFailKind::Other(5));
    }

    #[test]
    fn a_panicking_delete_still_finishes_and_the_next_one_works() {
        use std::sync::mpsc::sync_channel;
        let (tx, rx) = sync_channel(8);
        let handle = spawn_with(tx, |req, _stop, on_removed| {
            if req.node == NodeId(1) {
                panic!("executor bug");
            }
            let removed = Counts {
                files: 1,
                dirs: 0,
                bytes: 0,
            };
            on_removed(removed);
            Outcome::Removed(removed)
        });
        let finished = || match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            DeleteEvent::Finished(o) => o,
            other => panic!("unexpected {other:?}"),
        };

        handle.submit(request()).unwrap();
        assert!(matches!(finished(), Outcome::Partial { .. }));
        assert!(handle.wait_idle(Duration::from_secs(5)));

        let second = DeleteRequest {
            node: NodeId(2),
            ..request()
        };
        handle.submit(second).unwrap();
        assert_eq!(
            finished(),
            Outcome::Removed(Counts {
                files: 1,
                dirs: 0,
                bytes: 0
            })
        );
    }

    #[test]
    fn a_normal_outcome_passes_through() {
        let done = Outcome::Removed(Counts::default());
        assert_eq!(guarded(&request(), &mut |_| {}, |_| done.clone()), done);
    }
}
