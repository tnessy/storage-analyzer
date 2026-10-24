# T027 report (engineer-1)

**Status:** DONE

Files: `src/model/control.rs`, `src/model/mod.rs`, `src/scan/walker.rs`, `tests/model_controls.rs`, `tests/scan_walker.rs`, one arm in `src/app/controls.rs` (`RescanPlan::OtherFilesystem => {}` with `// T020b: message pending UX`).

## Failing first
- `model_controls`: 4 new tests failed (`rescanning_a_top_level_dir_whose_entry_had_an_issue_keeps_progress_consistent`, `another_filesystem_is_never_rescanned` (`Full` instead of `OtherFilesystem`), `a_rescan_root_found_on_another_device_becomes_an_other_filesystem_node`, and the property test with "attempt to subtract with overflow" at `control.rs`).
- `scan_walker`: `a_rescan_root_now_on_another_device_is_reported_and_not_listed` failed (3 messages: Identity, Listing, Done).
- Passed straight away (pin behaviour): `a_counted_top_level_dir_that_turns_into_a_leaf_leaves_the_total`, and the walker test for crossing allowed.

## Fix
- Bug 1: a new flag `Flags::TOP_COUNTED` marks the directories counted in `top_total`; set when the root's listing descends a directory. All `top_done`/`top_total` adjustments (`adjust_top_done`, `remove_subtree`, `apply_identity`) now go by this flag instead of guessing. `begin_rescan` on a top-level directory that was never counted starts counting it (as unfinished); a rescan root that turns into a leaf stops counting.
- Bug 2: `RescanPlan::OtherFilesystem` (first check in `rescan_plan`). `begin_rescan` on an `OTHER_FS` node is a no-op. The walker's rescan identity step reports `Identity` with `(Stat, OtherFilesystem)` and does not list when `one_filesystem` and the target's device differs from the root's. The model turns such a rescan root into an `OTHER_FS` node (size, directory count, pending work and progress adjusted; issue recorded), so totals equal those of a scan that met it as another filesystem from the start (tested).
- Property test (`progress_property`, proptest, random listings with entry issues, `DirIssue`, `Cancelled`, rescans and leaf identities): `top_level_progress()` equals a recount through the nodes the test knows were descended, never goes negative, and `done <= total`; also run with 2000 cases x 3.

## Fresh results
fmt ok; clippy -D warnings: only errors in other people's unfinished files; `cargo test` for lib + model_controls (30) + model_apply + model_indexes + delete_model + scan_walker (24) + delete_executor + write_guard + no_name_literal: all pass. The full `--all-targets` run does not compile right now because `tests/app_eta.rs` (not mine) misses the new `Services::delete` field.
