//! Scan-control keys: priority, zoom, skip, resume/rescan, stop and the one-filesystem hint (T020).
//!
//! Most tests drive the real walker over `FakeFs` (one thread) behind a gate that releases one
//! directory read at a time, so the scan only moves when the test says so. A few feed messages by
//! hand and run without a walker.

#[path = "support/fake_fs.rs"]
mod fake_fs;
#[path = "support/synthetic.rs"]
mod synthetic;

use std::cell::{Cell, RefCell};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use fake_fs::FakeFs;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::event_loop::{
    App, Clock, EventSource, ScanLaunch, ScanStarter, Services,
};
use storage_analyzer::app::list_state::{RowKey, View};
use storage_analyzer::app::state::AppState;
use storage_analyzer::app::terminal::PanicSlot;
use storage_analyzer::history::{FileHistory, HistoryStore};
use storage_analyzer::scan::{self, Errno, FsAccess, ScanMsg, StatInfo};
use storage_analyzer::types::{IssueKind, Kind, NodeId, Op, ScanState};
use storage_analyzer::ui::tile_colors::Hue;
use synthetic::ROOT_PATH;

const ROOT: NodeId = NodeId::ROOT;

// ---- harness ----

/// Lets directory reads through one permit at a time.
#[derive(Default)]
struct Gate {
    permits: Mutex<u32>,
    changed: Condvar,
}

impl Gate {
    fn grant(&self, n: u32) {
        *self.permits.lock().unwrap() += n;
        self.changed.notify_all();
    }

    /// Drops the permits that are left, so the next directory read waits for the test.
    fn hold(&self) {
        *self.permits.lock().unwrap() = 0;
    }

    fn take(&self) {
        let mut permits = self.permits.lock().unwrap();
        let started = Instant::now();
        while *permits == 0 {
            let (guard, timeout) = self
                .changed
                .wait_timeout(permits, Duration::from_millis(50))
                .unwrap();
            permits = guard;
            if timeout.timed_out() && started.elapsed() > Duration::from_secs(20) {
                return;
            }
        }
        *permits -= 1;
    }
}

struct Gated {
    fs: Arc<FakeFs>,
    gate: Arc<Gate>,
}

impl FsAccess for Gated {
    type Dir = <FakeFs as FsAccess>::Dir;

    fn open_dir(&self, path: &Path) -> Result<Self::Dir, Errno> {
        self.gate.take();
        self.fs.open_dir(path)
    }

    fn read_entries(&self, dir: &mut Self::Dir) -> Result<Vec<Box<[u8]>>, Errno> {
        self.fs.read_entries(dir)
    }

    fn stat_at(&self, dir: &Self::Dir, name: &[u8]) -> Result<StatInfo, Errno> {
        self.fs.stat_at(dir, name)
    }
}

#[derive(Clone, Default)]
struct TestClock(Rc<Cell<Duration>>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

/// Keys the test has pressed and the app has not read yet.
#[derive(Clone, Default)]
struct Keys(Rc<RefCell<std::collections::VecDeque<Event>>>);

impl EventSource for Keys {
    fn next(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
        Ok(self.0.borrow_mut().pop_front())
    }
}

struct Rig {
    app: App<TestBackend, Keys, TestClock>,
    keys: Keys,
    clock: TestClock,
    gate: Arc<Gate>,
    fs: Option<Arc<FakeFs>>,
    /// The sender of a hand-fed scan.
    tx: Rc<RefCell<Option<SyncSender<ScanMsg>>>>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        // Workers parked on the gate must be able to finish before `finish` joins them.
        self.gate.grant(1_000_000);
    }
}

fn build(
    flags: &[&str],
    fs: Option<Arc<FakeFs>>,
    permits: u32,
    services: Services,
    model: Option<storage_analyzer::model::Model>,
) -> Rig {
    let argv = std::iter::once(APP_ID).chain(flags.iter().copied()).chain([
        "--threads",
        "1",
        "--no-animation",
    ]);
    let config = cli::parse(APP_ID, argv).unwrap();
    let state = AppState::new(config, model.unwrap_or_else(synthetic::model));
    let clock = TestClock::default();
    let gate = Arc::new(Gate::default());
    gate.grant(permits);
    let keys = Keys::default();
    let tx: Rc<RefCell<Option<SyncSender<ScanMsg>>>> = Rc::default();
    let starter: ScanStarter = {
        let (fs, gate, slot) = (fs.clone(), gate.clone(), tx.clone());
        Box::new(move |launch: ScanLaunch| match fs {
            Some(fs) => Some(scan::start(
                Arc::new(Gated { fs, gate }),
                launch.root,
                launch.root_dev,
                launch.options,
                launch.control,
                launch.tx,
                launch.first_id,
            )),
            None => {
                *slot.borrow_mut() = Some(launch.tx);
                None
            }
        })
    };
    let app = App::new(
        Terminal::new(TestBackend::new(200, 24)).unwrap(),
        keys.clone(),
        clock.clone(),
        state,
        starter,
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    )
    .with_services(services);
    let mut rig = Rig {
        app,
        keys,
        clock,
        gate,
        fs,
        tx,
    };
    rig.app.start().unwrap();
    rig
}

