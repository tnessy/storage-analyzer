//! Which block devices hold a path (design §3.12). By path through `/proc/self/mountinfo`,
//! never by `st_dev` (btrfs subvolumes have anonymous ones). Read-only.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::sys::SysView;

/// `(major, minor)` of the devices to sum.
pub type DeviceSet = BTreeSet<(u32, u32)>;

const MOUNTINFO: &str = "/proc/self/mountinfo";
const SECTOR_BYTES: u64 = 512;
/// Field 3 of `/sys/dev/block/<maj:min>/stat` (0-based index 2): sectors read.
const SECTORS_READ_FIELD: usize = 2;

struct Mount {
    point: PathBuf,
    fstype: String,
    source: String,
}

/// `\040` and friends in mountinfo fields are the octal bytes of the character.
fn unescape(field: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = (bytes[i] == b'\\' && i + 4 <= bytes.len())
            .then(|| std::str::from_utf8(&bytes[i + 1..i + 4]).ok())
            .flatten()
            .and_then(|d| u8::from_str_radix(d, 8).ok());
        match octal {
            Some(b) => {
                out.push(b);
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    PathBuf::from(std::ffi::OsString::from_vec(out))
}

fn mounts(view: &dyn SysView) -> Option<Vec<Mount>> {
    let text = view.read_to_string(Path::new(MOUNTINFO)).ok()?;
    let parsed = text
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let mut right = right.split(' ');
            let fstype = right.next()?.to_string();
            let source = right.next()?.to_string();
            Some(Mount {
                point: unescape(point),
                fstype,
                source,
            })
        })
        .collect();
    Some(parsed)
}

/// The mount holding `path`: the longest mount point that is a prefix; a later line wins a tie.
fn holding<'a>(mounts: &'a [Mount], path: &Path) -> Option<&'a Mount> {
    mounts
        .iter()
        .filter(|m| path.starts_with(&m.point))
        .max_by_key(|m| m.point.components().count())
}

fn split_rdev(rdev: u64) -> (u32, u32) {
    (rustix::fs::major(rdev), rustix::fs::minor(rdev))
}

fn kernel_name(view: &dyn SysView, dev: (u32, u32)) -> Option<std::ffi::OsString> {
    let link = view
        .read_link(&PathBuf::from(format!(
            "/sys/dev/block/{}:{}",
            dev.0, dev.1
        )))
        .ok()?;
    link.file_name().map(ToOwned::to_owned)
}

/// Every member of the btrfs filesystem that `dev` belongs to (including `dev`).
fn btrfs_members(view: &dyn SysView, dev: (u32, u32)) -> Option<DeviceSet> {
    let kname = kernel_name(view, dev)?;
    let root = Path::new("/sys/fs/btrfs");
    for uuid in view.read_dir_names(root).ok()? {
        let devices = root.join(&uuid).join("devices");
        let Ok(names) = view.read_dir_names(&devices) else {
            continue;
        };
        if !names.contains(&kname) {
            continue;
        }
        let mut set = DeviceSet::new();
        for name in names {
            let text = view.read_to_string(&devices.join(name).join("dev")).ok()?;
            let (maj, min) = text.trim().split_once(':')?;
            set.insert((maj.parse().ok()?, min.parse().ok()?));
        }
        return Some(set);
    }
    None
}

/// The devices behind one mount, or `None` if its source is not a block device.
fn devices_of(view: &dyn SysView, mount: &Mount) -> Option<DeviceSet> {
    if !mount.source.starts_with('/') {
        return None;
    }
    let rdev = view.stat_rdev(Path::new(&mount.source)).ok()??;
    let dev = split_rdev(rdev);
    if mount.fstype == "btrfs"
        && let Some(members) = btrfs_members(view, dev)
    {
        return Some(members);
    }
    Some(DeviceSet::from([dev]))
}

/// The block devices to sum for a scan of `root`: the one holding `root` plus those of every
/// mount at or under it, each once. Mounts that are not block devices (tmpfs, overlay, FUSE,
/// NFS) contribute nothing; `None` if nothing is left. A nested unmounted subvolume belongs
/// to its parent filesystem. Resolved once, at the start of the scan.
pub fn resolve(root: &Path, view: &dyn SysView) -> Option<DeviceSet> {
    let mounts = mounts(view)?;
    let mut set = holding(&mounts, root)
        .and_then(|m| devices_of(view, m))
        .unwrap_or_default();
    for mount in mounts.iter().filter(|m| m.point.starts_with(root)) {
        set.extend(devices_of(view, mount).unwrap_or_default());
    }
    (!set.is_empty()).then_some(set)
}

/// Bytes read so far by all of `set`: the sum of sectors read times 512. Any device that
/// cannot be read makes the whole sum unavailable (dropping it would look like a reset).
pub fn read_bytes(view: &dyn SysView, set: &DeviceSet) -> Option<u64> {
    let mut total = 0u64;
    for (maj, min) in set {
        let text = view
            .read_to_string(&PathBuf::from(format!("/sys/dev/block/{maj}:{min}/stat")))
            .ok()?;
        let sectors: u64 = text
            .split_whitespace()
            .nth(SECTORS_READ_FIELD)?
            .parse()
            .ok()?;
        total = total.checked_add(sectors.checked_mul(SECTOR_BYTES)?)?;
    }
    Some(total)
}
