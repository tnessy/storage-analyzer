# T006 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree: HEAD plus the T006 files and the one-line `Model::apply` arm):
- fmt ok, whole-tree clippy clean;
- every suite passes in 3 consecutive debug runs (scan_walker 22, lib 86, app_loop 18, delete_executor 17 + 3 ignored, …);
- release `scan_walker` 22 pass; doc tests ok, release build ok.

Red-first is reported (all 22 failed against the stubs). The `NOFOLLOW` mutation on `RealFs` is caught.

**Pass 1, spec:** every T006 criterion and amendment is met:
- the `RealFs` flags (NOATIME retried without it on EPERM, NOFOLLOW, EINTR retry, `makedev`);
- `scan-<i>` thread names;
- listing before child push;
- one id block per listing;
- the errno mapping;
- `-x` (`OtherFilesystem`, not enqueued);
- skip → `Cancelled`;
- `cancel_all` → `drain_queued` → `Done { cancelled: true }`;
- bounded sends after cancel/close;
- stopping the workers is bounded at 2 s, then they are detached;
- the rescan `Identity`, with lists only for dirs, and files/symlinks never opened.

Contract change: only `ScanMsg::Identity`, as authorized. The `Model::apply` ignore arm is acceptable until T012.

**Pass 2, quality:** clear and small. Completion is decided under a lock and sent outside it. `ExitGuard` counts exits even on a panic. Ordering is preserved, because children are pushed before `done()`, so the pending count never touches 0 mid-job.

**Rulings on the flagged points:**
1. **The app only calls `cancel_all()`; workers drain the queue and park** (100 ms recheck, woken by `rescan`/close): **accepted.** It keeps the app side to one call.
2. **Once `cancel_all` or closing is set, a send on a full channel gives up; only closing abandons `Done`:** **accepted.** This matches design §3.8. A listing dropped after a stop leaves that dir unread, so `◌` stays, which is consistent with Stopped.
3. **The rescan marker is a `HashSet<NodeId>` in the walker:** **accepted**, with a known limitation (below).
4. **The kind mapping is duplicated in `scan::fs` and `delete::sys`:** **accepted for now.** Follow-up in T005: add `Kind::from_raw_mode(u32)` to `src/types.rs` (an authorized contract addition) and use it in both places.
5. **`SEND_RETRY` = 500 µs polling** (std has no stable `send_timeout`): **accepted.** T009 must report worker CPU while the channel is full. If it is material, back off exponentially up to a few ms.

**Known limitation, recorded (non-blocking):** `r` on a folder **still being scanned** (not skipped) leaves its old queued jobs in the queue. `begin_rescan` marks those nodes `REMOVED`, so the model ignores their listings, but the walker keeps walking the old subtree as orphans. The results are correct, but that subtree's I/O is wasted, and `Done` waits for it.
- Skip-then-resume and the D1 delete reconciliation are **not** affected, because their prefixes already dropped the old jobs.
- Revisit in T020: for example, cancel the old jobs by giving the rescan a fresh generation, or turn `r` on a still-scanning, unskipped folder into `p` (prioritise). That needs an architect/UX decision; tracked as plan item O7.
