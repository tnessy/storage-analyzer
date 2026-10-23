//! The scan tree: arena, aggregates and indexes (T003, T005, T011, T012).
//!
//! Only the main thread mutates the model. Bodies are stubs owned by later tasks.

use std::path::PathBuf;

use crate::delete::{DeleteEvent, DeleteRequest};
use crate::scan::{RawEntry, ScanMsg};
use crate::types::{Kind, NodeId, ScanIssue, ScanState, SizeMode};

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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ExtKey {
    Ext(Box<[u8]>),
    NoExt,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    _private: (),
}

impl Model {
    // T003
    pub fn new(_root_path: PathBuf, _root: RawEntry, _opts: ModelOptions) -> Model {
        todo!("T003")
    }
    pub fn root(&self) -> NodeId {
        todo!("T003")
    }
    pub fn exists(&self, _id: NodeId) -> bool {
        todo!("T003")
    }
    pub fn name(&self, _id: NodeId) -> &[u8] {
        todo!("T003")
    }
    pub fn parent(&self, _id: NodeId) -> Option<NodeId> {
        todo!("T003")
    }
    pub fn kind(&self, _id: NodeId) -> Kind {
        todo!("T003")
    }
    pub fn depth(&self, _id: NodeId) -> u32 {
        todo!("T003")
    }
    pub fn size(&self, _id: NodeId, _mode: SizeMode) -> u64 {
        todo!("T003")
    }
    /// `(files, dirs)` in the subtree.
    pub fn counts(&self, _id: NodeId) -> (u64, u64) {
        todo!("T003")
    }
    pub fn nlink(&self, _id: NodeId) -> u32 {
        todo!("T003")
    }
    /// `(dev, ino)`.
    pub fn identity(&self, _id: NodeId) -> (u64, u64) {
        todo!("T003")
    }
    pub fn children(&self, _id: NodeId) -> &[NodeId] {
        todo!("T003")
    }
    /// Sorted by `(size desc, name asc)`.
    pub fn children_sorted(&self, _id: NodeId, _mode: SizeMode) -> Vec<NodeId> {
        todo!("T003")
    }
    /// No scan work remains: the node is `DONE` and `pending_dirs == 0`.
    pub fn is_complete(&self, _id: NodeId) -> bool {
        todo!("T003")
    }
    /// Complete, and nothing inside was skipped or left unread.
    pub fn is_exact(&self, _id: NodeId) -> bool {
        todo!("T003")
    }
    pub fn flags(&self, _id: NodeId) -> Flags {
        todo!("T003")
    }
    pub fn issue(&self, _id: NodeId) -> Option<&ScanIssue> {
        todo!("T003")
    }
    pub fn issues(&self) -> &[ScanIssue] {
        todo!("T003")
    }
    pub fn path(&self, _id: NodeId) -> PathBuf {
        todo!("T003")
    }
    /// Path relative to the scan root.
    pub fn rel_path(&self, _id: NodeId) -> PathBuf {
        todo!("T003")
    }
    pub fn totals(&self) -> ScanTotals {
        todo!("T003")
    }
    pub fn state(&self) -> ScanState {
        todo!("T003")
    }
    pub fn apply(&mut self, _msg: ScanMsg) {
        todo!("T003")
    }

    // T005
    pub fn ext_totals(&self) -> Vec<(ExtKey, ExtTotals)> {
        todo!("T005")
    }
    pub fn category_of(_ext: &ExtKey) -> Category {
        todo!("T005")
    }
    /// Files by extension; directories by the largest category inside.
    pub fn category_of_node(&self, _id: NodeId) -> Category {
        todo!("T005")
    }
    /// Sorted by size, descending.
    pub fn top_files(&self, _mode: SizeMode) -> Vec<NodeId> {
        todo!("T005")
    }

    // T012
    pub fn set_focus(&mut self, _id: Option<NodeId>) {
        todo!("T012")
    }
    /// Reports once when the focused node goes from incomplete to complete.
    pub fn take_focus_completed(&mut self) -> Option<NodeId> {
        todo!("T012")
    }
    /// `false` if the node is already complete.
    pub fn mark_skipped(&mut self, _id: NodeId) -> bool {
        todo!("T012")
    }
    /// Returns the node's path and depth for `ScanHandle::rescan`.
    pub fn begin_rescan(&mut self, _id: NodeId) -> (PathBuf, u32) {
        todo!("T012")
    }

    // T011
    pub fn delete_refusal(&self, _id: NodeId) -> Option<Refusal> {
        todo!("T011")
    }
    pub fn delete_check(&self, _id: NodeId, _mode: SizeMode) -> DeleteCheck {
        todo!("T011")
    }
    pub fn begin_delete(&mut self, _id: NodeId) -> Result<DeleteRequest, Refusal> {
        todo!("T011")
    }
    pub fn apply_delete(&mut self, _ev: &DeleteEvent) -> Reconcile {
        todo!("T011")
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
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!a.contains(*b));
            }
        }
    }
}
