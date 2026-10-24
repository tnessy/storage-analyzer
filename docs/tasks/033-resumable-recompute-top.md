# T033: Resumable `recompute_top` after a delete (no ~0.5 s freeze)

- **Engineers:** **engineer-1 (model part)**, then **engineer-2 (Largest view part)**.
- **Depends on:** T030. **Schedule:** engineer-1 after T031; engineer-2 after T032.
- **Status:** v1 **pending details**. The architect's note was cut off at this item, and the team lead is asking for the rest. UX is asked for the wording of the `updating…` state. **Do not start until Heimdall amends this brief with both.**

## Problem
`recompute_top` takes 0.4–0.6 s at 20 M entries, runs after every `Removed`, and freezes the UI for about half a second at the end of a big delete.

## Direction (architect, design §5 "Post-delete index refresh")
- **Model (engineer-1):** make the rebuild **resumable**. Start it on `Removed`, then advance it by **≤ 5 ms of work per tick** from the event loop (`Model::advance_top_rebuild(budget) -> bool done`, with the exact API from the architect).
  - While it runs, `top_files()` returns the **old list minus removed nodes**, or another rule the architect gives.
  - **Test:** the result equals the one-shot rebuild, and every step stays within budget on a 1 M-file synthetic model.
- **App/UI (engineer-2):** call the advance step every tick while it is pending. The Largest view shows the UX wording (pending; working text `updating…`) until it is done. **Test:** the view shows the state, then the final list.
