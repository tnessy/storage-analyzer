//! Delete flow: the confirmation, the run, the result and quitting during a delete (T021).
//!
//! The model decides what may be deleted (`delete_refusal`, `begin_delete`, `apply_delete`); the
//! executor does it; this module turns keys and executor events into dialogs and messages.

use std::path::PathBuf;
use std::time::Duration;

use super::keymap::Action;
use super::list_state::View;
use super::map_state::Pane;
use super::state::{AppState, Dialog};
use super::tree_state::Row;
use crate::delete::{
    AbortReason, DeleteEvent, DeleteFailKind, DeleteFailure, DeleteRequest, Outcome,
};
use crate::model::{DeleteCheck, Reconcile, Refusal};
use crate::types::{Counts, NodeId};
use crate::ui::{copy, format, sanitize};

/// The open confirmation re-reads its numbers at most this often.
const PROMPT_REFRESH: Duration = Duration::from_millis(500);
/// `EBUSY`, shown if the executor cannot be reached (it is always there in the real app).
const EBUSY: i32 = 16;

/// The delete confirmation. Its numbers are refreshed while it is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeletePrompt {
    pub target: NodeId,
    pub check: DeleteCheck,
    pub checked_at: Duration,
    /// `p` was pressed: the scan of the target was moved to the front.
    pub prioritising: bool,
}

/// A delete that is running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteRun {
    pub target: NodeId,
    pub path: PathBuf,
    /// Known when the target was exact.
    pub total_files: Option<u64>,
    /// Files the confirmation counted (a lower bound when it was not exact).
    pub counted_files: u64,
    pub removed_files: u64,
    pub removed_dirs: u64,
}

/// The `Delete finished with errors` dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteDone {
    pub path: PathBuf,
    pub removed: Counts,
    /// Failure counts by kind, largest group first.
    pub groups: Vec<(DeleteFailKind, usize)>,
    /// The executor's panic message, if it panicked.
    pub panic_note: Option<String>,
}

/// The `Not deleted` dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteAborted {
    pub path: PathBuf,
    pub reason: AbortReason,
}

impl AppState {
    /// `d`: opens the confirmation, or says why not.
    pub fn press_delete(&mut self) {
        let Some(id) = self.delete_target() else {
            return;
        };
        if let Some(refusal) = self.model.delete_refusal(id) {
            let text = self.refusal_text(id, refusal);
            return self.say(text);
        }
        let check = self.model.delete_check(id, self.size_mode);
        self.dialog = Some(Dialog::Delete(DeletePrompt {
            target: id,
            check,
            checked_at: self.now,
            prioritising: false,
        }));
    }

    /// The row `d` acts on. A group row says what it is and returns `None`.
    fn delete_target(&mut self) -> Option<NodeId> {
        if self.view == View::Largest && self.focus == Pane::Tree {
            let selected = self.lists.selected_node(View::Largest);
            if selected.is_none() {
                self.say(copy::NOTHING_TO_DELETE);
            }
            return selected;
        }
        if self.tree.aggregate.is_some() || self.map.on_aggregate {
            let count = self.group_size();
            self.say(copy::delete_group(count));
            return None;
        }
        let id = self.tree.selection;
        self.model.exists(id).then_some(id)
    }

    fn group_size(&self) -> usize {
        if self.map.on_aggregate {
            return self.map.hidden().len();
        }
        self.tree
            .rows()
            .iter()
            .find_map(|row| match *row {
                Row::Aggregate { dir, count, .. } if Some(dir) == self.tree.aggregate => {
                    Some(count)
                }
                _ => None,
            })
            .unwrap_or(0)
    }

    fn refusal_text(&self, target: NodeId, refusal: Refusal) -> String {
        let path = |id: NodeId| sanitize::sanitize(&self.model.path(id).display().to_string());
        match refusal {
            Refusal::ScanRoot => copy::DELETE_SCAN_ROOT.to_owned(),
            Refusal::OtherFilesystem => copy::delete_other_fs(&path(target)),
            Refusal::AlreadyDeleting => copy::DELETE_ALREADY.to_owned(),
            Refusal::DeleteRunning(id) => copy::delete_running(&path(id)),
        }
    }

