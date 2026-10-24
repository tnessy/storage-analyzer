# T026: Tile colour assignment (option C: files by type, folders by identity)

- **Engineer:** engineer-2.
- **Wave:** 2.
- **Depends on:** T025 (merged).
- **Schedule:** **before T022 if T022 hasn't started; otherwise right after T022.** Either way, before T019.
- **Source:** the user found T025's same-hue alternate shades too similar and chose **option C** (2026-10-23). UX specified it in `ux/spec.md` §3.1 (mockup), §3.3, §7 ("Tile colours: option C"), §11 checks 3 and 17 (a)–(k). **The spec is the source of truth.** Ask Heimdall, who relays to UX, if anything is unclear.
- **Replaces** T025's alternate-shade scheme, including its tests for the old check 17.

## Goal
Within one map view, a colour means exactly one thing: a file type, or "just this folder". Neighbouring folders get different colours, and colours stay stable while sizes change.

## Acceptance criteria

### 1. Palette (`theme.rs`)
- **Delete** the alternate-shade table.
- **Add** the §7 palette with both truecolor and **xterm-256** values:

  | Hue | Truecolor | xterm-256 | Used for |
  |---|---|---|---|
  | yellow | #F0E442 | 221 | `[C]` |
  | blue | #0072B2 | 25 | `[B]`, white label |
  | orange | #E69F00 | 178 | `[V]` |
  | bluish green | #009E73 | 35 | `[A]` |
  | sky blue | #56B4E9 | 74 | `[I]` |
  | vermillion | #D55E00 | 166 | `[D]` |
  | reddish purple | #CC79A7 | 175 | `[O]` |
  | grey | #BBBBBB | 250 | aggregate only |

- Labels are black, except white on blue.
- The **folder order** is the table order (yellow → reddish purple), never grey.
- The app keeps rendering truecolor. Colour-depth detection is T023's job, but put the 256 values in the theme **now**, so check 3 can test them.

### 2. Pure assignment function (new, e.g. `src/ui/tile_colors.rs`)
`assign(tiles, adjacency, kinds, reserved, memory) -> (fills, memory')`, where each fill is a hue or `Unfilled`. Process the tiles in layout order (size desc, ties by NodeId). The *neighbours* are tiles already processed in this pass that share a border segment, files included.
1. **File tile:** its type hue. **Aggregate:** grey.
2. **Folder tile:** the free hues are the folder-order hues not in `reserved`.
   1. **Keep** the remembered hue if it is free and no neighbour uses it.
   2. **Otherwise pick** the first free hue in folder order, **starting at `NodeId mod 7`** and skipping reserved hues, that no neighbour uses. Remember it.
   3. **If every free hue is used by a neighbour,** take the free hue used least among the neighbours, lowest order on a tie.
   4. **No free hues:** `Unfilled`.

It is pure and deterministic, with no terminal and no model access. Compute adjacency from the snapped tile rects (a shared border segment of length ≥ 1).

### 3. Reserved set and memory (`AppState`)
- **`reserved: HashMap<NodeId /*scope folder*/, HueSet>`:** the hues of every category with at least one **direct file child** of that scope folder, including small files without a tile.
  - Built when the scope is first shown. Grows as new direct files appear: scan the scope's direct children at each relayout and union them in, never removing.
  - A rescan of that folder (`begin_rescan`) rebuilds it. A delete does not shrink it.
- **`folder_colors: HashMap<NodeId, Hue>`:** session-long memory, passed into `assign` and stored back.
- Both live in `AppState`. They are **not** in the model, because they are presentation state.
- **Bounded:** they grow only with folders and scope folders actually shown.

### 4. Rendering (`treemap.rs`)
- Fills come from `assign`. `Unfilled` folder tiles use the default background, as under NO_COLOR.
- The frame (grout) stays on default colours (T025).
- **Labels:**
  - **folder tiles:** `name/ size pct%`, with **no `[T]` tag**;
  - **file tiles:** `name size pct% [T]`.
- The selected-tile styling from T025 is unchanged.
- **Stability:** the assignment runs at relayout (≤ 2 Hz). Size changes and re-ranking alone never recolour a folder that keeps its adjacency. A folder recolours only when its hue is newly reserved or it would touch a same-coloured neighbour.
- **Model API note:** `Model::category_of_node` for **folders** no longer drives map fills. If no caller remains after this task, report it to Heimdall (engineer-1 owns the model) instead of deleting it.

### 5. Types view legend (`types_view.rs`)
A **2-cell colour swatch** before each category row, in truecolor (and 256 once T023 lands). Under NO_COLOR only the tag shows.

### 6. Help text
The help overlay is built in T023. Add the two help lines from §7 to `copy.rs` **now**, as constants T023 will render:
- `Map: files are coloured by type (tag shows it, e.g. [V] video). Folders end in / and get their own colour,`
- `never one used by a file type in the same view.`

## Files owned
- `src/ui/theme.rs`, `src/ui/treemap.rs`, `src/ui/tile_colors.rs` (new), `src/ui/types_view.rs`, `src/ui/copy.rs`
- `src/app/state.rs`, `src/app/map_state.rs`
- `tests/ui_treemap.rs`, `tests/ui_lists.rs`, `tests/snapshots/**` (regenerate and review the diffs)

## Tests to write first (each seen failing; include the output)
- **Check 17 (a)–(k), as written in `ux/spec.md` §11.** Use unit tests on `assign` where possible, plus TestBackend checks for (b), (f), (g), (h), (i) and (j). Remove or replace the T025 tests for the old shade scheme.
  - **(d) stability:** 20 relayouts with changing sizes and fixed adjacency recolour nothing. A new file type in the scope recolours only the folders that used that hue, once.
  - **(e) zoom:** in and out keeps every folder's colour, because the memory persists.
  - **(b):** run in **both** truecolor and 256 (render with the 256 palette through a test-only theme switch).
- **Check 3:** contrast ≥ 4.5 for all 8 fills in both columns. Also assert the spec's minimums: ≥ 5.19 truecolor, ≥ 5.53 for 256.
- **Reserved set:** a small file with no tile still reserves its hue. R never shrinks on delete (simulate a removed node), and it is rebuilt by `begin_rescan`.
- **`assign` start index:** with no memory and no reservations, a folder with NodeId 9 starts at hue `9 mod 7 = 2` (orange).
- **Types legend:** the swatch cells have the category's bg. Under NO_COLOR there is no swatch.

**Commands:** the five standard ones, plus `cargo insta review`.

## Out of scope
Colour-depth detection and NO_COLOR reading (T023; until then, test NO_COLOR through the theme's mono mode). The help overlay itself (T023).
