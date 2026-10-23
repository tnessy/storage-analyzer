# T021: Delete flow, result and quit-during-delete dialogs

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T010, T011, T018, T020, T023. **Last task in the plan.**
- **Review:** second reviewer for the confirmation logic (only `y` deletes).

## Goal
Brief M7 (v0.5) and M12 end-to-end in the UI:
- the confirmation shows path, size and counts (exact, or `≥` lower bounds for unfinished folders), and nothing is deleted until `y`;
- declining changes nothing;
- errors are shown, never silent;
- quitting during a delete needs `y`.

Includes the app half of **Odin gap 1** (plan D1).

Acceptance criteria:
1. **Trigger:** `d`/`Delete` from the tree, map or Largest. First check `Model::delete_refusal`, plus the UI aggregate check, and show the refusal text from UX §5.3 in the status line. Otherwise open the dialog.
2. **Dialog, exact target** (UX §5.3):
   - the path is sanitized and wrapped, never truncated;
   - `Folder · 31.0 GiB (disk usage) · N files · M folders`, or `File · 4.2 GiB (disk usage)`;
   - the hard-link line only when `outside_links > 0`;
   - the "cannot be undone" line;
   - hints.
3. **Dialog, unfinished target:**
   - title `Delete permanently? (scan of this folder not finished)`;
   - every count and size prefixed `≥`, with `(at least; still counting)`;
   - `Largest known inside:` with up to 3 children, or `Nothing listed inside yet.`;
   - the `◌ Everything inside will be deleted…` warning and the hard-link "not fully known" line;
   - the numbers update live, at most twice a second (the `delete_check` recompute is ≤ 2 Hz).
4. **Keys in the dialog:**
   - `y` → `begin_delete` + `ScanControl::cancel_prefix(path)` + `DeleteHandle::submit`;
   - `p` → priority on the target (the T020 path), the dialog stays open, and the hint becomes `▲ Scanning this folder first…`. When the target becomes exact, the dialog **changes in place** to the exact variant;
   - `n`/`Esc`/`Enter` cancel;
   - **every other key is ignored**.
5. **While deleting:**
   - the row shows `◌` + `(deleting…)`;
   - the status line shows `Deleting <path> … 12,400 / 48,212 files`, or `… 12,400 files removed` when `total_files` is `None`;
   - the rest of the UI stays usable.
6. **On `Finished`:** call `Model::apply_delete`. Then:
   - **Removed:** the message `Deleted <path> · freed X (N files)`, or the variant `(…, more than were counted)` when the removed count exceeds the check's count.
   - **Partial:**
     - the `Delete finished with errors` dialog, with failures grouped by kind using the UX labels;
     - `v` opens a scrollable list of the failed paths, and `Esc` goes back;
     - it must be dismissed with `Enter`.
   - **Aborted:** the `Not deleted` dialog with the reason text (UX §5.3), dismissed with `Enter`. This includes the new `AbortReason::MountPoint` and its UX text: `It's a mount point (another filesystem or subvolume is mounted here), so it can't be deleted from here. Unmount it first if you want to remove it.`
   - **`Reconcile::Rescan(id)` → call `app::rescan(id)` (T020).** This is the D1 path for Aborted and Partial.
7. **Quit while deleting** (M12, design §3.8):
   - `q`/`Ctrl-C` open `Quit while deleting?` with live counts (`N of M files removed.` or `N files removed so far.`);
   - **only `y` quits**: any other key, including `q` and `Ctrl-C`, closes the dialog and the delete continues;
   - on `y`: `DeleteHandle::stop()`, `wait_idle(2 s)`, restore the terminal, then print `<bin>: delete of <path> interrupted: N of M files removed` to stderr (no "of M" when unknown).
8. **SIGTERM/SIGHUP during a delete:** the same stop, wait, restore and message, with no prompt.

## Design references
§3.8 (flow, refused targets, quit, signals, reconciliation), §3.10 delete progress/result, §3.11 O3 mitigations + dialog `p`. ADR-007, ADR-009. UX §4 F4/F6, §5.2, §5.3, §5.4, §11 checks 6–8. Plan D1.

## Files owned
`src/ui/delete_dialog.rs`, `src/ui/result_dialog.rs`, `src/ui/quit_dialog.rs` (taking over the T015 quit rendering if it lives elsewhere, move it here), `src/app/delete_flow.rs`, `src/app/keymap.rs` (dialog keys), `src/app/state.rs`, `src/app/terminal.rs` (signal path), `src/ui/status.rs`, `src/ui/copy.rs`, `tests/app_delete.rs`, `tests/app_quit.rs`.

