//! Tree view state: selection, zoom, expansion and the visible row list (T016).

use std::collections::{HashMap, HashSet};

use crate::model::Model;
use crate::types::{Kind, NodeId, SizeMode};

use super::keymap::Action;

/// Children shown per folder before the `(N smaller, SIZE)` row; `→` on it reveals this many more.
pub const AGGREGATE_PAGE: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The `..` row, only when zoomed: pinned above the view root, never sorted or aggregated.
    Parent,
    Node {
        id: NodeId,
        depth: u16,
        /// Size of the row's parent in the active mode (the row itself for the view root).
        parent_size: u64,
        /// The last child of its parent (drawn with `└`).
        last: bool,
        /// Bit `k` is set when the ancestor at depth `k` has later siblings (drawn as `│`).
        trail: u128,
    },
    Aggregate {
        dir: NodeId,
        depth: u16,
        count: usize,
        bytes: u64,
        trail: u128,
    },
}

/// Events for other parts of the app (T020 reacts to zoom changes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppEvent {
    ZoomChanged { view_root: NodeId },
}

#[derive(Debug)]
pub struct TreeState {
    pub selection: NodeId,
    /// `Some(dir)` when the cursor is on `dir`'s aggregate row; `selection` is then `dir`.
    pub aggregate: Option<NodeId>,
    pub view_root: NodeId,
    /// The cursor is on the `..` row (the selection stays on the view root).
    pub on_parent: bool,
    pub expanded: HashSet<NodeId>,
    /// Extra pages of children revealed per folder; forgotten when the folder is collapsed.
    revealed: HashMap<NodeId, usize>,
    /// Screen line of the cursor, kept while the list changes underneath it.
    line: usize,
    top: usize,
    page: usize,
    rows: Vec<Row>,
    /// The selection's ancestors, nearest first, as of the last `prepare`.
    ancestors: Vec<NodeId>,
}

