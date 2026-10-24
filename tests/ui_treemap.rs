//! Treemap widget (T017): scope, tiles, sync with the tree, throttling, markers, hidden items.

#[path = "support/synthetic.rs"]
mod synthetic;

use std::collections::HashMap;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::map_state::Pane;
use storage_analyzer::app::state::AppState;
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, Kind, NodeId, SizeMode};
use storage_analyzer::ui;
use storage_analyzer::ui::theme::{ColorDepth, Theme};
use storage_analyzer::ui::tile_colors::{Fill, Hue};
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

fn perimeter(r: Rect) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    for x in r.x..r.right() {
        cells.push((x, r.y));
        cells.push((x, r.bottom() - 1));
    }
    for y in r.y..r.bottom() {
        cells.push((r.x, y));
        cells.push((r.right() - 1, y));
    }
    cells
}

#[test]
fn the_selected_tile_frame_is_reverse_bold_with_a_marked_title_and_keeps_its_fill() {
    let (scan, ids) = files(&[("movie.mkv", 600), ("notes.txt", 400)]);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down]);
    for focus_map in [false, true] {
        if focus_map {
            press(&mut state, &[KeyCode::Tab]);
        }
        let buf = render(&mut state, 80, 24);
        let r = tile_of(&state, ids[0]);
        for (x, y) in perimeter(r) {
            let m = buf[(x, y)].modifier;
            assert!(
                m.contains(Modifier::REVERSED | Modifier::BOLD),
                "frame cell ({x},{y}) focus_map={focus_map}"
            );
        }
        assert_eq!(buf[(r.x + 1, r.y)].symbol(), ">", "the title starts with >");
        let inner = &buf[(r.x + 2, r.bottom() - 2)];
        assert_eq!(
            inner.bg,
            Color::Rgb(0xE6, 0x9F, 0x00),
            "interior keeps its fill"
        );
        assert!(!inner.modifier.contains(Modifier::REVERSED));
        let other = tile_of(&state, ids[1]);
        for (x, y) in perimeter(other) {
            assert!(
                !buf[(x, y)].modifier.contains(Modifier::REVERSED),
                "other tiles are plain"
            );
        }
    }
}

#[test]
fn tile_frames_use_the_default_background_and_only_the_interior_is_filled() {
    let (scan, ids) = files(&[("movie.mkv", 600), ("notes.txt", 400)]);
    let mut state = app_state(scan);
    let buf = render(&mut state, 80, 24);
    for id in ids {
        let r = tile_of(&state, id);
        for (x, y) in perimeter(r) {
            assert_eq!(buf[(x, y)].bg, Color::Reset, "frame cell ({x},{y})");
        }
        assert_ne!(buf[(r.x + 2, r.bottom() - 2)].bg, Color::Reset);
    }
}

fn rgb_of(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        // The xterm-256 colour cube and grey ramp.
        Color::Indexed(n @ 16..=231) => {
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            let n = n - 16;
            (level(n / 36), level(n % 36 / 6), level(n % 6))
        }
        Color::Indexed(n @ 232..=255) => {
            let v = 8 + 10 * (n - 232);
            (v, v, v)
        }
        other => panic!("not a palette colour: {other:?}"),
    }
}

