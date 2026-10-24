//! Struct-of-arrays node storage, indexed by `NodeId`.
//!
//! Ids are handed out by the scanner in blocks that may arrive out of order, so the
//! arrays can contain placeholders (no parent, not the root) until their block arrives.

use super::Flags;
use super::names::NameRef;
use crate::types::{Kind, NodeId};

pub const NO_PARENT: u32 = u32::MAX;
pub const NO_ISSUE: u32 = u32::MAX;

/// A growable array that never moves what it holds: fixed blocks of `CHUNK` elements.
/// Growing it allocates one more block and touches only the elements it adds, so no
/// single apply step has to copy or zero hundreds of megabytes.
pub struct Chunked<T> {
    chunks: Vec<Vec<T>>,
    len: usize,
}

pub const CHUNK_BITS: usize = 16;
pub const CHUNK: usize = 1 << CHUNK_BITS;

impl<T> Default for Chunked<T> {
    fn default() -> Self {
        Chunked {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl<T: Clone> Chunked<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn capacity(&self) -> usize {
        self.chunks.len() * CHUNK
    }

    /// Grows to `new_len` elements, each new one a copy of `fill`.
    pub fn resize(&mut self, new_len: usize, fill: T) {
        while self.len < new_len {
            if self.len == self.capacity() {
                self.chunks.push(Vec::with_capacity(CHUNK));
            }
            let room = self.capacity() - self.len;
            let take = room.min(new_len - self.len);
            let last = self.chunks.len() - 1;
            let chunk = &mut self.chunks[last];
            chunk.resize(chunk.len() + take, fill.clone());
            self.len += take;
        }
    }
}

impl<T> std::ops::Index<usize> for Chunked<T> {
    type Output = T;

    fn index(&self, i: usize) -> &T {
        &self.chunks[i >> CHUNK_BITS][i & (CHUNK - 1)]
    }
}

impl<T> std::ops::IndexMut<usize> for Chunked<T> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        &mut self.chunks[i >> CHUNK_BITS][i & (CHUNK - 1)]
    }
}

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
    pub parent: Chunked<u32>,
    pub name: Chunked<NameRef>,
    pub kind: Chunked<Kind>,
    pub flags: Chunked<Flags>,
    pub own_disk: Chunked<u64>,
    pub own_apparent: Chunked<u64>,
    pub agg_disk: Chunked<u64>,
    pub agg_apparent: Chunked<u64>,
    pub files: Chunked<u64>,
    pub dirs: Chunked<u64>,
    pub pending_dirs: Chunked<u32>,
    pub inexact_dirs: Chunked<u32>,
    pub nlink: Chunked<u32>,
    pub ino: Chunked<u64>,
    pub dev: Chunked<u64>,
    pub issue: Chunked<u32>,
    pub children: Chunked<Vec<NodeId>>,
}

impl Tree {
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn capacity(&self) -> usize {
        self.parent.capacity()
    }

    /// Grows the arrays to `len` entries with placeholders.
    pub fn ensure_len(&mut self, len: usize) {
        if len <= self.len() {
            return;
        }
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
        self.children.resize(len, Vec::new());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growing_never_moves_existing_elements() {
        let mut a: Chunked<u64> = Chunked::default();
        a.resize(10, 7);
        let first = std::ptr::from_ref(&a[0]);
        a.resize(CHUNK + 5, 7);
        let second_block = std::ptr::from_ref(&a[CHUNK + 1]);
        assert_eq!(
            first,
            std::ptr::from_ref(&a[0]),
            "the first block did not move"
        );
        a.resize(10 * CHUNK + 3, 9);
        assert_eq!(first, std::ptr::from_ref(&a[0]));
        assert_eq!(second_block, std::ptr::from_ref(&a[CHUNK + 1]));
        assert_eq!(a.len(), 10 * CHUNK + 3);
        assert_eq!(
            (
                a[0],
                a[9],
                a[10],
                a[CHUNK + 4],
                a[CHUNK + 5],
                a[10 * CHUNK + 2]
            ),
            (7, 7, 7, 7, 9, 9)
        );
    }

    #[test]
    fn elements_read_back_across_block_boundaries() {
        let mut a: Chunked<u32> = Chunked::default();
        a.resize(3 * CHUNK + 1, 0);
        for i in [0, CHUNK - 1, CHUNK, 2 * CHUNK - 1, 2 * CHUNK, 3 * CHUNK] {
            a[i] = i as u32 + 1;
        }
        for i in [0, CHUNK - 1, CHUNK, 2 * CHUNK - 1, 2 * CHUNK, 3 * CHUNK] {
            assert_eq!(a[i], i as u32 + 1);
        }
        assert_eq!(a[5], 0);
    }

    #[test]
    fn capacity_is_whole_blocks_and_resize_only_grows() {
        let mut a: Chunked<u8> = Chunked::default();
        assert_eq!((a.len(), a.capacity()), (0, 0));
        a.resize(1, 0);
        assert_eq!(a.capacity(), CHUNK);
        a.resize(CHUNK, 0);
        assert_eq!(a.capacity(), CHUNK);
        a.resize(CHUNK + 1, 0);
        assert_eq!(a.capacity(), 2 * CHUNK);
        a.resize(5, 0);
        assert_eq!(a.len(), CHUNK + 1, "never shrinks");
    }
}
