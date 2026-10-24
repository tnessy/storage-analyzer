//! Tree view (T016): order, percentages, keys, live updates, markers, aggregate, sanitizing.

#[path = "support/synthetic.rs"]
mod synthetic;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::map_state::Pane;
use storage_analyzer::app::state::AppState;
use storage_analyzer::app::tree_state::{AGGREGATE_PAGE, AppEvent, Row};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, NodeId, Op, SizeMode};
use storage_analyzer::ui;
use synthetic::{Scan, file, folder};

const ROOT: NodeId = NodeId::ROOT;

fn app_state(scan: Scan) -> AppState {
    AppState::new(cli::parse(APP_ID, [APP_ID]).unwrap(), scan.model)
}

/// Renders like the app does (prepare the view, then draw) and returns every screen row.
fn draw(state: &mut AppState, w: u16, h: u16) -> Vec<String> {
    let area = Rect::new(0, 0, w, h);
    state.prepare_view(area);
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

/// The rows of the tree pane only.
fn tree_pane(screen: &[String], w: u16, h: u16) -> Vec<String> {
    let main = ui::regions(Rect::new(0, 0, w, h)).main;
    screen[usize::from(main.y)..usize::from(main.bottom())].to_vec()
}

fn row_with<'a>(rows: &'a [String], needle: &str) -> &'a str {
    rows.iter()
        .find(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle:?} in:\n{}", rows.join("\n")))
}

fn press(state: &mut AppState, codes: &[KeyCode]) {
    for &code in codes {
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

fn ids(state: &AppState) -> Vec<NodeId> {
    state
        .tree
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Node { id, .. } => Some(*id),
            Row::Parent | Row::Aggregate { .. } => None,
        })
        .collect()
}

fn lines_in_order(rows: &[String], names: &[&str]) -> bool {
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

#[test]
fn children_are_sorted_by_size_then_name() {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            file("bravo", 100),
            file("alpha", 100),
            file("zulu", 300),
            folder("mid"),
        ],
    );
    scan.list(kids[3], vec![file("inner", 200)]);
    let mut state = app_state(scan);
    let screen = draw(&mut state, 80, 24);
    let pane = tree_pane(&screen, 80, 24);
    assert!(
        lines_in_order(&pane, &["zulu", "mid/", "alpha", "bravo"]),
        "{}",
        pane.join("\n")
    );
    assert_eq!(ids(&state), vec![ROOT, kids[2], kids[3], kids[1], kids[0]]);
}

#[test]
fn percent_is_relative_to_each_rows_parent_and_zoom_rebases_it() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("aaa"), file("bbb", 400)]);
    let inner = scan.list(kids[0], vec![file("a_one", 400), file("a_two", 200)]);
    let mut state = app_state(scan);

    press(&mut state, &[KeyCode::Down, KeyCode::Right]);
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(row_with(&pane, "100.0").contains(synthetic::ROOT_PATH));
    assert!(
        row_with(&pane, "aaa/").contains(" 60.0"),
        "{}",
        pane.join("\n")
    );
    assert!(row_with(&pane, "bbb").contains(" 40.0"));
    assert!(row_with(&pane, "a_one").contains(" 66.7"));
    assert!(row_with(&pane, "a_two").contains(" 33.3"));

    press(&mut state, &[KeyCode::Enter]);
    assert_eq!(
        state.events,
        vec![AppEvent::ZoomChanged { view_root: kids[0] }]
    );
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        row_with(&pane, "aaa/").contains("100.0"),
        "{}",
        pane.join("\n")
    );
    assert!(row_with(&pane, "a_one").contains(" 66.7"));
    assert!(!pane.iter().any(|r| r.contains("bbb")));
    assert_eq!(ids(&state), vec![kids[0], inner[0], inner[1]]);

    press(&mut state, &[KeyCode::Backspace]);
    assert_eq!(state.tree.view_root, ROOT);
    assert_eq!(
        state.tree.selection, kids[0],
        "selection stays on the folder we left"
    );
    assert_eq!(
        state.events.last(),
        Some(&AppEvent::ZoomChanged { view_root: ROOT })
    );
}

