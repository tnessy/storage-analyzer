//! User-facing text (UX spec §5). Glyphs come from `theme::Glyphs`.

pub const TABS: [&str; 4] = ["1 Tree", "2 Types", "3 Largest", "4 Skipped"];
pub const COLUMN_HEADER: &str = "   SIZE      %  OF PARENT      NAME";

pub const SIZE_MODE_DISK: &str = "Disk usage";
pub const SIZE_MODE_APPARENT: &str = "Apparent size";
pub const HEADER_HINT: &str = "? help   q quit";

pub const MAP_TITLE: &str = "Map: ";
pub const MAP_UPDATING: &str = "(updating)";
pub const MAP_PARTIAL: &str = "(partial)";
pub const MAP_WAITING: &str = "Waiting for first results…";

pub const STARTING: &str = "Starting…";
pub const SCANNING: &str = "Scanning";
pub const ELAPSED: &str = "elapsed";
pub const ETA_UNKNOWN: &str = "ETA unknown (no previous scan)";
pub const STOPPED_TAIL: &str = "partial results · r on the root rescans";

/// Dropped from the right when they don't fit; the last one always stays.
pub const TREE_HINTS: [&str; 9] = [
    "↑↓ move",
    "←→ fold",
    "⏎ zoom",
    "⌫ up",
    "Tab map",
    "d delete",
    "p first",
    "x skip",
    "? help",
];

pub const QUIT_BODY: &str = "Scan results will be lost.";
pub const QUIT_ACTIONS: &str = "    q / y  Quit          [ n / Esc / Enter  Stay ]";

pub const SCAN_ERROR_TITLE: &str = "Scan error";
pub const SCAN_ERROR_BODY: &str = "A scan worker stopped unexpectedly:";
pub const SCAN_ERROR_TAIL: &str = "The scan is stopped; results are partial.";
pub const SCAN_ERROR_ACTIONS: &str = "[ any key  Close ]";

pub const AGGREGATE_WORD: &str = "smaller";
pub const EMPTY_FOLDER: &str = "This folder is empty.";
