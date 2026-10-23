//! Main loop and `AppState` (T015).

pub mod cli;
pub mod event_loop;
pub mod keymap;
pub mod state;
pub mod terminal;
pub mod tree_state;

use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use rustix::fs::{FileType, Mode, OFlags, fstat, open, stat};
use rustix::io::Errno;

use crate::model::{Model, ModelOptions};
use crate::scan::{self, RawEntry, RealFs};
use crate::types::Kind;
use cli::{AppConfig, CliError};
use event_loop::{App, CrosstermEvents, ScanStarter, SystemClock};
use state::AppState;

const EXIT_USAGE: u8 = 2;
const DISK_BLOCK_BYTES: u64 = 512;

/// The validated scan root.
#[derive(Debug)]
pub struct Root {
    pub path: PathBuf,
    pub entry: RawEntry,
}

#[derive(Debug)]
pub struct PreflightError {
    path: PathBuf,
    /// `None` means the path exists but is not a directory.
    errno: Option<Errno>,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.path.display();
        match self.errno {
            Some(Errno::NOENT) => write!(f, "{path}: no such file or directory"),
            Some(Errno::ACCESS | Errno::PERM) => write!(f, "{path}: permission denied"),
            Some(Errno::NOTDIR) | None => write!(f, "{path}: not a directory"),
            Some(e) => write!(
                f,
                "{path}: {}",
                std::io::Error::from(e).to_string().to_lowercase()
            ),
        }
    }
}

/// Opens the root before the terminal is touched (design §3.3). A symlink given as PATH is followed.
pub fn preflight(path: &Path) -> Result<Root, PreflightError> {
    let fail = |errno: Option<Errno>| PreflightError {
        path: path.to_owned(),
        errno,
    };
    let mode = stat(path).map_err(|e| fail(Some(e)))?.st_mode;
    if FileType::from_raw_mode(mode) != FileType::Directory {
        return Err(fail(None));
    }
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| fail(Some(e)))?;
    let st = fstat(&fd).map_err(|e| fail(Some(e)))?;
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| fail(e.raw_os_error().map(Errno::from_raw_os_error)))?;
    let entry = RawEntry {
        name: canonical.as_os_str().as_bytes().into(),
        kind: Kind::Dir,
        disk_bytes: u64::try_from(st.st_blocks)
            .unwrap_or(0)
            .saturating_mul(DISK_BLOCK_BYTES),
        apparent_bytes: u64::try_from(st.st_size).unwrap_or(0),
        ino: st.st_ino,
        dev: st.st_dev,
        nlink: u32::try_from(st.st_nlink).unwrap_or(u32::MAX),
        issue: None,
    };
    Ok(Root {
        path: canonical,
        entry,
    })
}

/// Starts the real parallel walker.
pub fn production_starter() -> ScanStarter {
    Box::new(|launch| {
        Some(scan::start(
            Arc::new(RealFs),
            launch.root,
            launch.root_dev,
            launch.options,
            launch.control,
            launch.tx,
            launch.first_id,
        ))
    })
}

fn run_tui(config: AppConfig, root: Root) -> std::io::Result<()> {
    let model = Model::new(root.path, root.entry, ModelOptions { top_n: config.top });
    let state = AppState::new(config, model);
    let terminate = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(signal, terminate.clone())?;
    }
    let panics = terminal::PanicSlot::default();
    terminal::install_panic_hook(panics.clone());
    let _guard = terminal::Guard::enter_terminal()?;
    let terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;
    App::new(
        terminal,
        CrosstermEvents,
        SystemClock::new(),
        state,
        production_starter(),
        terminate,
        panics,
    )
    .run()
}

/// Entry point for the binary. `bin` is the command name used in messages.
pub fn main_with(bin: &'static str) -> ExitCode {
    let config = match cli::parse(bin, std::env::args_os()) {
        Ok(c) => c,
        Err(CliError::Usage(message)) => {
            eprintln!("{}", cli::usage_error_text(bin, &message));
            return ExitCode::from(EXIT_USAGE);
        }
        Err(CliError::Clap(e)) => e.exit(),
    };
    let root = match preflight(&config.path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{bin}: {e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    match run_tui(config, root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{bin}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_and_file_give_the_spec_text() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert_eq!(
            preflight(&missing).unwrap_err().to_string(),
            format!("{}: no such file or directory", missing.display())
        );
        let file = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        assert_eq!(
            preflight(file.path()).unwrap_err().to_string(),
            format!("{}: not a directory", file.path().display())
        );
    }

    #[test]
    fn directory_root_is_canonical_with_its_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = preflight(dir.path()).unwrap();
        assert_eq!(root.path, std::fs::canonicalize(dir.path()).unwrap());
        assert_eq!(root.entry.kind, Kind::Dir);
        assert_ne!(root.entry.ino, 0);
    }
}