/// A real walker over `fs`, root listing allowed, nothing else.
fn walker(fs: FakeFs, flags: &[&str]) -> Rig {
    build(flags, Some(Arc::new(fs)), 0, Services::default(), None)
}

/// No walker: the test sends the messages.
fn manual(flags: &[&str]) -> Rig {
    build(flags, None, 0, Services::default(), None)
}

/// `/synthetic/root` with folders a, b and c, each holding one file.
fn three_folders() -> FakeFs {
    let fs = FakeFs::new(ROOT_PATH);
    // A rescan of the root re-reads its entry through the parent directory.
    fs.dir("/synthetic");
    for name in ["a", "b", "c"] {
        fs.dir(&format!("{ROOT_PATH}/{name}"));
        fs.file(&format!("{ROOT_PATH}/{name}/f.bin"), 4096, 1000);
    }
    fs
}

impl Rig {
    fn state(&self) -> &AppState {
        self.app.state()
    }

    fn st(&mut self) -> &mut AppState {
        self.app.state_mut()
    }

    fn step(&mut self, ms: u64) {
        self.clock
            .0
            .set(self.clock.0.get() + Duration::from_millis(ms));
        self.app.step().unwrap();
    }

    fn key(&mut self, code: KeyCode) {
        self.keys
            .0
            .borrow_mut()
            .push_back(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        self.step(100);
    }

    fn keys(&mut self, codes: &[KeyCode]) {
        for &code in codes {
            self.key(code);
        }
    }

    fn status(&self) -> String {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.width)
            .map(|x| buf[(x, 22)].symbol().to_owned())
            .collect::<String>()
            .trim_end()
            .to_owned()
    }

    fn screen(&self) -> Vec<String> {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect()
    }

    fn row_with(&self, needle: &str) -> String {
        self.screen()
            .into_iter()
            .find(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle:?}:\n{}", self.screen().join("\n")))
    }

    fn until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let started = Instant::now();
        while !done(self) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}:\n{}",
                self.screen().join("\n")
            );
            std::thread::sleep(Duration::from_millis(3));
            self.step(100);
        }
    }

    fn find(&self, rel: &str) -> NodeId {
        let model = &self.state().model;
        let mut cur = ROOT;
        for part in rel.split('/') {
            cur = *model
                .children(cur)
                .iter()
                .find(|&&c| model.exists(c) && model.name(c) == part.as_bytes())
                .unwrap_or_else(|| panic!("no {rel}"));
        }
        cur
    }

    fn path(&self, rel: &str) -> PathBuf {
        PathBuf::from(format!("{ROOT_PATH}/{rel}"))
    }

    /// Puts the tree cursor on `id`.
    fn select(&mut self, id: NodeId) {
        let st = self.st();
        st.tree.selection = id;
        st.tree.aggregate = None;
        st.tree.on_parent = false;
        st.tree.expanded.insert(ROOT);
        self.step(100);
    }

    fn send(&self, msg: ScanMsg) {
        self.tx
            .borrow()
            .as_ref()
            .expect("hand-fed rig")
            .send(msg)
            .unwrap();
    }

    fn opens(&self, rel: &str) -> usize {
        let wanted = format!("open {}", self.path(rel).display());
        self.fs
            .as_ref()
            .unwrap()
            .calls()
            .iter()
            .filter(|c| **c == wanted)
            .count()
    }

    /// Lets the map relayout (the loop would on its next draw).
    fn relayout(&mut self) {
        self.step(1000);
        self.st()
            .prepare_view(ratatui::layout::Rect::new(0, 0, 200, 24));
    }

    fn scan_done(&self) -> bool {
        self.state().phase() == ScanState::Done
    }
}

fn entry(name: &str, kind: Kind, bytes: u64) -> storage_analyzer::scan::RawEntry {
    synthetic::entry(name, kind, bytes)
}

fn skipped(name: &str) -> storage_analyzer::scan::RawEntry {
    let mut e = entry(name, Kind::Dir, 4096);
    e.issue = Some((Op::Read, IssueKind::SkippedByUser));
    e
}

// ---- priority, zoom (UX §11 check 9) and M11 ----

