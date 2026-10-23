//! Shared job queue: shallow-first, focus first, bounded by a frontier cap (design §3.5, §3.11).

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use super::control::ScanControl;
use crate::types::NodeId;

/// Above this many queued jobs, new jobs are ordered deepest-first to bound memory.
pub const DEFAULT_FRONTIER_CAP: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub dir: NodeId,
    pub path: PathBuf,
    pub depth: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Popped {
    Job(Job),
    /// The job's path is under a cancelled prefix; the caller reports `ScanMsg::Cancelled`.
    Cancelled(Job),
}

struct Entry {
    in_focus: bool,
    /// False for jobs pushed while the queue was at its cap.
    within_cap: bool,
    /// Higher pops first: shallow-first for normal jobs, deepest-first past the cap.
    rank: i64,
    seq: u64,
    job: Job,
}

impl Entry {
    fn key(&self) -> (bool, bool, i64, Reverse<u64>) {
        (self.in_focus, self.within_cap, self.rank, Reverse(self.seq))
    }
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

#[derive(Default)]
struct State {
    heap: BinaryHeap<Entry>,
    next_seq: u64,
    /// Queued plus in-progress jobs.
    pending: usize,
    closed: bool,
    seen_generation: u64,
    focus: Option<PathBuf>,
    rekeys: u64,
    last_rekey_ms: f64,
    max_queued: usize,
}

fn in_focus(focus: &Option<PathBuf>, path: &Path) -> bool {
    focus.as_deref().is_some_and(|f| path.starts_with(f))
}

impl State {
    /// Recomputes `in_focus` for every queued job if the focus changed since the last look.
    /// Other `ScanControl` changes (skips, cancel-all) only update the seen generation.
    fn refresh(&mut self, control: &ScanControl) {
        let generation = control.generation();
        if generation == self.seen_generation {
            return;
        }
        self.seen_generation = generation;
        let focus = control.focus();
        if focus == self.focus {
            return;
        }
        let started = Instant::now();
        self.focus = focus;
        let mut entries = std::mem::take(&mut self.heap).into_vec();
        for entry in &mut entries {
            entry.in_focus = in_focus(&self.focus, &entry.job.path);
        }
        self.heap = BinaryHeap::from(entries);
        self.rekeys += 1;
        self.last_rekey_ms = started.elapsed().as_secs_f64() * 1000.0;
    }
}

pub struct Queue {
    control: Arc<ScanControl>,
    cap: usize,
    state: Mutex<State>,
    ready: Condvar,
}

impl Queue {
    pub fn new(control: Arc<ScanControl>) -> Queue {
        Queue::with_cap(control, DEFAULT_FRONTIER_CAP)
    }

