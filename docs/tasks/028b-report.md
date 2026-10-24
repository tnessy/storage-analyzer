# T028b report (engineer-1)

**Status:** DONE. Files: `src/scan/walker.rs`, `tests/scan_walker.rs`. No app code touched.

- `ScanHandle::rescan_many(&[(NodeId, PathBuf, u32)])`: registers all rescan roots and pushes all jobs (each through `Queue::push_resume`) while holding the `done_sent` lock once, so a `Done` cannot appear between queued rescans. `rescan` is now `rescan_many` of one target. An empty list does nothing.
- `check_complete` reads the cancelled flag inside the same locked block that decides completeness (a rescan clears both under that lock).
- Tests (red first against a `todo!` stub): `rescan_many_gives_one_done_after_all_the_rescans` (50 rounds, 4 threads: exactly one `Done`, three relistings before it, nothing after) and `rescan_many_clears_a_stop_like_rescan_does`. My first version of the second one used a tree that could finish before the stop and stalled now and then; it uses the big tree of the other stop tests now. The `check_complete` change has no test of its own (the interleaving cannot be forced from outside).
- Results: scan_walker 27 pass over 6 runs; lib, app_controls, model_controls, delete_model pass; fmt and clippy (lib + scan_walker + app_controls) clean.