fn luminance(c: Color) -> f64 {
    let (r, g, b) = rgb_of(c);
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

const ALL_HUES: [Hue; 8] = [
    Hue::Yellow,
    Hue::Blue,
    Hue::Orange,
    Hue::BluishGreen,
    Hue::SkyBlue,
    Hue::Vermillion,
    Hue::ReddishPurple,
    Hue::Grey,
];

#[test]
fn the_palette_meets_the_contrast_floor_in_truecolor_and_256() {
    for (depth, minimum) in [
        (ColorDepth::TrueColor, 5.19),
        (ColorDepth::Indexed256, 5.53),
    ] {
        let mut theme = Theme::standard();
        theme.depth = depth;
        for hue in ALL_HUES {
            let sw = theme.swatch(hue).expect("a swatch");
            let c = contrast(sw.fill, sw.label);
            assert!(c >= 4.5, "{hue:?} {depth:?}: {c:.2}");
            assert!(
                c >= minimum - 0.01,
                "{hue:?} {depth:?}: {c:.2} below the spec minimum {minimum}"
            );
        }
    }
    let mut mono = Theme::standard();
    mono.depth = ColorDepth::Mono;
    assert!(ALL_HUES.iter().all(|&h| mono.swatch(h).is_none()));
    let t = Theme::standard();
    assert_eq!(
        t.swatch(Hue::Orange).unwrap().fill,
        Color::Rgb(0xE6, 0x9F, 0x00)
    );
    let mut t256 = Theme::standard();
    t256.depth = ColorDepth::Indexed256;
    assert_eq!(t256.swatch(Hue::Orange).unwrap().fill, Color::Indexed(178));
    assert_eq!(t256.swatch(Hue::Grey).unwrap().fill, Color::Indexed(250));
    assert_eq!(rgb_of(Color::Indexed(221)), (0xFF, 0xD7, 0x5F));
}

fn touch(a: Rect, b: Rect) -> bool {
    let overlap = |a0: u16, a1: u16, b0: u16, b1: u16| a1.min(b1) > a0.max(b0);
    (a.right() == b.x || b.right() == a.x) && overlap(a.y, a.bottom(), b.y, b.bottom())
        || (a.bottom() == b.y || b.bottom() == a.y) && overlap(a.x, a.right(), b.x, b.right())
}

/// The interior fill of a tile, read from the buffer.
fn interior(buf: &Buffer, r: Rect) -> Color {
    buf[(r.x + r.width / 2, r.y + r.height - 2)].bg
}

fn depth(state: &mut AppState, d: ColorDepth) {
    state.theme.depth = d;
}

/// A scope folder with the given direct files and `folders` sub-folders. The folders have a size
/// of their own and are still being scanned, so tests can grow them with later listings.
fn scope_with(files: &[(&str, u64)], folders: usize) -> (AppState, Vec<NodeId>, Vec<NodeId>) {
    let mut scan = Scan::new();
    let mut items: Vec<_> = files.iter().map(|(n, b)| file(n, *b)).collect();
    items.extend((0..folders).map(|i| folder(&format!("dir{i}")).bytes(3000 - 100 * i as u64)));
    let ids = scan.list(ROOT, items);
    let (file_ids, dir_ids) = ids.split_at(files.len());
    (app_state(scan), file_ids.to_vec(), dir_ids.to_vec())
}

#[test]
fn file_tiles_use_their_type_hue_and_the_aggregate_is_the_only_grey() {
    let mut files: Vec<_> = vec![("movie.mkv", 100_000u64), ("code.rs", 50_000)];
    let tiny: Vec<String> = (0..200).map(|i| format!("t{i:03}.rs")).collect();
    files.extend(tiny.iter().map(|n| (n.as_str(), 100)));
    let (mut state, ids, _) = scope_with(&files, 0);
    let buf = render(&mut state, 80, 24);
    assert_eq!(
        interior(&buf, tile_of(&state, ids[0])),
        Color::Rgb(0xE6, 0x9F, 0x00),
        "video"
    );
    assert_eq!(
        interior(&buf, tile_of(&state, ids[1])),
        Color::Rgb(0xF0, 0xE4, 0x42),
        "code"
    );
    let aggregate = state
        .map
        .tiles()
        .iter()
        .find(|t| t.id.is_none())
        .expect("aggregate");
    assert_eq!(interior(&buf, aggregate.rect), Color::Rgb(0xBB, 0xBB, 0xBB));
    for t in state.map.tiles().iter().filter(|t| t.id.is_some()) {
        assert_ne!(
            interior(&buf, t.rect),
            Color::Rgb(0xBB, 0xBB, 0xBB),
            "only the aggregate is grey"
        );
    }
}

#[test]
fn folders_never_use_the_hue_of_a_file_type_in_the_same_view() {
    for d in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
        let (mut state, files, folders) = scope_with(&[("a.mp4", 4000), ("b.zip", 3000)], 6);
        depth(&mut state, d);
        let buf = render(&mut state, 80, 24);
        let mut theme = Theme::standard();
        theme.depth = d;
        let orange = theme.swatch(Hue::Orange).unwrap().fill;
        let green = theme.swatch(Hue::BluishGreen).unwrap().fill;
        assert_eq!(interior(&buf, tile_of(&state, files[0])), orange);
        assert_eq!(interior(&buf, tile_of(&state, files[1])), green);
        for id in folders {
            let fill = interior(&buf, tile_of(&state, id));
            assert!(
                fill != orange && fill != green,
                "{d:?}: folder {id:?} uses a file-type hue"
            );
            assert_ne!(fill, Color::Reset, "{d:?}: folders are filled");
        }
    }
}

