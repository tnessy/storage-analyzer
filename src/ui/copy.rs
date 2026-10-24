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

pub const MAP_HINTS: [&str; 7] = [
    "arrows move",
    "⏎ zoom",
    "⌫ up",
    "Tab tree",
    "d delete",
    "p first",
    "? help",
];
pub const MAP_EMPTY_FOLDER: &str = "This folder is empty.";
pub const FOCUS_MARK: &str = "*";
pub const MAP_ALL_ZERO: &str = "items, all 0 B. Nothing to map.";
pub const NOTICE_SELECTED_LONG: &str = "selected item too small to show";
pub const NOTICE_SELECTED_SHORT: &str = "selected too small";
pub const NOTICE_SELECTED_MIN: &str = "selected hidden";

pub const TYPES_COLUMNS: &str = "   SIZE     FILES      %  TYPE";
pub const LARGEST_COLUMNS: &str = "   SIZE  PATH";
pub const SKIPPED_COLUMNS: &str = "  PATH (REASON)";
pub const PARTIAL: &str = "partial";
pub const NO_FILES_YET: &str = "No files yet.";
pub const NO_FILES_FOUND: &str = "No files found.";
pub const NOTHING_SKIPPED: &str = "Nothing skipped.";
pub const EXT_NONE: &str = "(none)";
pub const EXT_OTHER: &str = "(other)";

pub const CATEGORY_VIDEO: &str = "Video";
pub const CATEGORY_IMAGE: &str = "Images";
pub const CATEGORY_ARCHIVE: &str = "Archives";
pub const CATEGORY_CODE: &str = "Code & text";
pub const CATEGORY_BINARY: &str = "Binary";
pub const CATEGORY_DOCUMENTS: &str = "Documents";
pub const CATEGORY_AUDIO: &str = "Audio & other";

pub const REASON_DENIED: &str = "permission denied";
pub const REASON_VANISHED: &str = "vanished during scan";
pub const REASON_OTHER_FS: &str = "other filesystem";
pub const REASON_SKIPPED: &str = "skipped by you";
pub const REASON_READ_ERROR: &str = "read error:";

pub const LIST_HINTS: [&str; 4] = ["↑↓ move", "⏎ show in tree", "d delete", "? help"];

pub const PARENT_ROW_HINT: &str = "\"..\" is the parent folder. Press ⏎ to go up.";
pub const PARENT_ROW_NAME: &str = "..";
pub const PARENT_ROW_UP: &str = "up to";
pub const MAP_UP_HINT: &str = "⏎ goes up to";

/// The two help-overlay lines about map colours (T023 renders them).
pub const HELP_MAP_COLOURS: (&str, &str) = (
    "Map: files are coloured by type (tag shows it, e.g. [V] video). Folders end in / and get their own colour,",
    "never one used by a file type in the same view.",
);
