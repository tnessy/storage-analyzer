# T010: Delete executor (safety-critical)

- **Engineer:** engineer-1. **Wave:** 2. **Depends on:** T001 only. It may be pulled earlier. **Blocks:** T011, T021.
- **Review:** this needs a **second reviewer** in addition to Heimdall (design §8 "Watch"). Expect a longer review.

## Goal
Implement the delete executor thread from design §3.8 and ADR-007. This is the only code that removes user data (M7, M9).

Acceptance criteria:
1. `delete::spawn(root_fd, tx)` starts one executor thread. `submit(req)` returns `Err(Busy)` while a delete runs (one at a time).
2. **Resolution:** `openat2(root_fd, parent_rel, O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC, RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS|RESOLVE_NO_MAGICLINKS)`. For a target directly in the root, `parent_rel` is `.`. Failure → `Aborted { OpenFailed(errno) }`, or `NotFound` for ENOENT.
3. **Identity:** `statx(parent_fd, name, AT_SYMLINK_NOFOLLOW)`. If `(dev, ino)` ≠ `(expect_dev, expect_ino)`, or the kind differs → `Aborted { ChangedSinceScan }`, and **nothing is removed**.
4. A file, symlink or other target → `unlinkat(parent_fd, name, 0)`.
5. **Directory:**
   - `openat(parent_fd, name, O_DIRECTORY|O_NOFOLLOW)` and re-verify with `fstat`;
   - iterative post-order removal using dirfds only, with no recursion on the Rust stack;
   - never descend into an entry whose `st_dev` ≠ the target's: record `OtherFilesystem` and continue;
   - at most **64** open dirfds. Deeper levels close the far ancestors and reopen from the nearest held ancestor with the same no-follow and identity checks;
   - finish with `unlinkat(parent_fd, name, AT_REMOVEDIR)`.
6. **Failures:**
   - `EACCES`/`EPERM` → `PermissionDenied`
   - `EBUSY`/`ETXTBSY` → `Busy`
   - `EROFS` → `ReadOnlyFs`
   - `ENOENT` → success (not a failure)
   - else → `Other(errno)`

   Continue past failures. A non-empty dir left behind because of child failures is not an additional failure.
7. **Progress:** `DeleteEvent::Progress` at most every 100 ms. Then exactly one `Finished(Outcome)`: `Removed(Counts)` if there were no failures, `Partial { removed, failures }` otherwise, or `Aborted`.
8. **Stop:** `stop()` sets a flag checked between entries. The executor finishes the current entry and returns `Partial` (or `Removed` if it happened to finish). `wait_idle(max)` returns whether it went idle within `max`. Only quit and signals call `stop()` (T021).
9. **No symlink is ever followed, and nothing outside the target is touched.**

## Design references
§3.8 Executor/Cancellation, §3.10 Delete progress/result, §4 contracts, D8. ADR-007. Rust CVE-2022-21658 (design §2).

## Files owned
`src/delete/mod.rs` (taken over), `src/delete/executor.rs`, `src/delete/sys.rs` (thin rustix wrappers), `tests/delete_executor.rs`.

## Interfaces
**Consumes:** `DeleteRequest`/`DeleteEvent`/`Outcome` (T001). **Provides:** `DeleteHandle` to T011 tests and T021.

## Tests to write first (`tests/delete_executor.rs`, all inside temp dirs)
- **File** removed → `Removed { files: 1 }`. **Dir tree** (3 levels, 500 entries) removed → counts match, and the dir is gone.
- **Swap target for a symlink after the "scan"** (record dev/ino, then `rename` a symlink into place) → `Aborted(ChangedSinceScan)`, and the symlink's target and its contents are untouched.
- **Swap a parent component for a symlink** → `openat2` fails, the outcome is `Aborted`, and nothing is removed.
- **Symlink inside the subtree pointing outside** → the link is removed and the outside target is untouched.
- **Hard link inside, other link outside** → the outside link survives with its data.
- **`chmod 555` subdir** → `Partial` with `PermissionDenied` entries; everything else is removed.
- **400-deep tree** → removed, and the peak open fds stay ≤ 64 + a small constant (count `/proc/self/fd`).
- **Mount inside the target:** run under `unshare -rm` with a tmpfs mounted inside → not descended, recorded as `OtherFilesystem`, and the tmpfs contents survive. If user namespaces are unavailable, the test prints `SKIP: unshare unavailable` and passes. Put it behind a helper binary or `#[ignore]` with the command documented: `cargo test --test delete_executor -- --ignored mount`.
- **Progress** is rate-limited: with 50 k entries, there are ≤ (elapsed_ms / 100 + 2) Progress events.
- **`stop()`** mid-delete → `Partial` with removed < total, and `wait_idle(2s)` is true.
- A second `submit` while running → `Busy`.
- **Write-guard:** the `tests/write_guard.rs` test still passes (only `src/delete/**` uses the unlink calls).

## Out of scope
Model reconciliation and `delete_check` (T011). Dialog and UI (T021). Trash and multi-select (not in v1).

## Review round 2 (2026-10-23): required before commit
These come from Heimdall's review (`010-review.md`) and the architect's second review. **The commit is held until all of them land.**
1. **F1, the target is a mount point (design §3.8 step 2b):** after the identity check, if the target's (dev, `mnt_id`) differs from `parent_fd`'s, return `Aborted { reason: AbortReason::MountPoint }`. Nothing is removed.
   - **Contract change, authorized:** add `MountPoint` to `AbortReason` in `src/delete/msg.rs`. That is the only `msg.rs` edit.
   - **Tests:** (a) under `unshare -rm` (same pattern as the mount test), the target itself is a tmpfs mount point → `Aborted(MountPoint)` and its contents survive; (b) a same-device bind mount as the target → `MountPoint`. Both must be shown failing first.
2. **F2, panic safety:** run each request inside `catch_unwind`. A panic yields `Finished(Aborted { OpenFailed(EIO) })` and clears `busy`, so later submits work. Test: inject a panic through a test-only hook (for example a `cfg(test)` closure in `run_with_limit`) → `Finished(Aborted)`, then a second submit succeeds.
3. **`Drop` must not hang** (`010-review.md` item 1): with a full, undrained event channel and a live receiver, `drop(handle)` returns within 2 s. Red-first test.
4. **Pin `NOFOLLOW`** (`010-review.md` item 2): a `sys` test that `open_dir_at` on a symlink to a dir fails with the specific errno, and that fails when `NOFOLLOW` is removed.
5. **Bytes-freed test (architect):** for a tree with known single-link files, `Removed.bytes` equals the sum of their `blocks*512` plus the dirs' own blocks. A file with an outside hard link adds 0.
6. The architect's F3/F5 notes, as relayed by the team lead: address them, or answer them in your report.

The second review is done (architect, approved with F1/F2). After the fixes, Heimdall re-verifies, and also re-runs the mutation checks.
