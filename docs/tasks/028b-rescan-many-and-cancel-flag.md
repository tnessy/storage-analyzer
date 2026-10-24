# T028b: Walker follow-ups from the architect's T028 review (low)

- **Engineer:** engineer-1.
- **Schedule:** after T008 (or before, if T008 is waiting on something).
- **Depends on:** T028 (merged `a83a9c5`).
- **The app-side switch** to `rescan_many` is engineer-2's, folded into **T020b**, which follows. **Don't edit `src/app/**`.**

## 1. `ScanHandle::rescan_many(&[(NodeId, PathBuf, u32)])`
- **The problem:** several rescans queued during a stop are issued as separate `rescan()` calls. If the first job finishes before the second is pushed, a worker sends `Done` in between, and the app briefly shows Done (with `rescan_in_flight` cleared) while the second folder is still scanning. The totals stay correct; it is cosmetic.
- **The fix:** push all the jobs under **one** `done_sent` lock, through one `push_resume`-style call under the queue lock (add `Queue::push_resume_many`).
- **Walker test (red first):** two rescan jobs, the first folder tiny and the second gated. Exactly one `Done` arrives, after both folders are listed.

## 2. `check_complete` reads `is_cancel_all()` after releasing the `done_sent` lock
- **The problem:** a rescan pushed in between would turn a stopped scan's `Done { cancelled: true }` into `cancelled: false`. That can't happen today, but `ScanHandle::rescan` documents "works at any time".
- **The fix:** capture the cancelled flag **inside** the locked block.
- **Test:** a unit test with a hook, or a reasoned assertion test showing the flag is read under the lock. If no deterministic test is possible, say so and give one targeted mutation (README rule).

## Files owned
`src/scan/walker.rs`, `src/scan/queue.rs`, `tests/scan_walker.rs`.

## Commands
The five standard ones, plus 200 runs of the new walker test.