#[test]
fn zoom_sets_the_focus_going_back_to_the_root_clears_it_and_p_toggles() {
    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    let a = r.find("a");
    r.select(a);
    r.key(KeyCode::Enter);
    assert_eq!(
        r.state().control.focus(),
        Some(r.path("a")),
        "⏎ prioritises the folder"
    );
    assert_eq!(r.state().priority, Some(a));
    r.key(KeyCode::Backspace);
    assert_eq!(
        r.state().control.focus(),
        None,
        "back at the root clears it"
    );
    assert_eq!(r.state().priority, None);

    r.select(a);
    r.key(KeyCode::Char('p'));
    assert_eq!(r.state().control.focus(), Some(r.path("a")));
    assert_eq!(r.state().priority, Some(a));
    assert!(
        r.status().contains("▲ Scanning a/ first."),
        "{}",
        r.status()
    );
    let c = r.find("c");
    r.select(c);
    r.key(KeyCode::Char('p'));
    assert_eq!(
        r.state().priority,
        Some(c),
        "a new p replaces the old priority"
    );
    r.key(KeyCode::Char('p'));
    assert_eq!(r.state().priority, None, "p again clears it");
    assert_eq!(r.state().control.focus(), None);
    assert!(r.status().contains("Priority cleared."), "{}", r.status());
}

#[test]
fn the_priority_folder_is_scanned_before_the_others_and_announces_when_done() {
    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    let (a, b, c) = (r.find("a"), r.find("b"), r.find("c"));
    // The worker is already waiting to read `a`; prioritise `c`, which would come last.
    r.select(c);
    r.key(KeyCode::Char('p'));
    r.gate.grant(2);
    r.until("the priority folder", |r| r.state().model.is_complete(c));
    let model = &r.state().model;
    assert!(model.is_complete(a), "the read already under way finished");
    assert!(!model.is_complete(b), "c went before b");
    r.until("the message", |r| r.status().contains("fully scanned"));
    let status = r.status();
    assert!(status.starts_with("✓ c/ fully scanned: "), "{status}");
    assert!(status.contains("1 files"), "{status}");
    assert_eq!(r.state().priority, None, "▲ is cleared");
}

#[test]
fn while_a_priority_is_set_the_status_line_starts_with_the_first_prefix() {
    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    let b = r.find("b");
    r.select(b);
    r.key(KeyCode::Char('p'));
    // The transient message passes after five seconds; then the state text carries the prefix.
    r.step(6000);
    r.step(100);
    let status = r.status();
    assert!(status.starts_with("▲ First: b/"), "{status}");
    assert!(status.contains("Scanning"), "{status}");
}

// ---- skip and resume (UX §11 check 10) ----

#[test]
fn x_skips_a_folder_and_r_brings_back_the_full_totals() {
    let mut baseline = walker(three_folders(), &[]);
    baseline.gate.grant(100);
    baseline.until("the baseline scan", |r| r.scan_done());
    let expected = baseline.state().model.totals();

    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    // The single worker is already waiting to read `a`, so skip `c`, which has not been started.
    let c = r.find("c");
    r.select(c);
    r.key(KeyCode::Char('x'));
    assert!(
        r.status()
            .contains("⊘ Skipped c/. Its size is partial (≥). r resumes."),
        "{}",
        r.status()
    );
    assert!(r.state().control.is_cancelled(&r.path("c")));
    r.gate.grant(100);
    r.until("the scan to finish", |r| r.scan_done());

    let row = r.row_with("▸ c/");
    assert!(
        row.contains('⊘') && row.contains("(scan skipped)") && row.contains('≥'),
        "{row}"
    );
    assert!(
        r.row_with("100.0").contains('≥'),
        "every ancestor shows ≥: {}",
        r.row_with("100.0")
    );
    let skipped_totals = r.state().model.totals();
    assert!(
        skipped_totals.files < expected.files,
        "nothing under c/ was scanned"
    );
    assert_eq!(r.opens("c"), 0, "no listing for the skipped subtree");

    r.select(c);
    r.key(KeyCode::Char('r'));
    assert!(r.status().contains("◌ Resuming c/…"), "{}", r.status());
    r.until("the rescan to finish", |r| {
        r.scan_done() && !r.state().rescan_in_flight
    });
    let row = r.row_with("▸ c/");
    assert!(
        !row.contains('⊘') && !row.contains("(scan skipped)") && !row.contains('≥'),
        "{row}"
    );
    assert!(!r.row_with("100.0").contains('≥'));
    let totals = r.state().model.totals();
    assert_eq!(
        (totals.files, totals.dirs, totals.disk_bytes),
        (expected.files, expected.dirs, expected.disk_bytes)
    );
}