#[test]
fn touching_tiles_never_share_a_fill_while_free_hues_remain() {
    let (mut state, _, _) = scope_with(&[], 6);
    let buf = render(&mut state, 80, 24);
    let tiles = state.map.tiles().to_vec();
    assert_eq!(tiles.len(), 6);
    let mut touching = 0;
    for (i, a) in tiles.iter().enumerate() {
        for b in &tiles[i + 1..] {
            if touch(a.rect, b.rect) {
                touching += 1;
                assert_ne!(
                    interior(&buf, a.rect),
                    interior(&buf, b.rect),
                    "{:?} / {:?}",
                    a.rect,
                    b.rect
                );
            }
        }
    }
    assert!(touching >= 5);
    let (mut seven, _, _) = scope_with(&[], 7);
    let buf = render(&mut seven, 80, 24);
    let tiles = seven.map.tiles().to_vec();
    for (i, a) in tiles.iter().enumerate() {
        for b in &tiles[i + 1..] {
            if touch(a.rect, b.rect) {
                assert_ne!(interior(&buf, a.rect), interior(&buf, b.rect));
            }
        }
    }
}

fn colour_map(state: &AppState) -> HashMap<NodeId, Fill> {
    state
        .map
        .tiles()
        .iter()
        .zip(state.map.fills())
        .filter_map(|(t, f)| t.id.map(|id| (id, *f)))
        .collect()
}

/// Advances the loop clock by one relayout interval and applies a listing that changes a size.
fn nudge(state: &mut AppState, now: &mut Duration, dir: NodeId, next: &mut u32, bytes: u64) {
    *now += Duration::from_millis(600);
    state.tick(*now);
    state.model.apply(synthetic::listing(
        dir,
        *next,
        vec![synthetic::entry("g.rs", Kind::File, bytes)],
    ));
    *next += 1;
}

#[test]
fn colours_stay_put_while_sizes_change_and_move_once_for_a_new_file_type() {
    // Seven folders: the seventh stays unscanned until the end, to give the last step a size change.
    let (mut state, _, mut folders) = scope_with(&[("a.mp4", 5000)], 7);
    let spare = folders.pop().expect("a spare folder");
    let (mut now, mut next) = (Duration::ZERO, 9000);
    state.tick(now);
    render(&mut state, 80, 24);
    let first = colour_map(&state);
    let relayouts_before = state.map.relayouts();
    // Twenty size changes: every relayout reshuffles sizes and may reorder the tiles.
    for round in 0..20u64 {
        let dir = folders[(round % 6) as usize];
        if state.model.children(dir).is_empty() {
            nudge(&mut state, &mut now, dir, &mut next, 50 + round * 211);
        } else {
            now += Duration::from_millis(600);
            state.tick(now);
        }
        render(&mut state, 80, 24);
    }
    assert!(
        state.map.relayouts() >= relayouts_before + 5,
        "the map did relayout"
    );
    let later = colour_map(&state);
    for id in &folders {
        assert_eq!(
            first[id], later[id],
            "folder {id:?} was recoloured by size changes alone"
        );
    }

    // A new file type shows up in the scope: only the folder that used that hue moves, once.
    let Fill::Hue(taken) = later[&folders[0]] else {
        panic!("filled")
    };
    state.tile_colors.reserve(ROOT, [taken]);
    nudge(&mut state, &mut now, spare, &mut next, 77);
    render(&mut state, 80, 24);
    let moved = colour_map(&state);
    assert_ne!(
        moved[&folders[0]],
        Fill::Hue(taken),
        "the folder left the newly reserved hue"
    );
    for id in folders.iter().filter(|&&f| f != folders[0]) {
        if later[id] != Fill::Hue(taken) {
            assert_eq!(moved[id], later[id], "{id:?} was recoloured needlessly");
        }
    }
    for _ in 0..5 {
        now += Duration::from_millis(600);
        state.tick(now);
        render(&mut state, 80, 24);
    }
    assert_eq!(colour_map(&state), moved, "it moved once and then stayed");
}

