# T014: Layout (squarify + neighbor, pure)

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T001. **Blocks:** T017, T022.

## Goal
Pure treemap layout for brief M4: area is proportional to size within terminal-cell rounding, with no gaps and no overlaps. Spatial neighbor lookup for map focus.

Acceptance criteria:
1. `squarify(items, area, cell_aspect)`:
   - Squarified (Bruls et al. 2000), one level, over items sorted by (size desc, NodeId asc). The function sorts them itself; it doesn't trust the input order.
   - Lays out in float, with height scaled by `cell_aspect` (default 2.0, `--cell-aspect`).
   - **Snaps edges, not sizes**, to integer cells. Adjacent tiles share the rounded edges.
2. Items whose exact share is below 1 cell, and any tail too small to place, are merged into **one** aggregate tile `Tile { id: None }`, placed last. Zero-size items count toward the aggregate's N but take no area. If every item is 0 B, or the list is empty, the result is empty.
3. **Contract** (design §3.7): the tiles cover `area` exactly, don't overlap, and each tile's edges are within ±1 cell of its exact float rectangle's edges.
4. Deterministic: the same input gives the same output. Ties break by NodeId.
5. `neighbor(tiles, from, dir)` returns the tile adjacent in `dir` with the **largest overlap on the shared axis**, ties broken by nearest centre, then lowest index. It returns `None` at the edge.
6. Performance (a target, not a gate): ≤ 1 ms for 5,000 items into 160×20 in release mode. Print a `TARGET:` line from a test.

## Design references
§3.7, ADR-006, UX spec §3.3. Map focus keys: UX §6.

## Files owned
`src/layout/mod.rs` (taken over), `src/layout/squarify.rs`, `src/layout/neighbor.rs`. Unit and property tests go in those files.

## Interfaces
**Consumes:** `Tile`, `Direction` (`src/layout/contract.rs`, T001). **Provides:** `squarify`, `neighbor` to T017 and T022.

## Tests to write first
- Property tests (proptest, 1–500 items, sizes 0..1e12, areas from 1×1 to 200×60, aspect 1.0–3.0):
  - no overlap;
  - union = area (sum of tile areas = w·h);
  - ±1 cell per edge against the exact float layout;
  - determinism;
  - the aggregate appears at most once.
- **M4 proportionality:** 4 items with 50/25/15/10 % in 78×6 → each tile's area share is within the ±1-edge bound of its share.
- Aggregate: 1 large item and 200 tiny ones → exactly 2 tiles, with the second being `id: None`.
- Empty and all-zero input → `[]`.
- `neighbor`: a hand-built 2×2 grid and an L-shaped layout give the expected tile in each direction, and `None` at the edges.

Command: `cargo test layout::`. For the timing line: `cargo test --release layout:: -- --nocapture`.

## Out of scope
Rendering, colors and labels (T017). Relayout throttling (T017).
