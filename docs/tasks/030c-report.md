# T030c report (engineer-1)

**Status:** DONE.

1. **Incremental switch removed.** `Model::set_incremental_top` and its field are gone; a delete or rescan that leaves a list unable to show its top N now always starts the stepped rebuild (`refresh_top`). Callers updated: `src/app/event_loop.rs` (the one call), `tests/ui_lists.rs`, `tests/top_reserve.rs`, `tests/model_indexes.rs`, the harness. `recompute_top()` (blocking) stays as a public convenience used by tests and the harness. Test first: `a_rescan_that_empties_the_lists_rebuilds_them_in_steps` replaces the "only when asked" test and failed while the switch was still off by default.
2. **`m8_tree` sync:** `sync_fs(&root)` is in `tests/support/fixtures.rs` (uncommitted when this report was written); 200 runs of `scan_du_parity` with `SA_TEST_BTRFS_DIR`: 0 failures.
3. **Removal cost with entry count** (harness `--delete-top`, `$HOME` 19.9 M entries):
   | folder | entries | removal from the model (one step) | top-N rebuild |
   |---|---:|---:|---|
   | `~/workspace` | 602,376 | 101 ms (80-101 ms across runs) | not needed |
   | `~/xiv` | 37,472 | 2.6 ms | not needed |

   That is about 0.17 us per entry, so a folder reaches the architect's 250 ms threshold at roughly 1.5 M entries. Printed as `TARGET: delete_removal_ms=... for N entries`.

Fresh: fmt ok; clippy -D warnings clean; `cargo test --all-targets` all 30 binaries pass; doc ok; release build ok.
