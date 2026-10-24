//! Tree view: one line per visible row (UX §3.2).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use super::theme::Glyphs;
use super::{copy, format, sanitize};
use crate::app::map_state::Pane;
use crate::app::state::AppState;
use crate::app::tree_state::Row;
use crate::model::Flags;
use crate::types::{IssueKind, Kind, NodeId};

const BAR_CELLS: usize = 10;
/// The SIZE column: room for `≥999.9G`; ASCII spells the bound `>=`, one cell more (UX §7.1).
fn size_width(glyphs: &Glyphs) -> usize {
    if glyphs.ascii { 8 } else { 7 }
}

pub fn bar(fraction: f64, glyphs: &Glyphs) -> String {
    let eighths = (fraction.clamp(0.0, 1.0) * BAR_CELLS as f64 * 8.0).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let mut out = glyphs.bar_full.repeat(full);
    if full < BAR_CELLS {
        out.push_str(glyphs.bar_eighths[part]);
        out.push_str(&" ".repeat(BAR_CELLS - full - 1));
    }
    out
}

pub fn render_column_header(frame: &mut Frame, area: Rect, state: &AppState) {
    // The focused pane's title is bold with `*`.
    let mut text = if state.focus == Pane::Tree {
        format!("{}{}", copy::FOCUS_MARK, &copy::COLUMN_HEADER[1..])
    } else {
        copy::COLUMN_HEADER.to_owned()
    };
    // The wider ASCII SIZE column pushes every heading one cell right.
    if size_width(&state.glyphs) > 7 {
        text.insert(3, ' ');
    }
    frame.render_widget(Paragraph::new(text).style(state.theme.bold), area);
}

fn fraction(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

fn is_skipped(state: &AppState, id: NodeId) -> bool {
    state
        .model
        .issue(id)
        .is_some_and(|i| i.kind == IssueKind::SkippedByUser)
}

fn state_glyph(state: &AppState, id: NodeId) -> &'static str {
    let g = &state.glyphs;
    let flags = state.model.flags(id);
    if state.priority == Some(id) {
        g.prioritised
    } else if is_skipped(state, id) {
        g.skipped
    } else if !state.model.is_complete(id) || flags.contains(Flags::DELETING) {
        g.scanning
    } else if state.model.issue(id).is_some() {
        g.issue
    } else {
        " "
    }
}

fn suffix(state: &AppState, id: NodeId) -> String {
    if state.model.flags(id).contains(Flags::DELETING) {
        return format!(" ({})", super::ascii::text(&state.glyphs, "deleting…"));
    }
    let Some(issue) = state.model.issue(id) else {
        return String::new();
    };
    match issue.kind {
        IssueKind::SkippedByUser => " (scan skipped)".to_owned(),
        IssueKind::PermissionDenied => " (permission denied)".to_owned(),
        IssueKind::OtherFilesystem => " (other filesystem)".to_owned(),
        IssueKind::Vanished => " (vanished during scan)".to_owned(),
        IssueKind::Other(errno) => {
            format!(" (read error: {})", format::strerror(errno))
        }
    }
}

fn size_column(state: &AppState, bytes: u64, lower_bound: bool) -> String {
    let text = format::size_iec(bytes);
    let text = if lower_bound {
        format!("{}{text}", state.glyphs.lower_bound)
    } else {
        text
    };
    format!("{text:>width$}", width = size_width(&state.glyphs))
}

/// One screen line, split so the guides can be drawn dim.
struct RowText {
    prefix: String,
    guides: String,
    tail: String,
}

/// `│  ` for each ancestor with later siblings, then `├─▾ ` / `├── ` / `└── `.
fn guides(state: &AppState, depth: u16, last: bool, trail: u128, marker: &str) -> String {
    if depth == 0 {
        return String::new();
    }
    let g = &state.glyphs;
    let mut out = String::new();
    for level in 1..depth {
        if trail >> level.min(127) & 1 == 1 {
            out.push_str(g.guide_pipe);
            out.push_str("  ");
        } else {
            out.push_str("   ");
        }
    }
    out.push_str(if last { g.guide_elbow } else { g.guide_tee });
    out.push_str(g.rule);
    out.push_str(marker);
    out.push(' ');
    out
}

fn node_text(state: &AppState, row: &Row, cursor: bool) -> RowText {
    let Row::Node {
        id,
        depth,
        parent_size,
        last,
        trail,
    } = *row
    else {
        unreachable!("node_text takes node rows")
    };
    let g = &state.glyphs;
    let model = &state.model;
    let size = model.size(id, state.size_mode);
    let is_dir = model.kind(id) == Kind::Dir;
    let lower_bound =
        is_dir && (is_skipped(state, id) || (model.is_complete(id) && !model.is_exact(id)));
    let marker = if !is_dir {
        g.rule
    } else if state.tree.expanded.contains(&id) {
        g.expanded
    } else {
        g.collapsed
    };
    let name = if id == state.tree.view_root && depth == 0 && id == NodeId::ROOT {
        super::root_label(state)
    } else {
        sanitize::display_name(model.name(id))
    };
    let name = if is_dir && id != NodeId::ROOT {
        format!("{name}/")
    } else {
        name
    };
    RowText {
        prefix: format!(
            "{} {} {}  {}  {} ",
            if cursor { g.selection } else { " " },
            size_column(state, size, lower_bound),
            format::percent_tree(fraction(size, parent_size)),
            bar(fraction(size, parent_size), g),
            state_glyph(state, id),
        ),
        guides: guides(state, depth, last, trail, marker),
        tail: format!("{name}{}", suffix(state, id)),
    }
}