#[test]
fn x_on_a_skipped_or_a_finished_folder_says_so_and_changes_nothing() {
    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    let (a, b) = (r.find("a"), r.find("b"));
    r.select(a);
    r.key(KeyCode::Char('x'));
    r.key(KeyCode::Char('x'));
    assert!(
        r.status().contains("Already skipped. r resumes."),
        "{}",
        r.status()
    );
    assert_eq!(
        r.state().model.issues().len(),
        1,
        "the model was not asked again"
    );
    r.gate.grant(100);
    r.until("the scan to finish", |r| r.scan_done());
    r.select(b);
    r.key(KeyCode::Char('x'));
    assert!(
        r.status().contains("Already fully scanned."),
        "{}",
        r.status()
    );
    assert!(!r.state().control.is_cancelled(&r.path("b")));
}

#[test]
fn p_x_and_r_on_a_file_or_an_aggregate_row_say_they_work_on_folders() {
    let mut r = manual(&[]);
    let mut items = vec![entry("big.bin", Kind::File, 5000)];
    items.extend((0..60).map(|i| entry(&format!("d{i:02}"), Kind::Dir, 100 + i)));
    r.send(synthetic::listing(ROOT, 1, items));
    r.until("the listing", |r| {
        r.state().model.children(ROOT).len() == 61
    });
    let file = r.find("big.bin");
    for key in ['p', 'x', 'r'] {
        r.select(file);
        r.key(KeyCode::Char(key));
        assert!(
            r.status().contains("p, x and r work on folders."),
            "{key}: {}",
            r.status()
        );
    }
    assert_eq!(r.state().control.focus(), None);
    // The aggregate row of the tree.
    r.st().tree.aggregate = Some(ROOT);
    r.key(KeyCode::Char('p'));
    assert!(
        r.status().contains("p, x and r work on folders."),
        "{}",
        r.status()
    );
}

// ---- stop and the Stopping state ----

fn started_scan() -> Rig {
    let mut r = walker(three_folders(), &[]);
    r.gate.grant(1);
    r.until("the root listing", |r| {
        r.state().model.children(ROOT).len() == 3
    });
    r
}

#[test]
fn s_stops_through_a_frozen_stopping_line_to_stopped_and_r_on_the_root_rescans() {
    let mut baseline = walker(three_folders(), &[]);
    baseline.gate.grant(100);
    baseline.until("the baseline scan", |r| r.scan_done());
    let expected = baseline.state().model.totals();

    let mut r = started_scan();
    r.step(72_000);
    r.key(KeyCode::Char('s'));
    assert!(r.state().control.is_cancel_all());
    assert!(
        r.status().starts_with("■ Stopping…  01:1"),
        "{}",
        r.status()
    );
    let frozen = r.status();
    r.step(3000);
    r.step(3000);
    assert_eq!(
        r.status(),
        frozen,
        "elapsed is frozen while stopping, and there is no ETA"
    );
    assert!(!r.status().contains("ETA"));

    r.gate.grant(100);
    r.until("Stopped", |r| r.state().model.state() == ScanState::Stopped);
    let status = r.status();
    assert!(
        status.starts_with("■ Stopped at 01:1") && status.contains("partial results"),
        "{status}"
    );

    // Hold the gate: the rescan must not finish before the assertions look at it.
    r.gate.hold();
    r.select(ROOT);
    r.key(KeyCode::Char('r'));
    assert!(r.state().rescan_in_flight);
    assert!(
        !r.state().control.is_cancel_all(),
        "the rescan cleared cancel-all"
    );
    r.gate.grant(100);
    r.until("the rescan to finish", |r| {
        r.scan_done() && !r.state().rescan_in_flight
    });
    let totals = r.state().model.totals();
    assert_eq!(
        (totals.files, totals.dirs, totals.disk_bytes),
        (expected.files, expected.dirs, expected.disk_bytes)
    );
}

#[test]
fn while_stopping_r_p_and_x_only_show_a_message_and_are_not_replayed() {
    let mut r = started_scan();
    let a = r.find("a");
    r.key(KeyCode::Char('s'));
    assert!(r.status().contains("Stopping…"));
    let message = "Still stopping the scan. Try again when it says Stopped.";
    r.select(a);
    for key in ['r', 'p', 'x'] {
        r.key(KeyCode::Char(key));
        assert!(r.status().contains(message), "{key}: {}", r.status());
    }
    assert!(r.state().control.is_cancel_all(), "no rescan reset it");
    assert_eq!(r.state().control.focus(), None);
    assert!(
        !r.state().control.is_cancelled(&r.path("a")),
        "nothing was skipped"
    );
    assert_eq!(r.state().priority, None);
    let before = r.state().stop_elapsed;
    r.key(KeyCode::Char('s'));
    assert_eq!(r.state().stop_elapsed, before, "a second s does nothing");

    // Navigation and views work.
    r.key(KeyCode::Down);
    r.key(KeyCode::Char('2'));
    assert_eq!(r.state().view, View::Types);
    r.key(KeyCode::Char('1'));

    r.gate.grant(100);
    r.until("Stopped", |r| r.state().model.state() == ScanState::Stopped);
    let opens = r.fs.as_ref().unwrap().calls().len();
    r.step(6000);
    r.step(100);
    assert!(
        !r.status().contains("Resuming") && !r.state().rescan_in_flight,
        "{}",
        r.status()
    );
    assert_eq!(
        r.fs.as_ref().unwrap().calls().len(),
        opens,
        "the keys were not held for later"
    );
    assert_eq!(r.state().priority, None);
}

