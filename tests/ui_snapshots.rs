//! 80×24 snapshots of every screen, in Unicode and in the ASCII fallback (UX §11 check 1).
//! Dialog snapshots belong to T021.

#[path = "support/synthetic.rs"]
mod synthetic;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::state::AppState;
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, NodeId};
use storage_analyzer::ui;
use storage_analyzer::ui::theme::Glyphs;
use synthetic::{Scan, file, folder};

const ROOT: NodeId = NodeId::ROOT;

fn screen(state: &mut AppState, w: u16, h: u16) -> String {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn state(scan: Scan, finish: Option<bool>, ascii: bool, view: char) -> AppState {
    let config = cli::parse(APP_ID, [APP_ID, "--no-animation"]).unwrap();
    let mut state = AppState::new(config, scan.model);
    if let Some(cancelled) = finish {
        state.model.apply(ScanMsg::Done { cancelled });
    }
    if ascii {
        state.glyphs = Glyphs::ascii();
    }
    state.handle_key(KeyEvent::new(KeyCode::Char(view), KeyModifiers::NONE));
    state
}

fn showcase() -> Scan {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            folder(".cache"),
            folder("Videos"),
            folder(".local").issue(IssueKind::SkippedByUser),
            folder("Projects"),
            folder("Downloads").issue(IssueKind::PermissionDenied),
            file("notes.txt", 300_000),
        ],
    );
    scan.list(
        kids[0],
        vec![
            folder("yay"),
            folder("mozilla"),
            file("misc.bin", 1_000_000),
        ],
    );
    scan.list(kids[1], vec![file("movie.mkv", 40_000_000)]);
    scan.list(kids[3], vec![file("repo.zip", 20_000_000)]);
    scan
}

fn empty() -> Scan {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![]);
    scan
}

#[test]
fn every_screen_in_unicode_and_ascii() {
    for ascii in [false, true] {
        let tag = if ascii { "ascii" } else { "unicode" };
        let scenes: [(&str, Scan, Option<bool>, char); 9] = [
            ("scanning", showcase(), None, '1'),
            ("done", showcase(), Some(false), '1'),
            ("stopped", showcase(), Some(true), '1'),
            ("empty", empty(), Some(false), '1'),
            ("types", showcase(), Some(false), '2'),
            ("largest", showcase(), Some(false), '3'),
            ("skipped", showcase(), Some(false), '4'),
            ("scanning_types", showcase(), None, '2'),
            ("stopped_largest", showcase(), Some(true), '3'),
        ];
        for (name, scan, finish, view) in scenes {
            let mut s = state(scan, finish, ascii, view);
            insta::assert_snapshot!(format!("{name}_{tag}"), screen(&mut s, 80, 24));
        }
        let mut help = state(showcase(), None, ascii, '1');
        help.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        insta::assert_snapshot!(format!("help_{tag}"), screen(&mut help, 80, 24));
        let mut small = state(showcase(), None, ascii, '1');
        insta::assert_snapshot!(format!("too_small_{tag}"), screen(&mut small, 79, 24));
    }
}
