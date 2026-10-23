# T025: Selection styling, neighbour tile shades, `..` parent row (from user feedback on the first build)

- **Engineer:** engineer-2.
- **Wave:** 2.
- **Depends on:** T016, T017, T018 (all merged).
- **Schedule:** **next**, before T022 and T019.
- **Source:** the user ran the build ("layout looks pretty good") and asked for 3 changes. UX specified them in `ux/spec.md` §3.2.1, §3.2.2, §3.3, §7, the key table, §5.4 messages and §11 checks 16–18. **The spec is the source of truth. If anything is unclear, ask Heimdall, who relays to UX; don't guess.**

## Goal
1. A selection you can't miss, shown in exactly one pane, attribute-based so it is identical under NO_COLOR.
2. Same-category neighbouring tiles that stay distinguishable.
3. A `..` row to go up a level, with `⌫` returning the selection to the folder you came from.

## Acceptance criteria

### 1. Selection (§3.2.1, §3.3)
- **Tree focused:** the selected row is **REVERSED + BOLD across the full pane width**, every cell from column 0 to the right edge, with **no DIM cell** (guides, bar and suffix take the row style). `>` stays in column 1.
- **Map focused:** the selected tree row is **BOLD, with the name cells UNDERLINED**, plus `>`, and is **not** reversed.
- **Selected map tile, whatever the focus:**
  - double border;
  - its **whole frame** (border cells plus the title row) is REVERSED + BOLD;
  - the title starts with `>`;
  - the interior keeps its category fill.
- **No color dependence:** with `NO_COLOR=1` all of the above is identical, because it is attribute-based. **Rows in reverse video never use DIM** (§7).
- The focused pane keeps `*` before its title (unchanged).

### 2. Tile frame ("grout") and alternate shades (§3.3, §7)
- **Every tile's border and title row** are drawn on the terminal's **default background and foreground**. Only the **interior** is filled.
- **Theme:** add the 8 **alternate** fills with their label colours to `theme.rs`, exactly as in the §7 "Alternate shades" table. `[D]` alternate is #8A3D00 with **white** text, and aggregate alternate is #969696.
- **Assignment (deterministic):** walk the tiles in layout order (size desc, ties by NodeId). A tile takes the **alternate** shade if an already-coloured tile **shares a border segment** with it, has the same category, and uses the base shade; otherwise it takes the base.
- **Category** is still shown by the `[T]` tag, never by shade.
- NO_COLOR: no fills, so shades don't apply.

### 3. `..` row (§3.2.2)
- **When it shows:** only when the view root ≠ the scan root. It is row 0, above the view root row.
- **Look:**
  - SIZE, %, BAR and STATE are blank;
  - the name is `..  (up to <parent path>)`, with the path truncated from the **left** with `…`;
  - no guides.
- **Pinned:** never sorted, never in the 50-largest page or the aggregate, never in Types, Largest or the map. It scrolls as row 0, and `g` goes to it.
- **Selection:**
  - after a zoom, the selection is on the **view root row** (row 1);
  - `↑` on `..` does nothing;
  - `..` takes the §3.2.1 selection styling.
- **Keys on `..`:**
  - `⏎`, `⌫`, `-`, `←` and `h` go up one level;
  - `→` and `l` do nothing;
  - `d`, `p`, `x` and `r` show `".." is the parent folder. Press ⏎ to go up.` (copy constant) and change nothing: no `DeleteRequest`, no `ScanControl` change;
  - `Tab` moves focus to the map and **selects the largest tile**.
- **`⌫` everywhere (behaviour change):** after going up a level, the **selection is the folder you came from**, not the top row. Going up to the scan root still emits the `ZoomChanged` event (T020 clears the priority on it).
- **Map while `..` is selected:**
  - it shows the **view root's children**, the same tiles as with the view root row selected;
  - **no tile is highlighted**;
  - the title appends ` · ⏎ goes up to <parent>`, truncated **before** the hidden-items notice is shortened;
  - the `selected hidden` notice never fires for `..`.

**Key routing for later tasks:** `d` (T021) and `p`/`x`/`r` (T020) aren't wired yet. Put the `..` interception **in front of** those keys in `keymap.rs` now. T020 and T021 must keep it first (noted in their briefs).

## Design references
UX spec §3.2, §3.2.1, §3.2.2, §3.3, §5.4, §6 key table, §7, §11 checks 16–18. Design §3.7 (map scope), §3.9.

## Files owned
- `src/ui/theme.rs`, `src/ui/tree.rs`, `src/ui/treemap.rs`, `src/ui/copy.rs`
- `src/app/tree_state.rs`, `src/app/map_state.rs`, `src/app/keymap.rs`, `src/app/state.rs`
- `tests/ui_tree.rs`, `tests/ui_treemap.rs`, `tests/snapshots/**` (regenerate the affected snapshots and review each diff)

## Tests to write first (each must be seen failing; include the output in your report)
- **Check 16:**
  - tree focused: every cell of the selected row (column 0 to the pane edge) has REVERSED and BOLD, and none has DIM;
  - map focused: the selected row has BOLD, has UNDERLINED on the name cells, and has no REVERSED;
  - in both cases, every frame cell of the selected tile has REVERSED and the title starts with `>`;
  - repeat with `NO_COLOR=1`, with identical attributes.
- **Check 17:**
  - a fixture with 6 sibling folders of one category laid out in one row → no two tiles sharing a border segment have the same fill (truecolor);
  - every frame cell (border plus title) has the default bg;
  - every fill/label pair in both §7 tables has contrast ≥ 4.5 (the alternates are also ≥ 7:1 per UX; assert ≥ 7 for those);
  - assignment is deterministic: the same input gives the same shades.
- **Check 18:**
  - `..` is absent at the scan root;
  - after `⏎` on a folder, row 0 is `..` and the selection is row 1;
  - `⏎` on `..` gives the same state as `⌫`, and the selection becomes the folder you left;
  - `⌫` from a deep zoom selects the folder you left, at each level;
  - `d`, `p`, `x` and `r` on `..` show the message, with no `DeleteRequest` and no `ScanControl` change;
  - with 60 children, `..` is not inside the aggregate;
  - with `..` selected, the map shows the same tiles as with the view root selected, no tile is highlighted, and the title has ` · ⏎ goes up to …`;
  - `Tab` from `..` focuses the map with the largest tile selected;
  - `↑` on `..` does nothing, and `g` lands on `..`.

**Commands:** the five standard ones (`docs/tasks/README.md`), plus `cargo insta review` for the snapshots.

## Out of scope
256-colour mapping and mono fallbacks for the new shades (T023 adds the alternates to its 256-colour contrast check). `p`/`x`/`r`/`d` behaviour beyond the `..` interception.
