# T036b: Disk read speed: the `diskio` source (helper thread, `/sys` block-device counters)

- **Engineer:** engineer-1. **Schedule:** now. **Depends on:** T036a's contract `diskio::ReadBytesSource` (engineer-2 is creating it; code against the trait as written below).
- **Design:** design §3.12 **rev 12** and ADR-011 (authoritative). Rev 12 keeps the helper thread and atomics, and adopts the lead rulings: union resolved once, read error → unavailable for good, no generation counter.

## Source
- **Block-device read counters:** `/sys/dev/block/<maj:min>/stat`, field 3 (sectors read × 512 B). Device-wide, so the label is `disk` (UX has updated the spec).
- **Not** `/proc/self/io`.

## Resolving the devices (by path)
1. Take the longest `/proc/self/mountinfo` mount point that is a prefix of the path, decoding octal escapes. Unmounted nested subvolumes resolve to the parent filesystem.
2. If the mount source isn't a block device (tmpfs, overlay, FUSE, NFS, …) → unavailable.
3. **btrfs:** all members listed in `/sys/fs/btrfs/<uuid>/devices/`.
4. Use the device the filesystem sits on (partition, dm, LUKS mapper). **Never** the whole disk and **never** the devices under dm.
5. **The device set is resolved ONCE,** in the thread after it starts: everything mounted at or under the scan root (always including the root's own filesystem), each device counted once. That holds in both modes. There is **no** channel, **no** `add_mount`, and **no** re-resolution.

## Thread `diskio` (started after the first frame; T036c starts it)
- **Each second:** read and sum `sectors × 512` across the device set, and store into **atomics**:
  - the running total (`AtomicU64`);
  - an availability flag.
- **Why a thread:** resolving can rarely block (it reads mountinfo and stats the source device).
- **Unavailable** (nothing resolved, or **any read error, which is then permanent for the rest of the scan**) → the flag is false.
- **`DeviceStatSource` implements `ReadBytesSource`** by only **loading** those atomics, so it never blocks:
  - `read_bytes()` → `Some(total)` while available, else `None`.
- **Lifecycle** (design §3.8 / §4 rules apply):
  - name the thread exactly `diskio`. Its panics are recorded but never stop the scan (only `scan-*` threads do; T015 rule);
  - a stop flag, with every sleep at most 100 ms so it notices quickly;
  - `Drop` / `stop()` waits at most a bounded time (≤ 200 ms), then detaches. **Exit must never block.**
- **Read-only:** only `/proc` and `/sys`, via `File::open`. `tests/write_guard.rs` stays green.

## Interface (new files, all engineer-1's)
- `src/diskio/sys.rs`: the `SysView` trait (read a file, read a link, list a directory, stat a device), a real implementation, and a test fake rooted at a temp dir.
- `src/diskio/resolve.rs`: `resolve(root, &dyn SysView) -> Option<DeviceSet>`: the union of the root's filesystem and everything mounted at or under it.
- `src/diskio/thread.rs`: `DiskioHandle::start(root, Box<dyn SysView>) -> (DiskioHandle, DeviceStatSource)` and `stop()`.
- **Not** `src/diskio/mod.rs` or `meter.rs` (T036a). The `mod` lines go in at integration through Heimdall.

## Tests (red first)
**With the fake `/proc` + `/sys` tree:**
- ext4 on a partition → the partition, not the whole disk;
- btrfs with one device and with several → summed, each counted once;
- LUKS via `/dev/mapper` → the mapper's count only;
- tmpfs and NFS → `None`;
- a nested unmounted subvolume → the parent filesystem;
- an escaped mount point → decoded;
- a **union** of the root filesystem plus a block device mounted under it → both counted, each once; a non-block mount under it is ignored;
- a read error → unavailable, and it stays unavailable.

**Thread:**
- the total updates about once per second (fake clock or short real waits);
- `read_bytes()` never blocks while the thread is busy (hold a gate in a fake `SysView`; the call still returns at once);
- `stop()` returns within 250 ms even when the `SysView` blocks;
- a panic inside the thread doesn't stop a scan (recorded as thread `diskio`).

**Real machine:** skipped if `/sys/dev/block` is missing. Resolving `$HOME` (btrfs on `nvme1n1p2`) gives a non-empty set.
