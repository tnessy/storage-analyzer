# T026 review (Heimdall, 2026-10-23): APPROVED with a recorded process violation; committed

**Fresh evidence** (clean worktree: HEAD plus the T026 files):
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: lib 102, ui_treemap 37, ui_lists 17, …;
- release build ok.

**Real terminal** (by me, release binary under a pty at 110×30, on `/usr/share`, with zoom and view switches):
- exit 0;
- folder tiles show `name/ size pct%` with **no tag**;
- exactly **8 distinct fills**, matching the §7 palette byte for byte: #F0E442, #0072B2, #E69F00, #009E73, #56B4E9, #D55E00, #CC79A7, #BBBBBB.

**Pass 1, spec:** checks 17 (a)–(k) and 3 are covered.
- `assign` and `adjacency` are pure, follow the spec's order exactly (keep, then pick from `NodeId mod 7` skipping reserved hues, then least-used neighbour hue, then `Unfilled`), and tie-break deterministically.
- The reserved set is per scope, grows only, and includes tileless files.
- The memory lives in `AppState`.
- The palette has truecolor and 256 values, with contrast ≥ 5.19 / 5.53.
- The Types swatch shows, and is hidden in Mono.
- The help constants are added.

**Process violation (recorded):** the integration side (palette, map wiring, rendering) was written **before** its tests. The pure core and the swatch tests were red first. This is engineer-2's second lapse (after T015). In the T015 review I said the next one would be rejected.
- **I am not rejecting**, and I'm saying so explicitly rather than quietly waiving the rule. Rejection can't produce red-first evidence after the fact. What I need to know is whether the tests bite, so I checked that myself.
- **My mutation checks** (scratch worktree):

  | Mutation | Failures |
  |---|---|
  | (h) folders get a tag | 4 integration tests |
  | (h) folders lose the `/` | 4 integration tests |
  | (g) aggregate not grey | 1 unit + 2 integration |
  | (f) never unfilled | 1 unit + 2 integration |

  Engineer-2's own checks: ignoring R fails 3 tests; dropping memory fails the stability test.
- **Standard from now on, for both engineers:** a hand-off without red-first evidence for some production code is **changes requested** until there is a mutation check for **each** behaviour in that code. That means one targeted mutation per acceptance criterion, each shown failing, and Heimdall re-runs a sample. This is enforceable and gives the same assurance. The pattern (T004, T015, T026) is escalated to the team lead.

**Non-blocking notes:**
- `assign` clones the session-long `memory` map on every relayout (≤ 2 Hz). That is bounded by the folders shown, but taking `&mut` would avoid it. Optional.
- **Unused model code:** `Model::category_of_node`'s folder branch and the `dir_categories` cache now have no caller. Engineer-1 removes them in T012, because they are model code (added to T012).
- **Rescan hook:** `TileColors::forget_reserved(scope)` is the rebuild hook for a rescan. T020's rescan sequence must call it for the rescanned folder (added to T020).
