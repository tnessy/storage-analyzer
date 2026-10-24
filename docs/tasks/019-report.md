# T019 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/history/mod.rs`, `src/eta/sources.rs` (new), `src/eta/mod.rs`, `src/ui/{status,copy,theme,tile_colors}.rs`, `src/app/{event_loop,state,mod}.rs`, tests `tests/history.rs` (8), `tests/app_eta.rs` (22), `tests/app_loop.rs` (rig now uses `--no-animation`). `tile_colors.rs` only got a type annotation its tests needed once `serde_json` was in scope.

## Evidence (tests first)
- Written against stubs and seen failing before the code: all 7 failing `tests/history.rs` tests, both `eta::sources` unit tests, all 10 status-line tests (`app_eta`), and 10 of the 12 app-level wiring tests.
- The 2 wiring tests that passed on stubs are negative ones (`without_history_nothing_is_read_or_written`, `a_cancelled_scan_or_one_with_user_skips_writes_nothing`); per the new rule I broke the code once per criterion and watched the test fail: read without `--history` (zero-write test fails), a cancelled scan counts, user skips count, a subtree scan counts (this one needed a stronger test: the record is now marked so an overwrite shows), a load that rewrites a corrupt file. Each mutation was reverted.
- Fresh: fmt, full clippy, `cargo test --all-targets` (lib 104, app_eta 22, history 8, ui_treemap 37, ui_tree 31, rest unchanged), doc tests, release build all pass.

## Behaviour
- History (opt-in only): `HistoryStore` trait, `FileHistory` (atomic temp + `rename`, no temp left, directory created), `state_dir`, `upsert` (replaces the same root+options, newest last, 50 roots), `lookup`. Corrupt file → `Corrupt` error, ignored with the message `History file ignored: it could not be read.`, not touched until the next save. Read on its own `history-load` thread after frame 0; the save on a complete, unskipped `Done`, merged into the current file.
- `EtaSources::best()`: History > SessionPrevious > FsInodes; `inode_total` (mount root and `f_files > 0`), `StatvfsInodes` (read-only `statvfs`/`stat`) behind the `InodeSource` trait. `Services { history, inodes }` is injected with `App::with_services`.
- `AppState`: `rate`, `eta_sources`, `session_previous`, `scan_finished(ScanKind)`, `use_session_previous()`, `eta_view()`, `spinner_glyph()`. A root scan finished without cancel or user skips records the in-session total; subtree scans never use or update it.
- Status line per UX §5.2: Indeterminate line with the exact drop order, the Eta line with the 16/10-cell bar, elapsed and bar drops, past-estimate, `ETA estimating…` only with a source, nothing about ETA for the first 2 s. Labels `last scan` / `filesystem`. The spinner turns at 10 fps (`◜◝◞◟`), still `◌` with `--no-animation`, and the loop redraws on frame changes only while scanning.

## Concerns / for the lead
- The spec says the 80-column Indeterminate example is 79 cells; the string it gives is 78 cells. The tests use the string.
- `r` on the root (T020) must call `AppState::use_session_previous()` when it restarts the root scan, and call `scan_finished(ScanKind::Subtree)` (or nothing) for subtree rescans; `scan_finished(Root)` is called from `tick` when the first scan ends. A rescan's own Done will need `scan_finished(Root)` again from T020 since `finished_at` is already set.
- The history save runs synchronously on the loop thread (one small file); an unusually slow disk could hitch one frame.
- The `▲ First:` prefix and the Stopping state are T020's; the drop order is implemented per field list, not yet exercised with a prefix.
- Bar uses `░` for the empty part (new glyph `bar_empty`), no brackets around the bar.
