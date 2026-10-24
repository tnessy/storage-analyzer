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

#[derive(Debug, Default)]
pub struct HardLinks {
    map: HashMap<(u64, u64), HardLink>,
}

impl HardLinks {
    /// Registers an occurrence. Returns `true` if it is a duplicate, which is not charged.
    pub fn register(&mut self, dev: u64, ino: u64, node: NodeId) -> bool {
        match self.map.get_mut(&(dev, ino)) {
            Some(link) => {
                link.others.push(node);
                true
            }
            None => {
                self.map.insert(
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
        let Some(link) = self.map.get_mut(&(dev, ino)) else {
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
                self.map.remove(&(dev, ino));
                Removal::Counted(None)
            }
        }
    }

    /// Read side for T011's `delete_check`; drop the `cfg` there.
    #[cfg(test)]
    pub fn get(&self, dev: u64, ino: u64) -> Option<&HardLink> {
        self.map.get(&(dev, ino))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