#[test]
fn keys_expand_collapse_and_move() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("aaa"), file("bbb", 50)]);
    let inner = scan.list(kids[0], vec![file("a_one", 400)]);
    let mut state = app_state(scan);

    press(&mut state, &[KeyCode::Down]);
    assert_eq!(state.tree.selection, kids[0]);
    press(&mut state, &[KeyCode::Right]);
    assert!(draw(&mut state, 80, 24).join("\n").contains("a_one"));
    press(&mut state, &[KeyCode::Right]);
    assert_eq!(
        state.tree.selection, inner[0],
        "→ on an expanded folder enters it"
    );
    press(&mut state, &[KeyCode::Left]);
    assert_eq!(
        state.tree.selection, kids[0],
        "← on a file goes to the parent"
    );
    press(&mut state, &[KeyCode::Left]);
    assert!(
        !draw(&mut state, 80, 24).join("\n").contains("a_one"),
        "← collapses"
    );
    press(&mut state, &[KeyCode::Left]);
    assert_eq!(state.tree.selection, ROOT);
    press(&mut state, &[KeyCode::Char('G')]);
    assert_eq!(state.tree.selection, kids[1]);
    press(&mut state, &[KeyCode::Char('g')]);
    assert_eq!(state.tree.selection, ROOT);
    press(
        &mut state,
        &[KeyCode::Char('j'), KeyCode::Char('j'), KeyCode::Char('k')],
    );
    assert_eq!(state.tree.selection, kids[0]);
    press(&mut state, &[KeyCode::Up, KeyCode::Up]);
    assert_eq!(state.tree.selection, ROOT, "↑ at the top stays put");
}

#[test]
fn zoom_ignores_files_and_the_view_root() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("only", 5)]);
    let mut state = app_state(scan);
    press(
        &mut state,
        &[
            KeyCode::Enter,
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Backspace,
        ],
    );
    assert_eq!(state.tree.view_root, ROOT);
    assert!(state.events.is_empty());
}

#[test]
fn sizes_percentages_and_order_follow_the_scan() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("first"), folder("second")]);
    let mut state = app_state(scan);
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        lines_in_order(&pane, &["first", "second"]),
        "equal sizes order by name"
    );
    assert!(row_with(&pane, "first").contains('◌'));

    state.model.apply(synthetic::listing(
        kids[1],
        10,
        vec![synthetic::entry(
            "x",
            storage_analyzer::types::Kind::File,
            600,
        )],
    ));
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        lines_in_order(&pane, &["second", "first"]),
        "{}",
        pane.join("\n")
    );
    assert!(
        row_with(&pane, "second").contains("100.0"),
        "{}",
        pane.join("\n")
    );
    assert!(!row_with(&pane, "second").contains('◌'));

    state.model.apply(synthetic::listing(
        kids[0],
        11,
        vec![synthetic::entry(
            "y",
            storage_analyzer::types::Kind::File,
            200,
        )],
    ));
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        row_with(&pane, "second").contains(" 75.0"),
        "{}",
        pane.join("\n")
    );
    assert!(row_with(&pane, "first").contains(" 25.0"));
}

#[test]
fn selection_follows_the_item_through_live_resorting() {
    let mut scan = Scan::new();
    let dirs = scan.list(ROOT, (0..30).map(|i| folder(&format!("d{i:02}"))).collect());
    let mut state = app_state(scan);
    draw(&mut state, 80, 24);
    let mut next_id = 1000;
    let mut rng = 12345u64;
    let mut pending: Vec<NodeId> = dirs.clone();
    for _batch in 0..8 {
        let before = state.tree.selection;
        for _ in 0..3 {
            rng = rng
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let pick = pending.remove((rng >> 33) as usize % pending.len());
            let size = 100 + (rng >> 40) % 5000;
            state.model.apply(synthetic::listing(
                pick,
                next_id,
                vec![synthetic::entry(
                    "f",
                    storage_analyzer::types::Kind::File,
                    size,
                )],
            ));
            next_id += 1;
        }
        draw(&mut state, 80, 24);
        assert_eq!(
            state.tree.selection, before,
            "a re-sort moved the selection"
        );
        for _ in 0..2 {
            let order: Vec<NodeId> = std::iter::once(ROOT)
                .chain(state.model.children_sorted(ROOT, SizeMode::Disk))
                .collect();
            let at = order
                .iter()
                .position(|&n| n == state.tree.selection)
                .unwrap();
            let expected = order.get(at + 1).copied().unwrap_or(state.tree.selection);
            press(&mut state, &[KeyCode::Down]);
            draw(&mut state, 80, 24);
            assert_eq!(state.tree.selection, expected);
        }
    }
}

#[test]
fn removed_selection_moves_to_its_parent() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("aaa")]);
    let inner = scan.list(kids[0], vec![file("gone", 10)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Right, KeyCode::Down]);
    assert_eq!(state.tree.selection, inner[0]);
    // Selecting a node that no longer exists falls back to the nearest visible ancestor.
    state.tree.selection = NodeId(9999);
    draw(&mut state, 80, 24);
    assert_eq!(state.tree.selection, ROOT);
}

