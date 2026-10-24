//! The scan tree: arena, aggregates and indexes (T003, T005, T011, T012).
//!
//! Only the main thread mutates the model. Bodies are stubs owned by later tasks.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::scan::{RawEntry, ScanMsg};
use crate::types::{IssueKind, Kind, NodeId, Op, ScanIssue, ScanState, SizeMode};

mod category;
mod control;
mod delete;
mod hardlink;
mod index;
mod names;
mod tree;

use hardlink::HardLinks;
use index::Indexes;
pub use index::ext_of;
use names::NameArena;
use tree::{NO_ISSUE, NO_PARENT, NodeInit, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelOptions {
    pub top_n: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanTotals {
    pub entries_seen: u64,
    pub files: u64,
    pub dirs: u64,
    pub pending_dirs: u64,
    pub disk_bytes: u64,
    pub apparent_bytes: u64,
}

/// Why a delete cannot start. Aggregates are a UI concept, not a model one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    ScanRoot,
    OtherFilesystem,
    AlreadyDeleting,
    DeleteRunning(NodeId),
}

/// What the app must do after a delete event is applied (plan D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconcile {
    None,
    Rescan(NodeId),
}

/// What `r` does for a folder (design §3.11, plan O7). The app only executes the plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RescanPlan {
    /// The folder is complete (or nothing scans it): `begin_rescan` + `ScanHandle::rescan`.
    Full,
    /// The folder is still being scanned: focus it and resume these skipped folders
    /// (outermost only), each with `remove_prefixes_under`, `begin_rescan`, `rescan`.
    PrioritiseAndResume { resume: Vec<NodeId> },
    /// The root while the scan runs: nothing changes.
    NoneWhileScanning,
    /// Another filesystem: never scanned in one-filesystem mode (ADR-008), so never rescanned.
    OtherFilesystem,
}

/// Snapshot shown in the delete dialog. Counts are lower bounds unless `exact`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeleteCheck {
    pub exact: bool,
    pub largest_known: [Option<NodeId>; 3],
    pub files: u64,
    pub dirs: u64,
    pub size_bytes: u64,
    pub outside_links: u64,
    pub freed_bytes_upper: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtKey {
    Ext(Box<[u8]>),
    NoExt,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Video,
    Image,
    Archive,
    CodeText,
    Binary,
    Documents,
    AudioOther,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExtTotals {
    pub disk_bytes: u64,
    pub apparent_bytes: u64,
    pub count: u64,
}

/// Per-node state bits.
///
/// `REMOVED` marks a hole left by a delete: NodeIds are never reused, and messages
/// that target a removed node are ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags(u16);

impl Flags {
    pub const PENDING: Flags = Flags(1 << 0);
    pub const DONE: Flags = Flags(1 << 1);
    pub const INCOMPLETE: Flags = Flags(1 << 2);
    pub const ERROR: Flags = Flags(1 << 3);
    pub const OTHER_FS: Flags = Flags(1 << 4);
    pub const HARDLINK_DUP: Flags = Flags(1 << 5);
    pub const DELETING: Flags = Flags(1 << 6);
    pub const TOMBSTONE: Flags = Flags(1 << 7);
    pub const REMOVED: Flags = Flags(1 << 8);
    /// A directory counted in `top_level_progress` (a descended child of the root).
    pub const TOP_COUNTED: Flags = Flags(1 << 9);

    pub const fn empty() -> Flags {
        Flags(0)
    }

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Flags) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Flags) {
        self.0 &= !other.0;
    }
}

pub struct Model {
    tree: Tree,
    names: NameArena,
    hardlinks: HardLinks,
    issues: Vec<ScanIssue>,
    root_path: PathBuf,
    state: ScanState,
    entries_seen: u64,
    /// Descended directory children of the root, and how many of them are complete.
    top_total: u32,
    top_done: u32,
    indexes: Indexes,
    /// The folder being prioritised, until it completes.
    focus: Option<NodeId>,
    /// Nodes whose rescan has begun and whose fresh listing (or failure) has not arrived.
    rescan_roots: HashSet<NodeId>,
    /// The folder a delete is running on.
    delete_target: Option<NodeId>,
}

