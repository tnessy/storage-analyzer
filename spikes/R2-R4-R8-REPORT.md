# R2 / R4 / R8: scan throughput, btrfs parity, frontier and focus (T009)

Measured 2026-10-23 with `examples/scan_harness.rs` (release build; production code unchanged). Machine: 16 cores, NVMe, btrfs (`compress=zstd:1`), shared with other jobs (load average 13-55 while measuring), so absolute numbers are noisy: expect +-15 % between repeats (wall time of one $HOME scan varied 7.3 s to 20 s across repeats). All runs are **warm cache**: a cold run needs `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches`, which the harness only prints and I did not run. Cold numbers (where more threads should help) are therefore **not measured**. $HOME: 19.85 M entries, 2.67 TB.

## How to run
```
cargo run --release --example scan_harness -- ROOT [--threads N] [--cross-filesystems] [--du]
        [--focus-at-ms T PATH [--watch-only]] [--slow-apply-ms N]
cargo run --release --example scan_harness -- --bench-rows          # tree rows, 100 k / 1 M children
cargo run --release --example scan_harness -- --bench-queue [N]     # queue re-key cost
cargo test --example scan_harness                                    # smoke tests
```
Per run: a warm-up walker pass, a walker-only pass (messages counted and dropped = what the walker can do alone), then the real pass (8 ms apply budget per loop iteration, bounded channel of 1024, `Model::apply` on the main thread).

## R2: does single-thread apply keep up? ($HOME, warm)
| threads | walker alone | wall | entries/s end to end | pure apply rate | apply / walker | tick p95 | tick max | ticks at the 8 ms budget | RSS |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 0.27 M/s | 98.4 s | 0.20 M | 2.28 M/s | 848 % | 0.34 ms | 143 ms | 12 / 829 901 | 2379 MiB |
| 2 | 1.79 M/s | 14.6 s | 1.36 M | 2.58 M/s | 144 % | 0.08 ms | 122 ms | 51 / 441 612 | 4069 MiB |
| 4 | 3.35 M/s | 8.2 s | 2.41 M | 2.74 M/s | **82 %** | 0.04 ms | 118 ms | 688 / 130 857 | 4310 MiB |
| 8 | 5.77 M/s | 7.3 s | 2.71 M | 2.78 M/s | **48 %** | 8.00 ms | 127 ms | 816 / 13 202 | 4308 MiB |
| 16 | 6.74 M/s | 8.6 s | 2.32 M | 2.65 M/s | **39 %** | 0.13 ms | 147 ms | 751 / 57 123 | 4275 MiB |

`/usr` (347 k entries, warm; much shorter runs, so noisier): apply/walker 245 % (2 threads), 129 % (4), 76 % (8), 73 % (16).

```
TARGET: apply_vs_walker_rate_percent=82 (4 threads), 48 (8 threads), 39 (16 threads); target >= 70
TARGET: model_apply_entries_per_s=2.7e6 on $HOME (4.5e6 on /usr), single core
```
Reading:
- One main-thread apply handles about 2.7 M entries/s on this data. The walker reaches 5.8-6.7 M entries/s with 8-16 threads, so from 8 threads on **apply is the bottleneck** and the bounded channel throttles the workers (ADR-003 working as designed). The 70 % target holds up to 4 threads.
- Wall time is flat from 4 to 16 threads (8.2 / 7.3 / 8.6 s): more than about 4-8 threads buys nothing warm.
- **Recommendation:** keep `min(nproc, 8)` only if a cold run (not measured) shows I/O-bound scans benefiting; warm, 4 gives within 12 % of 8. Suggest measuring cold once with 4 / 8 before changing the default. A single apply thread is enough for the 20 M-entry data set (about 7-8 s of pure apply time).
- **Input latency (R2 vs the 50 ms target): not met in the tail.** The median tick is 0.01 ms and p95 under 0.4 ms, but every run has ticks of 118-147 ms, a few per scan. The 8 ms budget is only checked between messages, so one expensive `apply` stalls the loop. My hypothesis (not verified): the occasional `Vec::resize` of the 17 per-node arrays when they double (touching hundreds of MB of fresh pages), or a listing of a very large directory. Worth a follow-up (chunked arrays, or reserve ahead in steps) because the tick max is 3x the input-latency target.
- `recompute_top` on the final 19.85 M-entry model: **0.43-0.61 s** (O(files); T011 calls it after every delete, so a delete near the end of a big scan blocks the main thread for half a second: do it off-thread or incrementally if that matters).

