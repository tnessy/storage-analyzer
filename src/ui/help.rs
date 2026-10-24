//! The `?` overlay (UX §6): two columns, no scrolling, 80×24 at most.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{ascii, copy, format};
use crate::app::state::AppState;

/// Inner width of the overlay: two 37-cell columns and a 2-cell gap.
const COLUMN: usize = 37;
const GAP: usize = 2;
const KEY_COLUMN: usize = 10;
const OVERLAY_WIDTH: u16 = 80;

fn entry(keys: &str, what: &str) -> String {
    format!("{keys:<KEY_COLUMN$}{what}")
}

/// Greedy word wrap at `width` cells.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && format::width(&line) + 1 + format::width(word) > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The body lines, top to bottom (the key column uses ASCII names in the fallback).
pub fn lines(ascii_keys: bool) -> Vec<String> {
    let pick = |&(unicode, plain, what): &(&str, &str, &str)| {
        entry(if ascii_keys { plain } else { unicode }, what)
    };
    let left: Vec<String> = copy::HELP_LEFT.iter().map(pick).collect();
    let right: Vec<String> = copy::HELP_RIGHT.iter().map(pick).collect();
    let mut out = vec![format!(
        "{:<COLUMN$}{}{}",
        copy::HELP_LEFT_TITLE,
        " ".repeat(GAP),
        copy::HELP_RIGHT_TITLE
    )];
    for i in 0..left.len().max(right.len()) {
        let l = left.get(i).map_or("", String::as_str);
        let r = right.get(i).map_or("", String::as_str);
        out.push(format!("{l:<COLUMN$}{}{r}", " ".repeat(GAP)));
    }
    out.push(String::new());
    let footer_width = COLUMN * 2 + GAP;
    out.extend(wrap(copy::HELP_MAP_NOTE, footer_width));
    out.extend(wrap(copy::HELP_ETA_FOOTER, footer_width));
    out.extend(wrap(copy::HELP_HARD_LINK_NOTE, footer_width));
    out.extend(wrap(copy::HELP_DISK_NOTE, footer_width));
    out
}

pub fn render(frame: &mut Frame, screen: Rect, state: &AppState) {
    let g = &state.glyphs;
    let body = lines(g.ascii);
    let width = OVERLAY_WIDTH.min(screen.width);
    let height = (body.len() as u16 + 2).min(screen.height);
    let rect = Rect {
        x: screen.x + screen.width.saturating_sub(width) / 2,
        y: screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    };
    let block = Block::bordered()
        .border_type(g.dialog_border)
        .title(format!("═ {} ", copy::HELP_TITLE))
        .title_bottom(
            Line::from(format!(" {} ═", ascii::text(g, copy::HELP_CLOSE)))
                .alignment(Alignment::Right),
        );
    let inner_lines: Vec<Line> = body
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let text = format!(" {l}");
            if i == 0 {
                Line::styled(text, state.theme.bold)
            } else {
                Line::raw(text)
            }
        })
        .collect();
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(inner_lines).block(block), rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nineteen_entries_and_four_notes_make_a_22_row_overlay() {
        assert_eq!(copy::HELP_LEFT.len() + copy::HELP_RIGHT.len(), 19);
        assert!(copy::HELP_RIGHT.iter().any(|e| e.0 == "v"));
        assert_eq!(lines(false).len() + 2, 22, "{}", lines(false).len());
        assert_eq!(lines(true).len() + 2, 22);
    }
}
