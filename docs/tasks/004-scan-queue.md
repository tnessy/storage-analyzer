# T004: Scan job queue (shallow-first, focus re-key, frontier cap)

- **Engineer:** engineer-1. **Wave:** 1. **Depends on:** T001. **Blocks:** T006.

## Goal
Build the shared job queue from design §3.5 and ADR-009 as a self-contained, deterministic data structure. It delivers M11 shallow-first ordering and the priority mechanism.

Acceptance criteria:
1. `Job { dir: NodeId, path: PathBuf, depth: u32 }`.
2. A queue that is MPMC-safe, with a blocking `pop()` that returns `None` once the queue is closed or `cancel_all` is set. Use `Mutex` + `Condvar` + `BinaryHeap`; no busy-waiting.
3. **Ordering:** pop the highest `(in_focus desc, depth asc)`. Within equal keys, FIFO by insertion sequence, so tests are deterministic. `in_focus` = the job path is at or under the `ScanControl.focus` path (whole components).
4. **Re-key:** when `ScanControl.generation()` differs from the queue's last-seen generation, the next `push` or `pop` recomputes `in_focus` for all pending jobs and rebuilds the heap. That is O(pending), only when focus changes. It records the re-key duration, exposed by `last_rekey_ms()` for R8.
5. **Frontier cap:** above `cap` pending jobs (default 1,000,000, a constant), newly pushed jobs use deepest-first ordering (`depth desc`) to bound memory. Jobs already queued keep their keys. Expose `max_pending()` for R8.
6. **Cancellation:** `pop()` skips and returns to the caller (as a distinct `Popped::Cancelled(job)`) any job whose path `ScanControl::is_cancelled`, so the walker can send `ScanMsg::Cancelled { dir }`.
7. **Pending accounting:** `pending_jobs()` counts queued plus in-progress jobs (`done(job)` decrements it). The walker uses it to detect completion.

## Design references
§3.5 job queue, §3.11 mechanics, ADR-009 (O1, O2, O4).

## Files owned
`src/scan/queue.rs` (with its unit tests), plus a `mod queue;` line in `src/scan/mod.rs`. That one line is the only edit to `mod.rs`; T006 takes the file over afterwards.

## Interfaces
**Consumes:** `ScanControl` and `NodeId` (T001). **Provides:** `Queue`, `Job`, `Popped`, used only by T006.

## Tests to write first
- **M11 shallow-first:** push jobs at depths 3, 1, 2, 1, 3. They pop as 1, 1, 2, 3, 3, FIFO within a depth.
- **Focus:** with focus on `/r/b`, `/r/b/x` (depth 2) pops before `/r/a` (depth 1). Clearing the focus restores depth order. Re-keying happens once per generation bump, which a counter proves.
- Component matching: focus `/r/b` does not prioritise `/r/bc`.
- **Cap:** with `cap = 4`, pushing a 5th job at depth 9 and a 6th at depth 8 pops 9 before 8 once the shallower ones drain. Earlier jobs are unchanged.
- **Cancelled:** a cancelled-prefix job pops as `Popped::Cancelled`, never as `Popped::Job`.
- **Concurrency:** 8 consumer threads and 2 producers on 100 k jobs. Every job pops exactly once, `pending_jobs()` reaches 0, and `pop()` returns `None` after `close()`. Run with `cargo test --release scan::queue -- --test-threads=1` as well.

## Out of scope
Filesystem access and message sending (T006).
