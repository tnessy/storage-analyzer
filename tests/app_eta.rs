//! ETA sources and the status line's progress text (T019).

use std::time::Duration;

use storage_analyzer::eta::EtaSource;
use storage_analyzer::ui::format;
use storage_analyzer::ui::status::{
    EtaProgress, Progress, estimating_line, eta_line, indeterminate_line,
};

fn spec_progress() -> Progress {
    Progress {
        glyph: "◌".into(),
        word: "Scanning".into(),
        elapsed: Duration::from_secs(72),
        top: Some((12, 31)),
        queued: 12_400,
        entries: 1_240_000,
        bytes: (96.1 * 1024f64.powi(3)) as u64,
        rate_eps: Some(41_200.0),
    }
}

/// The line after each drop, widest first (UX §5.2 "80-column drop order").
const STEPS: [&str; 9] = [
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · 1.24M entries · 96.1 GiB · 41.2k/s · ETA unknown (no previous scan)",
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · 1.24M entries · 96.1 GiB · ETA unknown (no previous scan)",
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · 1.24M entries · ETA unknown (no previous scan)",
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · ETA unknown (no previous scan)",
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · ETA unknown",
    "◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k queued · ETA unknown",
    "◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown",
    "◌ Scanning  01:12 elapsed · 12/31 top-level done · ETA unknown",
    "◌ Scanning  01:12 elapsed · ETA unknown",
];

#[test]
fn the_full_line_is_the_spec_example() {
    assert_eq!(indeterminate_line(&spec_progress(), 400), STEPS[0]);
}

#[test]
fn at_80_columns_it_is_the_specs_example_line() {
    let line = indeterminate_line(&spec_progress(), 80);
    assert_eq!(
        line,
        "◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown"
    );
    // The spec says 79 columns; the string it gives is 78 wide. Either way it fits 80.
    assert_eq!(format::width(&line), 78);
}

#[test]
fn fields_drop_in_the_specified_order_one_step_at_a_time() {
    let p = spec_progress();
    for (i, step) in STEPS.iter().enumerate() {
        let width = format::width(step);
        assert_eq!(
            &indeterminate_line(&p, width),
            step,
            "exactly fits step {i}"
        );
        if let Some(next) = STEPS.get(i + 1) {
            assert_eq!(
                &indeterminate_line(&p, width - 1),
                next,
                "one cell less drops to step {}",
                i + 1
            );
        }
    }
}

#[test]
fn glyph_word_elapsed_and_eta_unknown_are_never_dropped() {
    let p = spec_progress();
    for width in [0, 10, 20, 30, 40] {
        let line = indeterminate_line(&p, width);
        assert!(
            line.contains("Scanning  01:12 elapsed · ETA unknown"),
            "{width}: {line}"
        );
        assert!(line.starts_with('◌'));
    }
}

#[test]
fn no_source_means_no_percent_bar_or_eta_digits_at_any_width() {
    for width in [80, 120, 200] {
        let line = indeterminate_line(&spec_progress(), width);
        assert!(!line.contains('%'), "{line}");
        assert!(!line.chars().any(|c| "█▉▊▋▌▍▎▏░".contains(c)), "{line}");
        let after = &line[line.find("ETA").expect("an ETA field")..];
        assert!(after.starts_with("ETA unknown"), "{after}");
        assert!(!after.chars().any(|c| c.is_ascii_digit()), "{after}");
    }
}

#[test]
fn absent_quantities_leave_their_fields_out() {
    let mut p = spec_progress();
    p.rate_eps = None;
    p.top = None;
    p.queued = 0;
    let line = indeterminate_line(&p, 400);
    assert_eq!(
        line,
        "◌ Scanning  01:12 elapsed · 1.24M entries · 96.1 GiB · ETA unknown (no previous scan)"
    );
    p.entries = 0;
    p.bytes = 0;
    assert_eq!(
        indeterminate_line(&p, 400),
        "◌ Scanning  01:12 elapsed · ETA unknown (no previous scan)"
    );
}

fn eta_progress(fraction: f32, source: EtaSource) -> EtaProgress {
    EtaProgress {
        glyph: "◌".into(),
        word: "Scanning".into(),
        elapsed: Duration::from_secs(72),
        fraction,
        source,
    }
}

