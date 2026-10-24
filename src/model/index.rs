//! Incremental per-extension totals and top-N file lists.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use super::{ExtKey, ExtTotals};
use crate::types::NodeId;

/// Longest extension kept as its own key; longer ones map to `Other`.
const MAX_EXT_BYTES: usize = 16;

/// The extension key of a file name (design §3.4).
pub fn ext_of(name: &[u8]) -> ExtKey {
    match name.iter().rposition(|&b| b == b'.') {
        None | Some(0) => ExtKey::NoExt,
        Some(dot) => {
            let ext = &name[dot + 1..];
            if ext.is_empty() {
                ExtKey::NoExt
            } else if ext.len() > MAX_EXT_BYTES {
                ExtKey::Other
            } else {
                ExtKey::Ext(ext.to_ascii_lowercase().into())
            }
        }
    }
}

/// How many extra entries a list keeps beyond `n`, as a multiple of `n`.
const RESERVE_FACTOR: usize = 4;

type Key = (u64, Reverse<NodeId>);

/// The `n` largest entries, bigger first, then smaller `NodeId` first, plus a reserve of
/// `RESERVE_FACTOR * n` more so that removing a few of the largest needs no rebuild.
///
/// `bound` is the largest key the heap ever evicted. Everything above it is in the heap,
/// so those entries are exact: the true ranking from the top, with nothing missing.
/// Entries at or below `bound` are kept but not trusted (a freed slot must not be filled
/// by a small file while bigger evicted ones exist elsewhere).
#[derive(Debug)]
pub struct TopN {
    n: usize,
    /// A min-heap on the key: the root is the entry to evict.
    heap: BinaryHeap<Reverse<Key>>,
    bound: Option<Key>,
}

impl TopN {
    pub fn new(n: usize) -> TopN {
        TopN {
            n,
            heap: BinaryHeap::new(),
            bound: None,
        }
    }

    pub fn offer(&mut self, size: u64, id: NodeId) {
        self.heap.push(Reverse((size, Reverse(id))));
        if self.heap.len() > self.n * (1 + RESERVE_FACTOR)
            && let Some(Reverse(evicted)) = self.heap.pop()
        {
            self.bound = self.bound.max(Some(evicted));
        }
    }

    pub fn limit(&self) -> usize {
        self.n
    }

    /// Takes these nodes out (they were deleted or changed).
    pub fn forget(&mut self, ids: &HashSet<NodeId>) {
        let kept: Vec<_> = std::mem::take(&mut self.heap)
            .into_vec()
            .into_iter()
            .filter(|Reverse((_, Reverse(id)))| !ids.contains(id))
            .collect();
        self.heap = BinaryHeap::from(kept);
    }

    fn exact(&self) -> impl Iterator<Item = Key> + '_ {
        self.heap
            .iter()
            .map(|r| r.0)
            .filter(|k| self.bound.is_none_or(|b| *k > b))
    }

    /// Entries known to be in the true ranking from the top.
    pub fn exact_len(&self) -> usize {
        self.exact().count()
    }

    /// The exact ranking cannot show `n` entries although more files exist.
    pub fn needs_rebuild(&self) -> bool {
        self.bound.is_some() && self.exact_len() < self.n
    }

    /// The `n` largest, biggest first.
    pub fn sorted(&self) -> Vec<NodeId> {
        let mut entries: Vec<Key> = self.exact().collect();
        entries.sort_unstable_by(|a, b| b.cmp(a));
        entries.truncate(self.n);
        entries.into_iter().map(|(_, Reverse(id))| id).collect()
    }
}

/// Top-N lists being rebuilt node by node, a few at a time.
#[derive(Debug)]
struct Rebuild {
    /// Nodes `next..end` still have to be looked at.
    next: usize,
    end: usize,
    disk: TopN,
    apparent: TopN,
}

#[derive(Debug)]
pub struct Indexes {
    exts: HashMap<ExtKey, ExtTotals>,
    pub top_disk: TopN,
    pub top_apparent: TopN,
    rebuild: Option<Rebuild>,
    epoch: u64,
}

impl Indexes {
    pub fn new(top_n: usize) -> Indexes {
        Indexes {
            exts: HashMap::new(),
            top_disk: TopN::new(top_n),
            top_apparent: TopN::new(top_n),
            rebuild: None,
            epoch: 0,
        }
    }

