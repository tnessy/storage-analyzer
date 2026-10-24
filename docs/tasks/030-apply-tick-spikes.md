# T030: No apply step over ~5 ms: diagnose tick spikes, split large listings

- **Engineer:** engineer-1. **Depends on:** T009 (merged). **Schedule:** engineer-1's **next** task.
- **Status:** v1 (team lead, per the architect's T009 assessment; design §4 `Listing` contract, §5 "Max work per apply step").

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
