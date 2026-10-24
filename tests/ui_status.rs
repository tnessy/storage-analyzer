//! The disk read speed in the status line (T036a, UX §5.2 and check 23) through the whole app
//! loop, with a fake clock and a fake byte source.

#[path = "support/synthetic.rs"]
mod synthetic;

use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crossterm::event::Event;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::event_loop::{
    App, Clock, EventSource, ScanLaunch, ScanStarter, Services,
};
use storage_analyzer::app::state::AppState;
use storage_analyzer::app::terminal::PanicSlot;
use storage_analyzer::diskio::ReadBytesSource;
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::ui::status;

#[derive(Clone, Default)]
struct TestClock(Rc<Cell<Duration>>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

struct NoKeys;

impl EventSource for NoKeys {
    fn next(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
        Ok(None)
    }
}

/// A byte source the test drives: `answer` is what the next reading returns.
#[derive(Clone, Default)]
struct FakeSource {
    answer: Rc<Cell<Option<u64>>>,
    calls: Rc<Cell<u32>>,
}

impl ReadBytesSource for FakeSource {
    fn read_bytes(&mut self) -> Option<u64> {
        self.calls.set(self.calls.get() + 1);
        self.answer.get()
    }
}

struct Rig {
    app: App<TestBackend, NoKeys, TestClock>,
    clock: TestClock,
    source: FakeSource,
}

/// The root is listed (so the scan is `Scanning`, not `Starting`) and then nothing more comes.
fn idle_starter() -> ScanStarter {
    Box::new(move |launch: ScanLaunch| {
        launch
            .tx
            .send(synthetic::listing(
                storage_analyzer::types::NodeId::ROOT,
                1,
                vec![synthetic::entry(
                    "a",
                    storage_analyzer::types::Kind::Dir,
                    4096,
                )],
            ))
            .unwrap();
        std::mem::forget(launch.tx);
        None
    })
}

fn rig() -> Rig {
    let clock = TestClock::default();
    let source = FakeSource::default();
    let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
    let mut app = App::new(
        Terminal::new(TestBackend::new(80, 24)).unwrap(),
        NoKeys,
        clock.clone(),
        AppState::new(config, synthetic::model()),
        idle_starter(),
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    )
    .with_services(Services {
        read_bytes: Box::new(source.clone()),
        ..Services::default()
    });
    app.start().unwrap();
    Rig { app, clock, source }
}

impl Rig {
    /// Moves the clock to `ms`, sets what the source will answer, runs one loop step.
    fn at(&mut self, ms: u64, answer: Option<u64>) {
        self.clock.0.set(Duration::from_millis(ms));
        self.source.answer.set(answer);
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
}

/// 64 KiB per 100 ms, i.e. 640 KiB/s.
fn steady(ms: u64) -> Option<u64> {
    Some(ms * 655)
}

#[test]
fn hidden_until_the_first_full_second_then_shown() {
    let mut r = rig();
    for ms in (0..=4_000).step_by(100) {
        r.at(ms, steady(ms));
        let status = r.status();
        // The first reading only opens a window; the first value exists after a full second,
        // and the Indeterminate line itself appears once 2 s have passed.
        if ms < 1_000 {
            assert!(!status.contains("disk"), "{ms} ms: {status}");
        }
        if ms >= 2_100 {
            assert!(status.contains("disk "), "{ms} ms: {status}");
            assert!(status.contains("B/s"), "{ms} ms: {status}");
        }
    }
    assert!(r.status().ends_with("ETA unknown"), "{}", r.status());
}

#[test]
fn a_silent_source_hides_the_field_and_it_comes_back_after_a_full_second() {
    let mut r = rig();
    for ms in (0..=7_000).step_by(100) {
        // Answers, goes silent between 3.5 s and 4.5 s (the samples at 4 s see nothing), answers.
        let answer = if (3_500..4_500).contains(&ms) {
            None
        } else {
            steady(ms)
        };
        r.at(ms, answer);
        let status = r.status();
        if (2_300..4_000).contains(&ms) {
            assert!(status.contains("disk"), "{ms} ms: {status}");
        }
        // The sample at 4 s found nothing: no value until a full window after the next reading.
        if (4_000..6_000).contains(&ms) {
            assert!(
                !status.contains("disk") && !status.contains("B/s"),
                "{ms} ms: {status}"
            );
        }
        if ms >= 6_100 {
            assert!(status.contains("disk"), "back at {ms} ms: {status}");
        }
    }
}

#[test]
fn no_source_means_no_field_and_no_fake_zero() {
    let mut r = rig();
    for ms in (0..=5_000).step_by(100) {
        r.at(ms, None);
        let status = r.status();
        assert!(
            !status.contains("disk") && !status.contains("B/s"),
            "{status}"
        );
    }
    assert!(r.status().contains("Scanning"), "{}", r.status());
}

#[test]
fn a_measured_zero_shows_as_zero() {
    let mut r = rig();
    for ms in (0..=4_000).step_by(100) {
        r.at(ms, Some(1_000));
    }
    assert!(r.status().contains("disk       0 B/s"), "{}", r.status());
}

#[test]
fn the_value_changes_at_most_once_per_second_and_the_source_is_asked_about_once_per_second() {
    let mut r = rig();
    let mut seen = Vec::new();
    let mut bytes = 0u64;
    for (i, ms) in (0..=6_000).step_by(100).enumerate() {
        // Faster every step, so every reading would give a different number.
        bytes += (i as u64 + 1) * 10_000;
        r.at(ms, Some(bytes));
        let status = r.status();
        let field = status.find("disk").map(|at| status[at..at + 16].to_owned());
        if seen.last() != Some(&field) {
            seen.push(field);
        }
    }
    let values = seen.iter().filter(|f| f.is_some()).count();
    assert!(values >= 2, "the value should move: {seen:?}");
    // 6 s of run, the field can only exist from about 1 s on: 5 changes at most (+ the start).
    assert!(seen.len() <= 7, "{seen:?}");
    assert!(
        r.source.calls.get() <= 7,
        "asked {} times",
        r.source.calls.get()
    );
}

fn with_rate(state: &mut AppState) {
    state.read_meter.sample(Duration::from_secs(10), Some(0));
    state
        .read_meter
        .sample(Duration::from_secs(11), Some(64 * 1024 * 1024));
    assert!(state.read_meter.rate().is_some());
}

#[test]
fn the_field_never_shows_outside_the_scanning_states() {
    let config = || cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
    let text = |state: &AppState| status::text(state, 200);

    // Starting (first 2 s) and Scanning with a rate: only Scanning shows it.
    let mut state = AppState::new(config(), synthetic::model());
    with_rate(&mut state);
    state.tick(Duration::from_secs(1));
    assert!(!text(&state).contains("disk"), "starting: {}", text(&state));
    state.model.apply(synthetic::listing(
        storage_analyzer::types::NodeId::ROOT,
        1,
        vec![synthetic::entry(
            "a",
            storage_analyzer::types::Kind::Dir,
            4096,
        )],
    ));
    state.tick(Duration::from_secs(5));
    assert!(
        text(&state).contains("disk  64.0 MiB/s"),
        "{}",
        text(&state)
    );

    // Still Starting after 2 s (no listing yet) with a rate: the Starting line has no field.
    let mut early = AppState::new(config(), synthetic::model());
    with_rate(&mut early);
    early.tick(Duration::from_secs(5));
    assert!(text(&early).contains("Starting"), "{}", text(&early));
    assert!(!text(&early).contains("disk"), "starting: {}", text(&early));

    // A rescan of a subtree after Done is not the first scan: no field. A root rescan is.
    let mut done = AppState::new(config(), synthetic::model());
    done.model.apply(ScanMsg::Done { cancelled: false });
    with_rate(&mut done);
    done.tick(Duration::from_secs(5));
    done.rescan_in_flight = true;
    done.scan_kind = storage_analyzer::app::state::ScanKind::Subtree;
    assert!(
        !text(&done).contains("disk"),
        "subtree rescan: {}",
        text(&done)
    );
    done.scan_kind = storage_analyzer::app::state::ScanKind::Root;
    done.tick(Duration::from_secs(8));
    assert!(text(&done).contains("disk"), "root rescan: {}", text(&done));

    // Stopping: cancel requested, workers not done.
    state.control.cancel_all();
    state.tick(Duration::from_secs(6));
    assert!(text(&state).contains("Stopping"), "{}", text(&state));
    assert!(!text(&state).contains("disk"), "stopping: {}", text(&state));

    // Stopped and Done.
    for (cancelled, word) in [(true, "Stopped"), (false, "Done")] {
        let mut state = AppState::new(config(), synthetic::model());
        with_rate(&mut state);
        state.model.apply(ScanMsg::Done { cancelled });
        state.tick(Duration::from_secs(5));
        assert!(text(&state).contains(word), "{}", text(&state));
        assert!(!text(&state).contains("disk"), "{word}: {}", text(&state));
    }

    // Deleting.
    let mut state = AppState::new(config(), synthetic::model());
    with_rate(&mut state);
    state.delete_run = Some(storage_analyzer::app::delete_flow::DeleteRun {
        target: storage_analyzer::types::NodeId::ROOT,
        path: "/x".into(),
        total_files: Some(10),
        counted_files: 10,
        removed_files: 1,
        removed_dirs: 0,
    });
    assert!(text(&state).contains("Deleting"));
    assert!(!text(&state).contains("disk"));
}

#[test]
fn the_field_has_no_colour_of_its_own() {
    let mut r = rig();
    for ms in (0..=3_500).step_by(100) {
        r.at(ms, steady(ms));
    }
    let buf = r.app.terminal().backend().buffer();
    let status = r.status();
    let at = status.find("disk").expect("the field is shown");
    let col = status[..at].chars().count() as u16;
    for x in col..col + 16 {
        let cell = &buf[(x, 22)];
        assert_eq!((cell.fg, cell.bg), (Color::Reset, Color::Reset), "x={x}");
    }
}
