//! Skip, rescan, identity refresh, focus and rescan planning (T012, design §3.11).
//!
//! `remove_subtree` is the single place that takes nodes out of the model; the delete
//! reconciliation (T011) reuses it.

use std::path::PathBuf;

use super::hardlink::Removal;
use super::tree::NO_ISSUE;
use super::{Flags, Model, RescanPlan};
use crate::scan::RawEntry;
use crate::types::{IssueKind, Kind, NodeId, Op, ScanState};

/// Sums that leave the model when nodes are removed.
#[derive(Default)]
struct Removed {
    disk: u64,
    apparent: u64,
    files: u64,
    dirs: u64,
    pending: u32,
    inexact: u32,
}

impl Model {
    /// Marks an unfinished directory as skipped by the user. The app cancels the prefix in
    /// `ScanControl`; the walker then answers with `Cancelled`. `false` if nothing to skip.
    pub fn mark_skipped(&mut self, id: NodeId) -> bool {
        if !self.tree.exists(id) || self.kind(id) != Kind::Dir || self.is_complete(id) {
            return false;
        }
        if self.tree.issue[id.0 as usize] != NO_ISSUE {
            return self
                .issue(id)
                .is_some_and(|i| i.kind == IssueKind::SkippedByUser);
        }
        self.record_issue(id, Op::Read, IssueKind::SkippedByUser);
        true
    }

    /// Forgets everything below `id` and makes it a pending directory again (a file or
    /// symlink target just gets its issue cleared). Returns the path and depth for
    /// `ScanHandle::rescan`. The app also calls `ScanControl::remove_prefixes_under`.
    pub fn begin_rescan(&mut self, id: NodeId) -> (PathBuf, u32) {
        if !self.tree.exists(id) {
            return (self.root_path.clone(), 0);
        }
        // Another filesystem is never scanned (ADR-008), so there is nothing to redo.
        if self.flags(id).contains(Flags::OTHER_FS) {
            return (self.path(id), self.depth(id));
        }
        self.remove_subtree(id, true);
        let top = self.top_level_ancestor(id);
        // Taken before a never-counted top-level folder starts to count, so it enters
        // the progress as unfinished.
        let top_was_complete = top.is_some_and(|t| self.top_counted(t) && self.is_complete(t));
        if self.kind(id) == Kind::Dir && self.parent(id) == Some(NodeId::ROOT) {
            self.count_top_dir(id);
        }

        self.remove_issue(id);
        let i = id.0 as usize;
        self.tree.flags[i].remove(Flags::ERROR);
        if self.kind(id) == Kind::Dir {
            let (pending, inexact) = (self.tree.pending_dirs[i], self.tree.inexact_dirs[i]);
            let ancestors: Vec<NodeId> = self.tree.chain(id).skip(1).collect();
            for a in ancestors {
                let a = a.0 as usize;
                let with_new = self.tree.pending_dirs[a] + 1;
                debug_assert!(with_new >= pending);
                self.tree.pending_dirs[a] = with_new - pending;
                debug_assert!(self.tree.inexact_dirs[a] >= inexact);
                self.tree.inexact_dirs[a] -= inexact;
            }
            self.tree.pending_dirs[i] = 1;
            self.tree.inexact_dirs[i] = 0;
            let flags = &mut self.tree.flags[i];
            flags.remove(Flags::DONE);
            flags.remove(Flags::INCOMPLETE);
            flags.insert(Flags::PENDING);
        }
        self.rescan_roots.insert(id);
        self.adjust_top_done(top, top_was_complete);
        (self.path(id), self.depth(id))
    }

