//! Types, Largest and Skipped views (T018).

#[path = "support/synthetic.rs"]
mod synthetic;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::list_state::View;
use storage_analyzer::app::state::AppState;
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, Kind, NodeId};
use storage_analyzer::ui;
use synthetic::{Scan, file, folder};

const ROOT: NodeId = NodeId::ROOT;

fn app_state(scan: Scan) -> AppState {
    AppState::new(cli::parse(APP_ID, [APP_ID]).unwrap(), scan.model)
}

fn draw(state: &mut AppState, w: u16, h: u16) -> Vec<String> {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(w))
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect()
}

/// Column header plus the list pane, i.e. what the active view draws.
fn view_part(screen: &[String], w: u16, h: u16) -> Vec<String> {
    let r = ui::regions(Rect::new(0, 0, w, h));
    screen[usize::from(r.column_header.y)..usize::from(r.main.bottom())].to_vec()
}

fn press(state: &mut AppState, codes: &[KeyCode]) {
    for &code in codes {
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

fn row_with<'a>(rows: &'a [String], needle: &str) -> &'a str {
    rows.iter()
        .find(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?} in:\n{}", rows.join("\n")))
}

fn in_order(rows: &[String], names: &[&str]) -> bool {
    let pos: Vec<usize> = names
        .iter()
        .map(|n| {
            rows.iter()
                .position(|r| r.contains(n))
                .unwrap_or(usize::MAX)
        })
        .collect();
    pos.windows(2).all(|w| w[0] < w[1])
}

/// Independent recount: (extension, bytes) per file, grouped by hand-written categories.
const FILES: [(&str, u64); 8] = [
    ("a.mkv", 5000),
    ("b.mp4", 3000),
    ("c.png", 700),
    ("d.zip", 400),
    ("e.rs", 100),
    ("f.rs", 50),
    ("noext", 20),
    ("g.pdf", 10),
];

fn typed_scan() -> Scan {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("sub"), file(FILES[0].0, FILES[0].1)]);
    scan.list(
        kids[0],
        FILES[1..].iter().map(|(n, b)| file(n, *b)).collect(),
    );
    scan
}

#[test]
fn tabs_mark_the_active_view_and_count_issues() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        vec![
            folder("x").issue(IssueKind::PermissionDenied),
            folder("y").issue(IssueKind::Vanished),
            file("z", 5),
        ],
    );
    let mut state = app_state(scan);
    let screen = draw(&mut state, 80, 24);
    assert!(
        screen[1].contains("[1 Tree]  2 Types  3 Largest  4 Skipped !2"),
        "{}",
        screen[1]
    );
    press(&mut state, &[KeyCode::Char('2')]);
    assert_eq!(state.view, View::Types);
    let screen = draw(&mut state, 80, 24);
    assert!(
        screen[1].contains("1 Tree  [2 Types]  3 Largest  4 Skipped !2"),
        "{}",
        screen[1]
    );
    press(
        &mut state,
        &[KeyCode::Char('3'), KeyCode::Char('4'), KeyCode::Char('1')],
    );
    assert_eq!(state.view, View::Tree);
}

#[test]
fn types_view_matches_a_brute_force_recount_of_the_whole_root() {
    let mut state = app_state(typed_scan());
    press(&mut state, &[KeyCode::Char('2')]);
    let screen = draw(&mut state, 80, 24);
    let part = view_part(&screen, 80, 24);

    // (category, bytes, files) by hand from FILES.
    let video = (5000 + 3000, 2);
    let image = (700, 1);
    let archive = (400, 1);
    let code = (150, 2);
    let binary = (20, 1);
    let docs = (10, 1);
    let total: u64 = FILES.iter().map(|f| f.1).sum();
    for (name, (bytes, count)) in [
        ("Video", video),
        ("Images", image),
        ("Archives", archive),
        ("Code", code),
        ("Binary", binary),
        ("Documents", docs),
    ] {
        let row = row_with(&part, name);
        assert!(row.contains(&ui::format::size_iec(bytes)), "{name}: {row}");
        assert!(row.contains(&format!(" {count} ")), "{name} count: {row}");
        let pct = ui::format::percent_tree(bytes as f64 / total as f64);
        assert!(row.contains(&pct), "{name} {pct}: {row}");
    }
    assert!(in_order(
        &part,
        &["Video", "Images", "Archives", "Code", "Binary", "Documents"]
    ));
}

