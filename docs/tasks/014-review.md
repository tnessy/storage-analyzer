# T014 review (Heimdall, 2026-10-23): CHANGES REQUESTED (small)

**Fresh evidence:** I ran it in a clean worktree from HEAD plus only the T014 files, because the working tree also holds engineer-1's T003 work in progress. Results:
- fmt ok, clippy clean;
- `cargo test --all-targets`: 22 + 4 + 7 pass;
- `cargo build --release` ok;
- `TARGET: squarify_5000_items_ms=0.076 (target 1.0)`.

**Pass 1, spec:**
- It meets the brief: no gaps or overlaps, ±1 edge, a single trailing aggregate, deterministic with NodeId tie-breaks, and `neighbor` behaves as specified.
- Only owned files were touched, plus `proptest-regressions/layout/squarify.txt`. That file is correct to commit.

**Ruling on the open question (zero-area tiles):** **accepted**, with two conditions.
- Dropping a tile whose snapped rect is empty keeps the contract (coverage, no overlap, ±1 edge). Reserving a minimum cell for it would break proportionality.
- **Condition 1:** document it in the `squarify` doc comment. Items missing from the output, including a sub-cell aggregate, are "not shown".
- **Condition 2:** the UI must never lose them silently. T017 computes `not_shown = items.len() − tiles with Some(id)`. If there is no aggregate tile and `not_shown > 0`, the map says so. The copy comes from UX: I've asked for it, and T017 is amended.

**Required changes:**
1. `squarify.rs`, the `layout_contract` property: the ±1-edge check compares against the module's own `exact_layout`, so a wrong squarify (non-proportional float rects) would still pass. Add an independent property: for every non-aggregate item, its exact rect's area in aspect space equals `size / total × w × h × aspect` within 1e-6 relative error, and the aggregate's area equals the sum of its members' shares. Show that it fails if you, for example, swap two areas in `prepare`.
2. `squarify.rs:143` and `neighbor.rs:5`: `pub fn squarify` has no doc comment. Document the contract: sorting, the aggregate (`id: None`, last), the ±1 edge, dropped zero-area tiles, and `cell_aspect` > 0 and finite. Add `debug_assert!(cell_aspect.is_finite() && cell_aspect > 0.0)`. Validating the input belongs to the CLI (T015 is amended).

There is nothing else; the code quality is good.
