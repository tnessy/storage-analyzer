//! The `diskio` helper thread: resolves the devices (which can rarely block) and samples
//! their read counters once a second into atomics. `DeviceStatSource` only loads them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::ReadBytesSource;
use super::resolve::{DeviceSet, read_bytes, resolve};
use super::sys::SysView;

const SAMPLE_EVERY: Duration = Duration::from_secs(1);
/// Longest sleep before the stop flag is looked at again.
const SLICE: Duration = Duration::from_millis(100);
/// How long `stop` waits for the thread before it lets go of it.
const STOP_WAIT: Duration = Duration::from_millis(200);

#[derive(Default)]
struct Shared {
    total: AtomicU64,
    available: AtomicBool,
    stop: AtomicBool,
    exited: (Mutex<bool>, Condvar),
}

/// Reads what the sampler thread stored. Never blocks.
#[derive(Clone)]
pub struct DeviceStatSource {
    shared: Arc<Shared>,
}

impl ReadBytesSource for DeviceStatSource {
    fn read_bytes(&mut self) -> Option<u64> {
        self.shared
            .available
            .load(Ordering::Acquire)
            .then(|| self.shared.total.load(Ordering::Acquire))
    }
}

/// Marks the source unavailable and the thread exited, however the thread ends.
struct ExitGuard(Arc<Shared>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.available.store(false, Ordering::Release);
        *self
            .0
            .exited
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
        self.0.exited.1.notify_all();
    }
}

pub struct DiskioHandle {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl DiskioHandle {
    /// Starts the thread. The device set is the root's filesystem plus everything mounted
    /// at or under it, resolved once by the thread at the start.
    pub fn start(root: PathBuf, view: Box<dyn SysView>) -> (DiskioHandle, DeviceStatSource) {
        let shared = Arc::new(Shared::default());
        let for_thread = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("diskio".into())
            .spawn(move || run(&for_thread, &root, view.as_ref()))
            .ok();
        if thread.is_none() {
            *shared
                .exited
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = true;
        }
        let source = DeviceStatSource {
            shared: Arc::clone(&shared),
        };
        (DiskioHandle { shared, thread }, source)
    }

    /// Stops the thread and waits at most `STOP_WAIT` for it; a thread stuck in a read is
    /// left behind (it ends with the process).
    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let (lock, cv) = &self.shared.exited;
        let exited = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (exited, _) = cv
            .wait_timeout_while(exited, STOP_WAIT, |done| !*done)
            .unwrap_or_else(PoisonError::into_inner);
        if *exited {
            let _ = thread.join();
        }
    }
}

impl Drop for DiskioHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sleep_until(shared: &Shared, deadline: Instant) {
    while !shared.stop.load(Ordering::Acquire) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(SLICE));
    }
}

fn run(shared: &Arc<Shared>, root: &Path, view: &dyn SysView) {
    let _guard = ExitGuard(Arc::clone(shared));
    let set: Option<DeviceSet> = resolve(root, view);
    let Some(set) = set else { return };
    while !shared.stop.load(Ordering::Acquire) {
        let started = Instant::now();
        // Any read error ends sampling: the sum would shrink and look like a counter reset.
        let Some(total) = read_bytes(view, &set) else {
            return;
        };
        shared.total.store(total, Ordering::Release);
        shared.available.store(true, Ordering::Release);
        sleep_until(shared, started + SAMPLE_EVERY);
    }
}
