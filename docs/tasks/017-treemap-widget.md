# T017: Treemap widget, map focus, tree ↔ map sync

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T014, T016, T005 (`category_of_node`). Read T022's R5 report first if it is done.

## Goal
Brief M4: a terminal treemap of the current directory, with area proportional to size within cell rounding, labels that fit or truncate, and tree and map **in sync**.

Acceptance criteria:
1. **Placement** (UX §3): the map title rule `Map: <path>` with `(updating)` while scanning and `(partial)` after a stop, then the map, which gets 40 % of the body rows (rounded in the map's favour, minimum 6 rows).
2. **Scope** (design §3.7, UX §3.3): the children of the selected item's parent. If the selected item is the view root, the view root's children are shown instead. The selected item's tile is highlighted.
3. **Tiles** use `layout::squarify` with `--cell-aspect`. Rendering:
   - single border, or **double border on the selected tile**;
   - label line 1 is `name size pct% [T]`, and line 2 holds whatever didn't fit;
   - truncation with `…` by display width;
   - tiles narrower than 6 cells get no label, but `!`/`⊘` still show;
   - the category fill and letter tag come from `Model::category_of_node` and the UX §7 table, put in `Theme`;
   - the aggregate tile is `(+N smaller)` and grey.
4. **Relayout throttling:** while scanning, the layout is recomputed at most every 500 ms (cached by scope id and area), but the selection highlight updates every frame. When the scan is done or stopped, relayout happens on change.
5. **Map focus:**
   - `Tab`/`Shift-Tab` toggle tree and map focus. The focused pane's title is bold with `*`;
   - in map focus, arrows/`hjkl` move with `layout::neighbor`;
   - `⏎` zooms, `⌫` goes up (same as the tree);
   - the key hints follow UX §6 "Map".
6. **Sync (M4):** there is **one** `selection: NodeId` (from T016). Moving in the map updates the tree selection and scrolls the tree to it, and moving in the tree updates the highlighted tile. Selecting the aggregate tile selects no node, the tree keeps its selection, and the aggregate gets the double border.
7. **Empty states** (UX §5.5): `Waiting for first results…`, `This folder is empty.`, and `N items, all 0 B. Nothing to map.`

## Design references
§3.7, §3.9 (map strip and focus), §5 relayout ≤ 2 Hz. ADR-006. UX §3, §3.3, §5.5, §6, §7.

## Files owned
`src/ui/treemap.rs`, `src/app/map_state.rs`, `src/ui/theme.rs` (adds category fills), `src/app/keymap.rs` (map keys), `src/app/state.rs` (focus pane), `tests/ui_treemap.rs`.

## Tests to write first (`tests/ui_treemap.rs`)
- **M4 proportional:** for a synthetic dir with 50/25/15/10 % children at 80×24, each rendered tile's cell area is within the ±1-edge bound of its share (reuse the T014 bound helper, made `pub(crate)`).
- **M4 labels:** a long name truncates with `…` inside its tile and never draws outside it.
- **M4 sync, tree → map:** pressing `↓` in the tree moves the double border to the matching tile, and changing the parent (zoom, or `→` into a child) changes the map scope.
- **M4 sync, map → tree:** after `Tab`, arrow moves change `selection`, the tree's `>` row matches, and it is scrolled into view.
- **Scope rule:** selecting the view root shows its children, and selecting a child shows its siblings.
- **Throttle:** a flood of messages over a simulated 2 s produces ≤ 5 relayouts, while a selection change in between redraws the highlight in the very next frame.
- **Markers:** a tile with an issue shows `!`, and a skipped one shows `⊘`, including when it is < 6 cells wide.
- **Snapshots:** the scanning, Done, Stopped `(partial)` and empty map states at 80×24.

Command: `cargo test --test ui_treemap`.

## Out of scope
Delete from the map (T021). 256-color mapping and NO_COLOR (T023).
