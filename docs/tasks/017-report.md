# T017 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/ui/treemap.rs`, `src/app/map_state.rs` (new); `src/ui/{theme,copy,mod,header,tree}.rs`, `src/app/{keymap,state,event_loop,mod}.rs`; `tests/ui_treemap.rs`, `tests/support/synthetic.rs` (`Item::bytes`), `tests/ui_tree.rs` (helper follows the new `prepare_view(Rect)`), snapshots `tests/snapshots/ui_treemap__*.snap`.

## Evidence
- Tests first against a stub `MapState` (no tiles): 21 of 22 tests failed on behaviour; `enter_and_backspace_zoom_from_the_map_like_the_tree` passed trivially because it only exercises the tree's zoom.
- Mutation: setting the relayout interval to zero made the throttle test fail (`relayouts: 41`); restored.
- Fresh: fmt, full clippy (`--all-targets --all-features -D warnings`) and `cargo test --all-targets` pass (ui_treemap 22, ui_tree 20, app_loop 19, lib 90, others unchanged).

## Behaviour
- Scope: the selected item's siblings; the view root's children when the view root is selected; the folder's own children when the tree cursor is on its `(N smaller)` row, with the aggregate tile double-bordered (and the tree selection still the folder). Moving onto the aggregate tile from the map sets `map.on_aggregate` and leaves the tree selection alone.
- Tiles via `layout::squarify`, single border, double on the selected tile, label `name size pct% [T]` in the top border and the rest on a second line, category fill and tag from `Model::category_of_node` and `Theme::category`, grey `(+N smaller)`. Tiles under 6 cells wide get no label; `!`/`⊘` markers still show (at the tile centre when there is no label).
- Relayout: cached by scope, area, size mode and aspect; while scanning a data change relayouts at most every 500 ms; a scope or area change relayouts at once; the highlight is drawn from the cache every frame.
- Hidden items: notice on the map rule per UX §3.3 (forms, selected-hidden forms, fit order, no notice when an aggregate tile is drawn, zero-byte items not counted).
- Focus: `Tab`/`Shift-Tab`, `*` on the focused pane's title (column header for the tree, `Map:` rule for the map), map hints, arrows/`hjkl` via `layout::neighbor`, `⏎`/`⌫` as in the tree.
- Empty states: `Waiting for first results…`, `This folder is empty.`, `N items, all 0 B. Nothing to map.`
- `AppState::prepare_view` now takes the screen `Rect` and prepares both panes; `AppState.now` carries the loop clock for the throttle.

## Concerns
- The T014 ±1-edge bound helper is not `pub(crate)`; the proportionality test recomputes the same bound in the test file.
- NO_COLOR and the 256-colour mapping are T023's. Fills are truecolor RGB only.
- The map reads the model's category per tile each relayout-free frame (`category_of_node` is cached in the model for dirs); not measured.
- A marker on a labelled tile overwrites one border cell near the top-right corner.