#[test]
fn markers_follow_the_state_table_and_precedence() {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            folder("scanning"),
            folder("skipped").issue(IssueKind::SkippedByUser),
            folder("denied").issue(IssueKind::PermissionDenied),
            folder("otherfs").issue(IssueKind::OtherFilesystem),
            folder("vanished").issue(IssueKind::Vanished),
            folder("broken").issue(IssueKind::Other(5)),
            folder("partial"),
            folder("prio"),
        ],
    );
    let inner = scan.list(kids[6], vec![folder("cut")]);
    scan.model.apply(ScanMsg::Cancelled { dir: inner[0] });
    let mut state = app_state(scan);
    state.priority = Some(kids[7]);
    let pane = tree_pane(&draw(&mut state, 100, 40), 100, 40);

    let scanning = row_with(&pane, "scanning/");
    assert!(
        scanning.contains('◌') && !scanning.contains('≥'),
        "{scanning}"
    );
    let skipped = row_with(&pane, "skipped/");
    assert!(
        skipped.contains('⊘') && skipped.contains('≥') && skipped.contains("(scan skipped)"),
        "{skipped}"
    );
    assert!(!skipped.contains('!'), "⊘ beats !");
    let denied = row_with(&pane, "denied/");
    assert!(
        denied.contains('!') && denied.contains("(permission denied)") && !denied.contains('⊘'),
        "{denied}"
    );
    assert!(row_with(&pane, "otherfs/").contains("(other filesystem)"));
    assert!(row_with(&pane, "vanished/").contains("(vanished during scan)"));
    assert!(row_with(&pane, "broken/").contains("(read error:"));
    let partial = row_with(&pane, "partial/");
    assert!(
        partial.contains('≥') && !partial.contains('◌'),
        "complete but inexact: {partial}"
    );
    let prio = row_with(&pane, "prio/");
    assert!(
        prio.contains('▲') && !prio.contains('◌'),
        "▲ beats ◌: {prio}"
    );
}

#[test]
fn a_big_folder_shows_fifty_rows_and_an_aggregate_that_reveals_more() {
    let mut scan = Scan::new();
    let items = (0..120)
        .map(|i| file(&format!("f{i:03}"), 1000 - i))
        .collect();
    let kids = scan.list(ROOT, items);
    let mut state = app_state(scan);
    let screen = draw(&mut state, 120, 100);
    let pane = tree_pane(&screen, 120, 100);
    let tail: u64 = (50..120).map(|i| 1000 - i).sum();
    let aggregate = format!("(70 smaller, {}", ui::format::size_iec(tail));
    assert!(
        pane.iter().any(|r| r.contains(&aggregate)),
        "{}",
        pane.join("\n")
    );
    assert!(pane.iter().any(|r| r.contains("f049")) && !pane.iter().any(|r| r.contains("f050")));

    press(&mut state, &[KeyCode::Char('G')]);
    assert_eq!(
        state.tree.aggregate,
        Some(ROOT),
        "the cursor can rest on the aggregate row"
    );
    press(&mut state, &[KeyCode::Enter]);
    assert!(state.events.is_empty(), "aggregate rows can't be zoomed");
    press(&mut state, &[KeyCode::Right]);
    let pane = tree_pane(&draw(&mut state, 120, 100), 120, 100);
    assert!(pane.iter().any(|r| r.contains("f099")) && !pane.iter().any(|r| r.contains("f100")));
    assert!(pane.iter().any(|r| r.contains("(20 smaller, ")));
    assert_eq!(ids(&state).len(), 1 + 100);
    assert_eq!(AGGREGATE_PAGE, 50);
    let _ = kids;
}

#[test]
fn the_reveal_survives_resorting_until_the_folder_is_collapsed() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("big")]);
    let dirs = scan.list(
        top[0],
        (0..120).map(|i| folder(&format!("d{i:03}"))).collect(),
    );
    let mut state = app_state(scan);
    press(
        &mut state,
        &[
            KeyCode::Down,
            KeyCode::Right,
            KeyCode::Char('G'),
            KeyCode::Right,
        ],
    );
    draw(&mut state, 120, 100);
    assert_eq!(ids(&state).len(), 2 + 100);

    for (n, &dir) in dirs.iter().rev().take(5).enumerate() {
        state.model.apply(synthetic::listing(
            dir,
            2000 + n as u32,
            vec![synthetic::entry(
                "f",
                storage_analyzer::types::Kind::File,
                10_000,
            )],
        ));
    }
    draw(&mut state, 120, 100);
    assert_eq!(ids(&state).len(), 2 + 100, "re-sort kept the revealed page");

    press(&mut state, &[KeyCode::Left]);
    assert_eq!(state.tree.aggregate, None);
    assert_eq!(state.tree.selection, top[0]);
    press(&mut state, &[KeyCode::Left, KeyCode::Right]);
    draw(&mut state, 120, 100);
    assert_eq!(
        ids(&state).len(),
        2 + AGGREGATE_PAGE,
        "collapse forgets the reveal"
    );
}

