# T016: Tree view (rows, sort, % of parent, markers, selection, zoom)

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T015 (and T003). **Blocks:** T017, T018, T020, T021.

## Goal
Brief M3: a navigable tree with the root at the top and children beneath their parent, largest first, showing size and percent of parent at every level, all updated live. Selection follows the item.

Acceptance criteria:
1. **Row anatomy** (UX §3.2): `[sel] SIZE % BAR STATE INDENT+TWISTY NAME SUFFIX`.
   - SIZE is IEC with 1 decimal, right-aligned in 7 cells, with a `≥` prefix when `!is_exact && is_complete`, or when the dir is skipped (UX §5.1).
   - % of parent has 1 decimal. A parent of size 0 shows `0.0` (lead decision).
   - BAR is 10 cells in eighth steps.
   - Dirs end in `/`. Suffixes are `(scan skipped)`, `(permission denied)`, `(other filesystem)`, `(vanished during scan)` and `(read error: …)`. `(deleting…)` is rendered from the `DELETING` flag; it is wired in T021.
2. **State glyph precedence** `▲ > ⊘ > ◌ > !` (UX §5.1):
   - `◌` ⇔ `!is_complete`;
   - `▲` when the node is `AppState.priority` (the field is defined here and set in T020);
   - `⊘` for the `SkippedByUser` issue;
   - `!` for any other issue on that row only.
3. **Order:** children use `Model::children_sorted(id, mode)`, i.e. (size desc, name asc), in the active size mode.
4. **Aggregate row** (plan D5, UX-approved; `ux/spec.md` §3.2):
   - an expanded dir shows its 50 largest children, and the rest become one row, `(N smaller, SIZE)`. The threshold is a constant;
   - `→` on it reveals the next 50 in place. The reveal stays for that folder until it is collapsed;
   - aggregate rows can't be zoomed or deleted;
   - **the selected item never disappears into the aggregate.** If a live re-sort pushes it below the cut-off, it stays as its own row just above the aggregate, and N/SIZE exclude it. Once the selection moves away, it folds back into the aggregate.
5. **Keys** (UX §6, tree focus):
   - `↑↓ j k`, `PgUp PgDn`, `g G`;
   - `→ l`: expand, or move to the first child;
   - `← h`: collapse, or move to the parent;
   - `⏎`: zoom into the folder (it becomes the view root, and the header shows the breadcrumb `/home/nss › .cache`);
   - `⌫ -`: zoom out one level.

   Zoom-prioritise is T020's job. Expose a `ZoomChanged` app event for it now.
6. **Selection by NodeId** (design §3.4, UX F2): live re-sorting never changes the selected item, and the viewport keeps the selected row on the same screen line where it can. If the selected node is removed, the selection moves to its parent.
7. **Names are sanitized** (design §4 Security) in `src/ui/sanitize.rs`:
   - C0/C1 controls, ESC and DEL become `\xNN`;
   - invalid UTF-8 is decoded lossily for display only;
   - truncation with `…` is by display width (`unicode-width`).
8. **Empty folder:** `This folder is empty.`

## Design references
§3.4 (lazy sort, stable NodeId), §3.10, §4 Security. UX §3.1, §3.2, §4 F2, §5.1, §5.5, §6, §10 row 1.

## Files owned
`src/ui/tree.rs` (taken over), `src/ui/sanitize.rs`, `src/app/tree_state.rs`, `src/app/keymap.rs` (tree keys), `src/app/state.rs` (adds `priority`, `view_root`, `selection`), `src/ui/header.rs` (breadcrumb), `tests/ui_tree.rs`.

## Interfaces
**Consumes:** the `Model` read API (T003), `Glyphs`/`Theme`/`format` (T015). **Provides:** `TreeState { selection: NodeId, view_root: NodeId, expanded }` and the `ZoomChanged` event. T017 shares `selection`.

## Tests to write first (`tests/ui_tree.rs`, TestBackend 80×24 plus model-level assertions)
- **M3 sort:** with synthetic sizes, each level's rows render in (size desc, name asc) order. Equal sizes order by name.
- **M3 percent of parent:** after expanding two levels, each row's `%` equals `100 × child / parent` to 1 decimal at **that** level. After zooming into a child, the percentages are relative to the new parents, and the view root row shows `100.0`.
- **M3 keys:** expand/collapse/enter/leave by keyboard update the visible rows and their sizes.
- **Live:** sizes and percentages change as further `ScanMsg`s are applied, and the order re-sorts.
- **UX §10 selection follows item:** feed a shuffling stream, press `↓` 20 times between batches, and assert that the selected NodeId follows the expected sequence.
- **Markers:** each §5.1 state renders its glyph, prefix and suffix. The precedence holds when several apply.
- **Aggregate:** 120 children → 50 rows plus `(70 smaller, X)`. `→` on it shows 50 more, and the reveal survives live re-sorts until collapse.
- **Selected item never hidden (UX §3.2):** select the 50th child, then apply messages that grow 5 siblings past it → it still renders as its own row directly above `(70 smaller, …)`, `selection` is unchanged, and the aggregate's N and SIZE exclude it. Move the selection up → it folds back into the aggregate.
- **UX §11 check 11:** a 200-character name and a name containing ESC stay within the row width, and no raw control byte reaches the buffer.
- Snapshot (insta): the UX §3.1 scanning screen layout at 80×24 (tree part).

Command: `cargo test --test ui_tree`. To review snapshots: `cargo insta review`.

## Out of scope
The map (T017). `p`/`x`/`r` handling (T020). Delete (T021). The `a` toggle (T023).

## Amendment (2026-10-23, from the T015 review)
- **Wire the production scanner:** once T006 is merged, `app/mod.rs::production_starter` calls `scan::start` with `RealFs`. Then add the real-handle variant of T015's exit test: a full channel at `q y` → `run` returns within 2 s.
- **One sanitizer:** move `ui/format.rs::sanitize` into `src/ui/sanitize.rs` (the file this task owns) and use it everywhere. Don't keep two.
- After T005 fixes `Model::path(root)`, remove the UI's root-name workaround.
