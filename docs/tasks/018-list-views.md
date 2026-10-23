# T018: Types, Largest, Skipped views

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T016, T005.

## Goal
- **M5:** file types for the whole root, with total size and count, sorted by size.
- **M6:** the largest files under the whole root, with size and path, updating live, N configurable.
- **M10:** the skipped paths, with reasons.

Acceptance criteria:
1. **View tabs** (UX §3): `[1 Tree]  2 Types  3 Largest  4 Skipped !N`, with keys `1–4`. `!N` is the issue count (`Model::issues().len()`).
2. **Types** (UX F3):
   - category rows from `Model::category_totals()`, sorted by size desc, with columns size, files and % of root;
   - `→` expands a category into its extensions (`ext_totals`, filtered by `category_of`), and `←` collapses it;
   - `(none)` and `(other)` are labelled as such;
   - states: `No files yet.`, `No files found.`, and the header shows `partial` after a stop.
3. **Largest:**
   - `Model::top_files(mode)` in order, with size and full path (sanitized, truncated from the **left** with `…` so the file name stays visible);
   - it updates live between frames;
   - `--top N` comes through `ModelOptions.top_n`;
   - `⏎` **shows the item in the tree**: switch to tab 1, set the view root to the scan root, expand the ancestors via `parent`, select it, and scroll it into view (design §3.10);
   - `d` is wired in T021.
4. **Skipped:**
   - path and reason, newest first. The reason text comes from UX/design §3.4: `permission denied`, `vanished during scan`, `other filesystem`, `skipped by you`, `read error: <strerror>`;
   - `r` on a `skipped by you` row is wired in T020;
   - `Nothing skipped.` when empty.
5. **Lists:** `↑↓ j k PgUp PgDn g G` move. The key hints are `↑↓ move ⏎ show in tree d delete ? help`.
6. **Scalability:** don't clone the whole list per frame. Render only the visible window.

## Design references
§3.4 Indexes, Issues, Category; §3.10 "Largest → show in tree", "Skip reasons". UX §3, §4 F3, §5.5, §6.

## Files owned
`src/ui/tabs.rs`, `src/ui/types_view.rs`, `src/ui/largest_view.rs`, `src/ui/skipped_view.rs`, `src/app/list_state.rs`, `src/app/keymap.rs` (view and list keys), `src/app/state.rs` (active view), `tests/ui_lists.rs`.

## Tests to write first (`tests/ui_lists.rs`)
- **M5:** a synthetic stream with known per-type totals. The Types view shows each category's size and file count, sorted by size, with values matching a brute-force recount. Expanding a category lists its extensions with matching totals. The totals cover the **whole root**, not the selected dir: change the tree selection and the view is identical.
- **M5 live:** the values change between two batches of messages while scanning.
- **M6 live:** after batch 1 the list shows the top files so far; after batch 2 a new larger file appears at the top, and the old last one drops out when N is reached. `--top 3` shows exactly 3 rows.
- **M6 whole root:** files from different subtrees all appear, regardless of the tree selection.
- **Show in tree:** `⏎` on a deep file → tab 1 is active, its ancestors are expanded, and `selection` equals that NodeId.
- **M10:** issues of each kind render the exact reason text, newest first. The `!N` badge equals the count.
- Snapshots of each view at 80×24 in the Starting, Scanning, Done and Stopped states, and the empty state.

Command: `cargo test --test ui_lists`.

## Out of scope
The ≥ 120-column Types side panel (**deferred**). Delete from Largest (T021). `r` on Skipped (T020).
