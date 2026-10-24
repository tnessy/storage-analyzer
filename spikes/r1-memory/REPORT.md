# R1: memory per entry of the real model (T008)

Measured 2026-10-23 with `cargo run --release -- ` (this crate), on the dev machine (16 cores, heavily loaded by other jobs: load average 25-55, so the throughput figures are lower bounds). Model code was not changed.

## Method
- `Stream` builds a synthetic `ScanMsg` stream lazily (only the model grows): 10.0 % directories, skewed fan-out (40 % empty, 35 % 1-5, 20 % 6-40, 4.5 % 41-500, 0.5 % 501-5000 entries), file sizes log-uniform 1 B - 4 MB (disk = rounded up to 4 KiB), 0.5 % of files are one of two links to the same inode, directories picked at random so the tree depth is about log N (always taking the newest directory built chains 460 levels deep and made `apply` 14x slower: see "Findings").
- Name lengths come from a histogram of 300 k names under `$HOME` (read with `read_dir`, no `stat`, nothing but the 255 counts kept, in `home-histogram.txt`): mean 19.0 bytes.
- Each size runs in its own process. RSS is `/proc/self/statm` resident pages, plus `VmSwap` (the machine was swapping during a first try; here nothing was swapped). A counting global allocator gives the heap bytes asked for (capacity, resident or not).
- Throughput = time spent inside `Model::apply` only, single thread.

## Results (release build)
| entries | RSS delta | B/entry (RSS) | heap asked | B/entry (heap) | apply |
|---:|---:|---:|---:|---:|---:|
| 1 M | 148 MiB | 155.4 | 150 MiB | 157.0 | 4.1 M entries/s |
| 5 M | 751 MiB | 157.4 | 1181 MiB | 247.6 | 4.1 M/s |
| 10 M | 1457 MiB | 152.8 | 2361 MiB | 247.6 | 3.8 M/s |
| **20 M** | **2914 MiB** | **152.8** | 4722 MiB | 247.6 | 3.4 M/s |

```
TARGET: model_bytes_per_entry=152.8 (target 120)
TARGET: model_rss_gib_at_20M=2.85 (about 2.3 at the target)
```
The target (120 B/entry, 2.3 GiB) is **exceeded by about 27 %**. Run-to-run noise on the RSS figure is about +-2 B/entry. "Heap asked" is larger than RSS from 5 M on because the arrays grow by doubling: capacity beyond the used length is reserved but never touched, so it is not resident. It is address space only (the allocator did not copy on growth: peak equals final).

## Where the 152.8 B go (per entry, 20 M)
Fixed per-node arrays in `model/tree.rs` (structure of arrays, every node has every field):

| field | bytes |
|---|---:|
| parent (u32) | 4 |
| name ref (u64 offset + u16 length, padded) | 16 |
| kind | 1 |
| flags (u16) | 2 |
| own_disk, own_apparent | 16 |
| agg_disk, agg_apparent | 16 |
| files, dirs (u64) | 16 |
| pending_dirs, inexact_dirs (u32) | 8 |
| nlink | 4 |
| ino | 8 |
| dev (u64) | 8 |
| issue (u32) | 4 |
| children `Vec<NodeId>` header | 24 |
| **per-node arrays** | **127** |
| name bytes (arena) | 19.0 |
| children elements (4 B per child) | 4.0 |
| **sum** | **150.0** |
| measured | 152.8 |
| remainder: allocator header per non-empty children Vec, hard-link map, ext map, top-N, chain bookkeeping | 2.8 |

Vec growth slack (capacity above length) costs address space, not RSS, as shown above; with exact growth the heap figure would be about the RSS figure. Names are 12.7 % of the total, per-node arrays 83 %, the children `Vec` headers alone 16 %.

Hard-link map (5 M entries): 0 % links 155.3, 0.5 % 155.8, 5 % 158.9 B/entry. That is about 157 bytes per linked pair (hash entry + `SmallVec`), 0.4 B/entry at a realistic 0.5 %. Not a factor.

## What would save how much (estimates, not implemented)
| change | saving per entry | how |
|---|---:|---|
| (a) dir-only fields in a dir table | **about 54 B** | `agg_disk`, `agg_apparent`, `files`, `dirs`, `pending_dirs`, `inexact_dirs`, `children` header = 64 B apply to the 10 % directories only (a file's aggregate is its own size). Files keep 63 B + a 4 B row index; directories 131 B: average 73.4 instead of 127. |
| (b) `dev_idx: u16` instead of `dev: u64` | 6 B | a small device table; one filesystem in practice. |
| (c) 12-byte `NameRef` | 4 B | `u32` offset + `u16` length + padding in a `u64`-free layout (limits the arena to 4 GiB: 380 MB at 20 M). |
| (d) `issue` and `nlink` in side tables | about 8 B | only a few nodes have an issue; only links have `nlink > 1` (the delete dialog and hard-link table need nothing else). |

(a) alone gives about 99 B/entry (1.85 GiB at 20 M), (a)+(b)+(c) about 89 B (1.65 GiB), with (d) about 81 B (1.5 GiB). So the 120 B target is reachable with (a) only; names and children elements (23 B) are the floor.

## Findings for Heimdall
1. **Target missed by 27 % at 20 M (2.85 GiB instead of 2.3 GiB).** The largest single cause is giving every node the directory-only fields (option (a)). Not a gate; a decision for the lead whether the dir table is worth the complexity (a split touches `tree.rs`, `mod.rs`, `control.rs`, `delete.rs`).
2. **`apply` is O(depth) per listing** (it walks the ancestors once per message). With the first, newest-first generator the tree was 464 levels deep at 200 k entries and 5 M entries took 75-80 s (0.07 M entries/s) instead of 1.2 s. Real trees are 10-40 levels deep, the M8 fixture (200 levels) is tiny, so this is not a problem in practice, but a pathological deep tree would slow the main thread. Worth keeping in mind for the 8 ms drain budget.
3. Throughput 3.4-4.1 M entries/s on one (loaded) core: the model is not the bottleneck of a scan.
4. RSS alone can under-report when the machine swaps (the first 5 M run showed 117 B/entry only because pages had been swapped out); the spike adds `VmSwap`.

## Reproduce
```
cargo test --manifest-path spikes/r1-memory/Cargo.toml
cargo run --release --manifest-path spikes/r1-memory/Cargo.toml                    # 1, 5, 10, 20 M
cargo run --release --manifest-path spikes/r1-memory/Cargo.toml -- --entries 20000000 --hardlinks 0.05 --names builtin
```
