//! What the screen shows below 80×24 (UX §3, §8).

pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

/// Whether the terminal is smaller than the floor.
pub fn is_too_small(width: u16, height: u16) -> bool {
    width < MIN_WIDTH || height < MIN_HEIGHT
}

/// `Terminal too small: need 80×24, have 64×20. Resize, or press q to quit.`
pub fn message(width: u16, height: u16) -> String {
    format!(
        "Terminal too small: need {MIN_WIDTH}×{MIN_HEIGHT}, have {width}×{height}. Resize, or press q to quit."
    )
}