#[test]
fn r_after_a_stop_is_a_full_rescan_at_once() {
    let mut r = started_scan();
    let a = r.find("a");
    r.key(KeyCode::Char('s'));
    r.gate.grant(100);
    r.until("Stopped", |r| r.state().model.state() == ScanState::Stopped);
    // Hold the gate: the rescan must not finish before the assertions look at it.
    r.gate.hold();
    r.select(a);
    r.key(KeyCode::Char('r'));
    assert!(r.state().rescan_in_flight, "{}", r.status());
    assert!(r.status().contains("Resuming a/"), "{}", r.status());
    r.gate.grant(100);
    r.until("the rescan to finish", |r| !r.state().rescan_in_flight);
    assert!(r.state().model.is_complete(a));
}

#[test]
fn an_internal_rescan_asked_for_while_stopping_waits_and_runs_once_after_done() {
    let mut r = started_scan();
    // `a` is already being read by the single worker; `c` has not been started.
    let c = r.find("c");
    r.key(KeyCode::Char('s'));
    r.st().rescan(c);
    assert_eq!(r.state().queued_rescans, vec![c], "queued, not started");
    assert!(r.state().control.is_cancel_all());
    assert_eq!(r.opens("c"), 0);
    r.gate.grant(100);
    r.until("the queued rescan to finish", |r| {
        r.state().queued_rescans.is_empty() && !r.state().rescan_in_flight && r.opens("c") >= 1
    });
    assert_eq!(r.opens("c"), 1, "exactly one rescan of c");
    assert!(r.state().model.is_complete(c));
}

// ---- r on a folder (plan handling) ----

/// Root with X (still scanning, holding two skipped folders and one pending one) and Y.
fn plan_scene(skips_in_x: usize) -> (Rig, NodeId, Vec<NodeId>) {
    let mut r = manual(&[]);
    r.send(synthetic::listing(
        ROOT,
        1,
        vec![entry("x", Kind::Dir, 4096), entry("y", Kind::Dir, 4096)],
    ));
    let mut items = vec![entry("pending", Kind::Dir, 4096)];
    items.extend((0..skips_in_x).map(|i| skipped(&format!("s{i}"))));
    r.send(synthetic::listing(NodeId(1), 10, items));
    r.until("the listings", |r| {
        r.state().model.exists(NodeId(1))
            && r.state().model.children(NodeId(1)).len() == 1 + skips_in_x
    });
    let x = r.find("x");
    let skips = (0..skips_in_x)
        .map(|i| r.find(&format!("x/s{i}")))
        .collect();
    (r, x, skips)
}

#[test]
fn r_on_an_unfinished_folder_moves_it_to_the_front_instead_of_restarting() {
    let (mut r, x, _) = plan_scene(0);
    r.select(x);
    r.key(KeyCode::Char('r'));
    assert_eq!(r.state().control.focus(), Some(r.path("x")));
    assert_eq!(r.state().priority, Some(x));
    assert!(
        r.status()
            .contains("Still scanning x/: moved it to the front instead of restarting."),
        "{}",
        r.status()
    );
    assert!(
        !r.state().rescan_in_flight,
        "the folder itself was not rescanned"
    );
    r.key(KeyCode::Char('r'));
    assert!(
        r.status().contains("Already scanning x/ first."),
        "{}",
        r.status()
    );
    assert_eq!(
        r.state().priority,
        Some(x),
        "r never toggles the priority off"
    );
}

#[test]
fn r_on_an_unfinished_folder_also_resumes_the_skipped_folders_inside() {
    for (skips, text) in [
        (
            1,
            "moved it to the front and resumed 1 skipped folder inside.",
        ),
        (
            2,
            "moved it to the front and resumed 2 skipped folders inside.",
        ),
    ] {
        let (mut r, x, skip_ids) = plan_scene(skips);
        r.select(x);
        r.key(KeyCode::Char('r'));
        assert!(r.status().contains(text), "{skips}: {}", r.status());
        for s in skip_ids {
            assert!(r.state().model.issue(s).is_none(), "the skip was cleared");
        }
    }
}

