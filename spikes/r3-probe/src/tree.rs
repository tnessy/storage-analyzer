use rand::Rng;

pub struct Child<N> {
    pub node: N,
    /// Only directories on the root's filesystem are descended into.
    pub descend: bool,
}

pub trait Tree {
    type Node;
    fn root(&self) -> Self::Node;
    /// Entries of a directory. Unreadable directories yield no children.
    fn children(&self, node: &Self::Node) -> Vec<Child<Self::Node>>;
}

/// One Knuth descent: N = Σ_d Π_{i≤d} b_i, one directory read per level.
pub fn probe_once<T: Tree>(tree: &T, rng: &mut impl Rng) -> f64 {
    let mut node = tree.root();
    let mut product = 1.0;
    let mut estimate = 0.0;
    loop {
        let mut kids = tree.children(&node);
        if kids.is_empty() {
            return estimate;
        }
        product *= kids.len() as f64;
        estimate += product;
        let pick = kids.swap_remove(rng.random_range(0..kids.len()));
        if !pick.descend {
            return estimate;
        }
        node = pick.node;
    }
}

/// Variant B: descend only into subdirectories, X(v) = e(v) + s(v)·X(child).
/// Unbiased for the entry count below the root, and never ends at a file.
pub fn probe_once_subdirs<T: Tree>(tree: &T, rng: &mut impl Rng) -> f64 {
    let mut node = tree.root();
    let mut weight = 1.0;
    let mut estimate = 0.0;
    loop {
        let kids = tree.children(&node);
        estimate += weight * kids.len() as f64;
        let mut subdirs: Vec<_> = kids.into_iter().filter(|c| c.descend).collect();
        if subdirs.is_empty() {
            return estimate;
        }
        weight *= subdirs.len() as f64;
        let i = rng.random_range(0..subdirs.len());
        node = subdirs.swap_remove(i).node;
    }
}

/// Shallow-first scan. At the first moment `seen` reaches each fraction of
/// `total`, records `(seen, pending_dirs)`.
pub fn bfs_checkpoints<T: Tree>(tree: &T, total: u64, fractions: &[f64]) -> Vec<(u64, u64)> {
    let mut queue = std::collections::VecDeque::from([tree.root()]);
    let (mut seen, mut next) = (0u64, 0);
    let mut out = Vec::new();
    while let Some(node) = queue.pop_front() {
        for child in tree.children(&node) {
            seen += 1;
            if child.descend {
                queue.push_back(child.node);
            }
        }
        while next < fractions.len() && seen as f64 >= fractions[next] * total as f64 {
            out.push((seen, queue.len() as u64));
            next += 1;
        }
    }
    out
}

/// Full read-only walk; counts every entry below the root.
pub fn true_count<T: Tree>(tree: &T) -> u64 {
    let mut count = 0;
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        for child in tree.children(&node) {
            count += 1;
            if child.descend {
                stack.push(child.node);
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    struct Uniform {
        b: usize,
        depth: u32,
    }

    impl Tree for Uniform {
        type Node = u32;
        fn root(&self) -> u32 {
            0
        }
        fn children(&self, d: &u32) -> Vec<Child<u32>> {
            if *d >= self.depth {
                return vec![];
            }
            (0..self.b)
                .map(|_| Child {
                    node: d + 1,
                    descend: d + 1 < self.depth,
                })
                .collect()
        }
    }

    /// Branching factor depends on a hash of the node id, with rare fat directories.
    struct Skewed {
        depth: u32,
    }

    fn mix(x: u64) -> u64 {
        let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    impl Tree for Skewed {
        type Node = (u64, u32);
        fn root(&self) -> (u64, u32) {
            (1, 0)
        }
        fn children(&self, &(id, d): &(u64, u32)) -> Vec<Child<(u64, u32)>> {
            if d >= self.depth {
                return vec![];
            }
            let h = mix(id);
            let b = if h.is_multiple_of(10) {
                20
            } else {
                1 + (h >> 8) % 4
            };
            (0..b)
                .map(|i| Child {
                    node: (mix(id ^ (i + 1) << 20), d + 1),
                    descend: d + 1 < self.depth,
                })
                .collect()
        }
    }

    #[test]
    fn uniform_tree_probe_is_exact_for_any_seed() {
        let t = Uniform { b: 4, depth: 5 };
        let truth: u64 = (1..=5).map(|k| 4u64.pow(k)).sum();
        assert_eq!(true_count(&t), truth);
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            assert_eq!(probe_once(&t, &mut rng), truth as f64);
        }
    }

    #[test]
    fn subdir_probe_exact_on_uniform_tree() {
        let t = Uniform { b: 4, depth: 5 };
        let truth: u64 = (1..=5).map(|k| 4u64.pow(k)).sum();
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            assert_eq!(probe_once_subdirs(&t, &mut rng), truth as f64);
        }
    }

    #[test]
    fn subdir_probe_is_unbiased_on_skewed_tree() {
        let t = Skewed { depth: 6 };
        let truth = true_count(&t) as f64;
        let n = 2000;
        let mean: f64 = (0..n)
            .map(|s| probe_once_subdirs(&t, &mut StdRng::seed_from_u64(s)))
            .sum::<f64>()
            / n as f64;
        assert!(
            (mean / truth - 1.0).abs() < 0.05,
            "mean={mean} truth={truth}"
        );
    }

    #[test]
    fn bfs_checkpoints_on_uniform_tree() {
        let t = Uniform { b: 2, depth: 3 }; // 2 + 4 + 8 = 14 entries
        let cp = bfs_checkpoints(&t, 14, &[0.1, 0.5, 1.0]);
        // After the root: seen 2, 2 dirs pending. Seen first reaches 7 after the third
        // dir (the first depth-2 dir): seen 8, 3 pending. The last dir ends at 14.
        assert_eq!(cp, vec![(2, 2), (8, 3), (14, 0)]);
    }

    #[test]
    fn skewed_tree_probe_is_unbiased() {
        let t = Skewed { depth: 6 };
        let truth = true_count(&t) as f64;
        assert!(truth > 100.0);
        let n = 2000;
        let mean: f64 = (0..n)
            .map(|s| probe_once(&t, &mut StdRng::seed_from_u64(s)))
            .sum::<f64>()
            / n as f64;
        assert!(
            (mean / truth - 1.0).abs() < 0.05,
            "mean={mean} truth={truth}"
        );
    }
}
