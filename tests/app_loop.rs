//! Headless runs of the full app loop (T015): first paint, input priority, elapsed, redraw cap, quit.

#[path = "support/synthetic.rs"]
mod synthetic;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::Cell as BufferCell;
use ratatui::layout::{Position, Size};
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::event_loop::{
    APPLY_BUDGET, App, Clock, EventSource, Flow, ScanLaunch, ScanStarter,
};
use storage_analyzer::app::state::AppState;
use storage_analyzer::app::terminal::{Guard, PanicSlot};
use storage_analyzer::scan::{ScanControl, ScanMsg};
use storage_analyzer::types::NodeId;

const WIDTH: u16 = 80;
const HEIGHT: u16 = 24;

#[derive(Clone, Default)]
struct FakeClock(Rc<Cell<Duration>>);

impl FakeClock {
    fn advance(&self, d: Duration) {
        self.0.set(self.0.get() + d);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// Test-side handle on the scan channel: lets a test fill it to capacity on demand.
#[derive(Clone, Default)]
struct Feed {
    tx: Rc<RefCell<Option<SyncSender<ScanMsg>>>>,
    carry: Rc<RefCell<Option<ScanMsg>>>,
    room_at_key: Rc<Cell<Option<bool>>>,
}

impl Feed {
    /// Sends from `source` until the channel is full.
    fn fill(&self, source: &mut impl Iterator<Item = ScanMsg>) {
        let tx = self.tx.borrow();
        let tx = tx.as_ref().expect("starter has not run");
        loop {
            let Some(msg) = self.carry.borrow_mut().take().or_else(|| source.next()) else {
                return;
            };
            if let Err(TrySendError::Full(msg)) = tx.try_send(msg) {
                *self.carry.borrow_mut() = Some(msg);
                return;
            }
        }
    }

    fn has_room(&self) -> bool {
        let tx = self.tx.borrow();
        tx.as_ref().is_some_and(|tx| {
            tx.try_send(synthetic::listing(NodeId::ROOT, 1, vec![]))
                .is_ok()
        })
    }

    fn starter(&self) -> ScanStarter {
        let slot = self.tx.clone();
        Box::new(move |launch: ScanLaunch| {
            *slot.borrow_mut() = Some(launch.tx);
            None
        })
    }
}

/// Scripted input on the fake clock. Blocking waits advance the clock, as real time would.
#[derive(Clone)]
struct Script {
    clock: FakeClock,
    queue: Rc<RefCell<VecDeque<(Duration, Event)>>>,
    /// When set, records whether the scan channel had room at the moment a key was delivered.
    feed: Rc<RefCell<Option<Feed>>>,
    /// Blocking waits advance the clock unless a test drives time itself.
    auto_advance: Rc<Cell<bool>>,
}

impl Script {
    fn new(clock: &FakeClock) -> Self {
        Self {
            clock: clock.clone(),
            queue: Rc::default(),
            feed: Rc::default(),
            auto_advance: Rc::new(Cell::new(true)),
        }
    }

    fn at(&self, at: Duration, event: Event) {
        self.queue.borrow_mut().push_back((at, event));
    }

    fn key_now(&self, code: KeyCode) {
        self.at(self.clock.now(), key(code));
    }
}

impl EventSource for Script {
    fn next(&mut self, timeout: Duration) -> io::Result<Option<Event>> {
        let now = self.clock.now();
        let mut queue = self.queue.borrow_mut();
        if let Some(&(at, _)) = queue.front()
            && at <= now + timeout
        {
            self.clock.advance(at.saturating_sub(now));
            if let Some(feed) = &*self.feed.borrow() {
                feed.room_at_key.set(Some(feed.has_room()));
            }
            return Ok(queue.pop_front().map(|(_, e)| e));
        }
        if self.auto_advance.get() {
            self.clock.advance(timeout);
        }
        Ok(None)
    }
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl_c() -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
}

/// A `TestBackend` that counts draws and can be told to panic on the next one.
struct Probe {
    inner: TestBackend,
    draws: Rc<Cell<u32>>,
    panic_on_draw: Rc<Cell<bool>>,
}

impl Backend for Probe {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a BufferCell)>,
    {
        assert!(!self.panic_on_draw.get(), "render panic (test)");
        self.draws.set(self.draws.get() + 1);
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}

struct Rig {
    panics: PanicSlot,
    app: App<Probe, Script, FakeClock>,
    clock: FakeClock,
    script: Script,
    panic_on_draw: Rc<Cell<bool>>,
}

fn rig(starter: ScanStarter) -> Rig {
    rig_with(|_| starter)
}

/// `make` receives the backend's draw counter, so a starter can see how many frames exist.
fn rig_with(make: impl FnOnce(&Rc<Cell<u32>>) -> ScanStarter) -> Rig {
    let clock = FakeClock::default();
    let script = Script::new(&clock);
    let draws = Rc::new(Cell::new(0));
    let panic_on_draw = Rc::new(Cell::new(false));
    let backend = Probe {
        inner: TestBackend::new(WIDTH, HEIGHT),
        draws: draws.clone(),
        panic_on_draw: panic_on_draw.clone(),
    };
    let starter = make(&draws);
    let panics = PanicSlot::default();
    let config = cli::parse(APP_ID, [APP_ID]).unwrap();
    let state = AppState::new(config, synthetic::model());
    let app = App::new(
        Terminal::new(backend).unwrap(),
        script.clone(),
        clock.clone(),
        state,
        starter,
        Arc::new(AtomicBool::new(false)),
        panics.clone(),
    );
    Rig {
        panics,
        app,
        clock,
        script,
        panic_on_draw,
    }
}

fn idle_starter() -> ScanStarter {
    // Keeps the sender alive so the channel stays open and silent.
    let keep = RefCell::new(None);
    Box::new(move |launch: ScanLaunch| {
        *keep.borrow_mut() = Some(launch.tx);
        std::mem::forget(keep);
        None
    })
}

fn scripted_starter(msgs: Vec<storage_analyzer::scan::ScanMsg>) -> ScanStarter {
    Box::new(move |launch: ScanLaunch| {
        for m in msgs {
            launch.tx.send(m).unwrap();
        }
        std::mem::forget(launch.tx);
        None
    })
}

fn screen(terminal: &Terminal<Probe>) -> Vec<String> {
    let buffer = terminal.backend().inner.buffer();
    buffer
        .content
        .chunks(WIDTH as usize)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

fn text(rig: &Rig) -> String {
    screen(rig.app.terminal()).join("\n")
}

fn status_line(rig: &Rig) -> String {
    screen(rig.app.terminal())[(HEIGHT - 2) as usize]
        .trim_end()
        .to_owned()
}

fn run_until(rig: &mut Rig, until: Duration, tick: Duration) {
    while rig.clock.now() < until {
        rig.app.step().unwrap();
        rig.clock.advance(tick);
    }
}

#[test]
fn m1_frame_zero_is_drawn_before_the_scanner_starts_and_within_one_second() {
    let frames_when_started = Rc::new(Cell::new(u32::MAX));
    let seen = frames_when_started.clone();
    let mut rig = rig_with(|draws| {
        let draws = draws.clone();
        Box::new(move |_launch| {
            seen.set(draws.get());
            None
        })
    });
    let t0 = Instant::now();
    rig.app.start().unwrap();
    let first_frame = t0.elapsed();
    assert!(
        first_frame <= Duration::from_millis(1000),
        "{first_frame:?}"
    );
    println!(
        "TARGET: first_frame_ms={:.1} (target 200)",
        first_frame.as_secs_f64() * 1000.0
    );
    assert_eq!(
        frames_when_started.get(),
        1,
        "frame 0 must be on screen when the scanner starts"
    );

    let screen = text(&rig);
    assert!(
        screen.contains(&format!(" {APP_ID}  {}", synthetic::ROOT_PATH)),
        "{screen}"
    );
    assert!(screen.contains("[1 Tree]") && screen.contains("4 Skipped"));
    assert!(screen.contains("Waiting for first results…"));
    assert!(screen.contains("◌ Starting…  00:00 elapsed"), "{screen}");
    assert!(screen.contains("? help"));
    let root_row = screen.lines().find(|l| l.contains("100.0"));
    assert!(
        root_row.is_some_and(|r| r.contains('◌') && r.contains(synthetic::ROOT_PATH)),
        "{screen}"
    );
}

/// A rig whose scan channel the test fills by hand.
fn fed_rig() -> (Rig, Feed) {
    let feed = Feed::default();
    let mut rig = rig(feed.starter());
    *rig.script.feed.borrow_mut() = Some(feed.clone());
    rig.app.start().unwrap();
    (rig, feed)
}

#[test]
fn input_is_handled_before_a_message_flood_is_applied() {
    let (mut rig, feed) = fed_rig();
    let mut flood = synthetic::flood(200_000);
    feed.fill(&mut flood);
    rig.script.key_now(KeyCode::Char('q'));
    rig.app.step().unwrap();
    assert_eq!(
        feed.room_at_key.get(),
        Some(false),
        "scan messages were applied before the pending key was read"
    );
    assert!(
        text(&rig).contains(&format!("Quit {APP_ID}?")),
        "key effect missing from the first frame"
    );
    let stats = rig.app.stats();
    assert!(stats.messages_applied > 0, "the flood was not applied");
    assert!(
        stats.max_apply <= APPLY_BUDGET + Duration::from_millis(20),
        "{:?}",
        stats.max_apply
    );
}

#[test]
fn input_latency_during_a_flood_is_measured() {
    let (mut rig, feed) = fed_rig();
    let mut flood = synthetic::flood(200_000);
    let mut latencies = Vec::new();
    for i in 0..1000 {
        feed.fill(&mut flood);
        rig.script
            .key_now(KeyCode::Char(if i % 2 == 0 { 'q' } else { 'n' }));
        let t = Instant::now();
        rig.app.step().unwrap();
        latencies.push(t.elapsed().as_secs_f64() * 1000.0);
        rig.clock.advance(Duration::from_millis(1));
    }
    latencies.sort_by(f64::total_cmp);
    println!(
        "TARGET: input_p95_ms={:.1} (target 50)",
        latencies[latencies.len() * 95 / 100]
    );
}

#[test]
fn ui_never_waits_for_a_silent_scanner() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    rig.script
        .at(Duration::from_secs(1), key(KeyCode::Char('q')));
    let frames_before = rig.app.stats().frames;
    run_until(&mut rig, Duration::from_secs(5), Duration::ZERO);
    assert!(rig.app.stats().frames > frames_before + 4);
    assert!(text(&rig).contains(&format!("Quit {APP_ID}?")));
}

#[test]
fn elapsed_text_changes_in_every_second_of_a_ten_second_run() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    let mut last = status_line(&rig);
    let mut changes = Vec::new();
    while rig.clock.now() < Duration::from_secs(10) {
        rig.app.step().unwrap();
        let now = status_line(&rig);
        if now != last {
            changes.push(rig.clock.now());
            last = now;
        }
    }
    for second in 0..10u64 {
        let (from, to) = (Duration::from_secs(second), Duration::from_secs(second + 1));
        assert!(
            changes.iter().any(|&t| t > from && t <= to),
            "no change in ({from:?}, {to:?}]: {changes:?}"
        );
    }
}

#[test]
fn data_redraws_are_capped_at_ten_per_second_under_a_flood() {
    let (mut rig, feed) = fed_rig();
    rig.script.auto_advance.set(false);
    // A producer thread keeps the channel busy while the test drives simulated time.
    let tx = feed.tx.borrow_mut().take().unwrap();
    let producer = std::thread::spawn(move || {
        let mut sent = 0u64;
        for msg in synthetic::endless() {
            if tx.send(msg).is_err() {
                return sent;
            }
            sent += 1;
        }
        sent
    });
    std::thread::sleep(Duration::from_millis(50));
    while rig.clock.now() < Duration::from_secs(3) {
        rig.app.step().unwrap();
        rig.clock.advance(Duration::from_millis(10));
    }
    let stats = rig.app.stats();
    drop(rig);
    assert!(producer.join().unwrap() > 0);
    assert!(stats.messages_applied > 1000);
    assert!(
        stats.data_frames <= 31,
        "data frames: {}",
        stats.data_frames
    );
    assert!(stats.data_frames >= 1, "no data frame was drawn");
}

fn press(events: Vec<Event>) -> (Flow, bool) {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    for e in events {
        rig.script.at(Duration::ZERO, e);
    }
    let flow = rig.app.step().unwrap();
    (flow, text(&rig).contains(&format!("Quit {APP_ID}?")))
}

#[test]
fn quit_always_asks_first() {
    assert_eq!(press(vec![key(KeyCode::Char('q'))]), (Flow::Continue, true));
    assert_eq!(press(vec![ctrl_c()]), (Flow::Continue, true));
}

#[test]
fn second_q_ctrl_c_or_y_quits() {
    for second in [key(KeyCode::Char('q')), ctrl_c(), key(KeyCode::Char('y'))] {
        assert_eq!(press(vec![key(KeyCode::Char('q')), second]).0, Flow::Quit);
    }
    assert_eq!(press(vec![ctrl_c(), ctrl_c()]).0, Flow::Quit);
}

#[test]
fn n_esc_enter_stay_and_other_keys_are_ignored() {
    for stay in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        assert_eq!(
            press(vec![key(KeyCode::Char('q')), key(stay)]),
            (Flow::Continue, false)
        );
    }
    assert_eq!(
        press(vec![
            key(KeyCode::Char('q')),
            key(KeyCode::Char('x')),
            key(KeyCode::Down)
        ]),
        (Flow::Continue, true)
    );
    assert_eq!(
        press(vec![key(KeyCode::Char('y'))]),
        (Flow::Continue, false)
    );
}

