//! Delete dialogs, the run, the result dialogs and the failure list (T021).
//!
//! The rig scans a real temp directory with the real walker and deletes with the real executor.

#[path = "support/synthetic.rs"]
mod synthetic;

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
use storage_analyzer::app::event_loop::{App, Clock, EventSource, Services};
use storage_analyzer::app::state::{AppState, Dialog};
use storage_analyzer::app::terminal::PanicSlot;
use storage_analyzer::app::{preflight, production_starter};
use storage_analyzer::delete::{self, DeleteEvent};
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::types::NodeId;

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
    panics: PanicSlot,
}

/// `big/` (3 files, one subfolder), `small/` (1 file) and `top.txt`.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    std::fs::create_dir_all(p.join("big/sub")).unwrap();
    std::fs::create_dir(p.join("small")).unwrap();
    for f in [
        "big/a.bin",
        "big/b.bin",
        "big/sub/c.bin",
        "small/s.bin",
        "top.txt",
    ] {
        std::fs::write(p.join(f), vec![7u8; 8192]).unwrap();
    }
    dir
}

fn rig() -> Rig {
    rig_in(fixture())
}

fn rig_in(dir: tempfile::TempDir) -> Rig {
    let root = preflight(dir.path()).unwrap();
    let fd = open(
        &root.path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    let (tx, rx) = sync_channel::<DeleteEvent>(64);
    let services = Services {
        delete: Some((delete::spawn(fd, tx), rx)),
        ..Services::default()
    };
    let model = Model::new(root.path, root.entry, ModelOptions { top_n: 100 });
    let config = cli::parse(APP_ID, [APP_ID, "--no-animation", "--threads", "1"]).unwrap();
    let (clock, keys) = (TestClock::default(), Keys::default());
    let panics = PanicSlot::default();
    let app = App::new(
        Terminal::new(TestBackend::new(100, 30)).unwrap(),
        keys.clone(),
        clock.clone(),
        AppState::new(config, model),
        production_starter(),
        Arc::new(AtomicBool::new(false)),
        panics.clone(),
    )
    .with_services(services);
    let mut rig = Rig {
        app,
        keys,
        clock,
        dir,
        panics,
    };
    rig.app.start().unwrap();
    rig.until("scan done", |r| !r.app.state().is_scanning());
    rig
}

impl Rig {
    fn state(&self) -> &AppState {
        self.app.state()
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

    /// A redraw without a key (state set by hand does not draw by itself).
    fn redraw(&mut self) {
        self.keys.0.borrow_mut().push_back(Event::Resize(100, 30));
        self.step(100);
    }

    fn chars(&mut self, s: &str) {
        for c in s.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    fn screen(&self) -> String {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn until(&mut self, what: &str, done: impl Fn(&Rig) -> bool) {
        let started = Instant::now();
        while !done(self) {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "timed out waiting for {what}:\n{}",
                self.screen()
            );
            std::thread::sleep(Duration::from_millis(3));
            self.step(100);
        }
    }

    fn find(&self, name: &str) -> NodeId {
        let model = &self.state().model;
        *model
            .children(NodeId::ROOT)
            .iter()
            .find(|&&c| model.name(c) == name.as_bytes())
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn select(&mut self, name: &str) {
        let id = self.find(name);
        let st = self.app.state_mut();
        st.tree.selection = id;
        st.tree.aggregate = None;
        st.tree.on_parent = false;
        st.tree.expanded.insert(NodeId::ROOT);
        self.step(100);
    }

    fn exists(&self, rel: &str) -> bool {
        self.dir.path().join(rel).exists()
    }

    /// The open dialog's paragraphs: inner text of the `║` rows, wrapped lines joined.
    fn paragraphs(&self) -> Vec<String> {
        let mut out = vec![String::new()];
        for row in self.screen().lines() {
            let (Some(a), Some(b)) = (row.find('║'), row.rfind('║')) else {
                continue;
            };
            if a == b {
                continue;
            }
            let inner = row[a + '║'.len_utf8()..b].trim();
            let last = out.last_mut().unwrap();
            if inner.is_empty() {
                if !last.is_empty() {
                    out.push(String::new());
                }
            } else {
                if !last.is_empty() {
                    last.push(' ');
                }
                last.push_str(inner);
            }
        }
        out.retain(|p| !p.is_empty());
        out
    }

    fn dialog(&self) -> Option<&Dialog> {
        self.state().dialog.as_ref()
    }

    fn transient(&self) -> String {
        self.state()
            .transient
            .as_ref()
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }
}

// ---- check 6: only `y` deletes ----

#[test]
fn exact_dialog_shows_the_numbers_and_only_y_deletes() {
    let mut r = rig();
    r.select("big");
    r.chars("d");
    let s = r.screen();
    assert!(s.contains("Delete permanently?"), "{s}");
    assert!(
        s.contains("Folder · 24.0 KiB (disk usage) · 3 files"),
        "{s}"
    );
    assert!(
        s.contains("3 files · 1 folders") || s.contains("3 files · 1 folder"),
        "{s}"
    );
    assert!(s.contains("This cannot be undone."), "{s}");
    assert!(!s.contains("Largest known"), "{s}");
    // Every other key cancels or is ignored; nothing is deleted.
    for k in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        r.key(k);
        assert!(r.dialog().is_none());
        assert!(r.exists("big"));
        r.chars("d");
        assert!(matches!(r.dialog(), Some(Dialog::Delete(_))));
    }
    for k in [
        KeyCode::Char('x'),
        KeyCode::Char('d'),
        KeyCode::Char('Y'),
        KeyCode::Tab,
    ] {
        r.key(k);
        assert!(matches!(r.dialog(), Some(Dialog::Delete(_))), "{k:?}");
    }
    assert!(r.exists("big"));
    r.chars("y");
    r.until("delete finished", |r| {
        !r.exists("big") && r.state().delete_run.is_none()
    });
    assert!(r.exists("small") && r.exists("top.txt"));
    assert!(r.dialog().is_none());
    let t = r.transient();
    assert!(
        t.starts_with("Deleted ") && t.contains("freed 24.0 KiB ") && t.contains("(3 files)"),
        "{t}"
    );
    // The row leaves the model on the step after the outcome frame.
    r.step(100);
    assert!(
        r.state()
            .model
            .children(NodeId::ROOT)
            .iter()
            .all(|&c| r.state().model.name(c) != b"big")
    );
}

#[test]
fn a_file_gets_the_file_variant() {
    let mut r = rig();
    r.select("top.txt");
    r.chars("d");
    let s = r.screen();
    assert!(s.contains("File · "), "{s}");
    assert!(!s.contains("folders"), "{s}");
}

#[test]
fn hard_link_line_appears_only_when_links_leave_the_folder() {
    let dir = fixture();
    std::fs::hard_link(
        dir.path().join("big/a.bin"),
        dir.path().join("outside-link"),
    )
    .unwrap();
    let mut r = rig_in(dir);
    r.select("big");
    r.chars("d");
    let text = r.paragraphs();
    assert!(
        text.iter().any(|p| p.starts_with(
            "Space freed may be less: 1 file also has hard links outside this folder, so at most "
        ) && p.ends_with(" KiB is freed.")),
        "{text:?}"
    );
    r.key(KeyCode::Esc);
    r.select("small");
    r.chars("d");
    assert!(!r.screen().contains("hard link"), "{}", r.screen());
}

#[test]
fn hard_link_line_counts_files_in_the_plural() {
    let dir = fixture();
    for (from, to) in [("big/a.bin", "l1"), ("big/b.bin", "l2")] {
        std::fs::hard_link(dir.path().join(from), dir.path().join(to)).unwrap();
    }
    let mut r = rig_in(dir);
    r.select("big");
    r.chars("d");
    let text = r.paragraphs();
    assert!(
        text.iter().any(|p| p.starts_with(
            "Space freed may be less: 2 files also have hard links outside this folder, so at most "
        )),
        "{text:?}"
    );
}

// ---- check 7: refusals ----

#[test]
fn the_scan_root_cannot_be_deleted() {
    let mut r = rig();
    let st = r.app.state_mut();
    st.tree.selection = NodeId::ROOT;
    r.step(100);
    r.chars("d");
    assert!(r.dialog().is_none());
    assert_eq!(r.transient(), "Can't delete the scan root.");
}

#[test]
fn a_second_delete_waits_for_the_first() {
    use storage_analyzer::app::delete_flow::DeleteRun;
    let mut r = rig();
    // A delete under way, without a real executor so that it cannot finish by itself.
    let big = r.find("big");
    let path = r.state().model.path(big);
    r.app.state_mut().model.begin_delete(big).unwrap();
    r.app.state_mut().delete_run = Some(DeleteRun {
        target: big,
        path: path.clone(),
        total_files: None,
        counted_files: 0,
        removed_files: 0,
        removed_dirs: 0,
    });
    r.select("small");
    r.chars("d");
    assert!(r.dialog().is_none());
    assert_eq!(
        r.transient(),
        format!(
            "One delete at a time. Wait for {} to finish.",
            path.display()
        )
    );
    // The folder being deleted itself says it is already going.
    r.select("big");
    r.chars("d");
    assert_eq!(r.transient(), "Already being deleted.");
}

#[test]
fn d_on_the_parent_row_still_gets_the_hint() {
    let mut r = rig();
    r.app.state_mut().tree.on_parent = true;
    r.chars("d");
    assert!(r.dialog().is_none());
    assert_eq!(
        r.transient(),
        "\"..\" is the parent folder. Press ⏎ to go up."
    );
}

// ---- unfinished folder: the dialog changes in place when the target becomes exact ----

#[test]
fn unfinished_target_shows_lower_bounds_and_p_asks_for_the_scan_first() {
    let mut r = rig();
    // Reopen as an unfinished target: a prompt built by hand over a complete folder is not
    // the unfinished variant, so pretend the check was taken before the scan finished.
    let big = r.find("big");
    let mut check = r.state().model.delete_check(big, r.state().size_mode);
    check.exact = false;
    check.largest_known = [None; 3];
    r.app.state_mut().dialog = Some(Dialog::Delete(
        storage_analyzer::app::delete_flow::DeletePrompt {
            target: big,
            check,
            checked_at: r.state().now,
            prioritising: false,
        },
    ));
    r.redraw();
    let s = r.screen();
    assert!(s.contains("(scan of this folder not finished)"), "{s}");
    assert!(s.contains("Nothing listed inside yet."), "{s}");
    assert!(s.contains("Finish scan first"), "{s}");
    let text = r.paragraphs();
    assert!(
        text.contains(
            &"Everything inside will be deleted, including files not counted yet.".to_owned()
        ),
        "no glyph before the warning: {text:?}"
    );
    assert!(
        text.contains(&"Hard links: not fully known (scan not finished).".to_owned()),
        "own paragraph: {text:?}"
    );
    r.chars("p");
    assert!(
        r.screen().contains("Scanning this folder first"),
        "{}",
        r.screen()
    );
    // Other keys are ignored, and the numbers refresh into the exact variant.
    r.chars("x");
    r.until("exact variant", |r| !r.screen().contains("not finished"));
    assert!(r.screen().contains("Delete permanently?"));
    assert!(r.exists("big"));
}

#[test]
fn a_confirmation_whose_target_becomes_refused_says_why() {
    use storage_analyzer::app::delete_flow::DeleteRun;
    let mut r = rig();
    r.select("small");
    r.chars("d");
    assert!(matches!(r.dialog(), Some(Dialog::Delete(_))));
    // Another delete starts while the dialog is open.
    let big = r.find("big");
    let path = r.state().model.path(big);
    r.app.state_mut().model.begin_delete(big).unwrap();
    r.app.state_mut().delete_run = Some(DeleteRun {
        target: big,
        path: path.clone(),
        total_files: None,
        counted_files: 0,
        removed_files: 0,
        removed_dirs: 0,
    });
    r.until("the dialog to close", |r| r.dialog().is_none());
    assert_eq!(
        r.transient(),
        format!(
            "One delete at a time. Wait for {} to finish.",
            path.display()
        )
    );
}

// ---- results ----

/// `big/locked/` cannot be emptied (mode 0500), so deleting `big` ends `Partial`.
fn locked_fixture() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture();
    let locked = dir.path().join("big/locked");
    std::fs::create_dir(&locked).unwrap();
    for f in ["x1", "x2"] {
        std::fs::write(locked.join(f), vec![1u8; 4096]).unwrap();
    }
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    dir
}

fn unlock(r: &Rig) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(
        r.dir.path().join("big/locked"),
        std::fs::Permissions::from_mode(0o700),
    );
}

#[test]
fn partial_result_groups_failures_and_v_lists_them() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let mut r = rig_in(locked_fixture());
    r.select("big");
    r.chars("dy");
    r.until("result dialog", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteDone(_)))
    });
    let s = r.screen();
    assert!(s.contains("Delete finished with errors"), "{s}");
    assert!(
        s.contains("Deleted ") && s.contains("freed ") && s.contains(" KiB"),
        "{s}"
    );
    assert!(s.contains("permission denied"), "{s}");
    assert!(s.contains("v  View failed paths"), "{s}");
    assert_eq!(r.state().failures().len(), 2, "{s}");

    r.chars("v");
    let s = r.screen();
    assert!(
        matches!(r.dialog(), Some(Dialog::FailedPaths { .. })),
        "{s}"
    );
    assert!(s.contains("Couldn't delete 2 items"), "{s}");
    assert!(s.contains("locked/x1") && s.contains("locked/x2"), "{s}");
    assert!(s.contains("j/k scroll  Esc close"), "{s}");
    assert!(!s.contains("PgUp"), "{s}");
    // Esc returns to the result dialog; Enter then dismisses it with the pointer to `v`.
    r.key(KeyCode::Esc);
    assert!(matches!(r.dialog(), Some(Dialog::DeleteDone(_))));
    r.key(KeyCode::Enter);
    assert!(r.dialog().is_none());
    assert_eq!(
        r.transient(),
        "2 items could not be deleted. Press v to see them."
    );

    // From the main screen `v` opens the list and `v` closes it again.
    r.chars("v");
    assert!(matches!(r.dialog(), Some(Dialog::FailedPaths { .. })));
    r.chars("v");
    assert!(r.dialog().is_none());
    // q opens the quit dialog from the list; the list is not the Skipped view.
    r.chars("v");
    r.chars("q");
    assert!(matches!(r.dialog(), Some(Dialog::Quit)));
    r.key(KeyCode::Esc);
    r.chars("4");
    assert!(!r.screen().contains("locked/x1"), "{}", r.screen());
    unlock(&r);
}

