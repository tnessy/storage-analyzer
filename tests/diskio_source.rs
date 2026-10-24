//! The disk-rate source: resolving devices from a fake `/proc` + `/sys`, and the sampler
//! thread (T036b).

#[path = "support/fake_sys.rs"]
mod fake_sys;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fake_sys::Fake;
use storage_analyzer::diskio::ReadBytesSource;
use storage_analyzer::diskio::resolve::{DeviceSet, read_bytes, resolve};
use storage_analyzer::diskio::sys::{FsSys, SysView};
use storage_analyzer::diskio::thread::{DeviceStatSource, DiskioHandle};

fn set(devs: &[(u32, u32)]) -> DeviceSet {
    devs.iter().copied().collect::<BTreeSet<_>>()
}

/// ext4 on `nvme0n1p2` (259:2) of the disk `nvme0n1` (259:0).
fn ext4_machine() -> Fake {
    let f = Fake::new();
    f.block_device("/dev/nvme0n1p2", 259, 2)
        .kernel_device(259, 0, "nvme0n1", 9_999)
        .kernel_device(259, 2, "nvme0n1p2", 1_000)
        .mount("/", "ext4", "/dev/nvme0n1p2");
    f
}

// ---- resolving ---------------------------------------------------------------

#[test]
fn ext4_resolves_to_the_partition_never_the_whole_disk() {
    let f = ext4_machine();
    let view = f.view();
    assert_eq!(
        resolve(Path::new("/home/user"), &view),
        Some(set(&[(259, 2)]))
    );
    assert_eq!(read_bytes(&view, &set(&[(259, 2)])), Some(1_000 * 512));
}

#[test]
fn btrfs_with_one_device_and_with_several_sums_each_member_once() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .kernel_device(8, 1, "sda1", 100)
        .kernel_device(8, 17, "sdb1", 200)
        .kernel_device(8, 33, "sdc1", 300)
        .kernel_device(8, 49, "sdd1", 40_000)
        .btrfs_members(
            "uuid-multi",
            &[("sda1", 8, 1), ("sdb1", 8, 17), ("sdc1", 8, 33)],
        )
        .btrfs_members("uuid-other", &[("sdd1", 8, 49)])
        .mount("/", "btrfs", "/dev/sda1");
    let view = f.view();
    let devices = resolve(Path::new("/data"), &view).unwrap();
    assert_eq!(devices, set(&[(8, 1), (8, 17), (8, 33)]));
    assert_eq!(
        read_bytes(&view, &devices),
        Some(600 * 512),
        "sdd1 is another filesystem"
    );

    // The same with a single member.
    let f = Fake::new();
    f.block_device("/dev/nvme1n1p2", 259, 2)
        .kernel_device(259, 2, "nvme1n1p2", 7)
        .btrfs_members("uuid-one", &[("nvme1n1p2", 259, 2)])
        .mount("/", "btrfs", "/dev/nvme1n1p2");
    assert_eq!(
        resolve(Path::new("/home"), &f.view()),
        Some(set(&[(259, 2)]))
    );
}

#[test]
fn btrfs_without_sysfs_members_falls_back_to_the_device_it_sits_on() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .kernel_device(8, 1, "sda1", 5)
        .mount("/", "btrfs", "/dev/sda1");
    assert_eq!(resolve(Path::new("/x"), &f.view()), Some(set(&[(8, 1)])));
}

#[test]
fn luks_counts_the_mapper_device_only() {
    let f = Fake::new();
    f.block_device("/dev/mapper/cryptroot", 254, 0)
        .kernel_device(254, 0, "dm-0", 700)
        .kernel_device(259, 3, "nvme0n1p3", 5_000_000) // the encrypted partition below
        .mount("/", "ext4", "/dev/mapper/cryptroot");
    let view = f.view();
    let devices = resolve(Path::new("/home"), &view).unwrap();
    assert_eq!(devices, set(&[(254, 0)]));
    assert_eq!(read_bytes(&view, &devices), Some(700 * 512));
}

#[test]
fn filesystems_without_a_block_device_have_no_source() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .kernel_device(8, 1, "sda1", 1)
        .plain_node("/dev/fake-not-a-block-device")
        .mount("/", "ext4", "/dev/sda1")
        .mount("/tmp", "tmpfs", "tmpfs")
        .mount("/mnt/nas", "nfs4", "server:/export")
        .mount("/srv/union", "overlay", "overlay")
        .mount("/mnt/remote", "fuse.sshfs", "user@host:/")
        .mount("/mnt/file", "ext4", "/dev/fake-not-a-block-device");
    let view = f.view();
    for path in [
        "/tmp/x",
        "/mnt/nas/a",
        "/srv/union",
        "/mnt/remote/b",
        "/mnt/file",
    ] {
        assert_eq!(resolve(Path::new(path), &view), None, "{path}");
    }
    assert!(resolve(Path::new("/home"), &view).is_some());
}