    pub fn with_cap(control: Arc<ScanControl>, cap: usize) -> Queue {
        Queue {
            control,
            cap,
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, job: Job) {
        let mut state = self.lock();
        state.refresh(&self.control);
        let within_cap = state.heap.len() < self.cap;
        let depth = i64::from(job.depth);
        let entry = Entry {
            in_focus: in_focus(&state.focus, &job.path),
            within_cap,
            rank: if within_cap { -depth } else { depth },
            seq: state.next_seq,
            job,
        };
        state.next_seq += 1;
        state.pending += 1;
        state.heap.push(entry);
        state.max_queued = state.max_queued.max(state.heap.len());
        drop(state);
        self.ready.notify_one();
    }

    /// Blocks until a job is available. `None` once the queue is closed or `cancel_all` is set.
    pub fn pop(&self) -> Option<Popped> {
        let mut state = self.lock();
        loop {
            if state.closed || self.control.is_cancel_all() {
                return None;
            }
            state.refresh(&self.control);
            if let Some(entry) = state.heap.pop() {
                let job = entry.job;
                return Some(if self.control.is_cancelled(&job.path) {
                    Popped::Cancelled(job)
                } else {
                    Popped::Job(job)
                });
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Marks one popped job (also a cancelled one) as finished.
    /// Returns the number of jobs still queued or in progress.
    pub fn done(&self) -> usize {
        let mut state = self.lock();
        debug_assert!(state.pending > 0, "done() without a matching push");
        state.pending -= 1;
        state.pending
    }

    /// Queued plus in-progress jobs; the scan is complete when this reaches 0.
    pub fn pending_jobs(&self) -> usize {
        self.lock().pending
    }

    /// Makes every blocked and future `pop` return `None`.
    pub fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_all();
    }

    /// Drops every queued job without reading it and returns how many were dropped.
    pub fn drain_queued(&self) -> usize {
        let mut state = self.lock();
        let drained = state.heap.len();
        state.heap.clear();
        state.pending -= drained;
        drained
    }

    /// Wakes blocked consumers so they notice `cancel_all`.
    pub fn wake_all(&self) {
        drop(self.lock());
        self.ready.notify_all();
    }

    /// How many times the queue re-keyed because the focus changed.
    pub fn rekey_count(&self) -> u64 {
        self.lock().rekeys
    }

    pub fn last_rekey_ms(&self) -> f64 {
        self.lock().last_rekey_ms
    }

    /// High-water mark of queued jobs.
    pub fn max_pending(&self) -> usize {
        self.lock().max_queued
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
    use std::thread;
    use std::time::Duration;

    use super::*;

    fn job(n: u32, path: &str, depth: u32) -> Job {
        Job {
            dir: NodeId(n),
            path: path.into(),
            depth,
        }
    }

    fn queue() -> (Arc<ScanControl>, Queue) {
        let control = Arc::new(ScanControl::new());
        let q = Queue::new(Arc::clone(&control));
        (control, q)
    }

    fn pop_dir(q: &Queue) -> u32 {
        match q.pop().unwrap() {
            Popped::Job(j) => j.dir.0,
            Popped::Cancelled(j) => panic!("unexpected cancelled {j:?}"),
        }
    }

    #[test]
    fn shallow_first_and_fifo_within_a_depth() {
        let (_c, q) = queue();
        for (n, depth) in [(1, 3), (2, 1), (3, 2), (4, 1), (5, 3)] {
            q.push(job(n, "/r/x", depth));
        }
        let order: Vec<u32> = (0..5).map(|_| pop_dir(&q)).collect();
        assert_eq!(order, vec![2, 4, 3, 1, 5]);
    }

    #[test]
    fn focus_beats_depth_and_clearing_restores_order() {
        let (c, q) = queue();
        q.push(job(1, "/r/a", 1));
        q.push(job(2, "/r/b/x", 2));
        c.set_focus(Some("/r/b".into()));
        assert_eq!(pop_dir(&q), 2);
        assert_eq!(pop_dir(&q), 1);
        assert_eq!(q.rekey_count(), 1);

        q.push(job(3, "/r/b/y", 2));
        q.push(job(4, "/r/a/z", 1));
        assert_eq!(
            pop_dir(&q),
            3,
            "new jobs under the focus are prioritised too"
        );
        q.push(job(5, "/r/b/w", 2));
        c.set_focus(None);
        assert_eq!(pop_dir(&q), 4);
        assert_eq!(pop_dir(&q), 5);
        assert_eq!(q.rekey_count(), 2, "one re-key per generation bump");
        assert!(q.last_rekey_ms() >= 0.0);
    }

    #[test]
    fn many_pushes_and_pops_do_not_rekey_without_a_change() {
        let (c, q) = queue();
        c.set_focus(Some("/r/b".into()));
        for n in 0..10 {
            q.push(job(n, "/r/a", 1));
        }
        for _ in 0..10 {
            pop_dir(&q);
        }
        assert_eq!(q.rekey_count(), 1);
    }

    #[test]
    fn focus_matches_whole_components() {
        let (c, q) = queue();
        q.push(job(1, "/r/bc", 1));
        q.push(job(2, "/r/b", 1));
        c.set_focus(Some("/r/b".into()));
        assert_eq!(pop_dir(&q), 2);
        assert_eq!(pop_dir(&q), 1);
    }

    #[test]
    fn past_the_cap_new_jobs_go_deepest_first_behind_the_rest() {
        let control = Arc::new(ScanControl::new());
        let q = Queue::with_cap(control, 4);
        for (n, depth) in [(1, 5), (2, 1), (3, 5), (4, 2)] {
            q.push(job(n, "/r/x", depth));
        }
        q.push(job(5, "/r/x", 9));
        q.push(job(6, "/r/x", 8));
        q.push(job(7, "/r/x", 10));
        let order: Vec<u32> = (0..7).map(|_| pop_dir(&q)).collect();
        assert_eq!(order, vec![2, 4, 1, 3, 7, 5, 6]);
        assert_eq!(q.max_pending(), 7);
    }

    #[test]
    fn cancelled_prefix_pops_as_cancelled() {
        let (c, q) = queue();
        c.cancel_prefix("/r/skip".into());
        q.push(job(1, "/r/skip/deep", 2));
        q.push(job(2, "/r/skipper", 1));
        assert_eq!(q.pop(), Some(Popped::Job(job(2, "/r/skipper", 1))));
        assert_eq!(q.pop(), Some(Popped::Cancelled(job(1, "/r/skip/deep", 2))));
    }

    #[test]
    fn only_a_focus_change_rekeys_not_skips_or_cancel_all() {
        let (c, q) = queue();
        q.push(job(1, "/r/a", 1));
        c.cancel_prefix("/r/skip".into());
        c.remove_prefixes_under(Path::new("/r/skip"));
        c.cancel_all();
        c.reset_cancel_all();
        q.push(job(2, "/r/b", 1));
        assert_eq!(q.rekey_count(), 0);
        c.set_focus(Some("/r/b".into()));
        assert_eq!(pop_dir(&q), 2);
        assert_eq!(q.rekey_count(), 1);
        c.set_focus(Some("/r/b".into()));
        c.cancel_prefix("/r/x".into());
        pop_dir(&q);
        assert_eq!(q.rekey_count(), 1, "same focus again is not a change");
    }

    #[test]
    fn drain_queued_drops_queued_jobs_but_not_in_progress_ones() {
        let (c, q) = queue();
        for n in 0..10 {
            q.push(job(n, "/r/a", 1));
        }
        pop_dir(&q);
        pop_dir(&q);
        c.cancel_all();
        assert_eq!(q.drain_queued(), 8);
        assert_eq!(q.pending_jobs(), 2);
        assert_eq!(q.pop(), None);
        assert_eq!(q.done(), 1);
        assert_eq!(q.done(), 0);

        c.reset_cancel_all();
        q.push(job(99, "/r/new", 1));
        assert_eq!(pop_dir(&q), 99, "drained jobs do not come back");
    }

    #[test]
    fn pending_counts_queued_and_in_progress_jobs() {
        let (_c, q) = queue();
        q.push(job(1, "/r/a", 1));
        q.push(job(2, "/r/b", 1));
        assert_eq!(q.pending_jobs(), 2);
        pop_dir(&q);
        assert_eq!(q.pending_jobs(), 2, "popped but not done");
        assert_eq!(q.done(), 1);
        pop_dir(&q);
        assert_eq!(q.done(), 0);
        assert_eq!(q.pending_jobs(), 0);
    }

    #[test]
    fn blocked_pop_wakes_on_push_close_and_cancel_all() {
        let (c, q) = queue();
        let q = Arc::new(q);
        let waiter = {
            let q = Arc::clone(&q);
            thread::spawn(move || q.pop())
        };
        thread::sleep(Duration::from_millis(50));
        q.push(job(1, "/r/a", 1));
        assert_eq!(waiter.join().unwrap(), Some(Popped::Job(job(1, "/r/a", 1))));

        let waiter = {
            let q = Arc::clone(&q);
            thread::spawn(move || q.pop())
        };
        thread::sleep(Duration::from_millis(50));
        c.cancel_all();
        q.wake_all();
        assert_eq!(waiter.join().unwrap(), None);
        c.reset_cancel_all();

        let waiter = {
            let q = Arc::clone(&q);
            thread::spawn(move || q.pop())
        };
        thread::sleep(Duration::from_millis(50));
        q.close();
        assert_eq!(waiter.join().unwrap(), None);
        assert_eq!(q.pop(), None, "stays closed");
    }

    #[test]
    fn every_job_is_popped_exactly_once_under_contention() {
        const JOBS: u32 = 100_000;
        let (_c, q) = queue();
        let q = Arc::new(q);
        let seen: Arc<Vec<AtomicU8>> = Arc::new((0..JOBS).map(|_| AtomicU8::new(0)).collect());

        let consumers: Vec<_> = (0..8)
            .map(|_| {
                let (q, seen) = (Arc::clone(&q), Arc::clone(&seen));
                thread::spawn(move || {
                    while let Some(Popped::Job(j)) = q.pop() {
                        seen[j.dir.0 as usize].fetch_add(1, Relaxed);
                        q.done();
                    }
                })
            })
            .collect();
        let producers: Vec<_> = (0..2)
            .map(|p| {
                let q = Arc::clone(&q);
                thread::spawn(move || {
                    for n in (p * JOBS / 2)..((p + 1) * JOBS / 2) {
                        q.push(job(n, "/r/x", n % 7));
                    }
                })
            })
            .collect();
        for p in producers {
            p.join().unwrap();
        }
        while q.pending_jobs() > 0 {
            thread::sleep(Duration::from_millis(5));
        }
        q.close();
        for c in consumers {
            c.join().unwrap();
        }
        assert!(seen.iter().all(|s| s.load(Relaxed) == 1));
    }
}
