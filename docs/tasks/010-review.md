# T010 review (Heimdall, 2026-10-23): CHANGES REQUESTED. Second review still required after the fixes.

**Fresh evidence** (clean worktree: HEAD plus the T010/T024 files):
- fmt ok, clippy clean;
- `cargo test --all-targets` pass (delete_executor 14 + 1 ignored);
- `cargo test --test delete_executor -- --ignored mount` passes. It really re-runs itself under `unshare -rm` and mounts a tmpfs; nested test output confirmed;
- release build ok.

**My mutation checks** (scratch worktree only; the engineer's files are untouched):

| Mutation | Result |
|---|---|
| `same_filesystem` always true | the mount test **fails** ✓ |
| target identity check disabled (`if false`) | `target_replaced_…` and `target_swapped_for_a_symlink…` **fail** ✓ |
| `NOFOLLOW` removed from `DIR_FLAGS` (`sys.rs`) | **all tests still pass** ✗ (see item 2) |

**Pass 1, spec:**
- It meets ADR-007/§3.8:
  - `openat2` with BENEATH, NO_SYMLINKS and NO_MAGICLINKS;
  - the (dev, ino, kind) check before anything is removed;
  - an iterative post-order walk over dirfds, with an fstat re-check on every open and reopen;
  - the st_dev (and mnt_id) boundary;
  - ≤ 64 held fds;
  - progress ≤ 10 Hz, typed outcomes, the stop flag, one delete at a time.
- Only owned files were touched; `msg.rs` is untouched.

**Required changes:**
1. **`Drop` hangs the process when the event channel is full and not drained.** This is the quit path (M12). I proved it with a probe: a `sync_channel(1)` already holding one event, a file delete submitted, `wait_idle` true, then `drop(handle)` was **still blocked after 3 s**. The cause: `busy` is cleared before the blocking `tx.send(Finished)`, and `Drop` then joins a thread that is stuck in `send`.
   - **Fix:** after a stop request, the executor must not block forever on `send`. For example, send `Finished` with a `try_send` loop that gives up once `stop` is set, with a short sleep between tries. `Drop` must return within a bounded time (≤ 2 s) whatever the receiver does.
   - Add this exact scenario as a test, red first. A non-draining receiver that is still alive is the case that hangs.
2. **`NOFOLLOW` on the dir opens is not covered by any test.** It sits behind the fstat identity check, so removing it is invisible. That is defence in depth, but it is a safety flag and must be pinned. Add a `sys` unit test: `open_dir_at(dir, name_of_symlink_to_a_dir)` returns `Err(ELOOP)`, or `ENOTDIR` with `O_DIRECTORY`; assert on the specific errno you observe. Show it failing with `NOFOLLOW` removed.

**Non-blocking:**
- A panic inside the executor leaves `busy` set, so every later `submit` returns `Busy`, and there is no outcome. Wrapping the request in `catch_unwind` and reporting `Aborted { OpenFailed(EIO) }` would keep the UI honest. Do it if it's cheap; otherwise list it as a known limit.

**Deviations ruled:**
- **`stx_mnt_id`** (bind mounts treated as another filesystem): **accepted**. It is stricter and safer. Recorded as plan D10, and the architect is informed: the scanner's one-filesystem check stays st_dev-only, so a same-device bind mount inside a delete target gives a visible `Partial` (`on another filesystem (not crossed)`) rather than a silent descent.
- **dev = `makedev(stx_dev_major, stx_dev_minor)`:** accepted. T006 must produce `dev` identically; its brief is amended with a test against `MetadataExt::dev()`.
- Names read once per directory, rel_path validation, hard-link bytes (0 while nlink > 1), ENOENT handling, Partial on stop, busy cleared before `Finished`, `try_send` progress: all accepted. The busy-before-Finished choice is fine once item 1 is fixed.

**Second reviewer:** after the fixes, a second reviewer (architect or team lead) reads `executor.rs` + `sys.rs` against ADR-007 before I commit. Requested via the team lead.

## Round 2 re-review (Heimdall, 2026-10-23): APPROVED by Heimdall. The commit waits only for the architect's second review.
**Fresh evidence** (clean worktree: HEAD plus `src/delete/{mod,executor,sys,msg}.rs` and `tests/delete_executor.rs`; file hashes match the working tree):
- fmt ok, clippy `-D warnings` clean;
- lib 58 (incl. the delete unit tests), delete_executor 17 + 3 ignored, model_apply 16, guards 7 + 11, all pass;
- 3 consecutive `cargo test --all-targets` runs are clean;
- `-- --ignored mount`: all 3 namespace tests pass (mount inside, bind inside, mount point as target);
- release build ok.

My first run failed (clippy, plus 2 panic tests) because it copied the files while engineer-1 was still editing. The rerun on a stable copy is clean. No defect.

**Mutation checks repeated** (scratch worktree):

| Guard disabled | Caught by |
|---|---|
| mount/mnt_id check inside the target | `mount_inside_…`, `mount_same_device_bind_inside_…` ✓ |
| target (dev, ino, kind) identity check | `target_replaced_…`, `target_swapped_for_a_symlink…` ✓ |
| `NOFOLLOW` in the dir-open flags | `sys::tests::opening_a_symlink_to_a_directory_is_refused` ✓ (was uncaught in round 1) |
| target mount-point check (F1) | `mount_point_as_the_target_is_refused` ✓ |
| unbounded `Finished` send / joining `Drop` | `dropping_the_handle_returns_even_if_nobody_reads_the_events` (reported red first) ✓ |

**`mod.rs` after the accidental `git checkout`:** I re-read the whole file.
- `Shared{Mutex<State{busy, exited}>, Condvar}`: `submit` holds the lock across the send, so the worker can't clear `busy` before it is set.
- `ExitGuard` marks `exited` even on a panic.
- `send_finished` retries with `try_send` and gives up once `stop` is set.
- `Drop` waits ≤ 2 s for `exited`, then joins or detaches.
- `guarded` uses `catch_unwind` and keeps the last counts.

No issues found.

**Deviation accepted (F2):** a panic yields `Partial { removed: last counts, failures: [target, Other(EIO)] }`, not my suggested `Aborted(OpenFailed(EIO))`. **Partial is the correct choice:** entries may already be gone, and UX's `Aborted` dialog says "Nothing was deleted". Partial also triggers the D1 rescan.

**Cross-task finding (sent to T015):** the app's panic hook runs **before** unwinding, so it also fires for panics that `guarded` catches on the `delete` thread. It would restore the terminal and print to stderr while the TUI keeps running. T015's hook must only tear down the terminal for panics on the main thread. Panics on other threads must not touch the terminal; they are reported through their outcome.