#[test]
fn done_and_stopped_status_text() {
    let mut msgs = synthetic::small_tree();
    msgs.push(synthetic::done(false));
    let mut done = rig(scripted_starter(msgs));
    done.app.start().unwrap();
    done.clock.advance(Duration::from_millis(150));
    done.app.step().unwrap();
    assert_eq!(
        status_line(&done),
        "✓ Done in 00:00 · 2 files · 18.3 KiB",
        "{}",
        text(&done)
    );

    let mut msgs = synthetic::small_tree();
    msgs.push(synthetic::done(true));
    let mut stopped = rig(scripted_starter(msgs));
    stopped.app.start().unwrap();
    stopped.clock.advance(Duration::from_millis(150));
    stopped.app.step().unwrap();
    assert_eq!(
        status_line(&stopped),
        "■ Stopped at 00:00 · partial results · r on the root rescans"
    );
}

#[test]
fn elapsed_freezes_when_the_scan_finishes() {
    let mut msgs = synthetic::small_tree();
    msgs.push(synthetic::done(false));
    let mut rig = rig(scripted_starter(msgs));
    rig.app.start().unwrap();
    rig.clock.advance(Duration::from_secs(75));
    rig.app.step().unwrap();
    let line = status_line(&rig);
    assert!(line.starts_with("✓ Done in 01:15"), "{line}");
    run_until(&mut rig, Duration::from_secs(200), Duration::from_secs(1));
    assert_eq!(status_line(&rig), line);
}

