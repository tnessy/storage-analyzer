# T005 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence:** I ran the full set myself, because the engineer only re-ran part of it after the last lint change. Clean worktree, HEAD plus the T005 files only:
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: lib 87, model_apply 17, model_indexes 10, scan_walker 22, app_loop 18, delete_executor 17 + 3 ignored, guards;
- release `model_indexes` passes; doc tests ok, release build ok.

The red-first evidence is reported: 10 index tests, the `kind_from_raw_mode` test, and the root-path test (`/data/x/` vs `/data/x`).

**Pass 1, spec:**
- `ext_of` follows every rule;
- the category table is one data table, `NoExt` → Binary;
- live ext and category totals, with `category_totals` as sums of per-ext totals;
- `top_files` for both modes, sorted desc with ties by smaller NodeId;
- `recompute_top`;
- the `path(root)` fix;
- the shared `Kind::from_raw_mode`, with both private copies removed.

The main test checks the brute-force equality after **every** message on random streams, at N = 1, 3 and 100. That is exactly the live M5/M6 requirement.

**Decisions ruled:**
1. **Ext totals and top lists skip hard-link duplicates, stat-issue entries and other-filesystem entries:** accepted. That is consistent with the root totals.
2. **Top lists hold regular files only; ext totals also count symlinks and other non-dirs:** accepted. M6 is "largest files", and M5 counts every non-directory entry.
3. **Dir category = the category with the most disk bytes among its children:** accepted. Ties go to the lower category, and an empty dir is AudioOther. Cost: the cache is keyed by the dir's disk size, so a call recomputes only dirs whose size changed since their last call. The work is proportional to recent change, not to the tree. That fits 2 Hz relayout of ≤ 60 tiles. T009's harness should log the time per relayout's `category_of_node` calls (added to T009).
4. **`recompute_top` does not touch ext totals:** accepted. T011 and T012 must subtract ext totals themselves (noted in their briefs).
5. **`Category`/`ExtKey` derive `Ord + Hash`:** accepted as a contract addition.

**Non-blocking notes:**
- `dir_categories` is a `Mutex` used only for interior mutability on the single-owner model. A `RefCell` would do, but the uncontended Mutex is harmless.
- The cache is never pruned for removed nodes. That is bounded by the dirs queried. Prune it in T011/T012 when nodes become `REMOVED`.
