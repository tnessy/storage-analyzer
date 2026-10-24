# T030 review (Heimdall, 2026-10-23): Parts 1–3 APPROVED but **not committed** (see incident); Part 4 needs round 2 (reserve); app/UI wiring → T030b (engineer-2)

**Fresh evidence** (clean worktree: HEAD plus every T030 file):
- fmt ok, whole-tree clippy clean;
- the whole suite passes twice with `--no-fail-fast` (lib 115, model_indexes 21, scan_walker 30, scan_du_parity 9, …);
- doc tests ok, release build ok;
- `app_controls` 100/100; the r1-memory spike passes 6.

**Results** (engineer-1's harness, `$HOME` 19.87 M entries):
- slow ticks ≥ 20 ms: 5–7 → 0 in most runs;
- **tick max 118–147 ms → 9.2 ms** (11.4 at 16 threads);
- apply 2.6–3.1 M entries/s; RSS 4.25 → 4.08 GiB.

The diagnosis (`--diag-ticks`) showed that every slow tick was one ordinary message causing 20 k–100 k minor page faults, from per-node array doubling and a hard-link map rehash. Listing size was not a factor on this data; split listings are kept anyway, as the architect asked.

**Rulings:**
- **Hard-link table sharded into 256 maps** (`src/model/hardlink.rs`, not in the file list): **accepted.** The diagnosis pointed there, it removed the last big stall, and the file is engineer-1's.
- **Part 4 app/UI wiring** goes to **engineer-2 as T030b**, after T029. Those are engineer-2's files, and engineer-1 didn't do the wiring.

**Intermittent btrfs parity failure** (opt-in `SA_TEST_BTRFS_DIR`; the default suite is unaffected):
- `a_folder_of_ten_thousand_files_arrives_in_parts_and_still_equals_du` failed about 1 in 60 runs: **the model is +4,096 bytes (one block) against `du`.**
- With a `sync -f` on the fixture before the scan (tested in my scratch worktree only): **0 in 80.**
- **Cause:** btrfs delayed allocation settles `st_blocks` after write-back, so the scan and `du` saw different states. **A fixture-timing problem, not a counting bug.**
- **Fix in round 2 (R2-1).**

## Part 4 round 2 (engineer-1)
The built API (`start_top_rebuild` / `step_top_rebuild` / `top_updating` / `top_epoch` / `set_incremental_top`, off by default) restarts a **full** rebuild after **every** delete and rescan. Design §3.4 "Top-N maintenance" requires a **reserve**:
- the exact top **N + R** per mode (R = 4N), so a delete that removes no list entry costs O(removed) and **never** rebuilds;
- a rebuild only when fewer than N entries remain.

Without that, every large delete would show `updating…` for about 1.5 s. Architect acceptance criteria 2 and 6 are unmet.

- **R2-1, test fix:** in every btrfs fixture (`tests/scan_du_parity.rs`, and any `tests/support/fixtures.rs` path used with `SA_TEST_BTRFS_DIR`), sync before scanning: `sync -f <dir>`, or `libc::syncfs` if you prefer. **Prove it:** 200 runs with `SA_TEST_BTRFS_DIR`, 0 failures.
- **R2-2, reserve:**
  - keep top N + R in `Indexes`, updated on insert, re-charge and rescan;
  - `top_files` returns the first N;
  - removals keep the list exact;
  - `refresh_top` starts a rebuild **only** when a mode has fewer than N left.
- **R2-3, names:** design §3.4 says `top_rebuild_pending()`, and you built `top_updating()`. Keep one name and align the design through the architect; `top_updating` is fine if the architect agrees. `top_epoch()` is accepted as an addition.
- **R2-4, tests (red first):** the architect's criteria **2** (a delete that removes no list member → 0 rebuild steps) and **6** (after a rebuild the reserve is back to N + R; a second large delete doesn't rebuild unless it removes more than R entries). For **3**, extend the property test with a second delete mid-rebuild and a root rescan mid-rebuild, if it doesn't already cover both (tell me which test does).
- **R2-5:** once the reserve exists, decide with Heimdall whether `set_incremental_top` stays as a switch, or whether incremental becomes the only behaviour. Preferred: the only behaviour, with no dead switch, once T030b wires the loop.

**Architect review requested** (via the team lead) of the T030 model changes (chunked storage, hard-link shards, split listings, and the rebuild once R2-2 lands) against design §3.4/§4/§5.

## Integration incident (Heimdall)
- **What happened:** I committed T030 (`8c6004d`) by staging the **current** working files after my verification. By then, engineer-1 had already started round 2 (`tests/top_reserve.rs` plus changes in `index.rs`/`mod.rs`), so the commit contained unverified work, and **HEAD failed 2 tests** (`top_reserve::the_lists_always_match_a_recount`, `scan_harness` smoke).
- **Fix:** I moved HEAD back with `git reset --soft HEAD~2` and then unstaged with `git reset`. Neither touches working files, and I confirmed engineer-1's files were byte-identical before and after (md5). The docs were re-committed separately.
- **T030 code will be committed together with round 2,** after a fresh verification of a stable tree.
- **Process change:** from now on I stage **only the hash-checked snapshot** I verified (as for T021), never "whatever is modified now".

## Architect review of T030 (2026-10-23)
**Parts 1–3: approved.**
- The diagnosed cause is confirmed.
- The listing-part rules are correct: every part adds sizes, counts and child pending work; only `last` calls `finish_dir`; parts after `last` are dropped.
- The sharded hard-link table keeps "the first-applied link is charged", so du parity holds.

**Part 4 is correct as far as it goes, but fails criteria 2, 3, 5 and 6.** These are added to round 2, and design §3.4 is updated:
- **R2-2 (refined), reserve with a bound:**
  - TopN keeps N + R (R = 4N);
  - `remove_subtree` takes removed nodes out of the heaps;
  - a rebuild runs only when `exact_len() < N`;
  - the **bound rule:** track the largest evicted key; only entries above it are exact; a finished rebuild takes the scratch list's bound.
- **R2-4 (refined):** a **property test for criterion 3** (random trees and deletes with out-of-order applies, a second delete mid-rebuild, a root rescan mid-rebuild → both modes equal a from-scratch recompute).
- **R2-6, criterion 5:** measure the rebuild's wall time on `$HOME` (20 M) with the harness, and report it (target ≈ 1.5 s or less).
- **R2-7, root rescan:** `begin_rescan(root)` must **not** walk all ids. Clear the lists, set the bound to the minimum, and cancel the rebuild.

**Commit plan:** Parts 1–3 can't be split cleanly from the working tree any more, because round 2 is interleaved in `index.rs`/`mod.rs` and I no longer have the verified snapshot. **T030 is committed as one change after round 2** passes verification. That also keeps the architect's rule that users never get a rebuild on every delete. **T030b (engineer-2) lands only after round 2** (already its stated dependency), so the wiring and the reserve reach users together.