#[test]
fn expanding_a_category_lists_its_extensions_and_collapsing_hides_them() {
    let mut state = app_state(typed_scan());
    press(&mut state, &[KeyCode::Char('2'), KeyCode::Right]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        row_with(&part, ".mkv").contains(&ui::format::size_iec(5000)),
        "{}",
        part.join("\n")
    );
    assert!(row_with(&part, ".mp4").contains(&ui::format::size_iec(3000)));
    assert!(in_order(&part, &["Video", ".mkv", ".mp4", "Images"]));
    press(&mut state, &[KeyCode::Left]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(!part.iter().any(|r| r.contains(".mkv")));

    // `(none)` and `(other)` are labelled.
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        vec![file("plain", 10), file("odd.thisextensionistoolong", 5)],
    );
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('2')]);
    let rows = |state: &mut AppState| view_part(&draw(state, 80, 24), 80, 24);
    press(&mut state, &[KeyCode::Right]); // Binary: the extensionless file
    assert!(row_with(&rows(&mut state), "(none)").contains("10B"));
    press(&mut state, &[KeyCode::Left, KeyCode::Down, KeyCode::Right]);
    assert!(row_with(&rows(&mut state), "(other)").contains("5B"));
}

#[test]
fn types_ignore_the_tree_selection() {
    let mut state = app_state(typed_scan());
    press(&mut state, &[KeyCode::Char('2')]);
    let before = view_part(&draw(&mut state, 80, 24), 80, 24);
    press(
        &mut state,
        &[
            KeyCode::Char('1'),
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Char('2'),
        ],
    );
    assert_ne!(state.tree.selection, ROOT);
    let after = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert_eq!(before, after);
}

#[test]
fn types_change_live_while_scanning() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("a"), folder("b")]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('2')]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        part.iter().any(|r| r.contains("No files yet.")),
        "{}",
        part.join("\n")
    );

    state.model.apply(synthetic::listing(
        kids[0],
        10,
        vec![synthetic::entry("v.mkv", Kind::File, 900)],
    ));
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        row_with(&part, "Video").contains("900B"),
        "{}",
        part.join("\n")
    );
    state.model.apply(synthetic::listing(
        kids[1],
        11,
        vec![synthetic::entry("w.mkv", Kind::File, 100)],
    ));
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        row_with(&part, "Video").contains("1000B"),
        "{}",
        part.join("\n")
    );
}

#[test]
fn done_without_files_says_no_files_found_and_stopped_marks_partial() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![]);
    let mut state = app_state(scan);
    state.model.apply(ScanMsg::Done { cancelled: false });
    for key in ['2', '3'] {
        press(&mut state, &[KeyCode::Char(key)]);
        let part = view_part(&draw(&mut state, 80, 24), 80, 24);
        assert!(
            part.iter().any(|r| r.contains("No files found.")),
            "{}",
            part.join("\n")
        );
        assert!(!part[0].contains("partial"));
    }
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("a.mkv", 5)]);
    let mut stopped = app_state(scan);
    stopped.model.apply(ScanMsg::Done { cancelled: true });
    for key in ['2', '3'] {
        press(&mut stopped, &[KeyCode::Char(key)]);
        let part = view_part(&draw(&mut stopped, 80, 24), 80, 24);
        assert!(part[0].contains("partial"), "{}", part[0]);
    }
}

fn largest_names(state: &mut AppState) -> Vec<String> {
    let part = view_part(&draw(state, 100, 30), 100, 30);
    part.iter()
        .filter(|r| r.contains("/synthetic/root/"))
        .map(|r| r.rsplit('/').next().unwrap().trim_end().to_owned())
        .collect()
}

