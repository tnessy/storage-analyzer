# T009 (spikes R2/R4/R8) review (Heimdall, 2026-10-23): APPROVED, committed
**Fresh evidence** (clean worktree: HEAD plus `examples/scan_harness.rs`; no production code):
- fmt ok, whole-tree clippy clean;
- the whole suite passes with `--no-fail-fast`;
- `cargo test --example scan_harness` passes 2; red first is reported;
- release build ok.

**My run:** `scan_harness /usr --threads 4` (warm): 347,377 entries in 0.10 s, apply at 131 % of the walker rate. The totals are **byte-exact** against `du -sx -B1` (17,924,702,208) and `--apparent-size` (17,111,121,743).

**Accepted findings** (warm cache, noisy machine, load 13–55; numbers are indicative):

| Area | Finding |
|---|---|
| R2 throughput | ~2.7 M entries/s single-thread apply on `$HOME` (19.85 M entries). Apply keeps up at 4 threads (82 %, target ≥ 70) and falls behind at 8 (48 %) and 16 (39 %). Wall time is flat from 4 to 16 threads (8.2 / 7.3 / 8.6 s). |
| Thread default | Engineer-1 recommends measuring **cold** at 4 vs 8 before changing `min(nproc, 8)`. That needs a cold-cache run with `sudo drop_caches`, **by the user**; the harness only prints the command. |
| Apply ticks | median 0.01 ms, **but a few 118–147 ms ticks per run**, against the 50 ms input target. → T030 |
| `recompute_top` | 0.43–0.61 s at 20 M entries. It runs once per delete `Removed`, so it is a visible pause after a large delete. Noted under T030. |
| R8 frontier | ~504 k pending dirs at peak; re-key 32.7 ms at 500 k; a focused folder completes 56–224 ms after focus. |
| R4 btrfs | byte-exact against du on `/usr` (zstd), `~/xiv`, and `/` with `-x` (24 other-fs mounts/subvolumes skipped exactly like du). `$HOME` differs by −0.0001 % (a live tree). |
| Amendments | worker CPU while the channel is full: 1.3 % of a core at 4 threads and 2.7 % at 16, **so no backoff is needed**. `category_of_node` p95 0.06 ms. Tree row build: 5.3 ms at 100 k children, **83.8 ms at 1 M**. → T032 |
| RSS anomaly | **4.3 GiB with ≥ 2 walker threads vs 2.4 GiB with 1**, same data. Not explained by the channel or glibc arenas. → T031 |

**Follow-ups:** written as **proposed** tasks T030–T032. They concern non-gating targets (plan §1), except that T030 bears on M1 responsiveness. I recommend all three. Owner/user decision via the team lead, with the architect consulted on T030 and T031.
