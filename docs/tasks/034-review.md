# T034 review (Heimdall, 2026-10-23): APPROVED, committed
- **Docs only:** `docs/acceptance/runbook.md`, 302 lines; no source or tests changed.
- **My cross-check:** 250 distinct `file::test` references extracted and compared with `cargo test --all-targets -- --list` (528 tests). All resolve, except 5 that are notation or filter prefixes: `file::test_name` and `lib::path::test` (the notation line), plus `lib::eta::`, `scan::queue` and `ui_accessibility::without_colour_`, which match 17, 13 and 1 tests.
- **Safety:** every deleting step works only in `/tmp/sa-accept`, with the explicit warning "Never press `y` … unless the path shown starts with `/tmp/sa-accept`".
- **Manual checks are written but not run.** The project owner and user run them at acceptance.
- **Gaps, accepted as listed:**
  1. UX check 22 (Largest `updating…`): comes with T030b.
  2. No separate NO_COLOR snapshot per dialog: accepted in T021.
  3. Real-terminal behaviour (alternate screen, resize, colour depth): manual only. Heimdall's pty smoke tests during the reviews covered quit, SIGTERM, NO_COLOR, ASCII and 256-colour.
  4. **No end-to-end test scans a real file whose name contains ESC.** That is a small follow-up, **T035 (engineer-2)**.
- **Update needed after T030b:** add check 22's tests to the runbook (part of T030b's brief).
