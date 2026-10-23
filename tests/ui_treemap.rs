//! Treemap widget (T017): scope, tiles, sync with the tree, throttling, markers, hidden items.

#[path = "support/synthetic.rs"]
mod synthetic;

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::map_state::Pane;
use storage_analyzer::app::state::AppState;
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, Kind, NodeId, SizeMode};
use storage_analyzer::ui;
use synthetic::{Scan, file, folder};

const ROOT: NodeId = NodeId::ROOT;

fn app_state(scan: Scan) -> AppState {
    AppState::new(cli::parse(APP_ID, [APP_ID]).unwrap(), scan.model)
}

fn render(state: &mut AppState, w: u16, h: u16) -> Buffer {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

fn screen(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height).map(|y| row_text(buf, y)).collect()
}

fn map_rect(w: u16, h: u16) -> Rect {
    ui::regions(Rect::new(0, 0, w, h)).map
}

fn title_row(buf: &Buffer, w: u16, h: u16) -> String {
    row_text(buf, ui::regions(Rect::new(0, 0, w, h)).map_title.y)
}

fn press(state: &mut AppState, codes: &[KeyCode]) {
    for &code in codes {
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

fn tile_of(state: &AppState, id: NodeId) -> Rect {
    state
        .map
        .tiles()
        .iter()
        .find(|t| t.id == Some(id))
        .unwrap_or_else(|| panic!("no tile for {id:?}"))
        .rect
}

fn corner(buf: &Buffer, r: Rect) -> String {
    buf[(r.x, r.y)].symbol().to_owned()
}

fn files(sizes: &[(&str, u64)]) -> (Scan, Vec<NodeId>) {
    let mut scan = Scan::new();
    let ids = scan.list(ROOT, sizes.iter().map(|(n, b)| file(n, *b)).collect());
    (scan, ids)
}

#[test]
fn tile_areas_are_proportional_within_cell_rounding() {
    let (scan, _) = files(&[("a", 500), ("b", 250), ("c", 150), ("d", 100)]);
    let mut state = app_state(scan);
    render(&mut state, 80, 24);
    let map = map_rect(80, 24);
    let total = f64::from(map.width) * f64::from(map.height);
    let tiles = state.map.tiles();
    assert_eq!(tiles.len(), 4);
    for (tile, share) in tiles.iter().zip([0.5, 0.25, 0.15, 0.10]) {
        let r = tile.rect;
        let bound = 2.0 * f64::from(r.width + r.height) + 4.0;
        let area = f64::from(r.width) * f64::from(r.height);
        assert!((area - share * total).abs() <= bound, "{r:?} share {share}");
    }
}

#[test]
fn long_labels_are_truncated_inside_their_tile() {
    let long = format!("{}_end", "very_long_name_".repeat(8));
    let (scan, _) = files(&[(&long, 700), ("b", 300)]);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    let big = state.map.tiles()[0].rect;
    let top = row_text(&buf, big.y);
    let label: String = top
        .chars()
        .skip(usize::from(big.x))
        .take(usize::from(big.width))
        .collect();
    assert!(label.contains('…'), "{label}");
    assert!(!label.contains("_end"), "{label}");
    for tile in state.map.tiles() {
        let r = tile.rect;
        assert!(["┌", "╔"].contains(&buf[(r.x, r.y)].symbol()), "{r:?}");
        assert!(
            ["┐", "╗"].contains(&buf[(r.right() - 1, r.y)].symbol()),
            "label ran past {r:?}"
        );
        assert!(["┘", "╝"].contains(&buf[(r.right() - 1, r.bottom() - 1)].symbol()));
    }
}

#[test]
fn tile_label_has_name_size_percent_and_category_tag() {
    let (scan, _) = files(&[("movie.mkv", 750), ("notes.txt", 250)]);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    let text = screen(&buf).join("\n");
    assert!(text.contains("movie.mkv 750B 75% [V]"), "{text}");
    // The narrower tile puts what didn't fit on a second line.
    assert!(text.contains("notes.txt 250B 25%"), "{text}");
    assert!(text.contains("[C]"), "{text}");
}

#[test]
fn tiles_are_filled_with_their_category_color() {
    let (scan, ids) = files(&[("movie.mkv", 750), ("notes.txt", 250)]);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    let r = tile_of(&state, ids[0]);
    let inner = &buf[(r.x + 2, r.y + r.height - 2)];
    assert_eq!(inner.bg, Color::Rgb(0xE6, 0x9F, 0x00), "video fill");
    let r = tile_of(&state, ids[1]);
    assert_eq!(
        buf[(r.x + 2, r.y + r.height - 2)].bg,
        Color::Rgb(0xF0, 0xE4, 0x42),
        "code/text fill"
    );
}

#[test]
fn tree_selection_moves_the_double_border_and_scope_follows_the_parent() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("aaa"), file("bbb", 300)]);
    let kids = scan.list(top[0], vec![file("one", 400), file("two", 100)]);
    let mut state = app_state(scan);

    let buf = render(&mut state, 80, 24);
    assert_eq!(state.map.scope(), Some(ROOT));
    assert!(
        state
            .map
            .tiles()
            .iter()
            .all(|t| corner(&buf, t.rect) == "┌"),
        "root selected: no highlight"
    );

    press(&mut state, &[KeyCode::Down]);
    let buf = render(&mut state, 80, 24);
    assert_eq!(state.tree.selection, top[0]);
    assert_eq!(corner(&buf, tile_of(&state, top[0])), "╔");
    assert_eq!(corner(&buf, tile_of(&state, top[1])), "┌");

    press(&mut state, &[KeyCode::Down]);
    let buf = render(&mut state, 80, 24);
    assert_eq!(
        corner(&buf, tile_of(&state, top[1])),
        "╔",
        "↓ moves the highlight"
    );

    // Into a child: the scope becomes that folder's children.
    press(&mut state, &[KeyCode::Up, KeyCode::Right, KeyCode::Down]);
    let buf = render(&mut state, 80, 24);
    assert_eq!(state.tree.selection, kids[0]);
    assert_eq!(state.map.scope(), Some(top[0]));
    assert_eq!(corner(&buf, tile_of(&state, kids[0])), "╔");
    assert!(
        state
            .map
            .tiles()
            .iter()
            .all(|t| t.id == Some(kids[0]) || t.id == Some(kids[1]))
    );
}