    /// Adds one counted non-directory. `is_file` also puts it into the top lists.
    pub fn add(&mut self, name: &[u8], id: NodeId, disk: u64, apparent: u64, is_file: bool) {
        let totals = self.exts.entry(ext_of(name)).or_default();
        totals.disk_bytes += disk;
        totals.apparent_bytes += apparent;
        totals.count += 1;
        if is_file {
            self.offer_top(id, disk, apparent);
        }
    }

    /// Takes one counted non-directory back out of the extension totals.
    pub fn remove(&mut self, name: &[u8], disk: u64, apparent: u64) {
        let key = ext_of(name);
        let Some(totals) = self.exts.get_mut(&key) else {
            debug_assert!(false, "removing from a missing extension {key:?}");
            return;
        };
        debug_assert!(totals.count >= 1 && totals.disk_bytes >= disk);
        totals.disk_bytes -= disk;
        totals.apparent_bytes -= apparent;
        totals.count -= 1;
        if totals.count == 0 {
            self.exts.remove(&key);
        }
    }

    /// Offers a file to the live lists, and to a running rebuild unless the rebuild has
    /// yet to look at this node itself.
    pub fn offer_top(&mut self, id: NodeId, disk: u64, apparent: u64) {
        self.top_disk.offer(disk, id);
        self.top_apparent.offer(apparent, id);
        if let Some(r) = &mut self.rebuild
            && !(r.next..r.end).contains(&(id.0 as usize))
        {
            r.disk.offer(disk, id);
            r.apparent.offer(apparent, id);
        }
    }

    /// Takes deleted or changed files out of the live lists and of a running rebuild.
    pub fn forget_top(&mut self, ids: &HashSet<NodeId>) {
        if ids.is_empty() {
            return;
        }
        self.top_disk.forget(ids);
        self.top_apparent.forget(ids);
        if let Some(r) = &mut self.rebuild {
            r.disk.forget(ids);
            r.apparent.forget(ids);
        }
    }

    /// Empty lists, no rebuild: for a tree that was emptied (a rescan of the root).
    pub fn reset_top(&mut self) {
        self.top_disk = TopN::new(self.top_disk.limit());
        self.top_apparent = TopN::new(self.top_apparent.limit());
        self.rebuild = None;
        self.epoch += 1;
    }

    /// A list lost so many entries that it cannot show its top `n` any more.
    pub fn top_needs_rebuild(&self) -> bool {
        self.top_disk.needs_rebuild() || self.top_apparent.needs_rebuild()
    }

    /// Starts (or restarts) a rebuild that will look at nodes `0..end`.
    pub fn begin_rebuild(&mut self, end: usize) {
        self.rebuild = Some(Rebuild {
            next: 0,
            end,
            disk: TopN::new(self.top_disk.limit()),
            apparent: TopN::new(self.top_apparent.limit()),
        });
    }

    /// The nodes the running rebuild has yet to look at.
    pub fn rebuild_range(&self) -> Option<(usize, usize)> {
        self.rebuild.as_ref().map(|r| (r.next, r.end))
    }

    /// A node the rebuild looked at counts for the new lists.
    pub fn rebuild_offer(&mut self, id: NodeId, disk: u64, apparent: u64) {
        if let Some(r) = &mut self.rebuild {
            r.disk.offer(disk, id);
            r.apparent.offer(apparent, id);
        }
    }

    pub fn rebuild_advance(&mut self, next: usize) {
        if let Some(r) = &mut self.rebuild {
            r.next = next;
        }
    }

    /// Replaces the live lists by the rebuilt ones.
    pub fn finish_rebuild(&mut self) {
        if let Some(r) = self.rebuild.take() {
            self.top_disk = r.disk;
            self.top_apparent = r.apparent;
            self.epoch += 1;
        }
    }

    pub fn rebuilding(&self) -> bool {
        self.rebuild.is_some()
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Largest disk size first, then key ascending.
    pub fn ext_totals(&self) -> Vec<(ExtKey, ExtTotals)> {
        let mut all: Vec<_> = self.exts.iter().map(|(k, t)| (k.clone(), *t)).collect();
        all.sort_by(|a, b| {
            b.1.disk_bytes
                .cmp(&a.1.disk_bytes)
                .then_with(|| a.0.cmp(&b.0))
        });
        all
    }
}
