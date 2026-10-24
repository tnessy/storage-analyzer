# T030c: T030 follow-ups (engineer-1; small)

- **Schedule:** after T030b merges, which is engineer-2's wiring.
- **Depends on:** T030, T030b.

1. **R2-1, btrfs fixture sync:** `sync_fs()` exists and the 10,000-file test calls it. **Call it in `m8_tree` too**, and in any other fixture used with `SA_TEST_BTRFS_DIR`. **Prove it:** 200 runs with `SA_TEST_BTRFS_DIR`, 0 failures. Reason: btrfs delayed allocation can shift `st_blocks` by a block between the scan and `du`; this was reproduced and fixed by `sync` in the T030 review.
2. **R2-5, remove the switch:** delete `set_incremental_top` and the one-shot path, so incremental top-N is the only behaviour. Update T030b's startup call accordingly. That is the only `src/app` edit, and it is coordinated with Heimdall.
3. ~~Incremental `remove_subtree`~~ **Architect: accepted as is for v1** (one ~80 ms step per confirmed delete). Incremental removal is **backlog**; revisit it if a real delete measures over 250 ms.
4. **Harness report:** log `~/workspace`'s entry count next to its 80 ms removal cost (the cost grows with folder size) in `spikes/R2-R4-R8-REPORT.md`.

**Files:** `tests/scan_du_parity.rs`, `tests/support/fixtures.rs`, `src/model/mod.rs`, `src/model/index.rs`, `src/model/control.rs`, `src/model/delete.rs`, plus the one app call.