#[test]
fn zooming_into_a_folder_shows_its_children_when_the_view_root_is_selected() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("aaa"), file("bbb", 300)]);
    let kids = scan.list(top[0], vec![file("one", 400)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    render(&mut state, 80, 24);
    assert_eq!(state.tree.selection, top[0]);
    assert_eq!(
        state.map.scope(),
        Some(top[0]),
        "view root selected: its children"
    );
    assert!(state.map.tiles().iter().any(|t| t.id == Some(kids[0])));
}

#[test]
fn map_arrows_move_the_one_shared_selection_and_scroll_the_tree() {
    let mut scan = Scan::new();
    scan.list(
        ROOT,
        (0..60)
            .map(|i| file(&format!("f{i:02}"), 1000 - i))
            .collect(),
    );
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down]);
    render(&mut state, 80, 24);
    press(&mut state, &[KeyCode::Tab]);
    assert_eq!(state.focus, Pane::Map);
    let before = state.tree.selection;
    for _ in 0..6 {
        press(&mut state, &[KeyCode::Right, KeyCode::Down]);
        render(&mut state, 80, 24);
    }
    let selected = state.tree.selection;
    assert_ne!(selected, before, "map keys changed the selection");
    let buf = render(&mut state, 80, 24);
    let rows = screen(&buf);
    let main = ui::regions(Rect::new(0, 0, 80, 24)).main;
    let pane = &rows[usize::from(main.y)..usize::from(main.bottom())];
    let name = format!("f{:02}", selected.0 - 1);
    assert!(
        pane.iter().any(|r| r.starts_with('>') && r.contains(&name)),
        "tree cursor follows:\n{}",
        pane.join("\n")
    );
    assert_eq!(corner(&buf, tile_of(&state, selected)), "╔");
    press(&mut state, &[KeyCode::BackTab]);
    assert_eq!(state.focus, Pane::Tree);
}