#[test]
fn transient_message_lasts_five_seconds_or_until_the_next_key() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    let now = rig.clock.now();
    rig.app
        .state_mut()
        .set_transient("Showing apparent size.", now);
    rig.script.at(Duration::ZERO, key(KeyCode::Char('x')));
    // Before any key is read the message would show; the key clears it.
    rig.app.step().unwrap();
    assert!(!status_line(&rig).contains("Showing apparent size."));

    let now = rig.clock.now();
    rig.app
        .state_mut()
        .set_transient("Showing disk usage.", now);
    run_until(
        &mut rig,
        now + Duration::from_millis(4900),
        Duration::from_millis(100),
    );
    assert!(status_line(&rig).contains("Showing disk usage."));
    run_until(
        &mut rig,
        now + Duration::from_millis(5300),
        Duration::from_millis(100),
    );
    assert!(status_line(&rig).contains("Scanning") || status_line(&rig).contains("Starting"));
}

#[test]
fn a_render_panic_unwinds_through_the_terminal_guard() {
    let restored = Rc::new(Cell::new(false));
    let mut rig = rig(idle_starter());
    let flag = restored.clone();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _guard = Guard::new(move || flag.set(true));
        rig.app.start().unwrap();
        rig.panic_on_draw.set(true);
        rig.script.key_now(KeyCode::Char('q'));
        let _ = rig.app.step();
    }));
    assert!(result.is_err());
    assert!(restored.get(), "terminal guard did not run");
}