    /// Removes the nodes below `id` (and `id` itself unless `keep_root`), subtracting their
    /// sizes, counts, pending work, issues, extension totals and hard-link charges from the
    /// ancestors. Node ids are never reused; removed nodes only get the `REMOVED` flag.
    /// The root itself can only be emptied (`keep_root`).
    pub(super) fn remove_subtree(&mut self, id: NodeId, keep_root: bool) {
        if !self.tree.exists(id) || (!keep_root && id == NodeId::ROOT) {
            return;
        }
        let removing_top = !keep_root && self.parent(id) == Some(NodeId::ROOT);
        let top = if removing_top {
            None
        } else {
            self.top_level_ancestor(id)
        };
        let top_was_complete = top.is_some_and(|t| self.top_counted(t) && self.is_complete(t));
        let top_dir_was_complete = removing_top && self.is_complete(id);
        let top_dir_counted = removing_top && self.top_counted(id);

        let removed = self.removed_sums(id, keep_root);
        let nodes = self.collect_subtree(id, !keep_root);
        for &n in &nodes {
            self.tree.flags[n.0 as usize].insert(Flags::REMOVED);
        }
        for &n in &nodes {
            self.forget_node(n);
        }
        self.remove_issues_of(&nodes);
        if keep_root && id == NodeId::ROOT {
            // Nothing is left to rank; the new scan fills the lists again.
            self.indexes.reset_top();
        } else {
            let files: std::collections::HashSet<NodeId> = nodes
                .iter()
                .copied()
                .filter(|n| self.tree.kind[n.0 as usize] == Kind::File)
                .collect();
            self.indexes.forget_top(&files);
        }

        let ancestors: Vec<NodeId> = self.tree.chain(id).skip(usize::from(!keep_root)).collect();
        // With `keep_root`, `id` itself loses the descendants' sums too (but keeps its own size).
        for a in ancestors {
            let a = a.0 as usize;
            self.tree.agg_disk[a] -= removed.disk;
            self.tree.agg_apparent[a] -= removed.apparent;
            self.tree.files[a] -= removed.files;
            self.tree.dirs[a] -= removed.dirs;
            self.tree.pending_dirs[a] -= removed.pending;
            self.tree.inexact_dirs[a] -= removed.inexact;
        }
        if keep_root {
            self.tree.children[id.0 as usize].clear();
        } else if let Some(parent) = self.parent(id) {
            self.tree.children[parent.0 as usize].retain(|&c| c != id);
        }

        if keep_root && id == NodeId::ROOT {
            self.top_total = 0;
            self.top_done = 0;
        }
        if top_dir_counted {
            self.top_total -= 1;
            if top_dir_was_complete {
                self.top_done -= 1;
            }
        }
        self.adjust_top_done(top, top_was_complete);
        self.refresh_top();
    }

    /// What leaves the ancestors: the descendants' sums, plus the node itself unless `keep_root`.
    fn removed_sums(&self, id: NodeId, keep_root: bool) -> Removed {
        let i = id.0 as usize;
        if keep_root {
            let (mut pending, mut inexact) = (0, 0);
            for &c in &self.tree.children[i] {
                pending += self.tree.pending_dirs[c.0 as usize];
                inexact += self.tree.inexact_dirs[c.0 as usize];
            }
            return Removed {
                disk: self.tree.agg_disk[i] - self.tree.own_disk[i],
                apparent: self.tree.agg_apparent[i] - self.tree.own_apparent[i],
                files: self.tree.files[i],
                dirs: self.tree.dirs[i],
                pending,
                inexact,
            };
        }
        let counted_dir = self.kind(id) == Kind::Dir && !self.flags(id).contains(Flags::OTHER_FS);
        Removed {
            disk: self.tree.agg_disk[i],
            apparent: self.tree.agg_apparent[i],
            files: self.tree.files[i] + u64::from(self.kind(id) != Kind::Dir),
            dirs: self.tree.dirs[i] + u64::from(counted_dir),
            pending: self.tree.pending_dirs[i],
            inexact: self.tree.inexact_dirs[i],
        }
    }

