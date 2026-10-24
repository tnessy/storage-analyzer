# T027: Rescan fixes: top-level progress underflow; never rescan another filesystem

- **Engineer:** engineer-1.
- **Schedule:** **now, before T007.**
- **Depends on:** T012, T011, T020 (all merged).
- **Source:** T020 review. engineer-2 found the underflow; Heimdall reproduced it and found the other-filesystem gap.

## Bug 1: `top_done` underflow (reproduced by Heimdall)
- **Repro:** a top-level directory whose **entry** carries an issue (for example `RawEntry.issue = Some((Stat, PermissionDenied))` in the root's listing). It is complete, never pending, and not counted in `top_total`/`top_done`. `begin_rescan` on it panics in debug at `src/model/control.rs:271` with "attempt to subtract with overflow". A top-level dir with a `DirIssue` does **not** trigger it.
- **Reach:** `r` on that row in the Tree or the Skipped list (wired by T020).
- **Fix:** keep `top_total`/`top_done` consistent when a rescan turns a not-counted top-level entry into a pending one. Either count it into `top_total` when it becomes a descended dir, or base the adjustment on whether the node was counted. A rescan root that ends up descended is counted from then on.
- **Tests, red first:**
  - the repro above, with `begin_rescan` then a fresh `Listing` → no panic, and `top_level_progress()` ends `(n, n)`, consistent with an equivalent clean scan;
  - the same for a top-level entry with an `OtherFilesystem` issue (see bug 2: it must be refused, so test `rescan_plan`);
  - **property test:** random streams of listings, entry issues, skips and rescans of top-level dirs keep `top_done ≤ top_total` and never panic.

## Bug 2: `r` can rescan a directory on another filesystem (ADR-008 violation)
`rescan_plan` returns `Full` for an `OTHER_FS` node. The walker's rescan path (`refresh_identity`) then lists it, because it doesn't check `one_filesystem`. With the default one-filesystem mode, totals would include another filesystem, so they would no longer match `du -x` (M8).
- **Model:** add `RescanPlan::OtherFilesystem` (contract addition, authorized). `rescan_plan(id)` returns it for `OTHER_FS` nodes.
- **Walker (defence in depth):** in a rescan job with `one_filesystem`, if the re-stat'ed target's `dev` ≠ `root_dev`, send `Identity` with `issue = (Stat, OtherFilesystem)` and **do not list**. The model then keeps or sets `OTHER_FS`. Apply the same rule wherever a rescan root could have changed device, for example after a mount appeared.
- **Tests, red first:**
  - `rescan_plan(other_fs_node) == OtherFilesystem`;
  - the walker with `FakeFs`: a rescan job whose target now has another `dev` sends `Identity` with the `OtherFilesystem` issue and no `Listing`; the model keeps the totals unchanged.

**App wiring for bug 2** (the message on `r`) is a one-line follow-up for engineer-2 once UX supplies the copy (asked via the team lead). Until then, the app treats `OtherFilesystem` like `NoneWhileScanning` with no message. **Add that match arm yourself**, so the crate compiles, with a `// T020b: message pending UX` comment. That is the only `src/app` edit allowed in this task.

## Files owned
`src/model/control.rs`, `src/model/mod.rs`, `src/scan/walker.rs`, `tests/model_controls.rs`, `tests/scan_walker.rs`, plus the one match arm in `src/app/controls.rs` described above.

## Commands
The five standard ones.
