# T031 review (Heimdall, 2026-10-23): APPROVED, committed together with T030c (shared `examples/scan_harness.rs`; one hash-checked snapshot)
**Diagnosis: no allocator problem; the memory budget is corrected to ~4.0 GiB.**

**Fresh evidence** (clean worktree, combined T031 + T030c snapshot):
- fmt ok, whole-tree clippy clean;
- every target passes;
- release build ok;
- top_reserve, ui_lists, app_delete and model_indexes 40/40 each;
- **btrfs parity 60/60** (`sync_fs` now also in `m8_tree`; T031's earlier run gave 100/100);
- harness smoke 100/100.

`mimalloc` is example-only. **My own measurement:** `scan_harness $HOME --threads 4 --malloc-report` gave **19,937,403 entries, RSS 4,061 MiB, and glibc in use 4,001 MiB with 73 MiB free**. That is live data, not fragmentation, and it matches engineer-1's table (4.0–4.1 GiB for glibc at 1/4/8 threads and for mimalloc; `malloc_trim` frees ~1 %).

**Explanation accepted:**
- **Names:** ~210 B/entry on real data against 153 B in the R1 spike, because names average 52 B here against 19 B in the spike's histogram, which came from the first 300 k names of a breadth-first walk.
- **Hard links:** 42 % of entries are duplicate hard links (~470 MiB in the table), against 0.5 % in the spike.
- **The earlier 2.4 GiB figure** is most likely RSS lowered by swapping (unproven).

**Consequences:**
- **Correct the budget to ~4.0 GiB for ~20 M entries** (~7 % of 60 GiB). The owner's "acceptable for v1" was based on 2.85–2.9 GiB, **so it needs re-confirming (plan O10).** The architect should update design §5 "Process memory".
- Possible reductions are backlog: a dir-table split (~54 B/entry), a compact hard-link table, shorter or shared names.
