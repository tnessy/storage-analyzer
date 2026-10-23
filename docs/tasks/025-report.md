# T025 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/ui/{theme,tree,treemap,copy,list_common}.rs`, `src/app/{tree_state,map_state,keymap,state}.rs`, tests `tests/ui_tree.rs` (+10), `tests/ui_treemap.rs` (+7). Existing snapshots are unchanged: the new styling is attributes and colors, which the text snapshots don't carry.

## Evidence (tests first; failing run before any implementation)
Against stubs (`on_parent` always false, alternate shade = base shade, old styles), these failed on behaviour:
- Check 16, tree: `tree_focus_selection_is_reverse_bold_across_the_pane_and_never_dim`, `map_focus_selection_is_bold_with_the_name_underlined_and_not_reversed`; tile frame: `the_selected_tile_frame_is_reverse_bold_with_a_marked_title_and_keeps_its_fill`.
- Check 17: `tile_frames_use_the_default_background_and_only_the_interior_is_filled`, `same_category_neighbours_get_different_fills`, `every_fill_and_label_pair_meets_the_contrast_floor` (`assert_ne!(fill, alt_fill)` and the §7 values).
- Check 18: 9 of the 10 new `..` tests in `ui_tree` plus `with_the_parent_row_selected_…`, `tab_from_the_parent_row_…` (10 + 6 = 16 of the 17 new tests failed). `backspace_from_a_deep_zoom_…` already passed: T016 already selected the folder you came from. `the_selected_hidden_notice_never_fires_for_the_parent_row` passed on the stub for the same reason.
- Fresh: fmt, full clippy, `cargo test --all-targets` (ui_tree 31, ui_treemap 29, ui_lists 14, app_loop 19, lib 92, rest unchanged), doc tests, release build all pass.

## What was built
- Selection: tree focus → whole row REVERSED+BOLD, no DIM (guides take the row style); map focus → BOLD with the name UNDERLINED, not reversed. Lists follow the same rule. Selected tile: double border, frame (borders and title row) REVERSED+BOLD, title starts with `>`, interior keeps its fill.
- Grout: tile border and title row use the default colors; only the interior is filled (`Block::new().style` on the inner area).
- Shades: base and alternate fills with label colors exactly as in UX §7 (checked: bases ≥ 4.5:1, alternates ≥ 7:1); `MapState` assigns them at relayout, walking tiles in layout order: alternate when an earlier base-shade tile of the same category shares a border segment.
- `..` row: `Row::Parent` pinned at row 0 when zoomed, blank size/%/bar/state columns, `..  (up to <parent>)` with the path cut from the left, `TreeState.on_parent`. `↑` on it does nothing, `g` goes to it, `⏎ ⌫ - ← h` go up and select the folder you left, `→ l` do nothing, `d p x r` show `".." is the parent folder. Press ⏎ to go up.` (new `Action::{Delete, Prioritise, Skip, Resume}` in `keymap.rs`, intercepted in `state.rs` ahead of any later handling), `Tab` focuses the map and selects the largest child. Map: same tiles as the view root row, none highlighted, title appends ` · ⏎ goes up to <parent>` (cut first, before the notice is shortened), `selected hidden` never fires.

## Concerns
- NO_COLOR is not read anywhere yet (T023). The new styling is attribute-based, so it is already identical under NO_COLOR; the tests assert modifiers only and do not set the variable.
- Where a tile is too small for a frame (under 2 cells) the selected tile is drawn reversed+bold on its fill instead.
- The grout makes every unselected frame the terminal's default foreground, so on a dark theme the frames are light on dark; this is what the spec asks for.