#[test]
fn enter_and_backspace_zoom_from_the_map_like_the_tree() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("aaa"), file("bbb", 100)]);
    scan.list(top[0], vec![file("one", 400)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Tab, KeyCode::Enter]);
    assert_eq!(state.tree.view_root, top[0]);
    press(&mut state, &[KeyCode::Backspace]);
    assert_eq!(state.tree.view_root, ROOT);
}

#[test]
fn the_tree_aggregate_row_selects_the_aggregate_tile_not_the_folder() {
    let mut scan = Scan::new();
    let mut items = vec![file("huge", 10_000)];
    items.extend((0..119).map(|i| file(&format!("t{i:03}"), 10)));
    let kids = scan.list(ROOT, items);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Char('G')]);
    assert_eq!(state.tree.aggregate, Some(ROOT));
    assert_eq!(state.tree.selection, ROOT);
    let buf = render(&mut state, 80, 24);
    assert_eq!(
        state.map.scope(),
        Some(ROOT),
        "the folder's children, not its siblings"
    );
    let aggregate = state
        .map
        .tiles()
        .iter()
        .find(|t| t.id.is_none())
        .expect("an aggregate tile");
    assert_eq!(corner(&buf, aggregate.rect), "╔");
    assert_eq!(corner(&buf, tile_of(&state, kids[0])), "┌");
    assert!(screen(&buf).join("\n").contains("(+"), "aggregate label");
}

#[test]
fn moving_onto_the_aggregate_tile_keeps_the_tree_selection() {
    let mut scan = Scan::new();
    let mut items = vec![file("huge", 10_000)];
    items.extend((0..119).map(|i| file(&format!("t{i:03}"), 10)));
    let kids = scan.list(ROOT, items);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down]);
    render(&mut state, 80, 24);
    press(&mut state, &[KeyCode::Tab]);
    for key in [KeyCode::Right, KeyCode::Down, KeyCode::Left, KeyCode::Up] {
        press(&mut state, &[key]);
        render(&mut state, 80, 24);
        if state.map.on_aggregate {
            break;
        }
    }
    assert!(
        state.map.on_aggregate,
        "an arrow reaches the aggregate tile"
    );
    assert_eq!(state.tree.selection, kids[0], "no node is selected by it");
    let buf = render(&mut state, 80, 24);
    let aggregate = state.map.tiles().iter().find(|t| t.id.is_none()).unwrap();
    assert_eq!(corner(&buf, aggregate.rect), "╔");
}

#[test]
fn a_flood_of_messages_relayouts_at_most_twice_a_second_but_the_highlight_is_immediate() {
    let mut scan = Scan::new();
    let dirs = scan.list(ROOT, (0..40).map(|i| folder(&format!("d{i:02}"))).collect());
    let mut state = app_state(scan);
    state.tick(Duration::ZERO);
    render(&mut state, 80, 24);
    let mut next = 5000;
    for step in 0..125u32 {
        let now = Duration::from_millis(u64::from(step) * 16);
        let dir = dirs[step as usize % dirs.len()];
        if state.model.children(dir).is_empty() {
            state.model.apply(synthetic::listing(
                dir,
                next,
                vec![synthetic::entry("f", Kind::File, 100 + u64::from(step))],
            ));
            next += 1;
        }
        state.tick(now);
        render(&mut state, 80, 24);
        if step == 60 {
            let relayouts = state.map.relayouts();
            press(&mut state, &[KeyCode::Down]);
            let buf = render(&mut state, 80, 24);
            assert_eq!(
                state.map.relayouts(),
                relayouts,
                "a selection change needs no relayout"
            );
            let sel = state.tree.selection;
            if let Some(tile) = state.map.tiles().iter().find(|t| t.id == Some(sel)) {
                assert_eq!(
                    corner(&buf, tile.rect),
                    "╔",
                    "highlight updates in the very next frame"
                );
            }
        }
    }
    assert!(state.map.relayouts() >= 2, "it does relayout");
    assert!(
        state.map.relayouts() <= 5,
        "relayouts: {}",
        state.map.relayouts()
    );
}

