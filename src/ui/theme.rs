//! Glyphs and styles. Every UI module takes its symbols from here (UX §7).

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use super::tile_colors::Hue;
use crate::model::Category;

#[derive(Clone, Debug)]
pub struct Glyphs {
    /// The ASCII fallback is active (non-UTF-8 locale): the renderer also folds leftover symbols.
    pub ascii: bool,
    pub scanning: &'static str,
    /// Frames of the activity spinner shown in place of `scanning` (unless animation is off).
    pub spinner: [&'static str; 4],
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
    /// The unfilled part of a progress bar.
    pub bar_empty: &'static str,
    /// Index n is a bar cell filled n eighths; index 0 is empty.
    pub bar_eighths: [&'static str; 8],
    pub selection: &'static str,
    pub dialog_border: BorderType,
    pub map_border: BorderType,
    pub map_selected_border: BorderType,
}

impl Glyphs {
    /// UX §7 ASCII fallback. Box-drawing and arrow characters in copy text are folded by
    /// `ascii::fold` after drawing.
    pub fn ascii() -> Self {
        Self {
            ascii: true,
            scanning: "~",
            spinner: ["|", "/", "-", "\\"],
            prioritised: "^",
            skipped: "-",
            lower_bound: ">=",
            expanded: "-",
            collapsed: "+",
            done: "",
            stopped: "",
            ellipsis: "...",
            separator: "-",
            breadcrumb: ">",
            rule: "-",
            guide_tee: "+",
            guide_elbow: "+",
            guide_pipe: "|",
            bar_full: "#",
            bar_empty: ".",
            bar_eighths: [" ", ".", ".", ".", "#", "#", "#", "#"],
            ..Self::unicode()
        }
    }

    /// Unicode unless the locale says otherwise: the first non-empty of `LC_ALL`, `LC_CTYPE` and
    /// `LANG` must name UTF-8.
    pub fn detect(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let utf8 = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|name| get(name).filter(|v| !v.is_empty()))
            .is_some_and(|v| {
                let v = v.to_ascii_lowercase();
                v.contains("utf-8") || v.contains("utf8")
            });
        if utf8 { Self::unicode() } else { Self::ascii() }
    }

    pub fn unicode() -> Self {
        Self {
            ascii: false,
            scanning: "◌",
            spinner: ["◜", "◝", "◞", "◟"],
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
            bar_empty: "░",
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

impl ColorDepth {
    /// `NO_COLOR` (non-empty) wins; then `COLORTERM`; then `TERM`.
    pub fn detect(get: &dyn Fn(&str) -> Option<String>) -> Self {
        if get("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return ColorDepth::Mono;
        }
        let colorterm = get("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if colorterm == "truecolor" || colorterm == "24bit" {
            return ColorDepth::TrueColor;
        }
        match get("TERM").as_deref() {
            None | Some("") | Some("dumb") => ColorDepth::Mono,
            Some(_) => ColorDepth::Indexed256,
        }
    }
}

/// Fill and label colors of one palette hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    pub fill: Color,
    pub label: Color,
}

const BLACK: Color = Color::Rgb(0, 0, 0);
const WHITE: Color = Color::Rgb(255, 255, 255);
/// Entries 16 and 231 of the xterm-256 cube are pure black and pure white.
const INDEXED_BLACK: Color = Color::Indexed(16);
const INDEXED_WHITE: Color = Color::Indexed(231);

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
        // Labels are plain black or white in every mode; with 256 colours they are indexed too,
        // so no 24-bit colour is ever sent to a terminal that may not understand it.
        let (fill, black, white_label) = match self.depth {
            ColorDepth::TrueColor => (Color::Rgb(r, g, b), BLACK, WHITE),
            ColorDepth::Indexed256 => (Color::Indexed(index), INDEXED_BLACK, INDEXED_WHITE),
            ColorDepth::Mono => return None,
        };
        Some(Swatch {
            fill,
            label: if white { white_label } else { black },
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
