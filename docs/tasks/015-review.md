# T015 review (Heimdall, 2026-10-23): CHANGES REQUESTED (small)

**Fresh evidence** (clean worktree: HEAD plus only the T015 files):
- fmt ok;
- **whole-tree** `clippy --all-targets -D warnings` is clean. The `delete_executor` warnings in the report were already fixed in the committed T010;
- lib 81, app_cli 7, app_loop 15, app_panic 1, and every other suite pass;
- doc tests ok, release build ok;
- the write guard passes with `src/ui/copy.rs` present (the T024 follow-up).

**Real-terminal smoke test (by me, under a pseudo-tty via `script`, 80×24):**
- frame 0 shows `◌ Starting…  00:00 elapsed` and `Waiting for first results…`;
- `q y` exits 0, and the alternate screen is entered and left;
- SIGTERM ends the process, and the alternate screen is left.

So the crossterm path works.

**Report's "conflict": not a conflict.** UX §5.2 (resolved): Starting has no ETA text for the first 2 s, then the Indeterminate line ends with `ETA unknown (no previous scan)`. That is what T015 does, and `eta_text_appears_only_after_two_seconds_without_a_source` pins it. The size format `812.0K` matches UX §7.1 (compact size, always 1 decimal from K up).

**Process:** this is the second test-first lapse on the team (after T004). I accept the mutation evidence this time; four mutations were caught, and two weak tests were rewritten until they bit. **Next time, missing red-first evidence means rejection.**

**Required changes:**
1. **Exit order and its test (T015 amendment, design §3.8 "Handle lifecycle"):**
   - `App::run` drops `rx` and then calls `shutdown()`, but it never calls `ScanControl::cancel_all()` first (step 1 of the design's order). Add it.
   - The required red-first test is missing: a scanner flooding a full `sync_channel(1024)` at `q y` → `run` returns within 2 s. Run it again through the terminate-flag (SIGTERM) path.

   Use a fake `ScanHandle`-like starter whose workers block on `send`. If the contract's `ScanHandle` can't be faked before T006, test it at the `App` level with a producer thread blocked on a full channel, and add the real-handle variant in T016.
2. **Panic attribution (`check_scan_health`):**
   - Every message in `PanicSlot` currently stops the scan and opens `Scan error`. A **caught delete-executor panic** (T010 F2, thread `delete`) would wrongly stop the scan.
   - **Fix:** record `(thread name, message)`. Only threads named `scan-*` count as scan failures. T006 is told to name its workers `scan-<i>`. Other threads' messages stay in the slot for T021's result dialog.
   - **Test:** a recorded panic from a thread named `delete` leaves the scan running, with no dialog.

**Accepted:**
- `tests/app_panic.rs`, a new file, single test because the hook is process-global;
- CLI validation in command-line order;
- the 250 ms signal check;
- `ScanLaunch`/`ScanStarter` as the T016+ interface;
- the production starter as a no-op until T006, now wired by T016.

**Follow-ups routed elsewhere:**
- `Model::path(root)` returns a trailing slash, because `PathBuf::push("")` adds one. That is a T003 defect, fixed in T005 (engineer-1). Remove the UI's root-name workaround in T016 after that.
- Name sanitizing currently lives in `ui/format.rs::sanitize`. T016 must consolidate it into one place (`ui/sanitize.rs`), not add a second implementation.