const ETA_STEPS: [&str; 4] = [
    "◌ Scanning ████████░░░░░░░░ ~50%  01:12 elapsed  ETA ~01:44 (estimate, last scan)",
    "◌ Scanning █████░░░░░ ~50%  01:12 elapsed  ETA ~01:44 (estimate, last scan)",
    "◌ Scanning █████░░░░░ ~50% · 01:12  ETA ~01:44 (estimate, last scan)",
    "◌ Scanning ~50% · 01:12  ETA ~01:44 (estimate, last scan)",
];

#[test]
fn the_eta_line_has_a_bar_percent_elapsed_and_a_labelled_estimate() {
    let p = eta_progress(0.5, EtaSource::History);
    let remaining = Some(Duration::from_secs(104));
    assert_eq!(eta_line(&p, remaining, 400), ETA_STEPS[0]);
    let fs = eta_line(&eta_progress(0.5, EtaSource::FsInodes), remaining, 400);
    assert!(fs.ends_with("ETA ~01:44 (estimate, filesystem)"), "{fs}");
    let session = eta_line(
        &eta_progress(0.5, EtaSource::SessionPrevious),
        remaining,
        400,
    );
    assert!(session.ends_with("(estimate, last scan)"), "{session}");
}

#[test]
fn the_eta_line_drops_the_bar_size_then_elapsed_then_the_bar() {
    let p = eta_progress(0.5, EtaSource::History);
    let remaining = Some(Duration::from_secs(104));
    for (i, step) in ETA_STEPS.iter().enumerate() {
        let width = format::width(step);
        assert_eq!(&eta_line(&p, remaining, width), step, "step {i}");
        if let Some(next) = ETA_STEPS.get(i + 1) {
            assert_eq!(
                &eta_line(&p, remaining, width - 1),
                next,
                "one cell less → step {}",
                i + 1
            );
        }
    }
    // What is never dropped, however narrow.
    let tiny = eta_line(&p, remaining, 10);
    for must in ["~50%", "ETA ~01:44", "estimate", "last scan"] {
        assert!(tiny.contains(must), "{tiny}");
    }
}

#[test]
fn a_fraction_never_reads_one_hundred_and_the_bar_fills_in_eighths() {
    let p = eta_progress(0.99, EtaSource::History);
    let line = eta_line(&p, Some(Duration::from_secs(1)), 400);
    assert!(line.contains("~99%") && !line.contains("100"), "{line}");
    let p = eta_progress(0.41, EtaSource::History);
    assert!(eta_line(&p, Some(Duration::from_secs(1)), 400).contains("██████▌░░░░░░░░░"));
}

#[test]
fn past_estimate_and_estimating_lines_follow_the_spec() {
    let p = EtaProgress {
        elapsed: Duration::from_secs(242),
        ..eta_progress(0.99, EtaSource::History)
    };
    assert_eq!(
        eta_line(&p, None, 400),
        "◌ Scanning ████████████████ ~99%  04:02 elapsed  ETA unknown (past estimate)"
    );
    assert_eq!(
        estimating_line("◌", "Scanning", Duration::from_secs(72)),
        "◌ Scanning  01:12 elapsed  ETA estimating…"
    );
    assert_eq!(
        estimating_line("◌", "Starting…", Duration::ZERO),
        "◌ Starting…  00:00 elapsed  ETA estimating…"
    );
}

// ---- the app with its sources and the history file ----

#[path = "support/synthetic.rs"]
mod synthetic;

use std::cell::{Cell, RefCell};
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Condvar, Mutex};

use crossterm::event::Event;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::event_loop::{App, Clock, EventSource, ScanLaunch, Services};
use storage_analyzer::app::state::{AppState, ScanKind};
use storage_analyzer::eta::sources::InodeSource;
use storage_analyzer::history::{FileHistory, HistoryEntry, HistoryError, HistoryStore};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, Kind, NodeId};