    /// Keys while the confirmation is open: only `y` deletes; `p` asks for the scan first.
    pub fn delete_prompt_key(&mut self, action: Action) {
        let Some(Dialog::Delete(prompt)) = &mut self.dialog else {
            return;
        };
        match action {
            Action::Yes => {
                let target = prompt.target;
                self.dialog = None;
                self.start_delete(target);
            }
            Action::No | Action::Esc | Action::Enter => self.dialog = None,
            Action::Prioritise if !prompt.check.exact && !prompt.prioritising => {
                prompt.prioritising = true;
                let target = prompt.target;
                self.set_priority(target);
            }
            _ => {}
        }
    }

    /// The confirmed delete: stop scanning the folder, tombstone it, hand it to the executor.
    fn start_delete(&mut self, target: NodeId) {
        let path = self.model.path(target);
        let counted = self.model.delete_check(target, self.size_mode).files;
        let request = match self.model.begin_delete(target) {
            Ok(request) => request,
            Err(refusal) => {
                let text = self.refusal_text(target, refusal);
                return self.say(text);
            }
        };
        self.control.cancel_prefix(path.clone());
        self.delete_run = Some(DeleteRun {
            target,
            path,
            total_files: request.total_files,
            counted_files: counted,
            removed_files: 0,
            removed_dirs: 0,
        });
        if !self.submit_delete(request) {
            // Unexpected: the executor is missing or busy. Undo the tombstone with an abort.
            self.on_delete_event(DeleteEvent::Finished(Outcome::Aborted {
                reason: AbortReason::OpenFailed(EBUSY),
            }));
        }
    }

    fn submit_delete(&self, request: DeleteRequest) -> bool {
        self.delete_handle
            .as_ref()
            .is_some_and(|handle| handle.submit(request).is_ok())
    }

    /// Keys on a result dialog: `Enter` closes it, `v` opens the failure list.
    pub fn delete_result_key(&mut self, action: Action) {
        match (&self.dialog, action) {
            (Some(Dialog::DeleteDone(_)), Action::ViewFailures) => {
                let back = self.dialog.take().map(Box::new);
                self.dialog = Some(Dialog::FailedPaths { scroll: 0, back });
            }
            (Some(Dialog::DeleteDone(_)), Action::Enter | Action::Esc) => {
                self.dialog = None;
                let n = self.last_delete_failures.len();
                self.say(copy::items_not_deleted(n));
            }
            (Some(Dialog::DeleteAborted(_)), Action::Enter | Action::Esc) => self.dialog = None,
            (_, Action::Quit) => self.dialog = Some(Dialog::Quit),
            _ => {}
        }
    }

    /// Keys in the failure list.
    pub fn failed_paths_key(&mut self, action: Action) {
        let page = crate::ui::result_dialog::page_rows(self.screen.height);
        let last =
            crate::ui::result_dialog::line_count(&self.last_delete_failures).saturating_sub(page);
        let Some(Dialog::FailedPaths { scroll, back }) = &mut self.dialog else {
            return;
        };
        match action {
            Action::Esc | Action::ViewFailures => {
                self.dialog = back.take().map(|b| *b);
            }
            Action::Quit => self.dialog = Some(Dialog::Quit),
            Action::Up => *scroll = scroll.saturating_sub(1),
            Action::Down => *scroll = (*scroll + 1).min(last),
            Action::PageUp => *scroll = scroll.saturating_sub(page),
            Action::PageDown => *scroll = (*scroll + page).min(last),
            Action::Top => *scroll = 0,
            Action::Bottom => *scroll = last,
            _ => {}
        }
    }

    /// `q` during a delete: only `y` quits; anything else keeps deleting.
    pub fn quit_while_deleting_key(&mut self, action: Action) {
        if action == Action::Yes {
            self.quit = true;
        } else {
            self.dialog = None;
        }
    }

