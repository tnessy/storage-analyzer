# T032 (PROPOSED, recommended): Stop re-sorting huge folders on every frame

- **Engineer:** engineer-2. **Depends on:** T016, T009. **Status:** proposed. A 50 ms target; this was first flagged in the T016 review.

## Problem
The tree's row build sorts an expanded folder's children on every key and frame: 5.3 ms at 100 k children, **83.8 ms at 1 M**, against the 50 ms input target.

## Acceptance
1. **Cache the sorted order** per expanded folder in `TreeState`. Re-sort only when the folder's content or sizes changed since the last sort (a per-dir change counter from the model, or the folder's aggregate size and child count as a cheap key) or the size mode changed.
   - While scanning, re-sort at most at the data-redraw rate.
   - A key press alone never re-sorts.
2. **Only the visible window** plus the 50-row page is materialised; the sort itself stays O(n log n) when needed.
3. **Behaviour unchanged:** order (size desc, name asc), selection by NodeId, the aggregate row, the `..` row, the pinned selected row.

**Tests first:**
- with a model whose sizes don't change, 10 key presses cause **1** sort (counted through a test hook);
- a size change causes a re-sort at the next data frame;
- all existing ui_tree tests stay green;
- harness or bench: row build with 1 M children under 50 ms after the first frame (`TARGET:` line).

## Files owned
`src/app/tree_state.rs`, `src/ui/tree.rs`, `tests/ui_tree.rs`. If a model change counter is needed, ask Heimdall first: `src/model` is engineer-1's.