#[test]
fn a_source_that_is_not_an_absolute_path_is_never_looked_up() {
    // A relative source ("tmpfs") would otherwise be looked up against the working
    // directory, where a block device of that name could exist.
    let f = Fake::new();
    f.block_device("tmpfs", 1, 1)
        .block_device("server:/export", 2, 2)
        .kernel_device(1, 1, "ram0", 1)
        .kernel_device(2, 2, "nbd0", 1)
        .mount("/tmp", "tmpfs", "tmpfs")
        .mount("/mnt/nas", "nfs4", "server:/export");
    let view = f.view();
    assert_eq!(resolve(Path::new("/tmp/x"), &view), None);
    assert_eq!(resolve(Path::new("/mnt/nas"), &view), None);
}

#[test]
fn an_unmounted_nested_subvolume_belongs_to_its_parent_filesystem() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .block_device("/dev/sdb1", 8, 17)
        .kernel_device(8, 1, "sda1", 1)
        .kernel_device(8, 17, "sdb1", 1)
        .btrfs_members("u1", &[("sda1", 8, 1)])
        .mount("/", "btrfs", "/dev/sda1")
        .mount("/data", "ext4", "/dev/sdb1");
    let view = f.view();
    assert_eq!(
        resolve(Path::new("/home/u/.snapshots/12/snapshot"), &view),
        Some(set(&[(8, 1)]))
    );
    assert_eq!(
        resolve(Path::new("/data/deep"), &view),
        Some(set(&[(8, 17)]))
    );
}

#[test]
fn the_longest_mount_prefix_wins_by_path_component() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .block_device("/dev/sdb1", 8, 17)
        .kernel_device(8, 1, "sda1", 1)
        .kernel_device(8, 17, "sdb1", 1)
        .mount("/", "ext4", "/dev/sda1")
        .mount("/home", "ext4", "/dev/sdb1");
    let view = f.view();
    assert_eq!(resolve(Path::new("/home/u"), &view), Some(set(&[(8, 17)])));
    assert_eq!(resolve(Path::new("/home"), &view), Some(set(&[(8, 17)])));
    assert_eq!(
        resolve(Path::new("/homework"), &view),
        Some(set(&[(8, 1)])),
        "not a path prefix"
    );
}

#[test]
fn escaped_mount_points_are_decoded() {
    let f = Fake::new();
    f.block_device("/dev/sdb1", 8, 17)
        .kernel_device(8, 17, "sdb1", 1)
        .mount("/mnt/my disk", "ext4", "/dev/sdb1")
        .mount("/mnt/tab\tand\\slash", "ext4", "/dev/sdb1");
    let view = f.view();
    assert_eq!(
        resolve(Path::new("/mnt/my disk/data"), &view),
        Some(set(&[(8, 17)]))
    );
    assert_eq!(
        resolve(Path::new("/mnt/tab\tand\\slash/x"), &view),
        Some(set(&[(8, 17)]))
    );
    assert_eq!(
        resolve(Path::new("/mnt/my"), &view),
        None,
        "no mount holds it"
    );
}

#[test]
fn without_a_mount_table_nothing_resolves() {
    let f = Fake::new();
    assert_eq!(resolve(Path::new("/"), &f.view()), None);
}

#[test]
fn the_root_filesystem_and_everything_mounted_below_it_make_the_set() {
    let f = Fake::new();
    for (node, maj, min, name) in [
        ("/dev/sda1", 8, 1, "sda1"),
        ("/dev/sdb1", 8, 17, "sdb1"),
        ("/dev/sdc1", 8, 33, "sdc1"),
        ("/dev/sdd1", 8, 49, "sdd1"),
    ] {
        f.block_device(node, maj, min)
            .kernel_device(maj, min, name, 10);
    }
    f.mount("/", "ext4", "/dev/sda1")
        .mount("/mnt/data", "ext4", "/dev/sdb1")
        .mount("/mnt/data/usb", "ext4", "/dev/sdc1")
        .mount("/mnt/data/ram", "tmpfs", "tmpfs")
        .mount("/other", "ext4", "/dev/sdd1");
    let view = f.view();
    assert_eq!(
        resolve(Path::new("/mnt/data"), &view),
        Some(set(&[(8, 17), (8, 33)])),
        "the root's filesystem and what is mounted below it, nothing else"
    );
    assert_eq!(
        resolve(Path::new("/mnt/data/ram"), &view),
        None,
        "tmpfs alone"
    );
}

