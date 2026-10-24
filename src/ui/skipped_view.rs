//! The Skipped view: every path the scan could not or did not read, newest first.

use std::os::unix::ffi::OsStrExt;

use ratatui::Frame;
use ratatui::layout::Rect;

use super::{copy, format, list_common, sanitize};
use crate::app::list_state::RowKey;
use crate::app::state::AppState;
use crate::types::IssueKind;

/// UX / design §3.4 "Skip reasons".
pub fn reason(kind: IssueKind) -> String {
    match kind {
        IssueKind::PermissionDenied => copy::REASON_DENIED.to_owned(),
        IssueKind::Vanished => copy::REASON_VANISHED.to_owned(),
        IssueKind::OtherFilesystem => copy::REASON_OTHER_FS.to_owned(),
        IssueKind::SkippedByUser => copy::REASON_SKIPPED.to_owned(),
        IssueKind::Other(errno) => {
            format!("{} {}", copy::REASON_READ_ERROR, format::strerror(errno))
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let issues = state.model.issues();
    let len = state.lists.skipped_len().min(issues.len());
    if len == 0 {
        return list_common::render_message(frame, area, copy::NOTHING_SKIPPED);
    }
    let cursor = state.lists.cursor(state.view);
    let (top, selected) = cursor.map_or((0, None), |c| (c.top(), c.selected()));
    let width = usize::from(area.width);
    let lines = (top..len)
        .take(usize::from(area.height))
        .map(|row| {
            let at = len - 1 - row;
            let issue = &issues[at];
            let tail = format!("  ({})", reason(issue.kind));
            let room = width.saturating_sub(2 + format::width(&tail));
            let path = sanitize::display_name(state.model.path(issue.node).as_os_str().as_bytes());
            let path = format::truncate_left(&path, room, state.glyphs.ellipsis);
            list_common::line(
                state,
                &format!("{path}{tail}"),
                selected == Some(&RowKey::Issue(issue.node)),
                width,
            )
        })
        .collect();
    list_common::render_lines(frame, area, lines);
}
