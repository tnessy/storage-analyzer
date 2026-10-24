//! The treemap strip: tiles, labels, markers, the map title rule and the hidden-items notice.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::tile_colors::Fill;
use super::{copy, format, sanitize};
use crate::app::map_state::Pane;
use crate::app::state::AppState;
use crate::layout::Tile;
use crate::types::{IssueKind, Kind, NodeId, ScanState};

/// Tiles narrower than this get no label (UX §3.3).
const MIN_LABEL_WIDTH: u16 = 6;
const PATH_FIT_FIRST: usize = 24;
const PATH_FIT_SECOND: usize = 12;

fn issue_marker(state: &AppState, id: NodeId) -> Option<&'static str> {
    state.model.issue(id).map(|i| match i.kind {
        IssueKind::SkippedByUser => state.glyphs.skipped,
        _ => state.glyphs.issue,
    })
}

/// `name size pct% [T]` split into the words that make it up.
fn label_words(state: &AppState, tile: &Tile) -> Vec<String> {
    let map = &state.map;
    match tile.id {
        None => {
            let bytes: u64 = map.hidden().iter().map(|&(_, s)| s).sum();
            vec![
                format!("(+{} {})", map.hidden().len(), copy::AGGREGATE_WORD),
                format::size_iec(bytes),
            ]
        }
        Some(id) => {
            let model = &state.model;
            let size = model.size(id, state.size_mode);
            let share = if map.total() == 0 {
                0.0
            } else {
                size as f64 / map.total() as f64
            };
            let only = map.tiles().len() == 1 && map.hidden().is_empty();
            let mut name = sanitize::display_name(model.name(id));
            if model.kind(id) == Kind::Dir {
                name.push('/');
            }
            let mut words = vec![
                name,
                format::size_iec(size),
                format::percent_tile(share, only),
            ];
            // Files carry their type tag; folders end in `/` and have none (UX §3.3).
            if model.kind(id) != Kind::Dir {
                let category = model.category_of_node(id);
                words.push(format!("[{}]", state.theme.category_tag(category)));
            }
            words
        }
    }
}

/// Packs the words into a border-title line and a second line for what didn't fit.
fn split_label(words: &[String], width: usize, ellipsis: &str) -> (String, String) {
    let Some((name, rest)) = words.split_first() else {
        return (String::new(), String::new());
    };
    let mut first = format::truncate(name, width, ellipsis);
    let mut taken = 0;
    if first == *name {
        for word in rest {
            if format::width(&first) + 1 + format::width(word) > width {
                break;
            }
            first.push(' ');
            first.push_str(word);
            taken += 1;
        }
    }
    let second = if first == *name || taken > 0 || format::width(name) > width {
        rest[taken.min(rest.len())..].join(" ")
    } else {
        rest.join(" ")
    };
    (first, format::truncate(&second, width, ellipsis))
}

