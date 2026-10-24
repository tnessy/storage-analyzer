# T020b: Message for `r`, `p` and `x` on another-filesystem folders

- **Engineer:** engineer-2. **Depends on:** **T027** (`RescanPlan::OtherFilesystem`; engineer-1) and T020 (merged).
- **Schedule:** right after T027 merges. **T021 goes first**, because it doesn't depend on T027.
- **Source:** UX `ux/spec.md` §5.4 and check 21 (2026-10-23). This replaces the T027 placeholder arm (`// T020b: message pending UX`).

## Acceptance criteria
1. **On a folder with `OTHER_FS`**, in the **tree** and in the **Skipped** view, `r`, `p` and `x` show the UX §5.4 message and **change nothing**: no rescan, no focus change, no skip, no `ScanControl` change.
   - For `r`, this is driven by `rescan_plan(id) == RescanPlan::OtherFilesystem`. For `p` and `x`, check `OTHER_FS` the same way.
   - The message is `<name>/ is on another filesystem. To scan it, use --cross-filesystems.`, using the **folder's name** (not its full path) with its trailing `/`.
2. **Long names:** the fixed text is 63 cells. At 80 columns, 15 cells remain for the name after the 1-cell margin. A longer name is shortened **in the middle** with `…` (`...` in ASCII), keeping the trailing `/`. `--cross-filesystems.` must always stay intact. Otherwise the text is identical in ASCII.
3. **`d` keeps its existing refusal text** (UX §5.3), unchanged.
4. Remove T027's placeholder comment.

## Files owned
`src/app/controls.rs`, `src/app/keymap.rs`, `src/ui/copy.rs`, and `tests/app_controls.rs` (new tests).

## Tests to write first (UX check 21)
- `r`, `p` and `x` on an `OTHER_FS` row in the tree, and on its Skipped row: the message appears, nothing changes, and no `ScanHandle::rescan` call is made (recording handle).
- **A 40-character folder name:** the message fits in 78 cells, keeps the trailing `/`, and ends with `--cross-filesystems.` intact, in Unicode and in ASCII (`...`).
- `d` on the same row still shows the existing other-filesystem refusal.

**Commands:** the five standard ones.

## Also in T020b (after T028b merges): use `ScanHandle::rescan_many`
- Where the app issues the rescans queued during Stopping (`queued_rescans`, including D1 delete-outcome rescans), call `rescan_many` **once** with all of them, instead of calling `rescan()` in a loop.
- **App-level test (deterministic):** two queued rescans, the first folder tiny. `rescan_in_flight` stays true until **both** folders are complete. Use a gated `FakeFs`.
- If T028b hasn't merged when you reach this, do the message part first and this part after.