impl TreeState {
    pub fn new() -> Self {
        Self {
            selection: NodeId::ROOT,
            aggregate: None,
            view_root: NodeId::ROOT,
            on_parent: false,
            expanded: HashSet::from([NodeId::ROOT]),
            revealed: HashMap::new(),
            line: 0,
            top: 0,
            page: 20,
            rows: Vec::new(),
            ancestors: Vec::new(),
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Index of the first visible row.
    pub fn top(&self) -> usize {
        self.top
    }

    pub fn is_cursor(&self, row: &Row) -> bool {
        match (*row, self.aggregate) {
            (Row::Parent, _) => self.on_parent,
            (Row::Node { id, .. }, None) => !self.on_parent && id == self.selection,
            (Row::Aggregate { dir, .. }, Some(on)) => dir == on,
            _ => false,
        }
    }

    fn cursor_index(&self, rows: &[Row]) -> Option<usize> {
        rows.iter().position(|r| self.is_cursor(r))
    }

    fn ancestors_of(model: &Model, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = model.parent(id);
        while let Some(p) = cur {
            out.push(p);
            cur = model.parent(p);
        }
        out
    }

    /// The visible rows for the current selection, zoom and expansion.
    fn build_rows(&self, model: &Model, mode: SizeMode) -> Vec<Row> {
        let mut on_path: HashSet<NodeId> = HashSet::new();
        if model.exists(self.selection) {
            on_path.insert(self.selection);
            on_path.extend(Self::ancestors_of(model, self.selection));
        }
        let mut out = Vec::new();
        if self.view_root != NodeId::ROOT {
            out.push(Row::Parent);
        }
        out.push(Row::Node {
            id: self.view_root,
            depth: 0,
            parent_size: model.size(self.view_root, mode),
            last: true,
            trail: 0,
        });
        self.push_children(model, mode, self.view_root, 1, 0, &on_path, &mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn push_children(
        &self,
        model: &Model,
        mode: SizeMode,
        dir: NodeId,
        depth: u16,
        trail: u128,
        on_path: &HashSet<NodeId>,
        out: &mut Vec<Row>,
    ) {
        if !self.expanded.contains(&dir) || model.kind(dir) != Kind::Dir {
            return;
        }
        let parent_size = model.size(dir, mode);
        let sorted = model.children_sorted(dir, mode);
        let pages = 1 + self.revealed.get(&dir).copied().unwrap_or(0);
        let limit = AGGREGATE_PAGE * pages;
        let (shown, hidden) = sorted.split_at(sorted.len().min(limit));
        // The selected item (or the folder holding it) never disappears into the aggregate.
        let (pinned, folded): (Vec<NodeId>, Vec<NodeId>) =
            hidden.iter().partition(|c| on_path.contains(c));
        let nodes: Vec<NodeId> = shown.iter().chain(&pinned).copied().collect();
        let has_aggregate = !folded.is_empty();
        for (i, &child) in nodes.iter().enumerate() {
            let last = i + 1 == nodes.len() && !has_aggregate;
            out.push(Row::Node {
                id: child,
                depth,
                parent_size,
                last,
                trail,
            });
            let below = if last {
                trail
            } else {
                trail | (1u128 << depth.min(127))
            };
            self.push_children(model, mode, child, depth + 1, below, on_path, out);
        }
        if has_aggregate {
            out.push(Row::Aggregate {
                dir,
                depth,
                count: folded.len(),
                bytes: folded.iter().map(|&c| model.size(c, mode)).sum(),
                trail,
            });
        }
    }

    fn set_cursor(&mut self, row: Row) {
        self.on_parent = matches!(row, Row::Parent);
        match row {
            Row::Parent => self.aggregate = None,
            Row::Node { id, .. } => {
                self.selection = id;
                self.aggregate = None;
            }
            Row::Aggregate { dir, .. } => {
                self.selection = dir;
                self.aggregate = Some(dir);
            }
        }
    }

    /// Brings rows and viewport up to date. Call before drawing, with the tree pane's height.
    pub fn prepare(&mut self, model: &Model, mode: SizeMode, height: usize) {
        let height = height.max(1);
        self.page = height;
        self.on_parent &= self.view_root != NodeId::ROOT;
        self.restore_selection(model);
        let mut rows = self.build_rows(model, mode);
        let idx = match self.cursor_index(&rows) {
            Some(i) => i,
            None => {
                self.recover_cursor(&rows);
                rows = self.build_rows(model, mode);
                self.cursor_index(&rows).unwrap_or(0)
            }
        };
        self.ancestors = Self::ancestors_of(model, self.selection);

        let max_top = rows.len().saturating_sub(height);
        let mut top = idx.saturating_sub(self.line.min(height - 1)).min(max_top);
        if idx < top {
            top = idx;
        } else if idx >= top + height {
            top = idx + 1 - height;
        }
        self.line = idx - top;
        self.top = top;
        self.rows = rows;
    }

    /// A removed selection moves to the nearest surviving ancestor.
    fn restore_selection(&mut self, model: &Model) {
        if model.exists(self.selection) {
            return;
        }
        self.selection = self
            .ancestors
            .iter()
            .copied()
            .find(|&a| model.exists(a))
            .unwrap_or(self.view_root);
        self.aggregate = None;
    }

    /// The cursor's row is hidden (collapsed ancestor, finished aggregate): use the nearest row above.
    fn recover_cursor(&mut self, rows: &[Row]) {
        if let Some(dir) = self.aggregate.take() {
            self.selection = dir;
        }
        let visible: HashSet<NodeId> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Node { id, .. } => Some(*id),
                Row::Parent | Row::Aggregate { .. } => None,
            })
            .collect();
        if !visible.contains(&self.selection) {
            self.selection = self
                .ancestors
                .iter()
                .copied()
                .find(|a| visible.contains(a))
                .unwrap_or(self.view_root);
        }
    }

    pub fn apply(
        &mut self,
        action: Action,
        model: &Model,
        mode: SizeMode,
        events: &mut Vec<AppEvent>,
    ) {
        self.prepare(model, mode, self.page);
        let rows = self.rows.clone();
        let Some(idx) = self.cursor_index(&rows) else {
            return;
        };
        let last = rows.len() - 1;
        let step = self.page.saturating_sub(1).max(1);
        match action {
            Action::Up => self.move_to(&rows, idx, idx.saturating_sub(1)),
            Action::Down => self.move_to(&rows, idx, (idx + 1).min(last)),
            Action::PageUp => self.move_to(&rows, idx, idx.saturating_sub(step)),
            Action::PageDown => self.move_to(&rows, idx, (idx + step).min(last)),
            Action::Top => self.move_to(&rows, idx, 0),
            Action::Bottom => self.move_to(&rows, idx, last),
            Action::Right => self.right(model, &rows, idx),
            Action::Left if matches!(rows[idx], Row::Parent) => self.zoom_out(model, events),
            Action::Left => self.left(model, &rows, idx),
            Action::Enter if matches!(rows[idx], Row::Parent) => self.zoom_out(model, events),
            Action::Enter => {
                if let Row::Node { id, .. } = rows[idx]
                    && id != self.view_root
                    && model.kind(id) == Kind::Dir
                {
                    self.view_root = id;
                    self.expanded.insert(id);
                    // The `..` row is now row 0 and the view root row is row 1.
                    self.line = 1;
                    events.push(AppEvent::ZoomChanged { view_root: id });
                }
            }
            Action::ZoomOut => self.zoom_out(model, events),
            _ => {}
        }
    }

    /// One level up; the selection becomes the folder we came from.
    pub fn zoom_out(&mut self, model: &Model, events: &mut Vec<AppEvent>) {
        if let Some(parent) = model.parent(self.view_root) {
            self.selection = self.view_root;
            self.aggregate = None;
            self.on_parent = false;
            self.view_root = parent;
            events.push(AppEvent::ZoomChanged { view_root: parent });
        }
    }

    /// Makes `id` visible and selected: back to the scan root, ancestors expanded.
    pub fn reveal(&mut self, model: &Model, id: NodeId, events: &mut Vec<AppEvent>) {
        if self.view_root != NodeId::ROOT {
            self.view_root = NodeId::ROOT;
            events.push(AppEvent::ZoomChanged {
                view_root: NodeId::ROOT,
            });
        }
        self.expanded.extend(Self::ancestors_of(model, id));
        self.selection = id;
        self.aggregate = None;
        self.on_parent = false;
    }

    fn move_to(&mut self, rows: &[Row], from: usize, to: usize) {
        self.set_cursor(rows[to]);
        let moved = to as isize - from as isize;
        self.line = (self.line as isize + moved).clamp(0, self.page as isize - 1) as usize;
    }

    fn right(&mut self, model: &Model, rows: &[Row], idx: usize) {
        match rows[idx] {
            Row::Parent => {}
            Row::Aggregate { dir, .. } => *self.revealed.entry(dir).or_insert(0) += 1,
            Row::Node { id, depth, .. } => {
                if model.kind(id) != Kind::Dir {
                    return;
                }
                if self.expanded.insert(id) {
                    return;
                }
                if let Some(&next) = rows.get(idx + 1)
                    && matches!(next, Row::Node { depth: d, .. } | Row::Aggregate { depth: d, .. } if d == depth + 1)
                {
                    self.move_to(rows, idx, idx + 1);
                }
            }
        }
    }

    fn left(&mut self, model: &Model, rows: &[Row], idx: usize) {
        match rows[idx] {
            Row::Parent => {}
            Row::Aggregate { dir, .. } => {
                self.selection = dir;
                self.aggregate = None;
            }
            Row::Node { id, .. } => {
                if id == self.view_root {
                    return;
                }
                if self.expanded.contains(&id) {
                    self.expanded.remove(&id);
                    self.revealed.remove(&id);
                } else if let Some(parent) = model.parent(id)
                    && let Some(at) = rows
                        .iter()
                        .position(|r| matches!(r, Row::Node { id: p, .. } if *p == parent))
                {
                    self.move_to(rows, idx, at);
                }
            }
        }
    }
}

impl Default for TreeState {
    fn default() -> Self {
        Self::new()
    }
}
