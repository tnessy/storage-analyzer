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
pub const HELP_RIGHT: [(&str, &str, &str); 9] = [
    ("d Delete", "d Delete", "Delete (asks first)"),
    ("p", "p", "Scan this folder first"),
    ("x", "x", "Skip this folder's scan"),
    ("r", "r", "Resume / rescan folder"),
    ("s", "s", "Stop the whole scan"),
    ("v", "v", "Last delete's failures"),
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

// ---- delete (UX §5.3) ----

pub const DELETE_TITLE: &str = "Delete permanently?";
pub const DELETE_TITLE_UNFINISHED: &str = "Delete permanently? (scan of this folder not finished)";
pub const DELETE_WARNING: &str = "This cannot be undone. Files are not moved to the trash.";
pub const DELETE_UNFINISHED_WARNING: &str =
    "Everything inside will be deleted, including files not counted yet.";
pub const DELETE_LARGEST: &str = "Largest known inside:";
pub const DELETE_NOTHING_LISTED: &str = "Nothing listed inside yet.";
pub const DELETE_AT_LEAST: &str = "(at least; still counting)";
pub const DELETE_ACTIONS: &str = "y  Delete             [ n / Esc / Enter  Cancel ]";
pub const DELETE_ACTIONS_UNFINISHED: &str =
    "y Delete now  p Finish scan first  [ n/Esc/Enter  Cancel ]";
pub const DELETE_ACTIONS_PRIORITISING: &str = "y Delete now   [ n/Esc/Enter  Cancel ]";
pub const DELETE_PRIORITISING: &str = "Scanning this folder first…";
pub const DELETE_SCAN_ROOT: &str = "Can't delete the scan root.";
pub const DELETE_ALREADY: &str = "Already being deleted.";

pub fn delete_group(count: usize) -> String {
    format!("\"{count} {AGGREGATE_WORD}\" is a group, not a file. Expand it to choose an item.")
}

pub fn delete_other_fs(path: &str) -> String {
    format!("{path} is on another filesystem and wasn't scanned. Not deleted.")
}

pub fn delete_running(path: &str) -> String {
    format!("One delete at a time. Wait for {path} to finish.")
}

pub fn deleting(glyph: &str, path: &str, removed: u64, total: Option<u64>) -> String {
    let removed_text = super::format::thousands(removed);
    match total {
        Some(total) => format!(
            "{glyph} Deleting {path} … {removed_text} / {} files",
            super::format::thousands(total)
        ),
        None => format!("{glyph} Deleting {path} … {removed_text} files removed"),
    }
}

pub fn deleted(path: &str, freed: &str, files: u64, more_than_counted: bool) -> String {
    let n = super::format::thousands(files);
    let tail = if more_than_counted {
        format!("{n} files, more than were counted")
    } else {
        format!("{n} files")
    };
    format!("Deleted {path} · freed {freed} ({tail})")
}

pub fn items_not_deleted(n: usize) -> String {
    format!("{n} items could not be deleted. Press v to see them.")
}

pub const NO_FAILURES: &str = "No delete failures to show.";

pub const RESULT_PARTIAL_TITLE: &str = "Delete finished with errors";
pub const RESULT_PARTIAL_ACTIONS: &str = "v  View failed paths      [ Enter  OK ]";
pub const RESULT_ABORTED_TITLE: &str = "Not deleted";
pub const RESULT_ABORTED_TAIL: &str = "Nothing was deleted.";
pub const RESULT_ABORTED_ACTIONS: &str = "[ Enter  OK ]";
pub const ABORT_CHANGED: &str = "It changed since it was scanned (it may have been replaced or moved). Rescan with r, then try again.";
pub const ABORT_NOT_FOUND: &str = "It no longer exists.";
pub const ABORT_MOUNT_POINT: &str = "It's a mount point (another filesystem or subvolume is mounted here), so it can't be deleted from here. Unmount it first if you want to remove it.";

pub fn abort_open_failed(strerror: &str, errno: i32) -> String {
    format!("It couldn't be opened (errno {errno}: {strerror}).")
}

pub const FAIL_PERMISSION: &str = "permission denied";
pub const FAIL_BUSY: &str = "busy or in use";
pub const FAIL_READ_ONLY: &str = "read-only filesystem";
pub const FAIL_OTHER_FS: &str = "on another filesystem (not crossed)";

pub fn fail_other(errno: i32, strerror: &str) -> String {
    format!("other error (errno {errno}: {strerror})")
}

pub fn deleted_counts(files: u64, dirs: u64, freed: &str) -> String {
    use super::format::thousands;
    format!(
        "Deleted {} files · {} folders · freed {freed}",
        thousands(files),
        thousands(dirs)
    )
}

pub fn failed_title(n: usize, target: &str) -> String {
    format!("Couldn't delete {n} items ═ {target}")
}

pub const FAILED_KEYS: &str = "j/k scroll  Esc close";

// ---- quit while deleting ----

pub const QUIT_DELETING_TITLE: &str = "Quit while deleting?";
pub const QUIT_DELETING_BODY: &str = "A delete is in progress:";
pub const QUIT_DELETING_ACTIONS: &str = "y  Quit anyway      [ any other key  Keep deleting ]";

pub fn quit_deleting_progress(removed: u64, total: Option<u64>) -> String {
    use super::format::thousands;
    match total {
        Some(total) => format!(
            "{} of {} files removed.",
            thousands(removed),
            thousands(total)
        ),
        None => format!("{} files removed so far.", thousands(removed)),
    }
}

/// The line printed after the terminal is restored (the command name is added by the caller).
pub fn delete_interrupted(path: &str, removed: u64, total: Option<u64>) -> String {
    match total {
        Some(total) => format!("delete of {path} interrupted: {removed} of {total} files removed"),
        None => format!("delete of {path} interrupted: {removed} files removed"),
    }
}

pub const DELETE_HARD_LINKS_UNKNOWN: &str = "Hard links: not fully known (scan not finished).";

pub fn delete_hard_links(files: u64, freed_upper: &str) -> String {
    let (n, verb) = if files == 1 {
        ("1 file".to_owned(), "has")
    } else {
        (format!("{} files", super::format::thousands(files)), "have")
    };
    format!(
        "Space freed may be less: {n} also {verb} hard links outside this folder, so at most {freed_upper} is freed."
    )
}

pub fn delete_ended_with_errors(path: &str, removed_files: u64, failed: usize) -> String {
    format!(
        "delete of {path} ended with errors: {removed_files} files removed, {failed} items could not be deleted"
    )
}

pub fn delete_not_done(path: &str) -> String {
    format!("delete of {path} was not done: nothing was deleted")
}

// ---- other-filesystem rows (UX §5.4) ----

/// Cells for the folder name and its `/` in the message, so it fits 78 cells.
pub const OTHER_FS_NAME_ROOM: usize = 15;
pub const OTHER_FS_ROW_ADVICE: &str =
    "is on another filesystem. To scan it, use --cross-filesystems.";
