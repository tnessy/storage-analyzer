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

## Part 4: top-N with a reserve, plus a resumable rebuild (user-approved; architect's API in design §3.4 "Top-N maintenance")
**Sequence:** after Parts 1–3. Tests first.

**File ownership:**
- `src/model/index.rs` and `src/model/mod.rs`;
- `src/model/delete.rs` and `src/model/control.rs` (the call sites);
- `src/app/event_loop.rs` (one call, plus the input-wait cap);
- `src/ui/largest_view.rs`;
- the `LARGEST_UPDATING` constant in `src/ui/copy.rs`.

### Model (design §3.4)
- **Reserve.** Each size mode keeps the exact top **N + R** files, with **R = 4N** (500 for N = 100). They are ordered by (size, NodeId) so entries can be removed, and updated on insert, hard-link re-charge and rescan. Removing files keeps the list exact for the remaining entries, because everything outside it is no larger than its smallest entry. `top_files(mode)` returns the first N.
- **A delete that removes no list entry costs O(removed) and never rebuilds.**
- **Rebuild only when a removal leaves fewer than N entries**:
  - `Model::top_rebuild_pending() -> bool`;
  - `Model::step_top_rebuild(budget: Duration)`: scans node ids from a cursor into a scratch top-(N+R) set, stops when the budget is spent, and at the end of the cursor swaps the scratch set in as the live lists.
- **During a rebuild:**
  - files applied with an id **below** the cursor are also offered to the scratch set;
  - another delete or a rescan removes nodes from **both** the live list and the scratch set;
  - `begin_rescan(root)` clears both and **cancels** the rebuild.
- **Ext/category totals are unaffected:** they are subtracted per removed node and never show `updating…`.
- **Replaces** the one-shot `recompute_top` at its delete and rescan call sites. Keep `recompute_top` only as a test reference, or remove it.

### Event loop
Call `step_top_rebuild(5 ms)` once per iteration **after the message drain** while pending. While pending, the loop never blocks on input for more than 10 ms, so the rebuild keeps stepping while the user is idle.

### Largest view: **ruling (Heimdall): UX owns the surface, so UX §5.5 / check 22 applies**
The architect's footer and `Updating…`-as-empty-state variant is **not** used. The two fit together: the "live" list during a rebuild **is** the old list minus deleted files, exact and in order, possibly fewer than N.
- **Placement:** `updating…` (ASCII `updating...`), right-aligned on the Largest view's **column-header row** with a 1-cell right margin, in the header style, with no colour or blinking. Copy constant `LARGEST_UPDATING`.
- **Shown only if `top_rebuild_pending()` is still true 200 ms after the rebuild started;** hidden as soon as the new list is swapped in.
- **Rows:** the live entries (the old list minus deleted files, in order). The swap to the rebuilt list happens in a **single frame**.
- **Edge case, every live entry deleted while pending:** show no rows, and **do not** show `No files found.` while a rebuild is pending; the header text appears after 200 ms as usual. Flagged to UX for confirmation (non-blocking).
- **Selection:** stays on the same file. If that file was deleted, it moves to the row now in the same position, or to the last row.
- **`⏎` and `d` stay enabled.**

### Acceptance (architect + UX check 22)
1. **Harness:** no top-N step over 5 ms at 20 M nodes, and deleting a top-level folder on `$HOME` keeps tick max under 50 ms (reported).
2. A delete that removes no list member triggers **no** rebuild (count rebuild steps through a test hook).
3. After completion, **both size modes equal a from-scratch recompute.** Property test: random trees and deletes with out-of-order applies, a second delete mid-rebuild, and a root rescan mid-rebuild.
4. **During a rebuild,** every displayed entry is in the true top N, in the correct order.
5. **On `$HOME` (20 M),** the rebuild finishes in about 1.5 s of wall time or less, and the loop keeps stepping while idle (reported).
6. **After a rebuild the reserve is back to N + R;** a second large delete right after doesn't rebuild unless it removes more than R list entries.
7. **Check 22, with a fake clock and a gated rebuild:**
   - no `updating…` at 100 ms;
   - present at 250 ms and fitting 80 columns;
   - rows equal the old list minus the deleted files;
   - `⏎` works meanwhile;
   - the text is gone after completion;
   - a rebuild under 200 ms never shows it.
8. **Selection rule:** a survivor stays selected; a deleted selection moves to the same position, or to the last row.