    /// Re-reads the open confirmation (at most twice a second); true if anything changed.
    /// An unfinished folder turns into the exact dialog by itself once its scan is done.
    pub fn refresh_delete_prompt(&mut self) -> bool {
        let Some(Dialog::Delete(prompt)) = &self.dialog else {
            return false;
        };
        if self.now.saturating_sub(prompt.checked_at) < PROMPT_REFRESH {
            return false;
        }
        let target = prompt.target;
        if let Some(refusal) = self.model.delete_refusal(target) {
            self.dialog = None;
            let text = self.refusal_text(target, refusal);
            self.say(text);
            return true;
        }
        let check = self.model.delete_check(target, self.size_mode);
        let now = self.now;
        let Some(Dialog::Delete(prompt)) = &mut self.dialog else {
            return false;
        };
        prompt.checked_at = now;
        let changed = prompt.check != check;
        prompt.check = check;
        changed
    }

    /// An executor event: progress, or the end of the delete.
    pub fn on_delete_event(&mut self, event: DeleteEvent) {
        match &event {
            DeleteEvent::Progress {
                removed_files,
                removed_dirs,
                ..
            } => {
                if let Some(run) = &mut self.delete_run {
                    run.removed_files = *removed_files;
                    run.removed_dirs = *removed_dirs;
                }
            }
            DeleteEvent::Finished(outcome) => {
                // Show the outcome first; removing a big subtree from the model takes a while,
                // so it waits for the next loop step. Until then the model still marks the
                // target as being deleted, so `d` keeps getting the one-at-a-time refusal.
                let run = self.delete_run.take();
                self.show_outcome(run, outcome);
                self.pending_delete = Some(event);
            }
        }
    }

    /// Applies the finished delete to the model (called at the start of the next loop step).
    pub fn apply_pending_delete(&mut self) {
        let Some(event) = self.pending_delete.take() else {
            return;
        };
        let reconcile = self.model.apply_delete(&event);
        if self.priority.is_some_and(|p| !self.model.exists(p)) {
            self.clear_priority();
        }
        if let Reconcile::Rescan(id) = reconcile {
            self.rescan(id);
        }
    }

    fn show_outcome(&mut self, run: Option<DeleteRun>, outcome: &Outcome) {
        let path = run.as_ref().map_or_else(PathBuf::new, |r| r.path.clone());
        self.last_delete_path.clone_from(&path);
        self.last_delete_failures.clear();
        match outcome {
            Outcome::Removed(counts) => {
                let more = run
                    .as_ref()
                    .is_some_and(|r| r.total_files.is_none() && counts.files > r.counted_files);
                let text = copy::deleted(
                    &sanitize::sanitize(&path.display().to_string()),
                    &format::size_iec_long(counts.bytes),
                    counts.files,
                    more,
                );
                self.say(text);
            }
            Outcome::Partial { removed, failures } => {
                self.last_delete_failures.clone_from(failures);
                self.dialog = Some(Dialog::DeleteDone(DeleteDone {
                    path,
                    removed: *removed,
                    groups: group_failures(failures),
                    panic_note: self.delete_panic.take(),
                }));
            }
            Outcome::Aborted { reason } => {
                self.dialog = Some(Dialog::DeleteAborted(DeleteAborted {
                    path,
                    reason: *reason,
                }));
            }
        }
    }

    /// `v`: the failed paths of the last delete.
    pub fn press_view_failures(&mut self) {
        if self.last_delete_failures.is_empty() {
            return self.say(copy::NO_FAILURES);
        }
        self.dialog = Some(Dialog::FailedPaths {
            scroll: 0,
            back: None,
        });
    }

    /// The failures of the last finished delete, replaced by the next one.
    pub fn failures(&self) -> &[DeleteFailure] {
        &self.last_delete_failures
    }
}

/// Failure counts by kind, the largest group first.
pub fn group_failures(failures: &[DeleteFailure]) -> Vec<(DeleteFailKind, usize)> {
    let mut groups: Vec<(DeleteFailKind, usize)> = Vec::new();
    for failure in failures {
        match groups.iter_mut().find(|(kind, _)| *kind == failure.kind) {
            Some((_, n)) => *n += 1,
            None => groups.push((failure.kind, 1)),
        }
    }
    groups.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    groups
}
