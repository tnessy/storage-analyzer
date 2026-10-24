//! The Largest view: the biggest files under the whole root (UX F3).

use ratatui::Frame;
use ratatui::layout::Rect;

use super::{copy, format, list_common, sanitize};
use crate::app::list_state::RowKey;
use crate::app::state::AppState;
use crate::types::ScanState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let files = state.lists.largest();
    if files.is_empty() {
        let scanning = matches!(state.phase(), ScanState::Starting | ScanState::Scanning);
        let message = if scanning {
            copy::NO_FILES_YET
        } else {
            copy::NO_FILES_FOUND
        };
        return list_common::render_message(frame, area, message);
    }
    let cursor = state.lists.cursor(state.view);
    let (top, selected) = cursor.map_or((0, None), |c| (c.top(), c.selected()));
    let width = usize::from(area.width);
    let lines = files
        .iter()
        .skip(top)
        .take(usize::from(area.height))
        .map(|&id| {
            use std::os::unix::ffi::OsStrExt;
            let size = format::size_iec(state.model.size(id, state.size_mode));
            let prefix = format!("{size:>7}  ");
            // The room left for the path after the selection mark and the size column.
            let room = width.saturating_sub(2 + format::width(&prefix));
            let path = sanitize::display_name(state.model.path(id).as_os_str().as_bytes());
            let path = format::truncate_left(&path, room, state.glyphs.ellipsis);
            list_common::line(
                state,
                &format!("{prefix}{path}"),
                selected == Some(&RowKey::Node(id)),
                width,
            )
        })
        .collect();
    list_common::render_lines(frame, area, lines);
}
