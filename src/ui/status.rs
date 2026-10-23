//! Status line text (UX §5.2). ETA text arrives with T019.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use super::{copy, format};
use crate::app::state::AppState;
use crate::types::ScanState;

const ETA_TEXT_AFTER: Duration = Duration::from_secs(2);

pub fn text(state: &AppState) -> String {
    if let Some(t) = &state.transient {
        return t.text.clone();
    }
    let g = &state.glyphs;
    let sep = g.separator;
    let elapsed = format::elapsed(state.elapsed);
    match state.model.state() {
        scan @ (ScanState::Starting | ScanState::Scanning) => {
            let word = if scan == ScanState::Starting {
                copy::STARTING
            } else {
                copy::SCANNING
            };
            let mut line = format!("{} {word}  {elapsed} {}", g.scanning, copy::ELAPSED);
            // With no ETA source the line says nothing about ETA until 2 s have passed (ADR-010).
            if state.elapsed >= ETA_TEXT_AFTER {
                line.push_str(&format!(" {sep} {}", copy::ETA_UNKNOWN));
            }
            line
        }
        ScanState::Done => {
            let t = state.model.totals();
            let bytes = match state.size_mode {
                crate::types::SizeMode::Disk => t.disk_bytes,
                crate::types::SizeMode::Apparent => t.apparent_bytes,
            };
            let mut line = format!(
                "{} Done in {elapsed} {sep} {} files {sep} {}",
                g.done,
                format::thousands(t.files),
                format::size_iec_long(bytes)
            );
            let skipped = state.model.issues().len();
            if skipped > 0 {
                line.push_str(&format!(" {sep} {skipped} skipped (press 4)"));
            }
            line
        }
        ScanState::Stopped => {
            format!(
                "{} Stopped at {elapsed} {sep} {}",
                g.stopped,
                copy::STOPPED_TAIL
            )
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let line = format::truncate(
        &format::sanitize(&text(state)),
        area.width as usize,
        state.glyphs.ellipsis,
    );
    frame.render_widget(Paragraph::new(line), area);
}
