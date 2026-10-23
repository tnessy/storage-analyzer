//! Shared dialog frame: centered, 64 wide or width−4, double border, title on the border.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::format;
use super::theme::Glyphs;

const MAX_WIDTH: u16 = 64;

fn dialog_rect(screen: Rect, lines: &[String]) -> Rect {
    let width = MAX_WIDTH.min(screen.width.saturating_sub(4)).max(1);
    let inner = (width as usize).saturating_sub(4).max(1);
    let rows: usize = lines
        .iter()
        .map(|l| format::width(l).div_ceil(inner).max(1))
        .sum();
    let height = (rows as u16).saturating_add(2).min(screen.height);
    Rect {
        x: screen.x + (screen.width.saturating_sub(width)) / 2,
        y: screen.y + (screen.height.saturating_sub(height)) / 2,
        width: width.min(screen.width),
        height,
    }
}

pub fn render(frame: &mut Frame, screen: Rect, glyphs: &Glyphs, title: &str, lines: &[String]) {
    let rect = dialog_rect(screen, lines);
    let body = lines
        .iter()
        .map(|l| format!("  {}", format::sanitize(l)))
        .collect::<Vec<_>>()
        .join("\n");
    let block = Block::bordered()
        .border_type(glyphs.dialog_border)
        .title(format!("═ {title} "));
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(body).block(block).wrap(Wrap { trim: false }),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_is_64_or_screen_minus_4_and_centered() {
        let r = dialog_rect(Rect::new(0, 0, 100, 30), &["x".into()]);
        assert_eq!((r.width, r.x), (64, 18));
        let r = dialog_rect(Rect::new(0, 0, 50, 30), &["x".into()]);
        assert_eq!((r.width, r.x), (46, 2));
    }

    #[test]
    fn long_lines_wrap_instead_of_truncating() {
        let r = dialog_rect(Rect::new(0, 0, 80, 40), &["y".repeat(120)]);
        assert_eq!(r.height, 2 + 2);
    }
}
