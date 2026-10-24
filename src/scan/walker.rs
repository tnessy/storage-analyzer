//! Worker threads, job processing and completion (design §3.3, §3.5, §3.11).

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use super::ScanOptions;
use super::control::ScanControl;
use super::fs::{Errno, FsAccess, StatInfo};
use super::msg::{RawEntry, ScanMsg};
use super::queue::{Job, Popped, Queue};
use crate::types::{IssueKind, Kind, NodeId, Op};

/// Longest `ScanHandle::shutdown` (and drop) waits for the workers.
const EXIT_WAIT: Duration = Duration::from_secs(2);
/// Retry interval of a send on a full channel.
const SEND_RETRY: Duration = Duration::from_micros(500);
/// How often a parked worker rechecks `cancel_all` (it is normally woken by `rescan`).
const PARK_RECHECK: Duration = Duration::from_millis(100);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// State shared by the workers and the handle; independent of the filesystem type.
struct Shared {
    queue: Queue,
    control: Arc<ScanControl>,
    tx: SyncSender<ScanMsg>,
    next_id: AtomicU32,
    root_dev: u64,
    one_filesystem: bool,
    /// Set when the handle is closed or the receiver is gone: workers stop, sends are abandoned.
    closing: AtomicBool,
    /// Directories whose next job is a rescan (their identity is refreshed first).
    rescans: Mutex<HashSet<NodeId>>,
    /// Whether `Done` was already sent for the work currently known.
    done_sent: Mutex<bool>,
    parked: Mutex<()>,
    unpark: Condvar,
    exited: Mutex<usize>,
    exited_changed: Condvar,
}

