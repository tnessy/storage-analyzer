# T007 report (engineer-1)

**Status:** DONE

Files: `tests/scan_du_parity.rs`, `tests/scan_robustness.rs`, `tests/support/fixtures.rs` (extended: `du_with`, `M8Tree`/`m8_tree`, `locations`, `scan_to_done`, `Scan::start_with`, `write_empty`). No source files changed.

## What the tests prove
- **M8 parity** on the design §4 tree (plain/zero-byte files, empty dirs, hard links in one dir and across dirs, dangling/loop/outside/inside symlinks, a 20 MiB sparse file, invalid-UTF-8 and newline names, 200-deep nesting, a `chmod 000` dir): root disk and apparent totals equal `LC_ALL=C du -s -B1`, `--apparent-size`, and both with `-x`, for 1 and 4 threads, on the default temp dir (tmpfs) and on btrfs (`SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests`). Without the variable the btrfs case prints `SKIP: SA_TEST_BTRFS_DIR is not set`.
- Hard links: exactly one `HARDLINK_DUP` per extra link (4), per inode all links but one are duplicates, duplicates have size 0.
- Symlinks are leaves without children; the file outside the root is never scanned.
- Awkward names and the 200-deep file are found with the right sizes, depth (202) and byte-exact paths.
- M10: the `chmod 000` dir gives exactly one issue (`Open`, `PermissionDenied`, path `locked`) and the scan completes. A deterministic race (`RacyFs` removes files right after the directory listing) yields exactly the expected `Vanished` stat issues. A thread deleting and recreating files in a 50 k-entry tree while it is scanned (4 threads): the scan reaches `Done`, the thread does not panic, and every issue is `Vanished` (about 170-200 observed per run).
- M9: atime, mtime, size and mode of every entry are unchanged after a scan (tmpfs and btrfs).

## Failing first
The tests were written against the already merged T003/T006, so they pass; I checked they can fail by breaking the model on purpose (not zeroing the size of duplicate hard links): `totals_equal_du_...` and `hard_links_are_counted_once_...` failed, then I restored the file.

## Findings (the T011 note)
- On this btrfs a directory has a `st_size` (56 for a small one; 120 on tmpfs), but `du --apparent-size` leaves directories out in both cases (a 5000-byte file in two nested dirs: 5000). The model's "directory apparent = 0" rule matches du on both. `directories_have_no_apparent_size_like_du` checks that apparent bytes of a folder equal the sum of its files.
- No parity differences were found, so nothing to report to Heimdall. 

## Fresh results
fmt ok; clippy (lib + my test targets) -D warnings clean; `scan_du_parity` (8), `scan_robustness` (2), `delete_model` (23), `model_controls` (30) pass; 15 repeated runs of the three real-filesystem suites with btrfs enabled: 0 failures. The full `--all-targets` run was not clean earlier because of another engineer's unfinished test files.
