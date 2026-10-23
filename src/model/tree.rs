//! Struct-of-arrays node storage, indexed by `NodeId`.
//!
//! Ids are handed out by the scanner in blocks that may arrive out of order, so the
//! arrays can contain placeholders (no parent, not the root) until their block arrives.

use super::Flags;
use super::names::NameRef;
use crate::types::{Kind, NodeId};

pub const NO_PARENT: u32 = u32::MAX;
pub const NO_ISSUE: u32 = u32::MAX;

pub struct NodeInit {
    pub parent: NodeId,
    pub name: NameRef,
    pub kind: Kind,
    pub flags: Flags,
    pub own_disk: u64,
    pub own_apparent: u64,
    pub nlink: u32,
    pub ino: u64,
    pub dev: u64,
}

#[derive(Default)]
pub struct Tree {
    pub parent: Vec<u32>,
    pub name: Vec<NameRef>,
    pub kind: Vec<Kind>,
    pub flags: Vec<Flags>,
    pub own_disk: Vec<u64>,
    pub own_apparent: Vec<u64>,
    pub agg_disk: Vec<u64>,
    pub agg_apparent: Vec<u64>,
    pub files: Vec<u64>,
    pub dirs: Vec<u64>,
    pub pending_dirs: Vec<u32>,
    pub inexact_dirs: Vec<u32>,
    pub nlink: Vec<u32>,
    pub ino: Vec<u64>,
    pub dev: Vec<u64>,
    pub issue: Vec<u32>,
    pub children: Vec<Vec<NodeId>>,
}

impl Tree {
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    /// Grows the arrays to `len` entries with placeholders.
    pub fn ensure_len(&mut self, len: usize) {
        if len <= self.len() {
            return;
        }
        let extra = len - self.len();
        self.parent.resize(len, NO_PARENT);
        self.name.resize(len, NameRef::default());
        self.kind.resize(len, Kind::File);
        self.flags.resize(len, Flags::empty());
        self.own_disk.resize(len, 0);
        self.own_apparent.resize(len, 0);
        self.agg_disk.resize(len, 0);
        self.agg_apparent.resize(len, 0);
        self.files.resize(len, 0);
        self.dirs.resize(len, 0);
        self.pending_dirs.resize(len, 0);
        self.inexact_dirs.resize(len, 0);
        self.nlink.resize(len, 0);
        self.ino.resize(len, 0);
        self.dev.resize(len, 0);
        self.issue.resize(len, NO_ISSUE);
        self.children
            .extend(std::iter::repeat_with(Vec::new).take(extra));
    }

    /// True for the root and for every filled node that has not been removed.
    pub fn exists(&self, id: NodeId) -> bool {
        let i = id.0 as usize;
        i < self.len()
            && (id == NodeId::ROOT || self.parent[i] != NO_PARENT)
            && !self.flags[i].contains(Flags::REMOVED)
    }

    /// Fills a placeholder slot; aggregates start as the node's own sizes.
    pub fn fill(&mut self, id: NodeId, init: NodeInit) {
        let i = id.0 as usize;
        self.parent[i] = init.parent.0;
        self.name[i] = init.name;
        self.kind[i] = init.kind;
        self.flags[i] = init.flags;
        self.own_disk[i] = init.own_disk;
        self.own_apparent[i] = init.own_apparent;
        self.agg_disk[i] = init.own_disk;
        self.agg_apparent[i] = init.own_apparent;
        self.nlink[i] = init.nlink;
        self.ino[i] = init.ino;
        self.dev[i] = init.dev;
    }

    /// `id` and then each ancestor up to the root.
    pub fn chain(&self, id: NodeId) -> Chain<'_> {
        Chain {
            tree: self,
            next: Some(id),
        }
    }
}

pub struct Chain<'a> {
    tree: &'a Tree,
    next: Option<NodeId>,
}

impl Iterator for Chain<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let cur = self.next?;
        let parent = self.tree.parent[cur.0 as usize];
        self.next = (parent != NO_PARENT).then_some(NodeId(parent));
        Some(cur)
    }
}