#[test]
fn the_selected_item_never_disappears_into_the_aggregate() {
    let mut scan = Scan::new();
    let mut items: Vec<_> = (0..120).map(|i| folder(&format!("d{i:03}"))).collect();
    items.extend((0..5).map(|i| folder(&format!("late{i}"))));
    let dirs = scan.list(ROOT, items);
    for (i, &dir) in dirs.iter().take(120).enumerate() {
        scan.list(dir, vec![file("f", 1000 - i as u64)]);
    }
    let mut state = app_state(scan);
    draw(&mut state, 120, 100);
    for _ in 0..50 {
        press(&mut state, &[KeyCode::Down]);
    }
    let selected = state.tree.selection;
    assert_eq!(selected, dirs[49], "the 50th child");

    for (n, &dir) in dirs.iter().skip(120).enumerate() {
        state.model.apply(synthetic::listing(
            dir,
            5000 + n as u32,
            vec![synthetic::entry(
                "f",
                storage_analyzer::types::Kind::File,
                5000 + n as u64,
            )],
        ));
    }
    let pane = tree_pane(&draw(&mut state, 120, 100), 120, 100);
    assert_eq!(state.tree.selection, selected, "selection is unchanged");
    let sorted = state.model.children_sorted(ROOT, SizeMode::Disk);
    let at = sorted.iter().position(|&n| n == selected).unwrap();
    assert!(
        at >= AGGREGATE_PAGE,
        "the live re-sort pushed it below the cut-off"
    );

    let last = pane.iter().rposition(|r| r.contains("d049")).unwrap();
    assert!(
        pane[last + 1].contains("smaller"),
        "own row directly above the aggregate:\n{}",
        pane.join("\n")
    );
    let hidden: Vec<NodeId> = sorted[AGGREGATE_PAGE..]
        .iter()
        .copied()
        .filter(|&n| n != selected)
        .collect();
    let bytes: u64 = hidden
        .iter()
        .map(|&n| state.model.size(n, SizeMode::Disk))
        .sum();
    let expected = format!("({} smaller, {}", hidden.len(), ui::format::size_iec(bytes));
    assert!(
        pane[last + 1].contains(&expected),
        "{} vs {expected}",
        pane[last + 1]
    );

    press(&mut state, &[KeyCode::Up]);
    let pane = tree_pane(&draw(&mut state, 120, 100), 120, 100);
    assert!(
        !pane.iter().any(|r| r.contains("d049")),
        "folded back into the aggregate"
    );
    let count = sorted.len() - AGGREGATE_PAGE;
    assert!(
        pane.iter()
            .any(|r| r.contains(&format!("({count} smaller, "))),
        "{}",
        pane.join("\n")
    );
}

#[test]
fn hostile_names_stay_inside_the_row_and_reach_the_buffer_escaped() {
    let mut scan = Scan::new();
    let long = "x".repeat(200);
    scan.list(
        ROOT,
        vec![
            file(&long, 10),
            file("esc\x1b[2Jname", 9),
            file("\u{9b}c1", 8),
            file("日本語のとても長いファイル名日本語のとても長いファイル名日本語のとても長いファイル名", 7),
        ],
    );
    let mut state = app_state(scan);
    let screen = draw(&mut state, 80, 24);
    let pane = tree_pane(&screen, 80, 24);
    let long_row = row_with(&pane, "xxxx");
    assert!(long_row.contains('…'), "{long_row}");
    assert!(
        pane.iter().any(|r| r.contains("esc\\x1b[2Jname")),
        "{}",
        pane.join("\n")
    );
    assert!(pane.iter().any(|r| r.contains("\\x9bc1")));
    for line in &screen {
        assert!(
            !line.chars().any(char::is_control),
            "raw control byte in {line:?}"
        );
    }
}

#[test]
fn an_empty_folder_says_so() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![]);
    let mut state = app_state(scan);
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    assert!(
        pane.iter().any(|r| r.contains("This folder is empty.")),
        "{}",
        pane.join("\n")
    );
}

