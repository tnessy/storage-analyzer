//! `AppState`: everything the UI shows besides the model's own data. Main thread only.

use std::time::Duration;

use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use super::cli::AppConfig;
use super::keymap::{self, Action};
use super::list_state::{ListState, View};
use super::map_state::{MapState, Pane};
use super::tree_state::{AppEvent, TreeState};
use crate::layout::Direction;
use crate::model::Model;
use crate::types::NodeId;
use crate::types::{ScanState, SizeMode};
use crate::ui::theme::{Glyphs, Theme};
use crate::ui::tile_colors::TileColors;

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
    /// The loop clock at the last `tick`.
    pub now: Duration,
    /// The folder whose scan is prioritised (set by T020).
    pub priority: Option<NodeId>,
    /// Events for other parts of the app; drained by the loop.
    pub events: Vec<AppEvent>,
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
            now: Duration::ZERO,
            priority: None,
            events: Vec::new(),
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
        self.now = now;
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
            (Some(Dialog::Quit), Action::No | Action::Esc | Action::Enter) => self.dialog = None,
            (Some(Dialog::ScanError(_)), _) => self.dialog = None,
            (Some(Dialog::Quit), _) => {}
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
            (None, action) if self.focus == Pane::Map => self.map_key(action),
            (None, action) if self.view != View::Tree => self.list_key(action),
            (None, action) => {
                self.map.on_aggregate = false;
                self.tree
                    .apply(action, &self.model, self.size_mode, &mut self.events);
            }
        }
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
            if let Some(id) = self.lists.selected_node(self.view, &self.model) {
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
