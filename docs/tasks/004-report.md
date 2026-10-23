# T004 report (engineer-1)

**Status:** DONE

Files: `src/scan/queue.rs` (with unit tests), `src/scan/mod.rs` (one line: `pub mod queue;`).

Red first: not observed. I wrote the queue and tests together, so no failing run exists. Instead I mutation-checked: changing the shallow-first rank to `depth` made 4 of 9 tests fail (shallow-first, focus, cap, cancelled); restored afterwards.

Fresh: fmt ok, clippy -D warnings clean, `cargo test --all-targets` pass (49 unit incl. 9 queue), `cargo test --release scan::queue -- --test-threads=1` pass, doc ok, release build ok.

## Deviations / decisions
- `pub mod queue;` instead of private `mod queue;` (private would raise dead-code warnings under `-D warnings`).
- `done()` takes no argument and returns the remaining pending count (so the caller can `close()` at 0). Call it for cancelled jobs too.
- Added `close()`, `wake_all()` (call after `cancel_all()` so blocked workers see it), `rekey_count()`. `pop()` returns `None` as soon as closed, even if jobs remain queued.
- Cap: a job is "over cap" when queued jobs `>= cap` at push time. Over-cap jobs sort after all in-cap jobs (deepest-first, FIFO), but focus still wins over both.
- Any `ScanControl` mutation (including cancel/skip) bumps the generation and triggers one O(pending) re-key.
- `max_pending()` is the high-water mark of *queued* jobs (not in-progress).

## Review fixes (2026-10-23)
- Added `drain_queued() -> usize` (empties the heap, subtracts from pending). Red first: test panicked on a `todo!` stub.
- Re-key now only when the focus value changed (generation is still recorded). Red first: `only_a_focus_change_rekeys_not_skips_or_cancel_all` failed. `rekey_count` therefore counts focus changes only.
- `done()` uses `debug_assert!(pending > 0)` instead of saturating.
