# T015: App skeleton (CLI, pre-TUI checks, terminal, event loop, frame 0, elapsed, idle quit)

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T001, T003 (and T006 for the real scanner; until it lands, test against a fake scanner start function). **Blocks:** T016 and every later UI task.

## Goal
The first usable build and the proof of brief M1:
- frame 0 within 1 s;
- input handled first;
- the UI never waits on the scan;
- elapsed time updates at ≥ 1 Hz (M2);
- quit always asks first (M12, idle variant).

Acceptance criteria:
1. **CLI** (design §4): `PATH` (default `.`), `-x/--one-file-system` (the default), `--cross-filesystems`, `--apparent`, `--threads N`, `--top N`, `--history`, `--no-animation`, `--cell-aspect F`. Parse them all now; tasks that need a flag read it from `AppConfig`. Validate `--cell-aspect` as finite and in 0.5–4.0, and `--threads`/`--top` as ≥ 1. An invalid value is a usage error with exit code 2 and no TUI (T014 review). The name in help and messages comes from `env!("CARGO_BIN_NAME")`; `APP_ID` is used for paths.
2. **Pre-TUI** (UX §5.5 "Pre-TUI errors"): `statx` and open the root dir fd before the terminal is touched. Errors print `<bin>: <path>: no such file or directory | not a directory | permission denied` to stderr and exit with code 2. No TUI is drawn.
3. **Terminal:** raw mode + alternate screen, with a guard that restores the terminal on drop. A **panic hook restores the terminal before printing** (design §4). SIGTERM/SIGHUP (signal-hook flag) restore the terminal and exit; T021 adds the delete-interrupt message.
4. **Frame 0 before anything else** (design §3.3):
   - Render the header, tabs, the root row with `◌`, the map area `Waiting for first results…`, and the status line `◌ Starting…  00:00 elapsed`. T019 decides whether `ETA estimating…` is appended (only when a source exists; see the T019 provisional rule).
   - Only **then** spawn the scanner (and, later, the opt-in history read in T019).
5. **Event loop** (ADR-003):
   1. Poll input with a timeout until the next tick.
   2. Handle **all** pending input first.
   3. Drain `ScanMsg` into `Model::apply` within an 8 ms budget (a constant).
   4. Render: data-driven redraws at most every 100 ms; input-driven redraws immediately, with all input in that iteration coalesced into one frame.

   When nothing changes, block on input without ticking (but keep the ≥ 1 Hz elapsed tick while scanning). The scan channel is a bounded `sync_channel(1024)`.
6. **Testable seams:**
   - `App` is generic over the ratatui `Backend`, an `EventSource` trait (crossterm in production, a scripted queue in tests), a `Clock` trait, and a `ScanStarter` closure. Tests run the full loop headlessly with `TestBackend`.
   - `tests/support/synthetic.rs` provides synthetic `ScanMsg` stream builders for every engineer-2 test file.
7. **Status line** (UX §5.2): Starting / Scanning (no ETA yet; T019 adds it) / Done `✓ Done in MM:SS · N files · X GiB` / Stopped. Elapsed is shown as `MM:SS` (`H:MM:SS` past an hour) and updates at ≥ 1 Hz. A **transient message** mechanism shows a message for 5 s or until the next key, then the state text returns. Later tasks reuse it.
8. **Quit dialog, idle/scanning variant** (UX §5.3 "Quit: no delete running", F6):
   - `q`/`Ctrl-C` open it; the first press never exits;
   - a second `q`/`Ctrl-C`, or `y`, quits;
   - `n`/`Esc`/`Enter` stay;
   - other keys are ignored.

   The title uses the binary name. The delete-running variant comes in T021.
9. **Theme and glyph structure:**
   - `src/ui/theme.rs` holds a `Glyphs` struct (`◌ ▲ ⊘ ≥ ▾ ▸ █…`, borders) and a `Theme` struct. Every later UI task uses them, and no glyph literals appear elsewhere. T023 adds the ASCII and colour-depth variants.
   - All copy goes in `src/ui/copy.rs`.
10. The main view region shows only the root row in this task: size, `◌` and the name, through a minimal `src/ui/tree.rs`. T016 takes `tree.rs` over.

## Design references
§3.3 (loop, redraw cap, first paint), §4 CLI/Error handling/State/Testing (M1, quit), §5 budget. ADR-003. UX §3 anatomy, §4 F1/F6, §5.2, §5.3 quit, §5.5.

## Files owned
- `src/main.rs` (taken over), `src/app/mod.rs`, `src/app/cli.rs`, `src/app/state.rs`, `src/app/event_loop.rs`, `src/app/terminal.rs`, `src/app/keymap.rs`
- `src/ui/mod.rs`, `src/ui/copy.rs`, `src/ui/theme.rs`, `src/ui/format.rs`, `src/ui/status.rs`, `src/ui/header.rs`, `src/ui/dialog.rs` (shared dialog frame: centered, 64 wide or width−4, double border, title on the border), `src/ui/tree.rs` (minimal)
- `tests/app_loop.rs`, `tests/app_cli.rs`, `tests/support/synthetic.rs`

## Interfaces
**Consumes:** `Model` (T003), `scan::start` (T006), `ScanControl` (T001). **Provides:** `App`, `AppState`, `EventSource`, `Clock`, transient messages, `Glyphs`/`Theme`, the dialog frame, `format::{size_iec, elapsed}`.

## Tests to write first
- **M1 first frame (gate):** run `App` headless with a scanner starter that records when it was called. Frame 0 is drawn **before** the starter runs, **within 1 s** of `App::run` start (assert ≤ 1,000 ms), and it shows the root row with `◌` and `Waiting for first results…`. Also print `TARGET: first_frame_ms=… (target 200)`.
- **M1 input first:**
  - pre-fill the channel with 200 k synthetic `Listing`s plus one `↓`/`q` key;
  - the key's effect appears in the first rendered frame after it is read, and no iteration applies messages for longer than budget + one message;
  - measure key-to-frame latency over 1,000 keys during a flood and print `TARGET: input_p95_ms=… (target 50)`. This does **not** assert on p95 (plan §1.1).
- **M1 never waits:** with a scanner that sends nothing for 5 s, keys are still handled and frames are drawn.
- **M2 elapsed:** with a fake clock, the elapsed text changes at least once in every 1,000 ms window of a 10 s synthetic run (UX §11 check 5).
- **Redraw cap:** under a constant message flood with no input, ≤ 10 data frames per simulated second.
- **Quit (M12, idle):** the first `q` never exits. `q q` exits. `q y` exits. `q n`, `q Esc` and `q Enter` stay. `Ctrl-C` behaves like `q`.
- **Pre-TUI errors:** a missing path, a file, and a `chmod 000` dir each give the exact stderr text and exit code 2, and no alternate-screen escape is written (assert on captured stdout). Spawn the built binary with `assert_cmd`, via `env!("CARGO_BIN_EXE_storage-analyzer")`.
- **Panic hook:** a test-only panic inside render leaves the terminal restored (the guard's drop is called; assert through a flag on the test backend wrapper).
- **Done/Stopped** status text matches UX §5.2.

Commands: `cargo test --test app_loop --test app_cli`, then `cargo run -- ~` to look at it yourself.

## Out of scope
- Tree navigation (T016), map (T017), other views (T018), ETA text (T019), scan control keys (T020), delete (T021), help/too-small/NO_COLOR (T023).
- The Types side panel at ≥ 120 columns is **deferred**: do not build it.
