//! The main loop (ADR-003, design §3.3): input first, bounded message drain, capped redraws.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, channel, sync_channel};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::Backend;

use super::state::{AppState, Dialog};
use super::terminal::PanicSlot;
use crate::eta::sources::InodeSource;
use crate::history::{self, HistoryEntry, HistoryError, HistoryStore};
use crate::scan::{ScanControl, ScanHandle, ScanMsg, ScanOptions};
use crate::types::NodeId;
use crate::ui::{self, copy};

/// Longest the main thread spends applying scan messages per iteration.
pub const APPLY_BUDGET: Duration = Duration::from_millis(8);
/// Data-driven redraws happen at most this often (≤ 10 Hz).
pub const DATA_REDRAW_INTERVAL: Duration = Duration::from_millis(100);
pub const SCAN_CHANNEL_CAPACITY: usize = 1024;
/// Scan worker threads are named with this prefix (T006).
pub const SCAN_THREAD_PREFIX: &str = "scan-";
const WORKERS_GONE: &str = "All scan workers exited before the scan finished.";
const MAX_EVENTS_PER_STEP: usize = 256;
const IDLE_TIMEOUT: Duration = Duration::from_secs(3600);
/// While scanning, wake at least this often to look at the scan channel.
const SCAN_POLL: Duration = Duration::from_millis(100);
/// The real event source never blocks longer than this, so signal flags are seen promptly.
const SIGNAL_CHECK: Duration = Duration::from_millis(250);

pub trait EventSource {
    /// Waits up to `timeout` for one event.
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>>;
}

/// Monotonic time since an arbitrary start.
pub trait Clock {
    fn now(&self) -> Duration;
}

pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

pub struct CrosstermEvents;

impl EventSource for CrosstermEvents {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        if event::poll(timeout.min(SIGNAL_CHECK))? {
            Ok(Some(event::read()?))
        } else {
            Ok(None)
        }
    }
}

/// Everything the scanner needs; handed to the starter after frame 0.
pub struct ScanLaunch {
    pub root: PathBuf,
    pub root_dev: u64,
    pub options: ScanOptions,
    pub control: Arc<ScanControl>,
    pub tx: SyncSender<ScanMsg>,
    pub first_id: u32,
}

pub type ScanStarter = Box<dyn FnOnce(ScanLaunch) -> Option<ScanHandle>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LoopStats {
    pub frames: u64,
    /// Frames drawn with no input in that iteration.
    pub data_frames: u64,
    pub messages_applied: u64,
    /// Longest single message drain, for the "input first" check.
    pub max_apply: Duration,
}

/// What the loop talks to besides the terminal and the scanner; tests swap these.
pub struct Services {
    pub history: Arc<dyn HistoryStore>,
    pub inodes: Box<dyn InodeSource>,
}

/// Nothing stored, no inode counts: the defaults for tests that don't care.
struct NoHistory;

impl HistoryStore for NoHistory {
    fn load(&self) -> Result<Vec<HistoryEntry>, HistoryError> {
        Ok(Vec::new())
    }

    fn save(&self, _entries: &[HistoryEntry]) -> Result<(), HistoryError> {
        Ok(())
    }
}

struct NoInodes;

impl InodeSource for NoInodes {
    fn used_inodes(&self, _root: &std::path::Path) -> Option<u64> {
        None
    }
}

impl Default for Services {
    fn default() -> Self {
        Self {
            history: Arc::new(NoHistory),
            inodes: Box::new(NoInodes),
        }
    }
}

pub struct App<B: Backend, E: EventSource, C: Clock> {
    terminal: Terminal<B>,
    events: E,
    clock: C,
    state: AppState,
    starter: Option<ScanStarter>,
    rx: Receiver<ScanMsg>,
    tx: Option<SyncSender<ScanMsg>>,
    terminate: Arc<AtomicBool>,
    panics: PanicSlot,
    last_draw: Option<Duration>,
    needs_draw: bool,
    backlog: bool,
    /// Every sender is gone; with a running scan that means the workers died.
    rx_disconnected: bool,
    stats: LoopStats,
    services: Services,
    /// The history file's contents, arriving from the loader thread.
    history_rx: Option<Receiver<Result<Vec<HistoryEntry>, HistoryError>>>,
    last_spinner_frame: usize,
}

fn io_err<T: std::error::Error>(e: T) -> io::Error {
    io::Error::other(e.to_string())
}

