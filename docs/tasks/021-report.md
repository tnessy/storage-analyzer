# T021 report: delete dialogs, quit during delete, failure list (engineer-2 / Sindri)

## Status: DONE_WITH_CONCERNS

## What was built
- `src/app/delete_flow.rs`: the whole delete flow on `AppState`.
  - `d` (gated by `Model::delete_refusal`), the confirmation, `y` (`cancel_prefix` + `begin_delete` + `submit`), and `p` in an unfinished dialog.
  - Executor events and the outcomes: `Removed` shows a transient; `Partial` shows the result dialog and keeps `last_delete_failures`; `Aborted` shows `Not deleted`.
  - Reconcile goes through `AppState::rescan` (queued while Stopping).
  - `v`, the failure list, and the quit-while-deleting keys.
- `src/ui/delete_dialog.rs`, `result_dialog.rs` (Partial, Aborted, failed list), `quit_dialog.rs` (the T015 quit rendering moved here, plus the deleting variant). All text is in `ui/copy.rs`.
- Loop (`event_loop.rs`): drains the delete channel each step; refreshes the open prompt at most every 500 ms (it turns exact in place); keeps `delete`-thread panics (`PanicSlot::take_if`) and shows the message as an extra line in the result dialog.
- Exit order (`App::finish`, now returns `Option<String>`): `stop()`, `wait_idle` up to 2 s while draining `Progress`, drop the handle, scan exit, return to `run_tui`, terminal guard drops, `main_with` prints `<bin>: delete of <path> interrupted: N of M files removed`. SIGTERM/SIGHUP take the same path.
- `production_services(root)` opens the root with `O_DIRECTORY|O_NOFOLLOW` and spawns the executor.
- Help: `v  Last delete's failures` (19 entries, still 20 rows); help snapshots regenerated.
- Status line: `Deleting <path> … N / M files` (or `… N files removed`).

## Tests
- `tests/app_delete.rs` (16, +1 `#[ignore]` mount-point test run under `unshare -rm`, which passes), `tests/app_quit.rs` (5), snapshots: exact, unfinished, after-`p`, Partial, failed paths, Aborted (MountPoint), both quit dialogs, in Unicode and ASCII (80x24).
- Red: against the stubs, 10 of 13 app_delete tests failed (3 passed vacuously: parent-row hint, running-status, second delete when the first had finished); app_quit did not compile (`finish()` returned `()`). Tests added later got one targeted breakage each: Partial rescan removed -> stopping test fails; panic note removed -> executor-panic test fails.
- Final runs: `cargo fmt --check` clean; `clippy --all-targets --all-features -D warnings` clean; `cargo test --all-targets` all pass (one rare flake, below); `cargo test --doc` ok; `cargo build --release` ok.

## Deviations and things to look at
- Failed-path ordering is by path, not by bytes: `DeleteFailure` carries no size, and the nodes may be gone from the model. Needs a decision if bytes ordering is wanted.
- NO_COLOR variants are not snapshotted: snapshots capture text only, so styles are untested.
- ASCII mode renders `≥` as `>` (existing `ascii::text` mapping), which reads like "greater than"; `>=` may be clearer (T016 owned).
- Dialog continuation lines of wrapped paragraphs lose the 2-space indent (shared `dialog.rs` behaviour, not changed).
- The hard-link and "Everything inside will be deleted" wordings are mine (the spec text was not in my context); UX should confirm. Abort reason texts for `ChangedSinceScan`, `NotFound`, `OpenFailed` are also mine.
- Small edits outside the listed files: `controls.rs` (`say`, `folder_label`, `set_priority`, `clear_priority` made `pub(super)`), `terminal.rs` (`PanicSlot::take_if`), `tests/app_eta.rs` (`delete: None` in a `Services` literal), `app/mod.rs` (production wiring, `run_tui` returns the interrupted line).
- Flake: `app_controls::r_after_a_stop_is_a_full_rescan_at_once` failed once in about 25 full runs (the T020 test; 0 failures in 15 isolated runs of that file). The change touches nothing in that path; likely a timing race in the test's gated walker. Not investigated further.
- A confirmed delete with no executor (only possible in tests with default `Services`) ends as an `Aborted(OpenFailed(EBUSY))` dialog rather than panicking.

## Round 2 (engineer-2)
Done, tests first (each was seen failing before the change):
1. Copy: hard-link line (`Space freed may be less: N files also have…`, singular `1 file also has`), unfinished warning without glyph, `Hard links: not fully known (scan not finished).` as its own paragraph (after the warning), Not-deleted reasons (changed / no longer exists / `couldn't be opened (errno N: …)`). Tests read the dialog as paragraphs (wrapped lines joined).
2. ASCII `>=`: `ascii::text` folds `≥` to `>=`; the dialog uses `Glyphs::lower_bound`. The tree SIZE column is 8 cells wide in ASCII (rows, `..` row and the column heading shift by one). Tests: the unfinished dialog says `>= 19.1M`, and the ASCII tree is exactly one cell right of the Unicode one with the headings moving with the columns. ASCII tree snapshots updated (scanning, done, empty, stopped). The list views (Types, Largest) never show a lower bound, so their 7-cell column is unchanged.
3. Exit (architect):
   - `DeleteHandle::detach()` skips Drop's second wait, and `Drop` waits only when it owns the thread. `delete::spawn_with` is now `pub` (`#[doc(hidden)]`) so tests can inject executors. Test: an executor that never finishes, `q y` returns in under 2.5 s.
   - `stop_delete` keeps `Finished`:
     - `Removed` prints nothing.
     - `Partial` with failures prints `delete of P ended with errors: N files removed, K items could not be deleted`.
     - `Aborted` prints `delete of P was not done: nothing was deleted`.
     - A stop-induced `Partial` without failures, or no `Finished`, still prints `interrupted: N of M files removed`, with N from `Progress`.
   - The earlier full-channel test now uses a scripted executor (7 files reported, `Finished` undeliverable) and asserts `interrupted: 7 of 10 files removed` within 2 s.
   - The confirmation whose target becomes refused now closes with the refusal text (`refusal_text` takes the target; it used the tree selection before, which was wrong for other-filesystem).
