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
4. **Rescan identity (plan D11, design §3.8):**
   - `ScanMsg::Identity { dir, dev, ino }` updates the node's identity;
   - a `DirIssue { Vanished }` for a dir that is a **rescan root** removes that node (and its subtree) instead of recording an issue;
   - tests for both.

   T012 owns the `apply` arm for `Identity`. T006 lands the variant first.
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
