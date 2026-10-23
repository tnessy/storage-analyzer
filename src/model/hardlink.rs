//! Hard-link index: which node is charged for each `(dev, ino)` with `nlink > 1`.

use std::collections::HashMap;

use smallvec::SmallVec;

use crate::types::NodeId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HardLink {
    pub counted: NodeId,
    pub others: SmallVec<[NodeId; 1]>,
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
