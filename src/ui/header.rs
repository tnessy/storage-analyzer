//! Header, tab row and key hints.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{copy, format, sanitize};
use crate::APP_ID;
use crate::app::state::AppState;
use crate::types::SizeMode;

pub fn render_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let mode = match state.size_mode {
        SizeMode::Disk => copy::SIZE_MODE_DISK,
        SizeMode::Apparent => copy::SIZE_MODE_APPARENT,
    };
    let right = format!("{mode}   {}", copy::HEADER_HINT);
    let width = area.width as usize;
    let left_room = width.saturating_sub(format::width(&right) + 2);
    let path = breadcrumb(state);
    let left = format::truncate(
        &format!(" {APP_ID}  {path}"),
        left_room,
        state.glyphs.ellipsis,
    );
    let pad = width.saturating_sub(format::width(&left) + format::width(&right));
    let line = Line::from(vec![
        Span::styled(left, state.theme.bold),
        Span::raw(" ".repeat(pad)),
        Span::raw(right),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// The scan root, then each folder down to the zoomed view root, joined by `›`.
fn breadcrumb(state: &AppState) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut parts = vec![super::root_label(state)];
    let rel = state.model.rel_path(state.tree.view_root);
    parts.extend(
        rel.components()
            .map(|c| sanitize::display_name(c.as_os_str().as_bytes())),
    );
    parts.join(&format!(" {} ", state.glyphs.breadcrumb))
}

pub fn render_tabs(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut spans = vec![Span::raw(" ")];
    for (i, tab) in copy::TABS.iter().enumerate() {
        if i == 0 {
            spans.push(Span::styled(format!("[{tab}]"), state.theme.bold));
        } else {
            spans.push(Span::raw((*tab).to_owned()));
        }
        if i == 3 && !state.model.issues().is_empty() {
            spans.push(Span::raw(format!(
                " {}{}",
                state.glyphs.issue,
                state.model.issues().len()
            )));
        }
        spans.push(Span::raw("  "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Hints are dropped from the right, but the last one (`? help`) always stays.
pub fn fit_hints(hints: &[&str], width: usize) -> String {
    let (last, rest) = hints.split_last().expect("hints are never empty");
    let join = |n: usize| {
        let mut parts: Vec<&str> = rest[..n].to_vec();
        parts.push(last);
        format!(" {}", parts.join("  "))
    };
    (0..=rest.len())
        .rev()
        .map(join)
        .find(|s| format::width(s) <= width)
        .unwrap_or_else(|| join(0))
}

pub fn render_hints(frame: &mut Frame, area: Rect) {
    let line = fit_hints(&copy::TREE_HINTS, area.width as usize);
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_drop_from_the_right_and_keep_help() {
        let full = fit_hints(&copy::TREE_HINTS, 200);
        assert!(full.starts_with(" ↑↓ move") && full.ends_with("? help"));
        let narrow = fit_hints(&copy::TREE_HINTS, 30);
        assert!(format::width(&narrow) <= 30 && narrow.ends_with("? help"));
        assert!(narrow.contains("↑↓ move"));
        assert!(!narrow.contains("x skip"));
        assert!(fit_hints(&copy::TREE_HINTS, 3).ends_with("? help"));
    }
}