#[derive(Clone, Default)]
struct TestClock(Rc<Cell<Duration>>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// No input; the test moves the clock itself.
struct Quiet;

impl EventSource for Quiet {
    fn next(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
        Ok(None)
    }
}

/// A store that counts calls and can hold `load` back until the test opens the gate.
struct Counting {
    inner: FileHistory,
    loads: AtomicUsize,
    saves: AtomicUsize,
    gate: Mutex<bool>,
    opened: Condvar,
}

impl Counting {
    fn new(dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            inner: FileHistory::new(dir.to_path_buf()),
            loads: AtomicUsize::new(0),
            saves: AtomicUsize::new(0),
            gate: Mutex::new(true),
            opened: Condvar::new(),
        })
    }

    fn gated(dir: &Path) -> Arc<Self> {
        let store = Self::new(dir);
        *store.gate.lock().unwrap() = false;
        store
    }

    fn open(&self) {
        *self.gate.lock().unwrap() = true;
        self.opened.notify_all();
    }

    fn calls(&self) -> (usize, usize) {
        (
            self.loads.load(Ordering::SeqCst),
            self.saves.load(Ordering::SeqCst),
        )
    }
}

impl HistoryStore for Counting {
    fn load(&self) -> Result<Vec<HistoryEntry>, HistoryError> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        let mut open = self.gate.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
        drop(open);
        self.inner.load()
    }

    fn save(&self, entries: &[HistoryEntry]) -> Result<(), HistoryError> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        self.inner.save(entries)
    }
}

struct FakeInodes(Option<u64>);

impl InodeSource for FakeInodes {
    fn used_inodes(&self, _root: &Path) -> Option<u64> {
        self.0
    }
}

struct Rig {
    app: App<TestBackend, Quiet, TestClock>,
    clock: TestClock,
    tx: Rc<RefCell<Option<SyncSender<ScanMsg>>>>,
}

fn rig(flags: &[&str], store: Arc<dyn HistoryStore>, inodes: Option<u64>) -> Rig {
    let argv = std::iter::once(APP_ID).chain(flags.iter().copied());
    let config = cli::parse(APP_ID, argv).unwrap();
    let state = AppState::new(config, synthetic::model());
    let clock = TestClock::default();
    let tx: Rc<RefCell<Option<SyncSender<ScanMsg>>>> = Rc::default();
    let slot = tx.clone();
    let starter: storage_analyzer::app::event_loop::ScanStarter =
        Box::new(move |launch: ScanLaunch| {
            *slot.borrow_mut() = Some(launch.tx);
            None
        });
    let app = App::new(
        Terminal::new(TestBackend::new(200, 24)).unwrap(),
        Quiet,
        clock.clone(),
        state,
        starter,
        Arc::new(AtomicBool::new(false)),
        storage_analyzer::app::terminal::PanicSlot::default(),
    )
    .with_services(Services {
        history: store,
        inodes: Box::new(FakeInodes(inodes)),
        delete: None,
    });
    let mut rig = Rig { app, clock, tx };
    rig.app.start().unwrap();
    rig
}

impl Rig {
    fn send(&self, msg: ScanMsg) {
        self.tx.borrow().as_ref().unwrap().send(msg).unwrap();
    }

    /// Moves the clock forward and runs one loop iteration.
    fn step(&mut self, ms: u64) {
        self.clock
            .0
            .set(self.clock.0.get() + Duration::from_millis(ms));
        self.app.step().unwrap();
    }

    fn status(&self) -> String {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, 22)].symbol().to_owned())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    /// Runs steps with short real sleeps until `done` holds, or fails after two seconds.
    fn until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let started = std::time::Instant::now();
        while !done(self) {
            assert!(
                started.elapsed() < Duration::from_secs(2),
                "timed out waiting for {what}: {}",
                self.status()
            );
            std::thread::sleep(Duration::from_millis(5));
            // The fake clock moves too, so the loop is allowed to redraw.
            self.step(100);
        }
    }
}

const DIRS: u32 = 10;
const FILES: u32 = 99;
/// Entries a full run of `scan_messages` produces: 10 folders plus 990 files.
const TOTAL: u64 = (DIRS + DIRS * FILES) as u64;

/// The root with `DIRS` folders, then one listing per folder, then `Done`.
fn scan_messages(done: Option<bool>) -> Vec<ScanMsg> {
    let top = (0..DIRS)
        .map(|i| synthetic::entry(&format!("d{i}"), Kind::Dir, 0))
        .collect();
    let mut msgs = vec![synthetic::listing(NodeId::ROOT, 1, top)];
    for i in 0..DIRS {
        let files = (0..FILES)
            .map(|f| synthetic::entry(&format!("f{f}"), Kind::File, 100))
            .collect();
        msgs.push(synthetic::listing(
            NodeId(1 + i),
            1 + DIRS + i * FILES,
            files,
        ));
    }
    msgs.extend(done.map(|cancelled| ScanMsg::Done { cancelled }));
    msgs
}

