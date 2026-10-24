# T030 (PROPOSED, recommended): Remove the 118–147 ms apply-tick spikes

- **Engineer:** engineer-1. **Depends on:** T009 (merged).
- **Status:** proposed. Needs owner/user approval: 50 ms is a target, not a gate (plan §1). It is recommended because the spikes are 3× the input-latency target during a large scan (M1: "the UI never waits").

## Problem
`scan_harness` on `$HOME` (20 M entries) shows a median apply tick of 0.01 ms, but a few ticks of 118–147 ms per run. During those, keystrokes wait.
- **Hypothesis (unverified):** the struct-of-arrays `Vec`s in `model/tree.rs` double their capacity, so one `ensure_len` copies up to ~2 GiB across 17 arrays.
- `recompute_top` (0.43–0.61 s at 20 M) is a second stall source, after a large delete.

## Acceptance
1. **Confirm the cause first.** Time `ensure_len` (or use `perf`) and report which ticks it explains.
2. **Fix within the existing design.** Options: grow in fixed chunks; a segmented arena (stable chunks, no big copies); or reserve from an estimate (history/session-previous) with geometric growth capped per tick. **No change to the `Model` API.**
3. **`recompute_top` after a delete:** move it to an incremental update (remove the deleted ids, then refill from a bounded scan) or to a helper thread (design §3.4 allows the latter). Pick one and justify it.
4. **Measure with the harness:** report `apply tick max` and p99 before and after on `$HOME`. **Target:** max ≤ 50 ms (reported, not gated).

**Tests first:** a unit test that growing past several doublings never moves already-existing names or nodes (behaviour unchanged). Plus a bench-style test printing `TARGET: apply_tick_max_ms`.

## Files owned
`src/model/tree.rs`, `src/model/names.rs`, `src/model/index.rs`, `src/model/mod.rs` (only if needed).