#[test]
fn largest_is_live_whole_root_and_limited_to_top_n() {
    let mut scan = Scan::with_top(3);
    let kids = scan.list(
        ROOT,
        vec![folder("p"), folder("q"), file("one", 10), file("two", 20)],
    );
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('3')]);
    assert_eq!(largest_names(&mut state), ["two", "one"]);

    state.model.apply(synthetic::listing(
        kids[0],
        10,
        vec![
            synthetic::entry("deep", Kind::File, 500),
            synthetic::entry("mid", Kind::File, 15),
        ],
    ));
    assert_eq!(
        largest_names(&mut state),
        ["deep", "two", "mid"],
        "exactly --top 3 rows"
    );
    // The tree selection does not matter: files from every subtree compete.
    press(
        &mut state,
        &[KeyCode::Char('1'), KeyCode::Down, KeyCode::Char('3')],
    );
    state.model.apply(synthetic::listing(
        kids[1],
        12,
        vec![synthetic::entry("huge", Kind::File, 900)],
    ));
    assert_eq!(
        largest_names(&mut state),
        ["huge", "deep", "two"],
        "the old last one dropped out"
    );
}

#[test]
fn long_paths_are_cut_from_the_left_so_the_file_name_stays() {
    let mut scan = Scan::new();
    let a = scan.list(ROOT, vec![folder(&"deep_directory_name_".repeat(3))]);
    let b = scan.list(a[0], vec![folder(&"another_long_directory_".repeat(3))]);
    scan.list(b[0], vec![file("precious_file_name.bin", 99)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('3')]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    let row = row_with(&part, "precious_file_name.bin");
    assert!(row.contains("…"), "{row}");
    assert!(row.trim_end().ends_with("precious_file_name.bin"), "{row}");
    assert!(!row.contains("synthetic"), "the left part was cut: {row}");
}

#[test]
fn enter_on_largest_shows_the_file_in_the_tree() {
    let mut scan = Scan::new();
    let a = scan.list(ROOT, vec![folder("a"), file("small", 1)]);
    let b = scan.list(a[0], vec![folder("b")]);
    let c = scan.list(b[0], vec![folder("c")]);
    let deep = scan.list(c[0], vec![file("treasure.bin", 777)]);
    let mut state = app_state(scan);
    // Zoom elsewhere first: showing in the tree goes back to the scan root.
    press(
        &mut state,
        &[
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('3'),
            KeyCode::Enter,
        ],
    );
    assert_eq!(state.view, View::Tree);
    assert_eq!(state.tree.selection, deep[0]);
    assert_eq!(state.tree.view_root, ROOT);
    for ancestor in [a[0], b[0], c[0]] {
        assert!(
            state.tree.expanded.contains(&ancestor),
            "{ancestor:?} expanded"
        );
    }
    let screen = draw(&mut state, 100, 30);
    let r = ui::regions(Rect::new(0, 0, 100, 30));
    let pane = &screen[usize::from(r.main.y)..usize::from(r.main.bottom())];
    assert!(
        pane.iter()
            .any(|row| row.starts_with('>') && row.contains("treasure.bin")),
        "{}",
        pane.join("\n")
    );
}

#[test]
fn list_cursor_moves_with_keys() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        (0..40)
            .map(|i| file(&format!("f{i:02}"), 1000 - i))
            .collect(),
    );
    let mut state = app_state(scan);
    press(
        &mut state,
        &[
            KeyCode::Char('3'),
            KeyCode::Down,
            KeyCode::Char('j'),
            KeyCode::Down,
        ],
    );
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(row_with(&part, ">").contains("f03"), "{}", part.join("\n"));
    press(&mut state, &[KeyCode::Char('k')]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(row_with(&part, ">").contains("f02"));
    press(&mut state, &[KeyCode::Char('G')]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(row_with(&part, ">").contains("f39"), "{}", part.join("\n"));
    press(&mut state, &[KeyCode::Char('g'), KeyCode::PageDown]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(!row_with(&part, ">").contains("f00"));
    press(&mut state, &[KeyCode::PageUp]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(row_with(&part, ">").contains("f00"));
}

#[test]
fn skipped_lists_every_reason_newest_first() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        vec![
            folder("denied").issue(IssueKind::PermissionDenied),
            folder("gone").issue(IssueKind::Vanished),
            folder("mnt").issue(IssueKind::OtherFilesystem),
            folder("mine").issue(IssueKind::SkippedByUser),
            folder("broken").issue(IssueKind::Other(5)),
        ],
    );
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('4')]);
    let screen = draw(&mut state, 100, 24);
    assert!(
        screen[1].contains("4 Skipped !5") || screen[1].contains("[4 Skipped] !5"),
        "{}",
        screen[1]
    );
    let part = view_part(&screen, 100, 24);
    for (path, reason) in [
        ("broken", "read error: input/output error"),
        ("mine", "skipped by you"),
        ("mnt", "other filesystem"),
        ("gone", "vanished during scan"),
        ("denied", "permission denied"),
    ] {
        let row = row_with(&part, &format!("/synthetic/root/{path}"));
        assert!(row.contains(reason), "{path}: {row}");
    }
    assert!(
        in_order(&part, &["broken", "mine", "mnt", "gone", "denied"]),
        "newest first:\n{}",
        part.join("\n")
    );
}