/// Sums of one listing, applied to the directory and its ancestors in a single walk.
#[derive(Default)]
struct ListingDelta {
    disk: u64,
    apparent: u64,
    files: u64,
    dirs: u64,
    descended_dirs: u32,
}

fn flags_for_issue(kind: IssueKind) -> Flags {
    match kind {
        IssueKind::OtherFilesystem => Flags::OTHER_FS,
        IssueKind::SkippedByUser => Flags::empty(),
        IssueKind::PermissionDenied | IssueKind::Vanished | IssueKind::Other(_) => Flags::ERROR,
    }
}

impl Model {
    // T003
    pub fn new(root_path: PathBuf, root: RawEntry, opts: ModelOptions) -> Model {
        let mut names = NameArena::default();
        let mut tree = Tree::default();
        tree.ensure_len(1);
        let is_dir = root.kind == Kind::Dir;
        tree.fill(
            NodeId::ROOT,
            NodeInit {
                parent: NodeId(NO_PARENT),
                name: names.push(&root.name),
                kind: root.kind,
                flags: if is_dir { Flags::PENDING } else { Flags::DONE },
                own_disk: root.disk_bytes,
                own_apparent: if is_dir { 0 } else { root.apparent_bytes },
                nlink: root.nlink,
                ino: root.ino,
                dev: root.dev,
            },
        );
        tree.pending_dirs[0] = u32::from(is_dir);
        Model {
            tree,
            names,
            hardlinks: HardLinks::default(),
            issues: Vec::new(),
            root_path,
            state: ScanState::Starting,
            entries_seen: 0,
            top_total: 0,
            top_done: 0,
            indexes: Indexes::new(opts.top_n),
            focus: None,
            rescan_roots: HashSet::new(),
            delete_target: None,
        }
    }

    pub fn root(&self) -> NodeId {
        NodeId::ROOT
    }

    pub fn exists(&self, id: NodeId) -> bool {
        self.tree.exists(id)
    }

    pub fn name(&self, id: NodeId) -> &[u8] {
        self.names.get(self.tree.name[id.0 as usize])
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let p = self.tree.parent[id.0 as usize];
        (p != NO_PARENT).then_some(NodeId(p))
    }

    pub fn kind(&self, id: NodeId) -> Kind {
        self.tree.kind[id.0 as usize]
    }

    pub fn depth(&self, id: NodeId) -> u32 {
        self.tree.chain(id).count() as u32 - 1
    }

    pub fn size(&self, id: NodeId, mode: SizeMode) -> u64 {
        let i = id.0 as usize;
        match mode {
            SizeMode::Disk => self.tree.agg_disk[i],
            SizeMode::Apparent => self.tree.agg_apparent[i],
        }
    }

    /// `(files, dirs)` in the subtree.
    pub fn counts(&self, id: NodeId) -> (u64, u64) {
        let i = id.0 as usize;
        (self.tree.files[i], self.tree.dirs[i])
    }

    pub fn nlink(&self, id: NodeId) -> u32 {
        self.tree.nlink[id.0 as usize]
    }

    /// `(dev, ino)`.
    pub fn identity(&self, id: NodeId) -> (u64, u64) {
        let i = id.0 as usize;
        (self.tree.dev[i], self.tree.ino[i])
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.tree.children[id.0 as usize]
    }

    /// Sorted by `(size desc, name asc)`.
    pub fn children_sorted(&self, id: NodeId, mode: SizeMode) -> Vec<NodeId> {
        let mut sorted = self.children(id).to_vec();
        sorted.sort_by(|&a, &b| {
            self.size(b, mode)
                .cmp(&self.size(a, mode))
                .then_with(|| self.name(a).cmp(self.name(b)))
        });
        sorted
    }

    /// No scan work remains: files, or a directory that is `DONE` with `pending_dirs == 0`.
    pub fn is_complete(&self, id: NodeId) -> bool {
        let i = id.0 as usize;
        self.tree.kind[i] != Kind::Dir
            || (self.tree.flags[i].contains(Flags::DONE) && self.tree.pending_dirs[i] == 0)
    }

    /// Complete, and nothing inside was skipped or left unread.
    pub fn is_exact(&self, id: NodeId) -> bool {
        self.is_complete(id) && self.tree.inexact_dirs[id.0 as usize] == 0
    }

