//! Squarified treemap layout and tile navigation (T014).

pub mod contract;
mod neighbor;
mod squarify;

pub use contract::{Direction, Tile};
pub use neighbor::neighbor;
pub use squarify::squarify;
