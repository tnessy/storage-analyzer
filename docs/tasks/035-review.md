# T035 review (Heimdall, 2026-10-23): APPROVED, committed with T030b
- **What it does:** `tests/app_names.rs` creates real files and folders whose names contain ESC `[2J`, BEL, C1 controls, invalid UTF-8 and U+202E, plus an unreadable folder with an escape in its name. The real walker scans them at 80×24.
- **What it asserts:** no cell holds a control or bidi character, and the escaped forms are shown, in the Tree/map, Largest, Skipped and both delete confirmations. It returns early when run as root.
- **Red:** with `sanitize` bypassed it fails (`tree and map: raw '\u{202e}' on row 19`).
- **Loop:** 60/60 runs, 0 failures.
- **Accepted:** it asserts the lowercase `\x1b` that `sanitize` actually writes (the brief said `\x1B`).
- **Runbook:** the test is listed under M10 and the UX check 11 row; gap 4 is removed.