    pub fn flags(&self, id: NodeId) -> Flags {
        self.tree.flags[id.0 as usize]
    }

    pub fn issue(&self, id: NodeId) -> Option<&ScanIssue> {
        let idx = self.tree.issue[id.0 as usize];
        (idx != NO_ISSUE).then(|| &self.issues[idx as usize])
    }

    /// Scan issues in insertion order. A rescan can remove entries, but never reorders the
    /// rest; rows refer to an issue by its node (`ScanIssue::node`), not by index (plan D12).
    pub fn issues(&self) -> &[ScanIssue] {
        &self.issues
    }

    pub fn path(&self, id: NodeId) -> PathBuf {
        let rel = self.rel_path(id);
        if rel.as_os_str().is_empty() {
            return self.root_path.clone();
        }
        self.root_path.join(rel)
    }

    /// Path relative to the scan root; empty for the root.
    pub fn rel_path(&self, id: NodeId) -> PathBuf {
        let mut names: Vec<&[u8]> = self
            .tree
            .chain(id)
            .filter(|&n| n != NodeId::ROOT)
            .map(|n| self.name(n))
            .collect();
        names.reverse();
        names.into_iter().map(OsStr::from_bytes).collect()
    }

    pub fn totals(&self) -> ScanTotals {
        let root = NodeId::ROOT;
        let (files, dirs) = self.counts(root);
        ScanTotals {
            entries_seen: self.entries_seen,
            files,
            dirs,
            pending_dirs: u64::from(self.tree.pending_dirs[0]),
            disk_bytes: self.size(root, SizeMode::Disk),
            apparent_bytes: self.size(root, SizeMode::Apparent),
        }
    }

    /// `(done, total)` over the root's descended directory children.
    pub fn top_level_progress(&self) -> (u32, u32) {
        (self.top_done, self.top_total)
    }

    pub fn state(&self) -> ScanState {
        self.state
    }

    /// Unknown, removed, non-directory and already-finished targets are ignored.
    pub fn apply(&mut self, msg: ScanMsg) {
        if self.state == ScanState::Starting {
            self.state = ScanState::Scanning;
        }
        match msg {
            ScanMsg::Listing {
                dir,
                first_id,
                entries,
            } => self.apply_listing(dir, first_id, entries),
            ScanMsg::DirIssue { dir, op, kind } => self.apply_dir_issue(dir, op, kind),
            ScanMsg::Cancelled { dir } => self.apply_cancelled(dir),
            ScanMsg::Done { cancelled } => self.apply_done(cancelled),
            ScanMsg::Identity { node, entry } => self.apply_identity(node, entry),
        }
    }

    /// A directory that exists and has not yet been listed, failed or cancelled.
    fn is_open_dir(&self, id: NodeId) -> bool {
        self.tree.exists(id) && self.kind(id) == Kind::Dir && !self.flags(id).contains(Flags::DONE)
    }

    fn apply_listing(&mut self, dir: NodeId, first_id: NodeId, entries: Vec<RawEntry>) {
        if !self.is_open_dir(dir) {
            return;
        }
        let Ok(count) = u32::try_from(entries.len()) else {
            return;
        };
        let Some(end) = first_id.0.checked_add(count) else {
            return;
        };
        if (first_id.0..end).any(|n| self.tree.exists(NodeId(n))) {
            return;
        }
        self.tree.ensure_len(end as usize);

        let mut delta = ListingDelta::default();
        // Under a folder being deleted, scan errors are noise.
        let quiet = self.is_tombstoned(dir);
        for (n, entry) in (first_id.0..end).zip(entries) {
            self.insert_entry(dir, NodeId(n), entry, quiet, &mut delta);
        }
        self.entries_seen += u64::from(count);
        if dir == NodeId::ROOT {
            self.top_total += delta.descended_dirs;
        }
        self.finish_dir(dir, &delta);
    }

