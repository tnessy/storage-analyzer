# T030c review (Heimdall, 2026-10-23): APPROVED, committed together with T031
- **Switch removed:** `set_incremental_top` and its field are gone (grep: none left in `src`, `tests` or `examples`). The stepped rebuild is now the only path; `recompute_top()` stays as a blocking convenience for tests and the harness. The new test `a_rescan_that_empties_the_lists_rebuilds_them_in_steps` was red while the switch defaulted to off.
- **R2-1:** `sync_fs` in `m8_tree` is in. btrfs parity: 200/200 (engineer-1) and 60/60 (Heimdall).
- **Removal cost vs size** (harness `--delete-top`, `$HOME`):

  | Folder | Entries | Removal |
  |---|---|---|
  | `~/workspace` | 602,376 | 80–101 ms |
  | `~/xiv` | 37,472 | 2.6 ms |

  That is ≈ 0.17 µs per entry, so **a single folder of ~1.5 M entries would cross the architect's 250 ms revisit threshold** for incremental removal (plan O9). Reported to the architect.