#[test]
fn a_skip_nested_inside_another_skip_is_not_counted() {
    let mut r = manual(&[]);
    r.send(synthetic::listing(
        ROOT,
        1,
        vec![entry("x", Kind::Dir, 4096)],
    ));
    r.send(synthetic::listing(
        NodeId(1),
        10,
        vec![
            entry("outer", Kind::Dir, 4096),
            entry("pending", Kind::Dir, 4096),
        ],
    ));
    r.send(synthetic::listing(
        NodeId(10),
        20,
        vec![entry("inner", Kind::Dir, 4096)],
    ));
    r.until("the listings", |r| {
        r.state().model.exists(NodeId(10)) && r.state().model.children(NodeId(10)).len() == 1
    });
    let (x, outer, inner) = (r.find("x"), r.find("x/outer"), r.find("x/outer/inner"));
    assert!(r.st().model.mark_skipped(inner));
    assert!(r.st().model.mark_skipped(outer));
    r.select(x);
    r.key(KeyCode::Char('r'));
    assert!(
        r.status().contains("resumed 1 skipped folder inside."),
        "{}",
        r.status()
    );
}

#[test]
fn r_on_the_root_while_scanning_changes_nothing() {
    let (mut r, _, _) = plan_scene(1);
    r.select(ROOT);
    r.key(KeyCode::Char('r'));
    assert!(
        r.status()
            .contains("Scan in progress. Press s to stop, then r to rescan."),
        "{}",
        r.status()
    );
    assert!(!r.state().rescan_in_flight);
    assert_eq!(r.state().control.focus(), None);
    assert_eq!(r.state().model.issues().len(), 1, "the skip is untouched");
}

#[test]
fn r_on_a_skipped_row_in_the_skipped_view_rescans_that_node() {
    let (mut r, x, skip_ids) = plan_scene(1);
    let _ = x;
    r.send(ScanMsg::Done { cancelled: false });
    r.until("Done", |r| r.state().model.state() == ScanState::Done);
    r.key(KeyCode::Char('4'));
    r.key(KeyCode::Char('r'));
    assert!(r.status().contains("Resuming s0/"), "{}", r.status());
    assert!(r.state().model.issue(skip_ids[0]).is_none());
}

// ---- rescans: reserved colours, root restart, history ----

#[test]
fn a_rescan_forgets_the_folders_reserved_file_type_hues() {
    let mut r = manual(&[]);
    r.send(synthetic::listing(
        ROOT,
        1,
        vec![entry("x", Kind::Dir, 4096)],
    ));
    r.send(synthetic::listing(
        NodeId(1),
        10,
        vec![
            entry("clip.mp4", Kind::File, 9000),
            entry("one", Kind::Dir, 3000),
            entry("two", Kind::Dir, 2000),
        ],
    ));
    r.until("the listings", |r| {
        r.state().model.exists(NodeId(1)) && r.state().model.children(NodeId(1)).len() == 3
    });
    let (x, one) = (r.find("x"), r.find("x/one"));
    r.st().tree.expanded.insert(x);
    r.select(one);
    r.relayout();
    assert!(
        r.state().tile_colors.reserved[&x].contains(Hue::Orange),
        "the video file reserves orange"
    );

    r.st().rescan(x);
    assert!(
        !r.state().tile_colors.reserved.contains_key(&x),
        "the rescan dropped the set"
    );
    r.send(synthetic::listing(
        x,
        100,
        vec![entry("one", Kind::Dir, 3000), entry("two", Kind::Dir, 2000)],
    ));
    r.until("the new listing", |r| {
        r.state().model.children(x).len() == 2
    });
    let new_one = r.find("x/one");
    r.st().tree.expanded.insert(x);
    r.select(new_one);
    r.relayout();
    assert!(
        !r.state().tile_colors.reserved[&x].contains(Hue::Orange),
        "after the video is gone, orange is free again"
    );
}

fn scan_stream(base: u32) -> Vec<ScanMsg> {
    let top = (0..10)
        .map(|i| entry(&format!("d{i}"), Kind::Dir, 0))
        .collect();
    let mut msgs = vec![synthetic::listing(ROOT, base, top)];
    for i in 0..10u32 {
        let files = (0..99)
            .map(|f| entry(&format!("f{f}"), Kind::File, 100))
            .collect();
        msgs.push(synthetic::listing(
            NodeId(base + i),
            base + 10 + i * 99,
            files,
        ));
    }
    msgs
}

