# T030b: Wire the stepped top-N rebuild into the loop and the Largest view

- **Engineer:** engineer-2. **Schedule:** after T029. **Depends on:** T030 round 2 (engineer-1: the reserve and API names). **Start only after Heimdall confirms that round 2 is merged.**
- **Source:** T030 Part 4 (user-approved), UX §5.5 and check 22, design §3.4.

## Acceptance
1. **Event loop:** each iteration, **after** the scan-message drain, call `model.step_top_rebuild(5 ms)` while a rebuild is pending (`top_updating()`, or the final name from round 2). While pending, the loop never blocks on input for more than 10 ms.
   - If round 2 keeps the `set_incremental_top` switch, turn it on once at startup.
2. **Largest view** (UX §5.5, check 22):
   - `updating…` (ASCII `updating...`), right-aligned on the column-header row with a 1-cell right margin, in the header style;
   - shown only if the rebuild is still pending **200 ms** after it started, **but immediately** when every visible entry was deleted;
   - **rows:** the old list minus deleted files; the new list appears in a single frame (use `top_epoch()` in the view's refresh stamp);
   - **empty case:** no rows, `No files found.` hidden while pending, nothing selected, and `⏎`/`d` show `Nothing to delete here.`;
   - if the finished list is empty: `No files found.`, and no `updating…`;
   - **selection:** the same file stays selected; if it was deleted, the same position, or the last row;
   - `⏎` and `d` stay enabled for every row.
   - Copy constant `LARGEST_UPDATING`.
3. **Tests (red first):**
   - every check 22 item, with a fake clock and a gated rebuild (no text at 100 ms; text at 250 ms within 80 columns; rows = old minus deleted; `⏎` works; text gone after; under 200 ms never shown; the empty edge case);
   - the selection rule;
   - the loop's 10 ms input-wait cap while pending.

## Files owned
`src/app/event_loop.rs`, `src/app/state.rs`, `src/ui/largest_view.rs`, `src/app/list_state.rs`, `src/ui/copy.rs`, `tests/ui_lists.rs`, `tests/app_loop.rs`, and the affected snapshots.

**Also:** turn on `set_incremental_top(true)` at startup. **Add the check 22 tests to `docs/acceptance/runbook.md`** (it lists check 22 as a gap until this task).

**Also (architect mitigation for the accepted ~80 ms `remove_subtree` step):** when a delete finishes, **draw the frame that shows its outcome** (the result message, or the Partial/Aborted dialog) **before** calling `Model::apply_delete`, which removes the subtree. The pause then comes after the user already sees what happened. Do it in the event loop's `DeleteEvent::Finished` handling.
- **Test:** with a model hook that records when the removal runs, the outcome frame is drawn first. The model-applied totals still update on the next frame.