4. Stub-passing tests, each with one breakage shown failing: status line (condition disabled -> fails), second delete (refusal disabled -> fails), parent-row hint (arm disabled -> fails; the test asserted only a non-empty message, so it now compares the exact hint text, which is why it was weak). The running-status test is now deterministic (injected `DeleteRun`, exact text for known and unknown totals).

Final: fmt, clippy `-D warnings`, doc tests and release build clean; `cargo test --all-targets --no-fail-fast` passes except `app_controls::r_after_a_stop_is_a_full_rescan_at_once` (T028, the walker race; it failed in most of my last full runs, with the tree carrying engineer-1's uncommitted `scan/queue.rs` change; not touched here). The `--ignored` mount-point test passes.

Not done: returning wrapped dialog lines to the 2-cell margin (UX stated rule, not in the round 2 list; it needs a change in `dialog.rs` shared by all dialogs). Say if it should go in here.

## Item 9 and 10
- 9: the failure-list footer is `j/k scroll  Esc close` in both modes (test + snapshots).
- 10: `app_controls` flake: `Gate::hold()` drops the leftover permits after `Stopped`, so the rescan in `r_after_a_stop_is_a_full_rescan_at_once` and in the "r on the root" test cannot finish before the assertions look at it; the gate is granted again afterwards. The queued-rescan test already asserts before granting. Evidence: 200 consecutive runs of the whole `app_controls` binary, 0 failures.
- Red/green for item 10: with `hold()` disabled, the `app_controls` binary failed 20 of 300 runs; with it, 0 of 200. The report's earlier "accepted as built" notes about the footer are superseded by item 9.

## Round 2 follow-up (architect)
- Security: the exit messages (`stop_delete`) and the pre-TUI path errors (`PreflightError`) now pass the path through `sanitize::sanitize`. Tests: `exit_messages_never_carry_escape_bytes_from_the_path` (interrupted, ended-with-errors, not-done; seen failing with sanitizing off) and `a_path_with_escape_bytes_is_printed_escaped` (missing path and not-a-directory; seen failing before the fix). Clap's own usage errors for an option containing ESC printed no ESC byte.
- Flake: on a clean extract of a83a9c5 (with this tree's `app_controls.rs`), 25 full parallel suite runs had 0 failures and 200 runs of the `app_controls` binary had 0 failures. This supports the architect's reading that my earlier failures came from a mixed tree.
- `spawn_with` gating: NOT done, it needs Cargo.toml (lead only): `[features] test-hooks = []` and a dev-dependency `storage-analyzer = { path = ".", features = ["test-hooks"] }` so integration tests enable it. Then `spawn_with` becomes `#[cfg(any(test, feature = "test-hooks"))] pub fn` with `spawn` calling a private inner function. It stays `pub` + `#[doc(hidden)]` until then.

## Round 3
- Flake (item 10): `git diff tests/app_controls.rs` shows the `Gate::hold()` change (+11 lines) in the working tree. After all round-3 changes: 200 runs of the whole `app_controls` binary, 0 failures.
- Full sizes in dialogs and messages (`size_iec_long`, five spots in `delete_dialog.rs`, `result_dialog.rs`, `delete_flow.rs`): summary line `Folder · 24.0 KiB`, lower bounds `≥ 19.1 MiB` / ASCII `>= 19.1 MiB`, largest-known entries, the hard-link `at most X KiB is freed`, the Partial dialog's `freed`, and the `Deleted … freed 24.0 KiB (3 files)` message. Each has an assertion (four failed before the change, plus the snapshots).
- Wrapped dialog lines (`dialog.rs`): paragraphs are wrapped by the dialog itself and every row keeps the 2-cell left margin; a line starting with spaces is a list item that keeps its indent, never wraps and ends in `…`. A line that fits keeps its spacing. Test `wrapped_lines_keep_the_margin_and_list_items_truncate` (failed first on an unindented continuation row), plus a unit test. A line of exactly 60 cells (the unfinished-dialog action hint) touches the right border; that is the UX text's length, not changed.
- Sanitizing the exit messages and the pre-TUI errors: done earlier (see above). `delete::spawn_with` stays `pub` + `#[doc(hidden)]` per the lead's ruling; no Cargo.toml change.
- Checks: fmt, clippy `-D warnings`, whole suite (no failures), `--ignored` mount-point tests, doc tests, release build.

## Round 3b: dialog width rule
- Usable text width is the dialog width minus 6 (`dialog.rs`), so text never touches the right border. `DELETE_ACTIONS_UNFINISHED` is now `y Delete now  p Finish scan first  [ n/Esc/Enter  Cancel ]` (58 cells).
- Test: every dialog in the snapshot loop (exact, unfinished, prioritising, Partial, Aborted, both quit-while-deleting, plain Quit) is checked row by row in Unicode and ASCII at 80x24 (dialog width 64): each inner row is 62 cells and ends in two spaces. Failed first on the old unfinished hint. The failure list is a full-screen overlay and is excluded. Snapshots updated.
