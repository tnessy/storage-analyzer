# T036b: Disk read speed: the real source (`/sys` block-device counters)

- **Engineer:** engineer-1. **Schedule:** now. **Depends on:** T036a's contract `diskio::ReadBytesSource` (engineer-2 is creating it; until it merges, code against the trait as written in `036a-read-speed-display.md`).
- **Design:** design §3.12 and ADR-011 (proposed; architect). The architect's message was truncated at the task split; Heimdall has asked for the rest. **This brief is the reconciled version.**

## Source (architect)
- **Block-device read counters only:** `/sys/dev/block/<maj:min>/stat`, field 3 (sectors read × 512 B).
- **Not** `/proc/self/io` (spike S-IO is backlog).
- The value is **device-wide** (it includes other programs). The label therefore says `disk`, pending UX's confirmation (T036c); see the brief M13 rule on labels.

## Resolving the devices (by path, not `st_dev`)
1. Take the `/proc/self/mountinfo` entry with the **longest mount point that is a prefix of the scan root**. Unmounted nested subvolumes resolve to the parent filesystem. **Decode mountinfo's octal escapes.**
2. Use the mount's source device. If it isn't a block device (tmpfs, overlay, FUSE, NFS) → **unavailable**.
3. **btrfs:** every member listed in `/sys/fs/btrfs/<uuid>/devices/`.
4. Use the device the filesystem sits on (partition, dm, LUKS mapper). **Never** the whole disk (other partitions would be counted) and **never** the devices under dm (double counting).

**`--cross-filesystems` (Heimdall ruling, a simplification to confirm with the architect):** resolve **once at start** as the union over every block-device mount whose mount point is at or under the scan root, plus the root's own, each device counted once. There is **no** dynamic re-resolution when the scan crosses a mount, so no scan→app signal is needed.

## Interface (reconciled; T036a's contract stays frozen)
- **New module files, all engineer-1's:** `src/diskio/sys.rs` (`SysView` trait: read a file, read a link, list a directory, stat a device; a real implementation plus a test fake rooted at a temp dir) and `src/diskio/resolve.rs` (`resolve(root, cross: bool, &dyn SysView) -> Option<DeviceSet>`).
- **`DiskSource` implements `ReadBytesSource`:** `read_bytes()` = the **sum of bytes read** over the `DeviceSet`.
  - **There is no rate computation here.** T036a's `ReadRateMeter` owns the one-second window, the at-most-once-per-second rule, and latching on reset/backwards. This replaces the architect's `DiskRate::{new, add, sample}`, which would duplicate the meter; the architect has been asked to confirm.
  - **Any read error on any device → `None`** from then on. The meter then hides the field for the rest of the scan. This is simpler and honest; the architect proposed dropping just that device, but a sum that suddenly shrinks would look like a reset anyway.
- **Cost:** `read_bytes()` is called at most once per second from the main loop. These are small in-memory `/sys` reads; no thread.

## Read-only
Only `/proc` and `/sys` are read. `tests/write_guard.rs` must stay green, so open files read-only (`File::open`).

## Tests (red first), using a fake `/proc` + `/sys` tree in a temp dir through `SysView`
- **ext4 on a partition** → that partition's count, not the whole disk's;
- **btrfs with one device, and with several** → summed, each counted once;
- **LUKS via `/dev/mapper`** → the mapper device's own count;
- **tmpfs and NFS** → `None` (unavailable);
- a **nested unmounted subvolume** → the parent filesystem's devices;
- an **escaped mount point** (`\040` etc.) → decoded and matched;
- **`--cross-filesystems` union** → each device counted once;
- **a read error** → `None`, and it stays `None`;
- **one real-machine test,** skipped when `/sys/dev/block` is missing. On this machine, scanning `$HOME` (btrfs on `nvme1n1p2`) resolves a non-empty device set.

## Wiring
Expose `DiskSource::resolve_for(root, cross) -> Box<dyn ReadBytesSource>`, returning `NoSource` when unresolved. The one-line change in `app/mod.rs::production_starter` belongs to **T036c** (engineer-2).

## Files owned
`src/diskio/sys.rs`, `src/diskio/resolve.rs`, `src/diskio/source.rs` (new), `tests/diskio.rs` (new). **Not** `src/diskio/mod.rs` or `meter.rs` (T036a); ask Heimdall for the `mod` declaration lines, which go in at integration.
