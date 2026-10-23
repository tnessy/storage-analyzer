# R3: Knuth probe ETA accuracy (rev 2: variants A and B, 100 seeds)

Run: 2026-10-23, release build. `/` (st_dev 36) and `/home` (st_dev 59) are separate btrfs subvolumes; both are on the same disk. The `$HOME` run stays on `/home`.
Command: `cargo run --release -- ROOT --seeds 100`. For each seed, K=16/64/256 are prefixes of one probe stream. The tables below are its output, unedited.
Tests (`cargo test`, 5 pass): uniform tree exact for both variants; skewed tree mean over 2,000 seeds within 5 % of truth for both variants; shallow-first checkpoints on a uniform tree.

## Variants
- **A (design §3.6 as written):** pick uniformly among all entries, stop when the pick is a file. N = Σ_d Π b_i.
- **B (subdirectory descent):** pick uniformly among the same-filesystem subdirectories, X(v) = e(v) + s(v)·X(child). It never stops at a file and is also unbiased. It reads one directory per level.

## Reading the tables
- *ratio* = estimate/true over 100 seeds (p10 / p50 / p90). *within 0.5–2×* = share of seeds in that band.
- *ETA err* = estimated remaining / true remaining − 1, at 10/50/90 % of the true scan, as p50 / p90 over all seeds. It uses `est_total = max(estimate, seen)` and a constant rate. −1.00 means the estimate was already below `seen`, so the ETA reads zero or "past estimate". Positive values mean the ETA is too long.
- Wall times are hot-cache (the full walk had just run).
- The shallow-first table simulates a BFS scan and uses `seen + pending_dirs` as the total. It has no probe and is deterministic.

## /home/nss (true_entries=19465730, walk_s=6.3, seeds=100)

| variant | K | ratio p10 | ratio p50 | ratio p90 | within 0.5-2x | ETA err@10% p50/p90 | @50% p50/p90 | @90% p50/p90 | mean ms |
|---|---|---|---|---|---|---|---|---|---|
| A (all entries) | 16 | 0.000 | 0.001 | 0.018 | 0% | -1.00 / -1.00 | -1.00 / -1.00 | -1.00 / -1.00 | 0.4 |
| A (all entries) | 64 | 0.000 | 0.003 | 0.019 | 3% | -1.00 / -1.00 | -1.00 / -1.00 | -1.00 / -1.00 | 1.8 |
| A (all entries) | 256 | 0.002 | 0.006 | 0.127 | 3% | -1.00 / -0.97 | -1.00 / -1.00 | -1.00 / -1.00 | 6.9 |
| B (subdirs) | 16 | 0.000 | 0.003 | 0.062 | 2% | -1.00 / -1.00 | -1.00 / -1.00 | -1.00 / -1.00 | 0.7 |
| B (subdirs) | 64 | 0.002 | 0.013 | 0.216 | 2% | -1.00 / -0.87 | -1.00 / -1.00 | -1.00 / -1.00 | 2.8 |
| B (subdirs) | 256 | 0.012 | 0.069 | 0.913 | 9% | -1.00 / -0.10 | -1.00 / -0.17 | -1.00 / -0.87 | 10.7 |

Shallow-first scan, lower-bound total = seen + pending_dirs:

| progress | seen | pending_dirs | (seen+pending)/true | ETA err |
|---|---|---|---|---|
| 10% | 1946576 | 317829 | 0.116 | -0.98 |
| 50% | 9732913 | 206129 | 0.511 | -0.98 |
| 90% | 17519182 | 51497 | 0.903 | -0.97 |

## /usr (true_entries=347376, walk_s=0.1, seeds=100)

| variant | K | ratio p10 | ratio p50 | ratio p90 | within 0.5-2x | ETA err@10% p50/p90 | @50% p50/p90 | @90% p50/p90 | mean ms |
|---|---|---|---|---|---|---|---|---|---|
| A (all entries) | 16 | 0.042 | 0.083 | 0.415 | 6% | -1.00 / -0.65 | -1.00 / -1.00 | -1.00 / -1.00 | 2.6 |
| A (all entries) | 64 | 0.062 | 0.134 | 0.799 | 9% | -0.96 / -0.22 | -1.00 / -0.40 | -1.00 / -1.00 | 10.3 |
| A (all entries) | 256 | 0.097 | 0.183 | 0.790 | 19% | -0.91 / -0.23 | -1.00 / -0.42 | -1.00 / -1.00 | 41.4 |
| B (subdirs) | 16 | 0.063 | 0.126 | 0.880 | 9% | -0.97 / -0.13 | -1.00 / -0.24 | -1.00 / -1.00 | 3.5 |
| B (subdirs) | 64 | 0.104 | 0.229 | 0.904 | 23% | -0.86 / -0.11 | -1.00 / -0.19 | -1.00 / -0.96 | 13.6 |
| B (subdirs) | 256 | 0.176 | 0.352 | 1.044 | 22% | -0.72 / +0.05 | -1.00 / +0.09 | -1.00 / +0.44 | 53.2 |