#[test]
fn the_next_delete_replaces_the_failures() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let mut r = rig_in(locked_fixture());
    r.select("big");
    r.chars("dy");
    r.until("result", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteDone(_)))
    });
    r.key(KeyCode::Enter);
    assert_eq!(r.state().failures().len(), 2);
    unlock(&r);
    r.select("small");
    r.chars("dy");
    r.until("second delete", |r| {
        !r.exists("small") && r.state().delete_run.is_none()
    });
    assert!(r.state().failures().is_empty());
    r.chars("v");
    assert_eq!(r.transient(), "No delete failures to show.");
}

#[test]
fn a_changed_folder_gives_the_not_deleted_dialog() {
    let mut r = rig();
    r.select("small");
    r.chars("d");
    // The folder is replaced between the scan and the confirmation (new inode).
    let p = r.dir.path().join("small");
    std::fs::remove_dir_all(&p).unwrap();
    std::fs::create_dir(&p).unwrap();
    r.chars("y");
    r.until("not deleted", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteAborted(_)))
    });
    let s = r.screen();
    assert!(s.contains("Not deleted"), "{s}");
    assert!(s.contains("Nothing was deleted."), "{s}");
    assert!(
        r.paragraphs().iter().any(|p| p.starts_with(
            "It changed since it was scanned (it may have been replaced or moved). \
             Rescan with r, then try again."
        )),
        "{:?}",
        r.paragraphs()
    );
    assert!(r.exists("small"));
    r.key(KeyCode::Enter);
    assert!(r.dialog().is_none());
    // The folder is shown again and its row is no longer marked as being deleted.
    assert!(!r.screen().contains("(deleting"), "{}", r.screen());
}

