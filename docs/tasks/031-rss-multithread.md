# T031 (PROPOSED, recommended): Explain the RSS 4.3 GiB (≥ 2 threads) vs 2.4 GiB (1 thread)

- **Engineer:** engineer-1. **Depends on:** T009. **Status:** proposed (investigation first; a fix only if the cause is clear).

## Problem
The same `$HOME` data gives 4.3 GiB RSS with ≥ 2 walker threads, but 2.4 GiB with 1 thread. The model alone measured 2.85 GiB in T008, and RSS is what the user sees. `MALLOC_ARENA_MAX=1` and `=2` don't change it, and neither does the channel size.

## Acceptance
1. **A heap profile** (`heaptrack` or `valgrind --tool=massif`, whichever is installed; ask the user if neither is) of `scan_harness $HOME --threads 1` vs `--threads 4`. Report the top allocation sites at peak.
2. **Name the cause**, with evidence. Candidates to check: per-worker buffers, the `RawDir` 32 KiB buffers, the queue's `PathBuf`s at the ~500 k frontier, `Vec<RawEntry>` held in the channel, and allocator fragmentation from cross-thread frees.
3. **If the fix is small and local to `scan/`,** implement it with a test and report RSS before and after. Otherwise, write up the options for the architect.

## Files owned
`src/scan/**` (only for a small fix), `spikes/R31-REPORT.md`.
