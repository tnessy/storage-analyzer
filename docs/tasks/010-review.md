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
