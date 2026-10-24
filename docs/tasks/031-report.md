# T031 report (engineer-1)

**Status:** DONE (diagnosis; no fix needed). Details and tables: `spikes/R31-REPORT.md`.

- Files: `examples/scan_harness.rs` (`--malloc-report`: `malloc_stats`, `mallinfo2`, `malloc_trim(0)`, RSS before and after, and a census of the model), `examples/scan_harness_mimalloc.rs` (same harness, mimalloc as global allocator).
- Result: RSS is 4.0-4.1 GiB for 1, 4 and 8 threads, with glibc and with mimalloc. glibc reports 98 % of its heap in use, and `malloc_trim(0)` frees 1 %. So it is live data, not fragmentation. The old 2.4 GiB single-thread figure came from a run on a swapping machine (RSS excludes swapped pages); I could not reproduce it.
- What the memory holds on `$HOME`: 19.9 M nodes at about 210 B each: 127 B of per-node arrays, names averaging 52 B (the R1 spike assumed 19 B), and 8.45 M duplicate hard links (42 % of nodes, about 470 MiB in the hard-link table).
- Recommendation: no allocator change (nothing for the architect or the owner to approve); use about 4.0 GiB as the real figure for a 20 M-entry tree; shrinking options are listed in the report.
- Test: `the_malloc_report_is_filled_in_when_asked_for` (harness smoke test).
- Also: T030c item R2-1 turned out to be half done in the committed tree: `sync_fs` was in the 10,000-file test but the call in `m8_tree` had been lost when my edit silently did not match after `rustfmt`; it is in the working tree now (`tests/support/fixtures.rs`), 200 runs of `scan_du_parity` with `SA_TEST_BTRFS_DIR`: 0 failures.