Shallow-first scan, lower-bound total = seen + pending_dirs:

| progress | seen | pending_dirs | (seen+pending)/true | ETA err |
|---|---|---|---|---|
| 10% | 34984 | 2359 | 0.108 | -0.99 |
| 50% | 173713 | 2449 | 0.507 | -0.99 |
| 90% | 312878 | 2861 | 0.909 | -0.92 |

## /home/nss/workspace/minecraft/pollution-craft-docs (true_entries=16723, walk_s=0.0, seeds=100)

| variant | K | ratio p10 | ratio p50 | ratio p90 | within 0.5-2x | ETA err@10% p50/p90 | @50% p50/p90 | @90% p50/p90 | mean ms |
|---|---|---|---|---|---|---|---|---|---|
| A (all entries) | 16 | 0.005 | 0.166 | 0.585 | 11% | -0.93 / -0.46 | -1.00 / -0.83 | -1.00 / -1.00 | 0.3 |
| A (all entries) | 64 | 0.081 | 0.279 | 0.931 | 21% | -0.80 / -0.08 | -1.00 / -0.14 | -1.00 / -0.69 | 1.0 |
| A (all entries) | 256 | 0.176 | 0.328 | 0.947 | 25% | -0.75 / -0.06 | -1.00 / -0.11 | -1.00 / -0.53 | 3.8 |
| B (subdirs) | 16 | 0.067 | 0.300 | 1.772 | 22% | -0.78 / +0.86 | -1.00 / +1.54 | -1.00 / +7.72 | 0.4 |
| B (subdirs) | 64 | 0.239 | 0.497 | 1.632 | 40% | -0.56 / +0.70 | -1.00 / +1.26 | -1.00 / +6.32 | 1.6 |
| B (subdirs) | 256 | 0.330 | 0.630 | 1.425 | 68% | -0.41 / +0.47 | -0.74 / +0.85 | -1.00 / +4.25 | 6.5 |

Shallow-first scan, lower-bound total = seen + pending_dirs:

| progress | seen | pending_dirs | (seen+pending)/true | ETA err |
|---|---|---|---|---|
| 10% | 1673 | 493 | 0.130 | -0.97 |
| 50% | 9006 | 436 | 0.565 | -0.94 |
| 90% | 15056 | 76 | 0.905 | -0.95 |


## Findings
- The brief's formula (A) was literally implemented, and it is biased low in practice because a descent ends at the first file picked. Its estimates are heavy-tailed.
- **B is clearly better but still not good enough.** At K=256 its p50 ratio is 0.07 on `$HOME`, 0.35 on `/usr` and 0.63 on the node_modules project, with 9 %, 22 % and 68 % of seeds within 0.5–2×. K=64 (the design default) is worse.
- On `$HOME` even B/256 has a median ETA error of −1.00 at 10/50/90 %: the probe is usually below `seen` well before 50 %.
- On the small node_modules project B has the opposite tail: p90 ETA error is +4 to +8 at 90 % (the ETA is several times too long).
- Probe cost is small (B/256: 11 ms on `$HOME`, 53 ms on `/usr`, hot cache). The issue is accuracy, not time.
- **`seen + pending_dirs` (shallow-first) is a good lower bound on progress, not an ETA.** At each checkpoint it is 0.12 / 0.51 / 0.90 of the true total on `$HOME` (similar elsewhere). So the fraction `seen / (seen + pending)` is close to 1 at all times and tells you nothing. The ETA error is −0.92 to −0.99 everywhere, because pending directories are not counted for their contents.

## Recommendation
- **A (design as written): drop.** It fails on all three roots.
- **B: usable only as a range, not as a point ETA.** If shown at all, show it as an order-of-magnitude hint ("at least X entries"; the probe's p10–p90 spans more than 10× on `$HOME`), and only with K ≥ 256. Do not present a `~%` from it. On trees like `$HOME` the value stays below `seen` early and gives "past estimate".
- With history off, the dependable default is elapsed time plus entries seen and directories pending, with no ETA. Use the history (*good*) and inode (*fair*, ext4/xfs) sources when they exist.
- ADR-005's trigger ("probe error > 2× at 50 %") fires for A on all roots and for B on `$HOME` and `/usr`; B on the node_modules project is within 2× in 68 % of seeds at K=256.
- Open: a better estimator (stratified by top-level directory, or importance-weighted by `st_nlink`/directory size) was not tried.
