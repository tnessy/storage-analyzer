//! The Types view: file categories for the whole root (UX F3).

use ratatui::Frame;
use ratatui::layout::Rect;

use super::{copy, format, list_common};
use crate::app::list_state::RowKey;
use crate::app::state::AppState;
use crate::model::{Category, ExtKey};
use crate::types::ScanState;

fn category_name(category: Category) -> &'static str {
    match category {
        Category::Video => copy::CATEGORY_VIDEO,
        Category::Image => copy::CATEGORY_IMAGE,
        Category::Archive => copy::CATEGORY_ARCHIVE,
        Category::CodeText => copy::CATEGORY_CODE,
        Category::Binary => copy::CATEGORY_BINARY,
        Category::Documents => copy::CATEGORY_DOCUMENTS,
        Category::AudioOther => copy::CATEGORY_AUDIO,
    }
}

fn ext_name(key: &ExtKey) -> String {
    match key {
        ExtKey::Ext(bytes) => format!(".{}", super::sanitize::display_name(bytes)),
        ExtKey::NoExt => copy::EXT_NONE.to_owned(),
        ExtKey::Other => copy::EXT_OTHER.to_owned(),
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let rows = state.lists.types_rows();
    if rows.is_empty() {
        let done = state.model.state() != ScanState::Scanning
            && state.model.state() != ScanState::Starting;
        let message = if done {
            copy::NO_FILES_FOUND
        } else {
            copy::NO_FILES_YET
        };
        return list_common::render_message(frame, area, message);
    }
    let cursor = state.lists.cursor(state.view);
    let (top, selected) = cursor.map_or((0, None), |c| (c.top(), c.selected()));
    let root_total = state.model.size(state.model.root(), state.size_mode);
    let width = usize::from(area.width);
    let lines = rows
        .iter()
        .skip(top)
        .take(usize::from(area.height))
        .map(|row| {
            let label = match &row.key {
                RowKey::Category(c) => {
                    let twisty = if state.lists.types_expanded.contains(c) {
                        state.glyphs.expanded
                    } else {
                        state.glyphs.collapsed
                    };
                    format!("{twisty} {}", category_name(*c))
                }
                RowKey::Ext(_, key) => format!("    {}", ext_name(key)),
                _ => String::new(),
            };
            let share = if root_total == 0 {
                0.0
            } else {
                row.bytes as f64 / root_total as f64
            };
            let text = format!(
                "{:>7} {:>9} {}  {label}",
                format::size_iec(row.bytes),
                format::thousands(row.files),
                format::percent_tree(share),
            );
            list_common::line(state, &text, selected == Some(&row.key), width)
        })
        .collect();
    list_common::render_lines(frame, area, lines);
}
