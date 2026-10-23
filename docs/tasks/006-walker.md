# T006: Walker (workers, real fs, errors, one-filesystem, ScanControl, rescan)

- **Engineer:** engineer-1. **Wave:** 1. **Depends on:** T004 (and T001). **Blocks:** T007, T009, T012, T019, T022.

## Goal
Implement `scan::start` and `ScanHandle` (design §3.3, §3.5, §3.11) over the `FsAccess` seam, plus `RealFs` on rustix.

Acceptance criteria:
1. **`RealFs`:**
   - `open_dir` uses `openat(AT_FDCWD, path, O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC|O_NOATIME)` and retries **without** `O_NOATIME` on `EPERM` (M9).
   - `read_entries` uses rustix `Dir`/`RawDir`, skipping `.` and `..`.
   - `stat_at` uses `statx(dirfd, name, AT_SYMLINK_NOFOLLOW, TYPE|MODE|NLINK|INO|SIZE|BLOCKS)`. It needs `STATX_BLOCKS` and maps `dev` from `stx_dev_major`/`minor`.
   - EINTR is retried. No `unwrap` on I/O.
   - `dev` = `rustix::fs::makedev(stx_dev_major, stx_dev_minor)`, the same formula as the delete executor (T010 review). The executor compares the scan's (dev, ino) against its own statx, so the two must match. Test: `RealFs::stat_at(...).dev == std::fs::symlink_metadata(p).dev()` for a file in a tempdir.
2. **Workers:** `opts.threads` OS threads, default `ScanOptions::default_threads()` = `min(available_parallelism, 8)`. That default is a **target to tune**, and T009 measures alternatives.
3. **Per job:** open, read, stat each entry, then **send the `Listing` before enqueuing child dirs** (ordering contract). Ids come from a shared `AtomicU32`: one `fetch_add(len)` per listing, starting at `first_id`.
4. **Errors (M10):** an open or read failure on a dir sends `DirIssue { dir, op, kind }`. A stat failure on one entry gives a `RawEntry` with `issue = Some((Stat, kind))`; if the stat fails, use `Kind::Other` and zero sizes. Mapping: `EACCES`/`EPERM` → `PermissionDenied`, `ENOENT` → `Vanished`, else `Other(errno)`. Never panic, never stop.
5. **One filesystem (ADR-008):** with `one_filesystem` (the default), an entry dir whose `dev` ≠ `root_dev` gets `issue = Some((Stat, OtherFilesystem))` and is **not** enqueued. With `--cross-filesystems`, everything is descended.
6. **ScanControl:**
   - A popped `Popped::Cancelled(job)` sends `Cancelled { dir }`.
   - `cancel_all` stops taking jobs. After `cancel_all()`, call `queue.wake_all()`. Remaining queued jobs are dropped with `queue.drain_queued()` (T004 review) without being read. When the in-progress jobs finish (`done()` reaches 0), `Done { cancelled: true }` is sent.
7. **Completion:** when `pending_jobs()` reaches 0, send `Done { cancelled: false }`. This happens again after every rescan that finishes.
8. **`rescan(dir, path, depth)`:** works during a scan and after Done/Stopped. It clears `cancel_all` via `reset_cancel_all` first. It pushes a job seeded with the existing `NodeId`, so the walker lists `dir` afresh with new child ids. Workers stay alive (they idle on `pop()`) until `shutdown()`.
9. **Backpressure:** `tx` is a bounded `SyncSender` (the caller chooses the capacity; the app uses 1024). Workers block on a full channel.
10. `shutdown()` closes the queue and joins every thread within a bounded time, also while workers are blocked on a full channel. Workers notice a disconnected receiver and exit.

## Design references
§3.3, §3.5, §3.11 mechanics. ADR-002, ADR-008, ADR-009.

## Files owned
`src/scan/mod.rs` (taken over), `src/scan/fs.rs` (adds `RealFs`), `src/scan/walker.rs`, `tests/scan_walker.rs`, `tests/support/fake_fs.rs` (an in-memory `FsAccess` with error injection, shared with T007 and T009, owned here). Integration tests include support files with `#[path = "support/fake_fs.rs"] mod fake_fs;`. There is no `tests/support/mod.rs`.

## Interfaces
- **Consumes:** `Queue` (T004), `ScanControl`/`ScanMsg`/`FsAccess` (T001).
- **Provides:** `scan::start`/`ScanHandle` to T015 and T019 (app wiring) and T009/T022 (harnesses).

