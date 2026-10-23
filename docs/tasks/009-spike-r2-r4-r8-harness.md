# T009: Spike R2/R4/R8, headless scan harness

- **Engineer:** engineer-1. **Wave:** 2. **Depends on:** T005, T006.

## Goal
Measure on the user's real data:
- **R2:** whether single-threaded apply keeps up with the walker, and what top-N recompute costs;
- **R4:** btrfs `du` parity, subvolume `st_dev` and `-x` semantics;
- **R8:** shallow-first frontier size and re-key cost.

Also measure thread-count alternatives for the `min(nproc, 8)` default. Every number is a **target**, not a gate.

Acceptance criteria:
1. `examples/scan_harness.rs` (`cargo run --release --example scan_harness -- ROOT [--threads N] [--cross-filesystems] [--focus-at-ms T PATH]`) runs `RealFs` → walker → `Model` without a terminal. The main thread applies with the same 8 ms budget loop the app will use.
2. It reports, once per second and at the end:
   - entries/s produced and applied;
   - channel occupancy;
   - p50/p95/max per-tick apply time;
   - `max_pending()` and `last_rekey_ms()` from the queue;
   - total wall time;
   - RSS;
   - `recompute_top` time on the final model.
3. It runs on `/usr` and `$HOME` with threads 1, 2, 4, 8 and 16. Run each one cold (`--drop-caches-hint` only prints the command; never run `sudo` from the harness) and warm.
4. **R4:** on `$HOME`:
   - compare harness totals to `du -s -B1`, `du -sx -B1` and `du -s -B1 --apparent-size`, and record them;
   - list the `OtherFilesystem` issues found (nested subvolumes, mounts);
   - note any compressed-file discrepancy (zstd on btrfs).
5. **R8:** with `--focus-at-ms 2000 <some big dir>`, record the re-key time and how fast the focused subtree completes compared with an unfocused run.
6. `spikes/R2-R4-R8-REPORT.md` holds the tables, the `TARGET:` lines (apply ≥ 70 % of walker rate per ADR-003, thread default), and a recommendation. **No production code changes in this task.**

## Files owned
`examples/scan_harness.rs`, `spikes/R2-R4-R8-REPORT.md`.

## Tests to write first
A smoke test inside the example (`#[cfg(test)]`) runs the harness loop on a `tempfile` fixture with 1,000 entries and asserts it reaches Done with entries applied equal to entries produced. Command: `cargo test --example scan_harness`.

## Out of scope
Tuning or fixing what it finds. Findings go to Heimdall, who routes them to the architect.

## Amendment (2026-10-23, from the T006 review)
Also report **worker CPU time while the channel is full** (`/proc/self/task/*/stat` utime + stime for the `scan-*` threads during a deliberately slow apply). The walker polls with `try_send` every 500 µs. If the CPU cost is material, recommend a backoff.