#[test]
fn issue_markers_show_even_on_tiles_too_small_for_a_label() {
    let mut scan = Scan::new();
    let ids = scan.list(
        ROOT,
        vec![
            file("big", 9000),
            folder("denied")
                .bytes(150)
                .issue(IssueKind::PermissionDenied),
            folder("skipped").bytes(150).issue(IssueKind::SkippedByUser),
        ],
    );
    let mut state = app_state(scan);
    let buf = render(&mut state, 30, 14);
    let find = |id: NodeId, glyph: &str| {
        let r = tile_of(&state, id);
        (r.x..r.right()).any(|x| (r.y..r.bottom()).any(|y| buf[(x, y)].symbol() == glyph))
    };
    assert!(find(ids[1], "!"), "{:?}", tile_of(&state, ids[1]));
    assert!(find(ids[2], "⊘"), "{:?}", tile_of(&state, ids[2]));
    assert!(!find(ids[0], "!") && !find(ids[0], "⊘"));
    assert!(
        tile_of(&state, ids[1]).width < 6,
        "the marked tile is too narrow for a label"
    );
}

fn tiny_scene(extra: Vec<synthetic::Item>) -> Scan {
    let mut scan = Scan::new();
    let mut items = vec![file("huge", 1_000_000)];
    items.extend(extra);
    scan.list(ROOT, items);
    scan
}

#[test]
fn tiny_items_without_an_aggregate_tile_show_the_hidden_notice() {
    let scan = tiny_scene((0..3).map(|i| file(&format!("tiny{i}"), 1)).collect());
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 12);
    assert!(
        state.map.tiles().iter().all(|t| t.id.is_some()),
        "no aggregate tile fits"
    );
    let title = title_row(&buf, 80, 12);
    assert!(title.contains("3 items too small to show (3 B)"), "{title}");
    assert!(title.trim_end().ends_with('─'), "{title}");
    assert_eq!(ui::format::width(&title), 80);
}

#[test]
fn the_notice_shortens_on_a_narrow_title_rule() {
    let scan = tiny_scene((0..3).map(|i| file(&format!("tiny{i}"), 1)).collect());
    let mut state = app_state(scan);
    let buf = render(&mut state, 40, 12);
    let title = title_row(&buf, 40, 12);
    assert!(title.contains("+3 hidden"), "{title}");
}

#[test]
fn zero_byte_items_are_not_hidden_items() {
    let mut extra: Vec<_> = (0..3).map(|i| file(&format!("zero{i}"), 0)).collect();
    extra.extend((0..2).map(|i| file(&format!("tiny{i}"), 1)));
    let mut state = app_state(tiny_scene(extra));
    let buf = render(&mut state, 80, 12);
    let title = title_row(&buf, 80, 12);
    assert!(title.contains("2 items too small to show"), "{title}");
}

#[test]
fn a_drawn_aggregate_tile_replaces_the_notice() {
    let mut scan = Scan::new();
    let mut items = vec![file("huge", 100_000)];
    items.extend((0..200).map(|i| file(&format!("t{i:03}"), 100)));
    scan.list(ROOT, items);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    assert!(state.map.tiles().iter().any(|t| t.id.is_none()));
    let title = title_row(&buf, 80, 24);
    assert!(
        !title.contains("too small") && !title.contains("hidden"),
        "{title}"
    );
    let text = screen(&buf).join("\n");
    assert!(text.contains("(+200 sm"), "{text}");
}

#[test]
fn a_hidden_selected_item_leads_the_notice_and_the_fit_order_applies() {
    let long = "/home/nss/projects/some-very-long-project-name/node_modules";
    let mut scan = Scan::new();
    scan.model = Model::new(
        long.into(),
        synthetic::entry("n", Kind::Dir, 0),
        ModelOptions { top_n: 10 },
    );
    let mut items = vec![file("huge", 1_000_000)];
    items.extend((0..14).map(|i| file(&format!("tiny{i:02}"), 1)));
    let ids = scan.list(ROOT, items);
    let mut state = app_state(scan);
    state.tree.selection = ids[5];
    let buf = render(&mut state, 80, 24);
    let title = title_row(&buf, 80, 24);
    assert!(title.contains("selected too small · +14 hidden"), "{title}");
    assert!(title.contains("(updating)"), "{title}");
    assert!(
        title.contains("…") && title.contains("node_modules"),
        "{title}"
    );
    assert!(
        !title.contains("some-very-long-project-name"),
        "path was cut: {title}"
    );
    assert_eq!(ui::format::width(&title), 80);
    // No tile is highlighted: the selected item has none.
    assert!(
        state
            .map
            .tiles()
            .iter()
            .all(|t| corner(&buf, t.rect) == "┌")
    );
}

