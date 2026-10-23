# T022: Spike R5, treemap jitter during a live scan

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T014, T006 (and T003). **Feeds:** T017.

## Goal
Retire R5 (design §7, ADR-006): is a squarified layout relaid out at ≤ 2 Hz readable while sizes change live, or do we need the ordered-treemap fallback while scanning?

Acceptance criteria:
1. `examples/spike_r5_jitter.rs ROOT [--interval-ms 500] [--size 78x6|158x16]` runs `RealFs` → walker → `Model` headlessly. Every interval it lays out the root's children (and the children of the currently largest top-level dir) with `layout::squarify`.
2. Per relayout, it computes:
   - the fraction of tiles whose rect changed;
   - the mean centroid displacement in cells;
   - the number of rank swaps among the top 8;
   - whether the selected tile (the largest at t = 2 s, held by NodeId) moved.

   It summarizes these over the scan as p50/p95/max.
3. It writes the ASCII frames to `spikes/r5-frames/<root>-<size>.txt` for eyeballing. This is the only write, and it is in the repo dir, not in the scanned root.
4. It compares intervals of 250, 500 and 1,000 ms on `$HOME` and `/usr`.
5. `spikes/R5-REPORT.md` holds the tables, a short verdict from eyeballing the frames, and a recommendation: keep the 500 ms throttle, change the interval, or use an ordered treemap while scanning (ADR-006 revisit).

## Files owned
`examples/spike_r5_jitter.rs`, `spikes/R5-REPORT.md`, `spikes/r5-frames/**`.

## Tests to write first
A `#[cfg(test)]` test in the example: the metric function on two hand-built layouts (identical, then one tile moved by 3 cells) returns 0 and the expected displacement. Command: `cargo test --example spike_r5_jitter`.

## Out of scope
Changing `layout` or building the widget (T017). Findings go to Heimdall.
