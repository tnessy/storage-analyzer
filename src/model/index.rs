//! Incremental per-extension totals and top-N file lists.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

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

/// The `n` largest entries seen: bigger first, then smaller `NodeId` first.
#[derive(Debug)]
pub struct TopN {
    n: usize,
    /// A min-heap on `(size, Reverse(id))`: the root is the entry to evict.
    heap: BinaryHeap<Reverse<(u64, Reverse<NodeId>)>>,
}

impl TopN {
    pub fn new(n: usize) -> TopN {
        TopN {
            n,
            heap: BinaryHeap::new(),
        }
    }

    pub fn offer(&mut self, size: u64, id: NodeId) {
        self.heap.push(Reverse((size, Reverse(id))));
        if self.heap.len() > self.n {
            self.heap.pop();
        }
    }

    pub fn clear(&mut self) {
        self.heap.clear();
    }

    pub fn sorted(&self) -> Vec<NodeId> {
        let mut entries: Vec<_> = self.heap.iter().map(|r| r.0).collect();
        entries.sort_unstable_by(|a, b| b.cmp(a));
        entries.into_iter().map(|(_, Reverse(id))| id).collect()
    }
}

#[derive(Debug)]
pub struct Indexes {
    exts: HashMap<ExtKey, ExtTotals>,
    pub top_disk: TopN,
    pub top_apparent: TopN,
}

impl Indexes {
    pub fn new(top_n: usize) -> Indexes {
        Indexes {
            exts: HashMap::new(),
            top_disk: TopN::new(top_n),
            top_apparent: TopN::new(top_n),
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

    pub fn offer_top(&mut self, id: NodeId, disk: u64, apparent: u64) {
        self.top_disk.offer(disk, id);
        self.top_apparent.offer(apparent, id);
    }

    pub fn clear_top(&mut self) {
        self.top_disk.clear();
        self.top_apparent.clear();
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