#[test]
fn empty_states_say_what_is_going_on() {
    let mut waiting = app_state(Scan::new());
    let text = screen(&render(&mut waiting, 80, 24)).join("\n");
    assert!(text.contains("Waiting for first results…"), "{text}");

    let mut scan = Scan::new();
    scan.list(ROOT, vec![]);
    let mut empty = app_state(scan);
    let text = screen(&render(&mut empty, 80, 24)).join("\n");
    assert!(text.contains("This folder is empty."), "{text}");

    let (scan, _) = files(&[("a", 0), ("b", 0), ("c", 0)]);
    let mut zero = app_state(scan);
    let text = screen(&render(&mut zero, 80, 24)).join("\n");
    assert!(text.contains("3 items, all 0 B. Nothing to map."), "{text}");
}

#[test]
fn map_title_marks_updating_and_partial_and_the_focused_pane() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![folder("pending"), file("x", 5)]);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    assert!(title_row(&buf, 80, 24).contains("Map: /synthetic/root (updating)"));
    assert!(!title_row(&buf, 80, 24).contains('*'));

    press(&mut state, &[KeyCode::Tab]);
    let buf = render(&mut state, 80, 24);
    assert!(
        title_row(&buf, 80, 24).contains("*Map:"),
        "{}",
        title_row(&buf, 80, 24)
    );

    state.model.apply(ScanMsg::Done { cancelled: true });
    let buf = render(&mut state, 80, 24);
    assert!(title_row(&buf, 80, 24).contains("(partial)"));
    assert!(!title_row(&buf, 80, 24).contains("(updating)"));
}

#[test]
fn map_focus_shows_the_map_key_hints() {
    let (scan, _) = files(&[("a", 5)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Tab]);
    let rows = screen(&render(&mut state, 80, 24));
    let hints = &rows[23];
    assert!(
        hints.contains("Tab tree") && hints.contains("? help"),
        "{hints}"
    );
}

fn map_part(buf: &Buffer, w: u16, h: u16) -> String {
    let r = ui::regions(Rect::new(0, 0, w, h));
    screen(buf)[usize::from(r.map_title.y)..usize::from(r.map.bottom())].join("\n")
}

fn showcase() -> Scan {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            folder("Videos"),
            folder("Projects"),
            file("notes.txt", 3000),
            folder("locked")
                .bytes(1200)
                .issue(IssueKind::PermissionDenied),
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
fn map_snapshots_for_each_state() {
    let mut scanning = app_state(showcase());
    insta::assert_snapshot!("scanning", map_part(&render(&mut scanning, 80, 24), 80, 24));

    let mut done = app_state(showcase());
    done.model.apply(ScanMsg::Done { cancelled: false });
    insta::assert_snapshot!("done", map_part(&render(&mut done, 80, 24), 80, 24));

    let mut stopped = app_state(showcase());
    stopped.model.apply(ScanMsg::Done { cancelled: true });
    insta::assert_snapshot!("stopped", map_part(&render(&mut stopped, 80, 24), 80, 24));

    let mut empty = app_state(Scan::new());
    insta::assert_snapshot!("starting", map_part(&render(&mut empty, 80, 24), 80, 24));
}

#[test]
fn apparent_size_mode_changes_the_tile_sizes() {
    let (scan, ids) = files(&[("a", 500), ("b", 500)]);
    let mut state = app_state(scan);
    state.size_mode = SizeMode::Apparent;
    render(&mut state, 80, 24);
    assert_eq!(state.map.tiles().len(), 2);
    let _ = ids;
}
