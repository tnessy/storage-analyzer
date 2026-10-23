# T020: Scan-control keys (p, zoom, x, r, s), messages, one-filesystem hint

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T016, T018, T012 (model controls), T006.

## Goal
The UI side of brief M11 and the stop Should-item, plus the **new one-filesystem status hint** (plan §1.6, D4 as re-worded and approved by UX in `ux/spec.md` §5.2).

Acceptance criteria:
1. **`p`** (UX F5) on a folder row or tile toggles the priority:
   - set: `ScanControl::set_focus(Some(path))` + `Model::set_focus(Some(id))` + `AppState.priority = Some(id)`. The message is `▲ Scanning .cache/ first.`;
   - pressing it again on the current priority clears it (`Priority cleared.`);
   - a new `p` or zoom replaces the old priority.
2. **Zoom prioritises** (M11, UX F2): the T016 `ZoomChanged` event sets the focus to the new view root. Zooming back up to the scan root clears it.
3. **Priority done:** when `Model::take_focus_completed()` returns the node, show `✓ .cache/ fully scanned: 64.9 GiB · 211,004 files` for 5 s and clear `▲`. While a priority is set, the status line prefix is `▲ First: .cache/`, and the bar is dropped first if space runs out.
4. **`x`:** `ScanControl::cancel_prefix(path)` + `Model::mark_skipped(id)`. The message is `⊘ Skipped .local/. Its size is partial (≥). r resumes.` No confirmation is asked. Two other cases (UX §5.4):
   - folder already skipped (its issue is `SkippedByUser`): don't call the model; show `Already skipped. r resumes.`
   - folder already complete: `mark_skipped` returns false; show `Already fully scanned.` (D6).
5. **`r`** on a folder, or on a `skipped by you` row in Skipped, runs the rescan sequence:
   1. `remove_prefixes_under(path)` (D2);
   2. `Model::begin_rescan(id)`;
   3. `ScanHandle::rescan(id, path, depth)`.

   The message is `◌ Resuming .local/…`. `r` on the root after a stop is a full rescan. Put this sequence in **one** function, `app::rescan(id)`; T021 reuses it for `Reconcile::Rescan`.
6. **`s`:** `ScanControl::cancel_all()` → the status becomes Stopped (UX §5.2).
7. `p`/`x`/`r` on a file or aggregate → `p, x and r work on folders.`
8. **One-filesystem hint** (UX-approved, `ux/spec.md` §5.2 "Other filesystems not scanned"). All of its text lives in **one constant** (template) in `src/ui/copy.rs`.
   - **Shown when:** at least 1 `OtherFilesystem` issue exists and `--cross-filesystems` is off. It is appended to **any** status state.
   - **One skip:** ` · not scanned (other fs): <path>`. `<path>` is the absolute path of the shallowest skipped entry (ties: first by name).
   - **Several skips:** ` · not scanned (other fs): <path> +N`, where N is the number of other skipped entries.
   - **Narrow widths:** drop the bar first. If it still doesn't fit, use ` · K other fs not scanned` (K = total).
   - **Never shown** with `--cross-filesystems`. The full list is in `4 Skipped`.
   - **Example:** scanning `/` on the user's machine shows ` · not scanned (other fs): /home`.

## Design references
§3.5 (one filesystem, subtree rescan), §3.11, ADR-008, ADR-009. UX §4 F2/F5, §5.1, §5.2, §5.4, §11 checks 9–10. Plan D2, D4, D6.

## Files owned
`src/app/controls.rs`, `src/app/keymap.rs` (p/x/r/s), `src/app/state.rs`, `src/ui/status.rs`, `src/ui/copy.rs`, `tests/app_controls.rs`.

## Tests to write first (`tests/app_controls.rs`; headless app with `FakeFs` + the real walker, `threads = 1`)
- **UX §11 check 9:** `⏎` on a folder sets `ScanControl.focus` to its path. `⌫` back to the root clears it. `p` toggles it.
- **M11 prioritise:** in a fixture where the folder hasn't been scanned yet, pressing `p` makes its contents (its child listings) appear before the other pending folders finish.
- **UX §11 check 10:** `x` → the row shows `⊘`, `(scan skipped)` and a `≥` size, and every ancestor shows `≥`. After `r` and completion, all of those marks are gone and the totals equal an unskipped run.
- **M11:** after `x`, no listing for that subtree is applied.
- **`s`** → Stopped text, and `◌` stays on the unfinished rows. Then `r` on the root → Done with full totals.
- **Priority done** message text, and `▲` is cleared.
- **One-filesystem hint:**
  - a `FakeFs` rooted at `/` with a different-`dev` child `home/` → the status contains ` · not scanned (other fs): /home`;
  - with 3 such entries at different depths → it names the shallowest path and ends `+2`;
  - at 80 columns with a long path → first the bar is dropped, then the text becomes ` · 3 other fs not scanned`;
  - it shows in the Starting, Scanning, Done and Stopped states;
  - with `--cross-filesystems` there is no hint.
- **`x` messages:** `x` on a skipped folder → `Already skipped. r resumes.`, and `mark_skipped` isn't called. `x` on a complete folder → `Already fully scanned.`
- `p` on a file → the `p, x and r work on folders.` message.

Command: `cargo test --test app_controls`.

## Out of scope
The delete dialog's `p` (T021). Model internals (T012).

## Amendment (2026-10-23): `r` on a folder still being scanned (O7 closed, design §3.11)
- **`r` on a folder or tile:** call `Model::rescan_plan(id)` (T012) and execute it:
  - **`Full`:** the existing rescan sequence (`app::rescan(id)`). The message is `◌ Resuming …` or the rescan text from UX §5.4.
  - **`PrioritiseAndResume { resume }`:**
    - set the priority exactly as `p` does (`ScanControl::set_focus`, `Model::set_focus`, `AppState.priority`). Unlike `p`, it doesn't toggle: if the folder is already the priority, it stays;
    - then, for each node in `resume`, run `app::rescan(node)`;
    - **never** call `ScanHandle::rescan` for the folder itself;
    - show the status message **from UX**. The architect's placeholder is `Still scanning: .cache/ moved to the front`. UX owns the wording and has been asked, so take the text from `ux/spec.md` §5.4 when it lands, and ask Heimdall if it's missing.
- **Skipped list (`4`):** `r` on a `skipped by you` row is the `Full` path for that row's node, as before.
- **Tests (red first):**
  - `r` on an unfinished folder sets the focus, shows the message, and calls no rescan for that folder. Use a recording `ScanStarter` handle, or count listings per path with `FakeFs`.
  - `r` on an unfinished folder that has a skipped child resumes only that child.
  - `r` on a complete folder does a full rescan.
