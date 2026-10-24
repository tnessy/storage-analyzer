# T022 report (engineer-2)

Status: DONE

Files: `examples/spike_r5_jitter.rs`, `spikes/R5-REPORT.md`, `spikes/r5-frames/*.txt` (4 files).

- Tests first: the six metric tests (identical layouts, a 3-cell move, a diagonal move, appearing and vanishing tiles, rank swaps, percentiles) failed against stubs, then passed (`cargo test --example spike_r5_jitter`).
- Verdict (full tables in `spikes/R5-REPORT.md`): **keep the 500 ms throttle; no ordered-treemap fallback needed.** On `$HOME` (19.6 M entries, 19 to 33 s warm scans) ranks are stable (p95 of 1 swap among the top 8), the big tiles keep their sides, and the drift is 1 to 6 columns per frame at 78x6. "100 % of tiles changed" at p95 is a 1-cell edge shift everywhere; p95 centroid move is about 3.5 cells at 500 ms (78x6) and about 7 at 158x16. 1,000 ms makes each step bigger and moves the selected tile in 60 % of relayouts.
- Limits: `/usr` scans in 0.4 s here (warm cache), so its tables are all zero and omitted; `$HOME` was also warm. One run per size.
- Deviations: `--interval-ms` takes a comma list (default `250,500,1000`) so all intervals are evaluated on the same scan; frames go to `spikes/r5-frames/<root>-<WxH>.txt` with all intervals in one file.
- Optional polish noted for T017: a 1-cell hysteresis on tile rects (a layout-contract decision, not part of the spike).
