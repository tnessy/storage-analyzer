# T025 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree: HEAD plus the T025 files):
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: ui_tree 31, ui_treemap 29, lib 92, …;
- release build ok.

**Red first:** 10/11 new ui_tree tests and 6/7 new ui_treemap tests failed first. The two exceptions are justified: `⌫`-selects-the-folder-left was already true after T016, and the never-fires notice test asserts an absence.

**Real-terminal run** (by me, release binary under a pty at 110×30, on `/usr/share/doc`). Keys: `j ⏎` (zoom), `k` (onto `..`), `p`, `⏎` (up), `Tab`, `Tab`, `q y`. Results:
- the `..  (up to …)` row appeared;
- `p` on `..` showed `".." is the parent folder. Press ⏎ to go up.`;
- the map title carried ` · ⏎ goes up to …`;
- the SGR stream uses reverse, bold and underline, and **no SGR combines dim with reverse**;
- 9 distinct RGB fills were drawn (base and alternate shades);
- exit 0, no panic.

**Pass 1, spec:** checks 16, 17 and 18 are each covered by tests:
- the full-width reversed+bold row with no DIM, and bold+underline when the map has focus;
- the reversed+bold frame on the selected tile with `>` and its fill kept;
- the grout frame on default colours;
- the 8 alternates from §7, assigned deterministically, with no same-fill neighbours in the 6-tile fixture;
- contrast ≥ 4.5, and ≥ 7 for the alternates;
- the `..` row: pinned, never in the aggregate, all its keys, the map behaviour, `Tab` selecting the largest tile.

**Accepted deviations:**
- `src/ui/list_common.rs` was touched (not in the file list). It applies the same §7 "reverse rows never use DIM" rule to the list views, which is consistent and small.
- The `..` interception lives in `state.rs` ahead of the key handlers, not in `keymap.rs`. That is fine: T020 and T021 keep it first.
- NO_COLOR isn't read yet, so the tests assert attributes only. Reading the variable is T023's job.
- A selected tile under 2 cells has no frame and is drawn reversed+bold on its fill. That is sensible, because there is no frame to style.
