# T006 report (engineer-1)

**Status:** DONE

Files: `src/scan/{mod,fs,walker}.rs`, `tests/scan_walker.rs`, `tests/support/fake_fs.rs`. Contract change (authorized): `ScanMsg::Identity { node, entry }` in `src/scan/msg.rs`, plus one match arm in `src/model/mod.rs::apply` (`Identity` ignored; T012 implements it) so the crate compiles.

## Failing first
`RealFs` got a `todo!` impl and the `start` stub still panicked: all 22 `scan_walker` tests failed. Then implemented; 22/22 pass, 5 repeated debug runs and 1 release run stable.
Mutation check: removing `NOFOLLOW` from the `RealFs` open flags fails `real_fs_refuses_symlinks_and_reports_permissions`.

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` all pass (86 lib, 22 scan_walker, others unchanged); doc ok; release build ok.

## Design
- `RealFs`: `openat(CWD, ..., NOFOLLOW|DIRECTORY|CLOEXEC|NOATIME)`, retried without NOATIME on EPERM; `RawDir` with a 32 KiB buffer; `statx` NOFOLLOW; EINTR retried; `dev = makedev(stx_dev_major, stx_dev_minor)` (the test compares with std `dev()`).
- Workers are named `scan-<i>`. Per job: open, read, stat every entry, send the `Listing`, then push child jobs. Ids: one `fetch_add(len)` per listing.
- Backpressure: `try_send` with a 500 us retry. Once `cancel_all` is set or the handle is closing, a full channel makes the send give up (message dropped). `Done` is abandoned only when closing. A gone receiver makes workers close the queue and exit.
- `cancel_all`: the app only calls `ScanControl::cancel_all()`, so no `wake_all` is needed. In-progress jobs finish; a worker that then gets `None` from `pop()` calls `drain_queued()`, re-checks completion and parks (100 ms recheck, woken by `rescan` or close). When pending reaches 0 exactly one `Done{cancelled: true}` goes out.
- Completion: a `done_sent` flag under a mutex, reset by `rescan`; `Done` is decided under the lock and sent outside it (no deadlock with a receiver that is draining).
- Rescan: `rescan()` resets `cancel_all`, records the node in a set, pushes a `Job`. The worker opens the parent, `stat_at(parent, name)` (NOFOLLOW) and sends `Identity`; it lists only if the target is still a directory (a symlink or file is never opened). ENOENT gives `DirIssue(Stat, Vanished)`; a parent open error gives `DirIssue(Open, ..)`.
- `ScanHandle::shutdown()` and `Drop` close the queue and wait at most 2 s for the workers, then detach.

## Decisions / things to check
- The rescan marker is a `HashSet<NodeId>` inside the walker, not a `Job` field, to avoid editing `queue.rs`. Two concurrent rescans of one node collapse into one identity check.
- A failed stat gives `Kind::Other`, zero sizes and `dev = ino = 0`.
- Entry directories on another device get `issue = (Stat, OtherFilesystem)` and are not enqueued, as the model expects.
- `RealFs` duplicates the small statx kind mapping from `delete::sys` (scan must not depend on the delete module).
- A scan root with no parent (`/`) skips the identity step and lists directly.
- If no worker thread can be spawned, the walker sends `DirIssue(root, Open, Other(EAGAIN))` and `Done`.
- Not covered: agreement of `makedev` with the delete executor is only transitive (both are checked against std's `dev()`); a mount-crossing real scan is not tested (T007).