## R8: frontier, re-key, focus
- **Frontier** (max discovered-but-unlisted directories, the queue's pending jobs; the harness reads it as `pending_dirs` of the model): **about 504 k** at 1 thread, 507 k at 16 threads, peaking around 2-4 s into the scan, on a tree of 2.0 M directories. 3.4-3.7 k on /usr. Far below the 1 M cap.
- **Re-key cost** (`--bench-queue 500000`, 500 k pending jobs, focus change then the next pop): **32.7 ms** (`last_rekey_ms`); clearing the focus 2.5 ms (nothing queued is in focus). The re-key runs under the queue lock, so all workers stall for that long, once per focus change.
```
TARGET: queue_rekey_ms_at_500000=32.7
```
- **Focus effect** (4 threads, focus set at 2.0 s on a folder in $HOME, versus the same run with `--watch-only`):
  | folder | completed after focus (focused) | completed (unfocused) |
  |---|---:|---:|
  | `~/workspace` (316 GB) | 224 ms (twice) | 15.4 s after the focus point in one repeat, never before the end in another |
  | `~/xiv` (272 GB, 37 k entries) | 56-61 ms | about 6.1 s |

  So prioritising works: the focused folder finishes in well under a second while the unfocused scan takes the rest of the scan. Total scan time was not affected by focusing (8.3 s focused vs 8.1-8.6 s); one focused repeat took 20 s and one unfocused 17 s, both explained by machine load.

## R4: btrfs parity
Totals of the harness (one-filesystem mode) against `LC_ALL=C du` in bytes (disk = `-B1`, apparent = `--apparent-size`):
| tree | harness disk | du -sx | difference | harness apparent | du apparent | difference |
|---|---:|---:|---:|---:|---:|---:|
| `/usr` (btrfs, zstd) | 17 924 702 208 | 17 924 702 208 | **0** | 17 111 121 743 | 17 111 121 743 | **0** |
| `~/xiv` (272 GB) | 291 562 401 792 | 291 562 401 792 | **0** | 291 538 937 209 | 291 538 937 209 | **0** |
| `/` with `-x` | 24 174 469 120 | 24 174 469 120 | **0** | 23 322 350 579 | 23 322 350 579 | **0** |
| `$HOME` (live, 19.85 M entries) | 2 667 359 526 912 | 2 667 363 131 392 | -3.6 MB (-0.0001 %) | 2 641 456 707 856 | 2 641 462 267 925 | -5.6 MB (-0.0002 %) |

The $HOME difference is the tree changing between the scan and `du` (other jobs write there; `du -s` without `-x` and `du -sx` differ from each other by 1.6 MB for the same reason). On trees that do not change the totals are **byte-exact**, in disk and apparent mode, including a zstd-compressed filesystem (`st_blocks` already reflects the compression; no discrepancy). Directories have a `st_size` on btrfs, and `du --apparent-size` leaves them out, as the model does.
- **`OtherFilesystem` issues:** `$HOME` has none (no nested subvolume under it). Scanning `/` with one-filesystem on gives 24, all of them mount points or subvolumes with their own `st_dev`: `/home /root /srv /boot /dev /proc /run /sys /tmp /.snapshots /mnt/stuff /var/cache /var/tmp /var/log /var/lib/portables /var/lib/machines /mnt/nassy3/{anime,manga,music,tv,stuff,things,downloads,git}`. btrfs subvolumes (`@home`, `@srv`, snapper's `.snapshots`) are reported like mounts, and `du -x` skips the same ones, so the byte-exact match above includes this rule.
- The plain `du -s /` (without `-x`) descends into `/home` and the network mounts and takes minutes; I did not wait for it.

## Worker CPU while the channel is full (amendment)
`--slow-apply-ms 20` makes the main thread slow so the channel stays full; the walker threads poll with `try_send` every 500 us. CPU time of all `scan-*` threads over a 3 s window (2 s after start), 100 % = one core:
```
TARGET: worker_cpu_percent_while_channel_full=1.3 (4 threads)
TARGET: worker_cpu_percent_while_channel_full=2.7 (16 threads)
```
Negligible (a few percent of one core in total). **No backoff needed.**

## `category_of_node` during a scan (amendment)
Timed every 500 ms for the 60 largest children of the root and of its largest folder (it is file-only now, so it is a name-extension lookup): **p50 0.041-0.052 ms, p95 0.054-0.065 ms** per batch on the 19.85 M-entry scan (17-197 samples). Not a cost.

## Tree row build with a huge folder (amendment)
`TreeState::prepare` (rows + viewport) with the root expanded, one `prepare` per frame, 40-line view:
```
TARGET: tree_row_build_ms_100k=5.3 (target 50)
TARGET: tree_row_build_ms_1000k=83.8 (target 50)
```
100 k children are fine; **1 M children cost 84 ms per frame, above the 50 ms input target** (almost all of it is sorting the children, `children_sorted`, on every call). Real folders with 1 M direct children are rare, but a node_modules-style or cache folder can have 100 k+. Options for the architect: keep a sorted order per folder that is updated on apply instead of sorting per frame, or sort only the top K plus the aggregate count.

## Other findings
1. **Process RSS is 4.3 GiB with 2 or more walker threads but 2.4 GiB with 1 thread** on the same 19.85 M entries (model alone about 2.4-2.9 GiB, see R1). The extra 1.8-1.9 GiB is not explained by: the channel size (16 / 1024 / 65 536 messages give 4.2 / 4.4 / 4.8 GiB), or glibc arenas (`MALLOC_ARENA_MAX=1` and `=2` give 4.1-4.2 GiB). It is not verified whether it is model growth under out-of-order id blocks, allocator fragmentation from entries allocated by workers and freed by the main thread, or something else. A heap profile (or jemalloc counters) is the next step; it matters for the R1 budget, since the real process, not the model, is what the user sees.
2. The `apply` tick spikes (above) and the 1 M-child row build are the two places where the 50 ms input target can be missed.
3. `recompute_top` at 0.4-0.6 s on 20 M entries.
