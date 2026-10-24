//! Scan-control keys: priority, skip, resume/rescan and stop (T020).
//!
//! The model owns what is true about the scan; these functions turn a key press into the calls
//! that change it (`ScanControl`, `Model`, `ScanHandle`) and into the message the user sees.

use super::list_state::View;
use super::state::{AppState, ScanKind};
use crate::model::RescanPlan;
use crate::types::{IssueKind, Kind, NodeId, ScanState};
use crate::ui::{copy, format, sanitize};

impl AppState {
    /// The scan as the user sees it: a rescan in flight counts as scanning even though the
    /// model's own state stays `Done` or `Stopped` until the new `Done` arrives.
    pub fn phase(&self) -> ScanState {
        if self.rescan_in_flight {
            ScanState::Scanning
        } else {
            self.model.state()
        }
    }

    /// Between `s` and the `Done { cancelled }` that ends the scan: workers are still draining,
    /// so nothing new may be started (UX §5.2).
    pub fn is_stopping(&self) -> bool {
        self.control.is_cancel_all() && self.is_scanning()
    }

    pub(super) fn say(&mut self, text: impl Into<String>) {
        let now = self.now;
        self.set_transient(text, now);
    }

    /// `name/` for a folder, as the rows show it.
    pub(super) fn folder_label(&self, id: NodeId) -> String {
        if id == self.model.root() {
            crate::ui::root_label(self)
        } else {
            format!("{}/", sanitize::display_name(self.model.name(id)))
        }
    }

    /// The folder `p`, `x` and `r` act on: the tree or map selection, if it is a folder row.
    fn folder_target(&self) -> Option<NodeId> {
        let id = self.tree.selection;
        let plain_row =
            self.tree.aggregate.is_none() && !self.map.on_aggregate && !self.tree.on_parent;
        (plain_row && self.model.exists(id) && self.model.kind(id) == Kind::Dir).then_some(id)
    }

    /// Sets (or, for a complete folder, ignores) the scan priority without a message.
    pub(super) fn set_priority(&mut self, id: NodeId) {
        self.control.set_focus(Some(self.model.path(id)));
        self.model.set_focus(Some(id));
        self.priority = (!self.model.is_complete(id)).then_some(id);
    }

    pub(super) fn clear_priority(&mut self) {
        self.control.set_focus(None);
        self.model.set_focus(None);
        self.priority = None;
    }

    /// Reacts to the tree's events: zooming into a folder prioritises it, going back to the
    /// scan root clears the priority (M11, UX F2).
    pub fn process_events(&mut self) {
        let events = std::mem::take(&mut self.events);
        for event in events {
            let super::tree_state::AppEvent::ZoomChanged { view_root } = event;
            if view_root == self.model.root() {
                self.clear_priority();
            } else {
                self.set_priority(view_root);
            }
        }
    }

    /// Once per loop iteration: announces a priority folder that has just been fully scanned.
    pub fn poll_controls(&mut self) {
        let Some(done) = self.model.take_focus_completed() else {
            return;
        };
        self.control.set_focus(None);
        self.priority = None;
        let (files, _dirs) = self.model.counts(done);
        let bytes = self.model.size(done, self.size_mode);
        let text = format!(
            "{}{} fully scanned: {} {} {} files",
            lead(self.glyphs.done),
            self.folder_label(done),
            format::size_iec_long(bytes),
            self.glyphs.separator,
            format::thousands(files)
        );
        self.say(text);
    }

    /// `p`: put the folder first, or take it off again.
    pub fn press_priority(&mut self) {
        if self.refuse_while_stopping() {
            return;
        }
        let Some(id) = self.folder_target() else {
            return self.say(copy::NOT_A_FOLDER);
        };
        if self.priority == Some(id) {
            self.clear_priority();
            return self.say(copy::PRIORITY_CLEARED);
        }
        if self.model.is_complete(id) {
            return self.say(copy::ALREADY_SCANNED);
        }
        self.set_priority(id);
        let text = format!(
            "{} Scanning {} first.",
            self.glyphs.prioritised,
            self.folder_label(id)
        );
        self.say(text);
    }

    /// `x`: stop scanning a folder; its size stays a lower bound.
    pub fn press_skip(&mut self) {
        if self.refuse_while_stopping() {
            return;
        }
        let Some(id) = self.folder_target() else {
            return self.say(copy::NOT_A_FOLDER);
        };
        if self
            .model
            .issue(id)
            .is_some_and(|i| i.kind == IssueKind::SkippedByUser)
        {
            return self.say(copy::ALREADY_SKIPPED);
        }
        if !self.model.mark_skipped(id) {
            return self.say(copy::ALREADY_SCANNED);
        }
        self.control.cancel_prefix(self.model.path(id));
        let text = format!(
            "{} Skipped {}. Its size is partial ({}). r resumes.",
            self.glyphs.skipped,
            self.folder_label(id),
            self.glyphs.lower_bound
        );
        self.say(text);
    }

