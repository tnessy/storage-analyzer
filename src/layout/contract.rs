//! Treemap tile types (design §3.7).

use ratatui::layout::Rect;

use crate::types::NodeId;

/// `id` is `None` for the `(+N smaller)` aggregate tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub id: Option<NodeId>,
    pub rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}
