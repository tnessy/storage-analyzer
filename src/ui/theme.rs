//! Glyphs and styles. Every UI module takes its symbols from here (UX §7).

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use crate::model::Category;

#[derive(Clone, Debug)]
pub struct Glyphs {
    pub scanning: &'static str,
    pub prioritised: &'static str,
    pub skipped: &'static str,
    pub issue: &'static str,
    pub lower_bound: &'static str,
    pub expanded: &'static str,
    pub collapsed: &'static str,
    pub done: &'static str,
    pub stopped: &'static str,
    pub ellipsis: &'static str,
    pub separator: &'static str,
    pub breadcrumb: &'static str,
    pub rule: &'static str,
    /// Tree guides: branch, last branch and the continuation bar.
    pub guide_tee: &'static str,
    pub guide_elbow: &'static str,
    pub guide_pipe: &'static str,
    pub bar_full: &'static str,
    /// Index n is a bar cell filled n eighths; index 0 is empty.
    pub bar_eighths: [&'static str; 8],
    pub selection: &'static str,
    pub dialog_border: BorderType,
    pub map_border: BorderType,
    pub map_selected_border: BorderType,
}

impl Glyphs {
    pub fn unicode() -> Self {
        Self {
            scanning: "◌",
            prioritised: "▲",
            skipped: "⊘",
            issue: "!",
            lower_bound: "≥",
            expanded: "▾",
            collapsed: "▸",
            done: "✓",
            stopped: "■",
            ellipsis: "…",
            separator: "·",
            breadcrumb: "›",
            rule: "─",
            guide_tee: "├",
            guide_elbow: "└",
            guide_pipe: "│",
            bar_full: "█",
            bar_eighths: [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉"],
            selection: ">",
            dialog_border: BorderType::Double,
            map_border: BorderType::Plain,
            map_selected_border: BorderType::Double,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    /// The selected row of the focused pane: reverse video and bold, never dim.
    pub selected: Style,
    /// The selected row when the other pane has focus: bold only.
    pub selected_unfocused: Style,
    pub underline: Style,
    /// The frame of the selected map tile.
    pub tile_frame_selected: Style,
    pub bold: Style,
    pub dim: Style,
}

impl Theme {
    pub fn standard() -> Self {
        Self {
            selected: Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            selected_unfocused: Style::new().add_modifier(Modifier::BOLD),
            underline: Style::new().add_modifier(Modifier::UNDERLINED),
            tile_frame_selected: Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
            bold: Style::new().add_modifier(Modifier::BOLD),
            dim: Style::new().add_modifier(Modifier::DIM),
        }
    }
}

/// Treemap tile colors and letter tag for one category (UX §7, Okabe-Ito).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CategoryStyle {
    pub fill: Color,
    pub label: Color,
    /// The alternate shade for a neighbour of the same category, with its label color.
    pub alt_fill: Color,
    pub alt_label: Color,
    pub tag: char,
}

const BLACK: Color = Color::Rgb(0, 0, 0);
const WHITE: Color = Color::Rgb(255, 255, 255);

impl Theme {
    pub fn category(&self, category: Category) -> CategoryStyle {
        let (base, label, alt, alt_label, tag) = match category {
            Category::Video => ((0xE6, 0x9F, 0x00), BLACK, (0xF0, 0xC5, 0x66), BLACK, 'V'),
            Category::Image => ((0x56, 0xB4, 0xE9), BLACK, (0x9A, 0xD2, 0xF2), BLACK, 'I'),
            Category::Archive => ((0x00, 0x9E, 0x73), BLACK, (0x66, 0xC5, 0xAB), BLACK, 'A'),
            Category::CodeText => ((0xF0, 0xE4, 0x42), BLACK, (0xBB, 0xB2, 0x33), BLACK, 'C'),
            Category::Binary => ((0x00, 0x72, 0xB2), WHITE, (0x00, 0x4A, 0x74), WHITE, 'B'),
            Category::Documents => ((0xD5, 0x5E, 0x00), BLACK, (0x8A, 0x3D, 0x00), WHITE, 'D'),
            Category::AudioOther => ((0xCC, 0x79, 0xA7), BLACK, (0xE0, 0xAF, 0xCA), BLACK, 'O'),
        };
        CategoryStyle {
            fill: rgb(base),
            label,
            alt_fill: rgb(alt),
            alt_label,
            tag,
        }
    }

    /// The `(+N smaller)` tile: grey, no category.
    pub fn aggregate(&self) -> CategoryStyle {
        CategoryStyle {
            fill: rgb((0xBB, 0xBB, 0xBB)),
            label: BLACK,
            alt_fill: rgb((0x96, 0x96, 0x96)),
            alt_label: BLACK,
            tag: ' ',
        }
    }
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}
