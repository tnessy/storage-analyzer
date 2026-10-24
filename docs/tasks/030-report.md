# T030 report (engineer-1)

**Status:** DONE, with the app/UI wiring of part 4 left open (see below).

Result on $HOME (19.87 M entries): `TARGET: apply_tick_max_ms=9.2 (target 50)`, was 118-147 ms; no tick over 20 ms in most runs. Table and diagnosis: `spikes/R2-R4-R8-REPORT.md`, section "T030".

## What was done
1. **Diagnosis** (`examples/scan_harness.rs --diag-ticks`, `Model::node_capacity()` `#[doc(hidden)]`): every slow tick was one ordinary message (<= 260 entries) with 20 k-100 k minor page faults. Causes: array regrowth (A) and, found by elimination and confirmed by the fix, the rehash of the hard-link map. Big listings (B) were not the cause on this data.
2. **Listing parts** (contract change `ScanMsg::Listing { dir, first_id, entries, last }`, `scan::msg::LISTING_PART_ENTRIES = 8192`): the walker sends parts with contiguous ids and queues child jobs after the last; the model adds every part but only `last` completes the folder; stray parts after `last` are ignored. Mechanical `last: true` edits in: `src/model/mod.rs`, `src/scan/walker.rs`, `examples/scan_harness.rs`, `tests/{model_apply,model_controls,model_indexes,delete_model,scan_walker}.rs`, `tests/support/synthetic.rs`, `spikes/r1-memory/src/lib.rs`.
3. **Chunked storage**: `Chunked<T>` (64 k-element blocks) for the 17 per-node arrays in `src/model/tree.rs`; 1 MiB blocks for names in `src/model/names.rs` (a name never straddles blocks).
4. **Sharded hard-link table** (`src/model/hardlink.rs`, not in the brief's file list; found by the diagnosis): 256 small maps instead of one.
5. **Resumable top-N rebuild** in `src/model/{mod,index}.rs`: `start_top_rebuild`, `step_top_rebuild(budget)`, `top_updating`, `top_epoch`, `set_incremental_top`.

## Tests, failing first
- Walker: 20,000-entry folder -> 3 parts with contiguous ids, only the last marked, children listed after it (failed with one part); small and empty folders send one part marked last.
- Model: only the last part completes (failed: the folder completed after part 1); stray part ignored; stopped scan leaves it unfinished.
- du parity: a 10,000-file folder equals `du` on tmpfs and btrfs.
- Chunked storage: names do not move when the arena grows (failed with the Vec-backed arena); `Chunked` pointer stability and block capacity (mutation check: a single growing Vec fails both); `node_capacity` grows in whole 64 k blocks.
- Hard-link table: no table holds more than 800 of 100,000 links (failed: one table of 100,000).
- Top rebuild: same lists as the blocking recompute, in at least 40 steps for 50,000 files; files listed and nodes removed during a rebuild are handled; default stays blocking. All failed against `todo!()` stubs first.

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` pass; doc ok; release build ok; harness smoke tests and `spikes/r1-memory` tests pass.

## Open: app and UI wiring for part 4 (engineer-2 / the lead)
Nothing changes for the user until the app does three things: (1) call `model.set_incremental_top(true)` once; (2) each loop iteration, if `model.top_updating()`, call `model.step_top_rebuild(Duration::from_millis(5))`; (3) the Largest view shows `updating…` (UX wording) while `top_updating()` and includes `top_epoch()` in the stamp that decides when the list is re-read. Until then deletes and rescans keep recomputing the lists at once (up to 0.5 s on 20 M entries).

## Part 4 review fixes (architect, 2026-10-23)
- **Reserve and exactness** (`src/model/index.rs`): each list keeps `N + 4N` entries and a `bound`, the largest key it ever evicted. Only entries above `bound` count as exact, so a small file cannot fill a freed slot while bigger evicted files exist outside the list. Removed nodes are taken out of the heaps (`remove_subtree`) and so are files whose size changes (`apply_identity`); a rebuild starts only when a list can no longer show its top N although more files exist (`needs_rebuild`). A finished rebuild takes the scratch list's own `bound`.
- **Root rescan** (`begin_rescan(ROOT)`): the lists are reset and any running rebuild is cancelled; the new scan refills them.
- **Bug found by the new property test** (shrunk seed saved in `tests/top_reserve.proptest-regressions`): a delete during a running rebuild can empty the scratch list's exact part (its `bound` came from files that were then deleted). After swapping in the rebuilt lists, `step_top_rebuild` now checks `needs_rebuild` and starts a clean second pass instead of returning "done" with empty lists.
- **Tests** (`tests/top_reserve.rs`, red first: 4 of the 7 failed): a delete with no listed file does not rebuild; the reserve absorbs a delete of part of the top; losing more than the reserve rebuilds and ends correct; a tiny file never takes a freed slot; root rescan clears without a rebuild and cancels a running one; **property test** (300 cases by default, 4,000 run once): random trees, listings applied out of id order, deletes, folder and root rescans during a rebuild, both size modes; whenever no rebuild runs the lists are exact, and after the rebuild they equal a recount.
- **Criterion 5 on $HOME** (`--delete-top`, 19.9 M entries, rebuild done in 5 ms steps): deleting `~/xiv` (272 GB) needs **no rebuild** (5 ms in total for the removal), `~/workspace` (316 GB) needs none either (80 ms, all of it the removal itself in one step), `~/Downloads` 1 ms. The worst case, a delete that needs a full rebuild, is the full pass measured before: 80-181 steps of at most 5 ms (about 0.4-0.9 s of work in total, 8.3 ms longest step on a loaded machine).
- **Finding:** removing a very large folder from the model (`remove_subtree`, e.g. `~/workspace`) is one 80 ms step, above the 50 ms input target. It happens once per confirmed delete; stepping it would be a further change.
- Still open: the lead has to rule on the `src/model/hardlink.rs` change; the app/UI wiring (above).

## Round 2 (lead's review `030-review.md`), final state
- **R2-1:** `fixtures::sync_fs` (`sync -f DIR`) runs at the end of `m8_tree` and before the scan in the 10,000-file parity test. 200 runs of the whole `scan_du_parity` binary with `SA_TEST_BTRFS_DIR`: **0 failures** (it was about 1 in 60).
- **R2-2 / R2-3:** reserve and bound as described above; API names kept: `top_updating`, `top_epoch`, `step_top_rebuild`, `start_top_rebuild`, `set_incremental_top`. Design §3.4 should say `top_updating` (the architect agreed to either).
- **R2-4:** criterion 2 = `deleting_a_folder_with_no_listed_file_does_not_rebuild` (0 steps) and `the_reserve_absorbs_...`; criterion 6 = `after_a_rebuild_the_reserve_is_back_and_later_deletes_within_it_do_not_rebuild` (after a rebuild a 5-entry delete costs nothing, using up the 20-entry reserve rebuilds). Mutation check: with the reserve factor set to 0 two of these fail. Criterion 3 = the property test `the_lists_always_match_a_recount`: its random operations include a delete while a rebuild is running (also a second one), a folder rescan and a root rescan while a rebuild is running, listings in random id order, both size modes.
- **R2-5:** `set_incremental_top` is still a switch (default off) because the app is not wired yet; once T030b calls it, drop the switch and make stepping the only behaviour.
- **R2-8:** the harness smoke test no longer asserts on the tick count; 200 runs, 0 failures.
- Fresh: fmt ok; clippy -D warnings clean; `cargo test --all-targets` all pass (28 test binaries); doc ok; release build ok.
