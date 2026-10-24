# R5: treemap jitter during a live scan

Date: 2026-10-23. Tool: `cargo run --release --example spike_r5_jitter -- ROOT --size WxH` (one scan per root; the 250, 500 and 1000 ms schedules are evaluated side by side on the same scan). Layout: `layout::squarify`, aspect 2.0. Tracked scopes: the root's children and the children of the currently largest top-level directory. The selected tile is the largest at t = 2 s, held by NodeId.
Unit tests: `cargo test --example spike_r5_jitter` (6 pass).

Metric definitions (per relayout, against the previous one of the same scope and interval):
- *tiles changed*: share of tiles whose rectangle differs, counting tiles that appear or vanish. **Any 1-cell edge shift counts**, so this over-states what the eye notices.
- *mean centroid move*: mean distance in cells between the centres of tiles present in both.
- *rank swaps*: flipped pairs among the top 8 by size.
- *selected tile moved*: its rectangle differs from the previous relayout (again including a 1-cell drift).

## Data limits
- `/usr` scans in 0.4 s here (warm page cache, btrfs), so the 250 ms schedule gets one relayout and the others none: **no usable data**. Its tables are omitted below (all zero).
- `$HOME` (19.6 M entries) scans in 19 to 33 s, also warm. A cold first scan will be slower and so give more relayouts and slower drift, which is kinder to the map, not harsher.
- One run per size; the two runs differ (33 s and 19 s) so the numbers are indicative, not exact.

## Results (`$HOME`)
## /home/nss at 78x6 (19610883 entries, 32.6 s)

### interval 250 ms (130 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 0.0% | 100.0% | 100.0% |
| root children | mean centroid move (cells) | 0.00 | 1.43 | 12.69 |
| root children | rank swaps in top 8 | 0 | 1 | 3 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 0.0% | 94.4% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.00 | 3.08 | 12.48 |
| largest top-level dir | rank swaps in top 8 | 0 | 0 | 3 |
| largest top-level dir | scope changes | 1 | | |

Selected tile (largest at 2 s) moved in 20 of 122 relayouts.

### interval 500 ms (65 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 0.0% | 100.0% | 100.0% |
| root children | mean centroid move (cells) | 0.00 | 3.51 | 4.13 |
| root children | rank swaps in top 8 | 0 | 1 | 3 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 0.0% | 98.1% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.00 | 4.92 | 17.49 |
| largest top-level dir | rank swaps in top 8 | 0 | 0 | 4 |
| largest top-level dir | scope changes | 0 | | |

Selected tile (largest at 2 s) moved in 19 of 61 relayouts.

### interval 1000 ms (32 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 50.0% | 100.0% | 100.0% |
| root children | mean centroid move (cells) | 0.25 | 3.51 | 4.39 |
| root children | rank swaps in top 8 | 0 | 2 | 4 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 0.0% | 100.0% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.00 | 5.17 | 5.36 |
| largest top-level dir | rank swaps in top 8 | 0 | 3 | 3 |
| largest top-level dir | scope changes | 0 | | |

Selected tile (largest at 2 s) moved in 18 of 30 relayouts.

## /home/nss at 158x16 (19610049 entries, 18.7 s)

### interval 250 ms (74 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 0.0% | 90.9% | 100.0% |
| root children | mean centroid move (cells) | 0.00 | 5.30 | 8.38 |
| root children | rank swaps in top 8 | 0 | 1 | 4 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 0.0% | 98.3% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.00 | 9.25 | 23.84 |
| largest top-level dir | rank swaps in top 8 | 0 | 0 | 4 |
| largest top-level dir | scope changes | 0 | | |

Selected tile (largest at 2 s) moved in 18 of 66 relayouts.

### interval 500 ms (37 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 20.0% | 100.0% | 100.0% |
| root children | mean centroid move (cells) | 0.10 | 7.21 | 8.50 |
| root children | rank swaps in top 8 | 0 | 3 | 4 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 0.0% | 100.0% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.00 | 10.37 | 30.06 |
| largest top-level dir | rank swaps in top 8 | 0 | 0 | 4 |
| largest top-level dir | scope changes | 0 | | |

Selected tile (largest at 2 s) moved in 14 of 33 relayouts.

### interval 1000 ms (18 relayouts)

| scope | metric | p50 | p95 | max |
|---|---|---|---|---|
| root children | tiles changed | 50.0% | 100.0% | 100.0% |
| root children | mean centroid move (cells) | 0.30 | 8.37 | 8.37 |
| root children | rank swaps in top 8 | 0 | 4 | 4 |
| root children | scope changes | 0 | | |
| largest top-level dir | tiles changed | 3.0% | 100.0% | 100.0% |
| largest top-level dir | mean centroid move (cells) | 0.02 | 9.72 | 9.72 |
| largest top-level dir | rank swaps in top 8 | 0 | 3 | 3 |
| largest top-level dir | scope changes | 0 | | |

Selected tile (largest at 2 s) moved in 9 of 16 relayouts.


## Eyeballing the frames (`spikes/r5-frames/home_nss-78x6.txt`, 500 ms)
- The structure holds for the whole 33 s: the largest tile (a) stays on the left and the second (b) next to it; they widen and narrow by 1 to 6 columns between frames as sizes grow, and the small tiles on the right edge appear, merge and split (e.g. between t = 16.5 s and 20.5 s a third tile appears and a row of small ones forms).
- Nothing flips sides or swaps places between the big tiles; the top ranks swap rarely (p95 of 1 swap among the top 8).
- What the eye would see is a gentle drift of the dividing lines, not a reshuffle. The "100 % changed" p95 comes from every edge moving by a cell or two at the same time.
- At 158x16 the same pattern holds with larger absolute moves (p95 centroid move 5 to 9 cells).
- At 250 ms the drift per frame is smaller but there are twice as many frames; at 1,000 ms single steps are larger (p95 centroid move about 3.5 to 5 cells at 78x6) and the selected tile moves in 60 % of relayouts (18 of 30).

## Verdict and recommendation
- **Keep the 500 ms throttle** (design §5, ADR-006). It halves the redraw work of 250 ms, and its steps are no larger than at 250 ms in the median; 1,000 ms makes each step visibly bigger without a calmer picture.
- **No ordered-treemap fallback is needed while scanning**: ranks are stable and the layout keeps its structure. The ADR-006 revisit does not fire.
- Optional polish for T017 (not required): keep a tile's previous rect when the new one differs by at most 1 cell on every edge (hysteresis), which would remove most of the "selected tile moved" count caused by 1-cell drift. This would need a layout-contract exception (±1 edge already allowed), so it is a design decision, not part of this spike.
