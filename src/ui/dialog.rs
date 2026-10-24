//! Shared dialog frame: centered, 64 wide or width−4, double border, title on the border.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::theme::Glyphs;
use super::{ascii, format, sanitize};

const MAX_WIDTH: u16 = 64;

/// Room for text: the dialog width minus the 2 border cells and a 2-cell margin on each side,
/// so text never touches a border (UX §5.3).
fn text_width(width: u16) -> usize {
    (width as usize).saturating_sub(6).max(1)
}

fn dialog_rect(screen: Rect, rows: usize, max_width: u16) -> Rect {
    let width = max_width.min(screen.width.saturating_sub(4)).max(1);
    let height = (rows as u16).saturating_add(2).min(screen.height);
    Rect {
        x: screen.x + (screen.width.saturating_sub(width)) / 2,
        y: screen.y + (screen.height.saturating_sub(height)) / 2,
        width: width.min(screen.width),
        height,
    }
}

/// Word-wraps `line` to `width` cells; a word longer than a row is cut.
fn wrap(line: &str, width: usize) -> Vec<String> {
    // A line that fits keeps its spacing (the key hints are aligned with spaces).
    if format::width(line) <= width {
        return vec![line.to_owned()];
    }
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in line.split(' ').filter(|w| !w.is_empty()) {
        let mut word = word.to_owned();
        while format::width(&word) > width {
            let cut = format::truncate(&word, width, "");
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
            }
            word = word[cut.len()..].to_owned();
            rows.push(cut);
        }
        if !row.is_empty() && format::width(&row) + 1 + format::width(&word) > width {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(&word);
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// Paragraphs wrap and keep the margin on every row. A line that starts with spaces is a list
/// item: it keeps its indent, never wraps and ends in an ellipsis if it is too long.
fn layout(lines: &[String], width: usize, ellipsis: &str) -> Vec<String> {
    lines
        .iter()
        .flat_map(|line| {
            if line.starts_with(' ') {
                vec![format::truncate(line, width, ellipsis)]
            } else {
                wrap(line, width)
            }
        })
        .collect()
}

pub fn render(frame: &mut Frame, screen: Rect, glyphs: &Glyphs, title: &str, lines: &[String]) {
    let lines: Vec<String> = lines
        .iter()
        .map(|l| sanitize::sanitize(&ascii::text(glyphs, l)))
        .collect();
    let width = MAX_WIDTH.min(screen.width.saturating_sub(4)).max(1);
    let rows = layout(&lines, text_width(width), glyphs.ellipsis);
    let rect = dialog_rect(screen, rows.len(), MAX_WIDTH);
    let body = rows
        .iter()
        .map(|l| format!("  {l}"))
        .collect::<Vec<_>>()
        .join("\n");
    let block = Block::bordered()
        .border_type(glyphs.dialog_border)
        .title(format!("═ {title} "));
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(body).block(block), rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_64_or_screen_minus_4_and_centered() {
        let r = dialog_rect(Rect::new(0, 0, 100, 30), 1, MAX_WIDTH);
        assert_eq!((r.width, r.x), (64, 18));
        let r = dialog_rect(Rect::new(0, 0, 50, 30), 1, MAX_WIDTH);
        assert_eq!((r.width, r.x), (46, 2));
    }

    #[test]
    fn paragraphs_wrap_at_word_breaks_and_list_items_truncate() {
        assert_eq!(wrap("aa bb cc", 5), vec!["aa bb", "cc"]);
        assert_eq!(wrap(&"y".repeat(12), 5), vec!["yyyyy", "yyyyy", "yy"]);
        let lines = ["  item item item".to_owned()];
        assert_eq!(layout(&lines, 8, "…"), vec!["  item …"]);
    }
}