#[test]
fn a_device_mounted_twice_is_counted_once_and_a_tmpfs_root_still_has_its_mounts() {
    let f = Fake::new();
    f.block_device("/dev/sdb1", 8, 17)
        .block_device("/dev/sdc1", 8, 33)
        .kernel_device(8, 17, "sdb1", 100)
        .kernel_device(8, 33, "sdc1", 50)
        .btrfs_members("u-b", &[("sdb1", 8, 17)])
        .mount("/mnt/data", "btrfs", "/dev/sdb1")
        .mount("/mnt/data/snaps", "btrfs", "/dev/sdb1")
        .mount("/run/scratch", "tmpfs", "tmpfs")
        .mount("/run/scratch/disk", "ext4", "/dev/sdc1");
    let view = f.view();
    let twice = resolve(Path::new("/mnt/data"), &view).unwrap();
    assert_eq!(twice, set(&[(8, 17)]));
    assert_eq!(read_bytes(&view, &twice), Some(100 * 512), "summed once");
    assert_eq!(
        resolve(Path::new("/run/scratch"), &view),
        Some(set(&[(8, 33)]))
    );
}

#[test]
fn a_device_that_cannot_be_read_makes_the_sum_unavailable() {
    let f = Fake::new();
    f.block_device("/dev/sda1", 8, 1)
        .kernel_device(8, 1, "sda1", 10)
        .kernel_device(8, 17, "sdb1", 20)
        .set_sectors(8, 33, 0);
    let view = f.view();
    assert_eq!(read_bytes(&view, &set(&[(8, 1), (8, 17)])), Some(30 * 512));
    // 8:49 has no stat file; dropping it silently would make the sum shrink.
    assert_eq!(read_bytes(&view, &set(&[(8, 1), (8, 49)])), None);
    f.file("/sys/dev/block/8:1/stat", "garbage");
    assert_eq!(read_bytes(&view, &set(&[(8, 1)])), None);
}

// ---- the thread ----------------------------------------------------------------

fn wait_for<T>(timeout: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn started(f: &Fake, root: &str) -> (DiskioHandle, DeviceStatSource) {
    DiskioHandle::start(PathBuf::from(root), Box::new(f.view()))
}

#[test]
fn the_total_is_stored_and_follows_the_counters_about_once_a_second() {
    let f = ext4_machine();
    let (mut handle, mut source) = started(&f, "/home");
    assert_eq!(
        wait_for(Duration::from_secs(2), || source.read_bytes()),
        Some(1_000 * 512)
    );

    f.set_sectors(259, 2, 5_000);
    let begun = Instant::now();
    let seen = wait_for(Duration::from_secs(3), || {
        source.read_bytes().filter(|&b| b == 5_000 * 512)
    });
    assert_eq!(seen, Some(5_000 * 512));
    assert!(
        begun.elapsed() < Duration::from_millis(2_200),
        "{:?}",
        begun.elapsed()
    );

    let reads_before = f.controls.stat_reads.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(3_100));
    let reads = f.controls.stat_reads.load(Ordering::SeqCst) - reads_before;
    assert!(
        (2..=5).contains(&reads),
        "about one read a second, got {reads} in 3.1 s"
    );
    handle.stop();
}

#[test]
fn the_source_is_unavailable_until_resolved_and_for_filesystems_without_a_disk() {
    let f = Fake::new();
    f.mount("/", "tmpfs", "tmpfs");
    let (mut handle, mut source) = started(&f, "/");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(source.read_bytes(), None);
    handle.stop();
}

#[test]
fn the_set_is_resolved_once_at_the_start_and_never_again() {
    let f = Fake::new();
    f.block_device("/dev/sdb1", 8, 17)
        .block_device("/dev/sdc1", 8, 33)
        .block_device("/dev/sdd1", 8, 49)
        .kernel_device(8, 17, "sdb1", 100)
        .kernel_device(8, 33, "sdc1", 200)
        .kernel_device(8, 49, "sdd1", 7_000)
        .mount("/mnt/data", "ext4", "/dev/sdb1")
        .mount("/mnt/data/usb", "ext4", "/dev/sdc1");
    let (mut handle, mut source) = started(&f, "/mnt/data");
    assert_eq!(
        wait_for(Duration::from_secs(2), || source.read_bytes()),
        Some(300 * 512)
    );

    // A device mounted later is not added.
    f.mount("/mnt/data/late", "ext4", "/dev/sdd1");
    f.set_sectors(8, 17, 150);
    assert_eq!(
        wait_for(Duration::from_secs(3), || source
            .read_bytes()
            .filter(|&b| b == 350 * 512)),
        Some(350 * 512)
    );
    handle.stop();
}