#[test]
fn a_running_delete_shows_in_the_status_line() {
    use storage_analyzer::app::delete_flow::DeleteRun;
    let mut r = rig();
    let big = r.find("big");
    let path = r.state().model.path(big);
    let run = |total| DeleteRun {
        target: big,
        path: path.clone(),
        total_files: total,
        counted_files: 0,
        removed_files: 12_400,
        removed_dirs: 0,
    };
    let status = |r: &Rig| {
        r.screen()
            .lines()
            .find(|l| l.contains("Deleting "))
            .unwrap_or_else(|| panic!("no status:\n{}", r.screen()))
            .trim()
            .to_owned()
    };
    r.app.state_mut().delete_run = Some(run(Some(48_212)));
    r.redraw();
    assert_eq!(
        &status(&r)[4..],
        &format!("Deleting {} … 12,400 / 48,212 files", path.display())
    );
    r.app.state_mut().delete_run = Some(run(None));
    r.redraw();
    assert_eq!(
        &status(&r)[4..],
        &format!("Deleting {} … 12,400 files removed", path.display())
    );
}

#[test]
fn an_executor_panic_adds_a_line_and_does_not_stop_the_scan() {
    if rustix::process::geteuid().is_root() {
        return;
    }
    let mut r = rig_in(locked_fixture());
    // The panic hook records the message before the executor's `Partial` is sent.
    r.panics
        .record(Some("delete".into()), "executor blew up".into());
    r.select("big");
    r.chars("dy");
    r.until("result dialog", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteDone(_)))
    });
    assert!(r.screen().contains("executor blew up"), "{}", r.screen());
    assert!(!matches!(r.dialog(), Some(Dialog::ScanError(_))));
    unlock(&r);
}