#[test]
fn eta_text_appears_only_after_two_seconds_without_a_source() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    assert_eq!(status_line(&rig), "◌ Starting…  00:00 elapsed");
    run_until(
        &mut rig,
        Duration::from_millis(1900),
        Duration::from_millis(100),
    );
    assert!(!status_line(&rig).contains("ETA"), "{}", status_line(&rig));
    run_until(
        &mut rig,
        Duration::from_millis(2300),
        Duration::from_millis(100),
    );
    assert!(
        status_line(&rig).ends_with("ETA unknown (no previous scan)"),
        "{}",
        status_line(&rig)
    );
}

#[test]
fn a_scan_worker_panic_shows_an_error_dialog_and_stops_the_scan() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    rig.panics.record(
        Some("scan-3".into()),
        "panicked at walker.rs:1:1:\nboom in worker".into(),
    );
    rig.clock.advance(Duration::from_secs(3));
    rig.app.step().unwrap();
    let screen = text(&rig);
    assert!(screen.contains("Scan error"), "{screen}");
    assert!(screen.contains("boom in worker"), "{screen}");
    assert!(
        status_line(&rig).starts_with("■ Stopped at 00:03"),
        "{}",
        status_line(&rig)
    );

    // Any key closes it; the app keeps running.
    rig.script.key_now(KeyCode::Enter);
    assert_eq!(rig.app.step().unwrap(), Flow::Continue);
    assert!(!text(&rig).contains("Scan error"));
    // `q` still asks before quitting.
    rig.script.key_now(KeyCode::Char('q'));
    assert_eq!(rig.app.step().unwrap(), Flow::Continue);
    assert!(text(&rig).contains(&format!("Quit {APP_ID}?")));
}