## Tests to write first (`tests/scan_walker.rs`, using `FakeFs` unless noted)
- **Ordering contract:** for each listing received, its `dir` was either the root or appeared as an entry in an earlier listing.
- **M11 shallow-first (integration):** in a fake tree 4 levels deep with `threads = 1`, every depth-1 dir's `Listing` arrives before any depth-3 dir's `Listing`.
- **Focus:** with `threads = 1`, setting focus on a depth-1 dir before start makes the whole focused subtree list before its unfocused siblings' children (design §4 "Skip and prioritise").
- **Skip:** after `cancel_prefix(/r/a)`, no `Listing` for any dir under `/r/a` arrives after its `Cancelled`.
- **M10:**
  - `EACCES` on open of one dir → `DirIssue(Open, PermissionDenied)`, and the rest completes.
  - `ENOENT` on stat mid-listing → an entry with a `Vanished` issue.
  - An unexpected errno → `Other(errno)`.
  - `Done` always arrives.
- **One filesystem:** a fake entry with a different `dev` → an `OtherFilesystem` issue, never listed. With `one_filesystem = false`, it is listed.
- **`cancel_all`** → `Done { cancelled: true }` and no further listings.
- **Rescan** after Done → a fresh `Listing` for the dir with new ids, then a second `Done`.
- **Shutdown** with a full channel and a dropped receiver returns within 2 s.
- **RealFs on a tempdir:** sizes equal `std::fs::symlink_metadata` (`blocks*512`, `len`). A symlink is reported as `Symlink` and not followed. A `chmod 000` dir gives `PermissionDenied`. Skip with a message when running as root.

## Out of scope
du parity across fixture shapes (T007). Model changes.

## Amendment (2026-10-23): bounded sends and shutdown (design §3.8 "Handle lifecycle", same rule for `ScanMsg`)
- **Bounded sends:** workers never block forever on `tx.send`. Use `send` while scanning normally (backpressure is intended), but once `cancel_all` is set or the queue is closed, a worker uses `try_send` and drops the message on `Full`, so it can exit.
  - `Done { cancelled: true }` may be dropped on exit. When the app is *not* exiting, `cancel_all` (the `s` key) is followed by continued draining, so `Done { cancelled: true }` does arrive.
- **`shutdown()` returns within 2 s even with a full channel and a live receiver that is not draining.** Test this, red first, in addition to the existing dropped-receiver test.
- `drain_queued()` after `cancel_all` (T004 fix) is part of this.

## Amendment (2026-10-23, rev 2): rescan refreshes the target's identity (design §4 / §3.8; plan D11 as accepted by the architect)
**Contract change, authorized** for `src/scan/msg.rs`: add exactly **`ScanMsg::Identity { node: NodeId, entry: RawEntry }`**.

**What a rescan job does first:** it re-stats its own target with **no-follow**. It opens the parent path (the same open flags as `open_dir`) and calls `stat_at(parent, name, AT_SYMLINK_NOFOLLOW)`. That gives a full `RawEntry`: name, kind, both sizes, dev, ino, nlink. A rescan job may target a **file** node too (a rescan after an aborted file delete).

**Outcomes:**
| Situation | Send |
|---|---|
| Target is still a dir | `Identity { node, entry }`, then open it as a dir and send its `Listing` as usual |
| Target is now a file, symlink or other (opening as a dir would fail with ENOTDIR/ELOOP) | `Identity { node, entry }` **only**, no `Listing` and no `DirIssue` |
| Target is gone (ENOENT on the stat) | `DirIssue { dir: node, op: Stat, kind: Vanished }` |
| Parent can't be opened, or another stat error | `DirIssue` with the mapped kind |

The `dev` formula is the same as everywhere else (`makedev`).

**Tests:**
- dir replaced by a new dir with the same name → `Identity` with the new ino, then a `Listing`;
- dir replaced by a regular file → `Identity` with `kind: File` and its sizes, and no `Listing`;
- dir replaced by a symlink to a dir → `Identity` with `kind: Symlink`, no `Listing`, and the symlink target is never opened (assert through `FakeFs` call recording);
- target removed → `DirIssue(Vanished)`;
- rescan of a file node → `Identity` only.

## Amendment (2026-10-23, from the T015 review): worker thread names
Name every worker thread `scan-<i>` with `std::thread::Builder::name`. The app's panic hook attributes panics by thread name: only `scan-*` panics stop the scan. Add a test that spawned workers carry that prefix (`thread::current().name()` inside a `FakeFs` call).