#[test]
fn r_on_the_root_after_done_uses_the_previous_total_and_updates_the_records() {
    let dir = tempfile::tempdir().unwrap();
    let store: Arc<dyn HistoryStore> = Arc::new(FileHistory::new(dir.path().to_path_buf()));
    let services = Services {
        history: store.clone(),
        ..Services::default()
    };
    let mut r = build(&["--history"], None, 0, services, None);
    for msg in scan_stream(1) {
        r.send(msg);
        r.step(250);
    }
    r.send(ScanMsg::Done { cancelled: false });
    r.step(250);
    r.step(250);
    assert!(r.status().starts_with("✓ Done in"), "{}", r.status());
    let first = r
        .state()
        .session_previous
        .clone()
        .expect("a record after Done");
    assert_eq!(first.entries, 1000);

    // Restart the root scan.
    r.select(ROOT);
    r.key(KeyCode::Char('r'));
    assert!(r.state().rescan_in_flight);
    r.st().transient = None;
    let mut seen_estimate = false;
    let mut next = 2000;
    for msg in scan_stream(next) {
        r.send(msg);
        r.step(300);
        seen_estimate |= r.status().contains("(estimate, last scan)");
    }
    next += 0;
    let _ = next;
    assert!(
        seen_estimate,
        "the previous total is the ETA source: {}",
        r.status()
    );
    r.send(ScanMsg::Done { cancelled: false });
    r.step(300);
    r.step(300);
    assert!(r.status().starts_with("✓ Done in"), "{}", r.status());
    assert!(!r.state().rescan_in_flight);
    let second = r.state().session_previous.clone().unwrap();
    assert_eq!(
        second.entries, 1000,
        "the new scan's own entries, not the running count"
    );
    let saved = store.load().unwrap();
    assert_eq!(
        saved.len(),
        1,
        "the history entry was replaced, not duplicated"
    );
    assert_eq!(saved[0].entries, 1000);
}

// ---- the Skipped list is keyed by node ----

#[test]
fn a_rescan_that_removes_an_earlier_issue_keeps_the_cursor_on_its_row() {
    let mut r = manual(&[]);
    r.send(synthetic::listing(
        ROOT,
        1,
        vec![entry("x", Kind::Dir, 4096)],
    ));
    r.send(synthetic::listing(
        NodeId(1),
        10,
        vec![skipped("a"), skipped("b"), skipped("c")],
    ));
    r.until("the listings", |r| r.state().model.issues().len() == 3);
    let (b, c) = (r.find("x/b"), r.find("x/c"));
    r.key(KeyCode::Char('4'));
    r.step(100);
    let cursor = |r: &Rig| {
        r.state()
            .lists
            .cursor(View::Skipped)
            .and_then(|c| c.selected().cloned())
    };
    assert_eq!(
        cursor(&r),
        Some(RowKey::Issue(c)),
        "newest first: c is the top row"
    );

    r.st().rescan(b);
    r.key(KeyCode::Char('z'));
    assert_eq!(r.state().model.issues().len(), 2, "b's issue is gone");
    assert_eq!(
        cursor(&r),
        Some(RowKey::Issue(c)),
        "removing b does not move the cursor"
    );
    let screen = r.screen().join("\n");
    assert!(
        screen.contains("/synthetic/root/x/c")
            && screen.contains("/synthetic/root/x/a")
            && !screen.contains("/synthetic/root/x/b"),
        "{screen}"
    );
}

// ---- the one-filesystem hint ----

fn hint_scene(paths: &[&str], flags: &[&str]) -> Rig {
    let mut r = manual(flags);
    // Folders "home" (depth 1), "mnt" (depth 1) and "deep/er" (depth 2) are other filesystems.
    let mut root_items = vec![
        entry("deep", Kind::Dir, 4096),
        entry("keep", Kind::File, 10),
    ];
    for p in paths.iter().filter(|p| !p.contains('/')) {
        let mut e = entry(p, Kind::Dir, 0);
        e.issue = Some((Op::Open, IssueKind::OtherFilesystem));
        root_items.push(e);
    }
    r.send(synthetic::listing(ROOT, 1, root_items));
    if paths.iter().any(|p| p.contains('/')) {
        let mut e = entry("er", Kind::Dir, 0);
        e.issue = Some((Op::Open, IssueKind::OtherFilesystem));
        r.send(synthetic::listing(NodeId(1), 50, vec![e]));
    }
    r.until("the listings", |r| {
        r.state().model.issues().len() == paths.len()
    });
    r
}

#[test]
fn one_other_filesystem_is_named_in_the_status_line() {
    let r = hint_scene(&["home"], &[]);
    assert!(
        r.status()
            .ends_with(" · not scanned (other fs): /synthetic/root/home"),
        "{}",
        r.status()
    );
}

