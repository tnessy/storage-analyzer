//! Opt-in history file; one of two modules allowed to write (T019).
//!
//! The file keeps the last complete scan per (canonical root, options), at most 50 roots, and is
//! written atomically. Nothing here runs unless the user passed `--history`.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Roots kept in the file; the least recently written one is dropped first.
pub const MAX_ROOTS: usize = 50;
pub const FILE_NAME: &str = "history.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The canonical root path (lossy UTF-8; it is only a lookup key).
    pub root: String,
    pub one_filesystem: bool,
    pub entries: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub duration_ms: u64,
}

#[derive(Debug)]
pub enum HistoryError {
    Io(io::Error),
    /// The file exists but is not a valid history file.
    Corrupt(String),
}

pub trait HistoryStore: Send + Sync {
    fn load(&self) -> Result<Vec<HistoryEntry>, HistoryError>;
    fn save(&self, entries: &[HistoryEntry]) -> Result<(), HistoryError>;
}

/// `$XDG_STATE_HOME/<app>` or `~/.local/state/<app>`. An empty variable counts as unset.
pub fn state_dir(xdg_state_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = match xdg_state_home.filter(|v| !v.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(home.filter(|v| !v.is_empty())?).join(".local/state"),
    };
    Some(base.join(crate::APP_ID))
}

/// Replaces the entry for the same (root, options) and moves it to the end; caps the list.
pub fn upsert(entries: &mut Vec<HistoryEntry>, new: HistoryEntry) {
    entries.retain(|e| !(e.root == new.root && e.one_filesystem == new.one_filesystem));
    entries.push(new);
    if entries.len() > MAX_ROOTS {
        entries.drain(..entries.len() - MAX_ROOTS);
    }
}

pub fn lookup<'a>(
    entries: &'a [HistoryEntry],
    root: &str,
    one_filesystem: bool,
) -> Option<&'a HistoryEntry> {
    entries
        .iter()
        .find(|e| e.root == root && e.one_filesystem == one_filesystem)
}

/// The history file in a directory.
pub struct FileHistory {
    dir: PathBuf,
}

impl FileHistory {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }
}

impl HistoryStore for FileHistory {
    fn load(&self) -> Result<Vec<HistoryEntry>, HistoryError> {
        match fs::read(self.path()) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| HistoryError::Corrupt(e.to_string()))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(HistoryError::Io(e)),
        }
    }

    /// Writes a temp file next to the history file, then renames it over.
    fn save(&self, entries: &[HistoryEntry]) -> Result<(), HistoryError> {
        let io = HistoryError::Io;
        fs::create_dir_all(&self.dir).map_err(io)?;
        let json =
            serde_json::to_vec_pretty(entries).map_err(|e| HistoryError::Corrupt(e.to_string()))?;
        let temp = self
            .dir
            .join(format!("{FILE_NAME}.tmp-{}", std::process::id()));
        let written = (|| {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&json)?;
            file.sync_all()?;
            fs::rename(&temp, self.path())
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written.map_err(HistoryError::Io)
    }
}
