//! Glyphs and styles. Every UI module takes its symbols from here (UX §7).

use ratatui::style::{Modifier, Style};
use ratatui::widgets::BorderType;

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
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub selected: Style,
    pub bold: Style,
    pub dim: Style,
}

impl Theme {
    pub fn standard() -> Self {
        Self {
            selected: Style::new().add_modifier(Modifier::REVERSED),
            bold: Style::new().add_modifier(Modifier::BOLD),
            dim: Style::new().add_modifier(Modifier::DIM),
        }
    }
}
