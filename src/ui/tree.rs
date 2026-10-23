//! Minimal tree view: the root row only (T016 takes this over).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use super::theme::Glyphs;
use super::{copy, format};
use crate::app::state::AppState;

const BAR_CELLS: usize = 10;

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
    frame.render_widget(
        Paragraph::new(copy::COLUMN_HEADER).style(state.theme.bold),
        area,
    );
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let g = &state.glyphs;
    let root = state.model.root();
    let size = state.model.size(root, state.size_mode);
    let pending = if state.model.is_complete(root) {
        " "
    } else {
        g.scanning
    };
    let name = super::root_label(state);
    let prefix = format!(
        "{} {:>7} {:>5}  {}  {} ",
        g.selection,
        format::size_iec(size),
        "100.0",
        bar(1.0, g),
        pending
    );
    let room = (area.width as usize).saturating_sub(format::width(&prefix));
    let line = format!("{prefix}{}", format::truncate(&name, room, g.ellipsis));
    let row = Rect {
        height: 1.min(area.height),
        ..area
    };
    frame.render_widget(Paragraph::new(line).style(state.theme.selected), row);
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