    /// `r`: resume a skipped folder or rescan one; on a folder still being scanned it moves the
    /// folder to the front instead (plan O7).
    pub fn press_resume(&mut self) {
        if self.refuse_while_stopping() {
            return;
        }
        let target = if self.view == View::Skipped {
            self.lists.selected_node(View::Skipped)
        } else {
            self.folder_target()
        };
        let Some(id) = target.filter(|&n| self.model.exists(n)) else {
            return self.say(copy::NOT_A_FOLDER);
        };
        let label = self.folder_label(id);
        match self.model.rescan_plan(id) {
            RescanPlan::NoneWhileScanning => self.say(copy::SCAN_IN_PROGRESS),
            // T020b: message pending UX
            RescanPlan::OtherFilesystem => {}
            RescanPlan::Full => {
                self.rescan(id);
                let text = format!("{} Resuming {label}…", self.glyphs.scanning);
                self.say(text);
            }
            RescanPlan::PrioritiseAndResume { resume } => {
                let was_first = self.priority == Some(id);
                self.set_priority(id);
                for node in &resume {
                    self.rescan(*node);
                }
                let text = match (resume.len(), was_first) {
                    (0, true) => format!("Already scanning {label} first."),
                    (0, false) => format!(
                        "Still scanning {label}: moved it to the front instead of restarting."
                    ),
                    (1, _) => format!(
                        "Still scanning {label}: moved it to the front and resumed 1 skipped folder inside."
                    ),
                    (n, _) => format!(
                        "Still scanning {label}: moved it to the front and resumed {n} skipped folders inside."
                    ),
                };
                self.say(text);
            }
        }
    }

    /// `s`: stop the whole scan. The status shows `Stopping…` until the workers say `Done`.
    pub fn stop_scan(&mut self) {
        if !self.is_scanning() || self.is_stopping() {
            return;
        }
        self.control.cancel_all();
        self.stop_elapsed = Some(self.elapsed);
    }

    fn refuse_while_stopping(&mut self) -> bool {
        let stopping = self.is_stopping();
        if stopping {
            self.say(copy::STILL_STOPPING);
        }
        stopping
    }

    /// The rescan sequence (also used by delete reconciliation): forget the folder's contents,
    /// scan it again. While stopping it is queued until `Done { cancelled }` arrives.
    pub fn rescan(&mut self, id: NodeId) {
        if self.is_stopping() {
            if !self.queued_rescans.contains(&id) {
                self.queued_rescans.push(id);
            }
            return;
        }
        let was_scanning = self.is_scanning();
        let path = self.model.path(id);
        self.control.remove_prefixes_under(&path);
        self.tile_colors.forget_reserved(id);
        let (path, depth) = self.model.begin_rescan(id);
        if !was_scanning {
            self.begin_scan_timer(id == self.model.root());
        }
        if let Some(handle) = &self.scan_handle {
            handle.rescan(id, path, depth);
        }
    }

    /// A rescan of the root (or a subtree) starts a fresh clock, rate and ETA.
    fn begin_scan_timer(&mut self, root: bool) {
        self.rescan_in_flight = true;
        self.scan_kind = if root {
            ScanKind::Root
        } else {
            ScanKind::Subtree
        };
        self.started_at = self.now;
        self.finished_at = None;
        self.elapsed = std::time::Duration::ZERO;
        self.stop_elapsed = None;
        self.rate = crate::eta::Rate::new(super::state::RATE_TAU);
        self.seen_baseline = self.model.totals().entries_seen;
        if root {
            self.use_session_previous();
        }
    }

    /// Applies one scan message; a `Done` ends any rescan in flight and starts queued ones.
    pub fn apply_scan_msg(&mut self, msg: crate::scan::ScanMsg) {
        let done = matches!(msg, crate::scan::ScanMsg::Done { .. });
        self.model.apply(msg);
        if done {
            self.rescan_in_flight = false;
            for id in std::mem::take(&mut self.queued_rescans) {
                self.rescan(id);
            }
        }
    }
}

/// A leading symbol and its space; nothing when the glyph set has none.
fn lead(glyph: &str) -> String {
    if glyph.is_empty() {
        String::new()
    } else {
        format!("{glyph} ")
    }
}
