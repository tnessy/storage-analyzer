# T019 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree: HEAD plus the T019/T022 files):
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: lib 104, app_eta 22, history 8, app_loop 19, …;
- release build ok.

**Process:** this follows the new rule. 10 of 12 wiring tests were red first. For the two negative tests that passed on stubs, a targeted breakage per criterion is reported (5 breakages, each failed then reverted). **My sample re-run:** forcing history on regardless of `--history` (`state.rs: if true`) makes `without_history_nothing_is_read_or_written` fail ✓.

**Real binary** (by me, release build under a pty, temp `XDG_STATE_HOME`):
- **Default run on `/usr/share/doc`:** zero files created under `XDG_STATE_HOME` (M9 / design §4 zero writes) ✓. The status line has no `%  ETA` (indeterminate, no source) ✓.
- **`--history` run:** exactly one `storage-analyzer/history.json` containing `{root: /usr/share/doc, one_filesystem: true, entries: 15769, dirs: 535, bytes: 389734400, duration_ms: 104}`. The bytes equal `du -s -B1` ✓.

**Pass 1, spec:**
- source priority History > SessionPrevious (complete root scans only) > FsInodes > none;
- history is opt-in, read on its own `history-load` thread after frame 0, written atomically with no temp file left, capped at 50 roots, a corrupt file ignored with a message;
- the status text follows UX §5.2, including the drop orders;
- `ETA estimating…` appears only with a source.

**Accepted small out-of-list edits:**
- `eta/mod.rs`: one `pub mod sources;` line;
- `theme.rs`: spinner frames and `bar_empty` in `Glyphs` (T023 owns the ASCII variants);
- `tile_colors.rs`: a one-line test type annotation (lint).

**Notes:**
- History save runs synchronously on the loop thread: one small file, at Done only. Accepted. If a slow disk ever hitches a frame, move the save to the `history-load`-style thread.
- **Spec nit for UX:** §5.2 says the 80-column Indeterminate example is "79 cols", but the string is 78 cells. The tests use the string. UX should fix the number.
- **For T020** (added to its brief): when `r` on the root restarts the scan, call `AppState::use_session_previous()`. The restarted scan's `Done` must reach `scan_finished(Root)` again, so the in-session source and the history update work for rescans.
