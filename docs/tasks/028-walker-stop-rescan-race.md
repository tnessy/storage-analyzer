# T028: Walker race: a rescan right after a stop can be lost

- **Engineer:** engineer-1.
- **Schedule:** **now, ahead of T008/T009.**
- **Depends on:** T006, T020, T027 (all merged).
- **Source:** the architect's second review of T021. Heimdall's first diagnosis ("test race") was **wrong**; this is a real race in `src/scan/walker.rs`. Design §3.3 now has the rule.

## The race
1. After a stop, the last worker finishes its job. `finish()` → `check_complete()` sends `Done { cancelled: true }`, and the worker goes back to `pop()`. `cancel_all` is still set, so `pop()` returns `None` and the worker enters the cancel branch.
2. Meanwhile the main thread applies `Done` and handles `r`. `ScanHandle::rescan()` clears `cancel_all`, resets `done_sent` and pushes the rescan job.
3. The worker now runs `drain_queued()`, which **discards the rescan job**. Then `check_complete()` sends a second `Done { cancelled: false }`.
4. **Result:** the rescan is lost, the app shows Done, and the folder stays unfinished.

**Impact:**
- `r` pressed quickly after a stop.
- Worse: rescans queued during Stopping, including D1 reconciliation after a failed or partial delete, start inside `apply_scan_msg(Done)`, which is exactly this window.
- Observed: `app_controls::r_after_a_stop_is_a_full_rescan_at_once` failed 3 of 30 runs. `an_internal_rescan_asked_for_while_stopping_waits_and_runs_once_after_done` is probably exposed too.

## Fix (design §3.3 rule)
"cancel is set → drain the queue" and the rescan's "clear cancel → push the job" must be **atomic with respect to each other, under the job-queue lock**. Two options:
- **(a) Queue-level operations:**
  - `pop()` notices `cancel_all`, **drains and returns `None` under its own lock** (returning the drained count so `done()` accounting stays right);
  - a new `Queue::push_resume(job)` **clears `cancel_all` and pushes under the same lock**, used by `ScanHandle::rescan`;
  - remove the separate `drain_queued()` call from the worker.
- **(b)** A cancel generation stamped on every job, so the drain drops only jobs older than the current generation.

Pick one and explain why in your report. **(a)** is the smaller change.

`done_sent`/`check_complete` must never send `Done { cancelled: false }` while a job pushed by `rescan` is pending. Add an assertion or a test.

## Tests (red first: show the failure before the fix)
- **Deterministic regression test:** add a test-only hook (`cfg(test)` or a `#[doc(hidden)]` pause point in `Shared`) that **pauses a worker between `pop() == None` and the drain**. With it, run the exact interleaving: stop → the worker paused → `rescan()` → release. Expected: the rescan job is processed, a `Listing` arrives for the folder, and the only `Done` after it is the rescan's.
- **Loop proof after the fix:**
  - `app_controls::r_after_a_stop_is_a_full_rescan_at_once`, **200 runs, 0 failures**;
  - `an_internal_rescan_asked_for_while_stopping_waits_and_runs_once_after_done`, 200 runs.

  Run the built test binary directly in the loop, as you did for T011.

## Files owned
`src/scan/walker.rs`, `src/scan/queue.rs`, `tests/scan_walker.rs` (new tests). Read `tests/app_controls.rs`, but don't change it unless the hook needs a one-line wiring. In that case, tell Heimdall first, because engineer-2 owns that file.

## Commands
The five standard ones, plus the loops above.
