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
pub const ETA_UNKNOWN_SHORT: &str = "ETA unknown";
pub const ETA_WORD: &str = "ETA";
pub const ETA_ESTIMATING: &str = "ETA estimating…";
pub const ETA_PAST_ESTIMATE: &str = "ETA unknown (past estimate)";
pub const ESTIMATE_WORD: &str = "estimate";
pub const SOURCE_LAST_SCAN: &str = "last scan";
pub const SOURCE_FILESYSTEM: &str = "filesystem";
pub const OF: &str = "of";
pub const TOP_LEVEL_DONE: &str = "top-level done";
pub const QUEUED: &str = "queued";
pub const FOLDERS_QUEUED: &str = "folders queued";
pub const ENTRIES: &str = "entries";
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

pub const HISTORY_IGNORED: &str = "History file ignored: it could not be read.";
pub const HISTORY_NOT_SAVED: &str = "Could not save the history file.";

pub const SHOWING_APPARENT: &str = "Showing apparent size.";
pub const SHOWING_DISK: &str = "Showing disk usage.";
pub const HELP_TITLE: &str = "Help";
pub const HELP_CLOSE: &str = "Esc or ? close";

/// Hints in the ASCII fallback: letter keys instead of arrows, `Enter`/`Bksp` spelled out.
pub const TREE_HINTS_ASCII: [&str; 9] = [
    "j/k move",
    "h/l fold",
    "Enter zoom",
    "Bksp up",
    "Tab map",
    "d delete",
    "p first",
    "x skip",
    "? help",
];
pub const MAP_HINTS_ASCII: [&str; 7] = [
    "hjkl move",
    "Enter zoom",
    "Bksp up",
    "Tab tree",
    "d delete",
    "p first",
    "? help",
];
pub const LIST_HINTS_ASCII: [&str; 4] = ["j/k move", "Enter show in tree", "d delete", "? help"];

pub const HELP_LEFT_TITLE: &str = "Move and view";
pub const HELP_RIGHT_TITLE: &str = "Act and scan";
/// Key column and description for each entry: (unicode keys, ASCII keys, description).
pub const HELP_LEFT: [(&str, &str, &str); 10] = [
    ("↑↓ j k", "Up Dn j k", "Move"),
    ("PgUp PgDn", "PgUp PgDn", "Page"),
    ("g G", "g G", "Top / bottom"),
    ("→ l", "Right l", "Expand / into"),
    ("← h", "Left h", "Collapse / parent"),
    ("⏎", "Enter", "Zoom in, scan it first"),
    ("⌫ -", "Bksp -", "Up one level"),
    ("Tab", "Tab", "Switch tree / map"),
    ("1 2 3 4", "1 2 3 4", "Tree Types Largest Skipped"),
    ("a", "a", "Disk usage / apparent"),
];
pub const HELP_RIGHT: [(&str, &str, &str); 8] = [
    ("d Delete", "d Delete", "Delete (asks first)"),
    ("p", "p", "Scan this folder first"),
    ("x", "x", "Skip this folder's scan"),
    ("r", "r", "Resume / rescan folder"),
    ("s", "s", "Stop the whole scan"),
    ("?", "?", "Show / hide this help"),
    ("q Ctrl-C", "q Ctrl-C", "Quit (asks first)"),
    ("Esc", "Esc", "Close dialog or help"),
];
pub const HELP_MAP_NOTE: &str = "Map: files are coloured by type; the tag shows it (e.g. [V] video). Folders end in / and get their own colour, never a file-type colour in this view.";
pub const HELP_ETA_FOOTER: &str = "ETA appears once a previous complete scan of this root is known. Run with --history to remember past scans (keeps one small file in ~/.local/state).";

pub const HELP_HARD_LINK_NOTE: &str = "Hard links count once, in the folder scanned first: totals match du, folder sizes may not. Deleting one name of a hard-linked file frees no space.";

/// ` · not scanned (other fs): <path>` plus ` +N` for more; `{path}` and `{more}` are filled in.
pub const OTHER_FS_HINT: &str = " · not scanned (other fs): {path}{more}";
/// The short form: ` · 3 other fs not scanned`.
pub const OTHER_FS_HINT_SHORT: &str = " · {count} other fs not scanned";

pub const NOT_A_FOLDER: &str = "p, x and r work on folders.";
pub const PRIORITY_CLEARED: &str = "Priority cleared.";
pub const ALREADY_SCANNED: &str = "Already fully scanned.";
pub const ALREADY_SKIPPED: &str = "Already skipped. r resumes.";
pub const STILL_STOPPING: &str = "Still stopping the scan. Try again when it says Stopped.";
pub const SCAN_IN_PROGRESS: &str = "Scan in progress. Press s to stop, then r to rescan.";
pub const STOPPING: &str = "Stopping…";
pub const FIRST_PREFIX: &str = "First:";