impl<B: Backend, E: EventSource, C: Clock> App<B, E, C> {
    pub fn new(
        terminal: Terminal<B>,
        events: E,
        clock: C,
        state: AppState,
        starter: ScanStarter,
        terminate: Arc<AtomicBool>,
        panics: PanicSlot,
    ) -> Self {
        let (tx, rx) = sync_channel(SCAN_CHANNEL_CAPACITY);
        Self {
            terminal,
            events,
            clock,
            state,
            starter: Some(starter),
            rx,
            tx: Some(tx),
            terminate,
            panics,
            last_draw: None,
            needs_draw: false,
            backlog: false,
            rx_disconnected: false,
            stats: LoopStats::default(),
            services: Services::default(),
            history_rx: None,
            last_spinner_frame: 0,
        }
    }

    pub fn with_services(mut self, services: Services) -> Self {
        self.services = services;
        self
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn terminal(&self) -> &Terminal<B> {
        &self.terminal
    }

    pub fn stats(&self) -> LoopStats {
        self.stats
    }

    /// Frame 0 first, then the scanner (M1).
    pub fn start(&mut self) -> io::Result<()>
    where
        B::Error: 'static,
    {
        let now = self.clock.now();
        self.state.started_at = now;
        self.state.tick(now);
        self.draw(now, true)?;
        if let (Some(starter), Some(tx)) = (self.starter.take(), self.tx.take()) {
            let root = self.state.model.root();
            self.state.scan_handle = starter(ScanLaunch {
                root: self.state.model.path(root),
                root_dev: self.state.model.identity(root).0,
                options: self.state.config.scan_options(),
                control: self.state.control.clone(),
                tx,
                first_id: NodeId::ROOT.0 + 1,
            });
        }
        self.start_eta_sources();
        Ok(())
    }

    /// Frame 0 is on screen: now look for an expected total. The history file is read on its own
    /// thread so a slow disk never delays input; the inode count is one `statvfs`.
    fn start_eta_sources(&mut self) {
        let root = self.state.model.path(self.state.model.root());
        self.state.eta_sources.inodes = self.services.inodes.used_inodes(&root);
        if !self.state.config.history {
            return;
        }
        let store = Arc::clone(&self.services.history);
        let (tx, rx) = channel();
        let spawned = thread::Builder::new()
            .name("history-load".into())
            .spawn(move || {
                let _ = tx.send(store.load());
            });
        if spawned.is_ok() {
            self.history_rx = Some(rx);
        }
    }

    /// Picks up the history file once it has been read.
    fn poll_history(&mut self) {
        let Some(rx) = &self.history_rx else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Ok(Vec::new()),
        };
        self.history_rx = None;
        match result {
            Ok(entries) => {
                let (root, one_filesystem) = self.state.root_key();
                self.state.eta_sources.history = history::lookup(&entries, &root, one_filesystem)
                    .map(|e| e.entries)
                    .filter(|&n| n > 0);
            }
            Err(_) => {
                let now = self.clock.now();
                self.state.set_transient(copy::HISTORY_IGNORED, now);
            }
        }
        self.needs_draw = true;
    }

    /// Writes the history record of a scan that has just finished, if the user opted in.
    fn write_history(&mut self) {
        let Some(entry) = self.state.pending_history_write.take() else {
            return;
        };
        // Merge into whatever is on disk now; an unreadable file is replaced by this record.
        let mut entries = self.services.history.load().unwrap_or_default();
        history::upsert(&mut entries, entry);
        if self.services.history.save(&entries).is_err() {
            let now = self.clock.now();
            self.state.set_transient(copy::HISTORY_NOT_SAVED, now);
            self.needs_draw = true;
        }
    }

    /// Runs until the user quits or a termination signal arrives.
    pub fn run(mut self) -> io::Result<()>
    where
        B::Error: 'static,
    {
        self.start()?;
        while self.step()? == Flow::Continue {}
        self.finish();
        Ok(())
    }

    /// Cancels the scan and joins its workers.
    pub fn finish(self) {
        let Self { rx, state, .. } = self;
        state.control.cancel_all();
        // Workers blocked on a full channel must see it close before they are joined.
        drop(rx);
        if let Some(handle) = state.scan_handle {
            handle.shutdown();
        }
    }