#[test]
fn skipped_says_nothing_skipped_when_empty() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("a", 1)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('4')]);
    let part = view_part(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        part.iter().any(|r| r.contains("Nothing skipped.")),
        "{}",
        part.join("\n")
    );
}

#[test]
fn list_views_show_the_list_key_hints() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("a", 1)]);
    let mut state = app_state(scan);
    for key in ['2', '3', '4'] {
        press(&mut state, &[KeyCode::Char(key)]);
        let screen = draw(&mut state, 80, 24);
        let hints = &screen[23];
        assert!(
            hints.contains("⏎ show in tree") && hints.contains("? help"),
            "{hints}"
        );
    }
}

fn showcase() -> Scan {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            folder("Videos"),
            folder("Projects"),
            file("notes.txt", 3000),
            folder("locked").issue(IssueKind::PermissionDenied),
            folder("skipped").issue(IssueKind::SkippedByUser),
        ],
    );
    scan.list(kids[0], vec![file("movie.mkv", 40_000)]);
    scan.list(
        kids[1],
        vec![file("repo.zip", 20_000), file("main.rs", 6000)],
    );
    scan
}

#[test]
fn snapshots_of_each_view_in_each_state() {
    for (view, key) in [("types", '2'), ("largest", '3'), ("skipped", '4')] {
        let scenes: [(&str, Scan, Option<bool>); 4] = [
            ("starting", Scan::new(), None),
            ("scanning", showcase(), None),
            ("done", showcase(), Some(false)),
            ("stopped", showcase(), Some(true)),
        ];
        for (name, scan, finish) in scenes {
            let mut state = app_state(scan);
            if let Some(cancelled) = finish {
                state.model.apply(ScanMsg::Done { cancelled });
            }
            press(&mut state, &[KeyCode::Char(key)]);
            let part = view_part(&draw(&mut state, 80, 24), 80, 24);
            insta::assert_snapshot!(format!("{view}_{name}"), part.join("\n"));
        }
        let mut empty = Scan::new();
        empty.list(ROOT, vec![]);
        let mut state = app_state(empty);
        state.model.apply(ScanMsg::Done { cancelled: false });
        press(&mut state, &[KeyCode::Char(key)]);
        let part = view_part(&draw(&mut state, 80, 24), 80, 24);
        insta::assert_snapshot!(format!("{view}_empty"), part.join("\n"));
    }
}

fn draw_buf(state: &mut AppState, w: u16, h: u16) -> ratatui::buffer::Buffer {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    terminal.backend().buffer().clone()
}

