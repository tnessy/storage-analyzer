# T018 report (engineer-2)

Status: DONE

Files: `src/app/list_state.rs`, `src/ui/{tabs,types_view,largest_view,skipped_view,list_common}.rs` (new); `src/app/{keymap,state,tree_state,mod}.rs`, `src/ui/{mod,header,tree,treemap,format,copy}.rs`; `tests/ui_lists.rs`, `tests/support/synthetic.rs` (`Scan::with_top`), snapshots `tests/snapshots/ui_lists__*.snap` (15).

## Evidence
- Tests first with a stub (`View` enum and the `1`–`4` keys only): all 14 `ui_lists` tests failed on missing behaviour. After the implementation the first run left 4 failing, 3 of them my own wrong expectations (`5000 B` shown as `4.9K`, `mid` 15 B beating `one` 10 B, a left-truncation expectation) and 1 a real bug: keys arriving before the next frame found no list cursor, so `3` then `⏎` did nothing. Fixed by preparing the list when the view switches.
- Fresh: fmt, full clippy and `cargo test --all-targets` pass; doc tests and the release build ok.

## Behaviour
- Tabs: active tab in brackets and bold, `!N` badge on Skipped; keys `1`–`4`. Column header per view; `partial` right-aligned after a stop.
- Types: categories from `category_totals()` sorted by size in the active mode (columns size, files, % of root), `→` expands into extensions (`.ext`, `(none)`, `(other)`), `←` collapses or goes back to the category. `No files yet.` / `No files found.`.
- Largest: `top_files(mode)` cached and refetched only when `entries_seen` or the mode changes (at most the data-redraw rate); only the visible window is rendered; paths are sanitized and cut from the left with `…`. `⏎` calls `TreeState::reveal`: view root back to the scan root (with a `ZoomChanged` event if it was zoomed), ancestors expanded, selection set, view switched to the tree.
- Skipped: newest first, `path  (reason)` with the exact reason texts; `Nothing skipped.`
- Cursors follow their row by key (category/ext, node id, issue index) and keep their screen line under live updates. Movement keys `↑↓ j k PgUp PgDn g G`; list key hints `↑↓ move  ⏎ show in tree  d delete  ? help`.
- `truncate_left` and `strerror` moved into `ui::format` and are shared with the tree and the map.

## Concerns
- The Skipped row puts the reason after the path, in parentheses, so the left-truncated path never hides it; UX §F3 says only "path and reason".
- `r` (T020) and `d` (T021) are not wired. `d`/`r` hints are shown as in the spec.
- Types `%` is relative to the root's total size (directories included), not to the sum of files.
- The Largest cache is refetched whenever any entry arrived; with a very large `--top` this is O(top) per data frame.