impl Shared {
    fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Relaxed)
    }

    fn close(&self) {
        self.closing.store(true, Ordering::Relaxed);
        self.queue.close();
        self.wake_parked();
    }

    fn wake_parked(&self) {
        drop(lock(&self.parked));
        self.unpark.notify_all();
    }

    /// Sends a message. Normally this waits for room (backpressure); once the scan is
    /// cancelled or closing, a full channel makes it give up. Returns false if not sent.
    fn send(&self, msg: ScanMsg) -> bool {
        self.send_until(msg, || self.is_closing() || self.control.is_cancel_all())
    }

    /// `Done` is worth waiting for while a cancelled scan is still being drained; only
    /// closing (or a gone receiver) abandons it.
    fn send_done(&self, cancelled: bool) -> bool {
        self.send_until(ScanMsg::Done { cancelled }, || self.is_closing())
    }

    fn send_until(&self, mut msg: ScanMsg, give_up: impl Fn() -> bool) -> bool {
        loop {
            match self.tx.try_send(msg) {
                Ok(()) => return true,
                Err(TrySendError::Disconnected(_)) => {
                    self.close();
                    return false;
                }
                Err(TrySendError::Full(back)) => {
                    if give_up() {
                        return false;
                    }
                    msg = back;
                    std::thread::sleep(SEND_RETRY);
                }
            }
        }
    }

    /// Sends `Done` once nothing is queued or running.
    fn check_complete(&self) {
        // Decided together with the cancel flag: a rescan clears both under this lock.
        let done = {
            let mut sent = lock(&self.done_sent);
            let complete = self.queue.pending_jobs() == 0 && !*sent;
            if complete {
                *sent = true;
            }
            complete.then(|| self.control.is_cancel_all())
        };
        if let Some(cancelled) = done {
            self.send_done(cancelled);
        }
    }

    fn park_while_cancelled(&self) {
        let mut guard = lock(&self.parked);
        while !self.is_closing() && self.control.is_cancel_all() {
            guard = self
                .unpark
                .wait_timeout(guard, PARK_RECHECK)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// Counts a worker as exited when its thread ends, also on a panic.
struct ExitGuard(Arc<Shared>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        *lock(&self.0.exited) += 1;
        self.0.exited_changed.notify_all();
    }
}

fn map_errno(e: Errno) -> IssueKind {
    match e {
        Errno::ACCESS | Errno::PERM => IssueKind::PermissionDenied,
        Errno::NOENT => IssueKind::Vanished,
        other => IssueKind::Other(other.raw_os_error()),
    }
}

fn raw_entry(name: &[u8], st: &StatInfo) -> RawEntry {
    RawEntry {
        name: name.into(),
        kind: st.kind,
        disk_bytes: st.disk_bytes,
        apparent_bytes: st.apparent_bytes,
        ino: st.ino,
        dev: st.dev,
        nlink: st.nlink,
        issue: None,
    }
}

fn unstatable_entry(name: Box<[u8]>, e: Errno) -> RawEntry {
    RawEntry {
        name,
        kind: Kind::Other,
        disk_bytes: 0,
        apparent_bytes: 0,
        ino: 0,
        dev: 0,
        nlink: 0,
        issue: Some((Op::Stat, map_errno(e))),
    }
}

struct Worker<F: FsAccess> {
    fs: Arc<F>,
    shared: Arc<Shared>,
}

impl<F: FsAccess> Worker<F> {
    fn run(&self) {
        let shared = &self.shared;
        while !shared.is_closing() {
            match shared.queue.pop() {
                Some(Popped::Job(job)) => {
                    self.process(&job);
                    self.finish();
                }
                Some(Popped::Cancelled(job)) => {
                    shared.send(ScanMsg::Cancelled { dir: job.dir });
                    self.finish();
                }
                // The cancel (and the drop of queued jobs) happened inside `pop`. A rescan may
                // already have cleared it again, so only a closing handle ends the worker.
                None if !shared.is_closing() => {
                    shared.check_complete();
                    shared.park_while_cancelled();
                }
                None => break,
            }
        }
    }

    fn finish(&self) {
        self.shared.queue.done();
        self.shared.check_complete();
    }

    fn process(&self, job: &Job) {
        let shared = &self.shared;
        let is_rescan = lock(&shared.rescans).remove(&job.dir);
        if is_rescan && !self.refresh_identity(job) {
            return;
        }
        let issue = |op, e| ScanMsg::DirIssue {
            dir: job.dir,
            op,
            kind: map_errno(e),
        };
        let mut dir = match self.fs.open_dir(&job.path) {
            Ok(d) => d,
            Err(e) => {
                shared.send(issue(Op::Open, e));
                return;
            }
        };
        let names = match self.fs.read_entries(&mut dir) {
            Ok(n) => n,
            Err(e) => {
                shared.send(issue(Op::Read, e));
                return;
            }
        };

        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let entry = match self.fs.stat_at(&dir, &name) {
                Ok(st) => {
                    let mut entry = raw_entry(&name, &st);
                    if st.kind == Kind::Dir && shared.one_filesystem && st.dev != shared.root_dev {
                        entry.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
                    }
                    entry
                }
                Err(e) => unstatable_entry(name, e),
            };
            entries.push(entry);
        }
        drop(dir);

        let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
        let first = shared.next_id.fetch_add(count, Ordering::Relaxed);
        let children: Vec<Job> = entries
            .iter()
            .zip(first..)
            .filter(|(e, _)| e.kind == Kind::Dir && e.issue.is_none())
            .map(|(e, id)| Job {
                dir: NodeId(id),
                path: job.path.join(OsStr::from_bytes(&e.name)),
                depth: job.depth + 1,
            })
            .collect();
        // The listing goes out before any child job exists (ordering contract).
        let sent = shared.send(ScanMsg::Listing {
            dir: job.dir,
            first_id: NodeId(first),
            entries,
        });
        if sent && !shared.control.is_cancel_all() {
            for child in children {
                shared.queue.push(child);
            }
        }
    }

    /// Re-stats a rescan root without following symlinks and reports what it is now.
    /// Returns true if the root is still a directory and should be listed.
    fn refresh_identity(&self, job: &Job) -> bool {
        let shared = &self.shared;
        let (Some(parent), Some(name)) = (job.path.parent(), job.path.file_name()) else {
            return true;
        };
        let issue = |op, e| ScanMsg::DirIssue {
            dir: job.dir,
            op,
            kind: map_errno(e),
        };
        let parent_dir = match self.fs.open_dir(parent) {
            Ok(d) => d,
            Err(e) => {
                shared.send(issue(Op::Open, e));
                return false;
            }
        };
        match self.fs.stat_at(&parent_dir, name.as_bytes()) {
            Err(e) => {
                shared.send(issue(Op::Stat, e));
                false
            }
            Ok(st) => {
                let mut entry = raw_entry(name.as_bytes(), &st);
                // A folder that moved to another device (a mount appeared) is not entered.
                let elsewhere =
                    st.kind == Kind::Dir && shared.one_filesystem && st.dev != shared.root_dev;
                if elsewhere {
                    entry.issue = Some((Op::Stat, IssueKind::OtherFilesystem));
                }
                shared.send(ScanMsg::Identity {
                    node: job.dir,
                    entry,
                }) && st.kind == Kind::Dir
                    && !elsewhere
            }
        }
    }
}

