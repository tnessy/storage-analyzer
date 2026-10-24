# T028 review (Heimdall, 2026-10-23): APPROVED, committed. Architect quick look requested on the queue lock logic.

**Fresh evidence** (clean worktree: HEAD plus `src/scan/{queue,walker}.rs` and `tests/scan_walker.rs`):
- fmt ok, whole-tree clippy clean;
- every suite passes (scan_walker 25, lib 107, …);
- doc tests ok, release build ok.

**Code:**
- `pop()` drops the queued jobs under its own lock when it sees `cancel_all`; `drain_queued()` is removed.
- `push_resume()` clears `cancel_all` and pushes under the same lock.
- `ScanHandle::rescan` holds `done_sent` across the reset and `push_resume`.
- **Lock order is `done_sent` → queue in both `rescan` and `check_complete`**, so there is no inversion.
- The worker's `None` branch no longer re-checks `cancel_all`.

This matches the T028 brief's option (a) and design §3.3.

**Brief gap, accepted with reasons:**
- No deterministic **walker-level** red test. The window is a few instructions wide before the fix and gone after it, so there is nothing left to pause in.
- The invariant is pinned at queue level instead: `pop_under_cancel_all_drops_the_queued_jobs_itself`, `a_resumed_job_survives_every_later_pop`, and a 200-round cancel/resume race. All three were reported red first.

**What the measurements show** (my instrumented runs of the whole `app_controls` binary under parallel load):

| | `r_after_a_stop` failures | Instrumented runs where the rescan was lost |
|---|---|---|
| Before T028 | 3 / 100 | **0 / 250** (every run ended with `a` complete and re-listed) |
| After T028 | 1 / 100 (3 / 45 in a longer loop) | **0 / 250** |

**Conclusion:**
- The observed flake is a **test-timing race**: the rescan of the tiny fake folder finishes inside the `r` keypress, before `assert!(rescan_in_flight)`. That was my first diagnosis, and it was right for this test.
- The architect's walker race is **also real by code reading** but too narrow for this test to hit. T028 closes it.
- **The test still needs the gate fix**, which is back in T021 round 2 as item 10. The "200 runs, 0 failures" in the brief can only hold after that.
