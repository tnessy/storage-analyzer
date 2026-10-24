//! Quitting while a delete runs (T021, UX check 8).

use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use rustix::fs::{Mode, OFlags, open};
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::delete_flow::DeleteRun;
use storage_analyzer::app::event_loop::{App, Clock, EventSource, Flow, Services};
use storage_analyzer::app::state::{AppState, Dialog};
use storage_analyzer::app::terminal::PanicSlot;
use storage_analyzer::app::{preflight, production_starter};
use storage_analyzer::delete::{self, DeleteEvent};
use storage_analyzer::model::{Model, ModelOptions};

#[derive(Clone, Default)]
struct TestClock(Rc<Cell<Duration>>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

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
    dir: tempfile::TempDir,
}

fn rig(files: usize, channel: usize) -> Rig {
    rig_with(files, channel, delete::spawn)
}

/// Like `rig`, with the executor made by `make` (tests inject slow or scripted ones).
fn rig_with(
    files: usize,
    channel: usize,
    make: impl FnOnce(
        std::os::fd::OwnedFd,
        std::sync::mpsc::SyncSender<DeleteEvent>,
    ) -> delete::DeleteHandle,
) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("big")).unwrap();
    for i in 0..files {
        std::fs::write(dir.path().join(format!("big/f{i:05}")), b"x").unwrap();
    }
    let root = preflight(dir.path()).unwrap();
    let fd = open(
        &root.path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    let (tx, rx) = sync_channel::<DeleteEvent>(channel);
    let services = Services {
        delete: Some((make(fd, tx), rx)),
        ..Services::default()
    };
    let model = Model::new(root.path, root.entry, ModelOptions { top_n: 100 });
    let config = cli::parse(APP_ID, [APP_ID, "--no-animation", "--threads", "1"]).unwrap();
    let (clock, keys) = (TestClock::default(), Keys::default());
    let app = App::new(
        Terminal::new(TestBackend::new(100, 30)).unwrap(),
        keys.clone(),
        clock.clone(),
        AppState::new(config, model),
        production_starter(),
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    )
    .with_services(services);
    let mut r = Rig {
        app,
        keys,
        clock,
        dir,
    };
    r.app.start().unwrap();
    let started = Instant::now();
    while r.app.state().is_scanning() {
        assert!(started.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(3));
        r.step();
    }
    r
}

impl Rig {
    fn step(&mut self) -> Flow {
        self.clock
            .0
            .set(self.clock.0.get() + Duration::from_millis(100));
        self.app.step().unwrap()
    }

    fn press(&mut self, code: KeyCode, mods: KeyModifiers) -> Flow {
        self.keys
            .0
            .borrow_mut()
            .push_back(Event::Key(KeyEvent::new(code, mods)));
        self.step()
    }