#[test]
fn a_partial_delete_while_stopping_queues_one_rescan_until_done() {
    use storage_analyzer::delete::{DeleteFailKind, DeleteFailure, Outcome};
    use storage_analyzer::scan::ScanMsg;
    let mut r = rig();
    let big = r.find("big");
    r.app.state_mut().model.begin_delete(big).unwrap();
    let st = r.app.state_mut();
    st.delete_run = Some(storage_analyzer::app::delete_flow::DeleteRun {
        target: big,
        path: st.model.path(big),
        total_files: None,
        counted_files: 0,
        removed_files: 0,
        removed_dirs: 0,
    });
    // A scan that is being stopped: cancelled, workers not done yet.
    st.rescan_in_flight = true;
    st.control.cancel_all();
    assert!(st.is_stopping());
    let partial = || {
        DeleteEvent::Finished(Outcome::Partial {
            removed: storage_analyzer::types::Counts::default(),
            failures: vec![DeleteFailure {
                rel_path: "big/x".into(),
                kind: DeleteFailKind::Busy,
            }],
        })
    };
    st.on_delete_event(partial());
    st.apply_pending_delete();
    assert_eq!(st.queued_rescans, vec![big]);
    // Repeating the reconcile (the dialog's own `Enter`, redraws) never queues it twice.
    st.rescan(big);
    assert_eq!(st.queued_rescans, vec![big]);
    st.apply_scan_msg(ScanMsg::Done { cancelled: true });
    assert!(st.queued_rescans.is_empty());
}