    fn insert_entry(
        &mut self,
        dir: NodeId,
        id: NodeId,
        entry: RawEntry,
        quiet: bool,
        delta: &mut ListingDelta,
    ) {
        let is_dir = entry.kind == Kind::Dir;
        let descend = is_dir && entry.issue.is_none();
        let mut flags = if descend { Flags::PENDING } else { Flags::DONE };
        if descend && dir == NodeId::ROOT {
            flags.insert(Flags::TOP_COUNTED);
        }
        if let Some((_, kind)) = entry.issue {
            flags.insert(flags_for_issue(kind));
        }
        let other_fs = is_dir && flags.contains(Flags::OTHER_FS);
        let duplicate =
            !is_dir && entry.nlink > 1 && self.hardlinks.register(entry.dev, entry.ino, id);
        if duplicate {
            flags.insert(Flags::HARDLINK_DUP);
        }
        let (own_disk, own_apparent) = match (is_dir, duplicate || other_fs) {
            (_, true) => (0, 0),
            (true, false) => (entry.disk_bytes, 0),
            (false, false) => (entry.disk_bytes, entry.apparent_bytes),
        };

        self.tree.fill(
            id,
            NodeInit {
                parent: dir,
                name: self.names.push(&entry.name),
                kind: entry.kind,
                flags,
                own_disk,
                own_apparent,
                nlink: entry.nlink,
                ino: entry.ino,
                dev: entry.dev,
            },
        );
        self.tree.pending_dirs[id.0 as usize] = u32::from(descend);
        if !is_dir && self.is_counted(id) {
            self.indexes.add(
                &entry.name,
                id,
                own_disk,
                own_apparent,
                entry.kind == Kind::File,
            );
        }
        self.tree.children[dir.0 as usize].push(id);
        if let Some((op, kind)) = entry.issue
            && !quiet
        {
            self.record_issue(id, op, kind);
        }

        delta.disk += own_disk;
        delta.apparent += own_apparent;
        match (is_dir, other_fs) {
            (false, _) => delta.files += 1,
            (true, false) => delta.dirs += 1,
            (true, true) => {}
        }
        delta.descended_dirs += u32::from(descend);
    }

    fn record_issue(&mut self, node: NodeId, op: Op, kind: IssueKind) {
        self.tree.issue[node.0 as usize] = self.issues.len() as u32;
        self.issues.push(ScanIssue { node, op, kind });
    }

    /// Adds the listing's sums to `dir` and its ancestors and retires `dir`'s own pending job.
    fn finish_dir(&mut self, dir: NodeId, delta: &ListingDelta) {
        let top = self.top_level_ancestor(dir);
        let top_was_complete = top.is_some_and(|t| self.is_complete(t));
        let mut cur = Some(dir);
        while let Some(n) = cur {
            let i = n.0 as usize;
            self.tree.agg_disk[i] += delta.disk;
            self.tree.agg_apparent[i] += delta.apparent;
            self.tree.files[i] += delta.files;
            self.tree.dirs[i] += delta.dirs;
            let pending = self.tree.pending_dirs[i] + delta.descended_dirs;
            debug_assert!(pending >= 1, "pending_dirs underflow at {n:?}");
            self.tree.pending_dirs[i] = pending - 1;
            cur = self.parent(n);
        }
        self.mark_done(dir);
        self.rescan_roots.remove(&dir);
        if let Some(t) = top
            && !top_was_complete
            && self.is_complete(t)
        {
            self.top_done += 1;
        }
    }

    /// The child of the root that is `id` or contains it; `None` for the root itself.
    fn top_level_ancestor(&self, id: NodeId) -> Option<NodeId> {
        self.tree
            .chain(id)
            .find(|&n| self.parent(n) == Some(NodeId::ROOT))
    }

    fn mark_done(&mut self, dir: NodeId) {
        let flags = &mut self.tree.flags[dir.0 as usize];
        flags.remove(Flags::PENDING);
        flags.insert(Flags::DONE);
    }

    fn apply_dir_issue(&mut self, dir: NodeId, op: Op, kind: IssueKind) {
        if !self.is_open_dir(dir) {
            return;
        }
        if kind == IssueKind::Vanished && self.drop_vanished_rescan_root(dir) {
            return;
        }
        if self.is_tombstoned(dir) {
            self.finish_dir(dir, &ListingDelta::default());
            return;
        }
        self.record_issue(dir, op, kind);
        self.tree.flags[dir.0 as usize].insert(flags_for_issue(kind));
        self.finish_dir(dir, &ListingDelta::default());
    }

