# T012: Model scan controls (skip, resume, rescan, focus, stop)

- **Engineer:** engineer-1. **Wave:** 2. **Depends on:** T003, T005 (ext/top-N subtraction), T006. **Blocks:** T011, T020.

## Goal
The model side of brief M11 and design §3.11: skip (`x`), resume and rescan (`r`), the priority-folder completion notice, and correct `is_complete`/`is_exact` through all of them.

Acceptance criteria:
1. `mark_skipped(id)` on an incomplete dir sets the `SkippedByUser` issue on `id` (it appears in `issues()`, so the Skipped view lists it as "skipped by you"). The app cancels the prefix in `ScanControl` itself. On a complete dir it is a no-op that returns `false`, and the UI shows the D6 message.
2. `Cancelled { dir }` under a skipped dir uses the T003 generic path. Result: the skipped dir and all its ancestors are `!is_exact` once nothing else is pending, so they get `≥` and no `◌`.
3. `begin_rescan(id)` (used for `r` resume, `r` rescan, `r` on the root after a stop, and delete reconciliation):
   - removes `id`'s children subtree from the model, marking the nodes `REMOVED` (never reused), subtracting their aggregates, counts, ext totals and top-N from the ancestors, and dropping their issues;
   - re-charges hard links whose counted occurrence was removed to a surviving `others` entry;
   - clears `SkippedByUser`, `INCOMPLETE`, **`DONE`** and the `inexact_dirs`/`pending_dirs` contributions of `id`, then **sets `PENDING`** and `pending_dirs += 1` up the chain. T003 ignores a `Listing` for a dir that is already `DONE` (architect-confirmed), so without this the rescan's listing would be dropped. Test: `begin_rescan` followed by a fresh `Listing` for `id` is applied;
   - returns the `(path, depth)` the app passes to `ScanHandle::rescan`.

   Put the subtree-removal logic in one function (`remove_subtree(id, keep_root: bool)`) in `src/model/control.rs`. T011 reuses it for `Removed`; do not duplicate it.

   Per D2, the app also calls `ScanControl::remove_prefixes_under(path)`.
4. **Rescan identity (plan D11 as accepted, design §4/§3.8):** handle `ScanMsg::Identity { node, entry: RawEntry }`. This is T012's `apply` arm; T006 lands the variant first.
   - **Update the node from `entry`:** kind, dev, ino, nlink, own sizes. Apply the size deltas to the ancestors, the ext totals and the top-N, so the totals stay exact.
   - **Dir became a file, symlink or other:** the node becomes that leaf entry with fresh sizes. Its old children are already removed by `begin_rescan`. It is complete, has no pending work (`pending_dirs` contributions retired up the chain), and the counts move the node from `dirs` to `files` up the chain. `top_level_progress` stays correct if it was a top-level dir.
   - **File target rescanned** (after an aborted file delete): same update path.
   - **Inode change (dev, ino differ):** unregister the old (dev, ino) from the hard-link table, re-charging another occurrence if this one was counted, and register the new one when nlink > 1.
   - **`DirIssue { Vanished }`:** removes the node and its subtree **only when the node is the current rescan root**. Track the open rescan roots, set by `begin_rescan`. Anywhere else, `Vanished` is a normal issue (a skipped path), as in T003.
   - **Tests (red first):**
     - dir replaced by a file: the node becomes `Kind::File` with the new sizes and totals stay exact. **A delete retry then succeeds**: `begin_delete` builds a request with the new identity and kind, and T010's executor accepts it (integration via `DeleteHandle`).
     - dir replaced by a symlink: the node becomes a `Symlink` entry with its own size and no children, and nothing behind it is scanned.
     - rescan root vanished: the node is removed, ancestors are reduced, and it is not counted.
     - `Vanished` for a non-root dir during a rescan: recorded as an issue, and the node stays.
     - inode change on a hard-linked file: the table holds only the new (dev, ino), and the totals are exact.
5. `top_level_progress` (T003) stays correct through skip, rescan and stop: a rescan of a top-level dir decrements `done` until it completes again, and a skipped top-level dir counts as done (it is complete, not exact). Add a test.
6. Late messages for `REMOVED` nodes, or for listings of dirs no longer in the model, are ignored without panicking.
7. `set_focus(Some(id))` / `set_focus(None)`: when the focus node goes `!is_complete → is_complete`, `take_focus_completed()` returns it once, and the focus clears. The UI shows `✓ … fully scanned` (UX §5.4).
8. After `Done { cancelled: true }` followed by `begin_rescan(root)` and the scan stream, the model ends `Done` with exact totals.