#[test]
fn zooming_in_and_out_keeps_every_folders_colour() {
    let (mut state, _, folders) = scope_with(&[("a.mp4", 5000)], 6);
    for &d in &folders {
        state.model.apply(synthetic::listing(
            d,
            100 + d.0,
            vec![synthetic::entry("x.rs", Kind::File, 40)],
        ));
    }
    render(&mut state, 80, 24);
    let before = colour_map(&state);
    // Zoom into the first folder (scope changes to its children), then back out.
    press(&mut state, &[KeyCode::Down, KeyCode::Down, KeyCode::Enter]);
    render(&mut state, 80, 24);
    assert_ne!(state.map.scope(), Some(ROOT));
    press(&mut state, &[KeyCode::Backspace]);
    render(&mut state, 80, 24);
    assert_eq!(state.map.scope(), Some(ROOT));
    assert_eq!(colour_map(&state), before);
}

#[test]
fn with_all_seven_types_present_folders_are_unfilled() {
    let files = [
        ("a.rs", 900),
        ("b.bin", 800),
        ("c.mp4", 700),
        ("d.zip", 600),
        ("e.png", 500),
        ("f.pdf", 400),
        ("g.mp3", 300),
    ];
    let (mut state, _, folders) = scope_with(&files, 2);
    let buf = render(&mut state, 100, 24);
    for id in folders {
        let r = tile_of(&state, id);
        assert_eq!(interior(&buf, r), Color::Reset, "folder {id:?} is unfilled");
        for (x, y) in perimeter(r) {
            assert_eq!(buf[(x, y)].bg, Color::Reset);
        }
    }
}