#[test]
fn a_panic_on_a_non_scan_thread_does_not_stop_the_scan() {
    let mut rig = rig(idle_starter());
    rig.app.start().unwrap();
    for thread in [Some("delete-1".to_owned()), None] {
        rig.panics
            .record(thread, "panicked at executor.rs:1:1:\nexecutor boom".into());
    }
    rig.clock.advance(Duration::from_secs(3));
    rig.app.step().unwrap();
    let screen = text(&rig);
    assert!(
        !screen.contains("Scan error") && !screen.contains("executor boom"),
        "{screen}"
    );
    assert!(
        status_line(&rig).starts_with("◌ Starting…  00:03"),
        "{}",
        status_line(&rig)
    );
}

/// A starter whose worker thread fills the scan channel and then blocks on a full channel.
fn blocked_worker_starter(
    control: Rc<RefCell<Option<Arc<ScanControl>>>>,
    worker: Rc<RefCell<Option<std::thread::JoinHandle<()>>>>,
) -> ScanStarter {
    Box::new(move |launch: ScanLaunch| {
        *control.borrow_mut() = Some(launch.control.clone());
        let tx = launch.tx;
        *worker.borrow_mut() = Some(std::thread::spawn(move || {
            for msg in synthetic::endless() {
                if tx.send(msg).is_err() {
                    return;
                }
            }
        }));
        None
    })
}

