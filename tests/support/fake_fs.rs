//! In-memory `FsAccess` with error injection and call recording (T006; shared with T007/T009).
#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use storage_analyzer::scan::{Errno, FsAccess, StatInfo};
use storage_analyzer::types::Kind;

#[derive(Clone, Debug)]
struct Node {
    info: StatInfo,
    children: Vec<Box<[u8]>>,
}

#[derive(Default)]
struct State {
    nodes: HashMap<PathBuf, Node>,
    fail_open: HashMap<PathBuf, Errno>,
    fail_read: HashMap<PathBuf, Errno>,
    /// Keyed by the full path of the entry.
    fail_stat: HashMap<PathBuf, Errno>,
    calls: Vec<String>,
    thread_names: Vec<Option<String>>,
    next_ino: u64,
}

pub struct FakeFs {
    state: Mutex<State>,
}

pub const DEV: u64 = 1;
const BLOCK: u64 = 4096;

pub struct FakeDir {
    path: PathBuf,
}

fn info(kind: Kind, disk: u64, apparent: u64, ino: u64) -> StatInfo {
    StatInfo {
        kind,
        disk_bytes: disk,
        apparent_bytes: apparent,
        ino,
        dev: DEV,
        nlink: 1,
    }
}

impl FakeFs {
    /// A tree holding just the root directory.
    pub fn new(root: &str) -> FakeFs {
        let fs = FakeFs {
            state: Mutex::new(State {
                next_ino: 100,
                ..State::default()
            }),
        };
        fs.insert(Path::new(root), Kind::Dir, BLOCK, 0);
        fs
    }

    fn insert(&self, path: &Path, kind: Kind, disk: u64, apparent: u64) {
        let mut st = self.state.lock().unwrap();
        st.next_ino += 1;
        let ino = st.next_ino;
        let node = Node {
            info: info(kind, disk, apparent, ino),
            children: Vec::new(),
        };
        if let Some(old) = st.nodes.insert(path.to_path_buf(), node)
            && let Some(parent) = path.parent().and_then(|p| st.nodes.get_mut(p))
        {
            // Replacing keeps the listing position of the old entry.
            let _ = old;
            let name: Box<[u8]> = path.file_name().unwrap().as_bytes().into();
            if !parent.children.contains(&name) {
                parent.children.push(name);
            }
            return;
        }
        if let Some(parent) = path.parent().and_then(|p| st.nodes.get_mut(p)) {
            parent
                .children
                .push(path.file_name().unwrap().as_bytes().into());
        }
    }

    pub fn dir(&self, path: &str) -> &Self {
        self.insert(Path::new(path), Kind::Dir, BLOCK, 0);
        self
    }

    pub fn file(&self, path: &str, disk: u64, apparent: u64) -> &Self {
        self.insert(Path::new(path), Kind::File, disk, apparent);
        self
    }

    pub fn symlink(&self, path: &str) -> &Self {
        self.insert(Path::new(path), Kind::Symlink, 0, 10);
        self
    }

    /// Removes the node and the entry in its parent (children are left orphaned).
    pub fn remove(&self, path: &str) -> &Self {
        let mut st = self.state.lock().unwrap();
        let path = Path::new(path);
        st.nodes.remove(path);
        let name = path.file_name().unwrap().as_bytes();
        if let Some(parent) = path.parent().and_then(|p| st.nodes.get_mut(p)) {
            parent.children.retain(|c| &**c != name);
        }
        self
    }

    pub fn set_dev(&self, path: &str, dev: u64) -> &Self {
        self.state
            .lock()
            .unwrap()
            .nodes
            .get_mut(Path::new(path))
            .unwrap()
            .info
            .dev = dev;
        self
    }

    pub fn set_nlink(&self, path: &str, ino: u64, nlink: u32) -> &Self {
        let mut st = self.state.lock().unwrap();
        let n = &mut st.nodes.get_mut(Path::new(path)).unwrap().info;
        n.ino = ino;
        n.nlink = nlink;
        drop(st);
        self
    }

    pub fn stat_info(&self, path: &str) -> StatInfo {
        self.state.lock().unwrap().nodes[Path::new(path)].info
    }

    pub fn fail_open(&self, path: &str, e: Errno) -> &Self {
        self.state.lock().unwrap().fail_open.insert(path.into(), e);
        self
    }

    pub fn fail_read(&self, path: &str, e: Errno) -> &Self {
        self.state.lock().unwrap().fail_read.insert(path.into(), e);
        self
    }

    pub fn fail_stat(&self, path: &str, e: Errno) -> &Self {
        self.state.lock().unwrap().fail_stat.insert(path.into(), e);
        self
    }

    /// `depth` levels below `root`, each directory holding `dirs` subdirectories and `files` files.
    pub fn generate(root: &str, depth: u32, dirs: u32, files: u32) -> FakeFs {
        let fs = FakeFs::new(root);
        fs.grow(Path::new(root), depth, dirs, files);
        fs
    }

    fn grow(&self, dir: &Path, depth: u32, dirs: u32, files: u32) {
        for f in 0..files {
            self.insert(&dir.join(format!("f{f}")), Kind::File, BLOCK, 100);
        }
        if depth == 0 {
            return;
        }
        for d in 0..dirs {
            let sub = dir.join(format!("d{d}"));
            self.insert(&sub, Kind::Dir, BLOCK, 0);
            self.grow(&sub, depth - 1, dirs, files);
        }
    }

    /// Every recorded call, e.g. `open /r/a`, `read /r/a`, `stat /r/a f0`.
    pub fn calls(&self) -> Vec<String> {
        self.state.lock().unwrap().calls.clone()
    }

    pub fn thread_names(&self) -> Vec<Option<String>> {
        self.state.lock().unwrap().thread_names.clone()
    }

    fn record(st: &mut State, call: String) {
        st.calls.push(call);
        st.thread_names
            .push(std::thread::current().name().map(str::to_owned));
    }
}

impl FsAccess for FakeFs {
    type Dir = FakeDir;

    fn open_dir(&self, path: &Path) -> Result<FakeDir, Errno> {
        let mut st = self.state.lock().unwrap();
        Self::record(&mut st, format!("open {}", path.display()));
        if let Some(e) = st.fail_open.get(path) {
            return Err(*e);
        }
        match st.nodes.get(path).map(|n| n.info.kind) {
            None => Err(Errno::NOENT),
            Some(Kind::Dir) => Ok(FakeDir {
                path: path.to_path_buf(),
            }),
            Some(Kind::Symlink) => Err(Errno::LOOP),
            Some(_) => Err(Errno::NOTDIR),
        }
    }

    fn read_entries(&self, dir: &mut FakeDir) -> Result<Vec<Box<[u8]>>, Errno> {
        let mut st = self.state.lock().unwrap();
        Self::record(&mut st, format!("read {}", dir.path.display()));
        if let Some(e) = st.fail_read.get(&dir.path) {
            return Err(*e);
        }
        Ok(st
            .nodes
            .get(&dir.path)
            .map(|n| n.children.clone())
            .unwrap_or_default())
    }

    fn stat_at(&self, dir: &FakeDir, name: &[u8]) -> Result<StatInfo, Errno> {
        let mut st = self.state.lock().unwrap();
        let path = dir.path.join(OsStr::from_bytes(name));
        Self::record(&mut st, format!("stat {}", path.display()));
        if let Some(e) = st.fail_stat.get(&path) {
            return Err(*e);
        }
        st.nodes.get(&path).map(|n| n.info).ok_or(Errno::NOENT)
    }
}