/// Sends every message, stepping the loop 250 ms after each.
fn run_scan(rig: &mut Rig, msgs: Vec<ScanMsg>) {
    for msg in msgs {
        rig.send(msg);
        rig.step(250);
    }
    rig.step(250);
}

fn temp_state_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join(APP_ID);
    (dir, state)
}

fn dir_is_empty(path: &Path) -> bool {
    !path.exists() || std::fs::read_dir(path).unwrap().next().is_none()
}

#[test]
fn without_history_nothing_is_read_or_written() {
    let (dir, state) = temp_state_dir();
    let store = Counting::new(&state);
    let mut rig = rig(&[], store.clone(), None);
    run_scan(&mut rig, scan_messages(Some(false)));
    std::thread::sleep(Duration::from_millis(50));
    rig.step(0);
    assert!(rig.status().starts_with("✓ Done in"), "{}", rig.status());
    assert_eq!(
        store.calls(),
        (0, 0),
        "no read and no write without --history"
    );
    assert!(dir_is_empty(dir.path()), "the state dir is untouched");
}

#[test]
fn with_history_a_complete_scan_is_written_atomically_and_the_next_run_uses_it() {
    let (dir, state) = temp_state_dir();
    let store = Counting::new(&state);
    let mut first = rig(&["--history"], store.clone(), None);
    run_scan(&mut first, scan_messages(Some(false)));
    first.until("the history write", |_| store.calls().1 == 1);
    let names: Vec<_> = std::fs::read_dir(&state)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["history.json"], "no temp file is left behind");
    let saved = FileHistory::new(state.clone()).load().unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(
        (saved[0].entries, saved[0].root.as_str()),
        (TOTAL, synthetic::ROOT_PATH)
    );

    // A second run of the same root reads it and shows an estimate from `last scan`.
    let mut second = rig(&["--history"], store.clone(), None);
    second.until("the history source", |r| {
        r.status().contains("ETA estimating") || r.status().contains("last scan")
    });
    let top = scan_messages(None);
    for (i, msg) in top.into_iter().enumerate() {
        second.send(msg);
        second.step(300);
        if i >= 6 {
            break;
        }
    }
    let status = second.status();
    assert!(
        status.contains("~") && status.contains("%") && status.contains("(estimate, last scan)"),
        "{status}"
    );
    drop(dir);
}

#[test]
fn the_history_file_keeps_fifty_roots() {
    let (_dir, state) = temp_state_dir();
    let seed: Vec<HistoryEntry> = (0..50)
        .map(|i| HistoryEntry {
            root: format!("/other{i}"),
            one_filesystem: true,
            entries: 5,
            dirs: 1,
            bytes: 1,
            duration_ms: 1,
        })
        .collect();
    FileHistory::new(state.clone()).save(&seed).unwrap();
    let store = Counting::new(&state);
    let mut rig = rig(&["--history"], store.clone(), None);
    run_scan(&mut rig, scan_messages(Some(false)));
    rig.until("the history write", |_| store.calls().1 == 1);
    let saved = FileHistory::new(state).load().unwrap();
    assert_eq!(saved.len(), 50);
    assert_eq!(saved.last().unwrap().root, synthetic::ROOT_PATH);
    assert_eq!(saved[0].root, "/other1", "the oldest was dropped");
}

#[test]
fn a_corrupt_history_file_is_ignored_with_a_message_and_not_overwritten_until_a_save() {
    let (_dir, state) = temp_state_dir();
    std::fs::create_dir_all(&state).unwrap();
    let file = state.join("history.json");
    std::fs::write(&file, b"{ broken").unwrap();
    let store = Counting::new(&state);
    let mut rig = rig(&["--history"], store.clone(), None);
    rig.until("the load", |_| store.calls().0 == 1);
    rig.until("the message", |r| {
        r.status().contains("History file ignored")
    });
    assert_eq!(std::fs::read(&file).unwrap(), b"{ broken");
    assert!(!rig.status().contains('%'));
    run_scan(&mut rig, scan_messages(Some(false)));
    rig.until("the write", |_| store.calls().1 == 1);
    assert_eq!(FileHistory::new(state).load().unwrap().len(), 1);
}

