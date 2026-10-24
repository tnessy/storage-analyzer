# T028 report (engineer-1): stop followed by rescan could lose the rescan

**Status:** DONE (no T028 brief existed when I started; I followed the lead's message and design §3.3)

Files: `src/scan/queue.rs`, `src/scan/walker.rs`, `tests/scan_walker.rs`.

## Cause
After a stop, the last worker sent `Done{cancelled:true}`, went back to `pop()`, got `None` because `cancel_all` was still set, and only then called `drain_queued()`. In that gap `rescan()` cleared `cancel_all`, reset `done_sent` and pushed the rescan job; the late drain removed it and `check_complete()` sent a second, spurious `Done{cancelled:false}`.

## Fix (atomic under the queue lock)
- `Queue::pop()` drops all queued jobs itself when it sees `cancel_all`, under the same lock it takes to decide that. `drain_queued()` is gone (its only caller was the worker; its test is replaced).
- New `Queue::push_resume(job)`: clears `cancel_all` and pushes the job under that lock. A job pushed by it can never be dropped by an older cancel.
- `ScanHandle::rescan` holds the `done_sent` lock across "reset done_sent, register the rescan root, `push_resume`", so a worker deciding whether the scan is complete sees either the old state or the whole resumed one.
- The worker's `None` branch no longer re-checks `cancel_all` (a rescan may have cleared it already): it only exits when the handle is closing; otherwise it checks completion and parks.

## Tests (queue tests failed first)
- `pop_under_cancel_all_drops_the_queued_jobs_itself`, `a_resumed_job_survives_every_later_pop`, `cancel_and_resume_racing_never_lose_the_resumed_job` (200 rounds): all 3 failed against the `push_resume` stub and the old `pop`.
- `a_rescan_right_after_a_stop_is_never_lost` (200 stop-then-rescan rounds against the real walker, 4 threads): it passed before the fix too, so I could not make it fail; the window is too narrow to hit reliably. I did not add a deterministic pause hook: after the fix there is no gap between "pop saw the cancel" and "drop the queued jobs" to pause in, so the queue-level tests pin the invariant instead.
- Original flaky test `r_after_a_stop_is_a_full_rescan_at_once` (app_controls): 0 failures in 200 runs after the fix (I did not measure its failure rate before the fix, so this is absence of failures, not a before/after comparison).

## Fresh results
fmt ok; clippy (lib + scan_walker, app_controls, model_controls) -D warnings clean; lib 108, scan_walker 25 (5 more release runs ok), app_controls 24, model_controls 30, delete_model 23, scan_du_parity 8, scan_robustness 2: all pass.