fn swatch_cells(buf: &ratatui::buffer::Buffer, y: u16) -> [ratatui::style::Color; 2] {
    let row: Vec<String> = (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect();
    let twisty = row
        .iter()
        .position(|s| s == "▸" || s == "▾")
        .expect("a twisty");
    [
        buf[(twisty as u16 - 3, y)].bg,
        buf[(twisty as u16 - 2, y)].bg,
    ]
}

fn row_y(buf: &ratatui::buffer::Buffer, needle: &str) -> u16 {
    (0..buf.area.height)
        .find(|&y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect::<String>()
                .contains(needle)
        })
        .unwrap_or_else(|| panic!("no row with {needle}"))
}

#[test]
fn types_rows_have_a_two_cell_swatch_in_the_category_colour_and_a_tag() {
    use ratatui::style::Color;
    let mut state = app_state(typed_scan());
    press(&mut state, &[KeyCode::Char('2'), KeyCode::Down]);
    let buf = draw_buf(&mut state, 80, 24);
    let video = row_y(&buf, "Video");
    let video_row: String = (0..80)
        .map(|x| buf[(x, video)].symbol().to_owned())
        .collect();
    assert!(video_row.contains("[V]"), "{video_row}");
    assert_eq!(
        swatch_cells(&buf, video),
        [Color::Rgb(0xE6, 0x9F, 0x00); 2],
        "video is orange"
    );
    let code = row_y(&buf, "Code");
    assert_eq!(
        swatch_cells(&buf, code),
        [Color::Rgb(0xF0, 0xE4, 0x42); 2],
        "code is yellow"
    );
    // The selected row keeps its swatch even though the row is reversed.
    press(&mut state, &[KeyCode::Up]);
    let buf = draw_buf(&mut state, 80, 24);
    assert_eq!(
        swatch_cells(&buf, row_y(&buf, "Video")),
        [Color::Rgb(0xE6, 0x9F, 0x00); 2]
    );
}

#[test]
fn without_colours_the_types_view_has_no_swatch_but_keeps_the_tag() {
    use ratatui::style::Color;
    let mut state = app_state(typed_scan());
    state.theme.depth = ui::theme::ColorDepth::Mono;
    press(&mut state, &[KeyCode::Char('2')]);
    let buf = draw_buf(&mut state, 80, 24);
    let y = row_y(&buf, "Video");
    assert_eq!(swatch_cells(&buf, y), [Color::Reset; 2]);
    let row: String = (0..80).map(|x| buf[(x, y)].symbol().to_owned()).collect();
    assert!(row.contains("[V]"), "{row}");
}

// ---- Largest `updating…` (UX §5.5, check 22) ----

mod updating {
    use std::time::Duration;

    use storage_analyzer::delete::{DeleteEvent, Outcome};
    use storage_analyzer::types::Counts;

    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    /// Folder `a` has `a_files` big files, `b` has 2 middling ones and `c` has 20 tiny ones; the
    /// lists keep the top 3 plus a reserve, so deleting `a` forces a stepped rebuild.
    fn scene(a_files: u64, glyphs_ascii: bool) -> (AppState, NodeId) {
        let mut scan = Scan::with_top(3);
        let kids = scan.list(ROOT, vec![folder("a"), folder("b"), folder("c")]);
        scan.list(
            kids[0],
            (0..a_files)
                .map(|i| file(&format!("a{i:02}"), 100_000 + i * 1000))
                .collect(),
        );
        scan.list(kids[1], vec![file("b1", 5_000), file("b2", 4_000)]);
        scan.list(
            kids[2],
            (0..20).map(|i| file(&format!("c{i:02}"), 10 + i)).collect(),
        );
        let mut state = app_state(scan);
        if glyphs_ascii {
            state.glyphs = storage_analyzer::ui::theme::Glyphs::ascii();
        }
        press(&mut state, &[KeyCode::Char('3')]);
        draw(&mut state, 100, 30);
        (state, kids[0])
    }

