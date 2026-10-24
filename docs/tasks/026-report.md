# T026 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/ui/tile_colors.rs` (new: `Hue`, `HueSet`, `TileKind`, `Fill`, `adjacency`, `assign`, `TileColors`), `src/ui/{theme,treemap,types_view,list_common,copy,mod}.rs`, `src/app/{state,map_state}.rs`, tests `tests/ui_treemap.rs`, `tests/ui_lists.rs`, snapshots (map and Types).

## Evidence
- `assign` and `adjacency` unit tests (10, in `tile_colors.rs`) were written against a stub: 9 failed (the 10th, determinism, passes on a stub that is deterministic). Then implemented.
- **Lapse:** the integration side (theme palette, map wiring, rendering) was implemented before its tests, so those tests were never seen failing as written. I compensated with mutation checks, reverted afterwards: ignoring the reserved set made `folders_never_use_the_hue_of_a_file_type…`, `with_all_seven_types…` and the stability test fail; dropping the memory made the stability test fail.
- The Types swatch tests were written first and failed (compile error on `HELP_MAP_COLOURS`; then the swatch offset was off by one cell, fixed).
- Fresh: fmt, full clippy, `cargo test --all-targets` (lib 102, ui_treemap 37, ui_lists 17, ui_tree 31, rest unchanged), doc tests, release build pass.

## Behaviour (UX §7 option C)
- Palette in `theme.rs` with truecolor and xterm-256 values; `Theme.depth` (`TrueColor`, `Indexed256`, `Mono`) is the test-only switch until T023. Contrast: all 8 fills ≥ 4.5 in both columns, and ≥ 5.19 (truecolor) / 5.53 (256) as the spec lists.
- Files take their type hue, the aggregate is grey, folders are assigned by `assign` (memory kept, else first free hue from `NodeId mod 7`, skipping reserved hues and neighbours; least-used fallback; unfilled when all 7 types are present). Assignment runs at relayout (≤ 2 Hz).
- `AppState.tile_colors` holds `reserved` (per scope folder, grows only, covers small files without tiles) and `folder_colors` (session memory). `forget_reserved(scope)` is the rebuild hook for a rescan; `begin_rescan` is still T012's `todo!()`, so nothing calls it yet.
- Labels: folders `name/ size pct%` (no tag), files `name size pct% [T]`. Frames stay on default colors. Unfilled and Mono tiles use the default background.
- Types view: a 2-cell swatch before each category row (kept on the reversed cursor row), plus the `[T]` tag; no swatch in Mono. The help lines are `copy::HELP_MAP_COLOURS` for T023.
- The T025 alternate-shade table, `CategoryStyle` and the old shade tests are gone.

## For the lead
- `Model::category_of_node` is still called, but only for files (the map and the Types legend). Nothing in the UI now calls it for a **folder**, so the folder branch of that model method (and its `dir_categories` cache) has no caller; engineer-1 may want to drop it.
- The spec's check (e) zoom test passes without the memory because the start hue is a function of NodeId; only the stability test depends on the memory.
