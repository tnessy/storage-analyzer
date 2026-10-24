# T020b review (Heimdall, 2026-10-23): APPROVED, committed
**Fresh evidence** (clean worktree: HEAD plus the T020b files; engineer-1's in-progress `examples/scan_harness.rs` excluded, which explains the `--all-targets` failure engineer-2 saw):
- fmt ok, **whole-tree** clippy clean;
- the whole suite passes with `--no-fail-fast` (app_controls 28);
- release build ok;
- **200/200** runs of the whole `app_controls` binary.

**Real terminal** (release build under a pty at 80×30, on `/`; keys `s G r p x d`): `tmp/ is on another filesystem. To scan it, use --cross-filesystems.` shows on the other-filesystem row, and `d` keeps `… wasn't scanned. Not deleted.` (UX §5.3/§5.4, check 21) ✓. No panic.

**Pass 1:**
- `r`, `p` and `x` in the tree and the Skipped view show the message and change nothing;
- `format::truncate_middle` cuts to 15 cells with `…` (`...` in ASCII) and keeps the trailing `/` and `--cross-filesystems.` intact (tested with a 40-character name);
- T027's placeholder arm is gone;
- `AppState::rescan_many` is used for the queued rescans and for the resume list of `r` on an unfinished folder.

**Evidence judgement on `queued_rescans_start_together_and_the_scan_stays_in_flight_until_both_finish`:** it passes on the old per-call loop too, because the gate stops the old race from appearing at app level. Engineer-2's breakage (drain only the first queued rescan) makes it fail, so it does pin "every queued rescan runs, and in-flight lasts until all are complete". **The atomicity itself is pinned at walker level by T028b's tests.** Accepted.

A regression back to a per-call loop in the app would not be caught by this test. That is low risk, since the walker-level guarantee holds for single calls, but noted.