#[test]
#[ignore = "needs unshare -rm"]
fn a_mount_point_target_gives_the_not_deleted_dialog() {
    const NAME: &str = "a_mount_point_target_gives_the_not_deleted_dialog";
    if std::env::var_os("DELETE_TEST_IN_NS").is_none() {
        let probe = std::process::Command::new("unshare")
            .args(["-rm", "true"])
            .status();
        if !probe.is_ok_and(|s| s.success()) {
            println!("SKIP: unshare unavailable");
            return;
        }
        let status = std::process::Command::new("unshare")
            .args(["-rm", "env", "DELETE_TEST_IN_NS=1"])
            .arg(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", NAME, "--nocapture"])
            .status()
            .unwrap();
        assert!(status.success(), "{NAME} failed inside the namespace");
        return;
    }
    let dir = fixture();
    std::fs::create_dir(dir.path().join("bind")).unwrap();
    let bound = std::process::Command::new("mount")
        .args(["--bind"])
        .arg(dir.path().join("small"))
        .arg(dir.path().join("bind"))
        .status()
        .unwrap();
    assert!(bound.success());
    let mut r = rig_in(dir);
    r.select("bind");
    r.chars("dy");
    r.until("not deleted", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteAborted(_)))
    });
    let s = r.screen();
    assert!(s.contains("It's a mount point"), "{s}");
    assert!(r.exists("small/s.bin"));
}

// ---- the outcome is shown before the (slow) removal from the model ----

/// Steps until the step that handled the executor's `Finished` (the outcome is on screen).
fn until_outcome(r: &mut Rig) {
    let started = Instant::now();
    while r.state().delete_run.is_some() {
        assert!(started.elapsed() < Duration::from_secs(10), "no outcome");
        std::thread::sleep(Duration::from_millis(3));
        r.step(100);
    }
}

#[test]
fn the_outcome_frame_is_drawn_before_the_removal_and_the_totals_follow_on_the_next_frame() {
    let mut r = rig();
    r.select("big");
    r.chars("dy");
    until_outcome(&mut r);
    // The frame with the outcome still lists the folder: the removal has not run yet.
    let screen = r.screen();
    assert!(screen.contains("Deleted "), "{screen}");
    assert!(screen.contains("big/"), "{screen}");
    let big = r.find("big");
    assert!(r.state().model.exists(big));
    // The next step removes it and the following frame shows the new totals.
    r.step(100);
    assert!(!r.state().model.exists(big));
    assert!(!r.screen().contains("big/"), "{}", r.screen());
}

