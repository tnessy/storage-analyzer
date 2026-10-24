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
