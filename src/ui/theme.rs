//! Glyphs and styles. Every UI module takes its symbols from here (UX §7).

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use super::tile_colors::Hue;
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
    pub depth: ColorDepth,
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
            depth: ColorDepth::TrueColor,
        }
    }
}

/// How many colors the terminal gets (detection is T023's; until then the app uses truecolor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Indexed256,
    /// No fills at all (NO_COLOR or a monochrome terminal).
    Mono,
}

/// Fill and label colors of one palette hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    pub fill: Color,
    pub label: Color,
}

const BLACK: Color = Color::Rgb(0, 0, 0);
const WHITE: Color = Color::Rgb(255, 255, 255);

/// Truecolor RGB, xterm-256 index, and whether the label is white (UX §7).
type PaletteEntry = (Hue, (u8, u8, u8), u8, bool);

const PALETTE: [PaletteEntry; 8] = [
    (Hue::Yellow, (0xF0, 0xE4, 0x42), 221, false),
    (Hue::Blue, (0x00, 0x72, 0xB2), 25, true),
    (Hue::Orange, (0xE6, 0x9F, 0x00), 178, false),
    (Hue::BluishGreen, (0x00, 0x9E, 0x73), 35, false),
    (Hue::SkyBlue, (0x56, 0xB4, 0xE9), 74, false),
    (Hue::Vermillion, (0xD5, 0x5E, 0x00), 166, false),
    (Hue::ReddishPurple, (0xCC, 0x79, 0xA7), 175, false),
    (Hue::Grey, (0xBB, 0xBB, 0xBB), 250, false),
];

impl Theme {
    /// `None` when the theme draws no fills.
    pub fn swatch(&self, hue: Hue) -> Option<Swatch> {
        let &(_, (r, g, b), index, white) = PALETTE.iter().find(|(h, ..)| *h == hue)?;
        let fill = match self.depth {
            ColorDepth::TrueColor => Color::Rgb(r, g, b),
            ColorDepth::Indexed256 => Color::Indexed(index),
            ColorDepth::Mono => return None,
        };
        Some(Swatch {
            fill,
            label: if white { WHITE } else { BLACK },
        })
    }

    /// The fixed file-type hue of a category.
    pub fn category_hue(&self, category: Category) -> Hue {
        match category {
            Category::CodeText => Hue::Yellow,
            Category::Binary => Hue::Blue,
            Category::Video => Hue::Orange,
            Category::Archive => Hue::BluishGreen,
            Category::Image => Hue::SkyBlue,
            Category::Documents => Hue::Vermillion,
            Category::AudioOther => Hue::ReddishPurple,
        }
    }

    pub fn category_tag(&self, category: Category) -> char {
        match category {
            Category::Video => 'V',
            Category::Image => 'I',
            Category::Archive => 'A',
            Category::CodeText => 'C',
            Category::Binary => 'B',
            Category::Documents => 'D',
            Category::AudioOther => 'O',
        }
    }
}
