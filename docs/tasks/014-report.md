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

## Review fixes
- `layout_contract` now also checks, independently of snapping, that each item's float rect area (in aspect space) equals `size / total × w × h × aspect` (1e-9 abs + 1e-6 rel), the aggregate equals the sum of its members' shares, and the shown areas sum to the area. With two areas deliberately swapped in `prepare` it failed (3 tests: `layout_contract` "id Some(NodeId(0)): area 1.0000022 want 438.99999", `proportional_within_rounding`, `tiny_items_merge_…`); reverted, 13 pass.
- `squarify` has a doc comment with the contract, including "not shown" for dropped empty tiles, and `debug_assert!(cell_aspect.is_finite() && cell_aspect > 0.0)`.
- `neighbor` already had a doc comment.
- fmt and clippy clean.