#[test]
fn zoomed_header_shows_a_breadcrumb() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("cache")]);
    scan.list(kids[0], vec![file("x", 10)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let screen = draw(&mut state, 100, 30);
    assert!(
        screen[0].contains(&format!("{} › cache", synthetic::ROOT_PATH)),
        "{}",
        screen[0]
    );
}

#[test]
fn scanning_screen_layout_snapshot() {
    let mut scan = Scan::new();
    let gib = 1024 * 1024 * 1024u64;
    let kids = scan.list(
        ROOT,
        vec![
            folder(".cache"),
            folder("Videos"),
            folder(".local").issue(IssueKind::SkippedByUser),
            folder("Projects"),
            folder("Downloads").issue(IssueKind::PermissionDenied),
            file("notes.txt", 3 * gib / 10),
        ],
    );
    scan.list(
        kids[0],
        vec![folder("yay"), folder("mozilla"), file("misc", gib)],
    );
    scan.model.apply(ScanMsg::DirIssue {
        dir: NodeId(7),
        op: Op::Read,
        kind: IssueKind::PermissionDenied,
    });
    scan.list(kids[1], vec![file("movie.mkv", 42 * gib)]);
    scan.list(kids[3], vec![file("repo.pack", 20 * gib)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Right]);
    let pane = tree_pane(&draw(&mut state, 80, 24), 80, 24);
    insta::assert_snapshot!(pane.join("\n"));
}

fn cursor_line(pane: &[String]) -> usize {
    pane.iter()
        .position(|r| r.starts_with('>'))
        .expect("a cursor row")
}

#[test]
fn page_keys_move_by_a_page_and_the_viewport_follows() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        (0..60)
            .map(|i| file(&format!("f{i:02}"), 1000 - i))
            .collect(),
    );
    let mut state = app_state(scan);
    draw(&mut state, 100, 40);
    press(&mut state, &[KeyCode::PageDown]);
    let pane = tree_pane(&draw(&mut state, 100, 40), 100, 40);
    assert_eq!(pane.len(), 20);
    assert_eq!(
        state.tree.selection,
        NodeId(19),
        "a page is the pane height minus one"
    );
    assert!(
        pane.iter().any(|r| r.starts_with('>') && r.contains("f18")),
        "{}",
        pane.join("\n")
    );
    press(&mut state, &[KeyCode::PageDown, KeyCode::PageDown]);
    draw(&mut state, 100, 40);
    assert!(state.tree.top() > 0, "the pane scrolled");
    press(
        &mut state,
        &[
            KeyCode::PageUp,
            KeyCode::PageUp,
            KeyCode::PageUp,
            KeyCode::PageUp,
        ],
    );
    assert_eq!(state.tree.selection, ROOT);
}

#[test]
fn live_changes_keep_the_selected_row_on_the_same_screen_line() {
    let mut scan = Scan::new();
    let dirs = scan.list(ROOT, (0..40).map(|i| folder(&format!("d{i:02}"))).collect());
    for (i, &dir) in dirs.iter().take(30).enumerate() {
        scan.list(dir, vec![file("f", 1000 - i as u64)]);
    }
    let mut state = app_state(scan);
    draw(&mut state, 100, 40);
    for _ in 0..8 {
        press(&mut state, &[KeyCode::Down]);
    }
    let before = tree_pane(&draw(&mut state, 100, 40), 100, 40);
    let line = cursor_line(&before);
    assert_eq!(line, 8, "eight moves down put the cursor on line 8");
    let selected = state.tree.selection;
    // A folder further down the list finishes scanning and jumps above the selection.
    state.model.apply(synthetic::listing(
        dirs[35],
        9000,
        vec![synthetic::entry(
            "big",
            storage_analyzer::types::Kind::File,
            5000,
        )],
    ));
    let after = tree_pane(&draw(&mut state, 100, 40), 100, 40);
    assert_eq!(state.tree.selection, selected);
    assert_eq!(cursor_line(&after), line, "{}", after.join("\n"));
    assert!(
        after[line].contains(&format!("d{:02}", selected.0 - 1)),
        "{}",
        after[line]
    );
}

#[test]
fn the_root_row_header_and_breadcrumb_show_the_model_path_exactly() {
    use storage_analyzer::model::{Model, ModelOptions};
    // The root's stored name differs from its path, so only `Model::path` gives this text.
    let model = Model::new(
        "/synthetic/real".into(),
        synthetic::entry("short", storage_analyzer::types::Kind::Dir, 0),
        ModelOptions { top_n: 10 },
    );
    let mut scan = Scan::new();
    scan.model = model;
    let kids = scan.list(ROOT, vec![folder("cache")]);
    scan.list(kids[0], vec![file("x", 10)]);
    let mut state = app_state(scan);
    let screen = draw(&mut state, 100, 30);
    assert!(
        screen[0].contains("storage-analyzer  /synthetic/real "),
        "{}",
        screen[0]
    );
    let pane = tree_pane(&screen, 100, 30);
    assert!(
        row_with(&pane, "100.0").contains("/synthetic/real"),
        "{}",
        pane[0]
    );
    assert!(
        !screen.join("\n").contains("/synthetic/real/"),
        "no trailing slash anywhere"
    );
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let screen = draw(&mut state, 100, 30);
    assert!(
        screen[0].contains("/synthetic/real › cache"),
        "{}",
        screen[0]
    );
}

