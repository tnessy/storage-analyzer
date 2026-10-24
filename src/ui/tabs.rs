//! The view tab row and the per-view column headers.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::{copy, format};
use crate::app::list_state::View;
use crate::app::state::AppState;
use crate::types::ScanState;

const VIEWS: [View; 4] = [View::Tree, View::Types, View::Largest, View::Skipped];

/// `[1 Tree]  2 Types  3 Largest  4 Skipped !N`: the active tab in brackets and bold.
pub fn render_tabs(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut spans = vec![Span::raw(" ")];
    for (tab, view) in copy::TABS.iter().zip(VIEWS) {
        if view == state.view {
            spans.push(Span::styled(format!("[{tab}]"), state.theme.bold));
        } else {
            spans.push(Span::raw((*tab).to_owned()));
        }
        if view == View::Skipped && !state.model.issues().is_empty() {
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

pub fn render_column_header(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = match state.view {
        View::Tree => return super::tree::render_column_header(frame, area, state),
        View::Types => copy::TYPES_COLUMNS,
        View::Largest => copy::LARGEST_COLUMNS,
        View::Skipped => copy::SKIPPED_COLUMNS,
    };
    let mut line = text.to_owned();
    if state.phase() == ScanState::Stopped {
        let room = usize::from(area.width).saturating_sub(format::width(&line));
        let tag = copy::PARTIAL;
        line.push_str(&format!("{tag:>room$}"));
    }
    frame.render_widget(Paragraph::new(line).style(state.theme.bold), area);
}
