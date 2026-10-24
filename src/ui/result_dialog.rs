//! What a delete leaves behind: the error and abort dialogs, and the failed-paths list.

use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{ascii, copy, dialog, format, sanitize};
use crate::app::delete_flow::{DeleteAborted, DeleteDone, group_failures};
use crate::app::state::AppState;
use crate::delete::{AbortReason, DeleteFailKind, DeleteFailure};

fn label(kind: DeleteFailKind) -> String {
    match kind {
        DeleteFailKind::PermissionDenied => copy::FAIL_PERMISSION.to_owned(),
        DeleteFailKind::Busy => copy::FAIL_BUSY.to_owned(),
        DeleteFailKind::ReadOnlyFs => copy::FAIL_READ_ONLY.to_owned(),
        DeleteFailKind::OtherFilesystem => copy::FAIL_OTHER_FS.to_owned(),
        DeleteFailKind::Other(errno) => copy::fail_other(errno, &format::strerror(errno)),
    }
}

fn path_text(path: &std::path::Path) -> String {
    sanitize::sanitize(&path.display().to_string())
}

pub fn render_done(frame: &mut Frame, state: &AppState, done: &DeleteDone) {
    let mut lines = vec![
        path_text(&done.path),
        String::new(),
        copy::deleted_counts(
            done.removed.files,
            done.removed.dirs,
            &format::size_iec_long(done.removed.bytes),
        ),
        String::new(),
    ];
    for &(kind, n) in &done.groups {
        lines.push(format!(
            "{:>7}  {}",
            format::thousands(n as u64),
            label(kind)
        ));
    }
    if let Some(note) = &done.panic_note {
        lines.push(String::new());
        lines.extend(note.lines().map(str::to_owned));
    }
    lines.extend([String::new(), copy::RESULT_PARTIAL_ACTIONS.to_owned()]);
    dialog::render(
        frame,
        frame.area(),
        &state.glyphs,
        copy::RESULT_PARTIAL_TITLE,
        &lines,
    );
}

fn reason_text(reason: AbortReason) -> String {
    match reason {
        AbortReason::ChangedSinceScan => copy::ABORT_CHANGED.to_owned(),
        AbortReason::NotFound => copy::ABORT_NOT_FOUND.to_owned(),
        AbortReason::MountPoint => copy::ABORT_MOUNT_POINT.to_owned(),
        AbortReason::OpenFailed(errno) => copy::abort_open_failed(&format::strerror(errno), errno),
    }
}

pub fn render_aborted(frame: &mut Frame, state: &AppState, aborted: &DeleteAborted) {
    let lines = [
        path_text(&aborted.path),
        String::new(),
        reason_text(aborted.reason),
        copy::RESULT_ABORTED_TAIL.to_owned(),
        String::new(),
        copy::RESULT_ABORTED_ACTIONS.to_owned(),
    ];
    dialog::render(
        frame,
        frame.area(),
        &state.glyphs,
        copy::RESULT_ABORTED_TITLE,
        &lines,
    );
}

// ---- the failed-paths list ----

/// One line of the list: a group header, a path, or a gap between groups.
enum Entry<'a> {
    Header(DeleteFailKind, usize),
    Path(&'a DeleteFailure),
    Gap,
}

fn entries(failures: &[DeleteFailure]) -> Vec<Entry<'_>> {
    let mut out = Vec::new();
    for (kind, n) in group_failures(failures) {
        if !out.is_empty() {
            out.push(Entry::Gap);
        }
        out.push(Entry::Header(kind, n));
        let mut paths: Vec<&DeleteFailure> = failures.iter().filter(|f| f.kind == kind).collect();
        paths.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        out.extend(paths.into_iter().map(Entry::Path));
    }
    out
}

/// Lines the list shows at once.
pub fn page_rows(screen_height: u16) -> usize {
    usize::from(screen_height.saturating_sub(2)).max(1)
}

/// Lines in the whole list.
pub fn line_count(failures: &[DeleteFailure]) -> usize {
    entries(failures).len()
}

pub fn render_failed(frame: &mut Frame, state: &AppState, scroll: usize) {
    let area = frame.area();
    let g = &state.glyphs;
    let failures = state.failures();
    let all = entries(failures);
    let page = page_rows(area.height);
    let first = scroll.min(all.len().saturating_sub(page));
    let width = usize::from(area.width).saturating_sub(4);
    let lines: Vec<Line> = all
        .iter()
        .skip(first)
        .take(page)
        .map(|entry| match entry {
            Entry::Header(kind, n) => {
                Line::styled(format!(" {} ({n})", label(*kind)), state.theme.bold)
            }
            Entry::Path(f) => Line::raw(format!(
                "   {}",
                format::truncate_left(&path_text(&f.rel_path), width.saturating_sub(2), g.ellipsis)
            )),
            Entry::Gap => Line::raw(""),
        })
        .collect();
    let last = first + page >= all.len();
    let end = (first + page).min(all.len());
    let cue = format!("{} {end} of {}", if last { "↑" } else { "↓" }, all.len());
    let keys = copy::FAILED_KEYS;
    let target = path_text(&state.last_delete_path);
    let block = Block::bordered()
        .border_type(g.dialog_border)
        .title(ascii::text(
            g,
            &format!("═ {} ", copy::failed_title(failures.len(), &target)),
        ))
        .title_bottom(
            Line::from(ascii::text(g, &format!(" {cue} ═ {keys} "))).alignment(Alignment::Right),
        );
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
