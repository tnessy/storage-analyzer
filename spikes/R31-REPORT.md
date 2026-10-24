# T031: why was RSS 4.3 GiB with 2+ walker threads and 2.4 GiB with 1?

Measured 2026-10-23 on `$HOME` (19.93 M entries), warm cache, release build, `scan_harness --malloc-report` (new: glibc `malloc_stats()` and `mallinfo2()`, then `malloc_trim(0)` and a second RSS reading; `--malloc-report` also prints a census of what the model holds) and `scan_harness_mimalloc` (the same harness with mimalloc as the global allocator, example only). The machine was shared and loaded.

## Answer
**There is no thread-count effect and no fragmentation or leak.** The 2.4 GiB figure for one thread was a measurement artefact; with the current code every configuration ends at about 4.0-4.1 GiB, and almost all of it is live model data.

| allocator | walker threads | RSS at the end | after `malloc_trim(0)` | glibc "in use" |
|---|---:|---:|---:|---:|
| glibc | 1 | 4058 MiB | 4020 MiB | 4016 MiB |
| glibc | 4 | 4100 MiB | 4061 MiB | 4004 MiB |
| glibc | 8 | 4059 MiB | 4019 MiB | 3999 MiB |
| mimalloc | 1 | 4051 MiB | 4015 MiB | n/a |
| mimalloc | 4 | 4055 MiB | 4023 MiB | n/a |

- **glibc keeps 98 % in use**: `malloc_stats()` shows arena 0 with about 4.15 GB "in use" out of 4.15 GB, and the other arenas only 10-60 MB. `malloc_trim(0)` gives back only 40 MB (1 %). That is not fragmentation: step 1 of the plan (in use close to the model size, trim frees little) says the memory is really in use.
- **mimalloc changes nothing** (4.02-4.05 GiB), so swapping the allocator would not help. No production change is proposed on this point (so there is nothing for the architect or the owner to approve).
- The earlier one-thread figure (2379 MiB, T009) was taken during a 98 s run on a machine that was swapping at the time (36 GB of swap in use, load average above 20); RSS does not count pages that were swapped out. The R1 spike hit the same effect and added `VmSwap` to its figure. I could not repeat the old number: one thread now gives 4.06 GiB, like every other configuration. (This is the most likely explanation, not a proof, since the old run cannot be replayed.)

## What the 4.0 GiB are (census of the model, 8 threads)
| | |
|---|---:|
| nodes | 19.94 M (0.90 M directories, 4.5 %) |
| duplicate hard links | **8.45 M (42 % of all nodes)** |
| names | 988 MiB, **52 bytes each** on average |
| per-node arrays (127 B x nodes) | 2415 MiB |
| children vectors (elements 76 MiB + capacity and headers) | about 123 MiB |
| sum of the above | 3526 MiB |
| rest (hard-link table: about 56 B per duplicate link, extension totals, top lists) | about 470 MiB |
| glibc "in use" | 3999 MiB |

So the model costs about **210 bytes per entry on this data**, against 153 in the R1 spike. The spike's synthetic data was too light in two ways: its name-length histogram came from the first 300 k names of a breadth-first walk (mean 19 B; the whole tree averages 52 B, long content-hash names), and it had 0.5 % hard-link duplicates instead of 42 % (a package store full of hard links).

## Recommendation
1. Do nothing about the allocator; do not add mimalloc or jemalloc.
2. The memory is the model. If it must shrink, the options with data behind them are (a) the directory table split from R1 (about 54 B/entry), (b) a more compact hard-link table: 8.45 M duplicates cost about 470 MiB (a duplicate is stored as a 4-byte id inside a hash entry plus its share of the inode's entry), (c) shorter or shared name storage (names are 25 % of the total). These are design decisions for the architect; none is needed to meet any gate.
3. Update the R1 numbers in the design/budget: use about 4.0 GiB for a 20 M-entry `$HOME`-like tree, not 2.3-2.9 GiB.

## Reproduce
```
cargo run --release --example scan_harness -- $HOME --threads 4 --malloc-report
cargo run --release --example scan_harness_mimalloc -- $HOME --threads 4 --malloc-report
```
