//! State of the Types, Largest and Skipped views (T018).

use std::collections::HashSet;

use super::keymap::Action;
use crate::model::{Category, ExtKey, Model};
use crate::types::{NodeId, SizeMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Tree,
    Types,
    Largest,
    Skipped,
}

/// Identity of a list row, so the cursor follows its item while the list changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKey {
    Category(Category),
    Ext(Category, ExtKey),
    Node(NodeId),
    /// The node an issue belongs to. A node has at most one issue, and positions in
    /// `Model::issues()` shift when a rescan removes one, so rows are keyed by node.
    Issue(NodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypesRow {
    pub key: RowKey,
    pub bytes: u64,
    pub files: u64,
}

/// A selected row plus the scroll position that keeps it on the same screen line.
#[derive(Debug, Default)]
pub struct Cursor {
    selected: Option<RowKey>,
    index: usize,
    top: usize,
    line: usize,
    page: usize,
}

impl Cursor {
    pub fn top(&self) -> usize {
        self.top
    }

    pub fn selected(&self) -> Option<&RowKey> {
        self.selected.as_ref()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// Re-finds the selected row in the current list and fixes the viewport.
    pub fn prepare(
        &mut self,
        len: usize,
        find: &dyn Fn(&RowKey) -> Option<usize>,
        key_at: &dyn Fn(usize) -> RowKey,
        height: usize,
    ) {
        let height = height.max(1);
        self.page = height;
        if len == 0 {
            *self = Self {
                page: height,
                ..Self::default()
            };
            return;
        }
        let idx = match self.selected.as_ref().and_then(find) {
            Some(i) => i,
            None => {
                let i = self.index.min(len - 1);
                self.selected = Some(key_at(i));
                i
            }
        };
        let max_top = len.saturating_sub(height);
        let mut top = idx.saturating_sub(self.line.min(height - 1)).min(max_top);
        if idx < top {
            top = idx;
        } else if idx >= top + height {
            top = idx + 1 - height;
        }
        self.index = idx;
        self.line = idx - top;
        self.top = top;
    }

    /// Moves the cursor for the movement actions.
    pub fn apply(
        &mut self,
        action: Action,
        len: usize,
        find: &dyn Fn(&RowKey) -> Option<usize>,
        key_at: &dyn Fn(usize) -> RowKey,
    ) {
        self.prepare(len, find, key_at, self.page);
        if len == 0 {
            return;
        }
        let step = self.page.saturating_sub(1).max(1);
        let from = self.index;
        let to = match action {
            Action::Up => from.saturating_sub(1),
            Action::Down => (from + 1).min(len - 1),
            Action::PageUp => from.saturating_sub(step),
            Action::PageDown => (from + step).min(len - 1),
            Action::Top => 0,
            Action::Bottom => len - 1,
            _ => return,
        };
        self.selected = Some(key_at(to));
        self.index = to;
        let moved = to as isize - from as isize;
        self.line = (self.line as isize + moved).clamp(0, self.page as isize - 1) as usize;
    }
}

#[derive(Debug, Default)]
pub struct ListState {
    /// Categories expanded into their extensions in the Types view.
    pub types_expanded: HashSet<Category>,
    types_rows: Vec<TypesRow>,
    types_cursor: Cursor,
    largest: Vec<NodeId>,
    largest_stamp: Option<(u64, SizeMode, u64)>,
    largest_cursor: Cursor,
    skipped_len: usize,
    skipped_cursor: Cursor,
}

/// The row (newest first) of the issue on `key`'s node, if it still has one.
fn issue_row(issues: &[crate::types::ScanIssue], key: &RowKey) -> Option<usize> {
    let RowKey::Issue(node) = key else {
        return None;
    };
    issues
        .iter()
        .rposition(|i| i.node == *node)
        .map(|at| issues.len() - 1 - at)
}

fn sized(mode: SizeMode, t: &crate::model::ExtTotals) -> u64 {
    match mode {
        SizeMode::Disk => t.disk_bytes,
        SizeMode::Apparent => t.apparent_bytes,
    }
}

impl ListState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn types_rows(&self) -> &[TypesRow] {
        &self.types_rows
    }

    pub fn largest(&self) -> &[NodeId] {
        &self.largest
    }

    pub fn skipped_len(&self) -> usize {
        self.skipped_len
    }

    pub fn cursor(&self, view: View) -> Option<&Cursor> {
        match view {
            View::Tree => None,
            View::Types => Some(&self.types_cursor),
            View::Largest => Some(&self.largest_cursor),
            View::Skipped => Some(&self.skipped_cursor),
        }
    }

    fn build_types(&mut self, model: &Model, mode: SizeMode) {
        let mut categories: Vec<(Category, u64, u64)> = model
            .category_totals()
            .into_iter()
            .map(|(c, t)| (c, sized(mode, &t), t.count))
            .collect();
        categories.sort_by_key(|&(c, bytes, _)| (std::cmp::Reverse(bytes), c));
        let exts = model.ext_totals();
        let mut rows = Vec::new();
        for (category, bytes, files) in categories {
            rows.push(TypesRow {
                key: RowKey::Category(category),
                bytes,
                files,
            });
            if self.types_expanded.contains(&category) {
                let mut own: Vec<_> = exts
                    .iter()
                    .filter(|(k, _)| Model::category_of(k) == category)
                    .map(|(k, t)| (k.clone(), sized(mode, t), t.count))
                    .collect();
                own.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                rows.extend(own.into_iter().map(|(k, bytes, files)| TypesRow {
                    key: RowKey::Ext(category, k),
                    bytes,
                    files,
                }));
            }
        }
        self.types_rows = rows;
    }

    /// Brings the active list's rows and viewport up to date. `height` is the pane's rows.
    pub fn prepare(&mut self, view: View, model: &Model, mode: SizeMode, height: usize) {
        match view {
            View::Tree => {}
            View::Types => {
                self.build_types(model, mode);
                let rows = &self.types_rows;
                self.types_cursor.prepare(
                    rows.len(),
                    &|k| rows.iter().position(|r| &r.key == k),
                    &|i| rows[i].key.clone(),
                    height,
                );
            }
            View::Largest => {
                // While a rebuild is pending the list is the old one minus deleted files, which
                // changes without any new entry, so it is re-read every time (it is short).
                let stamp = (model.totals().entries_seen, mode, model.top_epoch());
                if model.top_updating() || self.largest_stamp != Some(stamp) {
                    self.largest = model.top_files(mode);
                    self.largest_stamp = (!model.top_updating()).then_some(stamp);
                }
                let list = &self.largest;
                self.largest_cursor.prepare(
                    list.len(),
                    &|k| match k {
                        RowKey::Node(id) => list.iter().position(|n| n == id),
                        _ => None,
                    },
                    &|i| RowKey::Node(list[i]),
                    height,
                );
            }
            View::Skipped => {
                let issues = model.issues();
                let len = issues.len();
                self.skipped_len = len;
                self.skipped_cursor.prepare(
                    len,
                    &|k| issue_row(issues, k),
                    &|i| RowKey::Issue(issues[len - 1 - i].node),
                    height,
                );
            }
        }
    }

    /// Movement and expansion keys for the active list.
    pub fn apply(
        &mut self,
        view: View,
        action: Action,
        model: &Model,
        mode: SizeMode,
        height: usize,
    ) {
        self.prepare(view, model, mode, height);
        match view {
            View::Tree => {}
            View::Types => self.apply_types(action),
            View::Largest => {
                let list = &self.largest;
                self.largest_cursor.apply(
                    action,
                    list.len(),
                    &|k| match k {
                        RowKey::Node(id) => list.iter().position(|n| n == id),
                        _ => None,
                    },
                    &|i| RowKey::Node(list[i]),
                );
            }
            View::Skipped => {
                let issues = model.issues();
                let len = issues.len();
                self.skipped_cursor
                    .apply(action, len, &|k| issue_row(issues, k), &|i| {
                        RowKey::Issue(issues[len - 1 - i].node)
                    });
            }
        }
    }

    fn apply_types(&mut self, action: Action) {
        let rows = &self.types_rows;
        let find = |k: &RowKey| rows.iter().position(|r| &r.key == k);
        let key_at = |i: usize| rows[i].key.clone();
        match action {
            Action::Right => {
                if let Some(RowKey::Category(c)) = self.types_cursor.selected().cloned() {
                    self.types_expanded.insert(c);
                }
            }
            Action::Left => match self.types_cursor.selected().cloned() {
                Some(RowKey::Category(c)) => {
                    self.types_expanded.remove(&c);
                }
                Some(RowKey::Ext(c, _)) => {
                    // Back to the category row above.
                    let at = self.types_cursor.index();
                    if let Some(up) = (0..at).rev().find(|&i| rows[i].key == RowKey::Category(c)) {
                        let moved = at - up;
                        for _ in 0..moved {
                            self.types_cursor
                                .apply(Action::Up, rows.len(), &find, &key_at);
                        }
                    }
                }
                _ => {}
            },
            other => self.types_cursor.apply(other, rows.len(), &find, &key_at),
        }
    }

    /// The node behind the selected Largest or Skipped row.
    pub fn selected_node(&self, view: View) -> Option<NodeId> {
        match (view, self.cursor(view)?.selected()?) {
            (View::Largest, RowKey::Node(id)) => Some(*id),
            (View::Skipped, RowKey::Issue(node)) => Some(*node),
            _ => None,
        }
    }
}
