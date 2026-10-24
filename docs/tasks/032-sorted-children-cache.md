# T032: Partial sort plus cached order for huge expanded folders

- **Engineer:** engineer-2. **Depends on:** T016, T009. **Schedule:** after T029.
- **Status:** v1, low priority (architect's T009 assessment, item 4).

## Problem
The tree row build sorts an expanded folder's children on every key and frame: 5.3 ms at 100 k children, **83.8 ms at 1 M**, against the 50 ms input target.

## Acceptance
1. **Partial sort:** plan D5 shows only the 50 largest children plus the aggregate row (or more after `→` reveals). Use `select_nth_unstable_by` to find the top k, where k = revealed rows + the pinned selection, and sort only those k by (size desc, name asc). The aggregate's N and SIZE come from the rest without sorting them.
2. **Cache that order** per expanded folder in `TreeState`.
   - **While scanning,** recompute it **at most twice a second**, like the treemap relayout.
   - **When idle,** recompute only when the folder's content or size changed, or the size mode changed.
   - **A key press alone never recomputes.**
3. **Behaviour unchanged:** ordering, selection by NodeId, the aggregate row, the `..` row, the pinned selected row, and the reveal pages.

## Tests (red first)
- With unchanged sizes, 10 key presses cause **1** sort (a test hook counter).
- While scanning, a flood of size changes over a simulated 2 s causes ≤ 5 recomputes.
- With 1,000 children and k = 50, the rows equal a full-sort reference (property test with random sizes and ties).
- Every existing ui_tree test stays green.
- `TARGET:` row build with 1 M children after the first frame (expected well under 50 ms).

## Files owned
`src/app/tree_state.rs`, `src/ui/tree.rs`, `tests/ui_tree.rs`.
