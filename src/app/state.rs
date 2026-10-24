//! `AppState`: everything the UI shows besides the model's own data. Main thread only.

use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use super::cli::AppConfig;
use super::delete_flow::{DeleteAborted, DeleteDone, DeletePrompt, DeleteRun};
use super::keymap::{self, Action};
use super::list_state::{ListState, View};
use super::map_state::{MapState, Pane};
use super::tree_state::{AppEvent, TreeState};
use crate::eta::sources::EtaSources;
use crate::eta::{EtaInput, EtaView, Rate};
use crate::history::HistoryEntry;
use crate::layout::Direction;
use crate::model::Model;
use crate::scan::{ScanControl, ScanHandle};
use crate::types::NodeId;
use crate::types::{IssueKind, ScanState, SizeMode};
use crate::ui::theme::{Glyphs, Theme};
use crate::ui::tile_colors::TileColors;

/// How long a transient status message stays (UX §5.2).
pub const TRANSIENT_TTL: Duration = Duration::from_secs(5);
/// Time constant of the entries-per-second average (design §3.6).
pub(super) const RATE_TAU: Duration = Duration::from_secs(5);
/// One spinner frame lasts this long, so it never moves faster than 10 fps (UX §7).
const SPINNER_FRAME_MS: u128 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    Quit,
    /// The key reference (`?`).
    Help,
    /// A scan worker died; the text is its panic message.
    ScanError(String),
    /// Confirm a delete (UX §5.3).
    Delete(DeletePrompt),
    /// `Delete finished with errors`.
    DeleteDone(DeleteDone),
    /// `Not deleted`.
    DeleteAborted(DeleteAborted),
    /// The failed paths of the last delete; `back` is where `Esc`/`v` returns to.
    FailedPaths {
        scroll: usize,
        back: Option<Box<Dialog>>,
    },
    /// `q` while a delete runs: only `y` quits.
    QuitWhileDeleting,
}

/// What this session knows about a finished scan of the root (never persisted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    pub root: String,
    pub one_filesystem: bool,
    pub entries: u64,
}

