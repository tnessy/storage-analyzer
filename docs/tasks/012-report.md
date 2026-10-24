# T012 report (engineer-1)

**Status:** DONE, except one test that needs T011 (see below)

Files: `src/model/control.rs` (new), `src/model/mod.rs`, `tests/model_controls.rs`. Small supporting edits in model internals I wrote earlier: `src/model/hardlink.rs` (`HardLinks::remove`, `Removal`), `src/model/index.rs` (`Indexes::remove`), `tests/model_indexes.rs` (folder-category tests replaced by a file-category test).

## Failing first
All 23 `model_controls` tests failed (the stubs were `todo!`, `Identity` was ignored) before any implementation. After implementing: 23/23 pass, 3 repeated runs stable.

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` all pass (105 lib, 23 model_controls, others unchanged); doc ok; release build ok.

## What is in
- `mark_skipped`, `begin_rescan` (+ `remove_subtree(id, keep_root)` in `control.rs`, reusable by T011), `ScanMsg::Identity`, vanished rescan roots, `set_focus` / `take_focus_completed`, `rescan_plan` with `RescanPlan::{Full, PrioritiseAndResume, NoneWhileScanning}`.
- Removed the folder branch and the `dir_categories` cache; `category_of_node` is now file-only (it is still called by `app::map_state` and `ui::treemap` for files, so it stays).
- Integration tests with `FakeFs` + the real walker (`threads = 1`, channel capacity 1): skip then resume equals an unskipped scan; rescanning a folder clears the skips inside it; the `rescan_plan` acceptance sequence lists every directory exactly once, lists S afterwards, ends exact, and `take_focus_completed` reports F.

## Decisions to check
- `remove_subtree` marks nodes `REMOVED`, subtracts sizes/counts/pending/inexact from the ancestors, takes counted files out of the ext totals, drops their issues (`swap_remove`, so the issue order can change), re-charges a hard link to the next live duplicate (copying the removed link's sizes), clears the focus/rescan-root entries, and calls `recompute_top()` once. `top_total`/`top_done` are adjusted (root rescan resets both to 0).
- `mark_skipped` records a `SkippedByUser` issue with `Op::Read`. If the node already has another issue it returns false.
- `begin_rescan` on a file or symlink only clears its issue and registers it as a rescan root (no `PENDING`).
- `Identity` is applied only for nodes registered by `begin_rescan`; anything else is ignored. A rescan root leaves the set on its listing, a `DirIssue`, or when it turns into a leaf.
- `DirIssue(Vanished)` on a rescan root (not the scan root) removes the node; elsewhere it is an ordinary issue.
- `set_focus` ignores an already-complete node, so `take_focus_completed` only reports a real transition. `rescan_plan` treats `Starting` like `Scanning`.
- `rescan_plan` reads only the issue list (bounded by the number of skips times depth).
- `ext`/top/hard-link bookkeeping for `Identity` runs through the same helpers (`unregister_link`, `recharge`).

## Not done here
- The criterion-4 test "a delete retry succeeds after dir became a file" needs `Model::begin_delete` (T011); I will add it there.
- `forget_reserved` (UI tile colours) is called by the app side; the model does not know about it.