#[test]
fn labels_distinguish_folders_from_files() {
    let (mut state, files, folders) = scope_with(&[("movie.mkv", 6000)], 2);
    let buf = render(&mut state, 100, 24);
    let text = |r: Rect| {
        (r.x..r.right())
            .map(|x| buf[(x, r.y)].symbol().to_owned())
            .collect::<String>()
    };
    let file_label = text(tile_of(&state, files[0]));
    assert!(
        file_label.contains("movie.mkv") && file_label.contains("[V]"),
        "{file_label}"
    );
    for id in folders {
        let r = tile_of(&state, id);
        let all: String = (r.y..r.bottom())
            .map(|y| {
                (r.x..r.right())
                    .map(|x| buf[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect();
        assert!(all.contains("dir"), "{all}");
        assert!(
            all.contains('/') && !all.contains("[C]") && !all.contains("[V]") && !all.contains('['),
            "{all}"
        );
    }
}

#[test]
fn without_colours_nothing_is_filled_but_labels_still_tell_files_from_folders() {
    let (mut state, files, folders) = scope_with(&[("movie.mkv", 6000)], 3);
    depth(&mut state, ColorDepth::Mono);
    let buf = render(&mut state, 100, 24);
    for t in state.map.tiles() {
        for y in t.rect.y..t.rect.bottom() {
            for x in t.rect.x..t.rect.right() {
                assert_eq!(buf[(x, y)].bg, Color::Reset, "no fill at ({x},{y})");
            }
        }
    }
    let r = tile_of(&state, files[0]);
    let top: String = (r.x..r.right())
        .map(|x| buf[(x, r.y)].symbol().to_owned())
        .collect();
    assert!(top.contains("[V]"), "{top}");
    let r = tile_of(&state, folders[0]);
    let top: String = (r.x..r.right())
        .map(|x| buf[(x, r.y)].symbol().to_owned())
        .collect();
    assert!(top.contains("dir0/") && !top.contains('['), "{top}");
}

#[test]
fn the_reserved_set_counts_small_files_never_shrinks_and_is_rebuilt_on_demand() {
    // A tiny .zip with no tile of its own still reserves bluish green.
    let mut files = vec![("big.mp4", 100_000u64), ("tiny.zip", 1)];
    let names: Vec<String> = (0..2).map(|i| format!("t{i}.rs")).collect();
    files.extend(names.iter().map(|n| (n.as_str(), 1)));
    let (mut state, _, folders) = scope_with(&files, 2);
    let buf = render(&mut state, 80, 24);
    assert!(
        state.map.hidden().iter().any(|_| true),
        "small files have no tile"
    );
    let green = Theme::standard().swatch(Hue::BluishGreen).unwrap().fill;
    for id in folders {
        assert_ne!(interior(&buf, tile_of(&state, id)), green);
    }
    let set = state.tile_colors.reserved[&ROOT];
    assert!(
        set.contains(Hue::BluishGreen) && set.contains(Hue::Orange) && set.contains(Hue::Yellow)
    );

    // Fewer files later (a delete) does not shrink it.
    let mut after = state.tile_colors.reserve(ROOT, []);
    assert!(after.contains(Hue::BluishGreen));
    after = state.tile_colors.reserve(ROOT, [Hue::Blue]);
    assert!(after.contains(Hue::BluishGreen) && after.contains(Hue::Blue));

    // A rescan of the folder rebuilds it from what is there now.
    state.tile_colors.forget_reserved(ROOT);
    assert!(!state.tile_colors.reserved.contains_key(&ROOT));
    state.model.apply(synthetic::listing(
        folders_first(&state),
        4000,
        vec![synthetic::entry("n.rs", Kind::File, 55)],
    ));
    state.tick(Duration::from_secs(10));
    render(&mut state, 80, 24);
    let rebuilt = state.tile_colors.reserved[&ROOT];
    assert!(rebuilt.contains(Hue::BluishGreen) && !rebuilt.contains(Hue::Blue));
}

fn folders_first(state: &AppState) -> NodeId {
    state
        .model
        .children(ROOT)
        .iter()
        .copied()
        .find(|&c| state.model.kind(c) == Kind::Dir)
        .expect("a folder")
}

fn zoomed_scene() -> (AppState, Vec<NodeId>) {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("aaa"), file("zzz", 5)]);
    let kids = scan.list(top[0], vec![file("one.mkv", 700), file("two.mkv", 300)]);
    (app_state(scan), kids)
}

#[test]
fn with_the_parent_row_selected_the_map_shows_the_same_tiles_but_highlights_none() {
    let (mut state, kids) = zoomed_scene();
    press(&mut state, &[KeyCode::Down, KeyCode::Enter]);
    let at_root_row = render(&mut state, 100, 24);
    let tiles_before: Vec<_> = state.map.tiles().to_vec();
    assert!(tiles_before.iter().any(|t| t.id == Some(kids[0])));

    press(&mut state, &[KeyCode::Up]);
    assert!(state.tree.on_parent);
    let buf = render(&mut state, 100, 24);
    assert_eq!(state.map.tiles(), tiles_before.as_slice());
    for t in state.map.tiles() {
        assert_eq!(corner(&buf, t.rect), "┌", "no tile is highlighted");
        assert!(
            !buf[(t.rect.x, t.rect.y)]
                .modifier
                .contains(Modifier::REVERSED)
        );
    }
    let title = title_row(&buf, 100, 24);
    assert!(title.contains(" · ⏎ goes up to /synthetic/root"), "{title}");
    assert!(!title_row(&at_root_row, 100, 24).contains("goes up"));
}

#[test]
fn the_selected_hidden_notice_never_fires_for_the_parent_row() {
    let mut scan = Scan::new();
    let top = scan.list(ROOT, vec![folder("aaa")]);
    let mut items = vec![file("huge", 1_000_000)];
    items.extend((0..4).map(|i| file(&format!("tiny{i}"), 1)));
    scan.list(top[0], items);
    let mut state = app_state(scan);
    press(&mut state, &[KeyCode::Down, KeyCode::Enter, KeyCode::Up]);
    let buf = render(&mut state, 100, 24);
    let title = title_row(&buf, 100, 24);
    assert!(!title.contains("selected"), "{title}");
    assert!(
        title.contains("4 items too small to show") || title.contains("+4 hidden"),
        "{title}"
    );
}

#[test]
fn tab_from_the_parent_row_focuses_the_map_with_the_largest_tile_selected() {
    let (mut state, kids) = zoomed_scene();
    press(
        &mut state,
        &[KeyCode::Down, KeyCode::Enter, KeyCode::Up, KeyCode::Tab],
    );
    assert_eq!(state.focus, Pane::Map);
    assert!(!state.tree.on_parent);
    assert_eq!(state.tree.selection, kids[0], "the largest tile");
    let buf = render(&mut state, 100, 24);
    assert_eq!(corner(&buf, tile_of(&state, kids[0])), "╔");
}