fn draw_tile(frame: &mut Frame, state: &AppState, index: usize, selected: bool) {
    let tile = &state.map.tiles()[index];
    let r = tile.rect;
    // An unfilled folder, or a theme without colours, is drawn on the default background.
    let swatch = match state.map.fills().get(index) {
        Some(Fill::Hue(hue)) => state.theme.swatch(*hue),
        _ => None,
    };
    let mut paint = swatch.map_or_else(Style::new, |s| Style::new().bg(s.fill).fg(s.label));
    let frame_style = if selected {
        state.theme.tile_frame_selected
    } else {
        Style::new()
    };
    let marker = tile.id.and_then(|id| issue_marker(state, id));
    let words = label_words(state, tile);
    let bordered = r.width >= 2 && r.height >= 2;
    let labelled = bordered && r.width >= MIN_LABEL_WIDTH;
    if bordered {
        // The frame and its title row stay on the terminal's colors; only the inside is filled.
        let border = if selected {
            state.glyphs.map_selected_border
        } else {
            state.glyphs.map_border
        };
        let mut block = Block::bordered()
            .border_type(border)
            .border_style(frame_style)
            .title_style(frame_style);
        let mark = if selected { state.glyphs.selection } else { "" };
        let reserve = if marker.is_some() { 2 } else { 0 };
        let room =
            usize::from(r.width.saturating_sub(2)).saturating_sub(reserve + format::width(mark));
        let (first, second) = split_label(&words, room, state.glyphs.ellipsis);
        if labelled {
            block = block.title(format!("{mark}{first}"));
        }
        let inner = block.inner(r);
        frame.render_widget(block, r);
        frame.render_widget(Block::new().style(paint), inner);
        if labelled && inner.height > 0 && !second.is_empty() {
            frame.render_widget(Paragraph::new(second).style(paint), inner);
        }
    } else {
        if selected {
            paint = paint.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        frame.render_widget(Block::new().style(paint), r);
    }
    if let Some(glyph) = marker {
        let (at, glyph_style) = if labelled {
            (
                (r.right() - 2, r.y),
                frame_style.add_modifier(Modifier::BOLD),
            )
        } else {
            (
                (r.x + r.width / 2, r.y + r.height / 2),
                paint.add_modifier(Modifier::BOLD),
            )
        };
        let cell = Rect::new(at.0, at.1, 1, 1);
        frame.render_widget(Paragraph::new(glyph).style(glyph_style), cell);
    }
}

enum Empty {
    Waiting,
    Folder,
    AllZero(usize),
}

fn empty_state(state: &AppState) -> Option<Empty> {
    let map = &state.map;
    if !map.tiles().is_empty() || !map.hidden().is_empty() {
        return None;
    }
    if map.child_count() > 0 {
        return Some(Empty::AllZero(map.child_count()));
    }
    let scope = map.scope()?;
    if state.model.state() == ScanState::Starting || !state.model.is_complete(scope) {
        Some(Empty::Waiting)
    } else {
        Some(Empty::Folder)
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    frame.render_widget(Clear, area);
    match empty_state(state) {
        Some(Empty::Waiting) => frame.render_widget(Paragraph::new(copy::MAP_WAITING), area),
        Some(Empty::Folder) => frame.render_widget(Paragraph::new(copy::MAP_EMPTY_FOLDER), area),
        Some(Empty::AllZero(n)) => {
            let text = format!("{n} {}", copy::MAP_ALL_ZERO);
            frame.render_widget(Paragraph::new(text), area);
        }
        None => {
            let selected = state.map.selected_index(&state.tree);
            for i in 0..state.map.tiles().len() {
                draw_tile(frame, state, i, selected == Some(i));
            }
        }
    }
}

// ---- the title rule ----

struct Notice {
    count: usize,
    bytes: u64,
    selected_hidden: bool,
}

fn notice(state: &AppState) -> Option<Notice> {
    let map = &state.map;
    let hidden = map.hidden();
    if hidden.is_empty() || map.tiles().iter().any(|t| t.id.is_none()) {
        return None;
    }
    let on_node = state.tree.aggregate.is_none() && !map.on_aggregate && !state.tree.on_parent;
    Some(Notice {
        count: hidden.len(),
        bytes: hidden.iter().map(|&(_, s)| s).sum(),
        selected_hidden: on_node && hidden.iter().any(|&(id, _)| id == state.tree.selection),
    })
}

/// 1 = longest. UX §3.3 "Wording, longest first".
fn notice_text(n: &Notice, form: u8, sep: &str) -> String {
    let c = n.count;
    if n.selected_hidden {
        return match form {
            1 => format!(
                "{} {sep} {c} hidden ({})",
                copy::NOTICE_SELECTED_LONG,
                format::size_iec_long(n.bytes)
            ),
            2 => format!("{} {sep} +{c} hidden", copy::NOTICE_SELECTED_SHORT),
            _ => copy::NOTICE_SELECTED_MIN.to_owned(),
        };
    }
    match form {
        1 => format!(
            "{c} items too small to show ({})",
            format::size_iec_long(n.bytes)
        ),
        2 => format!("{c} too small to show"),
        _ => format!("+{c} hidden"),
    }
}

/// `Map: path (updating) ───── notice ─`, fitted to `width` in the order of UX §3.3.
pub fn title_text(state: &AppState, width: usize) -> String {
    let g = &state.glyphs;
    let scope = state.map.scope().unwrap_or(state.tree.view_root);
    let path = {
        use std::os::unix::ffi::OsStrExt;
        sanitize::display_name(state.model.path(scope).as_os_str().as_bytes())
    };
    let status = match state.model.state() {
        ScanState::Starting | ScanState::Scanning => Some(copy::MAP_UPDATING),
        ScanState::Stopped => Some(copy::MAP_PARTIAL),
        ScanState::Done => None,
    };
    let star = if state.focus == Pane::Map {
        copy::FOCUS_MARK
    } else {
        ""
    };
    let notice = notice(state);

    // On the `..` row the title also says where ⏎ goes (UX §3.2.2).
    let up_path = state
        .tree
        .on_parent
        .then(|| state.model.parent(state.tree.view_root))
        .flatten()
        .map(|p| {
            use std::os::unix::ffi::OsStrExt;
            sanitize::display_name(state.model.path(p).as_os_str().as_bytes())
        });

    // (path limit, notice form, show status, up-path limit): each step keeps the earlier ones.
    // The `⏎ goes up to` path is cut first, before the notice is shortened.
    type Step = (Option<usize>, u8, bool, Option<usize>);
    let steps: [Step; 8] = [
        (None, 1, true, None),
        (None, 1, true, Some(PATH_FIT_FIRST)),
        (None, 1, true, Some(PATH_FIT_SECOND)),
        (Some(PATH_FIT_FIRST), 1, true, Some(PATH_FIT_SECOND)),
        (Some(PATH_FIT_FIRST), 2, true, Some(PATH_FIT_SECOND)),
        (Some(PATH_FIT_FIRST), 2, false, Some(PATH_FIT_SECOND)),
        (Some(PATH_FIT_SECOND), 2, false, Some(PATH_FIT_SECOND)),
        (Some(PATH_FIT_SECOND), 3, false, Some(PATH_FIT_SECOND)),
    ];
    let build = |step: Step| {
        let up = up_path.as_ref().map_or(String::new(), |p| {
            let p = step
                .3
                .map_or(p.clone(), |m| format::truncate_left(p, m, g.ellipsis));
            format!(" {} {} {p}", g.separator, copy::MAP_UP_HINT)
        });
        let shown = step.0.map_or(path.clone(), |m| {
            format::truncate_left(&path, m, g.ellipsis)
        });
        let status = status
            .filter(|_| step.2)
            .map_or(String::new(), |s| format!(" {s}"));
        let left = format!(" {star}{}{shown}{status}{up} ", copy::MAP_TITLE);
        let right = notice.as_ref().map_or(String::new(), |n| {
            format!(" {} {}", notice_text(n, step.1, g.separator), g.rule)
        });
        (left, right)
    };
    let fits = |(left, right): &(String, String)| {
        format::width(left) + format::width(right) + usize::from(!right.is_empty()) <= width
    };
    let (left, right) = steps
        .iter()
        .map(|&s| build(s))
        .find(fits)
        .unwrap_or_else(|| build(steps[steps.len() - 1]));
    let left = format::truncate(
        &left,
        width.saturating_sub(format::width(&right)),
        g.ellipsis,
    );
    let fill = width.saturating_sub(format::width(&left) + format::width(&right));
    format!("{left}{}{right}", g.rule.repeat(fill))
}

pub fn render_title(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut style = Style::new();
    if state.focus == Pane::Map {
        style = style.add_modifier(Modifier::BOLD);
    }
    frame.render_widget(
        Paragraph::new(title_text(state, usize::from(area.width))).style(style),
        area,
    );
}
