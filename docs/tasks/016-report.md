# T016 report (engineer-2)

Status: DONE_WITH_CONCERNS

## Files
`src/app/tree_state.rs` (new), `src/app/{keymap,state,event_loop,mod}.rs`, `src/ui/{tree,sanitize,header,theme,copy,format,mod,status,dialog}.rs`, `tests/ui_tree.rs`, `tests/app_loop.rs` (real-scanner test), `tests/support/synthetic.rs` (`Scan` builder), `tests/snapshots/ui_tree__scanning_screen_layout_snapshot.snap`.

## Evidence (tests first)
- `tests/ui_tree.rs` and the sanitizer unit tests were written against stubs first: 14 of 15 tree tests and 3 of 3 sanitizer tests failed on behaviour (empty rows, no escaping) before any implementation. `zoom_ignores_files_and_the_view_root` passed on the stubs because it asserts "nothing happens".
- Real scanner: `quitting_during_a_real_scan_with_a_full_channel_returns_promptly` failed with "the real scanner produced nothing" against the no-op starter; passes with `scan::start(RealFs)` wired in `production_starter`. It runs 3,000 directories, lets the walker fill the channel, then `q y` → `Flow::Quit`, `finish()` joins in < 2 s.
- Mutation: removing the line-keeping logic from the viewport made `live_changes_keep_the_selected_row_on_the_same_screen_line` fail ("eight moves down put the cursor on line 8, left 0"). A first version of that test survived the mutation, so I added the absolute-line assertion.
- Fresh: fmt clean; `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo test --all-targets` all pass (lib 90, app_cli 7, app_loop 19, app_panic 1, ui_tree 17, others unchanged); doc tests ok; release build ok.

## What was built
- `TreeState { selection, aggregate, view_root, expanded }` with the row builder (sort via `children_sorted`, 50-child pages, aggregate row, the selected item pinned just above the aggregate and excluded from its N/SIZE), cursor and viewport logic (selection by `NodeId`, the cursor keeps its screen line under live changes, a removed selection falls back to its nearest surviving ancestor), keys `↑↓ jk PgUp PgDn g G → l ← h ⏎ ⌫ -`, and `AppEvent::ZoomChanged` in `AppState.events` for T020.
- Row rendering per UX §3.2/§5.1: `≥` for complete-but-inexact and skipped dirs, glyph precedence `▲ ⊘ ◌ !`, suffixes, `0.0` for a zero-size parent, `This folder is empty.`, header breadcrumb `root › a › b`.
- One sanitizer: `ui/sanitize.rs` (`sanitize`, `display_name`): controls (C0, DEL, C1, ESC) become `\xNN`, invalid UTF-8 is decoded lossily. `format::sanitize` was deleted; all callers use the new one. Truncation by display width stays in `format::truncate`.
- `production_starter` is public and starts the real walker. `App::finish()` was split out of `run()` so a test can step the loop and then clean up.

## Concerns / deviations
- The root-name workaround stays: `Model::path(root)` still adds a trailing slash until T005 lands. The UI uses the model's root name (`ui::root_label`).
- Tree guides (`├─ └─ │`) are not drawn; rows are indented two spaces per level with a `▾/▸` twisty. The snapshot reflects that.
- `(deleting…)` is rendered from `Flags::DELETING` but cannot be exercised until T011 sets it; no test covers it.
- `PgUp/PgDn` move by pane height − 1. The aggregate row is a cursor stop with `selection` set to its folder and `tree.aggregate = Some(dir)`; T017 should treat that as "folder selected".
- A tree with a very large expanded folder rebuilds and sorts its children on every key and frame (the 50-row cut is applied after sorting). Fine at today's sizes; measure with R2.

## Review fixes (tests first)
1. Workaround removed: `ui::root_label` now sanitizes `Model::path(root)`. `the_root_row_header_and_breadcrumb_show_the_model_path_exactly` uses a model whose root name (`short`) differs from its path (`/synthetic/real`), so it failed first (the old code printed the name); it also checks the header, the root row, the breadcrumb and that no `real/` trailing slash appears.
2. Tree guides per UX §3.1: `├─▾ dir/`, `├── file`, `│  ├── …`, `└──` for the last sibling and for the `(N smaller, SIZE)` row (always last, the pinned selected row sits above it). `Row` carries `last` and a `trail` bit set; glyphs are `Glyphs::{guide_tee, guide_elbow, guide_pipe}` (plus `rule`), drawn in `Theme::dim`. The root/view-root row has no guide. Tests `guides_follow_the_two_level_pattern_of_the_spec` and `the_aggregate_row_is_the_last_branch` failed first; the snapshot was regenerated and now matches the mockup's shape.
3. Fresh: fmt, full clippy, `cargo test --all-targets` and the release build are clean/green (ui_tree 20).