#[test]
fn a_d_pressed_between_the_outcome_and_the_removal_gets_the_one_at_a_time_refusal() {
    let mut r = rig();
    r.select("big");
    r.chars("dy");
    until_outcome(&mut r);
    let small = r.find("small");
    r.app.state_mut().tree.selection = small;
    let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    r.app.state_mut().handle_key(key);
    assert!(r.dialog().is_none());
    assert!(
        r.transient().starts_with("One delete at a time. Wait for "),
        "{}",
        r.transient()
    );
}

#[test]
fn a_partial_outcome_shows_its_dialog_before_the_rescan_starts() {
    use storage_analyzer::model::Flags;
    if rustix::process::geteuid().is_root() {
        return;
    }
    let mut r = rig_in(locked_fixture());
    r.select("big");
    r.chars("dy");
    r.until("result dialog", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteDone(_)))
    });
    let big = r.find("big");
    assert!(r.screen().contains("Delete finished with errors"));
    assert!(
        !r.state().rescan_in_flight,
        "the rescan waits for the next step"
    );
    assert!(r.state().model.flags(big).contains(Flags::DELETING));
    r.step(100);
    assert!(!r.state().model.flags(big).contains(Flags::DELETING));
    r.until("the rescan", |r| !r.state().rescan_in_flight);
    unlock(&r);
}

#[test]
fn an_aborted_outcome_shows_its_dialog_before_the_rescan_starts() {
    use storage_analyzer::model::Flags;
    let mut r = rig();
    r.select("small");
    r.chars("d");
    let p = r.dir.path().join("small");
    std::fs::remove_dir_all(&p).unwrap();
    std::fs::create_dir(&p).unwrap();
    r.chars("y");
    r.until("not deleted", |r| {
        matches!(r.dialog(), Some(Dialog::DeleteAborted(_)))
    });
    let small = r.find("small");
    assert!(!r.state().rescan_in_flight);
    assert!(r.state().model.flags(small).contains(Flags::DELETING));
    r.step(100);
    assert!(!r.state().model.flags(small).contains(Flags::DELETING));
}

// ---- check 20: the `v` key without failures ----

#[test]
fn v_without_failures_says_so() {
    let mut r = rig();
    r.chars("v");
    assert_eq!(r.transient(), "No delete failures to show.");
    assert!(r.dialog().is_none());
}

// ---- help ----

#[test]
fn help_lists_the_failure_key() {
    let mut r = rig();
    r.chars("?");
    assert!(
        r.screen().contains("Last delete's failures"),
        "{}",
        r.screen()
    );
}

// ---- 80×24 snapshots of every delete dialog (UX §11) ----

mod dialogs {
    use std::path::PathBuf;

    use ratatui::layout::Rect;
    use storage_analyzer::app::delete_flow::{
        DeleteAborted, DeleteDone, DeletePrompt, DeleteRun, group_failures,
    };
    use storage_analyzer::delete::{AbortReason, DeleteFailKind, DeleteFailure};
    use storage_analyzer::types::Counts;
    use storage_analyzer::ui;
    use storage_analyzer::ui::theme::Glyphs;

    use super::*;

    use super::synthetic::{Scan, file, folder};

    /// `Videos` is complete; `Projects` still has an unlisted subfolder.
    fn scene(ascii: bool) -> AppState {
        let mut scan = Scan::new();
        let kids = scan.list(
            NodeId::ROOT,
            vec![
                folder("Videos"),
                folder("Projects"),
                file("notes.txt", 300_000),
            ],
        );
        let inside = scan.list(
            kids[0],
            vec![
                file("movie.mkv", 40_000_000),
                file("clip.mp4", 9_000_000),
                folder("raw"),
            ],
        );
        scan.list(inside[2], vec![file("take1.mov", 2_000_000)]);
        scan.list(
            kids[1],
            vec![
                file("repo.zip", 20_000_000),
                folder("target"),
                file("a.rs", 4_000),
            ],
        );
        let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
        let mut state = AppState::new(config, scan.model);
        if ascii {
            state.glyphs = Glyphs::ascii();
        }
        state
    }

