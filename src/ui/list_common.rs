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

/// Like `line`, with a two-cell colour swatch starting at character `at` of `text` (which holds two
/// placeholder spaces there). The swatch keeps its color on a reversed cursor row.
pub fn line_with_swatch(
    state: &AppState,
    text: &str,
    cursor: bool,
    width: usize,
    at: usize,
    swatch: Option<ratatui::style::Color>,
) -> Line<'static> {
    let base = line(state, text, cursor, width);
    let Some(color) = swatch else {
        return base;
    };
    let plain: String = base.spans.iter().map(|s| s.content.as_ref()).collect();
    let chars: Vec<char> = plain.chars().collect();
    if chars.len() < at + 2 {
        return base;
    }
    let mark = ratatui::style::Style::new()
        .bg(color)
        .remove_modifier(ratatui::style::Modifier::REVERSED);
    let spans = vec![
        ratatui::text::Span::raw(chars[..at].iter().collect::<String>()),
        ratatui::text::Span::styled("  ", mark),
        ratatui::text::Span::raw(chars[at + 2..].iter().collect::<String>()),
    ];
    Line::from(spans).style(base.style)
}
