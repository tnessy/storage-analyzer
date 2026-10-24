# T030: No apply step over ~5 ms: diagnose tick spikes, split large listings

- **Engineer:** engineer-1. **Depends on:** T009 (merged). **Schedule:** engineer-1's **next** task.
- **Status:** **approved by the user for v1** (2026-10-23). Per the architect's T009 assessment; design §4 `Listing` contract, §5 "Max work per apply step" and "Post-delete index refresh". **Order: Part 2 first** (split listings); Part 1 is a cheap diagnosis that can run alongside. Tests first.

## Background
`$HOME` (20 M entries) shows a median apply tick of 0.01 ms, but **118–147 ms** maxima. The 8 ms budget is only checked *between* messages. Two causes fit:
- **(A)** a doubling of the 17 per-node arrays (copying ~1.5 GB at 16 M entries ≈ 100 ms);
- **(B)** applying one listing from a very large folder (300 k entries at 2.7 M/s ≈ 110 ms).

## Part 1: diagnosis (first, and cheap)
In `examples/scan_harness.rs`, log **every tick over 20 ms**:
- the entry count of each message applied in that tick;
- whether node capacity changed during it (compare `Vec::capacity` before and after; add a `#[doc(hidden)] pub fn node_capacity()` on `Model` if needed).

Run it on `$HOME` and record in `spikes/R2-R4-R8-REPORT.md` which cause explains which spike.

## Part 2: split large listings (do it regardless of the diagnosis)
**Contract change, authorized** (design §4): `ScanMsg::Listing { dir, first_id, entries, last: bool }`.
- **Walker (T006 code):**
  - a folder with **> 8,192 entries** is sent as several `Listing` parts of ≤ 8,192 entries each, with contiguous ids. One `fetch_add` for the whole folder is fine; each part's `first_id` is offset.
  - **Child jobs are enqueued only after the `last` part** is sent (ordering contract).
  - Folders with ≤ 8,192 entries send one part with `last: true`.
- **Model (T003 code):**
  - every part adds its entries and aggregates;
  - **only `last` marks the dir `DONE`** and retires its own pending count;
  - the "listing for a DONE dir is ignored" rule still applies to stray parts after `last`;
  - parts for a `REMOVED` or tombstoned dir behave as before.
- **Update every constructor** of `Listing` (tests, `synthetic.rs` and other support code) to pass `last: true`. This touches files of both engineers, so you own these mechanical edits for this task only. Tell Heimdall which files.

## Part 3: chunked node storage, **only if Part 1 shows growth spikes (A)**
- Fixed 64 k-element blocks (`index = i >> 16`, `i & 0xFFFF`) for the per-node arrays and the name arena, so growth never copies existing data. No `Model` API change.
- If Part 1 shows only (B), skip Part 3 and say so.

## Tests (red first)
- **Walker:** a fake folder with 20,000 entries → 3 parts (8,192 + 8,192 + 3,616), contiguous ids, only the last has `last: true`, and no child `Listing` arrives before the last part.
- **Model:**
  - after part 1 of 3, the dir is not complete and its aggregates include part 1;
  - after `last`, it is complete;
  - a part after `last` is ignored.
- **du parity** (T007 suite) still green, with a fixture folder of 10,000 files.
- **Part 3, if done:** growth past several blocks never moves existing names or nodes.
- **Harness:** report `apply tick max` and p99 before and after on `$HOME`. **Target:** max ≤ 50 ms, and no single apply over ~5 ms for split listings. Reported, not gated.

## Files owned
`src/scan/msg.rs` (the contract change), `src/scan/walker.rs`, `src/model/mod.rs`, `src/model/tree.rs`, `src/model/names.rs` (Part 3 only), `examples/scan_harness.rs`, `tests/scan_walker.rs`, `tests/model_apply.rs`, plus the mechanical `last: true` edits listed above.

## Part 4: resumable `recompute_top` (approved; waiting for the architect's API details)
> **Approved by the user for v1 (2026-10-23).** **Sequence:** do Parts 1–3 first. Start Part 4 **only after Heimdall adds the architect's API details here** (requested; pending). The UX spec (§5.5, check 22) is already included below. File ownership for this part: the model, the event-loop call, `src/ui/largest_view.rs` and the `LARGEST_UPDATING` constant in `src/ui/copy.rs`.

(Folded in from the former T033.)
**Problem:** `recompute_top` takes 0.4–0.6 s at 20 M entries and runs after every `Removed`, which freezes the UI for about half a second.
- **Model:**
  - `apply_delete(Removed)` starts a **resumable** rebuild instead of the one-shot one;
  - `Model::advance_top_rebuild(budget: Duration) -> bool` (`true` = done) does ≤ ~5 ms of work per call;
  - `Model::top_rebuild_pending() -> bool`;
  - while pending, `top_files()` returns the previous list **minus nodes that no longer exist**. Ask Heimdall if the architect specifies otherwise.
  - Also make the T012 rescan path use it (`begin_rescan` currently calls the one-shot rebuild).
- **Event loop:** call `advance_top_rebuild(5 ms)` once per loop iteration while pending, after input and scan apply.
- **Largest view (UX §5.5, check 22; wording final):**
  - **Placement:** `updating…` (ASCII `updating...`), right-aligned on the Largest view's **column-header row** with a 1-cell right margin. Not on the tab, not in the rows. It uses the header's own style, with no colour and no blinking (identical under NO_COLOR). Copy constant `LARGEST_UPDATING`.
  - **Show it only if the rebuild is still running 200 ms after it started** (no flash); hide it as soon as the new list is in place.
  - **Rows meanwhile:** the previous list stays, and rows for deleted files are removed at once. The new list then replaces the old one **in a single frame**: no partial or empty state.
  - **Selection:** stays on the same file. If that file was deleted, it moves to the row now in the same position, or to the last row.
  - **`⏎` (show in tree) and `d` stay enabled**, since every row shown still exists.
- **File ownership for this part:** you also own `src/app/event_loop.rs` (the one call), `src/ui/largest_view.rs` and one constant in `src/ui/copy.rs`. engineer-2 is on T029 (`sanitize.rs` and tests) and won't touch these meanwhile.
- **Tests (red first):**
  - on a synthetic 1 M-file model, every `advance_top_rebuild(5 ms)` call returns within ~2× budget, and the final list equals the one-shot rebuild;
  - while pending, `top_files()` holds no removed node;
  - **Check 22 (UX), with a gated recompute and a fake clock:**
    - no `updating…` at 100 ms;
    - present at 250 ms and fitting 80 columns;
    - the rows equal the old list minus the deleted files;
    - `⏎` works meanwhile;
    - the text is gone after completion;
    - a recompute that finishes under 200 ms never shows it.
  - **Selection rule:** the selected file survives → it stays selected; it was deleted → the same position, or the last row.
