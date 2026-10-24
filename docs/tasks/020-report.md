# T020 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/app/controls.rs` (new), `src/app/{state,keymap,event_loop,list_state,mod}.rs`, `src/ui/{status,copy,treemap,tabs,types_view,largest_view,skipped_view}.rs`, `tests/app_controls.rs` (24 tests).

## Evidence (tests first)
- `tests/app_controls.rs` was written against stubs (`phase()` = model state, `rescan` and `process_events` empty, `fit_hint` empty): 23 of 24 failed on behaviour, the 24th only checks that the copy constants exist. The harness is the real walker over `FakeFs` (one thread) behind a gate that lets one directory read through per permit, plus a hand-fed variant for the cases that need a hand-built model.
- The Skipped re-key (plan D12) was written before its test failed on its own, so I broke it on purpose: with the old position-as-key scheme restored, `a_rescan_that_removes_an_earlier_issue_keeps_the_cursor_on_its_row` fails; reverted.
- Fresh: fmt, full clippy, `cargo test --all-targets`, doc tests, release build all pass.

## Behaviour
- `p`/`x`/`r`/`s` (Tree view; `r` also on Skipped rows), all text in `copy.rs`, all state changes in `app/controls.rs`:
  - `p` toggles the priority (`ScanControl::set_focus` + `Model::set_focus` + `AppState.priority`); zoom prioritises the view root, and zooming to the scan root clears it (via `ZoomChanged`, consumed in the loop by `process_events`). A priority folder that completes shows `✓ x/ fully scanned: … · N files` for 5 s and clears `▲`. While a priority is set the status starts with `▲ First: x/`.
  - `x`: `cancel_prefix` + `mark_skipped`; `Already skipped. r resumes.` and `Already fully scanned.` cases.
  - `r`: `Model::rescan_plan`: `Full` → `AppState::rescan` (the single sequence: `remove_prefixes_under`, `forget_reserved`, `begin_rescan`, `ScanHandle::rescan`); `PrioritiseAndResume` → priority plus a rescan per outermost skipped folder (never the folder itself), with UX's 0/1/N messages and `Already scanning x/ first.`; `NoneWhileScanning` → `Scan in progress. Press s to stop, then r to rescan.`
  - `p x r` on a file or aggregate row → `p, x and r work on folders.`; the `..` interception stays ahead of all of them.
- Stopping: `s` calls `cancel_all` and freezes elapsed; the app is Stopping while cancel-all is set and the scan is not Done (`■ Stopping…  01:12 elapsed`, no ETA). `r/p/x` then only show `Still stopping the scan. Try again when it says Stopped.` and are not replayed; a second `s` does nothing; an internal `rescan()` call is queued and runs once after `Done`. Stopped shows the frozen time.
- A rescan makes `phase()` Scanning until its `Done` arrives (the model's own state stays Done/Stopped). Restarting the root resets the clock, rate and baseline of entries seen, calls `use_session_previous()`, and the restarted scan's `Done` goes through `scan_finished(Root)` again (session record and `--history` updated with that scan's own entry count, not a running total). Subtree rescans don't touch either.
- Skipped list rows are keyed by node (`RowKey::Issue(NodeId)`), so a rescan removing another issue doesn't move a held cursor.
- One-filesystem hint: ` · not scanned (other fs): <path>` (shallowest, ties by name) plus ` +N`, on every state; narrow widths drop the bar first (the line is built for the room left), then ` · K other fs not scanned`; never with `--cross-filesystems`. Wording is two constants in `copy.rs` (`OTHER_FS_HINT`, `OTHER_FS_HINT_SHORT`).
- `AppState` now owns `control` and the `ScanHandle`; `App::finish` takes them from there.

## Concerns
- **Model bug for engineer-1 (T012):** `begin_rescan` (via `adjust_top_done`) underflows `top_done` (`attempt to subtract with overflow`, `model/control.rs`) when rescanning a *top-level* directory that is complete but was never counted in `top_done`, e.g. an entry that came with an issue (skipped by user or permission denied) directly under the root. `r` on such a row in the Skipped list would panic in a debug build. My test avoids it by putting the skipped folders one level down. Needs a fix and a test on the model side.
- `r` on a still-scanning folder shows the UX message only; the folder-level `Stopping` gate uses `ScanControl::is_cancel_all`, which `ScanHandle::rescan` resets, so a rescan started from the walker-side queue could clear it. I did not find a case in the tests.
- The ◌-stays-on-unfinished-rows check after `s` (brief) is not asserted: after `Done { cancelled }` the model marks those rows incomplete, and its own tests cover that; my test checks the Stopped text and the full rescan instead.
- A priority/skip/resume in the Types or Largest views is ignored (only Tree and Skipped handle those keys).