/// Which scan just finished: only the root's own scan feeds the ETA records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanKind {
    Root,
    Subtree,
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
    pub tree: TreeState,
    pub map: MapState,
    /// Presentation memory for the map's colours (UX §7, option C).
    pub tile_colors: TileColors,
    pub lists: ListState,
    pub view: View,
    /// Rows of the list pane at the last `prepare_view`.
    list_height: usize,
    /// Which pane the arrow keys drive.
    pub focus: Pane,
    /// The terminal size at the last `prepare_view`.
    pub screen: Rect,
    /// The loop clock at the last `tick`.
    pub now: Duration,
    /// The folder whose scan is prioritised (set by T020).
    pub priority: Option<NodeId>,
    /// Events for other parts of the app; drained by the loop.
    pub events: Vec<AppEvent>,
    /// EWMA of entries per second, fed on every tick.
    pub rate: Rate,
    pub eta_sources: EtaSources,
    pub session_previous: Option<SessionRecord>,
    /// A finished scan the loop should write to the history file (only with `--history`).
    pub pending_history_write: Option<HistoryEntry>,
    /// Shared with the scan workers: focus, skipped prefixes, cancel-all.
    pub control: Arc<ScanControl>,
    /// The running walker; `None` in tests that feed messages by hand.
    pub scan_handle: Option<ScanHandle>,
    /// A rescan has started and its `Done` has not arrived yet (the model's own state stays
    /// `Done`/`Stopped` until it does).
    pub rescan_in_flight: bool,
    /// The running delete, if any.
    pub delete_run: Option<DeleteRun>,
    pub delete_handle: Option<crate::delete::DeleteHandle>,
    /// Failures of the most recent finished delete; replaced by the next one.
    pub last_delete_failures: Vec<crate::delete::DeleteFailure>,
    /// The target the failures belong to (for the overlay's title).
    pub last_delete_path: std::path::PathBuf,
    /// Set when the user quit with `y` during a delete: the line for stderr, once known.
    /// The panic message of the delete executor, for the result dialog.
    pub delete_panic: Option<String>,
    /// Whether the scan in progress (or last finished) is the root's own, or a subtree rescan.
    pub scan_kind: ScanKind,
    /// Elapsed time when `s` was pressed; the status shows it frozen while stopping.
    pub stop_elapsed: Option<Duration>,
    /// Rescans the app asked for while stopping; they start right after `Done { cancelled }`.
    pub queued_rescans: Vec<NodeId>,
    /// `entries_seen` when the current scan began (the model's counter never goes down).
    pub seen_baseline: u64,
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
            tree: TreeState::new(),
            map: MapState::new(),
            tile_colors: TileColors::default(),
            lists: ListState::new(),
            view: View::Tree,
            list_height: 20,
            focus: Pane::Tree,
            screen: Rect::new(0, 0, 80, 24),
            now: Duration::ZERO,
            priority: None,
            events: Vec::new(),
            rate: Rate::new(RATE_TAU),
            eta_sources: EtaSources::default(),
            session_previous: None,
            pending_history_write: None,
            control: Arc::new(ScanControl::new()),
            scan_handle: None,
            rescan_in_flight: false,
            delete_run: None,
            delete_handle: None,
            last_delete_failures: Vec::new(),
            last_delete_path: std::path::PathBuf::new(),
            delete_panic: None,
            scan_kind: ScanKind::Root,
            stop_elapsed: None,
            queued_rescans: Vec::new(),
            seen_baseline: 0,
            dialog: None,
            transient: None,
            quit: false,
            started_at: Duration::ZERO,
            finished_at: None,
            elapsed: Duration::ZERO,
        }
    }

    pub fn is_scanning(&self) -> bool {
        matches!(self.phase(), ScanState::Starting | ScanState::Scanning)
    }

    /// The estimator's view of the scan right now.
    pub fn eta_view(&self) -> EtaView {
        let totals = self.model.totals();
        let (top_done, top_total) = self.model.top_level_progress();
        crate::eta::view(&EtaInput {
            elapsed: self.elapsed,
            seen: self.scan_entries(),
            pending_dirs: totals.pending_dirs,
            rate_eps: self.rate.eps(),
            source: self.eta_sources.best(),
            state: self.phase(),
            top_done,
            top_total,
        })
    }

    /// Entries seen by the scan in progress (the model's counter keeps counting across rescans).
    pub fn scan_entries(&self) -> u64 {
        self.model
            .totals()
            .entries_seen
            .saturating_sub(self.seen_baseline)
    }

    /// The canonical root and the options that make two scans comparable.
    pub fn root_key(&self) -> (String, bool) {
        let root = self.model.path(self.model.root());
        (
            root.to_string_lossy().into_owned(),
            self.config.one_filesystem,
        )
    }

    /// Whether the user skipped any folder; such a scan is not a full picture of its root.
    fn has_user_skips(&self) -> bool {
        self.model
            .issues()
            .iter()
            .any(|i| i.kind == IssueKind::SkippedByUser)
    }

    /// Records a finished scan: the in-session total and, if the user opted in, a history write.
    /// Only a complete scan of the root counts; subtree rescans never do.
    pub fn scan_finished(&mut self, kind: ScanKind) {
        if kind != ScanKind::Root || self.model.state() != ScanState::Done || self.has_user_skips()
        {
            return;
        }
        let (root, one_filesystem) = self.root_key();
        let totals = self.model.totals();
        self.session_previous = Some(SessionRecord {
            root: root.clone(),
            one_filesystem,
            entries: self.scan_entries(),
        });
        if self.config.history {
            self.pending_history_write = Some(HistoryEntry {
                root,
                one_filesystem,
                entries: self.scan_entries(),
                dirs: totals.dirs,
                bytes: totals.disk_bytes,
                duration_ms: u64::try_from(self.elapsed.as_millis()).unwrap_or(u64::MAX),
            });
        }
    }

    /// Takes the in-session record as the ETA source for a rescan of the root, if it is for the
    /// same root and options.
    pub fn use_session_previous(&mut self) {
        let (root, one_filesystem) = self.root_key();
        self.eta_sources.session = self
            .session_previous
            .as_ref()
            .filter(|r| r.root == root && r.one_filesystem == one_filesystem)
            .map(|r| r.entries);
    }

    /// The activity glyph: an arc that turns at 10 frames per second, or a still `◌` with
    /// `--no-animation`.
    pub fn spinner_glyph(&self) -> &'static str {
        if self.config.no_animation {
            return self.glyphs.scanning;
        }
        self.glyphs.spinner[self.spinner_frame()]
    }

    /// Index of the current spinner frame.
    pub fn spinner_frame(&self) -> usize {
        (self.now.as_millis() / SPINNER_FRAME_MS) as usize % self.glyphs.spinner.len()
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
        self.now = now;
        if self.is_scanning() {
            self.rate
                .sample(now.saturating_sub(self.started_at), self.scan_entries());
        }
        if self.finished_at.is_none() && !self.is_scanning() {
            self.finished_at = Some(now);
            self.scan_finished(self.scan_kind);
        }
        // `s` freezes the clock: the Stopping and Stopped lines show the time it was pressed.
        let elapsed = self.stop_elapsed.unwrap_or_else(|| {
            self.finished_at
                .unwrap_or(now)
                .saturating_sub(self.started_at)
        });
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
        let too_small = crate::ui::too_small::is_too_small(self.screen.width, self.screen.height);
        match (&self.dialog, action) {
            (None | Some(Dialog::ScanError(_)), Action::Quit) if self.delete_run.is_some() => {
                self.dialog = Some(Dialog::QuitWhileDeleting);
            }
            (None | Some(Dialog::ScanError(_)), Action::Quit) => self.dialog = Some(Dialog::Quit),
            (Some(Dialog::QuitWhileDeleting), action) => self.quit_while_deleting_key(action),
            (Some(Dialog::Delete(_)), action) => self.delete_prompt_key(action),
            (Some(Dialog::DeleteDone(_) | Dialog::DeleteAborted(_)), action) => {
                self.delete_result_key(action);
            }
            (Some(Dialog::FailedPaths { .. }), action) => self.failed_paths_key(action),
            (Some(Dialog::Quit), Action::Quit | Action::Yes) => self.quit = true,
            (Some(Dialog::Quit), Action::No | Action::Esc | Action::Enter) => self.dialog = None,
            (Some(Dialog::ScanError(_)), _) => self.dialog = None,
            (Some(Dialog::Quit), _) => {}
            (Some(Dialog::Help), Action::Esc | Action::Help) => self.dialog = None,
            (Some(Dialog::Help), Action::Quit) => self.dialog = Some(Dialog::Quit),
            (Some(Dialog::Help), _) => {}
            // Below 80×24 every key but the quit keys is ignored (UX §3).
            (None, _) if too_small => {}
            (None, Action::Help) => self.dialog = Some(Dialog::Help),
            (None, Action::ToggleSize) => self.toggle_size_mode(),
            (None, Action::Stop) => self.stop_scan(),
            (None, Action::ViewFailures) => self.press_view_failures(),
            (None, Action::View(n)) => {
                self.view = [View::Tree, View::Types, View::Largest, View::Skipped]
                    [usize::from(n.clamp(1, 4)) - 1];
                // Keys can arrive before the next frame; the list must already have a cursor.
                self.lists
                    .prepare(self.view, &self.model, self.size_mode, self.list_height);
            }
            (None, Action::SwitchPane) => {
                if self.focus == Pane::Tree && self.view == View::Tree && self.tree.on_parent {
                    self.select_largest_tile();
                }
                self.focus = match self.focus {
                    Pane::Tree => Pane::Map,
                    Pane::Map => Pane::Tree,
                };
            }
            // The `..` row takes these before any later task acts on them.
            (None, Action::Delete | Action::Prioritise | Action::Skip | Action::Resume)
                if self.focus == Pane::Tree && self.view == View::Tree && self.tree.on_parent =>
            {
                let now = self.now;
                self.set_transient(crate::ui::copy::PARENT_ROW_HINT, now);
            }
            (None, Action::Prioritise) if self.view == View::Tree => self.press_priority(),
            (None, Action::Skip) if self.view == View::Tree => self.press_skip(),
            (None, Action::Resume) if matches!(self.view, View::Tree | View::Skipped) => {
                self.press_resume();
            }
            (None, Action::Delete) if matches!(self.view, View::Tree | View::Largest) => {
                self.press_delete();
            }
            (None, action) if self.focus == Pane::Map => self.map_key(action),
            (None, action) if self.view != View::Tree => self.list_key(action),
            (None, action) => {
                self.map.on_aggregate = false;
                self.tree
                    .apply(action, &self.model, self.size_mode, &mut self.events);
            }
        }
    }

    fn toggle_size_mode(&mut self) {
        let (mode, text) = match self.size_mode {
            SizeMode::Disk => (SizeMode::Apparent, crate::ui::copy::SHOWING_APPARENT),
            SizeMode::Apparent => (SizeMode::Disk, crate::ui::copy::SHOWING_DISK),
        };
        self.size_mode = mode;
        let now = self.now;
        self.set_transient(text, now);
    }

    /// From the `..` row, `Tab` moves to the map and picks its largest tile.
    fn select_largest_tile(&mut self) {
        // The map shows the view root's children, and its largest tile is the largest child.
        let largest = self
            .model
            .children_sorted(self.tree.view_root, self.size_mode)
            .into_iter()
            .next();
        if let Some(id) = largest {
            self.tree.selection = id;
            self.tree.aggregate = None;
            self.tree.on_parent = false;
            self.map.on_aggregate = false;
        }
    }

    fn list_key(&mut self, action: Action) {
        let height = self.list_height;
        if action == Action::Enter {
            if let Some(id) = self.lists.selected_node(self.view) {
                self.view = View::Tree;
                self.tree.reveal(&self.model, id, &mut self.events);
            }
            return;
        }
        self.lists
            .apply(self.view, action, &self.model, self.size_mode, height);
    }

    fn map_key(&mut self, action: Action) {
        let dir = match action {
            Action::Up => Direction::Up,
            Action::Down => Direction::Down,
            Action::Left => Direction::Left,
            Action::Right => Direction::Right,
            Action::Enter | Action::ZoomOut => {
                if !self.map.on_aggregate && self.tree.aggregate.is_none() {
                    self.tree
                        .apply(action, &self.model, self.size_mode, &mut self.events);
                }
                return;
            }
            _ => return,
        };
        let Some(tile) = self.map.step(&self.tree, dir) else {
            return;
        };
        match tile.id {
            Some(id) => {
                self.tree.selection = id;
                self.tree.aggregate = None;
                self.map.on_aggregate = false;
            }
            None => self.map.on_aggregate = true,
        }
    }

    /// Brings the tree's rows, the map's layout and the viewports up to date; call before drawing.
    pub fn prepare_view(&mut self, screen: Rect) {
        self.screen = screen;
        let regions = crate::ui::regions(screen);
        self.tree.prepare(
            &self.model,
            self.size_mode,
            usize::from(regions.main.height),
        );
        self.list_height = usize::from(regions.main.height);
        self.lists
            .prepare(self.view, &self.model, self.size_mode, self.list_height);
        self.map.prepare(
            &self.model,
            &self.tree,
            self.size_mode,
            self.config.cell_aspect,
            regions.map,
            self.now,
            self.is_scanning(),
            &self.theme,
            &mut self.tile_colors,
        );
    }
}