#[test]
fn guides_follow_the_two_level_pattern_of_the_spec() {
    let mut scan = Scan::new();
    let top = scan.list(
        ROOT,
        vec![folder("cache"), file("videos", 400), file("last", 100)],
    );
    let cache = scan.list(
        top[0],
        vec![folder("yay"), file("mozilla", 200), file("pip", 100)],
    );
    scan.list(cache[0], vec![file("inner", 300)]);
    let mut state = app_state(scan);
    press(
        &mut state,
        &[KeyCode::Down, KeyCode::Right, KeyCode::Down, KeyCode::Right],
    );
    let pane = tree_pane(&draw(&mut state, 100, 30), 100, 30);
    let name_part = |name: &str| -> String {
        let row = row_with(&pane, name);
        let at = row.find(['├', '└', '│']).expect("guides");
        row[at..].trim_end().to_owned()
    };
    assert_eq!(name_part("cache/"), "├─▾ cache/");
    assert_eq!(name_part("yay/"), "│  ├─▾ yay/");
    assert_eq!(name_part("inner"), "│  │  └── inner");
    assert_eq!(name_part("mozilla"), "│  ├── mozilla");
    assert_eq!(name_part("pip"), "│  └── pip");
    assert_eq!(name_part("videos"), "├── videos");
    assert_eq!(name_part("last"), "└── last");
    let root = row_with(&pane, "100.0");
    assert!(
        !root.contains('├') && !root.contains('└'),
        "the root row has no guide: {root}"
    );
}

#[test]
fn the_aggregate_row_is_the_last_branch() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        (0..60)
            .map(|i| file(&format!("f{i:02}"), 1000 - i))
            .collect(),
    );
    let mut state = app_state(scan);
    let pane = tree_pane(&draw(&mut state, 100, 100), 100, 100);
    let agg = row_with(&pane, "smaller");
    assert!(agg.contains("└── (10 smaller"), "{agg}");
    assert!(row_with(&pane, "f49").contains("├── f49"));
}

fn draw_buf(state: &mut AppState, w: u16, h: u16) -> Buffer {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    terminal.backend().buffer().clone()
}

fn buf_row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

