//! Squarified treemap layout and tile navigation (T014).

pub mod contract;

use ratatui::layout::Rect;

pub use contract::{Direction, Tile};

use crate::types::NodeId;

pub fn squarify(_items: &[(NodeId, u64)], _area: Rect, _cell_aspect: f32) -> Vec<Tile> {
    todo!("T014")
}

pub fn neighbor(_tiles: &[Tile], _from: usize, _dir: Direction) -> Option<usize> {
    todo!("T014")
}
