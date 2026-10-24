# T036b: Disk read speed: the `diskio` source (helper thread, `/sys` block-device counters)

- **Engineer:** engineer-1. **Schedule:** now. **Depends on:** T036a's contract `diskio::ReadBytesSource` (engineer-2 is creating it; code against the trait as written below).
- **Design:** design §3.12 rev 11 and ADR-011 (architect, **revised**; this supersedes the earlier no-thread version and Heimdall's interim rulings on read errors and union-at-start).

## Source
- **Block-device read counters:** `/sys/dev/block/<maj:min>/stat`, field 3 (sectors read × 512 B). Device-wide, so the label is `disk` (UX has updated the spec).
- **Not** `/proc/self/io`.

## Resolving the devices (by path)
1. Take the longest `/proc/self/mountinfo` mount point that is a prefix of the path, decoding octal escapes. Unmounted nested subvolumes resolve to the parent filesystem.
2. If the mount source isn't a block device (tmpfs, overlay, FUSE, NFS, …) → unavailable.
3. **btrfs:** all members listed in `/sys/fs/btrfs/<uuid>/devices/`.
4. Use the device the filesystem sits on (partition, dm, LUKS mapper). **Never** the whole disk and **never** the devices under dm.
5. **`--cross-filesystems`:** a **union that follows the scan.** The main loop sends newly crossed mount points over a channel (T036c wires that), and the thread resolves them and adds each new device **once**.

## Thread `diskio` (started after the first frame; T036c starts it)
- **Each second:** read and sum `sectors × 512` across the current device set, and store into **atomics**:
  - the running total (`AtomicU64`);
  - an availability flag;
  - a **device-set generation** (`AtomicU64`, bumped whenever a device is added or the set changes).
- **Why a thread:** resolving can rarely block (it reads mountinfo and stats the source device).
- **Unavailable** (nothing resolved, or a read error) → the flag is false.
- **`DeviceStatSource` implements `ReadBytesSource`** by only **loading** those atomics, so it never blocks:
  - `read_bytes()` → `Some(total)` while available, else `None`;
  - `generation()` → the current set generation (the contract method T036a adds).
- **Lifecycle** (design §3.8 / §4 rules apply):
  - name the thread exactly `diskio`. Its panics are recorded but never stop the scan (only `scan-*` threads do; T015 rule);
  - a stop flag, with every sleep at most 100 ms so it notices quickly;
  - `Drop` / `stop()` waits at most a bounded time (≤ 200 ms), then detaches. **Exit must never block.**
- **Read-only:** only `/proc` and `/sys`, via `File::open`. `tests/write_guard.rs` stays green.

## Interface (new files, all engineer-1's)
- `src/diskio/sys.rs`: the `SysView` trait (read a file, read a link, list a directory, stat a device), a real implementation, and a test fake rooted at a temp dir.
- `src/diskio/resolve.rs`: `resolve(path, &dyn SysView) -> Option<DeviceSet>`.
- `src/diskio/thread.rs`: `DiskioHandle::start(root, cross, Box<dyn SysView>) -> (DiskioHandle, DeviceStatSource)`, plus `DiskioHandle::add_mount(path)` and `stop()`.
- **Not** `src/diskio/mod.rs` or `meter.rs` (T036a). The `mod` lines go in at integration through Heimdall.

## Tests (red first)
**With the fake `/proc` + `/sys` tree:**
- ext4 on a partition → the partition, not the whole disk;
- btrfs with one device and with several → summed, each counted once;
- LUKS via `/dev/mapper` → the mapper's count only;
- tmpfs and NFS → `None`;
- a nested unmounted subvolume → the parent filesystem;
- an escaped mount point → decoded;
- the `--cross-filesystems` union, via `add_mount` → the generation bumps once per new device, and a mount already in the set doesn't bump it;
- a read error → unavailable.

**Thread:**
- the total updates about once per second (fake clock or short real waits);
- `read_bytes()` never blocks while the thread is busy (hold a gate in a fake `SysView`; the call still returns at once);
- `stop()` returns within 250 ms even when the `SysView` blocks;
- a panic inside the thread doesn't stop a scan (recorded as thread `diskio`).

**Real machine:** skipped if `/sys/dev/block` is missing. Resolving `$HOME` (btrfs on `nvme1n1p2`) gives a non-empty set.