/// The screen row of the selected tree row (the one with `>` in column 1).
fn selected_y(buf: &Buffer, w: u16, h: u16) -> u16 {
    let main = ui::regions(Rect::new(0, 0, w, h)).main;
    (main.y..main.bottom())
        .find(|&y| buf[(0, y)].symbol() == ">")
        .unwrap_or_else(|| {
            panic!(
                "no selected row:\n{}",
                (0..h)
                    .map(|y| buf_row(buf, y))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

fn two_level_state() -> (AppState, Vec<NodeId>) {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("alpha"), file("beta.txt", 40)]);
    scan.list(kids[0], vec![file("inner.bin", 60)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Right]);
    (state, kids)
}

#[test]
fn tree_focus_selection_is_reverse_bold_across_the_pane_and_never_dim() {
    let (mut state, _) = two_level_state();
    let buf = draw_buf(&mut state, 80, 24);
    let y = selected_y(&buf, 80, 24);
    assert!(buf_row(&buf, y).contains("alpha/"));
    for x in 0..80 {
        let m = buf[(x, y)].modifier;
        assert!(
            m.contains(Modifier::REVERSED | Modifier::BOLD),
            "col {x}: {m:?}"
        );
        assert!(!m.contains(Modifier::DIM), "col {x} is dim");
    }
    // Rows below (with guides) are not reversed.
    assert!(!buf[(40, y + 1)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn map_focus_selection_is_bold_with_the_name_underlined_and_not_reversed() {
    let (mut state, _) = two_level_state();
    press(&mut state, &[KeyCode::Tab]);
    assert_eq!(state.focus, Pane::Map);
    let buf = draw_buf(&mut state, 80, 24);
    let y = selected_y(&buf, 80, 24);
    let row = buf_row(&buf, y);
    let at = row.find("alpha/").expect("name");
    let col = row[..at].chars().count() as u16;
    for x in 0..80 {
        let m = buf[(x, y)].modifier;
        assert!(!m.contains(Modifier::REVERSED), "col {x}");
        assert!(
            m.contains(Modifier::BOLD) || row.chars().nth(x as usize) == Some(' '),
            "col {x}"
        );
    }
    for x in col..col + 6 {
        assert!(
            buf[(x, y)].modifier.contains(Modifier::UNDERLINED),
            "name col {x}"
        );
    }
    assert!(!buf[(col - 2, y)].modifier.contains(Modifier::UNDERLINED));
}

fn zoomed(depth_names: &[&str]) -> (AppState, Vec<NodeId>) {
    let mut scan = Scan::new();
    let mut parent = ROOT;
    let mut chain = Vec::new();
    for name in depth_names {
        let kids = scan.list(parent, vec![folder(name), file(&format!("{name}.sib"), 1)]);
        chain.push(kids[0]);
        parent = kids[0];
    }
    scan.list(parent, vec![file("leaf", 5)]);
    (app_state(scan), chain)
}

fn pane(buf: &Buffer, w: u16, h: u16) -> Vec<String> {
    let main = ui::regions(Rect::new(0, 0, w, h)).main;
    (main.y..main.bottom()).map(|y| buf_row(buf, y)).collect()
}

#[test]
fn the_parent_row_appears_only_when_zoomed_and_sits_above_the_view_root() {
    let (mut state, chain) = zoomed(&["aaa"]);
    let rows = pane(&draw_buf(&mut state, 100, 24), 100, 24);
    assert!(
        !rows.iter().any(|r| r.contains("..  (up to")),
        "{}",
        rows.join("\n")
    );

    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let buf = draw_buf(&mut state, 100, 24);
    let rows = pane(&buf, 100, 24);
    assert!(
        rows[0].contains("..  (up to /synthetic/root)"),
        "{}",
        rows.join("\n")
    );
    assert!(
        rows[0].trim_start().starts_with(".."),
        "blank size columns: {}",
        rows[0]
    );
    assert!(
        rows[1].starts_with('>') && rows[1].contains("aaa/"),
        "{}",
        rows.join("\n")
    );
    assert_eq!(state.tree.selection, chain[0]);
    assert!(!state.tree.on_parent);
}

#[test]
fn up_on_the_view_root_reaches_the_parent_row_and_up_again_stays() {
    let (mut state, _) = zoomed(&["aaa"]);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter, KeyCode::Up]);
    assert!(state.tree.on_parent);
    let rows = pane(&draw_buf(&mut state, 100, 24), 100, 24);
    assert!(rows[0].starts_with('>'), "{}", rows.join("\n"));
    press(&mut state, &[KeyCode::Up]);
    assert!(state.tree.on_parent);
    press(
        &mut state,
        &[KeyCode::Down, KeyCode::Down, KeyCode::Char('g')],
    );
    assert!(state.tree.on_parent, "g goes to the parent row");
}

#[test]
fn parent_row_is_selected_like_any_row() {
    let (mut state, _) = zoomed(&["aaa"]);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter, KeyCode::Up]);
    let buf = draw_buf(&mut state, 100, 24);
    let y = selected_y(&buf, 100, 24);
    assert!(buf_row(&buf, y).contains("..  (up to"));
    for x in 0..100 {
        assert!(
            buf[(x, y)]
                .modifier
                .contains(Modifier::REVERSED | Modifier::BOLD),
            "col {x}"
        );
    }
}

#[test]
fn enter_on_the_parent_row_equals_backspace_and_selects_the_folder_you_left() {
    let (mut a, chain) = zoomed(&["aaa", "bbb"]);
    press(
        &mut a,
        &[KeyCode::Down, KeyCode::Enter, KeyCode::Down, KeyCode::Enter],
    );
    assert_eq!(a.tree.view_root, chain[1]);
    let (mut b, _) = zoomed(&["aaa", "bbb"]);
    press(
        &mut b,
        &[KeyCode::Down, KeyCode::Enter, KeyCode::Down, KeyCode::Enter],
    );

    press(&mut a, &[KeyCode::Up, KeyCode::Enter]);
    press(&mut b, &[KeyCode::Backspace]);
    for s in [&a, &b] {
        assert_eq!(s.tree.view_root, chain[0]);
        assert_eq!(s.tree.selection, chain[1], "the folder you came from");
        assert!(!s.tree.on_parent);
    }
    assert_eq!(a.events, b.events);
}

#[test]
fn backspace_from_a_deep_zoom_selects_the_folder_left_at_each_level() {
    let (mut state, chain) = zoomed(&["aaa", "bbb", "ccc"]);
    for _ in 0..3 {
        press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    }
    assert_eq!(state.tree.view_root, chain[2]);
    for level in (0..3).rev() {
        press(&mut state, &[KeyCode::Backspace]);
        assert_eq!(state.tree.selection, chain[level], "level {level}");
        assert_eq!(
            state.tree.view_root,
            if level == 0 { ROOT } else { chain[level - 1] }
        );
    }
    assert_eq!(
        state.events.last(),
        Some(&AppEvent::ZoomChanged { view_root: ROOT })
    );
}

#[test]
fn left_and_h_and_minus_go_up_from_the_parent_row_but_right_does_nothing() {
    for up in [
        KeyCode::Left,
        KeyCode::Char('h'),
        KeyCode::Char('-'),
        KeyCode::Enter,
        KeyCode::Backspace,
    ] {
        let (mut state, chain) = zoomed(&["aaa"]);
        press(
            &mut state,
            &[KeyCode::Down, KeyCode::Enter, KeyCode::Up, up],
        );
        assert_eq!(state.tree.view_root, ROOT, "{up:?}");
        assert_eq!(state.tree.selection, chain[0]);
    }
    for noop in [KeyCode::Right, KeyCode::Char('l')] {
        let (mut state, chain) = zoomed(&["aaa"]);
        press(
            &mut state,
            &[KeyCode::Down, KeyCode::Enter, KeyCode::Up, noop],
        );
        assert_eq!(state.tree.view_root, chain[0]);
        assert!(state.tree.on_parent);
    }
}

#[test]
fn d_p_x_r_on_the_parent_row_show_the_message_and_change_nothing() {
    for key in ['d', 'p', 'x', 'r'] {
        let (mut state, chain) = zoomed(&["aaa"]);
        press(&mut state, &[KeyCode::Down, KeyCode::Enter, KeyCode::Up]);
        let events = state.events.len();
        press(&mut state, &[KeyCode::Char(key)]);
        let screen = draw(&mut state, 100, 24);
        let status = &screen[22];
        assert!(
            status.contains("\"..\" is the parent folder. Press ⏎ to go up."),
            "{key}: {status}"
        );
        assert!(state.tree.on_parent && state.tree.view_root == chain[0]);
        assert_eq!(state.events.len(), events, "no event, no request");
        assert!(state.priority.is_none());
    }
    // Elsewhere those keys are not intercepted.
    let (mut state, _) = zoomed(&["aaa"]);
    press(
        &mut state,
        &[KeyCode::Down, KeyCode::Enter, KeyCode::Char('d')],
    );
    assert!(!draw(&mut state, 100, 24)[22].contains("parent folder"));
}

#[test]
fn the_parent_row_is_never_inside_the_aggregate_or_counted_in_the_page() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("big")]);
    scan.list(
        top[0],
        (0..60)
            .map(|i| file(&format!("f{i:02}"), 1000 - i))
            .collect(),
    );
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let rows = pane(&draw_buf(&mut state, 120, 100), 120, 100);
    assert!(rows[0].contains("..  (up to"));
    assert_eq!(rows.iter().filter(|r| r.contains("..  (up to")).count(), 1);
    assert!(
        rows.iter().any(|r| r.contains("(10 smaller")),
        "{}",
        rows.join("\n")
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r.contains("├── f") || r.contains("└── f"))
            .count(),
        50
    );
}

