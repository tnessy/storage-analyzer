//! `AppState`: everything the UI shows besides the model's own data. Main thread only.

use std::time::Duration;

use crossterm::event::{KeyEvent, KeyEventKind};

use super::cli::AppConfig;
use super::keymap::{self, Action};
use crate::model::Model;
use crate::types::{ScanState, SizeMode};
use crate::ui::theme::{Glyphs, Theme};

/// How long a transient status message stays (UX §5.2).
pub const TRANSIENT_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    Quit,
    /// A scan worker died; the text is its panic message.
    ScanError(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transient {
    pub text: String,
    pub until: Duration,
}

pub struct AppState {
    pub config: AppConfig,
    pub model: Model,
    pub size_mode: SizeMode,
    pub glyphs: Glyphs,
    pub theme: Theme,
    pub dialog: Option<Dialog>,
    pub transient: Option<Transient>,
    pub quit: bool,
    /// Clock reading when the scan started; elapsed is measured from here.
    pub started_at: Duration,
    /// Frozen when the scan reaches Done or Stopped.
    pub finished_at: Option<Duration>,
    pub elapsed: Duration,
}

impl AppState {
    pub fn new(config: AppConfig, model: Model) -> Self {
        let size_mode = if config.apparent {
            SizeMode::Apparent
        } else {
            SizeMode::Disk
        };
        Self {
            config,
            model,
            size_mode,
            glyphs: Glyphs::unicode(),
            theme: Theme::standard(),
            dialog: None,
            transient: None,
            quit: false,
            started_at: Duration::ZERO,
            finished_at: None,
            elapsed: Duration::ZERO,
        }
    }

    pub fn is_scanning(&self) -> bool {
        matches!(
            self.model.state(),
            ScanState::Starting | ScanState::Scanning
        )
    }

    /// Shows `text` in place of the status line for 5 s or until the next key.
    pub fn set_transient(&mut self, text: impl Into<String>, now: Duration) {
        self.transient = Some(Transient {
            text: text.into(),
            until: now + TRANSIENT_TTL,
        });
    }

    /// Advances elapsed time and expires the transient message.
    /// Returns whether anything visible changed.
    pub fn tick(&mut self, now: Duration) -> bool {
        if self.finished_at.is_none() && !self.is_scanning() {
            self.finished_at = Some(now);
        }
        let elapsed = self
            .finished_at
            .unwrap_or(now)
            .saturating_sub(self.started_at);
        let mut changed = elapsed.as_secs() != self.elapsed.as_secs();
        self.elapsed = elapsed;
        if self.transient.as_ref().is_some_and(|t| t.until <= now) {
            self.transient = None;
            changed = true;
        }
        changed
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        self.transient = None;
        let action = keymap::action(key);
        match (&self.dialog, action) {
            (None | Some(Dialog::ScanError(_)), Action::Quit) => self.dialog = Some(Dialog::Quit),
            (Some(Dialog::Quit), Action::Quit | Action::Yes) => self.quit = true,
            (Some(Dialog::Quit), Action::No) => self.dialog = None,
            (Some(Dialog::ScanError(_)), _) => self.dialog = None,
            _ => {}
        }
    }
}
