//! A fake `/proc` + `/sys` + `/dev` tree in a temp dir, as a `SysView` (T036b).
#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use storage_analyzer::diskio::sys::{FsSys, SysView};

#[derive(Default)]
pub struct Controls {
    /// Reads of `/sys/dev/block/*/stat` fail while set.
    pub fail_stat: AtomicBool,
    /// Reads of `/sys/dev/block/*/stat` wait while `true`.
    pub gate: (Mutex<bool>, Condvar),
    pub stat_reads: AtomicUsize,
}

pub struct Fake {
    dir: tempfile::TempDir,
    rdevs: Arc<Mutex<HashMap<PathBuf, u64>>>,
    mounts: Mutex<Vec<String>>,
    knames: Mutex<HashMap<(u32, u32), String>>,
    pub controls: Arc<Controls>,
}

impl Fake {
    pub fn new() -> Fake {
        Fake {
            dir: tempfile::tempdir().unwrap(),
            rdevs: Arc::default(),
            mounts: Mutex::default(),
            knames: Mutex::default(),
            controls: Arc::default(),
        }
    }

    fn at(&self, path: &str) -> PathBuf {
        self.dir.path().join(path.trim_start_matches('/'))
    }

    pub fn file(&self, path: &str, content: &str) -> &Self {
        let p = self.at(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
        self
    }

    pub fn link(&self, path: &str, target: &str) -> &Self {
        let p = self.at(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, p).unwrap();
        self
    }

    pub fn dir(&self, path: &str) -> &Self {
        std::fs::create_dir_all(self.at(path)).unwrap();
        self
    }

    /// `/dev/...` node that `stat_rdev` reports as a block device.
    pub fn block_device(&self, node: &str, major: u32, minor: u32) -> &Self {
        self.rdevs
            .lock()
            .unwrap()
            .insert(PathBuf::from(node), rustix::fs::makedev(major, minor));
        self
    }

    /// A non-block `/dev` node (a regular file).
    pub fn plain_node(&self, node: &str) -> &Self {
        self.file(node, "")
    }

    /// The kernel's view of a device: a sysfs directory with its read counter, and the
    /// `/sys/dev/block/<maj:min>` symlink to it, as on a real machine.
    pub fn kernel_device(&self, major: u32, minor: u32, kname: &str, sectors_read: u64) -> &Self {
        self.knames
            .lock()
            .unwrap()
            .insert((major, minor), kname.to_string());
        self.link(
            &format!("/sys/dev/block/{major}:{minor}"),
            &format!("../../devices/pci0000:00/block/{kname}"),
        );
        self.set_sectors(major, minor, sectors_read)
    }

    pub fn set_sectors(&self, major: u32, minor: u32, sectors_read: u64) -> &Self {
        let dir = match self.knames.lock().unwrap().get(&(major, minor)) {
            Some(kname) => format!("/sys/devices/pci0000:00/block/{kname}"),
            None => format!("/sys/dev/block/{major}:{minor}"),
        };
        self.file(
            &format!("{dir}/stat"),
            &format!("  120 0 {sectors_read} 55 3 0 8 2 0 40 57 0 0 0 0 0 0\n"),
        )
    }

    /// A btrfs filesystem and its member devices (kernel name, major, minor).
    pub fn btrfs_members(&self, uuid: &str, members: &[(&str, u32, u32)]) -> &Self {
        for (kname, maj, min) in members {
            self.file(
                &format!("/sys/fs/btrfs/{uuid}/devices/{kname}/dev"),
                &format!("{maj}:{min}\n"),
            );
        }
        self
    }

    /// One `mountinfo` line; `point` is written escaped as the kernel does.
    pub fn mount(&self, point: &str, fstype: &str, source: &str) -> &Self {
        let escaped = point
            .replace('\\', "\\134")
            .replace(' ', "\\040")
            .replace('\t', "\\011")
            .replace('\n', "\\012");
        let id = self.mounts.lock().unwrap().len() + 40;
        let line =
            format!("{id} 1 0:{id} / {escaped} rw,relatime shared:1 - {fstype} {source} rw\n");
        self.mounts.lock().unwrap().push(line);
        let all: String = self.mounts.lock().unwrap().concat();
        self.file("/proc/self/mountinfo", &all)
    }

    pub fn view(&self) -> FakeView {
        FakeView {
            inner: FsSys::new(self.dir.path()),
            rdevs: Arc::clone(&self.rdevs),
            controls: Arc::clone(&self.controls),
        }
    }
}

pub struct FakeView {
    inner: FsSys,
    rdevs: Arc<Mutex<HashMap<PathBuf, u64>>>,
    controls: Arc<Controls>,
}

impl SysView for FakeView {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let is_stat = path.starts_with("/sys/dev/block") && path.ends_with("stat");
        if is_stat {
            let (lock, cv) = &self.controls.gate;
            let mut closed = lock.lock().unwrap();
            while *closed {
                closed = cv.wait(closed).unwrap();
            }
            drop(closed);
            self.controls.stat_reads.fetch_add(1, Ordering::SeqCst);
            if self.controls.fail_stat.load(Ordering::SeqCst) {
                return Err(io::Error::from_raw_os_error(5));
            }
        }
        self.inner.read_to_string(path)
    }

    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        self.inner.read_link(path)
    }

    fn read_dir_names(&self, path: &Path) -> io::Result<Vec<OsString>> {
        self.inner.read_dir_names(path)
    }

    fn stat_rdev(&self, path: &Path) -> io::Result<Option<u64>> {
        if let Some(rdev) = self.rdevs.lock().unwrap().get(path) {
            return Ok(Some(*rdev));
        }
        // Not a block device, but it must exist.
        self.inner.stat_rdev(path)
    }
}
