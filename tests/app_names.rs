//! Hostile file names end to end: real files with control characters, invalid UTF-8 and bidi
//! overrides in their names are scanned by the real walker and must never reach the screen raw.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::event_loop::{App, Clock, EventSource};
use storage_analyzer::app::state::{AppState, Dialog};
use storage_analyzer::app::terminal::PanicSlot;
use storage_analyzer::app::{preflight, production_starter};
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
struct Keys(Rc<RefCell<VecDeque<Event>>>);

impl EventSource for Keys {
    fn next(&mut self, _timeout: Duration) -> io::Result<Option<Event>> {
        Ok(self.0.borrow_mut().pop_front())
    }
}

/// The names under test, as raw bytes.
const ESC_FILE: &[u8] = b"esc\x1b[2Jfile.txt";
const BEL_DIR: &[u8] = b"bel\x07dir";
const C1_FILE: &[u8] = "c1\u{85}\u{9b}file".as_bytes();
const BAD_UTF8_FILE: &[u8] = b"bad\xff\xfename.bin";
const BIDI_FILE: &[u8] = "evil\u{202E}txt.exe".as_bytes();
const LOCKED_DIR: &[u8] = b"locked\x1b]0;title\x07dir";

struct Rig {
    app: App<TestBackend, Keys, TestClock>,
    keys: Keys,
    clock: TestClock,
    _dir: tempfile::TempDir,
}

fn put_file(dir: &std::path::Path, name: &[u8], len: usize) {
    std::fs::write(dir.join(OsStr::from_bytes(name)), vec![1u8; len]).unwrap();
}

fn rig() -> Rig {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    put_file(p, ESC_FILE, 30_000);
    put_file(p, C1_FILE, 20_000);
    put_file(p, BAD_UTF8_FILE, 10_000);
    put_file(p, BIDI_FILE, 5_000);
    std::fs::create_dir(p.join(OsStr::from_bytes(BEL_DIR))).unwrap();
    put_file(&p.join(OsStr::from_bytes(BEL_DIR)), b"inside.bin", 40_000);
    let locked = p.join(OsStr::from_bytes(LOCKED_DIR));
    std::fs::create_dir(&locked).unwrap();
    put_file(&locked, b"secret", 100);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let root = preflight(p).unwrap();
    let model = Model::new(root.path, root.entry, ModelOptions { top_n: 100 });
    let config = cli::parse(APP_ID, [APP_ID, "--no-animation", "--threads", "1"]).unwrap();
    let (clock, keys) = (TestClock::default(), Keys::default());
    let app = App::new(
        Terminal::new(TestBackend::new(80, 24)).unwrap(),
        keys.clone(),
        clock.clone(),
        AppState::new(config, model),
        production_starter(),
        Arc::new(AtomicBool::new(false)),
        PanicSlot::default(),
    );
    let mut rig = Rig {
        app,
        keys,
        clock,
        _dir: dir,
    };
    rig.app.start().unwrap();
    let started = Instant::now();
    while rig.app.state().is_scanning() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "scan timed out"
        );
        std::thread::sleep(Duration::from_millis(3));
        rig.step();
    }
    rig
}

impl Drop for Rig {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let locked = self._dir.path().join(OsStr::from_bytes(LOCKED_DIR));
        let _ = std::fs::set_permissions(locked, std::fs::Permissions::from_mode(0o700));
    }
}

impl Rig {
    fn step(&mut self) {
        self.clock
            .0
            .set(self.clock.0.get() + Duration::from_millis(100));
        self.app.step().unwrap();
    }

    fn key(&mut self, code: KeyCode) {
        self.keys
            .0
            .borrow_mut()
            .push_back(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        self.step();
    }

    /// Every cell of the last frame, row by row.
    fn cells(&self) -> Vec<String> {
        let buf = self.app.terminal().backend().buffer();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect()
            })
            .collect()
    }

    fn screen(&self) -> String {
        self.cells().join("\n")
    }

    fn select_named(&mut self, name: &[u8]) {
        let model = &self.app.state().model;
        let id = *model
            .children(NodeId::ROOT)
            .iter()
            .find(|&&c| model.exists(c) && model.name(c) == name)
            .expect("the name was scanned");
        let st = self.app.state_mut();
        st.tree.selection = id;
        st.tree.aggregate = None;
        st.tree.on_parent = false;
        st.tree.expanded.insert(NodeId::ROOT);
        self.step();
    }
}

/// No cell holds a control character or a bidi control, whatever the view.
fn assert_clean(r: &Rig, what: &str) {
    for (y, row) in r.cells().iter().enumerate() {
        for c in row.chars() {
            let bidi = matches!(
                c,
                '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
            );
            assert!(
                !c.is_control() && !bidi,
                "{what}: raw {c:?} on row {y}:\n{}",
                r.screen()
            );
        }
    }
}

#[test]
fn hostile_names_never_reach_the_screen_raw_in_any_view_or_the_delete_confirmation() {
    if rustix::process::geteuid().is_root() {
        return; // root can read the locked directory, so there is no Skipped row to show
    }
    let mut r = rig();

    // Tree and map (both are on the first screen).
    r.key(KeyCode::Char('1'));
    assert_clean(&r, "tree and map");
    let tree = r.screen();
    assert!(tree.contains("esc\\x1b[2Jfile.txt"), "{tree}");
    assert!(tree.contains("bel\\x07dir/"), "{tree}");
    assert!(tree.contains("c1\\x85\\x9bfile"), "{tree}");
    assert!(tree.contains("evil\\u{202E}txt.exe"), "{tree}");
    assert!(tree.contains("bad\u{fffd}\u{fffd}name.bin"), "{tree}");

    // Largest shows full paths.
    r.key(KeyCode::Char('3'));
    assert_clean(&r, "largest");
    assert!(r.screen().contains("esc\\x1b[2Jfile.txt"), "{}", r.screen());

    // Skipped shows the unreadable folder.
    r.key(KeyCode::Char('4'));
    assert_clean(&r, "skipped");
    assert!(
        r.screen().contains("locked\\x1b]0;title\\x07dir"),
        "{}",
        r.screen()
    );

    // The delete confirmation names the path.
    r.key(KeyCode::Char('1'));
    r.select_named(BIDI_FILE);
    r.key(KeyCode::Char('d'));
    assert!(matches!(r.app.state().dialog, Some(Dialog::Delete(_))));
    assert_clean(&r, "delete confirmation (bidi)");
    assert!(
        r.screen().contains("evil\\u{202E}txt.exe"),
        "{}",
        r.screen()
    );
    r.key(KeyCode::Esc);
    r.select_named(ESC_FILE);
    r.key(KeyCode::Char('d'));
    assert_clean(&r, "delete confirmation (ESC)");
    assert!(r.screen().contains("esc\\x1b[2Jfile.txt"), "{}", r.screen());
}
