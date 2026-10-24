# T012 review (Heimdall, 2026-10-23): CHANGES REQUESTED (one small item)

**Fresh evidence** (clean worktree: HEAD plus `src/model/{hardlink,index,mod,control}.rs`, `tests/model_controls.rs` and `tests/model_indexes.rs`):
- fmt ok, whole-tree clippy clean;
- model_controls 23, lib 105, model_indexes 8, and every other suite pass in 3 consecutive runs;
- release build ok.

All 23 were reported red first.

**Pass 1, spec:** met.
- `mark_skipped`.
- `begin_rescan` through one `remove_subtree(id, keep_root)`, which T011 reuses: sizes, counts, ext totals, top-N, hard-link re-charge, issues.
- `top_level_progress` across skips and rescans.
- `Identity`: dir→file, dir→symlink, replaced dir, file size/inode change, vanished rescan root (`Vanished` elsewhere stays an issue).
- `set_focus` / `take_focus_completed`.
- `rescan_plan` with `Full`, `PrioritiseAndResume` (outermost skips only) and `NoneWhileScanning`; Stopped/Done gives `Full`.
- The integration tests with `FakeFs` + the real walker, including the O7 "every dir listed once" check.

**Required change: issue removal breaks a UI contract.**
- `remove_issue` uses `swap_remove`, so `issues()` order changes after a rescan.
- **More important:** `src/app/list_state.rs:23` (T018) keys the Skipped list's rows **by index into `issues()`**, documented as "stable: issues are only appended". Once issues can be removed, any index-based key points at the wrong issue after a rescan, whatever the removal method.

**Decision (Heimdall; recorded as plan D12):**
- **Model (T012, now):** `issues()` keeps **insertion order**, and entries can be removed. Remove with an order-preserving `retain`, then **one** re-index pass of `tree.issue` per `remove_subtree`/vanish. That is O(issues) per rescan, which is rare, instead of O(issues) per removed issue. Update the doc comment on `Model::issues()`: "insertion order; entries may be removed by rescans; a node has at most one issue, so key UI rows by `ScanIssue::node`, not by index."
  - **Test, red first:** with 3 issues A, B, C (in that order), rescan the subtree holding B → `issues()` is `[A, C]` in that order, and `issue(C.node)` still resolves to C.
- **UI (T020, engineer-2):** re-key `RowKey::Issue` by `NodeId` instead of by index. Added to the T020 brief. No path removes an issue at runtime before T020/T021 wire rescans, so this order is safe.

**Choices accepted:**
- `Identity` is applied only for nodes `begin_rescan` registered. Late or stray `Identity` messages are ignored.
- `rescan_plan` treats Starting like Scanning.
- `set_focus` on an already-complete folder stores nothing, so only real transitions are reported.
- **Out-of-list edits, all necessary:**
  - `HardLinks::remove` in `hardlink.rs` and `Indexes::remove` in `index.rs`;
  - removal of the folder branch of `category_of_node` and of the `dir_categories` cache, as the T026 note asked (`category_of_node` is now file-only and still used);
  - in `tests/model_indexes.rs`, the three folder-category tests replaced by one file-category test.
- **Deferred:** the "delete retry succeeds after a dir became a file" test goes to T011, because it needs `begin_delete`. Agreed; it is already listed in T012 criterion 4, so carry it into T011.