/// Owns the worker threads and the job queue.
pub struct ScanHandle {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

/// Starts the workers. `first_id` is the first `NodeId` the scan may allocate.
pub fn start<F: FsAccess + 'static>(
    fs: Arc<F>,
    root: PathBuf,
    root_dev: u64,
    opts: ScanOptions,
    control: Arc<ScanControl>,
    tx: SyncSender<ScanMsg>,
    first_id: u32,
) -> ScanHandle {
    let shared = Arc::new(Shared {
        queue: Queue::new(Arc::clone(&control)),
        control,
        tx,
        next_id: AtomicU32::new(first_id),
        root_dev,
        one_filesystem: opts.one_filesystem,
        closing: AtomicBool::new(false),
        rescans: Mutex::new(HashSet::new()),
        done_sent: Mutex::new(false),
        parked: Mutex::new(()),
        unpark: Condvar::new(),
        exited: Mutex::new(0),
        exited_changed: Condvar::new(),
    });
    shared.queue.push(Job {
        dir: NodeId::ROOT,
        path: root,
        depth: 0,
    });

    let threads: Vec<JoinHandle<()>> = (0..opts.threads.max(1))
        .filter_map(|i| {
            let worker = Worker {
                fs: Arc::clone(&fs),
                shared: Arc::clone(&shared),
            };
            let for_guard = Arc::clone(&shared);
            std::thread::Builder::new()
                .name(format!("scan-{i}"))
                .spawn(move || {
                    let _exit = ExitGuard(for_guard);
                    worker.run();
                })
                .ok()
        })
        .collect();
    if threads.is_empty() {
        shared.send(ScanMsg::DirIssue {
            dir: NodeId::ROOT,
            op: Op::Open,
            kind: IssueKind::Other(Errno::AGAIN.raw_os_error()),
        });
        shared.send_done(false);
    }
    ScanHandle { shared, threads }
}

impl ScanHandle {
    /// Works at any time, also after `Done` / `Stopped`.
    pub fn rescan(&self, dir: NodeId, path: PathBuf, depth: u32) {
        self.rescan_many(&[(dir, path, depth)]);
    }

    /// Rescans several directories as one step: no `Done` can appear between them.
    pub fn rescan_many(&self, targets: &[(NodeId, PathBuf, u32)]) {
        let shared = &self.shared;
        if targets.is_empty() || shared.is_closing() {
            return;
        }
        // One step: a worker deciding that the scan is complete sees either the old state or
        // all of the resumed one, and the queue clears the cancel and adds each job atomically.
        let mut done_sent = lock(&shared.done_sent);
        *done_sent = false;
        for (dir, path, depth) in targets {
            lock(&shared.rescans).insert(*dir);
            shared.queue.push_resume(Job {
                dir: *dir,
                path: path.clone(),
                depth: *depth,
            });
        }
        drop(done_sent);
        shared.wake_parked();
    }

    /// Stops the workers and waits up to 2 s for them; stragglers are detached.
    pub fn shutdown(mut self) {
        self.stop_workers();
    }

    fn stop_workers(&mut self) {
        let shared = &self.shared;
        shared.close();
        let total = self.threads.len();
        let exited = lock(&shared.exited);
        let (exited, _) = shared
            .exited_changed
            .wait_timeout_while(exited, EXIT_WAIT, |n| *n < total)
            .unwrap_or_else(PoisonError::into_inner);
        let all_exited = *exited >= total;
        drop(exited);
        for thread in self.threads.drain(..) {
            if all_exited {
                let _ = thread.join();
            }
        }
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.stop_workers();
    }
}
