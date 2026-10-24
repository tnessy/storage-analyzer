# T011 report (engineer-1)

**Status:** DONE (needs the second reviewer, with T010)

Files: `src/model/delete.rs` (new), `src/model/mod.rs`, `tests/delete_model.rs`, `tests/support/fixtures.rs` (new; T007 extends). Also, from the lead's D12 note: the issue list now keeps insertion order when issues are removed (`src/model/control.rs`, tests in `tests/model_controls.rs`).

## Failing first
- D12: `removing_issues_keeps_the_list_in_insertion_order` failed with `[3, 2]` instead of `[2, 3]` (the old `swap_remove`), then fixed with one order-preserving `retain` plus one relink pass per removal.
- T011: all 22 `delete_model` tests failed against the `todo!("T011")` stubs. After implementing, 22/22 pass; 12 repeated runs stable once the hard-link test was made deterministic (see below).
- Mutations that make tests fail: counting a delete's `Cancelled` as a skip (tombstone test fails); not clearing the tombstone on `Aborted` (2 tests fail).

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` all pass (105 lib, 22 delete_model, 25 model_controls, ...); doc ok; release build ok.
`TARGET: delete_check_ms_per_1M=18.6 (target 50)` (release, 1 M nodes in 1000 directories, 2% hard links).

## Behaviour
- `delete_refusal`: root, `OTHER_FS`, node or ancestor `DELETING`/`TOMBSTONE`, a delete already running (`DeleteRunning(target)`). A node that no longer exists is reported as `AlreadyDeleting`.
- `delete_check`: counts are what is inside the node (not the node itself), lower bounds unless `exact`; `largest_known` = the 3 largest children in the given mode; `outside_links` counts distinct inodes in the subtree with fewer in-tree links than `nlink`; `freed_bytes_upper` = disk size minus the disk bytes charged inside for those inodes.
- `begin_delete` flags the target `DELETING|TOMBSTONE`; `total_files`/`total_dirs` are `Some` only when exact and include the target itself, to match the executor's counts.
- Under the tombstone: `DirIssue` and entry issues are discarded (the dir still counts as listed), `Cancelled` only decrements pending, listings are still applied.
- `apply_delete`: `Removed` -> `remove_subtree(target, false)` (T012) -> `None`; `Partial` -> clear tombstone, record the failures as issues on the nodes at their paths (target if unknown; busy/read-only map to `Other(errno)`), `Rescan(id)`; `Aborted` (any reason, including `MountPoint`) -> clear tombstone on the whole subtree, `Rescan(id)`. A `Finished` with no delete begun, and `Progress`, change nothing.
- `begin_rescan` after `Partial` removes those failure issues again with the rest of the subtree; the failure list for the user comes from the executor outcome (T021).

## Decisions / things to check
- In the aborted-unfinished real-fs test I make the identity check fail by raising `expect_ino` by one, rather than swapping directories (a swap would change the tree and `du`). Same code path (`ChangedSinceScan`), tree unchanged.
- The hard-link test deletes with parallel workers, so which directory holds the charged link is random; it repeats until a run deleted the charged link (up to 50), then also deletes the other directory.
- The real-fs early-delete tests drive the scan with `threads = 1` and a 4-message channel, and check `!is_complete(victim)` before `begin_delete`.
- `du --apparent-size` on this machine (tmpfs) ignores directory sizes, so apparent totals match the model's "directory apparent = 0". T007 should recheck this on ext4/btrfs.
- Carried over from T012: "a dir replaced by a file, rescanned, then a delete retry succeeds" is `a_folder_replaced_by_a_file_can_be_rescanned_and_then_deleted` (real executor, passes).
- `fixtures.rs` has: `write`, `du`, `root_entry`, `new_model`, `assert_matches_du`, `check_exact`, and `Scan` (a live real scan applied step by step, with `find` and the app-side `rescan`).

## Review fixes (2026-10-23)
- Flaky hard-link test: it now finds the folder that holds the charged link after the scan and deletes that one (then, in a second run, the other one); no retry loop. 40 runs of the whole release `delete_model` binary and 40 runs of the test alone: 0 failures. (A first attempt looped `cargo test --release`, which also failed 7 of 40 times, but only because the shared tree was mid-edit and did not compile; running the built binary directly showed the test itself is stable.)
- A target that left the model mid-delete (a parent rescan): `apply_delete` returns `Reconcile::None` for `Partial`, `Aborted` and `Removed`, and the delete slot is free again. Red first: `a_target_that_left_the_model_mid_delete_needs_no_rescan` failed with `Rescan(NodeId(1))`.
- Doc comment on `Model::issues()` (D12).
- Removed `record_failures`, `node_at` and the errno mapping: the scan issue list stays scan-only. The two tests that asserted failure issues now assert the list is empty (red first: both failed), the rest of their checks are unchanged.
- Not done (optional item): setting `OTHER_FS` on the node after `Aborted(MountPoint)`. The sizes and counts of an `OTHER_FS` directory are excluded from the aggregates when it is inserted, so flagging an already counted node later would make the totals wrong. The app can remember the refusal instead.

## Proof for the release loop (lead's request)
`for i in 1..40: target/release/deps/delete_model-<hash>` (built with `cargo test --release --test delete_model --no-run` on the current tree), summarised with `sort | uniq -c`:
```
     21 test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
     19 test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```
40 of 40 runs ok. Partial deletes add nothing to the scan issue list: `partial_clears_the_tombstone_and_asks_for_a_rescan` and `a_partial_delete_asks_for_a_rescan_that_matches_du` assert `issues().is_empty()`. The optional `OTHER_FS` marking after `Aborted(MountPoint)` is not done (backlog).
