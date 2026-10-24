//! Rendering and user-facing copy (T015+).

pub mod ascii;
pub mod copy;
pub mod dialog;
pub mod format;
pub mod header;
pub mod help;
pub mod hints;
pub mod largest_view;
pub mod list_common;
pub mod sanitize;
pub mod skipped_view;
pub mod status;
pub mod tabs;
pub mod theme;
pub mod tile_colors;
pub mod too_small;
pub mod tree;
pub mod treemap;
pub mod types_view;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::APP_ID;
use crate::app::list_state::View;
use crate::app::state::{AppState, Dialog};

/// Rows outside the two panes: header, tabs, column header, map rule, status, hints.
const CHROME_ROWS: u16 = 6;
const MIN_MAP_ROWS: u16 = 6;

/// Screen regions (UX §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Regions {
    pub header: Rect,
    pub tabs: Rect,
    pub column_header: Rect,
    pub main: Rect,
    pub map_title: Rect,
    pub map: Rect,
    pub status: Rect,
    pub hints: Rect,
}

pub fn regions(area: Rect) -> Regions {
    let body = area.height.saturating_sub(CHROME_ROWS);
    // Main 60 %, map 40 % rounded in the map's favour, map at least 6 rows.
    let map_rows = (body * 2).div_ceil(5).max(MIN_MAP_ROWS).min(body);
    let main_rows = body - map_rows;
    let mut y = area.y;
    let mut row = |rows: u16| {
        let rect = Rect {
            x: area.x,
            y: y.min(area.bottom()),
            width: area.width,
            height: rows.min(area.bottom().saturating_sub(y)),
        };
        y = y.saturating_add(rows);
        rect
    };
    Regions {
        header: row(1),
        tabs: row(1),
        column_header: row(1),
        main: row(main_rows),
        map_title: row(1),
        map: row(map_rows),
        status: row(1),
        hints: row(1),
    }
}

/// The scan root as shown in the header, the root row and the map title.
pub fn root_label(state: &AppState) -> String {
    use std::os::unix::ffi::OsStrExt;
    sanitize::display_name(state.model.path(state.model.root()).as_os_str().as_bytes())
}

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    if too_small::is_too_small(area.width, area.height) {
        let text = too_small::message(area.width, area.height);
        let paragraph =
            ratatui::widgets::Paragraph::new(text).wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(paragraph, area);
    } else {
        render_screen(frame, state);
    }
    render_dialogs(frame, state);
    if state.glyphs.ascii {
        ascii::fold(frame.buffer_mut());
    }
}

fn render_screen(frame: &mut Frame, state: &AppState) {
    let r = regions(frame.area());
    header::render_header(frame, r.header, state);
    tabs::render_tabs(frame, r.tabs, state);
    tabs::render_column_header(frame, r.column_header, state);
    match state.view {
        View::Tree => tree::render(frame, r.main, state),
        View::Types => types_view::render(frame, r.main, state),
        View::Largest => largest_view::render(frame, r.main, state),
        View::Skipped => skipped_view::render(frame, r.main, state),
    }
    treemap::render_title(frame, r.map_title, state);
    treemap::render(frame, r.map, state);
    status::render(frame, r.status, state);
    hints::render_hints(frame, r.hints, state);
}

fn render_dialogs(frame: &mut Frame, state: &AppState) {
    match &state.dialog {
        Some(Dialog::Quit) => {
            let lines = [
                copy::QUIT_BODY.to_owned(),
                String::new(),
                copy::QUIT_ACTIONS.trim().to_owned(),
            ];
            dialog::render(
                frame,
                frame.area(),
                &state.glyphs,
                &format!("Quit {APP_ID}?"),
                &lines,
            );
        }
        Some(Dialog::ScanError(message)) => {
            let mut lines = vec![copy::SCAN_ERROR_BODY.to_owned()];
            lines.extend(message.lines().map(str::to_owned));
            lines.extend([
                String::new(),
                copy::SCAN_ERROR_TAIL.to_owned(),
                String::new(),
                copy::SCAN_ERROR_ACTIONS.to_owned(),
            ]);
            dialog::render(
                frame,
                frame.area(),
                &state.glyphs,
                copy::SCAN_ERROR_TITLE,
                &lines,
            );
        }
        Some(Dialog::Help) => help::render(frame, frame.area(), state),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_at_80x24_follow_the_spec() {
        let r = regions(Rect::new(0, 0, 80, 24));
        assert_eq!((r.header.y, r.tabs.y, r.column_header.y), (0, 1, 2));
        // body = 18 rows: map 40 % rounded up = 8, main 10.
        assert_eq!((r.main.height, r.map.height), (10, 8));
        assert_eq!(r.map_title.y, r.main.bottom());
        assert_eq!((r.status.y, r.hints.y), (22, 23));
    }

    #[test]
    fn map_keeps_six_rows_and_tiny_areas_do_not_panic() {
        assert_eq!(regions(Rect::new(0, 0, 80, 14)).map.height, 6);
        for h in 0..14 {
            let r = regions(Rect::new(0, 0, 10, h));
            assert!(r.hints.bottom() <= h);
        }
    }
}
