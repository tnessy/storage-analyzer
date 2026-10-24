//! Hard-link index: which node is charged for each `(dev, ino)` with `nlink > 1`.

use std::collections::HashMap;

use smallvec::SmallVec;

use crate::types::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardLink {
    pub counted: NodeId,
    pub others: SmallVec<[NodeId; 1]>,
}

/// What `HardLinks::remove` found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removal {
    NotFound,
    /// It was an uncharged duplicate.
    Duplicate,
    /// It was the charged occurrence; this duplicate is charged from now on, if any.
    Counted(Option<NodeId>),
}

/// The table is split into this many hash maps: a map that grows rehashes all of itself
/// in one step, and one huge map would stall a single `apply` for tens of milliseconds.
const SHARDS: usize = 256;

type Table = HashMap<(u64, u64), HardLink>;

#[derive(Debug)]
pub struct HardLinks {
    shards: Vec<Table>,
}

impl Default for HardLinks {
    fn default() -> Self {
        HardLinks {
            shards: (0..SHARDS).map(|_| Table::new()).collect(),
        }
    }
}

impl HardLinks {
    fn shard(&mut self, dev: u64, ino: u64) -> &mut Table {
        let mixed =
            (ino ^ dev.wrapping_mul(0x9E37_79B9_7F4A_7C15)).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        &mut self.shards[(mixed >> 56) as usize % SHARDS]
    }

    /// Registers an occurrence. Returns `true` if it is a duplicate, which is not charged.
    pub fn register(&mut self, dev: u64, ino: u64, node: NodeId) -> bool {
        let map = self.shard(dev, ino);
        match map.get_mut(&(dev, ino)) {
            Some(link) => {
                link.others.push(node);
                true
            }
            None => {
                map.insert(
                    (dev, ino),
                    HardLink {
                        counted: node,
                        others: SmallVec::new(),
                    },
                );
                false
            }
        }
    }

    /// Drops `node` from the table. If it was the charged occurrence, the first `alive`
    /// duplicate takes over and is returned.
    pub fn remove(
        &mut self,
        dev: u64,
        ino: u64,
        node: NodeId,
        alive: impl Fn(NodeId) -> bool,
    ) -> Removal {
        let map = self.shard(dev, ino);
        let Some(link) = map.get_mut(&(dev, ino)) else {
            return Removal::NotFound;
        };
        if link.counted != node {
            let before = link.others.len();
            link.others.retain(|n| *n != node);
            return if link.others.len() < before {
                Removal::Duplicate
            } else {
                Removal::NotFound
            };
        }
        match link.others.iter().position(|n| alive(*n)) {
            Some(at) => {
                link.counted = link.others.remove(at);
                Removal::Counted(Some(link.counted))
            }
            None => {
                map.remove(&(dev, ino));
                Removal::Counted(None)
            }
        }
    }

    #[cfg(test)]
    pub fn largest_table(&self) -> usize {
        self.shards.iter().map(Table::len).max().unwrap_or(0)
    }

    /// Read side for T011's `delete_check`; drop the `cfg` there.
    #[cfg(test)]
    pub fn get(&mut self, dev: u64, ino: u64) -> Option<&HardLink> {
        self.shard(dev, ino).get(&(dev, ino))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_spread_over_many_small_tables_so_none_rehashes_in_one_big_step() {
        let mut h = HardLinks::default();
        for i in 0..100_000u64 {
            h.register(1, 1_000_000 + i * 3, NodeId(i as u32));
        }
        // Growing a table rehashes all of it; keeping each one small keeps that step short.
        assert!(
            h.largest_table() < 800,
            "largest table {}",
            h.largest_table()
        );
        for i in (0..100_000u64).step_by(7919) {
            assert_eq!(
                h.get(1, 1_000_000 + i * 3).unwrap().counted,
                NodeId(i as u32)
            );
        }
    }

    #[test]
    fn removing_the_charged_link_promotes_the_first_live_duplicate() {
        let mut h = HardLinks::default();
        for n in 1..=4 {
            h.register(1, 7, NodeId(n));
        }
        assert_eq!(h.remove(1, 7, NodeId(3), |_| true), Removal::Duplicate);
        assert_eq!(
            h.remove(1, 7, NodeId(1), |n| n != NodeId(2)),
            Removal::Counted(Some(NodeId(4)))
        );
        assert_eq!(h.get(1, 7).unwrap().counted, NodeId(4));
        assert_eq!(h.remove(1, 7, NodeId(4), |_| false), Removal::Counted(None));
        assert!(h.get(1, 7).is_none());
        assert_eq!(h.remove(1, 7, NodeId(2), |_| true), Removal::NotFound);
    }

    #[test]
    fn first_occurrence_is_charged_later_ones_are_duplicates() {
        let mut h = HardLinks::default();
        assert!(!h.register(1, 7, NodeId(3)));
        assert!(h.register(1, 7, NodeId(4)));
        assert!(h.register(1, 7, NodeId(5)));
        assert!(
            !h.register(2, 7, NodeId(6)),
            "same ino on another device is distinct"
        );
        let link = h.get(1, 7).unwrap();
        assert_eq!(link.counted, NodeId(3));
        assert_eq!(link.others.as_slice(), &[NodeId(4), NodeId(5)]);
        assert!(h.get(9, 9).is_none());
    }
}