## Design references
§3.5 Subtree rescan, §3.11 mechanics (`Cancelled`, `r` resume, `is_complete` vs `is_exact`, priority folder done), D2. ADR-009.

## Files owned
`src/model/mod.rs` (fill in the T012 stubs), `src/model/control.rs`, `tests/model_controls.rs`.

## Tests to write first
Model-level tests use hand-built streams. The integration tests use `FakeFs` + walker (`#[path = "support/fake_fs.rs"]`).
- **M11 skip:** skip `/r/a` mid-scan. `/r/a` and every ancestor end `is_complete && !is_exact`, `/r/a` has `SkippedByUser`, and the totals equal the brute force over what was listed.
- **M11 resume:** after the skip, `begin_rescan(/r/a)` + `remove_prefixes_under` + `rescan` → the final totals equal a full unskipped scan, `is_exact` everywhere, and no `SkippedByUser` remains.
- **Rescan clears inner skips** (D2): skip `/r/a/b`, then rescan `/r/a` → `/r/a/b` is scanned.
- **Rescan hard-link re-charge:** with a counted link inside the rescanned subtree and a duplicate outside, the root totals stay unchanged during and after the rescan.
- **Focus completion:** `take_focus_completed()` returns `Some` exactly once, at the right message.
- **Stop + root rescan** → exact totals.
- A late `Listing` for a `REMOVED` dir is ignored, and the totals are unchanged.

Command: `cargo test --test model_controls`.

## Out of scope
Key handling and status copy (T020). Delete tombstones (T011).

## Amendment (2026-10-23): `r` on a folder still being scanned (O7, design §3.11)
The architect's rule: `r` on a folder that is **not yet complete** means **prioritise it (`p`) and resume any skipped folders inside it**. No rescan of the folder itself, and no per-rescan generation. Once the folder is complete, `r` is a real rescan (`begin_rescan`).

The rule lives in **one place, the model**, and T020 only executes it:
- **Add** `Model::rescan_plan(id) -> RescanPlan`, a new read method in `src/model/mod.rs`:
  ```rust
  pub enum RescanPlan {
      /// `id` is complete: run begin_rescan(id) + ScanHandle::rescan.
      Full,
      /// `id` is still scanning: focus it, and resume only these skipped folders inside it
      /// (each: remove_prefixes_under, begin_rescan, ScanHandle::rescan).
      PrioritiseAndResume { resume: Vec<NodeId> },
  }
  ```
  `resume` lists the nodes at or under `id` that carry `SkippedByUser`, outermost only (a skipped folder inside another skipped folder is covered by its ancestor). Keep it bounded: walk only the issue list filtered by ancestry, not the whole subtree.
- **Tests (red first):**
  - a complete folder → `Full`;
  - an incomplete folder with no skips → `PrioritiseAndResume { resume: [] }`;
  - with two skipped folders inside it, one nested in the other → only the outer one;
  - a skip *outside* `id` is not included.
- **Acceptance test (integration, `FakeFs` + the real walker, `threads = 1`):** start a scan; skip folder `S` inside `F`; while `F` is still incomplete, run the app-side sequence for `rescan_plan(F)`: focus `F`, then for each resume node `remove_prefixes_under`, `begin_rescan` and `rescan`.
  - **`F` itself gets no new job:** the scan's total `Listing` count equals an unskipped full scan's count plus 0 for `F`'s already-listed dirs. In other words, no dir outside `S` is listed twice. Assert it by counting listings per path.
  - **`S` is listed afterwards.**
  - **The totals at Done equal an unskipped scan.**

**Reconciled with UX §4 F5 / §11 (2026-10-23):**
- **"Still being scanned" means `state() == Scanning && !is_complete(id)`.** After `s` (Stopped), unread folders stay `!is_complete`, but nothing is scanning them, so `rescan_plan` returns **`Full`**. The same holds after Done. Add the test: stop the scan, then `rescan_plan(unfinished folder) == Full`.
- **Root while scanning:** `rescan_plan(root)` returns a third variant, **`RescanPlan::NoneWhileScanning`**. UX says nothing changes; the app shows `Scan in progress. Press s to stop, then r to rescan.` Add the test.
