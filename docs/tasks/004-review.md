# T004 review (Heimdall, 2026-10-23): CHANGES REQUESTED

**Fresh evidence:** in the clean worktree, 9 queue tests pass. `--release --test-threads=1` passes, and 5 repeated release runs pass (the concurrency test is stable).

**Process:** the tests weren't seen failing first. Mutation-checking (rank → depth fails 4 of 9) proves the tests can fail, so I accept it **this once**. The new tests below must be shown red before the fix.

**Pass 1, spec:**
- Ordering (focus, shallow-first, FIFO), the cap fallback, `Popped::Cancelled` and the pending accounting all match.
- `pub mod queue` is accepted. The `done()`, `close()`, `wake_all()` and `rekey_count()` additions are accepted.
- **Gap: no way to drain after `cancel_all`.** T006 criterion 6 requires "remaining jobs are drained without being read" so that `Done { cancelled: true }` can be sent. But `pop()` returns `None` with jobs still queued, and `pending` still counts them, so it never reaches 0. Stale jobs would also come back to life after `reset_cancel_all` on a rescan. Add `drain_queued(&self) -> usize`: it empties the heap, subtracts the drained count from `pending`, and returns the count. Test: push 10, pop 2 (in progress), `cancel_all` + `drain_queued` → returns 8, `pending_jobs() == 2`; after two `done()` calls → 0; after `reset_cancel_all` + push 1, the next pop is that new job.

**Pass 2, quality:**
1. **Scalability:** `refresh` rebuilds the whole heap on **every** generation bump. That includes `cancel_prefix` (each `x` skip, each delete confirmation) and `cancel_all`, not just focus changes. At the 1 M frontier this is an O(n) rebuild under the queue mutex, stalling every worker, for no reason. Rebuild only when the focus value actually changed: compare it with `self.focus`, and still record `seen_generation`. Test: `cancel_prefix` doesn't increase `rekey_count`; `set_focus` does.
2. `done()` uses `saturating_sub`, which hides a double `done()`. Use `debug_assert!(pending > 0)`.

Everything else is clean and readable.