    fn apply_cancelled(&mut self, dir: NodeId) {
        if !self.is_open_dir(dir) {
            return;
        }
        // A delete's cancels (the app cancels the target's jobs) are not skips.
        if !self.is_tombstoned(dir) {
            self.tree.flags[dir.0 as usize].insert(Flags::INCOMPLETE);
            let ancestors: Vec<NodeId> = self.tree.chain(dir).collect();
            for n in ancestors {
                let i = n.0 as usize;
                self.tree.inexact_dirs[i] += 1;
            }
        }
        self.finish_dir(dir, &ListingDelta::default());
    }

    fn apply_done(&mut self, cancelled: bool) {
        if cancelled {
            for i in 0..self.tree.len() {
                let unread = self.tree.exists(NodeId(i as u32))
                    && self.tree.kind[i] == Kind::Dir
                    && !self.tree.flags[i].contains(Flags::DONE);
                if unread {
                    self.tree.flags[i].insert(Flags::INCOMPLETE);
                }
            }
        }
        self.state = if cancelled {
            ScanState::Stopped
        } else {
            ScanState::Done
        };
    }

    // T005
    /// Largest disk size first, then key ascending.
    pub fn ext_totals(&self) -> Vec<(ExtKey, ExtTotals)> {
        self.indexes.ext_totals()
    }

    pub fn category_of(ext: &ExtKey) -> Category {
        category::category_of(ext)
    }

    /// The category of a file, from its extension. Folders are coloured by identity, not type.
    pub fn category_of_node(&self, id: NodeId) -> Category {
        Self::category_of(&ext_of(self.name(id)))
    }

    /// The largest files by size, biggest first, ties by smaller `NodeId`.
    pub fn top_files(&self, mode: SizeMode) -> Vec<NodeId> {
        match mode {
            SizeMode::Disk => self.indexes.top_disk.sorted(),
            SizeMode::Apparent => self.indexes.top_apparent.sorted(),
        }
    }

    /// Per-category sums of the per-extension totals, largest disk size first.
    pub fn category_totals(&self) -> Vec<(Category, ExtTotals)> {
        let mut sums: BTreeMap<Category, ExtTotals> = BTreeMap::new();
        for (key, totals) in self.ext_totals() {
            let sum = sums.entry(Self::category_of(&key)).or_default();
            sum.disk_bytes += totals.disk_bytes;
            sum.apparent_bytes += totals.apparent_bytes;
            sum.count += totals.count;
        }
        let mut all: Vec<_> = sums.into_iter().collect();
        all.sort_by_key(|(_, t)| std::cmp::Reverse(t.disk_bytes));
        all
    }

    /// Rebuilds the top-N lists from all files (after a delete). O(files).
    pub fn recompute_top(&mut self) {
        self.indexes.clear_top();
        for i in 0..self.tree.len() {
            let id = NodeId(i as u32);
            if self.tree.exists(id) && self.tree.kind[i] == Kind::File && self.is_counted(id) {
                self.indexes
                    .offer_top(id, self.tree.agg_disk[i], self.tree.agg_apparent[i]);
            }
        }
    }

    /// Not a duplicate hard link, an unreadable entry or another filesystem.
    fn is_counted(&self, id: NodeId) -> bool {
        let flags = self.flags(id);
        ![Flags::HARDLINK_DUP, Flags::ERROR, Flags::OTHER_FS]
            .iter()
            .any(|f| flags.contains(*f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_insert_remove_contains() {
        let mut f = Flags::empty();
        assert!(!f.contains(Flags::DONE));
        f.insert(Flags::DONE);
        f.insert(Flags::REMOVED);
        assert!(f.contains(Flags::DONE) && f.contains(Flags::REMOVED));
        assert!(!f.contains(Flags::PENDING));
        f.remove(Flags::DONE);
        assert!(!f.contains(Flags::DONE) && f.contains(Flags::REMOVED));
    }

    #[test]
    fn flag_bits_are_distinct() {
        let all = [
            Flags::PENDING,
            Flags::DONE,
            Flags::INCOMPLETE,
            Flags::ERROR,
            Flags::OTHER_FS,
            Flags::HARDLINK_DUP,
            Flags::DELETING,
            Flags::TOMBSTONE,
            Flags::REMOVED,
            Flags::TOP_COUNTED,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!a.contains(*b));
            }
        }
    }
}
