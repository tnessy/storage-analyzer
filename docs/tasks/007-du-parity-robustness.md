# T007: M8 du-parity and M10 robustness integration suite

- **Engineer:** engineer-1. **Wave:** 1. **Depends on:** T003, T006.

## Goal
Prove brief M8 (totals match `du` in the same mode, hard links counted once, symlinks not followed) and M10 (no crash, issues reported) end-to-end: `RealFs` → walker → `Model`.

Acceptance criteria:
1. A fixture builder in `tests/support/fixtures.rs` creates the trees from design §4 Testing in a temp dir:
   - hard links inside one subdir and across subdirs;
   - symlinks: dangling, loop, and pointing outside the root;
   - a sparse file;
   - a `chmod 000` dir;
   - names with invalid UTF-8 and with a newline;
   - 200-deep nesting;
   - empty dirs and zero-byte files.
2. A helper runs scan + apply to `Done` (threads = 4, also threads = 1).
3. Root totals equal:
   - `du -s -B1` (disk mode);
   - `du -s -B1 --apparent-size` (apparent mode);
   - `du -sx -B1` (default one-filesystem mode).

   Run `du` with `LC_ALL=C`. For the `chmod 000` dir, compare against du's total while ignoring du's exit status.
4. The suite runs on the tmpfs temp dir (default `TMPDIR`), **and** on btrfs when `SA_TEST_BTRFS_DIR` is set to a writable dir (for example `~/tmp/sa-tests`). Otherwise the btrfs case is skipped with a printed message.
5. **M10 race:** a racing thread deletes and recreates files in a 50 k-entry fixture while it is scanned. The scan reaches `Done`, never panics, and the vanished entries appear as `Vanished` issues. The test may write only inside its own temp dir.
6. **M9:** after a scan, every fixture file's atime and mtime and the tree hash (paths, sizes, mtimes) are unchanged.
7. Hard links counted once: `du`-equal totals with links across subdirs, and exactly one `HARDLINK_DUP` per extra link.

## Design references
§1 (du 9.12 facts), §3.4, §3.5, §4 Testing (integration and robustness), §6 M8/M10. ADR-008.

## Files owned
`tests/support/fixtures.rs`, `tests/scan_du_parity.rs`, `tests/scan_robustness.rs`.

## Tests to write first
They are the deliverable: write them, see them fail (for example, temporarily point the helper at an empty model, or check that the assertions fail on a fixture where you know the answer is off), then make sure they pass against T003 + T006. If a parity test fails because of model or walker code, report it to Heimdall with the diff of totals. **Do not patch T003/T006 files in this task**; a fix goes in as a follow-up task.

Commands: `cargo test --test scan_du_parity --test scan_robustness`, and `SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests cargo test --test scan_du_parity`.

## Out of scope
Delete parity (T011). UI.

## Note (2026-10-23, re-order)
T007 now runs after T012 and T011. `tests/support/fixtures.rs` may already exist from T011: extend it, don't replace it.