#[test]
fn a_read_error_makes_the_source_unavailable_for_the_rest_of_the_scan() {
    let f = ext4_machine();
    let (mut handle, mut source) = started(&f, "/");
    assert!(wait_for(Duration::from_secs(2), || source.read_bytes()).is_some());
    f.controls.fail_stat.store(true, Ordering::SeqCst);
    assert_eq!(
        wait_for(Duration::from_secs(3), || source
            .read_bytes()
            .is_none()
            .then_some(())),
        Some(())
    );
    f.controls.fail_stat.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(1_300));
    assert_eq!(source.read_bytes(), None, "it does not come back");
    handle.stop();
}

fn close_gate(f: &Fake) {
    *f.controls.gate.0.lock().unwrap() = true;
}

fn open_gate(f: &Fake) {
    *f.controls.gate.0.lock().unwrap() = false;
    f.controls.gate.1.notify_all();
}

#[test]
fn reading_the_source_never_blocks_while_the_thread_is_busy() {
    let f = ext4_machine();
    close_gate(&f);
    let (mut handle, mut source) = started(&f, "/");
    std::thread::sleep(Duration::from_millis(150)); // the thread is now stuck in a read
    let begun = Instant::now();
    for _ in 0..10_000 {
        assert_eq!(source.read_bytes(), None);
    }
    assert!(
        begun.elapsed() < Duration::from_millis(100),
        "{:?}",
        begun.elapsed()
    );
    open_gate(&f);
    assert!(wait_for(Duration::from_secs(2), || source.read_bytes()).is_some());
    handle.stop();
}

#[test]
fn stop_returns_within_250_ms_even_when_a_read_hangs() {
    let f = ext4_machine();
    close_gate(&f);
    let (mut handle, _source) = started(&f, "/");
    std::thread::sleep(Duration::from_millis(100));
    let begun = Instant::now();
    handle.stop();
    assert!(
        begun.elapsed() < Duration::from_millis(250),
        "{:?}",
        begun.elapsed()
    );
    open_gate(&f); // let the abandoned thread end
}

#[test]
fn dropping_the_handle_stops_a_healthy_thread_quickly() {
    let f = ext4_machine();
    let (handle, mut source) = started(&f, "/");
    assert!(wait_for(Duration::from_secs(2), || source.read_bytes()).is_some());
    let begun = Instant::now();
    drop(handle);
    assert!(
        begun.elapsed() < Duration::from_millis(250),
        "{:?}",
        begun.elapsed()
    );
}

/// A view that panics on its first read and records which thread it ran on.
struct Panicking(Arc<Mutex<Option<String>>>);

impl SysView for Panicking {
    fn read_to_string(&self, _: &Path) -> io::Result<String> {
        *self.0.lock().unwrap() = std::thread::current().name().map(str::to_owned);
        panic!("test panic on the disk thread");
    }
    fn read_link(&self, _: &Path) -> io::Result<PathBuf> {
        Err(io::ErrorKind::NotFound.into())
    }
    fn read_dir_names(&self, _: &Path) -> io::Result<Vec<OsString>> {
        Ok(vec![])
    }
    fn stat_rdev(&self, _: &Path) -> io::Result<Option<u64>> {
        Ok(None)
    }
}

#[test]
fn a_panic_on_the_disk_thread_is_tagged_and_only_hides_the_field() {
    let ran_on = Arc::new(Mutex::new(None));
    let (mut handle, mut source) =
        DiskioHandle::start(PathBuf::from("/"), Box::new(Panicking(Arc::clone(&ran_on))));
    assert!(wait_for(Duration::from_secs(2), || ran_on.lock().unwrap().clone()).is_some());
    assert_eq!(ran_on.lock().unwrap().as_deref(), Some("diskio"));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(source.read_bytes(), None);
    let begun = Instant::now();
    handle.stop();
    assert!(begun.elapsed() < Duration::from_millis(250));
}

// ---- the real machine ------------------------------------------------------------

#[test]
fn the_real_home_resolves_to_a_readable_device_set() {
    if !Path::new("/sys/dev/block").is_dir() || !Path::new("/proc/self/mountinfo").exists() {
        println!("SKIP: /sys/dev/block or /proc is missing");
        return;
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let view = FsSys::real();
    match resolve(&home, &view) {
        Some(devices) => {
            assert!(!devices.is_empty());
            assert!(read_bytes(&view, &devices).is_some(), "{devices:?}");
            println!("HOME is on {devices:?}");
        }
        None => println!("SKIP: $HOME is not on a block device here (tmpfs, overlay, network)"),
    }
}
