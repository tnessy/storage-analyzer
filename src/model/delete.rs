//! The model side of delete: refusals, the confirmation numbers, the tombstone lifecycle
//! and reconciliation for each outcome (T011, design §3.8).

use std::collections::HashMap;

use super::{DeleteCheck, Flags, Model, Reconcile, Refusal};
use crate::delete::{DeleteEvent, DeleteRequest, Outcome};
use crate::types::{Kind, NodeId, SizeMode};

impl Model {
    /// Why `id` cannot be deleted right now. A node that is gone counts as already being deleted.
    pub fn delete_refusal(&self, id: NodeId) -> Option<Refusal> {
        if !self.tree.exists(id) {
            return Some(Refusal::AlreadyDeleting);
        }
        if id == NodeId::ROOT {
            return Some(Refusal::ScanRoot);
        }
        if self.flags(id).contains(Flags::OTHER_FS) {
            return Some(Refusal::OtherFilesystem);
        }
        let deleting = |n: NodeId| {
            let f = self.flags(n);
            f.contains(Flags::DELETING) || f.contains(Flags::TOMBSTONE)
        };
        if self.tree.chain(id).any(deleting) {
            return Some(Refusal::AlreadyDeleting);
        }
        self.delete_target.map(Refusal::DeleteRunning)
    }

    /// The numbers for the confirmation dialog. O(subtree). Counts cover what is inside
    /// `id` (not `id` itself) and are lower bounds unless `exact`.
    pub fn delete_check(&self, id: NodeId, mode: SizeMode) -> DeleteCheck {
        let (files, dirs) = self.counts(id);
        let mut largest = self.children_sorted(id, mode).into_iter();
        let largest_known = [largest.next(), largest.next(), largest.next()];

        // Per inode: occurrences inside the subtree, the link count, disk bytes charged inside.
        let mut inodes: HashMap<(u64, u64), (u32, u32, u64)> = HashMap::new();
        for n in self.collect_subtree(id, true) {
            let i = n.0 as usize;
            if self.tree.kind[i] != Kind::Dir && self.tree.nlink[i] > 1 {
                let entry = inodes
                    .entry((self.tree.dev[i], self.tree.ino[i]))
                    .or_insert((0, self.tree.nlink[i], 0));
                entry.0 += 1;
                entry.2 += self.tree.own_disk[i];
            }
        }
        let shared = inodes.values().filter(|(inside, links, _)| inside < links);
        let (outside_links, kept_bytes) =
            shared.fold((0, 0), |(n, bytes), (_, _, disk)| (n + 1, bytes + disk));

        DeleteCheck {
            exact: self.is_complete(id) && self.is_exact(id),
            largest_known,
            files,
            dirs,
            size_bytes: self.size(id, mode),
            outside_links,
            freed_bytes_upper: self.size(id, SizeMode::Disk) - kept_bytes,
        }
    }

    /// Re-checks the refusals, tombstones `id` and builds the executor's request. The app
    /// then cancels the scan prefix of `id`.
    pub fn begin_delete(&mut self, id: NodeId) -> Result<DeleteRequest, Refusal> {
        if let Some(refusal) = self.delete_refusal(id) {
            return Err(refusal);
        }
        let check = self.delete_check(id, SizeMode::Disk);
        let kind = self.kind(id);
        let (dev, ino) = self.identity(id);
        let is_dir = u64::from(kind == Kind::Dir);
        self.tree.flags[id.0 as usize].insert(Flags::DELETING);
        self.tree.flags[id.0 as usize].insert(Flags::TOMBSTONE);
        self.delete_target = Some(id);
        Ok(DeleteRequest {
            node: id,
            rel_path: self.rel_path(id),
            expect_dev: dev,
            expect_ino: ino,
            kind,
            total_files: check.exact.then_some(check.files + (1 - is_dir)),
            total_dirs: check.exact.then_some(check.dirs + is_dir),
        })
    }

    /// Applies an executor event. `Rescan(id)` means the app must run `begin_rescan(id)`.
    pub fn apply_delete(&mut self, ev: &DeleteEvent) -> Reconcile {
        let DeleteEvent::Finished(outcome) = ev else {
            return Reconcile::None;
        };
        let Some(target) = self.delete_target.take() else {
            return Reconcile::None;
        };
        // The folder may have left the model meanwhile (a rescan of a parent): nothing to repair.
        if !self.tree.exists(target) {
            return Reconcile::None;
        }
        match outcome {
            Outcome::Removed(_) => {
                self.remove_subtree(target, false);
                Reconcile::None
            }
            Outcome::Partial { .. } => {
                self.clear_tombstone(target);
                Reconcile::Rescan(target)
            }
            Outcome::Aborted { .. } => {
                self.clear_tombstone(target);
                Reconcile::Rescan(target)
            }
        }
    }

    /// Whether `dir` is inside the folder being deleted.
    pub(super) fn is_tombstoned(&self, dir: NodeId) -> bool {
        self.delete_target
            .is_some_and(|t| self.tree.chain(dir).any(|n| n == t))
    }

    fn clear_tombstone(&mut self, target: NodeId) {
        if !self.tree.exists(target) {
            return;
        }
        for n in self.collect_subtree(target, true) {
            let flags = &mut self.tree.flags[n.0 as usize];
            flags.remove(Flags::DELETING);
            flags.remove(Flags::TOMBSTONE);
        }
    }
}