#[test]
fn long_parent_paths_are_cut_from_the_left() {
    let long = "/home/nss/projects/an-extremely-long-project-directory-name/with/many/levels/here";
    let mut scan = Scan::new();
    scan.model = storage_analyzer::model::Model::new(
        long.into(),
        synthetic::entry("n", storage_analyzer::types::Kind::Dir, 0),
        storage_analyzer::model::ModelOptions { top_n: 10 },
    );
    let kids = scan.list(ROOT, vec![folder("aaa")]);
    scan.list(kids[0], vec![file("x", 1)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let rows = pane(&draw_buf(&mut state, 80, 24), 80, 24);
    assert!(
        rows[0].contains("(up to …") && rows[0].trim_end().ends_with("here)"),
        "{}",
        rows[0]
    );
    assert!(ui::format::width(rows[0].trim_end()) <= 80);
}

#[test]
fn a_name_with_a_bidi_override_shows_escaped_within_its_width() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        vec![file("evil\u{202E}txt.exe", 5_000), file("plain", 1_000)],
    );
    let mut state = app_state(scan);
    let screen = draw(&mut state, 80, 24);
    let joined = screen.join("\n");
    assert!(!joined.contains('\u{202E}'), "{joined}");
    let row = row_with(&screen, "evil");
    assert!(row.contains("evil\\u{202E}txt.exe"), "{row:?}");
    assert!(storage_analyzer::ui::format::width(row) <= 80, "{row:?}");
}