#[test]
fn a_cancelled_scan_or_one_with_user_skips_writes_nothing() {
    // (messages, label)
    let cancelled = (scan_messages(Some(true)), "cancelled");
    let mut skipped_entry = synthetic::entry("skipped", Kind::Dir, 0);
    skipped_entry.issue = Some((storage_analyzer::types::Op::Open, IssueKind::SkippedByUser));
    let skipped = (
        vec![
            synthetic::listing(NodeId::ROOT, 1, vec![skipped_entry]),
            ScanMsg::Done { cancelled: false },
        ],
        "user skip",
    );
    for (msgs, label) in [cancelled, skipped] {
        let (_dir, state) = temp_state_dir();
        let store = Counting::new(&state);
        let mut rig = rig(&["--history"], store.clone(), None);
        run_scan(&mut rig, msgs);
        std::thread::sleep(Duration::from_millis(50));
        rig.step(100);
        assert_eq!(store.calls().1, 0, "{label}: nothing written");
        assert!(
            rig.app.state().session_previous.is_none(),
            "{label}: no in-session record"
        );
    }
}

#[test]
fn a_late_history_read_moves_the_line_from_unknown_to_estimating_to_an_estimate() {
    let (_dir, state) = temp_state_dir();
    let seed = HistoryEntry {
        root: synthetic::ROOT_PATH.into(),
        one_filesystem: true,
        entries: TOTAL,
        dirs: 10,
        bytes: 1,
        duration_ms: 1,
    };
    FileHistory::new(state.clone()).save(&[seed]).unwrap();
    let store = Counting::gated(&state);
    let mut rig = rig(&["--history"], store.clone(), None);

    let kind = |s: &str| {
        if s.contains("ETA estimating") {
            'E'
        } else if s.contains("(estimate,") {
            'T'
        } else if s.contains("ETA unknown (no previous scan)") {
            'I'
        } else {
            '-'
        }
    };
    let mut seen = String::new();
    // Five quiet seconds on the fake clock (no entries yet): the read is still held back.
    for _ in 0..20 {
        rig.step(250);
        let s = rig.status();
        if kind(&s) == 'I' {
            assert!(!s.contains('%') && !s.contains("ETA ~"), "{s}");
        }
        seen.push(kind(&s));
    }
    assert!(seen.ends_with("III"), "{seen}");
    store.open();
    let mut feed = scan_messages(None).into_iter();
    rig.until("the history source", |r| kind(&r.status()) != 'I');
    seen.push(kind(&rig.status()));
    for _ in 0..16 {
        if let Some(msg) = feed.next() {
            rig.send(msg);
        }
        rig.step(250);
        seen.push(kind(&rig.status()));
    }
    let after_open: String = seen.chars().skip(20).collect();
    assert!(after_open.contains('E'), "estimating shows once: {seen}");
    assert!(after_open.contains('T'), "then a real estimate: {seen}");
    assert!(!after_open.contains('I'), "never back to unknown: {seen}");
    let first_t = after_open.find('T').unwrap();
    assert!(
        !after_open[first_t..].contains('E'),
        "no estimating after an estimate: {seen}"
    );
}

#[test]
fn the_inode_count_is_the_source_when_nothing_better_exists() {
    let (_dir, state) = temp_state_dir();
    let store = Counting::new(&state);
    let mut rig = rig(&[], store, Some(TOTAL));
    let mut msgs = scan_messages(None).into_iter();
    for _ in 0..14 {
        if let Some(m) = msgs.next() {
            rig.send(m);
        }
        rig.step(300);
    }
    let status = rig.status();
    assert!(
        status.contains("(estimate, filesystem)") && status.contains('~'),
        "{status}"
    );
}

#[test]
fn a_finished_root_scan_becomes_the_in_session_source_for_a_rescan() {
    let (_dir, state) = temp_state_dir();
    let mut first = rig(&[], Counting::new(&state), None);
    run_scan(&mut first, scan_messages(Some(false)));
    let record = first
        .app
        .state()
        .session_previous
        .clone()
        .expect("a record after a complete Done");
    assert_eq!(
        (record.entries, record.root.as_str()),
        (TOTAL, synthetic::ROOT_PATH)
    );

    // A rescan of the root in the same session starts with that total as its source.
    let mut again = rig(&[], Counting::new(&state), None);
    again.app.state_mut().session_previous = Some(record);
    again.app.state_mut().use_session_previous();
    let mut msgs = scan_messages(None).into_iter();
    for _ in 0..14 {
        if let Some(m) = msgs.next() {
            again.send(m);
        }
        again.step(300);
    }
    let status = again.status();
    assert!(status.contains("(estimate, last scan)"), "{status}");
}

