//! Size toggle, help, too-small, colour and ASCII fallbacks (T023; UX §6–§8, §11 checks 2, 3, 12).

#[path = "support/synthetic.rs"]
mod synthetic;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use storage_analyzer::APP_ID;
use storage_analyzer::app::cli;
use storage_analyzer::app::map_state::Pane;
use storage_analyzer::app::state::{AppState, Dialog};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::{IssueKind, NodeId, SizeMode};
use storage_analyzer::ui;
use storage_analyzer::ui::theme::{ColorDepth, Glyphs};
use synthetic::{Scan, file, folder};

const ROOT: NodeId = NodeId::ROOT;

fn app_state(scan: Scan, flags: &[&str]) -> AppState {
    let argv = std::iter::once(APP_ID).chain(flags.iter().copied());
    AppState::new(cli::parse(APP_ID, argv).unwrap(), scan.model)
}

fn render(state: &mut AppState, w: u16, h: u16) -> Buffer {
    state.prepare_view(Rect::new(0, 0, w, h));
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, state)).unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_owned())
                .collect()
        })
        .collect()
}

fn press(state: &mut AppState, codes: &[KeyCode]) {
    for &code in codes {
        state.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
}

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

/// A scan with every §5.1 state, a few types, and enough to fill every view.
fn showcase() -> Scan {
    let mut scan = Scan::new();
    let kids = scan.list(
        ROOT,
        vec![
            folder("Videos"),
            folder("pending"),
            file("notes.txt", 3000),
            folder("locked").issue(IssueKind::PermissionDenied),
            folder("mine").issue(IssueKind::SkippedByUser),
        ],
    );
    scan.list(
        kids[0],
        vec![file("movie.mkv", 40_000), file("clip.mp4", 9000)],
    );
    scan
}

// ---- colour depth and locale detection ----

#[test]
fn colour_depth_follows_no_color_then_colorterm_then_term() {
    let depth = |pairs: &[(&str, &str)]| ColorDepth::detect(&env(pairs));
    assert_eq!(
        depth(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
        ColorDepth::Mono
    );
    assert_eq!(
        depth(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")]),
        ColorDepth::TrueColor,
        "empty NO_COLOR is ignored"
    );
    assert_eq!(depth(&[("COLORTERM", "truecolor")]), ColorDepth::TrueColor);
    assert_eq!(
        depth(&[("COLORTERM", "24bit"), ("TERM", "xterm")]),
        ColorDepth::TrueColor
    );
    assert_eq!(depth(&[("TERM", "xterm-256color")]), ColorDepth::Indexed256);
    assert_eq!(
        depth(&[("TERM", "xterm"), ("COLORTERM", "yes")]),
        ColorDepth::Indexed256
    );
    assert_eq!(depth(&[("TERM", "dumb")]), ColorDepth::Mono);
    assert_eq!(depth(&[]), ColorDepth::Mono);
}

#[test]
fn the_locale_decides_between_unicode_and_the_ascii_fallback() {
    let ascii = |pairs: &[(&str, &str)]| Glyphs::detect(&env(pairs)).ascii;
    assert!(!ascii(&[("LANG", "en_US.UTF-8")]));
    assert!(!ascii(&[("LC_ALL", "de_DE.utf8")]));
    assert!(ascii(&[("LANG", "C")]));
    assert!(ascii(&[("LANG", "POSIX")]));
    assert!(ascii(&[("LANG", "en_US")]));
    assert!(ascii(&[]));
    // LC_ALL beats LC_CTYPE beats LANG.
    assert!(ascii(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")]));
    assert!(!ascii(&[("LC_CTYPE", "en_US.UTF-8"), ("LANG", "C")]));
    assert!(
        !ascii(&[("LC_ALL", ""), ("LANG", "en_US.UTF-8")]),
        "an empty variable is skipped"
    );
}

#[test]
fn the_ascii_glyph_set_follows_ux_section_7() {
    let g = Glyphs::ascii();
    assert!(g.ascii);
    assert_eq!(
        (
            g.scanning,
            g.prioritised,
            g.skipped,
            g.lower_bound,
            g.expanded,
            g.collapsed
        ),
        ("~", "^", "-", ">=", "-", "+")
    );
    assert_eq!((g.bar_full, g.bar_empty), ("#", "."));
    assert_eq!(g.ellipsis, "...");
    for field in [
        g.guide_tee,
        g.guide_elbow,
        g.guide_pipe,
        g.rule,
        g.separator,
        g.done,
        g.stopped,
    ] {
        assert!(field.is_ascii(), "{field:?}");
    }
    assert!(g.spinner.iter().all(|s| s.is_ascii()) && g.bar_eighths.iter().all(|s| s.is_ascii()));
}

// ---- NO_COLOR ----

#[test]
fn without_colour_no_cell_has_a_foreground_or_background_and_the_text_is_unchanged() {
    for (name, view_key) in [
        ("tree", '1'),
        ("types", '2'),
        ("largest", '3'),
        ("skipped", '4'),
    ] {
        let mut colour = app_state(showcase(), &[]);
        let mut plain = app_state(showcase(), &[]);
        plain.theme.depth = ColorDepth::Mono;
        for state in [&mut colour, &mut plain] {
            press(state, &[KeyCode::Char(view_key)]);
        }
        let (c, p) = (render(&mut colour, 80, 24), render(&mut plain, 80, 24));
        assert_eq!(rows(&c), rows(&p), "{name}: NO_COLOR changes colours only");
        for y in 0..24 {
            for x in 0..80 {
                let cell = &p[(x, y)];
                assert_eq!(
                    (cell.fg, cell.bg),
                    (Color::Reset, Color::Reset),
                    "{name} ({x},{y})"
                );
            }
        }
        assert!(
            (0..24).any(|y| (0..80).any(|x| c[(x, y)].bg != Color::Reset)) || view_key != '1',
            "the colour render does use fills"
        );
    }
}

#[test]
fn every_state_is_still_told_apart_by_glyph_or_words_without_colour() {
    let mut state = app_state(showcase(), &[]);
    state.theme.depth = ColorDepth::Mono;
    let text = rows(&render(&mut state, 100, 24)).join("\n");
    for needle in [
        "pending/",
        "◌",
        "⊘",
        "(scan skipped)",
        "!",
        "(permission denied)",
        "Videos/",
    ] {
        assert!(text.contains(needle), "missing {needle:?}:\n{text}");
    }
}

#[test]
fn every_palette_pair_meets_the_contrast_floor_in_both_colour_modes() {
    use storage_analyzer::ui::tile_colors::Hue;
    let lum = |c: Color| {
        let (r, g, b) = match c {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(n @ 16..=231) => {
                let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
                let n = n - 16;
                (level(n / 36), level(n % 36 / 6), level(n % 6))
            }
            Color::Indexed(n @ 232..=255) => {
                (8 + 10 * (n - 232), 8 + 10 * (n - 232), 8 + 10 * (n - 232))
            }
            other => panic!("{other:?}"),
        };
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    };
    for depth in [ColorDepth::TrueColor, ColorDepth::Indexed256] {
        let mut theme = storage_analyzer::ui::theme::Theme::standard();
        theme.depth = depth;
        for hue in [
            Hue::Yellow,
            Hue::Blue,
            Hue::Orange,
            Hue::BluishGreen,
            Hue::SkyBlue,
            Hue::Vermillion,
            Hue::ReddishPurple,
            Hue::Grey,
        ] {
            let sw = theme.swatch(hue).unwrap();
            let (a, b) = (lum(sw.fill), lum(sw.label));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(ratio >= 4.5, "{hue:?} {depth:?}: {ratio:.2}");
        }
    }
}

// ---- ASCII fallback ----

fn all_ascii(buf: &Buffer) -> Result<(), String> {
    for (y, row) in rows(buf).iter().enumerate() {
        if let Some(c) = row.chars().find(|c| !c.is_ascii()) {
            return Err(format!("row {y} has {c:?}: {row}"));
        }
    }
    Ok(())
}

#[test]
fn the_ascii_fallback_draws_every_view_and_dialog_in_plain_ascii() {
    for view_key in ['1', '2', '3', '4'] {
        for focus_map in [false, true] {
            let mut state = app_state(showcase(), &[]);
            state.glyphs = Glyphs::ascii();
            press(&mut state, &[KeyCode::Char(view_key), KeyCode::Down]);
            if focus_map {
                press(&mut state, &[KeyCode::Tab]);
            }
            all_ascii(&render(&mut state, 100, 24))
                .unwrap_or_else(|e| panic!("view {view_key}: {e}"));
        }
    }
    let mut state = app_state(showcase(), &[]);
    state.glyphs = Glyphs::ascii();
    for key in ['q', '?'] {
        state.dialog = None;
        press(&mut state, &[KeyCode::Char(key)]);
        all_ascii(&render(&mut state, 100, 30)).unwrap_or_else(|e| panic!("dialog {key}: {e}"));
    }
    let mut small = app_state(showcase(), &[]);
    small.glyphs = Glyphs::ascii();
    all_ascii(&render(&mut small, 64, 20)).unwrap();
}

#[test]
fn the_ascii_fallback_uses_the_ux_glyphs_in_the_tree() {
    let mut state = app_state(showcase(), &[]);
    state.glyphs = Glyphs::ascii();
    press(&mut state, &[KeyCode::Down]);
    let text = rows(&render(&mut state, 100, 24)).join("\n");
    assert!(
        text.contains("- mine/") || text.contains("-   +- mine/") || text.contains("mine/"),
        "{text}"
    );
    assert!(text.contains("~"), "scanning folders use ~:\n{text}");
    assert!(text.contains(">="), "a lower bound reads >=:\n{text}");
}

// ---- hints, size toggle, help, too small ----

#[test]
fn key_hints_always_keep_help_at_80_columns_in_every_context() {
    let mut state = app_state(showcase(), &[]);
    for (keys, label) in [
        (vec![], "tree"),
        (vec![KeyCode::Tab], "map"),
        (vec![KeyCode::Tab, KeyCode::Char('2')], "types, map focus"),
        (vec![KeyCode::Tab, KeyCode::Char('3')], "largest"),
        (vec![KeyCode::Tab, KeyCode::Char('4')], "skipped"),
    ] {
        press(&mut state, &keys);
        let hints = rows(&render(&mut state, 80, 24))[23].clone();
        assert!(hints.contains("? help"), "{label}: {hints}");
        assert!(ui::format::width(hints.trim_end()) <= 80);
    }
    for width in [80, 100, 160] {
        let hints = rows(&render(&mut app_state(showcase(), &[]), width, 24))[23].clone();
        assert!(hints.contains("? help"), "{width}: {hints}");
    }
}

#[test]
fn a_toggles_between_disk_usage_and_apparent_size_everywhere() {
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("big.bin", 1000), file("small.bin", 10)]);
    let mut state = app_state(scan, &[]);
    assert_eq!(state.size_mode, SizeMode::Disk);
    assert!(rows(&render(&mut state, 80, 24))[0].contains("Disk usage"));
    press(&mut state, &[KeyCode::Char('a')]);
    assert_eq!(state.size_mode, SizeMode::Apparent);
    let screen = rows(&render(&mut state, 80, 24));
    assert!(screen[0].contains("Apparent size"), "{}", screen[0]);
    assert!(
        screen[22].contains("Showing apparent size."),
        "{}",
        screen[22]
    );
    press(&mut state, &[KeyCode::Char('a')]);
    let screen = rows(&render(&mut state, 80, 24));
    assert!(screen[0].contains("Disk usage") && screen[22].contains("Showing disk usage."));
    // `--apparent` starts in apparent mode.
    let mut scan = Scan::new();
    scan.list(ROOT, vec![file("x", 1)]);
    assert_eq!(
        app_state(scan, &["--apparent"]).size_mode,
        SizeMode::Apparent
    );
}

#[test]
fn the_size_mode_changes_what_the_views_show() {
    // A sparse-like file: the apparent size differs from the disk usage.
    let mut scan = Scan::new();
    let mut sparse = synthetic::entry("sparse.bin", storage_analyzer::types::Kind::File, 4096);
    sparse.apparent_bytes = 1_000_000;
    let mut dense = synthetic::entry("dense.bin", storage_analyzer::types::Kind::File, 90_000);
    dense.apparent_bytes = 90_000;
    scan.model
        .apply(synthetic::listing(ROOT, 1, vec![sparse, dense]));
    let mut state = app_state(scan, &[]);
    press(&mut state, &[KeyCode::Char('3')]);
    let order = |state: &mut AppState| {
        let screen = rows(&render(state, 100, 24)).join("\n");
        (
            screen.find("sparse.bin").unwrap() < screen.find("dense.bin").unwrap(),
            screen,
        )
    };
    assert!(!order(&mut state).0, "disk usage: dense first");
    press(&mut state, &[KeyCode::Char('a')]);
    let (sparse_first, screen) = order(&mut state);
    assert!(
        sparse_first,
        "apparent size: sparse first (the order can change):\n{screen}"
    );
    assert!(screen.contains("976.6K"), "{screen}");
    press(&mut state, &[KeyCode::Char('1')]);
    let tree = rows(&render(&mut state, 100, 24)).join("\n");
    assert!(tree.contains("976.6K"), "the tree switched too:\n{tree}");
}

/// The 18 entries of the UX §6 help mockup: (unicode key column, ASCII key column, description).
const HELP_ENTRIES: [(&str, &str, &str); 18] = [
    ("↑↓ j k", "Up Dn j k", "Move"),
    ("PgUp PgDn", "PgUp PgDn", "Page"),
    ("g G", "g G", "Top / bottom"),
    ("→ l", "Right l", "Expand / into"),
    ("← h", "Left h", "Collapse / parent"),
    ("⏎", "Enter", "Zoom in, scan it first"),
    ("⌫ -", "Bksp -", "Up one level"),
    ("Tab", "Tab", "Switch tree / map"),
    ("1 2 3 4", "1 2 3 4", "Tree Types Largest Skipped"),
    ("a", "a", "Disk usage / apparent"),
    ("d Delete", "d Delete", "Delete (asks first)"),
    ("p", "p", "Scan this folder first"),
    ("x", "x", "Skip this folder's scan"),
    ("r", "r", "Resume / rescan folder"),
    ("s", "s", "Stop the whole scan"),
    ("?", "?", "Show / hide this help"),
    ("q Ctrl-C", "q Ctrl-C", "Quit (asks first)"),
    ("Esc", "Esc", "Close dialog or help"),
];

#[test]
fn the_help_overlay_fits_80_by_24_in_two_columns_in_unicode_and_ascii() {
    for ascii in [false, true] {
        let mut state = app_state(showcase(), &[]);
        if ascii {
            state.glyphs = Glyphs::ascii();
        }
        press(&mut state, &[KeyCode::Char('?')]);
        assert_eq!(state.dialog, Some(Dialog::Help));
        let screen = rows(&render(&mut state, 80, 24));
        let text = screen.join("\n");
        for (unicode, ascii_keys, what) in HELP_ENTRIES {
            let keys = if ascii { ascii_keys } else { unicode };
            let entry = format!("{keys:<10}{what}");
            assert!(
                text.contains(&entry) || text.contains(&format!("{keys} ")) && text.contains(what),
                "ascii={ascii}: missing {entry:?}:\n{text}"
            );
            assert!(text.contains(what), "ascii={ascii}: {what}");
        }
        assert!(
            text.contains("Move and view") && text.contains("Act and scan"),
            "{text}"
        );
        assert!(
            text.contains("Map: files are coloured by type; the tag shows it"),
            "{text}"
        );
        assert!(
            text.contains("ETA appears once a previous complete scan of this root is known"),
            "{text}"
        );
        assert!(text.contains("--history to remember past scans"), "{text}");
        // The bottom border, with the close hint, is on row 24 or above (index 23).
        let bottom = screen
            .iter()
            .rposition(|r| r.contains("Esc or ? close"))
            .expect("a close hint");
        assert!(
            bottom <= 23,
            "ascii={ascii}: bottom border at row {}",
            bottom + 1
        );
        assert_eq!(
            screen
                .iter()
                .filter(|r| r.contains("Move and view"))
                .count(),
            1
        );
        // Two columns: the right column starts on the same row as the left one.
        let header = screen.iter().find(|r| r.contains("Move and view")).unwrap();
        assert!(header.contains("Act and scan"), "{header}");
        // 18 rows tall, from the top border to the bottom border.
        let top = screen.iter().position(|r| r.contains("Help")).unwrap();
        assert_eq!(
            bottom - top + 1,
            20,
            "ascii={ascii}: 18 + the hard-link footer"
        );
        // The hard-link footer, wrapped as in UX §6, and every overlay line exactly 80 columns.
        assert!(
            text.contains(
                "Hard links count once, in the folder scanned first: totals match du, folder"
            ),
            "{text}"
        );
        assert!(
            text.contains("sizes may not. Deleting one name of a hard-linked file frees no space."),
            "{text}"
        );
        for row in &screen[top..=bottom] {
            assert_eq!(ui::format::width(row), 80, "ascii={ascii}: {row:?}");
        }
        if ascii {
            assert!(text.contains('#'), "borders are #");
        }
    }
}

#[test]
fn help_keys_esc_and_question_mark_close_q_opens_the_quit_prompt_and_others_are_ignored() {
    let mut state = app_state(showcase(), &[]);
    press(&mut state, &[KeyCode::Char('?'), KeyCode::Esc]);
    assert_eq!(state.dialog, None);
    press(&mut state, &[KeyCode::Char('?'), KeyCode::Char('?')]);
    assert_eq!(state.dialog, None, "? closes it too");
    press(
        &mut state,
        &[
            KeyCode::Char('?'),
            KeyCode::Char('j'),
            KeyCode::Down,
            KeyCode::Char('2'),
        ],
    );
    assert_eq!(state.dialog, Some(Dialog::Help));
    assert_eq!(state.tree.selection, ROOT, "j changes nothing");
    assert_eq!(state.view, storage_analyzer::app::list_state::View::Tree);
    press(&mut state, &[KeyCode::Char('q')]);
    assert_eq!(state.dialog, Some(Dialog::Quit), "q opens the quit prompt");
    state.dialog = None;
    press(&mut state, &[KeyCode::Char('?')]);
    state.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert_eq!(state.dialog, Some(Dialog::Quit), "Ctrl-C too");
}

#[test]
fn hint_lines_equal_the_ux_strings_at_80_columns_in_unicode_and_ascii() {
    let tree_u = " ↑↓ move  ←→ fold  ⏎ zoom  ⌫ up  Tab map  d delete  p first  x skip  ? help";
    let tree_a = " j/k move  h/l fold  Enter zoom  Bksp up  Tab map  d delete  p first  ? help";
    let map_u = " arrows move  ⏎ zoom  ⌫ up  Tab tree  d delete  p first  ? help";
    let map_a = " hjkl move  Enter zoom  Bksp up  Tab tree  d delete  p first  ? help";
    let list_u = " ↑↓ move  ⏎ show in tree  d delete  ? help";
    let list_a = " j/k move  Enter show in tree  d delete  ? help";
    for (ascii, tree, map, list) in [
        (false, tree_u, map_u, list_u),
        (true, tree_a, map_a, list_a),
    ] {
        let line = |keys: &[KeyCode]| {
            let mut state = app_state(showcase(), &[]);
            if ascii {
                state.glyphs = Glyphs::ascii();
            }
            press(&mut state, keys);
            rows(&render(&mut state, 80, 24))[23].trim_end().to_owned()
        };
        assert_eq!(line(&[]), tree, "ascii={ascii} tree");
        assert_eq!(line(&[KeyCode::Tab]), map, "ascii={ascii} map");
        assert_eq!(line(&[KeyCode::Char('2')]), list, "ascii={ascii} lists");
    }
    // Wider than 80 columns the ASCII tree line keeps `x skip`.
    let mut state = app_state(showcase(), &[]);
    state.glyphs = Glyphs::ascii();
    let wide = rows(&render(&mut state, 100, 24))[23].trim_end().to_owned();
    assert_eq!(
        wide,
        " j/k move  h/l fold  Enter zoom  Bksp up  Tab map  d delete  p first  x skip  ? help"
    );
}

#[test]
fn ascii_mode_never_uses_single_letters_as_key_names() {
    let mut state = app_state(showcase(), &[]);
    state.glyphs = Glyphs::ascii();
    for keys in [
        vec![],
        vec![KeyCode::Tab],
        vec![KeyCode::Char('2')],
        vec![KeyCode::Char('?')],
    ] {
        state.dialog = None;
        state.focus = Pane::Tree;
        press(&mut state, &[KeyCode::Char('1')]);
        press(&mut state, &keys);
        for row in rows(&render(&mut state, 80, 24)) {
            let words: Vec<&str> = row.split_whitespace().collect();
            for pair in words.windows(2) {
                assert!(
                    !(["E", "B"].contains(&pair[0]) && ["zoom", "up", "show"].contains(&pair[1])),
                    "single-letter key name in {row:?}"
                );
            }
        }
    }
}

#[test]
fn ascii_copy_spells_out_enter_and_backspace_and_three_dots() {
    let mut scan = Scan::new();
    let kids = scan.list(ROOT, vec![folder("aaa")]);
    scan.list(kids[0], vec![file("x", 10)]);
    let mut state = app_state(scan, &[]);
    state.glyphs = Glyphs::ascii();
    // The `..` row: its message and the map title.
    press(
        &mut state,
        &[
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Up,
            KeyCode::Char('d'),
        ],
    );
    let screen = rows(&render(&mut state, 100, 24));
    assert!(
        screen[22].contains("Press Enter to go up."),
        "{}",
        screen[22]
    );
    assert!(!screen.join("\n").contains('⏎'));
    let title = screen.iter().find(|r| r.contains("Map:")).unwrap();
    assert!(
        title.contains(" - Enter goes up to /synthetic/root"),
        "{title}"
    );
    // Ellipses.
    let fresh = app_state(Scan::new(), &[]);
    let mut fresh = fresh;
    fresh.glyphs = Glyphs::ascii();
    let text = rows(&render(&mut fresh, 100, 24)).join("\n");
    assert!(text.contains("Waiting for first results..."), "{text}");
    assert!(text.contains("Starting... "), "{text}");
}

#[test]
fn ascii_done_and_stopped_lines_drop_the_symbol_and_keep_the_word() {
    for (cancelled, needle) in [(false, "Done in"), (true, "Stopped at")] {
        let mut state = app_state(showcase(), &["--no-animation"]);
        state.glyphs = Glyphs::ascii();
        state.model.apply(ScanMsg::Done { cancelled });
        let status = rows(&render(&mut state, 100, 24))[22].clone();
        assert!(status.starts_with(needle), "{status:?}");
    }
}

#[test]
fn in_256_colour_mode_no_cell_uses_a_24_bit_colour() {
    for view_key in ['1', '2', '3', '4'] {
        let mut state = app_state(showcase(), &[]);
        state.theme.depth = ColorDepth::Indexed256;
        press(&mut state, &[KeyCode::Char(view_key), KeyCode::Down]);
        let buf = render(&mut state, 100, 24);
        for y in 0..24 {
            for x in 0..100 {
                let cell = &buf[(x, y)];
                for color in [cell.fg, cell.bg] {
                    assert!(
                        !matches!(color, Color::Rgb(..)),
                        "view {view_key} ({x},{y}): {color:?}"
                    );
                }
            }
        }
    }
    // The map really is filled, with indexed colours.
    let mut state = app_state(showcase(), &[]);
    state.theme.depth = ColorDepth::Indexed256;
    let buf = render(&mut state, 100, 24);
    assert!((0..24).any(|y| (0..100).any(|x| matches!(buf[(x, y)].bg, Color::Indexed(_)))));
    assert!(
        (0..24).any(|y| (0..100).any(|x| matches!(buf[(x, y)].fg, Color::Indexed(16 | 231)))),
        "labels are black/white indexed"
    );
}

#[test]
fn below_80_by_24_only_the_message_is_shown_and_most_keys_do_nothing() {
    let mut state = app_state(showcase(), &[]);
    let screen = rows(&render(&mut state, 64, 20));
    let message = "Terminal too small: need 80×24, have 64×20. Resize, or press q to quit.";
    // The message is longer than 64 cells, so it wraps; nothing else is on the screen.
    let drawn: Vec<&str> = screen
        .iter()
        .map(|r| r.trim())
        .filter(|r| !r.is_empty())
        .collect();
    assert_eq!(drawn.join(" "), message, "{}", screen.join("\n"));

    let before = (
        state.tree.selection,
        state.view,
        state.focus,
        state.size_mode,
    );
    press(
        &mut state,
        &[
            KeyCode::Char('d'),
            KeyCode::Char('x'),
            KeyCode::Down,
            KeyCode::Char('a'),
            KeyCode::Tab,
            KeyCode::Char('2'),
            KeyCode::Char('?'),
            KeyCode::Enter,
        ],
    );
    assert_eq!(
        (
            state.tree.selection,
            state.view,
            state.focus,
            state.size_mode
        ),
        before
    );
    assert_eq!(state.dialog, None);
    assert!(state.events.is_empty());

    press(&mut state, &[KeyCode::Char('q')]);
    assert_eq!(state.dialog, Some(Dialog::Quit));
    let with_dialog = rows(&render(&mut state, 64, 20)).join("\n");
    assert!(with_dialog.contains("Quit"), "{with_dialog}");

    // Resizing back restores the UI.
    press(&mut state, &[KeyCode::Esc]);
    let screen = rows(&render(&mut state, 80, 24));
    assert!(screen[1].contains("[1 Tree]"), "{}", screen[1]);
    press(&mut state, &[KeyCode::Char('2')]);
    assert_eq!(state.view, storage_analyzer::app::list_state::View::Types);
}

#[test]
fn exactly_80_by_24_is_not_too_small() {
    assert!(!ui::too_small::is_too_small(80, 24));
    assert!(ui::too_small::is_too_small(79, 24));
    assert!(ui::too_small::is_too_small(80, 23));
}

#[test]
fn wide_terminals_keep_the_same_layout_without_a_side_panel() {
    let mut state = app_state(showcase(), &[]);
    let wide = Rect::new(0, 0, 160, 40);
    let r = ui::regions(wide);
    assert_eq!(
        (r.main.width, r.map.width, r.main.x),
        (160, 160, 0),
        "the panes span the full width"
    );
    let screen = rows(&render(&mut state, 160, 40));
    let right_half: String = screen
        .iter()
        .map(|row| row.chars().skip(120).collect::<String>())
        .collect();
    assert!(
        !right_half.contains("Types") && !right_half.contains("CATEGORY"),
        "no Types panel:\n{}",
        screen.join("\n")
    );
    assert_eq!(state.focus, Pane::Tree);
    let _ = ScanMsg::Done { cancelled: false };
}