    fn delete(state: &mut AppState, id: NodeId) {
        state.model.begin_delete(id).unwrap();
        state
            .model
            .apply_delete(&DeleteEvent::Finished(Outcome::Removed(Counts::default())));
    }

    fn finish_rebuild(state: &mut AppState) {
        while !state.model.step_top_rebuild(Duration::from_millis(5)) {}
    }

    fn header(state: &mut AppState, w: u16) -> String {
        let screen = draw(state, w, 24);
        screen[usize::from(ui::regions(Rect::new(0, 0, w, 24)).column_header.y)].clone()
    }

    #[test]
    fn the_text_waits_200_ms_then_shows_right_aligned_and_the_old_list_minus_deleted_stays() {
        let (mut state, a) = scene(13, false);
        delete(&mut state, a);
        assert!(
            state.model.top_updating(),
            "the delete must start a rebuild"
        );
        state.tick(at(0));
        state.tick(at(100));
        let head = header(&mut state, 80);
        assert!(!head.contains("updating"), "{head:?}");
        state.tick(at(250));
        let head = header(&mut state, 80);
        assert!(head.ends_with("updating… "), "{head:?}");
        assert!(head.contains("SIZE  PATH"), "{head:?}");
        assert!(storage_analyzer::ui::format::width(&head) <= 80);
        // The old list minus the deleted folder: only b's files, no gap, no empty state.
        let names = largest_names(&mut state);
        assert_eq!(names, ["b1", "b2"], "{names:?}");
        // The new list replaces it in one frame and the text goes away.
        finish_rebuild(&mut state);
        state.tick(at(300));
        let head = header(&mut state, 80);
        assert!(!head.contains("updating"), "{head:?}");
        assert_eq!(largest_names(&mut state), ["b1", "b2", "c19"]);
    }

    #[test]
    fn ascii_spells_it_with_three_dots() {
        let (mut state, a) = scene(13, true);
        delete(&mut state, a);
        state.tick(at(0));
        state.tick(at(250));
        let head = header(&mut state, 80);
        assert!(head.ends_with("updating... "), "{head:?}");
    }

    #[test]
    fn a_rebuild_under_200_ms_never_shows_the_text() {
        let (mut state, a) = scene(13, false);
        delete(&mut state, a);
        state.tick(at(0));
        state.tick(at(150));
        assert!(!header(&mut state, 80).contains("updating"));
        finish_rebuild(&mut state);
        state.tick(at(160));
        state.tick(at(400));
        assert!(!header(&mut state, 80).contains("updating"));
    }

    #[test]
    fn enter_on_a_row_still_shows_it_in_the_tree_while_updating() {
        let (mut state, a) = scene(13, false);
        delete(&mut state, a);
        state.tick(at(0));
        state.tick(at(250));
        draw(&mut state, 100, 30);
        press(&mut state, &[KeyCode::Enter]);
        assert_eq!(state.view, View::Tree);
        assert_eq!(state.model.name(state.tree.selection), b"b1");
    }

    #[test]
    fn every_visible_row_deleted_shows_the_text_at_once_and_no_false_empty_state() {
        let (mut state, a) = scene(40, false);
        delete(&mut state, a);
        assert!(state.model.top_updating());
        state.tick(at(0));
        state.tick(at(10));
        let screen = draw(&mut state, 80, 24);
        let part = view_part(&screen, 80, 24);
        assert!(part[0].ends_with("updating… "), "{:?}", part[0]);
        assert!(
            part[1..].iter().all(|r| r.trim().is_empty()),
            "no rows and no `No files found.`: {part:?}"
        );
        assert_eq!(state.lists.selected_node(View::Largest), None);
        for key in [KeyCode::Enter, KeyCode::Char('d')] {
            state.transient = None;
            press(&mut state, &[key]);
            assert_eq!(
                state.transient.as_ref().map(|t| t.text.as_str()),
                Some("Nothing to delete here."),
                "{key:?}"
            );
            assert!(state.dialog.is_none());
        }
        finish_rebuild(&mut state);
        state.tick(at(20));
        let part = view_part(&draw(&mut state, 80, 24), 80, 24);
        assert!(!part[0].contains("updating"), "{:?}", part[0]);
        assert!(part[1].contains("b1"), "{part:?}");
    }

