# T015 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/main.rs`, `src/app/{mod,cli,state,event_loop,terminal,keymap}.rs`, `src/ui/{mod,copy,theme,format,status,header,dialog,tree}.rs`, `tests/{app_loop,app_cli}.rs`, `tests/support/synthetic.rs`.

## Evidence
- I wrote the implementation before the integration tests (a lapse against test-first), so there was no red run. I compensated with mutation checks, each reverted afterwards:
  - frame 0 drawn after the starter: `m1_frame_zero…` failed ("frame 0 must be on screen when the scanner starts");
  - first `q` quits: `quit_always_asks_first` failed;
  - scan messages drained before input: `input_is_handled_before_a_message_flood_is_applied` failed ("scan messages were applied before the pending key was read");
  - redraw cap removed: `data_redraws_are_capped…` failed ("data frames: 54");
  - two earlier versions of the cap and input-first tests survived their mutation. I rewrote them (a test-side `Feed` that fills the channel, and a producer thread with a test-driven clock) until they failed.
- Fresh: `cargo fmt --all -- --check` clean. Clippy on my targets (`--lib --bins --test app_loop --test app_cli`) clean. `cargo test --all-targets`: 80 lib + 7 `app_cli` + 13 `app_loop` pass, and the other suites pass. Doc tests ok. Release build ok.
- `cargo clippy --all-targets --all-features -- -D warnings` as a whole **fails in `tests/delete_executor.rs`** (unused `idle`, lines 611 and 625). It is not my file (T010), and I left it alone.
- `TARGET: first_frame_ms=0.5 (target 200)`; `TARGET: input_p95_ms=0.1–8.7 (target 50)`, depending on the run. The `app_loop` test's fake clock is simulated, and the first-frame and latency numbers are real wall time on a `TestBackend`, not a real terminal.
- I could not run `cargo run -- ~` interactively here (no tty), so the real terminal path (`CrosstermBackend`, raw mode, signals) is untested.

## Decisions and deviations
- The production starter starts nothing: `RealFs` doesn't implement `FsAccess` until T006, so `scan::start` can't be called yet. Wiring it is a one-line change in `app/mod.rs::production_starter`.
- Starting and Scanning status lines show no ETA text for the first 2 s (`◌ Starting…  00:00 elapsed`), then append `· ETA unknown (no previous scan)` (lead ruling; test `eta_text_appears_only_after_two_seconds_without_a_source`, failed first). T019 replaces them.
- `size_iec` always gives one decimal from K upward (`812.0K`). The spec's example shows `812K`. Please confirm.
- The root row shows the canonical path as its name; `Model::path(root)` adds a trailing slash, so the UI uses the model's root name instead.
- The CLI takes `--threads`/`--top`/`--cell-aspect` as strings and validates them myself, so I can report the first invalid flag in command-line order, as UX §5.5 requires. Default `--top` is 100.
- The real event source never blocks more than 250 ms, so SIGTERM/SIGHUP flags are seen promptly. The quit message on signals (T021) is not added.
- The `App` loop only redraws on data when `needs_draw` is set and 100 ms have passed since the last frame. Elapsed-second ticks use the same cap.
- Not built: spinner, `--no-animation` behavior, ETA text, "not scanned (other fs)" text, key hints that match real keys (the tree hints show the spec's text although most keys do nothing yet).
- `ScanOptions`, `ScanLaunch` and `AppState` are the interface for T016 onward. `App::state_mut()` is for later tasks' tests.

## Architect fix: panic hook (worker panics must not touch the terminal)
- `terminal::install_panic_hook_with(restore, slot)` records the installing (main) thread. A panic there restores the terminal first, then prints. A panic on any other thread only pushes its message into a `PanicSlot` and prints nothing, so the display stays intact.
- `App::new` takes the `PanicSlot`. Each step, a recorded worker panic (or, when a scan handle exists, all senders gone before Done) applies `Done{cancelled:true}` to the model (status `■ Stopped …`) and opens a `Scan error` dialog (`Dialog::ScanError`); any key closes it, `q` still opens the quit dialog.
- Failing first: `tests/app_panic.rs` (a single test, since the hook is process-global) failed with "terminal was restored for a worker panic"; `a_scan_worker_panic_shows_an_error_dialog_and_stops_the_scan` failed on the missing dialog. Both pass now.
- New file `tests/app_panic.rs`; not in the brief's list, so please confirm.
- The channel-disconnect path needs a real `ScanHandle` (T006) and is untested; today's tests cover the panic-slot path only.
