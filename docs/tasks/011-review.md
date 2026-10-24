# T011 review (Heimdall, 2026-10-23): CHANGES REQUESTED (three small items). Architect second review requested in parallel.

**Fresh evidence** (clean worktree: HEAD plus `src/model/{mod,control,delete}.rs`, `tests/delete_model.rs` and `tests/support/fixtures.rs`):
- fmt ok, whole-tree clippy clean;
- every suite passes in **5 debug runs** (delete_model 22);
- release build ok;
- `TARGET: delete_check_ms_per_1M=18.2 (target 50)` ✓.

**But in release mode, `hard_links_across_the_deleted_subtree_still_match_du` fails 8 of 40 runs** ("no run deleted the charged link"). Debug: 0 of 20.

**Pass 1, spec:** the model side of M7 v0.5 and D1 is met, with the two code findings below.
- **Refusals:** root, `OTHER_FS`, inside a running delete, a second delete.
- **`delete_check`:** counts inside the target, the 3 largest children, `outside_links`, and `freed_bytes_upper`. `kept ≤ size`, so the subtraction can't underflow.
- **`begin_delete`:** tombstones the target; request totals include the target, which matches the executor's counts, and are only set when exact.
- **While tombstoned:** issues are discarded, and the target's `Cancelled` is not counted as a skip.
- **`Removed`:** goes through T012's `remove_subtree`, including the hard-link re-charge.
- **`Partial`/`Aborted`:** clear the tombstone on the whole subtree and return `Rescan(id)`.
- Late messages are ignored.
- The T012 carry-over test (a folder replaced by a file, rescanned, then the delete retried with the real executor) passes.

The mutations reported by engineer-1 are noted: counting a delete's `Cancelled` as a skip, and not clearing the tombstone on `Aborted`.

## Required changes
1. **Make the hard-link test deterministic.** It retries up to 50 times, hoping the parallel scan charges the link inside `a`. In release builds the race almost always goes the other way.
   - **Fix:** after each scan, *look up* which directory holds the charged link (`!HARDLINK_DUP`) and delete **that** one. That covers the re-charge path every time. Then, in a second fresh fixture, delete the directory holding the **duplicate**.
   - Drop the retry loop.
   - **Prove it:** 40 release runs, 0 failures (`for i in $(seq 40); do cargo test --release --test delete_model || break; done`).
2. **`apply_delete` when the target no longer exists.** If something removed the target while it was being deleted (for example a rescan of an ancestor through `r`), then `Partial`/`Aborted` returns `Rescan(target)` for a node that no longer exists, and the app's `begin_rescan` would index a removed node.
   - **Fix:** return `Reconcile::None` when `!self.tree.exists(target)`, for every outcome.
   - **Test (red first):** `begin_delete(x)`, then `begin_rescan(parent of x)`, then `apply_delete(Finished(Aborted))` → `Reconcile::None`, and nothing panics.
3. **The `Model::issues()` doc comment** was the first item of this brief (from the T012 re-review) and is still missing. Add: "insertion order; entries may be removed by rescans; a node has at most one issue, so key UI rows by `ScanIssue::node`, not by index." (The `remove_issues_of` comment is fixed ✓.)

## Accepted
- **Aborted test forces the identity failure with `expect_ino + 1`** instead of swapping directories. It is deterministic, and the swap scenario is already covered by T010's executor tests.
- **Counts:** `delete_check` counts what is inside the target, while the request totals include the target. The dialog shows "N files · M folders" inside it, and the executor's counter includes the target.
- **`du --apparent-size` on tmpfs ignores directory sizes:** T007 must check apparent-size parity on btrfs as well (it's already in its brief; noted again there).

## Question for the architect (non-blocking; raised via the team lead)
Design §3.8 says delete failures "are added to the issue list". `record_failures` does add them, but the D1 rescan that follows a `Partial` removes the target's subtree, and its issues with it. So the Skipped list loses them. Only the result dialog (T021, which must be dismissed explicitly, with `v` listing the paths) keeps them.
- **Option A:** accept that the result dialog is the "never silent" channel, and amend §3.8.
- **Option B:** keep a separate session list of delete failures that survives rescans, shown in Skipped.

Until decided, T021 builds the dialog from `Outcome` (it already does).
