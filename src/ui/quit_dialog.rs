//! The quit confirmations: the plain one, and the one shown while a delete runs (UX §5.3).

use ratatui::Frame;

use super::{copy, dialog, sanitize};
use crate::APP_ID;
use crate::app::state::AppState;

pub fn render(frame: &mut Frame, state: &AppState) {
    let (title, lines) = match &state.delete_run {
        Some(run) if state.dialog == Some(crate::app::state::Dialog::QuitWhileDeleting) => (
            copy::QUIT_DELETING_TITLE.to_owned(),
            vec![
                copy::QUIT_DELETING_BODY.to_owned(),
                sanitize::sanitize(&run.path.display().to_string()),
                copy::quit_deleting_progress(run.removed_files, run.total_files),
                String::new(),
                copy::QUIT_DELETING_ACTIONS.to_owned(),
            ],
        ),
        _ => (
            format!("Quit {APP_ID}?"),
            vec![
                copy::QUIT_BODY.to_owned(),
                String::new(),
                copy::QUIT_ACTIONS.trim().to_owned(),
            ],
        ),
    };
    dialog::render(frame, frame.area(), &state.glyphs, &title, &lines);
}