    /// One iteration: wait for input, handle all of it, drain scan messages, maybe draw.
    pub fn step(&mut self) -> io::Result<Flow>
    where
        B::Error: 'static,
    {
        let timeout = self.next_timeout();
        let mut redraw = false;
        let mut handled = 0;
        let mut next = self.events.next(timeout)?;
        while let Some(event) = next {
            redraw |= self.handle_event(event);
            handled += 1;
            if handled == MAX_EVENTS_PER_STEP {
                break;
            }
            next = self.events.next(Duration::ZERO)?;
        }
        if self.state.quit || self.terminate.load(Ordering::Relaxed) {
            return Ok(Flow::Quit);
        }

        if self.drain_scan() > 0 {
            self.needs_draw = true;
        }
        self.check_scan_health();
        let now = self.clock.now();
        if self.state.tick(now) {
            self.needs_draw = true;
        }
        self.state.process_events();
        self.state.poll_controls();
        self.poll_history();
        self.write_history();
        if self.state.is_scanning() && !self.state.config.no_animation {
            let frame = self.state.spinner_frame();
            if frame != self.last_spinner_frame {
                self.last_spinner_frame = frame;
                self.needs_draw = true;
            }
        }
        let due = self
            .last_draw
            .is_none_or(|t| now.saturating_sub(t) >= DATA_REDRAW_INTERVAL);
        if redraw || (self.needs_draw && due) {
            self.draw(now, redraw)?;
        }
        Ok(Flow::Continue)
    }

    /// Returns whether the event needs a redraw.
    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                self.state.handle_key(key);
                true
            }
            Event::Resize(..) => true,
            _ => false,
        }
    }

    /// A dead scan worker stops the scan and tells the user; the rest of the UI keeps working.
    fn check_scan_health(&mut self) {
        // Only scan workers decide the scan's fate; a helper such as the delete executor
        // reports its own failures.
        let mut failures: Vec<String> = self
            .panics
            .take()
            .into_iter()
            .filter(|p| {
                p.thread
                    .as_deref()
                    .is_some_and(|t| t.starts_with(SCAN_THREAD_PREFIX))
            })
            .map(|p| p.message)
            .collect();
        if self.state.scan_handle.is_some() && self.state.is_scanning() && self.rx_disconnected {
            failures.push(WORKERS_GONE.to_owned());
        }
        if failures.is_empty() {
            return;
        }
        self.state.apply_scan_msg(ScanMsg::Done { cancelled: true });
        self.state.dialog = Some(Dialog::ScanError(failures.join("\n")));
        self.needs_draw = true;
    }

    fn drain_scan(&mut self) -> u64 {
        let start = Instant::now();
        let mut applied = 0;
        self.backlog = false;
        loop {
            match self.rx.try_recv() {
                Ok(msg) => {
                    self.state.apply_scan_msg(msg);
                    applied += 1;
                    if start.elapsed() >= APPLY_BUDGET {
                        self.backlog = true;
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.rx_disconnected = true;
                    break;
                }
            }
        }
        self.stats.messages_applied += applied;
        self.stats.max_apply = self.stats.max_apply.max(start.elapsed());
        applied
    }

    fn next_timeout(&self) -> Duration {
        if self.backlog {
            return Duration::ZERO;
        }
        let now = self.clock.now();
        let mut timeout = IDLE_TIMEOUT;
        if let (true, Some(last)) = (self.needs_draw, self.last_draw) {
            timeout = timeout.min((last + DATA_REDRAW_INTERVAL).saturating_sub(now));
        }
        if self.state.is_scanning() {
            let next_second =
                self.state.started_at + Duration::from_secs(self.state.elapsed.as_secs() + 1);
            timeout = timeout.min(next_second.saturating_sub(now)).min(SCAN_POLL);
        }
        if let Some(t) = &self.state.transient {
            timeout = timeout.min(t.until.saturating_sub(now));
        }
        timeout
    }

    fn draw(&mut self, now: Duration, input_driven: bool) -> io::Result<()>
    where
        B::Error: 'static,
    {
        let area = self.terminal.size().map_err(io_err)?;
        self.state.prepare_view(ratatui::layout::Rect::from(area));
        self.terminal
            .draw(|frame| ui::render(frame, &self.state))
            .map_err(io_err)?;
        self.last_draw = Some(now);
        self.needs_draw = false;
        self.stats.frames += 1;
        if !input_driven {
            self.stats.data_frames += 1;
        }
        Ok(())
    }
}
