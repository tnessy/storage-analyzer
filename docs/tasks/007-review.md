# T007 review (Heimdall, 2026-10-23): APPROVED, committed. This is the M8/M9/M10 evidence.

**Fresh evidence** (clean worktree: HEAD plus `tests/scan_du_parity.rs`, `tests/scan_robustness.rs` and `tests/support/fixtures.rs` only; no source files):
- fmt ok, whole-tree clippy clean;
- every suite passes (scan_du_parity 8, scan_robustness 2, delete_model 23 against the extended fixtures, …);
- **3 runs with `SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests` (btrfs, `/dev/nvme1n1p2`)**, plus the default temp dir (tmpfs): all pass;
- without the variable, the 3 btrfs cases print `SKIP`, so with it they really ran.

**Red first:** not possible, because it is a suite over merged code; the README rule applies.
- engineer-1's breakage (stop zeroing duplicate hard-link sizes) failed the parity and hard-link tests.
- **My sample breakage:** giving dirs their `st_size` as apparent bytes in `insert_entry` fails `totals_equal_du_in_disk_apparent_and_one_filesystem_modes` and `directories_have_no_apparent_size_like_du` on btrfs ✓.

**What this proves:**
- **M8:** walker + model totals equal `LC_ALL=C du -s -B1`, `--apparent-size`, and both with `-x`, for 1 and 4 threads, on tmpfs and btrfs. The fixtures follow design §4:
  - hard links (one `HARDLINK_DUP` per extra link);
  - dangling, looping, outside and inside symlinks (never followed);
  - a sparse file;
  - invalid-UTF-8 and newline names;
  - 200-deep nesting;
  - a `chmod 000` dir (exactly one `PermissionDenied`);
  - empty dirs and zero-byte files.
- **Btrfs apparent size:** dirs do have an `st_size` on btrfs, but `du --apparent-size` leaves them out there too. The model's "dir apparent = 0" matches du on both filesystems.
- **M9:** atime, mtime, size and mode are unchanged after a scan.
- **M10:** a deterministic vanish-after-listing wrapper gives exact `Vanished` issues. A threaded delete/recreate in a 50 k tree during the scan finishes without a panic, and every issue is `Vanished`.
