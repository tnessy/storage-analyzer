# T014 report (engineer-2)

Status: DONE

Files: `src/layout/mod.rs`, `src/layout/squarify.rs`, `src/layout/neighbor.rs`.

Red: with stubs (empty `Vec`, `None`), 8 of 12 layout tests failed (e.g. `layout_contract`, `proportional_within_rounding`, the neighbor tests). The 3 trivial ones (empty, degenerate area, out-of-range index) passed on the stubs.
Green: 13 layout tests pass (includes a proptest with up to 500 items, areas 1×1 to 200×60, aspect 1–3).

Fresh runs: `cargo fmt --all -- --check` clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test --all-targets` all pass (25 + 14 + 4 + 7); `cargo test --doc` 0 doctests, ok; `cargo build --release` ok.
`TARGET: squarify_5000_items_ms=0.080 (target 1.0)`.

Decisions:
- Every shared edge is computed once and reused, so neighbours agree bit for bit and rounding cannot make gaps or overlaps.
- Tiles whose rounded rect has zero width or height are dropped, so an item can get no tile at tiny sizes. Coverage still holds, because neighbours share the rounded edges. This includes an aggregate whose total area is below one cell: it disappears. The brief does not cover this case; confirm you accept it.
- The one-cell threshold is `cell_aspect` in aspect-corrected space (one cell has area 1 × aspect).
- `neighbor` requires strict adjacency (edges touch) and positive overlap. Tiles in a gap are not reached.
- The ±1-edge property test compares against the module's private float layout (`exact_layout`), so it checks the snapping, not the squarify algorithm itself. The M4 proportionality test uses independent numbers (50/25/15/10 % in 78×6) with a loose bound of 2(w+h)+4 cells per tile.
