# T016 review (Heimdall, 2026-10-23): CHANGES REQUESTED (two items)

**Fresh evidence** (clean worktree: HEAD plus the T016 files):
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: lib 90, app_loop 19, ui_tree 17, app_panic 1, model/scan/delete/guard suites unchanged;
- release build ok.

The red-first evidence is reported (14/15 tree tests, 3/3 sanitizer tests, and the real-scanner exit test). The viewport mutation is caught after the test was strengthened.

**End-to-end smoke test (by me, pty 100×24, the release binary on `/usr/share/doc`):**
- the scan reached Done;
- the root row reads `371.7M`, and `du -s -B1` gives 389,734,400 B = 371.7 MiB, **an exact match**;
- `j` moved the selection, and `q y` exited 0.

This is the first build that scans for real.

**Required changes:**
1. **Remove the root-name workaround.** T005 is committed (`24222f9`), and `Model::path(root)` now returns `root_path` with no trailing slash. Delete the `ui::root_label` workaround and use `Model::path(root)` in `ui/mod.rs`, `ui/tree.rs` and `ui/header.rs`, sanitized. Add a test: the root row and the header show `/synthetic/root`, not `/synthetic/root/`. It should fail first if the workaround were still needed; at minimum, assert the exact text.
2. **Draw the tree guides.** `ux/spec.md` §3.1 (mockup A1) draws guides: `├─▾ .cache/`, `│  ├── yay/`, `│  └── (12 smaller, 10.2G)`. §7 says tree guides are dim decoration. The current 2-space indent doesn't match the spec, so this is required.
   - Use `├─`, `└─` and `│` with the twisty (or `──` for leaves), as in the mockup. The last child, and the aggregate row as the last row, use `└─`.
   - Put the guide glyphs in `Glyphs` (theme), so that T023's ASCII fallback (`+-|`) can swap them.
   - Render them in the dim style.
   - Update the snapshot, and add a test that a two-level expansion produces the mockup's guide pattern, including `│` continuation lines and `└` on the last sibling.

**Rulings on the other concerns:**
3. `(deleting…)` is rendered from `DELETING` but untested until T011/T021: **accepted.** T021 covers it.
4. The aggregate row is a cursor stop (`selection` = its folder, `tree.aggregate = Some(dir)`): **accepted.** T017 is told to treat it as "aggregate of that folder selected", so the map highlights the `(+N smaller)` tile.
5. Expanded folders are re-sorted on every key and frame: **accepted for now.** T009 (R2) measures the row-build time with one expanded folder of 100 k and 1 M children. If an input-driven frame then exceeds the 50 ms target, cache the sorted order per dir, invalidated by a per-dir change counter, in a follow-up.

**Also accepted:** `App::finish()` split out of `run()`; `PgUp/PgDn` by pane height − 1; `production_starter` made public; the new snapshot file under `tests/snapshots/` (insta).
