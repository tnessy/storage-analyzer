# T003 review (Heimdall, 2026-10-23): CHANGES REQUESTED

**Fresh evidence** (clean worktree: HEAD plus the T003 and T004 files):
- fmt ok, clippy clean;
- `cargo test --all-targets`: 49 lib + 14 model_apply + 4 + 7 pass.

The red-first evidence is reported (14 failing at `todo!`).

**Pass 1, spec:**
- **Missing: criterion 11, `top_level_progress() -> (u32, u32)`.** It was added to the brief by the ADR-010 amendment, and T019 depends on it. Add it as specified:
  - O(1) query, maintained incrementally;
  - `total` = descended dir children of the root;
  - `done` = how many of them are complete;
  - tests: `done` increments exactly when each top-level dir completes, and stays `< total` after `Done { cancelled: true }` with unfinished top-level dirs. Also check that an `OTHER_FS` child is excluded from `total`.
- Everything else matches the criteria: per-listing batching, pending/complete/exact semantics, hard-link charging, `OTHER_FS` handling, and raw-byte paths. Only owned files were touched.

**Pass 2, quality:**
1. `mod.rs` `finish_dir` and `queue`-style counters: `pending_dirs` uses `saturating_add(...).saturating_sub(1)`. That **hides accounting bugs**: an underflow would silently read 0 and mark a dir complete too early. Use plain arithmetic with `debug_assert!(pending >= 1)`, or checked arithmetic that panics in debug builds. Apply the same to `inexact_dirs` where it decrements later. The proptest then catches drift.
2. Non-blocking, memory: by my reading the struct-of-arrays layout is roughly 150 B/node before name bytes and `Vec` growth slack. The main contributors are: every node carries the dir-only fields (`agg_*`, `files`, `dirs`, `pending`, `inexact`, `children` = 24 B), `NameRef` is 16 B after padding, and `dev` is u64 where the design has `dev_idx: u16`. **No change now.** T008 measures it. Its report must break bytes down per field so we can decide on a file/dir split with data.
3. Non-blocking: `finish_dir` and `apply_cancelled` collect ancestors into a `Vec` per message. Walking the parent array in place would avoid an allocation on the hot path. Optional.

**Ignored-message decisions accepted.** The documented rule that a rescan must clear `DONE` is already in T012. Panicking accessors on never-allocated ids are accepted (callers use model-issued ids). Commit `tests/model_apply.proptest-regressions` as a seed.

## Re-review 2026-10-23: APPROVED, committed
- **Fresh run** (clean worktree, T003 + T004 files only): fmt ok, clippy clean; lib 51, model_apply 16, guard tests pass. The release `model_apply` run passes too.
- **Fixes verified in code:**
  - `top_level_progress()` returns stored counters, so it is O(1);
  - `top_done` is bumped in `finish_dir` on the incomplete→complete edge of the top-level ancestor;
  - `pending_dirs` uses plain arithmetic with `debug_assert!`, and the debug-build proptest exercises it;
  - `finish_dir` no longer allocates.
- The red-first evidence for the new tests is reported.
- The deleted `todo!`-phase proptest seed is fine: a seed from a stub panic has no value.
