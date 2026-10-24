# T028b review (Heimdall, 2026-10-23): APPROVED, committed
- **Fresh evidence** (clean worktree: HEAD plus `src/scan/walker.rs` and `tests/scan_walker.rs`):
  - fmt and clippy clean;
  - the whole suite passes with `--no-fail-fast`; doc tests ok; release build ok;
  - **200/200** runs of the whole `scan_walker` binary and **100/100** of `app_controls`.
- **`rescan_many`:** holds `done_sent` across every `push_resume`. `check_complete` needs that same lock, so no `Done` can be sent between the jobs. A single `push_resume_many` at queue level isn't needed for that.
- **`rescan`** delegates to `rescan_many`. **`check_complete`** captures the cancelled flag inside the locked block.
- **Red first:** both new tests failed against a `todo!` stub.
- **No dedicated test for the cancelled-flag capture:** the interleaving can't be forced from outside, and today the app only rescans after the cancelled `Done`. Accepted as a documented limit; the code reading is clear.
- No app code was touched. **engineer-2's T020b now switches the queued rescans to `rescan_many`.**