    fn ch(&mut self, c: char) -> Flow {
        self.press(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn screen(&self) -> String {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Pretends a delete of `big` is running, without an executor request.
    fn fake_run(&mut self, total: Option<u64>, removed: u64) {
        let path = self.dir.path().canonicalize().unwrap().join("big");
        self.app.state_mut().delete_run = Some(DeleteRun {
            target: self
                .app
                .state()
                .model
                .children(storage_analyzer::types::NodeId::ROOT)[0],
            path,
            total_files: total,
            counted_files: total.unwrap_or(0),
            removed_files: removed,
            removed_dirs: 0,
        });
    }
}

#[test]
fn quit_during_a_delete_asks_and_only_y_quits() {
    let mut r = rig(3, 8);
    r.fake_run(Some(10), 3);
    r.ch('q');
    assert!(matches!(
        r.app.state().dialog,
        Some(Dialog::QuitWhileDeleting)
    ));
    let s = r.screen();
    assert!(s.contains("Quit while deleting?"), "{s}");
    assert!(s.contains("A delete is in progress:"), "{s}");
    assert!(s.contains("3 of 10 files removed."), "{s}");
    // Any other key, including the quit keys, closes the dialog and the delete goes on.
    for (code, mods) in [
        (KeyCode::Char('q'), KeyModifiers::NONE),
        (KeyCode::Char('c'), KeyModifiers::CONTROL),
        (KeyCode::Esc, KeyModifiers::NONE),
        (KeyCode::Enter, KeyModifiers::NONE),
        (KeyCode::Char('n'), KeyModifiers::NONE),
    ] {
        assert_eq!(r.press(code, mods), Flow::Continue);
        assert!(r.app.state().dialog.is_none(), "{code:?}");
        assert!(r.app.state().delete_run.is_some());
        r.ch('q');
        assert!(matches!(
            r.app.state().dialog,
            Some(Dialog::QuitWhileDeleting)
        ));
    }
    assert_eq!(r.ch('y'), Flow::Quit);
    let line = r.app.finish().expect("interrupted line");
    assert!(line.starts_with("delete of "), "{line}");
    assert!(
        line.ends_with("interrupted: 3 of 10 files removed"),
        "{line}"
    );
}

#[test]
fn unknown_total_leaves_out_of_m() {
    let mut r = rig(3, 8);
    r.fake_run(None, 5);
    r.ch('q');
    assert!(
        r.screen().contains("5 files removed so far."),
        "{}",
        r.screen()
    );
    assert_eq!(r.ch('y'), Flow::Quit);
    let line = r.app.finish().unwrap();
    assert!(line.ends_with("interrupted: 5 files removed"), "{line}");
}

#[test]
fn quit_without_a_delete_prints_nothing() {
    let mut r = rig(1, 8);
    r.ch('q');
    assert!(matches!(r.app.state().dialog, Some(Dialog::Quit)));
    assert_eq!(r.ch('y'), Flow::Quit);
    assert_eq!(r.app.finish(), None);
}

#[test]
fn quit_with_the_event_channel_full_exits_within_two_seconds() {
    // An executor that reports 7 files, then runs until told to stop; the channel holds one
    // event and nobody reads it, so the executor's `Finished` could never be delivered.
    let mut r = rig_with(10, 1, |_fd, tx| {
        delete::spawn_with(tx, |_req, stop, on_removed| {
            std::thread::sleep(Duration::from_millis(150));
            on_removed(storage_analyzer::types::Counts {
                files: 7,
                dirs: 0,
                bytes: 7,
            });
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
            delete::Outcome::Partial {
                removed: storage_analyzer::types::Counts::default(),
                failures: Vec::new(),
            }
        })
    });
    start_delete(&mut r);
    std::thread::sleep(Duration::from_millis(400));
    for c in ['q', 'y'] {
        r.keys.0.borrow_mut().push_back(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
    }
    let t0 = Instant::now();
    assert_eq!(r.step(), Flow::Quit);
    let line = r.app.finish();
    assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    let line = line.expect("interrupted line");
    assert!(
        line.contains("interrupted: 7 of 10 files removed"),
        "{line}"
    );
}

#[test]
fn sigterm_during_a_delete_also_reports() {
    let mut r = rig(3, 8);
    r.fake_run(Some(10), 4);
    // `terminate` is the flag the signal handlers set.
    let _ = &r.dir;
    r.app.state_mut().quit = true;
    assert_eq!(r.step(), Flow::Quit);
    let line = r.app.finish().unwrap();
    assert!(
        line.ends_with("interrupted: 4 of 10 files removed"),
        "{line}"
    );
}

/// Starts a delete of `big` through the keys, with the rig's executor.
fn start_delete(r: &mut Rig) {
    let id = r
        .app
        .state()
        .model
        .children(storage_analyzer::types::NodeId::ROOT)[0];
    r.app.state_mut().tree.selection = id;
    r.app
        .state_mut()
        .tree
        .expanded
        .insert(storage_analyzer::types::NodeId::ROOT);
    r.ch('d');
    r.ch('y');
    assert!(r.app.state().delete_run.is_some());
}

#[test]
fn an_executor_that_never_finishes_cannot_hold_up_quitting() {
    let mut r = rig_with(3, 8, |_fd, tx| {
        delete::spawn_with(tx, |_req, _stop, _on| {
            std::thread::sleep(Duration::from_secs(8));
            delete::Outcome::Removed(storage_analyzer::types::Counts::default())
        })
    });
    start_delete(&mut r);
    r.ch('q');
    let t0 = Instant::now();
    assert_eq!(r.ch('y'), Flow::Quit);
    let line = r.app.finish();
    // One wait of at most 2 s, not one for the stop and another for dropping the handle.
    assert!(
        t0.elapsed() < Duration::from_millis(2500),
        "{:?}",
        t0.elapsed()
    );
    assert!(line.expect("a line").contains("interrupted"));
}

/// An executor that ends, with `outcome`, shortly after it is told to stop.
fn ends_after_stop(
    outcome: delete::Outcome,
) -> impl FnOnce(std::os::fd::OwnedFd, std::sync::mpsc::SyncSender<DeleteEvent>) -> delete::DeleteHandle
{
    move |_fd, tx| {
        delete::spawn_with(tx, move |_req, stop, _on| {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
            std::thread::sleep(Duration::from_millis(100));
            outcome.clone()
        })
    }
}

fn quit_now(r: &mut Rig) -> Option<String> {
    r.ch('q');
    assert_eq!(r.ch('y'), Flow::Quit);
    // `Rig` owns the app; `finish` consumes it.
    let app = std::mem::replace(&mut r.app, dummy_app());
    app.finish()
}

fn dummy_app() -> App<TestBackend, Keys, TestClock> {
    let config = cli::parse(APP_ID, [APP_ID]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = preflight(dir.path()).unwrap();
    App::new(
        Terminal::new(TestBackend::new(10, 5)).unwrap(),
        Keys::default(),
        TestClock::default(),
        AppState::new(
            config,
            Model::new(root.path, root.entry, ModelOptions { top_n: 1 }),
        ),
        Box::new(|_| None),
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    )
}

#[test]
fn a_delete_that_finishes_while_quitting_prints_no_interrupted_line() {
    let counts = storage_analyzer::types::Counts {
        files: 3,
        dirs: 1,
        bytes: 3,
    };
    let mut r = rig_with(3, 8, ends_after_stop(delete::Outcome::Removed(counts)));
    start_delete(&mut r);
    assert_eq!(quit_now(&mut r), None);
}

#[test]
fn a_delete_that_ends_with_errors_while_quitting_says_so() {
    let outcome = delete::Outcome::Partial {
        removed: storage_analyzer::types::Counts {
            files: 2,
            dirs: 0,
            bytes: 2,
        },
        failures: vec![delete::DeleteFailure {
            rel_path: "big/x".into(),
            kind: delete::DeleteFailKind::Busy,
        }],
    };
    let mut r = rig_with(3, 8, ends_after_stop(outcome));
    start_delete(&mut r);
    let line = quit_now(&mut r).expect("a line");
    assert!(!line.contains("interrupted"), "{line}");
    assert!(
        line.contains("ended with errors") && line.contains("1 items could not be deleted"),
        "{line}"
    );
}

#[test]
fn a_delete_that_was_refused_at_the_last_moment_says_nothing_was_deleted() {
    let outcome = delete::Outcome::Aborted {
        reason: delete::AbortReason::NotFound,
    };
    let mut r = rig_with(3, 8, ends_after_stop(outcome));
    start_delete(&mut r);
    let line = quit_now(&mut r).expect("a line");
    assert!(
        line.contains("nothing was deleted") && !line.contains("interrupted"),
        "{line}"
    );
}

#[test]
fn a_stopped_delete_reports_how_far_it_got() {
    // The real executor, stopped between entries, ends `Partial` with no failures: interrupted.
    let outcome = delete::Outcome::Partial {
        removed: storage_analyzer::types::Counts {
            files: 1,
            dirs: 0,
            bytes: 1,
        },
        failures: Vec::new(),
    };
    let mut r = rig_with(3, 8, ends_after_stop(outcome));
    start_delete(&mut r);
    let line = quit_now(&mut r).expect("a line");
    assert!(line.contains("interrupted"), "{line}");
}

#[test]
fn exit_messages_never_carry_escape_bytes_from_the_path() {
    let evil = std::path::PathBuf::from("/x/a\x1b[2Jb");
    // Interrupted (no outcome seen).
    let mut r = rig(3, 8);
    r.fake_run(Some(10), 4);
    r.app.state_mut().delete_run.as_mut().unwrap().path = evil.clone();
    let line = quit_now(&mut r).expect("a line");
    assert!(!line.contains('\x1b'), "{line:?}");
    assert!(line.contains("a\\x1b[2Jb"), "{line}");
    // The real outcome, with errors and aborted.
    let partial = delete::Outcome::Partial {
        removed: storage_analyzer::types::Counts::default(),
        failures: vec![delete::DeleteFailure {
            rel_path: "x".into(),
            kind: delete::DeleteFailKind::Busy,
        }],
    };
    let aborted = delete::Outcome::Aborted {
        reason: delete::AbortReason::NotFound,
    };
    for outcome in [partial, aborted] {
        let mut r = rig_with(3, 8, ends_after_stop(outcome));
        start_delete(&mut r);
        r.app.state_mut().delete_run.as_mut().unwrap().path = evil.clone();
        let line = quit_now(&mut r).expect("a line");
        assert!(!line.contains('\x1b'), "{line:?}");
    }
}