#[test]
fn several_name_the_shallowest_and_count_the_rest() {
    let r = hint_scene(&["mnt", "deep/er", "home"], &[]);
    let status = r.status();
    // The shallowest, first by name: /synthetic/root/home beats /synthetic/root/mnt.
    assert!(
        status.ends_with(" · not scanned (other fs): /synthetic/root/home +2"),
        "{status}"
    );
}

#[test]
fn a_narrow_line_drops_the_bar_first_and_then_shortens_the_hint() {
    use storage_analyzer::ui::status::fit_hint;
    let full = " · not scanned (other fs): /a/very/long/path/name/that/is/long";
    let short = " · 3 other fs not scanned";
    let line = |room: usize| {
        if room >= 80 {
            "◌ Scanning ████████░░░░░░░░ ~50%  01:12 elapsed  ETA ~01:44 (estimate, last scan)"
                .to_owned()
        } else {
            "◌ Scanning ~50% · 01:12  ETA ~01:44 (estimate, last scan)".to_owned()
        }
    };
    let wide = fit_hint(&line, full, short, 200);
    assert!(wide.contains('█') && wide.ends_with("/long"), "{wide}");
    let no_bar = fit_hint(&line, full, short, 120);
    assert!(
        !no_bar.contains('█') && no_bar.ends_with("/long"),
        "the bar goes first: {no_bar}"
    );
    let shortened = fit_hint(&line, full, short, 80);
    assert!(
        shortened.ends_with(" · 3 other fs not scanned"),
        "{shortened}"
    );
    assert!(shortened.contains("~50%"));
}

#[test]
fn the_hint_shows_in_every_state_and_never_with_cross_filesystems() {
    let mut r = hint_scene(&["home"], &[]);
    let suffix = " · not scanned (other fs): /synthetic/root/home";
    assert!(r.status().ends_with(suffix), "scanning: {}", r.status());
    r.send(ScanMsg::Done { cancelled: false });
    r.until("Done", |r| r.state().model.state() == ScanState::Done);
    assert!(
        r.status().starts_with("✓ Done in") && r.status().ends_with(suffix),
        "{}",
        r.status()
    );
    let mut stopped = hint_scene(&["home"], &[]);
    stopped.send(ScanMsg::Done { cancelled: true });
    stopped.until("Stopped", |r| r.state().model.state() == ScanState::Stopped);
    assert!(
        stopped.status().starts_with("■ Stopped") && stopped.status().ends_with(suffix),
        "{}",
        stopped.status()
    );

    let starting = {
        let mut r = manual(&[]);
        let mut e = entry("home", Kind::Dir, 0);
        e.issue = Some((Op::Open, IssueKind::OtherFilesystem));
        r.send(synthetic::listing(ROOT, 1, vec![e]));
        r.until("issue", |r| r.state().model.issues().len() == 1);
        r
    };
    assert!(starting.status().ends_with(suffix), "{}", starting.status());

    let cross = hint_scene(&["home"], &["--cross-filesystems"]);
    assert!(
        !cross.status().contains("not scanned"),
        "{}",
        cross.status()
    );
}

#[test]
fn the_wording_lives_in_one_template_constant_in_copy() {
    assert!(storage_analyzer::ui::copy::OTHER_FS_HINT.contains("not scanned (other fs):"));
    assert!(storage_analyzer::ui::copy::OTHER_FS_HINT_SHORT.contains("other fs not scanned"));
}

// ---- the `..` interception stays first ----

#[test]
fn p_x_and_r_on_the_parent_row_still_only_show_the_parent_message() {
    let mut r = manual(&[]);
    r.send(synthetic::listing(
        ROOT,
        1,
        vec![entry("x", Kind::Dir, 4096)],
    ));
    r.send(synthetic::listing(
        NodeId(1),
        10,
        vec![entry("pending", Kind::Dir, 4096)],
    ));
    r.until("the listings", |r| {
        r.state().model.exists(NodeId(1)) && r.state().model.children(NodeId(1)).len() == 1
    });
    let x = r.find("x");
    r.select(x);
    r.keys(&[KeyCode::Enter, KeyCode::Up]);
    assert!(r.state().tree.on_parent);
    for key in ['p', 'x', 'r', 's'] {
        r.key(KeyCode::Char(key));
        if key != 's' {
            assert!(
                r.status().contains("\"..\" is the parent folder"),
                "{key}: {}",
                r.status()
            );
        }
    }
    assert_eq!(
        r.state().priority,
        Some(x),
        "zoom prioritised x; p on .. changed nothing"
    );
    assert!(!r.state().control.is_cancelled(&r.path("x")));
}
