//! The key-hint row (UX §6): hints by context, dropped from the right, `? help` always kept.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use super::{copy, format};
use crate::app::list_state::View;
use crate::app::map_state::Pane;
use crate::app::state::AppState;

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

pub fn render_hints(frame: &mut Frame, area: Rect, state: &AppState) {
    let ascii = state.glyphs.ascii;
    let hints: &[&str] = match (state.focus, state.view, ascii) {
        (Pane::Map, _, false) => &copy::MAP_HINTS,
        (Pane::Map, _, true) => &copy::MAP_HINTS_ASCII,
        (Pane::Tree, View::Tree, false) => &copy::TREE_HINTS,
        (Pane::Tree, View::Tree, true) => &copy::TREE_HINTS_ASCII,
        (Pane::Tree, _, false) => &copy::LIST_HINTS,
        (Pane::Tree, _, true) => &copy::LIST_HINTS_ASCII,
    };
    let line = fit_hints(hints, area.width as usize);
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