    fn draw(state: &mut AppState) -> String {
        state.prepare_view(Rect::new(0, 0, 80, 24));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| ui::render(f, state)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn prompt(state: &AppState, name: &str, prioritising: bool) -> Dialog {
        let model = &state.model;
        let id = *model
            .children(NodeId::ROOT)
            .iter()
            .find(|&&c| model.name(c) == name.as_bytes())
            .unwrap();
        Dialog::Delete(DeletePrompt {
            target: id,
            check: model.delete_check(id, state.size_mode),
            checked_at: Duration::ZERO,
            prioritising,
        })
    }

    fn failures() -> Vec<DeleteFailure> {
        let f = |p: &str, kind| DeleteFailure {
            rel_path: PathBuf::from(p),
            kind,
        };
        vec![
            f("Videos/locked/x1", DeleteFailKind::PermissionDenied),
            f("Videos/locked/x2", DeleteFailKind::PermissionDenied),
            f("Videos/in-use.log", DeleteFailKind::Busy),
            f("Videos/odd", DeleteFailKind::Other(5)),
        ]
    }

    #[test]
    fn every_delete_dialog_in_unicode_and_ascii() {
        for ascii in [false, true] {
            let tag = if ascii { "ascii" } else { "unicode" };
            let mut shots: Vec<(&str, AppState)> = Vec::new();
            let mut add =
                |name, dialog: &dyn Fn(&AppState) -> Dialog, tweak: &dyn Fn(&mut AppState)| {
                    let mut s = scene(ascii);
                    let d = dialog(&s);
                    s.dialog = Some(d);
                    tweak(&mut s);
                    shots.push((name, s));
                };
            add("exact", &|s| prompt(s, "Videos", false), &|_| {});
            add("unfinished", &|s| prompt(s, "Projects", false), &|_| {});
            add(
                "unfinished_prioritising",
                &|s| prompt(s, "Projects", true),
                &|_| {},
            );
            let done = |s: &AppState| {
                let list = failures();
                let _ = s;
                Dialog::DeleteDone(DeleteDone {
                    path: PathBuf::from("/synthetic/root/Videos"),
                    removed: Counts {
                        files: 12_400,
                        dirs: 310,
                        bytes: 31_000_000_000,
                    },
                    groups: group_failures(&list),
                    panic_note: None,
                })
            };
            add("partial", &done, &|s| s.last_delete_failures = failures());
            add(
                "failed_paths",
                &|_| Dialog::FailedPaths {
                    scroll: 0,
                    back: None,
                },
                &|s| {
                    s.last_delete_failures = failures();
                    s.last_delete_path = PathBuf::from("/synthetic/root/Videos");
                },
            );
            add(
                "aborted_mount_point",
                &|_| {
                    Dialog::DeleteAborted(DeleteAborted {
                        path: PathBuf::from("/synthetic/root/Videos"),
                        reason: AbortReason::MountPoint,
                    })
                },
                &|_| {},
            );
            let run = |total| DeleteRun {
                target: NodeId::ROOT,
                path: PathBuf::from("/synthetic/root/Videos"),
                total_files: total,
                counted_files: 0,
                removed_files: 12_400,
                removed_dirs: 0,
            };
            add(
                "quit_deleting_known",
                &|_| Dialog::QuitWhileDeleting,
                &|s| s.delete_run = Some(run(Some(48_212))),
            );
            add(
                "quit_deleting_unknown",
                &|_| Dialog::QuitWhileDeleting,
                &|s| s.delete_run = Some(run(None)),
            );
            add("quit", &|_| Dialog::Quit, &|_| {});
            for (name, mut state) in shots {
                let screen = draw(&mut state);
                // UX §5.3: text never touches the right border (2-cell margin on both sides).
                if name != "failed_paths" {
                    for row in inner_rows(&screen) {
                        assert!(
                            row.chars().count() == 62 && row.ends_with("  "),
                            "{name} {tag}: {row:?}"
                        );
                    }
                }
                insta::assert_snapshot!(format!("delete_{name}_{tag}"), screen);
            }
        }
    }

    /// UX §7.1: with `>=` the ASCII SIZE column is 8 cells wide and every column still lines up.
    #[test]
    fn ascii_size_column_holds_the_two_cell_lower_bound() {
        use storage_analyzer::types::IssueKind;
        let mut offsets = Vec::new();
        let mut bar_cols = Vec::new();
        for ascii in [false, true] {
            let mut scan = Scan::new();
            scan.list(
                NodeId::ROOT,
                vec![
                    folder("Skip")
                        .issue(IssueKind::SkippedByUser)
                        .bytes(5_000_000),
                    file("f.bin", 9_000_000),
                ],
            );
            let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
            let mut state = AppState::new(config, scan.model);
            if ascii {
                state.glyphs = Glyphs::ascii();
            }
            let screen = draw(&mut state);
            let rows: Vec<&str> = screen.lines().collect();
            let header = rows.iter().find(|r| r.contains("NAME")).unwrap();
            let name_col = |row: &str, name: &str| {
                let at = row
                    .find(name)
                    .unwrap_or_else(|| panic!("{name} in {row:?}"));
                row[..at].chars().count()
            };
            let col_of = |name: &str| {
                let row = rows.iter().find(|r| r.contains(name)).unwrap();
                name_col(row, name)
            };
            // The root row is full: its bar is the reference for the headings.
            let full = state.glyphs.bar_full;
            let root = rows.iter().find(|r| r.contains(full)).unwrap();
            bar_cols.push(name_col(root, full));
            offsets.push(name_col(root, full) as isize - name_col(header, "OF PARENT") as isize);
            let skip = rows.iter().find(|r| r.contains("Skip/")).unwrap();
            let bound = if ascii { ">=" } else { "≥" };
            assert!(skip.contains(&format!(" {bound}4.8M ")), "{skip:?}");
            assert_eq!(col_of("Skip/"), col_of("f.bin"), "ascii={ascii}\n{screen}");
        }
        // The headings move with the columns: same distance to the bar in both modes.
        assert_eq!(offsets[0], offsets[1], "{offsets:?}");
        // ASCII spends one more cell on SIZE, so everything after it moves right by one.
        assert_eq!(bar_cols[1], bar_cols[0] + 1, "{bar_cols:?}");
    }

    /// The unfinished dialog says `>=` in ASCII, never a lone `>` before a size.
    #[test]
    fn ascii_unfinished_dialog_spells_the_lower_bound_out() {
        let mut state = scene(true);
        state.dialog = Some(prompt(&state, "Projects", false));
        let screen = draw(&mut state);
        assert!(screen.contains(">= 19.1 MiB"), "{screen}");
        assert!(!screen.contains("19.1M "), "{screen}");
        assert!(screen.contains("19.1 MiB  repo.zip"), "{screen}");
        assert!(!screen.contains("- > "), "{screen}");
    }

    /// The inner rows of the open dialog: the text between the border cells.
    fn inner_rows(screen: &str) -> Vec<String> {
        screen
            .lines()
            .filter_map(|row| {
                let (a, b) = (row.find('║')?, row.rfind('║')?);
                (a != b).then(|| row[a + '║'.len_utf8()..b].to_owned())
            })
            .collect()
    }

    /// UX §5.3: wrapped lines keep the 2-cell inner margin; list items stay two cells further
    /// in, never wrap and end in `…`.
    #[test]
    fn wrapped_lines_keep_the_margin_and_list_items_truncate() {
        let mut state = scene(false);
        state.dialog = Some(Dialog::DeleteAborted(DeleteAborted {
            path: PathBuf::from("/synthetic/root/Videos"),
            reason: AbortReason::MountPoint,
        }));
        let rows = inner_rows(&draw(&mut state));
        assert!(rows.len() > 6, "the reason text wraps: {rows:?}");
        for row in rows.iter().filter(|r| !r.trim().is_empty()) {
            assert!(row.starts_with("  ") && !row.starts_with("   "), "{row:?}");
        }

        let mut scan = Scan::new();
        let kids = scan.list(NodeId::ROOT, vec![folder("Long"), folder("Other")]);
        let long = "x".repeat(90);
        scan.list(kids[0], vec![file(&long, 1_000_000), folder("open")]);
        let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
        let mut state = AppState::new(config, scan.model);
        state.dialog = Some(prompt(&state, "Long", false));
        let rows = inner_rows(&draw(&mut state));
        let item = rows.iter().find(|r| r.contains("xxxx")).expect("the item");
        assert!(
            item.starts_with("    ") && item.trim_end().ends_with('…'),
            "{item:?}"
        );
        assert_eq!(
            rows.iter().filter(|r| r.contains('x')).count(),
            1,
            "{rows:?}"
        );
    }

    /// A crafted name cannot display reversed: the confirmation shows the escape, not U+202E.
    #[test]
    fn a_bidi_override_in_the_path_is_escaped_in_the_confirmation() {
        let mut scan = Scan::new();
        scan.list(NodeId::ROOT, vec![folder("evil\u{202E}txt.exe")]);
        scan.list(NodeId(1), vec![file("x", 10)]);
        let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
        let mut state = AppState::new(config, scan.model);
        state.dialog = Some(prompt(&state, "evil\u{202E}txt.exe", false));
        let screen = draw(&mut state);
        assert!(!screen.contains('\u{202E}'), "{screen}");
        assert!(screen.contains("evil\\u{202E}txt.exe"), "{screen}");
    }
}
