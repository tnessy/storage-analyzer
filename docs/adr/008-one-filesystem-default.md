# 008: Stay on one filesystem by default
- Status: accepted (user confirmed 2026-10-23)
- Context: Should-item "stay on one filesystem". Scanning `/` without it walks /proc, /sys and network mounts. On btrfs each subvolume has its own st_dev (fact: / = 36, /home = 59), so nested subvolumes (e.g. snapshots) look like separate filesystems. M8 requires parity with du "in the same mode".
- Options:
  - Default off (du/ncdu behaviour), with `-x` to enable.
  - **Default on, with `--cross-filesystems` to disable.**
- Decision: Default on. It is safer and faster, and it avoids double-counting btrfs snapshots that share extents. The UI marks skipped mount points as OTHER_FS and lists them in the Skipped view with reason `other filesystem`, so it is visible what was left out. Parity tests compare against `du -sx`.
- Consequences: On btrfs, unmounted nested subvolumes inside the root are excluded by default. This is surprising if the user expects them, so the status bar shows "N other-fs skipped".
- Blast radius / reversibility: One flag default. Trivial to flip.
- Revisit if: The user expects snapshots or subvolumes included by default.

## Amendment (2026-11-07): cross local filesystems by default

- Default now enters other local filesystems (`/home`, `/tmp`, `/boot`, btrfs subvolumes incl. `.snapshots`) and skips pseudo (proc, sysfs, devtmpfs, cgroup, ...), `fuse.*` and network (nfs, cifs, 9p, ...) mounts, found via `/proc/self/mountinfo` (`scan/mounts.rs`). Skipped ones still show as `(other filesystem)`.
- `-x` / `--one-file-system` restores the old behaviour; `--cross-filesystems` enters everything.
- Known cost: btrfs snapshots sharing extents are counted per snapshot, so totals can exceed real usage. `du -sx` parity applies to `-x` only.
