# T035: End-to-end test: a real file whose name contains ESC (gap 4 from the T034 runbook)

- **Engineer:** engineer-2. **Schedule:** after T030b. **Depends on:** T029. Small.
- **Test:**
  1. In a temp dir, create a file and a folder whose names contain ESC (`\x1b[2J`), BEL (`\x07`), a C1 control, an invalid-UTF-8 byte and U+202E.
  2. Scan them with the real walker into the app (headless TestBackend, 80×24).
  3. Assert that no rendered buffer cell contains a raw control character or U+202E, in the Tree, map, Largest and Skipped views and in the delete confirmation.
  4. Assert the escaped forms (`\x1B`, `\u{202E}`) appear.
- **Red first:** show it failing with `sanitize` bypassed (one targeted breakage).
- **Files:** `tests/app_names.rs` (new). Add the test to `docs/acceptance/runbook.md` under M10/security.
