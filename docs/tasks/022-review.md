# T022 (spike R5) review (Heimdall, 2026-10-23): APPROVED, committed
- **Fresh:** `cargo test --example spike_r5_jitter` 6 pass; red first is reported. The report and the frames (`spikes/r5-frames/`, 72 KB) were read.
- **Verdict accepted: keep the 500 ms relayout throttle, no ordered-treemap fallback.** On `$HOME` (19.6 M entries):
  - top-8 rank swaps ≤ 1 at p95;
  - the selected large tile moved in 19 of 61 relayouts, mostly 1-cell edge drift;
  - the p95 mean centroid move is ~3.5 cells at 78×6.

  ADR-006's revisit trigger does not fire. Reported to the architect via the team lead.
- **Limits accepted as stated:** warm cache only; `/usr` was too fast to measure; one run per size. A cold first scan would only slow the drift.
- **Deviation accepted:** `--interval-ms` takes a comma list (all three schedules evaluated on one scan).
