# T003: Model core (arena, apply, aggregation, completeness, hard links, issues)

- **Engineer:** engineer-1. **Wave:** 1. **Depends on:** T001. **Blocks:** T005, T007, T008, T012, T015, T016.
- **Parallel with:** engineer-2's T013 and T014.

## Goal
Implement the arena tree and `Model::apply(ScanMsg)` so that live partial totals are correct (M1, M3, M4) and match du (M8). This also covers the `is_complete` and `is_exact` flags and the hard-link and issue bookkeeping.

Acceptance criteria:
1. `Model::new` creates `NodeId::ROOT` from the root `RawEntry`, with `pending_dirs = 1` (the root itself is discovered but not yet listed). Ids from scanner listings start at 1.
2. `apply(Listing { dir, first_id, entries })`:
   - creates nodes `first_id..first_id+len` as children of `dir` (growing the vectors as needed);
   - adds each entry's own sizes and counts to `dir` and every ancestor. **Batch per listing:** sum the listing first, then walk the ancestors once. That makes it O(depth) per listing, not per entry;
   - marks `dir` `DONE`;
   - for each child dir that will be descended, `pending_dirs += 1` on it and its ancestors. Then `dir` and its ancestors get `-= 1` for `dir` itself.
3. Sizes follow design §3.4 and §1. Files: disk = `st_blocks*512`, apparent = `st_size`. Dirs: disk = own `st_blocks*512`, apparent = **0**. Symlinks: their own size. `agg_*` includes the dir's own size.
4. Counts: `files` counts non-directories (files, symlinks, other, and hard-link duplicates) in the subtree. `dirs` counts subdirectories in the subtree. Neither includes the node itself.
5. **Hard links:** a non-directory with `nlink > 1` goes into `HashMap<(dev, ino), HardLink { counted, others }>`. The first one applied is charged. Later ones get `HARDLINK_DUP` and own sizes 0.
6. **Issues:**
   - `DirIssue { dir, op, kind }` adds a `ScanIssue`, sets `ERROR` and the node's `issue` index, and counts as listed (`DONE`, pending `-= 1`), so the dir is complete with a known-so-far size.
   - An entry whose `RawEntry.issue` is set gets its own `ScanIssue`.
   - An entry dir with `OtherFilesystem` gets `OTHER_FS`. It is **not** pending and **not** counted in sizes (`du -x`).
7. `Cancelled { dir }` (generic part): the node becomes `INCOMPLETE`, pending `-= 1` up the chain, and `inexact_dirs += 1` up the chain. T012 adds the user-skip marking and T011 the tombstone case.
8. `Done { cancelled }`: the state becomes `Done` or `Stopped`. With `cancelled`, every dir that is not `DONE` gets `INCOMPLETE`. `pending_dirs` stays unchanged, so `◌` stays (UX §5.5), and those dirs are not exact.
9. `is_complete(id)` = file, or (`DONE` and `pending_dirs == 0`). `is_exact(id)` = `is_complete` and `inexact_dirs == 0`.
10. `children_sorted(id, mode)` orders by (size desc, name bytes asc). `path`/`rel_path` are rebuilt from parents and names, using raw bytes (`OsStr::from_bytes`).
11. `totals()` returns entries seen, files, dirs, root `pending_dirs`, and the root aggregates. `state()` returns `Starting` before the first message, `Scanning` after it, then `Done`/`Stopped`.
12. Names live in one byte arena (no per-node `String`/`PathBuf`). Choose the physical layout you want (split file/dir tables are welcome). Design §5 sets a **target** of ≤ 120 B/entry, which T008 measures.

## Design references
§3.3 (ordering contract, id allocation), §3.4 (all of it), §3.10 per-folder complete flag, §3.11 `is_complete` vs `is_exact`. ADR-004.

## Files owned
- `src/model/mod.rs` (taken over from T001), `src/model/tree.rs`, `src/model/names.rs`, `src/model/hardlink.rs`
- `tests/model_apply.rs`
- Leave the T005, T011 and T012 method bodies as their `todo!` stubs.

## Interfaces
- **Consumes:** `ScanMsg`, `RawEntry`, `NodeId`, `Kind`, `IssueKind`, `ScanIssue`, `ScanState` (T001).
- **Provides:** the read API of `Model` used by T005, T011, T012 and every UI task. Signatures are frozen in T001.

## Tests to write first (unit + `tests/model_apply.rs`)
- A hand-built message stream for a 3-level tree. After each message: per-dir `agg_disk`, `agg_apparent`, `files`, `dirs` and `pending_dirs` are as expected, and `is_complete` flips exactly when the last descendant dir is listed.
- Partial totals during the stream equal the sum of what has been applied (live correctness, M1).
- Dir apparent = 0. Dir disk includes its own blocks.
- Hard links: two links in different dirs. Only the first applied is charged, root totals count it once, and the second carries `HARDLINK_DUP`.
- `DirIssue(PermissionDenied)` → complete with an issue. `issues()` has one entry with the right node, op and kind.
- An `OtherFilesystem` entry → `OTHER_FS`, not counted, not pending, and the parent can complete.
- `Cancelled` → the parent completes but `!is_exact`.
- `Done { cancelled: true }` with unread dirs → `Stopped`; the unread dirs and their ancestors are `!is_complete`.
- `children_sorted` breaks ties by name; the order is stable across repeated calls.
- `path()` round-trips invalid UTF-8 and newline names byte for byte.
- Property test (proptest): for random trees streamed in any parent-before-child order, the final root totals equal the sum over all non-duplicate entries, and every dir `is_complete`.

## Out of scope
Ext/category/top-N indexes (T005). Skip/resume/rescan/focus (T012). Delete (T011). Thread safety (the model is single-owner).
