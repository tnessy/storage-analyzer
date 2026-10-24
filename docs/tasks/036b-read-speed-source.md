# T036b: Disk read speed: the real source (PENDING the architect's design)

- **Engineer:** engineer-1. **Depends on:** T036a's contract (`diskio::ReadBytesSource`, in that task's brief).
- **Status:** **do not start until Heimdall adds the architect's design here** (choice of `/proc/self/io` vs `/proc/diskstats` for the scan root's block device, mapping st_dev to the device, btrfs and multi-device behaviour, permissions).

## Known constraints (team lead / user)
- **Reads `/proc` and `/sys` only. Read-only;** it must not trip `tests/write_guard.rs`.
- **Cheap and non-blocking:** `read_bytes()` is called on the main loop at most once per second. Keep a file open, or re-read a small file; no allocation churn. If a read could ever block, sample on a helper thread instead and say so.
- **Unavailable → `None`**, never a fake 0.
- **Tests with fake `/proc` contents** (an injectable root path or reader).
- **Wire it in** `app/mod.rs::production_starter` in place of `NoSource` (one line, coordinated with Heimdall, since engineer-2 owns the file).