#[test]
fn a_subtree_rescan_neither_uses_nor_updates_the_session_record() {
    let (_dir, state) = temp_state_dir();
    let mut rig = rig(&[], Counting::new(&state), None);
    rig.app.state_mut().scan_finished(ScanKind::Subtree);
    assert!(
        rig.app.state().session_previous.is_none(),
        "a subtree scan makes no record"
    );
    run_scan(&mut rig, scan_messages(Some(false)));
    // Mark the record so an overwrite would show.
    rig.app
        .state_mut()
        .session_previous
        .as_mut()
        .expect("a record")
        .entries = 1;
    let before = rig.app.state().session_previous.clone();
    assert!(before.is_some());
    rig.app.state_mut().scan_finished(ScanKind::Subtree);
    assert_eq!(
        rig.app.state().session_previous,
        before,
        "and does not change an existing one"
    );
    // A record for a different root or different options is not used.
    let mut other = before.unwrap();
    other.root = "/elsewhere".into();
    rig.app.state_mut().session_previous = Some(other);
    rig.app.state_mut().use_session_previous();
    assert_eq!(rig.app.state().eta_sources.session, None);
}

#[test]
fn with_no_source_the_line_carries_true_quantities_and_no_estimate() {
    let (_dir, state) = temp_state_dir();
    let mut rig = rig(&[], Counting::new(&state), None);
    let mut msgs = scan_messages(None).into_iter();
    // Stop while folders are still queued, so every field is there.
    for _ in 0..6 {
        if let Some(m) = msgs.next() {
            rig.send(m);
        }
        rig.step(500);
    }
    let status = rig.status();
    let model = &rig.app.state().model;
    let totals = model.totals();
    let (done, total) = model.top_level_progress();
    assert!(
        !status.contains('%') && !status.contains("ETA ~"),
        "{status}"
    );
    assert!(
        status.ends_with("ETA unknown (no previous scan)"),
        "{status}"
    );
    assert!(
        status.contains(&format!("{done} of {total} top-level done")),
        "{status}"
    );
    assert!(
        status.contains(&format!(
            "≥{} folders queued",
            format::count_compact(totals.pending_dirs)
        )),
        "{status}"
    );
    assert!(
        status.contains(&format!(
            "{} entries",
            format::count_compact(totals.entries_seen)
        )),
        "{status}"
    );
}

#[test]
fn the_estimate_always_carries_a_tilde_and_never_reads_one_hundred_before_done() {
    let (_dir, state) = temp_state_dir();
    let mut rig = rig(&[], Counting::new(&state), Some(TOTAL));
    let mut msgs = scan_messages(None).into_iter();
    let mut estimates = 0;
    for _ in 0..40 {
        if let Some(m) = msgs.next() {
            rig.send(m);
        }
        rig.step(300);
        let s = rig.status();
        if s.contains("ETA") && s.chars().any(|c| c.is_ascii_digit()) && s.contains("estimate,") {
            estimates += 1;
            assert!(s.contains("ETA ~") && s.contains("estimate"), "{s}");
            assert!(!s.contains("~100%"), "{s}");
        }
    }
    assert!(estimates > 0);
    rig.send(ScanMsg::Done { cancelled: false });
    rig.step(300);
    assert!(rig.status().starts_with("✓ Done in"), "{}", rig.status());
}

#[test]
fn the_spinner_moves_at_most_ten_times_a_second_and_stands_still_without_animation() {
    let (_dir, state) = temp_state_dir();
    let glyph_over_time = |flags: &[&str]| {
        let mut rig = rig(flags, Counting::new(&state), None);
        rig.send(scan_messages(None).remove(0));
        let mut glyphs = Vec::new();
        for _ in 0..40 {
            rig.step(50);
            glyphs.push(rig.status().chars().next().unwrap());
        }
        glyphs
    };
    let animated = glyph_over_time(&[]);
    let changes = animated.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(changes >= 3, "it moves: {animated:?}");
    assert!(changes <= 20, "at most 10 fps over 2 s: {changes}");
    let still = glyph_over_time(&["--no-animation"]);
    assert!(still.iter().all(|&c| c == '◌'), "{still:?}");
}
