//! Pieces shared by the three list views.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::Paragraph;

use super::format;
use crate::app::map_state::Pane;
use crate::app::state::AppState;

/// One list line: the selection mark, then `text`, cut to `width` and reversed on the cursor row.
pub fn line(state: &AppState, text: &str, cursor: bool, width: usize) -> Line<'static> {
    let mark = if cursor { state.glyphs.selection } else { " " };
    let text = format::truncate(&format!("{mark} {text}"), width, state.glyphs.ellipsis);
    match (cursor, state.focus) {
        (false, _) => Line::raw(text),
        // The focused pane's selection is reverse and bold; with the map focused it is bold only.
        (true, Pane::Tree) => Line::styled(format!("{text:<width$}"), state.theme.selected),
        (true, Pane::Map) => Line::styled(text, state.theme.selected_unfocused),
    }
}

pub fn render_lines(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

pub fn render_message(frame: &mut Frame, area: Rect, message: &str) {
    frame.render_widget(Paragraph::new(format!("  {message}")), area);
}
