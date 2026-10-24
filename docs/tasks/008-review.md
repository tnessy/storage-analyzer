# T008 (spike R1) review (Heimdall, 2026-10-23): APPROVED, committed
- **Fresh:** the spike crate passes fmt, clippy and its 6 tests. `home-histogram.txt` holds **only counts per name length, no names** (as the brief allowed). The main crate is unaffected.
- **Result (a target, not a gate):** **152.8 B/entry at 20 M entries (2.85 GiB RSS)**, against the 120 B target, so about 27 % over.

  | Entries | B/entry | Apply rate |
  |---|---|---|
  | 1 M | 155.4 | 4.1 M/s |
  | 5 M | 157.4 | 4.1 M/s |
  | 10 M | 152.8 | 3.8 M/s |
  | 20 M | 152.8 | 3.4 M/s |

  Method: one process per size, swap added to RSS.
- **Breakdown:** per-node arrays 127 B, names 19 B, child refs 4 B, hard-link map ~0.4 B.
- **Estimated savings (not implemented):**
  - dir-only fields moved to a dir table: ≈ 54 B, which alone gives ~99 B, so the target is reachable;
  - `dev_idx: u16`: 6 B;
  - a 12-byte `NameRef`: 4 B;
  - all three together: ~89 B.
- **Observation:** apply is O(depth) per listing. A pathological 464-deep chain was slow (5 M entries in ~80 s, against 1.2 s at a realistic depth). Real trees are 10–40 deep.
- **Decision needed (not engineering's):** whether to act on the 120 B target. That is plan O3 (user via project-owner). The architect is asked whether the dir-table split should become a task. On the user's 60 GiB machine, 2.85 GiB at 20 M entries is workable today.
