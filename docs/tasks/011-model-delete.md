# T011: Model delete (delete_check, refusals, reconciliation incl. aborted unfinished delete)

- **Engineer:** engineer-1. **Wave:** 2. **Depends on:** T005, T010, T012. **Blocks:** T021.
- **Review:** second reviewer required (design §8), together with T010.

## Goal
The model side of M7 (v0.5): the confirmation numbers, refusals, the tombstone lifecycle, and reconciliation for every `Outcome`. This includes **Odin gap 1**: an aborted delete of an unfinished folder clears the tombstone and resumes that subtree's scan (plan D1).

Acceptance criteria:
1. `delete_refusal(id)`:
   - `ScanRoot` for the root;
   - `OtherFilesystem` for `OTHER_FS`;
   - `AlreadyDeleting` if the node or any ancestor is `DELETING`/`TOMBSTONE`;
   - `DeleteRunning(target)` while another delete runs.
2. `delete_check(id) -> DeleteCheck`:
   - `exact` = `is_complete && is_exact`;
   - `files`, `dirs`, `size_bytes` in the `mode` passed in;
   - `largest_known`: the 3 largest current children by the current size mode;
   - `outside_links` = the number of files in the subtree with `nlink > 1` whose occurrences inside the subtree (from the hard-link table) are fewer than `nlink`;
   - `freed_bytes_upper` = disk size minus those files' disk bytes.

   Cost is O(subtree). Measure it on a synthetic 1 M-node subtree and print `TARGET: delete_check_ms_per_1M=… (target 50)`. This does not gate.
3. `begin_delete(id)` re-checks the refusals, marks the target `DELETING` and `TOMBSTONE`, and returns a `DeleteRequest`:
   - `rel_path`;
   - `expect_dev`/`expect_ino`;
   - `kind`;
   - `total_files`/`total_dirs` = `Some` only if the check is exact.

   The app cancels the scan prefix (UX F4 "After `y`").
4. **While tombstoned:** `DirIssue`s and entry issues under it are discarded (design §3.5). `Cancelled` under it decrements pending only (no `inexact`). Listings under it are still applied, so the progress numbers can grow.
5. `apply_delete(&Finished(outcome)) -> Reconcile`:
   - **`Removed`:**
     - remove the subtree by reusing T012's `remove_subtree(id, keep_root = false)` (`REMOVED` holes; do not duplicate it) and subtract its aggregates, counts and pending/inexact contributions from the ancestors;
     - subtract the ext totals;
     - re-charge hard links whose counted node was removed to a surviving `others` entry, adding its sizes back along its own ancestors;
     - call `recompute_top()`.

     Returns `Reconcile::None`.
   - **`Partial`:** clear the tombstone flags, add the failures to the issue list (they must never be silent), and return `Reconcile::Rescan(id)`. The app then runs `begin_rescan(id)` (T012).
   - **`Aborted` (D1, Odin gap 1):** clear `DELETING`/`TOMBSTONE` on the target **and every node under it**, and return `Reconcile::Rescan(id)`. After the app's rescan, the subtree is exact again. This is because confirming cancelled its pending jobs, and without a rescan it would stay incomplete forever.
   - `Progress` events return `Reconcile::None` and don't change the model.
6. A failed delete must not leave any node `DELETING`. Deleting a top-level dir reduces `top_level_progress().1` (total), and its `done` contribution too if it was complete. Add a test.
7. **Invariant (design §3.8, tested):** after any delete, the model totals equal `du` on the remaining tree.

## Design references
§3.8 Refused targets/Reconciliation/Invariant, §3.10 hard-link note, §3.11 O3 mitigations + dialog `p`, §4 `DeleteCheck`. ADR-007, ADR-009. Plan D1.

## Files owned
`src/model/mod.rs` (fill in the T011 stubs), `src/model/delete.rs`, `tests/delete_model.rs`.

## Tests to write first (`tests/delete_model.rs`)
- **Refusals:** the root, `OTHER_FS`, inside a running delete, and a second delete each produce the right `Refusal`.
- **`outside_links`:** a file with 2 links both inside → 0. One inside and one outside → 1, and `freed_bytes_upper` excludes its size.
- **`delete_check` exact vs unfinished:** mid-scan → `exact = false`, and the counts are lower bounds (≤ the final counts). After completion → `exact = true` and the counts equal the final ones.
- **Confirm → removed (real fs):** scan a fixture, delete a subdir via `DeleteHandle` (T010), apply events → the model totals equal `du -s -B1` on the remaining tree (disk and apparent). Repeat with a hard link whose counted copy is inside the deleted subdir and whose other copy survives: the totals still equal du.
- **Decline:** opening `delete_check` without `begin_delete` changes nothing (model hash before == after), and the fixture tree hash is unchanged.
- **Early delete (design §4):** start a scan on a 20 k fixture with `threads = 1`, `begin_delete` an unfinished subdir, cancel its prefix, run the executor → the subtree is gone. After the scan completes, the totals equal du on the remaining tree.
- **Odin gap 1, aborted unfinished delete:** same setup, but swap the target for another dir before submitting (so the identity check fails) → `Aborted(ChangedSinceScan)`. Then `apply_delete` returns `Rescan(id)`. After the app-side sequence (`remove_prefixes_under`, `begin_rescan`, `rescan`), no node is `DELETING`/`TOMBSTONE`, the subtree reaches `is_complete && is_exact`, and the totals equal du on the (unchanged) tree.
- **Aborted(MountPoint):** `apply_delete` treats it like any `Aborted`: it clears the tombstones and returns `Rescan(id)`. The totals after the rescan equal those before the delete (unit test with a synthetic event; no mount needed).
- **Partial:** a `chmod 555` subdir → `Rescan(id)`, the failures are in `issues()`, and after the rescan the totals equal du.
- **Late messages:** a `Listing` that arrives after `Removed` for a dir inside the removed subtree is ignored.

Commands: `cargo test --test delete_model`. To print the timing: `cargo test --release --test delete_model -- --nocapture delete_check_timing`.

## Out of scope
Dialogs and key handling (T021). Executor internals (T010).

## Note from the T005 review
`recompute_top()` does not touch ext totals. `Removed` must subtract the ext totals of the removed files, through T012's `remove_subtree` and `Indexes::remove`. A re-charged hard link must be added back to the ext totals and the top lists.

## Note (2026-10-23, re-order): fixtures
T011 now runs **before** T007. If your real-filesystem tests need a fixture builder, create **`tests/support/fixtures.rs`** in this task, with only what you need (hard links in and out of a subtree, a `chmod 555` dir, plus a helper that runs `du -s -B1`). T007 then takes the file over and extends it. Include it with `#[path = "support/fixtures.rs"]`.
