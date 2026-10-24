# T036c: Disk read speed: wiring, label and help line

- **Engineer:** engineer-2. **Depends on:** T036a and T036b merged, plus **UX's label decision**.
- **Schedule:** after both merge.

1. **Production wiring (architect rev 11):** after the first frame, start the `diskio` thread with `DiskioHandle::start(root, cross, RealSysView)`, and pass its `DeviceStatSource` to the app instead of `NoSource`.
   - **Stop it in the exit sequence** (bounded; exit must never block).
   - **`--cross-filesystems`:** when the scan first meets a directory on a new device (a `RawEntry` whose `dev` differs from all devices seen so far, during apply), call `handle.add_mount(path)` once per new device.
2. **Label:** `disk` is already in T036a (UX decided). Nothing to change here unless UX revises it.
3. **Help footer** (UX §6, exact wording): two new lines, `Disk speed: reads on the disk(s) being scanned, from all programs, not just` / `this one. Hidden when unavailable (e.g. tmpfs, NFS, overlay, ZFS).`
   - The help is now **22 rows** at 24, and every line is exactly 80 columns.
   - **Check 19(c)** now expects **four footer notes** and must find `Disk speed:`. The key-entry count stays 19.
   - Regenerate the help snapshots.
4. **Rescans:** UX §5.2 rules (no field during a subtree rescan after Done), as in T036a.
5. **Tests:**
   - with a fake `SysView`, a full app run shows `disk …/s` after one second of fake-clock time and hides it at Done;
   - **`--cross-filesystems`:** reaching a second device calls `add_mount` once, and the meter shows no spike;
   - exit with the `diskio` thread blocked in a fake `SysView` returns within 2 s;
   - the help tests are updated.
6. **Runbook:** update the read-speed row and the manual check: on `$HOME` the field appears within about 1 s of a cold-ish scan; on `/tmp` (tmpfs) it never appears.

**Files:** `src/app/mod.rs`, `src/app/event_loop.rs`, `src/app/state.rs`, `src/ui/help.rs`, `src/ui/copy.rs`, `tests/app_loop.rs`, `tests/ui_accessibility.rs`, the help snapshots, `docs/acceptance/runbook.md`.