    /// `id`'s descendants (and `id` itself if `with_root`), parents before children.
    pub(super) fn collect_subtree(&self, id: NodeId, with_root: bool) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.tree.children[id.0 as usize].clone();
        if with_root {
            out.push(id);
        }
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.tree.children[n.0 as usize].iter().copied());
        }
        out
    }

    /// Takes one removed node out of the indexes, the hard-link table and the issue list.
    fn forget_node(&mut self, n: NodeId) {
        let i = n.0 as usize;
        if self.tree.kind[i] != Kind::Dir {
            let (disk, apparent) = (self.tree.own_disk[i], self.tree.own_apparent[i]);
            if self.is_counted(n) {
                self.indexes
                    .remove(self.names.get(self.tree.name[i]), disk, apparent);
            }
            if self.tree.nlink[i] > 1 {
                self.unregister_link(n);
            }
        }
        self.tree.children[i] = Vec::new();
        self.rescan_roots.remove(&n);
        if self.focus == Some(n) {
            self.focus = None;
        }
    }

    /// Removes `n` from the hard-link table; if it was charged, the next live link is.
    fn unregister_link(&mut self, n: NodeId) {
        let i = n.0 as usize;
        let (dev, ino) = (self.tree.dev[i], self.tree.ino[i]);
        let (disk, apparent) = (self.tree.own_disk[i], self.tree.own_apparent[i]);
        let tree = &self.tree;
        let alive = |x: NodeId| tree.exists(x) && x != n;
        if let Removal::Counted(Some(next)) = self.hardlinks.remove(dev, ino, n, alive) {
            self.recharge(next, disk, apparent);
        }
    }

    /// Makes a duplicate hard link the charged one.
    fn recharge(&mut self, node: NodeId, disk: u64, apparent: u64) {
        let i = node.0 as usize;
        self.tree.flags[i].remove(Flags::HARDLINK_DUP);
        self.tree.own_disk[i] = disk;
        self.tree.own_apparent[i] = apparent;
        self.add_to_chain(node, disk, apparent);
        if self.is_counted(node) {
            let kind = self.tree.kind[i];
            self.indexes.add(
                self.names.get(self.tree.name[i]),
                node,
                disk,
                apparent,
                kind == Kind::File,
            );
        }
    }

    /// Adds sizes to `from` and every ancestor.
    fn add_to_chain(&mut self, from: NodeId, disk: u64, apparent: u64) {
        let mut cur = Some(from);
        while let Some(n) = cur {
            self.tree.agg_disk[n.0 as usize] += disk;
            self.tree.agg_apparent[n.0 as usize] += apparent;
            cur = self.parent(n);
        }
    }

    /// Drops the node's issue.
    fn remove_issue(&mut self, node: NodeId) {
        self.remove_issues_of(&[node]);
    }

    /// Drops the issues of `nodes`, keeping the others in insertion order (rows refer to
    /// issues by node, plan D12), and rebuilds the node-to-issue links once.
    fn remove_issues_of(&mut self, nodes: &[NodeId]) {
        let mut any = false;
        for &n in nodes {
            any |= std::mem::replace(&mut self.tree.issue[n.0 as usize], NO_ISSUE) != NO_ISSUE;
        }
        if !any {
            return;
        }
        let tree = &self.tree;
        self.issues
            .retain(|i| tree.issue[i.node.0 as usize] != NO_ISSUE);
        for (slot, issue) in self.issues.iter().enumerate() {
            self.tree.issue[issue.node.0 as usize] = slot as u32;
        }
    }

    fn top_counted(&self, id: NodeId) -> bool {
        self.flags(id).contains(Flags::TOP_COUNTED)
    }

    /// A top-level directory that is about to be scanned counts from now on.
    fn count_top_dir(&mut self, id: NodeId) {
        if !self.top_counted(id) {
            self.tree.flags[id.0 as usize].insert(Flags::TOP_COUNTED);
            self.top_total += 1;
        }
    }

    fn uncount_top_dir(&mut self, id: NodeId) {
        if self.top_counted(id) {
            self.tree.flags[id.0 as usize].remove(Flags::TOP_COUNTED);
            self.top_total -= 1;
        }
    }

    /// Keeps `top_done` right after something changed the completeness of `top`.
    fn adjust_top_done(&mut self, top: Option<NodeId>, was_complete: bool) {
        let Some(t) = top.filter(|&t| self.tree.exists(t) && self.top_counted(t)) else {
            return;
        };
        match (was_complete, self.is_complete(t)) {
            (true, false) => self.top_done -= 1,
            (false, true) => self.top_done += 1,
            _ => {}
        }
    }

    /// A vanished rescan root disappears; any other `Vanished` is an ordinary issue.
    pub(super) fn drop_vanished_rescan_root(&mut self, dir: NodeId) -> bool {
        if dir == NodeId::ROOT || !self.rescan_roots.contains(&dir) {
            return false;
        }
        self.remove_subtree(dir, false);
        true
    }

    /// Updates a rescan root from its re-stat'ed entry (design §4, plan D11).
    pub(super) fn apply_identity(&mut self, node: NodeId, entry: RawEntry) {
        if !self.tree.exists(node) || !self.rescan_roots.contains(&node) {
            return;
        }
        if entry.kind == Kind::Dir && entry.issue == Some((Op::Stat, IssueKind::OtherFilesystem)) {
            self.turn_into_other_filesystem(node, &entry);
            return;
        }
        let new_is_dir = entry.kind == Kind::Dir;
        if !new_is_dir && !self.tree.children[node.0 as usize].is_empty() {
            self.remove_subtree(node, true);
        }
        let i = node.0 as usize;
        let old_is_dir = self.tree.kind[i] == Kind::Dir;
        if self.tree.kind[i] == Kind::File {
            // Its size changes: the old entry must not stay in the lists.
            self.indexes
                .forget_top(&std::collections::HashSet::from([node]));
        }
        let (old_disk, old_apparent) = (self.tree.own_disk[i], self.tree.own_apparent[i]);
        let top = self.top_level_ancestor(node).filter(|&t| t != node);
        let top_was_complete = top.is_some_and(|t| self.top_counted(t) && self.is_complete(t));
        let is_top_dir = self.parent(node) == Some(NodeId::ROOT);

        if !old_is_dir {
            if self.is_counted(node) {
                self.indexes
                    .remove(self.names.get(self.tree.name[i]), old_disk, old_apparent);
            }
            if self.tree.nlink[i] > 1 {
                self.unregister_link(node);
            }
        }
        let (old_disk, old_apparent) = (self.tree.own_disk[i], self.tree.own_apparent[i]);

        let duplicate =
            !new_is_dir && entry.nlink > 1 && self.hardlinks.register(entry.dev, entry.ino, node);
        let (new_disk, new_apparent) = match (new_is_dir, duplicate) {
            (true, _) => (entry.disk_bytes, 0),
            (false, true) => (0, 0),
            (false, false) => (entry.disk_bytes, entry.apparent_bytes),
        };

        // Sizes: the node and every ancestor.
        let mut cur = Some(node);
        while let Some(n) = cur {
            let k = n.0 as usize;
            self.tree.agg_disk[k] = self.tree.agg_disk[k] + new_disk - old_disk;
            self.tree.agg_apparent[k] = self.tree.agg_apparent[k] + new_apparent - old_apparent;
            cur = self.parent(n);
        }
        // Counts and pending work: the ancestors only (the node's own are set below).
        let ancestors: Vec<NodeId> = self.tree.chain(node).skip(1).collect();
        for a in &ancestors {
            let a = a.0 as usize;
            if old_is_dir {
                self.tree.dirs[a] -= 1;
            } else {
                self.tree.files[a] -= 1;
            }
            if new_is_dir {
                self.tree.dirs[a] += 1;
            } else {
                self.tree.files[a] += 1;
            }
        }
        match (old_is_dir, new_is_dir) {
            (true, false) => {
                self.add_pending(node, -1);
                let flags = &mut self.tree.flags[i];
                flags.remove(Flags::PENDING);
                flags.insert(Flags::DONE);
                self.rescan_roots.remove(&node);
                if is_top_dir {
                    self.uncount_top_dir(node);
                }
            }
            (false, true) => {
                self.add_pending(node, 1);
                let flags = &mut self.tree.flags[i];
                flags.remove(Flags::DONE);
                flags.insert(Flags::PENDING);
                if is_top_dir {
                    self.count_top_dir(node);
                }
            }
            _ => {}
        }

        self.tree.kind[i] = entry.kind;
        self.tree.dev[i] = entry.dev;
        self.tree.ino[i] = entry.ino;
        self.tree.nlink[i] = entry.nlink;
        self.tree.own_disk[i] = new_disk;
        self.tree.own_apparent[i] = new_apparent;
        if !new_is_dir {
            self.tree.files[i] = 0;
            self.tree.dirs[i] = 0;
        }
        let flags = &mut self.tree.flags[i];
        flags.remove(Flags::HARDLINK_DUP);
        if duplicate {
            flags.insert(Flags::HARDLINK_DUP);
        }
        if !new_is_dir && self.is_counted(node) {
            self.indexes.add(
                self.names.get(self.tree.name[i]),
                node,
                new_disk,
                new_apparent,
                entry.kind == Kind::File,
            );
        }
        if !old_is_dir || !new_is_dir {
            self.refresh_top();
        }
        self.adjust_top_done(top, top_was_complete);
    }

    /// The rescan found the folder on another device: it is flagged `OTHER_FS` like such an
    /// entry would have been, and leaves the sizes, the directory count and the progress.
    fn turn_into_other_filesystem(&mut self, node: NodeId, entry: &RawEntry) {
        let i = node.0 as usize;
        if self.kind(node) != Kind::Dir {
            return;
        }
        let top = self.top_level_ancestor(node).filter(|&t| t != node);
        let top_was_complete = top.is_some_and(|t| self.top_counted(t) && self.is_complete(t));
        let old_disk = self.tree.own_disk[i];
        let mut cur = Some(node);
        while let Some(n) = cur {
            self.tree.agg_disk[n.0 as usize] -= old_disk;
            cur = self.parent(n);
        }
        let ancestors: Vec<NodeId> = self.tree.chain(node).skip(1).collect();
        for a in ancestors {
            self.tree.dirs[a.0 as usize] -= 1;
        }
        self.add_pending(node, -1);
        self.tree.own_disk[i] = 0;
        self.tree.dev[i] = entry.dev;
        self.tree.ino[i] = entry.ino;
        let flags = &mut self.tree.flags[i];
        flags.remove(Flags::PENDING);
        flags.insert(Flags::DONE);
        flags.insert(Flags::OTHER_FS);
        self.remove_issue(node);
        self.record_issue(node, Op::Stat, IssueKind::OtherFilesystem);
        self.uncount_top_dir(node);
        self.rescan_roots.remove(&node);
        self.adjust_top_done(top, top_was_complete);
    }

    /// Adds `delta` (+1 or -1) to `pending_dirs` of `node` and all its ancestors.
    fn add_pending(&mut self, node: NodeId, delta: i32) {
        let mut cur = Some(node);
        while let Some(n) = cur {
            let p = &mut self.tree.pending_dirs[n.0 as usize];
            if delta > 0 {
                *p += 1;
            } else {
                debug_assert!(*p >= 1, "pending_dirs underflow at {n:?}");
                *p -= 1;
            }
            cur = self.parent(n);
        }
    }

    /// Focus on a folder that is still scanning; completion is reported once.
    pub fn set_focus(&mut self, id: Option<NodeId>) {
        self.focus = id.filter(|&n| self.tree.exists(n) && !self.is_complete(n));
    }

    /// The focused folder, once, when it has become complete. The focus then clears.
    pub fn take_focus_completed(&mut self) -> Option<NodeId> {
        let focus = self.focus?;
        if !self.tree.exists(focus) {
            self.focus = None;
            return None;
        }
        if self.is_complete(focus) {
            self.focus = None;
            return Some(focus);
        }
        None
    }

    /// How `r` should treat `id` right now (design §3.11, plan O7).
    pub fn rescan_plan(&self, id: NodeId) -> RescanPlan {
        if self.tree.exists(id) && self.flags(id).contains(Flags::OTHER_FS) {
            return RescanPlan::OtherFilesystem;
        }
        let scanning = matches!(self.state, ScanState::Starting | ScanState::Scanning);
        if !scanning {
            return RescanPlan::Full;
        }
        if id == NodeId::ROOT {
            return RescanPlan::NoneWhileScanning;
        }
        if self.is_complete(id) {
            return RescanPlan::Full;
        }
        RescanPlan::PrioritiseAndResume {
            resume: self.outermost_skips_under(id),
        }
    }

    /// Skipped folders at or under `id` that have no skipped folder above them (up to `id`).
    fn outermost_skips_under(&self, id: NodeId) -> Vec<NodeId> {
        let is_skip = |n: NodeId| {
            self.issue(n)
                .is_some_and(|i| i.kind == IssueKind::SkippedByUser)
        };
        let mut found: Vec<NodeId> = self
            .issues
            .iter()
            .map(|i| i.node)
            .filter(|&n| self.tree.exists(n) && is_skip(n))
            .filter(|&n| {
                if n == id {
                    return true;
                }
                // Between `n` and `id` (inclusive of `id`) no other folder is skipped.
                let above: Vec<NodeId> = self
                    .tree
                    .chain(n)
                    .skip(1)
                    .take_while(|&a| a != id)
                    .collect();
                let reaches_id = self.tree.chain(n).any(|a| a == id);
                reaches_id && !is_skip(id) && !above.into_iter().any(is_skip)
            })
            .collect();
        found.sort();
        found
    }
}