## Tests to write first
In `tests/app_delete.rs`, headless, with a real `DeleteHandle` in a temp fixture:
- **UX §11 check 6:**
  - `Enter`, `n` and `Esc` → no `DeleteRequest` and the tree hash is unchanged;
  - a sweep over every other printable key, plus arrows and `Tab` → ignored, the dialog stays open, no request;
  - `y` → exactly one request.
- **Unfinished:** the target is `!is_exact` → the title contains `scan of this folder not finished`, every count has `≥`, and ≤ 3 known children are listed. `p` sets the focus and the dialog stays open. After the target completes, the title drops `not finished` and the `≥` prefixes disappear (same dialog instance).
- **UX §11 check 7, refusals:** the scan root, an aggregate row, an aggregate tile, an `OTHER_FS` node, and a second delete while one runs each produce the exact text and no request.
- **M7 confirm:** removed from disk, the model totals equal `du -s -B1` of the remaining fixture, and the success message is shown.
- **M7 error visible:** a `chmod 555` subdir → the `Delete finished with errors` dialog with `permission denied N`. `v` lists the paths. After dismissal, the rescan completes and the totals equal du.
- **Odin gap 1:** delete an unfinished folder whose identity changed (swap it before `y`) → the `Not deleted` dialog with the `ChangedSinceScan` text. After dismissal, no row shows `(deleting…)`, the folder's scan resumes, and it ends `is_complete && is_exact` with the totals equal to du.

- **MountPoint (UX §11 check 7):** a target that is a mount point. Use `unshare -rm` with the T010 helper pattern, `#[ignore]`d and run with `-- --ignored`. Result: the `Not deleted` dialog with the MountPoint text and `Nothing was deleted.`, nothing on disk changes, and after the D1 rescan the model totals are unchanged.

In `tests/app_quit.rs`, **UX §11 check 8 / M12:**
- with a delete running (a large fixture, or a test executor that blocks on a latch), `q q` and `Ctrl-C Ctrl-C` do not exit, and the delete continues (more files get removed afterwards);
- `q y` exits, and stderr contains `interrupted: N of M files removed`;
- the quit-while-deleting dialog text matches UX §5.3, including the unfinished `removed so far` variant.

Snapshots at 80×24, with NO_COLOR and ASCII variants, of:
- the exact dialog;
- the unfinished dialog;
- the after-`p` dialog;
- the Partial dialog;
- the failed-paths list;
- the Aborted dialog;
- both quit dialogs.

Commands: `cargo test --test app_delete --test app_quit`.

## Out of scope
Executor and model internals (T010, T011). Trash and multi-select.

## Amendment (2026-10-23): delete exit order (design §3.8 "Handle lifecycle")
On `y` in the quit-while-deleting dialog, and on SIGTERM/SIGHUP during a delete:
1. `DeleteHandle::stop()`;
2. wait at most 2 s for `wait_idle`, **draining `DeleteEvent` while waiting** (poll `wait_idle` in short slices, and `try_recv` the events in between);
3. drop the handle. Its `Drop` is bounded and detaches if needed (T010);
4. apply the T015 scan exit order;
5. restore the terminal;
6. print the stderr line.

The **"interrupted: N of M" numbers come from the last `Progress` received, or the executor's shared counters**, never from `Finished`, which may be dropped on exit.

**Test (red first):** a delete running with a `sync_channel(1)` event channel that the test keeps full → `q y` exits within 2 s, and stderr carries the last known N.

## Amendment (2026-10-23, architect's T010 second review + T015 review)
- **Gate deletes in the model, not on the executor's `Busy`:** `busy` clears just before `Finished` is sent. Use `Model::delete_refusal` (`DELETING`/`TOMBSTONE`, `DeleteRunning`) as the one-delete-at-a-time gate. Treat `submit` → `Busy` only as an unexpected-state fallback (status message, no crash).
- **Caught executor panic:** `Finished(Partial { failures: [target, Other(EIO)] })` opens the normal result dialog. Also show the stored panic message from `PanicSlot` (thread `delete`) as one extra line in that dialog. Note: T015's `check_scan_health` currently `take()`s the whole slot and drops non-scan entries. Change it so `delete`-thread entries are kept for this dialog. Test: an injected executor panic → the result dialog with that line, and the scan is not stopped.

## Amendment (2026-10-23, from T020 rev 2)
**The D1 reconciliation rescan (after `Partial` or `Aborted`) goes through `app::rescan`.** While the app is **Stopping** (see T020), it is queued and runs right after `Done { cancelled: true }` is applied; it is never dropped.

**Delete stays allowed while Stopping** (UX §11 check 9).

**Test:** a delete finishing `Partial` during Stopping → its rescan starts only after `Done`, exactly once.