/// Runs the app to its end with the scan channel full, and checks that quitting is prompt,
/// cancels the scan, and lets the blocked worker go.
fn assert_quit_is_prompt(quit: impl FnOnce(&Script, &Arc<AtomicBool>)) {
    let control = Rc::default();
    let worker = Rc::default();
    let clock = FakeClock::default();
    let script = Script::new(&clock);
    let terminate = Arc::new(AtomicBool::new(false));
    let backend = Probe {
        inner: TestBackend::new(WIDTH, HEIGHT),
        draws: Rc::default(),
        panic_on_draw: Rc::default(),
    };
    let state = AppState::new(cli::parse(APP_ID, [APP_ID]).unwrap(), synthetic::model());
    let app = App::new(
        Terminal::new(backend).unwrap(),
        script.clone(),
        clock,
        state,
        blocked_worker_starter(Rc::clone(&control), Rc::clone(&worker)),
        terminate.clone(),
        PanicSlot::default(),
    );
    quit(&script, &terminate);
    let t0 = Instant::now();
    app.run().unwrap();
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    let control: Arc<ScanControl> = control.borrow().clone().expect("starter ran");
    assert!(control.is_cancel_all(), "quit did not cancel the scan");
    let worker = worker.borrow_mut().take().unwrap();
    let joined_at = Instant::now();
    while !worker.is_finished() {
        assert!(
            joined_at.elapsed() < Duration::from_secs(2),
            "worker still blocked"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    worker.join().unwrap();
}

#[test]
fn quit_with_a_full_scan_channel_via_q_y_returns_promptly() {
    assert_quit_is_prompt(|script, _| {
        script.at(Duration::from_millis(50), key(KeyCode::Char('q')));
        script.at(Duration::from_millis(60), key(KeyCode::Char('y')));
    });
}

#[test]
fn quit_with_a_full_scan_channel_via_sigterm_returns_promptly() {
    assert_quit_is_prompt(|_, terminate| {
        signal_hook::flag::register(signal_hook::consts::SIGTERM, terminate.clone()).unwrap();
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
    });
}

#[test]
fn quitting_during_a_real_scan_with_a_full_channel_returns_promptly() {
    use storage_analyzer::app::{preflight, production_starter};
    use storage_analyzer::model::{Model, ModelOptions};

    let dir = tempfile::tempdir().unwrap();
    for i in 0..3000 {
        std::fs::create_dir(dir.path().join(format!("d{i:04}"))).unwrap();
    }
    let root = preflight(dir.path()).unwrap();
    let model = Model::new(root.path, root.entry, ModelOptions { top_n: 10 });
    let clock = FakeClock::default();
    let script = Script::new(&clock);
    let backend = Probe {
        inner: TestBackend::new(WIDTH, HEIGHT),
        draws: Rc::default(),
        panic_on_draw: Rc::default(),
    };
    let state = AppState::new(cli::parse(APP_ID, [APP_ID]).unwrap(), model);
    let mut app = App::new(
        Terminal::new(backend).unwrap(),
        script.clone(),
        clock,
        state,
        production_starter(),
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    );
    app.start().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    app.step().unwrap();
    assert!(
        app.stats().messages_applied > 0,
        "the real scanner produced nothing"
    );
    // The walker refills the channel while the app is not draining it.
    std::thread::sleep(Duration::from_millis(300));
    script.at(Duration::ZERO, key(KeyCode::Char('q')));
    script.at(Duration::ZERO, key(KeyCode::Char('y')));
    let t0 = Instant::now();
    assert_eq!(app.step().unwrap(), Flow::Quit);
    app.finish();
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
}