    #[test]
    fn a_list_that_really_is_empty_says_so_when_the_rebuild_ends() {
        let mut scan = Scan::with_top(3);
        let kids = scan.list(ROOT, vec![folder("a")]);
        scan.list(
            kids[0],
            (0..40)
                .map(|i| file(&format!("a{i:02}"), 1000 + i))
                .collect(),
        );
        let mut state = app_state(scan);
        state.model.apply(ScanMsg::Done { cancelled: false });
        press(&mut state, &[KeyCode::Char('3')]);
        draw(&mut state, 80, 24);
        delete(&mut state, kids[0]);
        state.tick(at(0));
        let part = view_part(&draw(&mut state, 80, 24), 80, 24);
        assert!(!part.join("\n").contains("No files found."), "{part:?}");
        finish_rebuild(&mut state);
        state.tick(at(10));
        let part = view_part(&draw(&mut state, 80, 24), 80, 24);
        assert!(part.join("\n").contains("No files found."), "{part:?}");
        assert!(!part[0].contains("updating"), "{:?}", part[0]);
    }

    /// `y` holds the top file and 14 more; `x` holds two files; `z` is tiny. Deleting `y`
    /// leaves `x`'s files in a pending rebuild.
    fn selection_scene() -> (AppState, NodeId) {
        let mut scan = Scan::with_top(3);
        let kids = scan.list(ROOT, vec![folder("x"), folder("y"), folder("z")]);
        scan.list(kids[0], vec![file("x1", 90_000), file("x2", 80_000)]);
        let mut y = vec![file("y00", 1_000_000)];
        y.extend((1..15).map(|i| file(&format!("y{i:02}"), 1_000 + i)));
        scan.list(kids[1], y);
        scan.list(
            kids[2],
            (0..20).map(|i| file(&format!("z{i:02}"), 10 + i)).collect(),
        );
        let mut state = app_state(scan);
        press(&mut state, &[KeyCode::Char('3')]);
        draw(&mut state, 100, 30);
        (state, kids[1])
    }

    fn selected_name(state: &AppState) -> Vec<u8> {
        let id = state.lists.selected_node(View::Largest).unwrap();
        state.model.name(id).to_vec()
    }

    #[test]
    fn a_surviving_file_stays_selected_when_its_row_moves() {
        let (mut state, y) = selection_scene();
        assert_eq!(largest_names(&mut state)[..3], ["y00", "x1", "x2"]);
        press(&mut state, &[KeyCode::Down, KeyCode::Down]);
        assert_eq!(selected_name(&state), b"x2");
        delete(&mut state, y);
        assert!(state.model.top_updating());
        state.tick(at(0));
        draw(&mut state, 100, 30);
        assert_eq!(selected_name(&state), b"x2");
        assert_eq!(largest_names(&mut state)[..2], ["x1", "x2"]);
    }

    #[test]
    fn a_deleted_selection_takes_the_row_now_in_its_place_or_the_last_one() {
        let (mut state, y) = selection_scene();
        delete(&mut state, y);
        assert!(state.model.top_updating());
        state.tick(at(0));
        draw(&mut state, 100, 30);
        // y00 was row 0: the row now at position 0 is x1.
        assert_eq!(selected_name(&state), b"x1");

        let (mut state, y) = selection_scene();
        press(&mut state, &[KeyCode::Char('G')]);
        let before = largest_names(&mut state).len();
        let position = state.lists.cursor(View::Largest).unwrap().index();
        assert_eq!(position, before - 1);
        delete(&mut state, y);
        state.tick(at(0));
        draw(&mut state, 100, 30);
        let names = largest_names(&mut state);
        let expected = names[position.min(names.len() - 1)].clone();
        assert_eq!(selected_name(&state), expected.as_bytes());
    }
}
