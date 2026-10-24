# T030b review (Heimdall, 2026-10-23): CHANGES REQUESTED (one item: the architect's mitigation)

**Fresh evidence** (clean worktree: HEAD plus exactly the T030b files, hash-recorded; engineer-1's T031 files excluded):
- fmt ok, whole-tree clippy clean;
- **every target passes, including the examples**: ui_lists 24 (8 new `updating::*`), app_loop 20, …;
- release build ok;
- 50/50 runs each of ui_lists, app_loop and app_controls.

**Pass 1:**
- `set_incremental_top(true)` at start;
- `step_top_rebuild(5 ms)` after the drain while `top_updating()`;
- a 10 ms input-wait cap while pending, a redraw when the rebuild ends;
- the Largest view per UX §5.5 / check 22 (header placement, the 200 ms delay or immediate when empty, old list minus deleted, a one-frame swap, `top_epoch()` in the stamp, `No files found.` hidden while pending, the `Nothing to delete here.` path, the selection rule);
- the runbook's check 22 row is filled in.

Red first, or a targeted breakage per criterion, is reported (README rule).

**Out-of-list edits accepted:** `ui/tabs.rs` (~6 lines, header drawing) and `app/delete_flow.rs` (the `d` message on an empty Largest list).

## Required: the architect's mitigation, added to this brief on 2026-10-23, is **not done**
`src/app/delete_flow.rs::on_delete_event` → `DeleteEvent::Finished` calls `self.model.apply_delete(&event)` (the ~80 ms `remove_subtree` on a large folder) **before** `show_outcome(...)`, in the same handler with no frame between them. The user therefore sees nothing for the length of the removal, and only then the result.

**Fix:**
1. On `Finished`, first record the outcome and show it (`show_outcome`: the message, or the Partial/Aborted dialog), and **mark the model application as pending**.
2. Draw that frame.
3. Apply the removal (`apply_delete`, the priority check, the `Reconcile::Rescan`) at the start of the **next** loop step.

Keep it in one small, obvious place (for example `pending_delete_apply: Option<DeleteEvent>`, handled right after input).

- **Nothing may read stale state in between:**
  - a second `d` during that one frame must be refused as "delete running" (the model's delete target is still set, so this holds naturally; test it);
  - the outcome text must use the counts from `Outcome`, not from the model.
- **Tests (red first):**
  - with a recording hook, the frame containing the outcome text is drawn **before** `apply_delete` runs, and the totals reflect the removal on the next frame;
  - a `d` pressed in between gets the existing "One delete at a time" refusal;
  - Partial/Aborted → the dialog shows first, then the D1 rescan is issued after it.

**Then:** the five commands plus 50-run loops of app_delete and ui_lists. I'll re-verify and commit the snapshot.

## Re-review (2026-10-23): APPROVED, committed (hash-checked snapshot; together with T035)
- **Fresh evidence** (clean worktree: HEAD plus exactly the T030b and T035 files and the runbook edit; engineer-1's T031 files excluded):
  - fmt ok, whole-tree clippy clean;
  - **every target passes, including the examples**; `--ignored` passes;
  - release build ok;
  - **60/60 runs each** of app_delete, ui_lists, app_names, app_loop and app_controls.
- **Mitigation verified in code:**
  - `Finished` → `show_outcome` first, then `pending_delete = Some(event)`;
  - the outcome frame is forced out, past the 10 Hz cap;
  - the next `step()` begins with `apply_pending_delete()` (`apply_delete`, the priority check, `Reconcile::Rescan` through the gated `rescan`) before input;
  - the target stays tombstoned in the gap, so `d` gets "One delete at a time".
- **Tests:** four new app_delete tests were red on the old order. Two existing tests were adapted to the extra step.
