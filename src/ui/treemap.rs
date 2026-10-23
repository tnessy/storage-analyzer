//! The treemap strip: tiles, labels, markers, the map title rule and the hidden-items notice.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::theme::CategoryStyle;
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
fn label_words(state: &AppState, tile: &Tile, style: &CategoryStyle) -> Vec<String> {
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
            vec![
                name,
                format::size_iec(size),
                format::percent_tile(share, only),
                format!("[{}]", style.tag),
            ]
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

fn draw_tile(frame: &mut Frame, state: &AppState, tile: &Tile, selected: bool) {
    let r = tile.rect;
    let style = match tile.id {
        Some(id) => state.theme.category(state.model.category_of_node(id)),
        None => state.theme.aggregate(),
    };
    let paint = Style::new().bg(style.fill).fg(style.label);
    let marker = tile.id.and_then(|id| issue_marker(state, id));
    let words = label_words(state, tile, &style);
    let bordered = r.width >= 2 && r.height >= 2;
    let labelled = bordered && r.width >= MIN_LABEL_WIDTH;
    if bordered {
        let border = if selected {
            state.glyphs.map_selected_border
        } else {
            state.glyphs.map_border
        };
        let mut block = Block::bordered().border_type(border).style(paint);
        let reserve = if marker.is_some() { 2 } else { 0 };
        let room = usize::from(r.width.saturating_sub(2)).saturating_sub(reserve);
        let (first, second) = split_label(&words, room, state.glyphs.ellipsis);
        if labelled {
            block = block.title(first);
        }
        let inner = block.inner(r);
        frame.render_widget(block, r);
        if labelled && inner.height > 0 && !second.is_empty() {
            frame.render_widget(Paragraph::new(second).style(paint), inner);
        }
    } else {
        frame.render_widget(Block::new().style(paint), r);
    }
    if let Some(glyph) = marker {
        let at = if labelled {
            (r.right() - 2, r.y)
        } else {
            (r.x + r.width / 2, r.y + r.height / 2)
        };
        let cell = Rect::new(at.0, at.1, 1, 1);
        frame.render_widget(
            Paragraph::new(glyph).style(paint.add_modifier(Modifier::BOLD)),
            cell,
        );
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
            for (i, tile) in state.map.tiles().iter().enumerate() {
                draw_tile(frame, state, tile, selected == Some(i));
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
    let on_node = state.tree.aggregate.is_none() && !map.on_aggregate;
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

    // (path limit, notice form, show status): each step keeps the earlier ones.
    let steps: [(Option<usize>, u8, bool); 6] = [
        (None, 1, true),
        (Some(PATH_FIT_FIRST), 1, true),
        (Some(PATH_FIT_FIRST), 2, true),
        (Some(PATH_FIT_FIRST), 2, false),
        (Some(PATH_FIT_SECOND), 2, false),
        (Some(PATH_FIT_SECOND), 3, false),
    ];
    let build = |step: (Option<usize>, u8, bool)| {
        let shown = step.0.map_or(path.clone(), |m| {
            format::truncate_left(&path, m, g.ellipsis)
        });
        let status = status
            .filter(|_| step.2)
            .map_or(String::new(), |s| format!(" {s}"));
        let left = format!(" {star}{}{shown}{status} ", copy::MAP_TITLE);
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
