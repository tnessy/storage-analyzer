# T008: Spike R1, memory per entry

- **Engineer:** engineer-1. **Wave:** 1. **Depends on:** T003. **Parallel with:** engineer-2's tasks.

## Goal
Measure the real model's memory per entry at **20 M** entries (design §7 R1, rev 7: R3 measured 19.46 M entries in the user's `$HOME`). Design §5 sets a **target** (not a gate) of ≤ 120 B/entry including names.

Acceptance criteria:
1. `spikes/r1-memory/` is a binary crate that depends on the main crate by path. It builds a synthetic `ScanMsg` stream for N entries (default 20 M) with realistic shape: about 10 % dirs, fan-out drawn from a skewed distribution, name lengths sampled from a distribution recorded on the user's `$HOME`. The spike may read `$HOME` names read-only to build that histogram; it stores only the histogram.
2. It applies the stream through `Model::apply`. It reports RSS (from `/proc/self/statm`) before and after, bytes per entry, and apply throughput (entries/s, single core).
3. It runs at N = 1 M, 5 M, 10 M and **20 M**, and prints `TARGET: model_bytes_per_entry=… (target 120)` and `TARGET: model_rss_gib_at_20M=…` (about 2.3 GiB at the target).
4. `spikes/r1-memory/REPORT.md` holds the table and, if the target is exceeded, what dominates (names, child vecs, hard-link map) and options. **Do not change model code in this task.** Findings go to Heimdall.

## Files owned
`spikes/r1-memory/**`.

## Tests to write first
A small-N test (10 k) asserts the stream is valid: parent-before-child order, and after apply the root is complete with the expected entry count. Commands: `cargo test --manifest-path spikes/r1-memory/Cargo.toml`, then `cargo run --release --manifest-path spikes/r1-memory/Cargo.toml -- --entries 20000000`.

## Out of scope
Optimizing the model. Scanner memory (the frontier is measured in T009/R8).