fn aggregate_text(state: &AppState, row: &Row, cursor: bool) -> RowText {
    let Row::Aggregate {
        dir,
        depth,
        count,
        bytes,
        trail,
    } = *row
    else {
        unreachable!("aggregate_text takes aggregate rows")
    };
    let parent = state.model.size(dir, state.size_mode);
    RowText {
        prefix: format!(
            "{} {} {}  {}    ",
            if cursor { state.glyphs.selection } else { " " },
            size_column(state, bytes, false),
            format::percent_tree(fraction(bytes, parent)),
            bar(fraction(bytes, parent), &state.glyphs),
        ),
        guides: guides(state, depth, true, trail, state.glyphs.rule),
        tail: format!(
            "({count} {}, {})",
            copy::AGGREGATE_WORD,
            format::size_iec(bytes)
        ),
    }
}

/// `..  (up to /parent/path)`, the path cut from the left to fit.
fn parent_text(state: &AppState, cursor: bool, width: usize) -> RowText {
    use std::os::unix::ffi::OsStrExt;
    let g = &state.glyphs;
    let prefix = format!(
        "{} {:>size$} {:>5}  {:BAR_CELLS$}    ",
        if cursor { g.selection } else { " " },
        "",
        "",
        "",
        size = size_width(g),
    );
    let head = format!("{}  ({} ", copy::PARENT_ROW_NAME, copy::PARENT_ROW_UP);
    let parent = state
        .model
        .parent(state.tree.view_root)
        .unwrap_or_else(|| state.model.root());
    let path = sanitize::display_name(state.model.path(parent).as_os_str().as_bytes());
    let room = width.saturating_sub(format::width(&prefix) + format::width(&head) + 1);
    let path = format::truncate_left(&path, room, g.ellipsis);
    RowText {
        prefix,
        guides: String::new(),
        tail: format!("{head}{path})"),
    }
}

fn row_line(state: &AppState, row: &Row, width: usize) -> Line<'static> {
    let cursor = state.tree.is_cursor(row);
    let text = match row {
        Row::Parent => parent_text(state, cursor, width),
        Row::Node { .. } => node_text(state, row, cursor),
        Row::Aggregate { .. } => aggregate_text(state, row, cursor),
    };
    let room = width.saturating_sub(format::width(&text.prefix) + format::width(&text.guides));
    let tail = format::truncate(&text.tail, room, state.glyphs.ellipsis);
    let used = format::width(&text.prefix) + format::width(&text.guides) + format::width(&tail);
    let theme = &state.theme;
    if !cursor {
        return Line::from(vec![
            Span::raw(text.prefix),
            Span::styled(text.guides, theme.dim),
            Span::raw(tail),
        ]);
    }
    // The selected row: no dim cell anywhere (UX §3.2.1).
    let pad = Span::raw(" ".repeat(width.saturating_sub(used)));
    if state.focus == Pane::Tree {
        Line::from(vec![
            Span::raw(text.prefix),
            Span::raw(text.guides),
            Span::raw(tail),
            pad,
        ])
        .style(theme.selected)
    } else {
        Line::from(vec![
            Span::raw(text.prefix),
            Span::raw(text.guides),
            Span::styled(tail, theme.underline),
            pad,
        ])
        .style(theme.selected_unfocused)
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let tree = &state.tree;
    let width = usize::from(area.width);
    let mut lines: Vec<Line> = tree
        .rows()
        .iter()
        .skip(tree.top())
        .take(usize::from(area.height))
        .map(|row| row_line(state, row, width))
        .collect();
    let root = tree.view_root;
    if tree.rows().len() == 1
        && state.model.kind(root) == Kind::Dir
        && state.model.children(root).is_empty()
        && state.model.is_complete(root)
    {
        lines.push(Line::raw(format!("    {}", copy::EMPTY_FOLDER)));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_is_ten_cells_in_eighth_steps() {
        let g = Glyphs::unicode();
        assert_eq!(bar(1.0, &g), "██████████");
        assert_eq!(bar(0.0, &g), " ".repeat(10));
        assert_eq!(bar(0.5, &g), "█████     ");
        assert_eq!(bar(0.55, &g), "█████▌    ");
        for f in [0.0, 0.01, 0.33, 0.999, 1.0] {
            assert_eq!(bar(f, &g).chars().count(), 10);
        }
    }
}
