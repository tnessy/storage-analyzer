# T010 report (engineer-1)

**Status:** DONE (needs the second reviewer, per brief)

Files: `src/delete/{mod,executor,sys}.rs`, `tests/delete_executor.rs`. `msg.rs` untouched.

## Failing first
`cargo test --test delete_executor`: 14 tests failed, because `spawn` was the `todo!("T010")` stub. Unit tests in `executor.rs` were written with the implementation.

## Mutation checks (proof the safety tests bite)
- Removing the reopen identity check made `directory_replaced_while_the_walk_is_below_it_is_detected_on_reopen` fail (after I fixed that test, which first passed for the wrong reason).
- Dropping `NO_SYMLINKS|NO_MAGICLINKS` from `openat2` made `parent_component_swapped_for_a_symlink_aborts` fail (after I changed its symlink to a relative one; an absolute one was already refused by `BENEATH`).
- Not mutation-checked: the target identity check (`ChangedSinceScan`) beyond the two tests that cover it, the `NOFOLLOW` open flags, and the `st_dev`/mount-id check (covered only by the `unshare` test).

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` pass (52 unit, 14 delete_executor + 1 ignored, 14 model_apply, 4, 7); doc ok; release build ok. `cargo test --test delete_executor -- --ignored mount` passes under `unshare -rm` (tmpfs inside target not descended, contents survive). Peak open fds during the 400-deep delete: 67 above baseline (limit 64 + parent fd + listing temp fd + sampler), asserted <= 72.

## Decisions / deviations (please review)
- **Mount check also compares `stx_mnt_id`** (when the kernel reports it), so a same-device bind mount is treated as `OtherFilesystem` too. Stricter than the brief's `st_dev`-only rule.
- **Device number** is `makedev(stx_dev_major, stx_dev_minor)` from `statx`. T006 must produce `dev` the same way (it equals `st_dev` from `stat`; the tests rely on that).
- **Entry names of a directory are read in full when it is opened** and kept in the frame (also after its fd is closed), so a reopen never re-lists. Memory is O(width of the widest open directory).
- **Fd limit:** at most 64 directory fds held (the deepest ones), plus the target's parent fd and a transient listing fd. When the top frame has no fd, a block of up to 32 frames is reopened from the parent fd, each checked with no-follow and (dev, ino). A mismatch or error there stops the walk: one failure is recorded for that path and the outcome is `Partial`.
- **`rel_path` validation:** only plain components; empty, `.`, `..`, absolute paths are refused as `Aborted(OpenFailed(EINVAL))`.
- **Bytes:** files with `nlink > 1` add 0 bytes to `removed_bytes` (a surviving link frees nothing); directories add their own blocks.
- **Failure paths** are relative to the scan root (target `rel_path` + names below it).
- **ENOENT** at any step is ignored and not counted (no failure, no count). A target that vanished between identity check and unlink gives `Removed` with zero counts.
- **Stop:** checked before every entry; if the walk stops early the outcome is `Partial` even with no failures. `submit` resets the stop flag; `stop()` while idle has no effect.
- **`busy` is cleared before `Finished` is sent**, so the receiver can resubmit immediately; `wait_idle` can return true while `Finished` is still in flight.
- **Progress** uses `try_send` (skipped when the channel is full), first event after >= 100 ms.
- `Drop` for `DeleteHandle` sets stop, closes the request channel and joins the thread.
- `total_files`/`total_dirs` in the request are not used by the executor.
- If the thread cannot be spawned, `submit` returns `Busy` (the contract has no other error).
- A panic inside the executor would leave `busy` set; not handled.

## Limits to know
The stop test is probabilistic at integration level (accepts `Removed` if the delete finished first); the deterministic version is the unit test `stop_between_entries_gives_partial_without_failures`.
`tests/delete_executor.rs` serialises its tests with a global mutex because it counts process fds.

## Review fixes (2026-10-23)
- **F1** target that is a mount point: after the identity check, the target's (dev, mnt_id) is compared with the parent's (`Meta::same_mount_as`); mismatch -> `Aborted(MountPoint)` (new `AbortReason` variant), nothing removed. Red first: `mount_point_as_the_target_is_refused` (tmpfs and same-device bind) failed. The same-device bind mount *inside* the target (`mount_same_device_bind_inside_target_is_not_descended`) passed straight away; it exercises the existing mnt_id check. All three need `unshare -rm`: `cargo test --test delete_executor -- --ignored mount` (they re-run themselves in the namespace, SKIP if unavailable).
- **F2** `guarded()` wraps the executor in `catch_unwind`: a panic gives `Finished(Partial{removed: last counts, failures: [target, Other(EIO)]})` and clears busy. Unit-tested via the closure (red = the function did not exist).
- **F3** residual statx/unlinkat race documented in a comment at the unlink call.
- **Bytes**: new test `bytes_for_a_file_with_all_its_links_inside_the_target_count_once` passed without code change (the last link sees nlink == 1 and carries the bytes).
- **Hang fix**: `Finished` is sent with a `try_send` retry loop that gives up once `stop` is set; `Drop` sets stop and waits at most 2 s for the executor to exit, then detaches instead of joining. Red first: `dropping_the_handle_returns_even_if_nobody_reads_the_events` timed out ("Drop hung on a full channel"). State is now one `Mutex<State{busy, exited}>` + Condvar.
- **NOFOLLOW** pinned by `delete::sys::tests::opening_a_symlink_to_a_directory_is_refused` (expects `ENOTDIR`); removing `NOFOLLOW` from the open flags makes it fail (verified).
- Process note: I ran `git checkout src/delete/mod.rs` by mistake, which reverted my uncommitted mod.rs to the T001 stub; I rewrote it from scratch with the fixes. No other files were affected.

## Round 2 additions
- Panic test with a real second delete: `spawn_with(tx, run)` takes the run function, so `a_panicking_delete_still_finishes_and_the_next_one_works` injects a panic for request 1 and a normal result for request 2. Removing `catch_unwind` makes it fail (verified). Panic outcome is `Partial{last counts, [target, Other(EIO)]}` (architect's wording) rather than `Aborted(OpenFailed(EIO))` from the brief text; say if you want the latter.
- `bytes_are_the_blocks_of_single_link_files_and_directories`: single-link files + dirs' own blocks; a file with an outside hard link adds 0. Passed without code change.
