//! Which colour each treemap tile gets (UX §7, option C): files by type, folders by identity.
//!
//! Pure and deterministic: no terminal, no model.

use std::collections::HashMap;

use ratatui::layout::Rect;

use crate::types::NodeId;

/// The Okabe-Ito hues, in folder order (grey is reserved for the aggregate tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hue {
    Yellow,
    Blue,
    Orange,
    BluishGreen,
    SkyBlue,
    Vermillion,
    ReddishPurple,
    Grey,
}

impl Hue {
    /// The hues folders may use, in the order they are tried.
    pub const FOLDER_ORDER: [Hue; 7] = [
        Hue::Yellow,
        Hue::Blue,
        Hue::Orange,
        Hue::BluishGreen,
        Hue::SkyBlue,
        Hue::Vermillion,
        Hue::ReddishPurple,
    ];

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of hues.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HueSet(u8);

impl HueSet {
    pub fn contains(self, hue: Hue) -> bool {
        self.0 & hue.bit() != 0
    }

    pub fn insert(&mut self, hue: Hue) {
        self.0 |= hue.bit();
    }
}

/// What the map remembers about colours between relayouts (UX §7).
///
/// - `reserved`: per scope folder, the file-type hues of its direct file children, including files
///   too small for a tile. It only grows; a rescan of the folder rebuilds it (`forget_reserved`).
/// - `folder_colors`: the hue each folder was last given, for the whole session.
///
/// Both grow only with the folders actually shown.
#[derive(Debug, Default)]
pub struct TileColors {
    pub reserved: HashMap<NodeId, HueSet>,
    pub folder_colors: HashMap<NodeId, Hue>,
}

impl TileColors {
    /// Adds `hues` to the scope folder's reserved set.
    pub fn reserve(&mut self, scope: NodeId, hues: impl IntoIterator<Item = Hue>) -> HueSet {
        let set = self.reserved.entry(scope).or_default();
        for hue in hues {
            set.insert(hue);
        }
        *set
    }

    /// Forgets a scope folder's reserved set, so the next relayout rebuilds it (a rescan).
    pub fn forget_reserved(&mut self, scope: NodeId) {
        self.reserved.remove(&scope);
    }
}

/// What a tile is, for colouring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    File(Hue),
    Folder(NodeId),
    Aggregate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Hue(Hue),
    /// A folder with no free hue: drawn on the default background.
    Unfilled,
}

fn shares_border(a: Rect, b: Rect) -> bool {
    let overlap = |a0: u16, a1: u16, b0: u16, b1: u16| a1.min(b1) > a0.max(b0);
    ((a.right() == b.x || b.right() == a.x) && overlap(a.y, a.bottom(), b.y, b.bottom()))
        || ((a.bottom() == b.y || b.bottom() == a.y) && overlap(a.x, a.right(), b.x, b.right()))
}

/// For each tile, the earlier tiles (lower index) that share a border segment with it.
pub fn adjacency(rects: &[Rect]) -> Vec<Vec<usize>> {
    (0..rects.len())
        .map(|i| {
            (0..i)
                .filter(|&j| shares_border(rects[i], rects[j]))
                .collect()
        })
        .collect()
}

/// Colours the tiles in layout order. `memory` maps folders to the hue they were given before;
/// the returned map is `memory` plus this pass's choices.
pub fn assign(
    kinds: &[TileKind],
    adjacency: &[Vec<usize>],
    reserved: HueSet,
    memory: &HashMap<NodeId, Hue>,
) -> (Vec<Fill>, HashMap<NodeId, Hue>) {
    let mut memory = memory.clone();
    let mut fills: Vec<Fill> = Vec::with_capacity(kinds.len());
    let free: Vec<Hue> = Hue::FOLDER_ORDER
        .into_iter()
        .filter(|&h| !reserved.contains(h))
        .collect();
    for (i, kind) in kinds.iter().enumerate() {
        let fill = match *kind {
            TileKind::File(hue) => Fill::Hue(hue),
            TileKind::Aggregate => Fill::Hue(Hue::Grey),
            TileKind::Folder(id) => {
                let near: Vec<Hue> = adjacency[i]
                    .iter()
                    .filter_map(|&n| match fills[n] {
                        Fill::Hue(h) => Some(h),
                        Fill::Unfilled => None,
                    })
                    .collect();
                match pick_folder_hue(id, &free, &near, memory.get(&id).copied()) {
                    Some(hue) => {
                        memory.insert(id, hue);
                        Fill::Hue(hue)
                    }
                    None => Fill::Unfilled,
                }
            }
        };
        fills.push(fill);
    }
    (fills, memory)
}

fn pick_folder_hue(id: NodeId, free: &[Hue], near: &[Hue], remembered: Option<Hue>) -> Option<Hue> {
    if free.is_empty() {
        return None;
    }
    let usable = |h: Hue| free.contains(&h) && !near.contains(&h);
    if let Some(h) = remembered.filter(|&h| usable(h)) {
        return Some(h);
    }
    let start = id.0 as usize % Hue::FOLDER_ORDER.len();
    let order = (0..Hue::FOLDER_ORDER.len())
        .map(|k| Hue::FOLDER_ORDER[(start + k) % Hue::FOLDER_ORDER.len()]);
    if let Some(h) = order.into_iter().find(|&h| usable(h)) {
        return Some(h);
    }
    // Every free hue touches this tile: the least used one, lowest folder order on a tie.
    free.iter()
        .copied()
        .min_by_key(|h| near.iter().filter(|n| *n == h).count())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_FOLDER_HUES: usize = 7;

    fn folder(id: u32) -> TileKind {
        TileKind::Folder(NodeId(id))
    }

    fn run(kinds: &[TileKind], adj: &[Vec<usize>], reserved: HueSet) -> Vec<Fill> {
        assign(kinds, adj, reserved, &HashMap::new()).0
    }

    #[test]
    fn adjacency_lists_earlier_tiles_sharing_a_border_segment() {
        let rects = [
            Rect::new(0, 0, 10, 5),
            Rect::new(10, 0, 10, 5),
            Rect::new(0, 5, 20, 5),
            Rect::new(30, 0, 5, 5),
        ];
        assert_eq!(adjacency(&rects), vec![vec![], vec![0], vec![0, 1], vec![]]);
        // Touching only at a corner is not a shared segment.
        let corner = [Rect::new(0, 0, 5, 5), Rect::new(5, 5, 5, 5)];
        assert_eq!(adjacency(&corner), vec![Vec::<usize>::new(), Vec::new()]);
    }

    #[test]
    fn file_tiles_take_their_type_hue_and_the_aggregate_is_grey() {
        let kinds = [
            TileKind::File(Hue::Orange),
            TileKind::File(Hue::Yellow),
            TileKind::Aggregate,
        ];
        let fills = run(&kinds, &[vec![], vec![0], vec![1]], HueSet::default());
        assert_eq!(
            fills,
            [
                Fill::Hue(Hue::Orange),
                Fill::Hue(Hue::Yellow),
                Fill::Hue(Hue::Grey)
            ]
        );
    }

    #[test]
    fn a_folder_starts_at_its_node_id_modulo_seven() {
        let fills = run(&[folder(9)], &[vec![]], HueSet::default());
        assert_eq!(fills, [Fill::Hue(Hue::FOLDER_ORDER[2])]);
        assert_eq!(Hue::FOLDER_ORDER[2], Hue::Orange);
        let fills = run(&[folder(14)], &[vec![]], HueSet::default());
        assert_eq!(fills, [Fill::Hue(Hue::Yellow)]);
    }

    #[test]
    fn reserved_hues_are_skipped_and_the_search_wraps() {
        let mut reserved = HueSet::default();
        reserved.insert(Hue::Orange);
        reserved.insert(Hue::BluishGreen);
        // NodeId 9 starts at orange (reserved), then bluish green (reserved), then sky blue.
        assert_eq!(
            run(&[folder(9)], &[vec![]], reserved),
            [Fill::Hue(Hue::SkyBlue)]
        );
        // Starting at reddish purple (6) and wrapping to yellow.
        let mut purple = HueSet::default();
        purple.insert(Hue::ReddishPurple);
        assert_eq!(
            run(&[folder(6)], &[vec![]], purple),
            [Fill::Hue(Hue::Yellow)]
        );
    }

    #[test]
    fn neighbours_never_share_a_folder_hue_while_free_hues_remain() {
        // Six same-start siblings in a row: each touches the previous one.
        let kinds: Vec<_> = (0..6).map(|i| folder(7 * i)).collect(); // all start at yellow
        let adj: Vec<Vec<usize>> = (0..6)
            .map(|i| if i == 0 { vec![] } else { vec![i - 1] })
            .collect();
        let fills = run(&kinds, &adj, HueSet::default());
        for i in 1..6 {
            assert_ne!(fills[i], fills[i - 1], "tiles {} and {}", i - 1, i);
        }
        // A 7-tile grid (3 columns): every pair of edge neighbours differs.
        let rects: Vec<Rect> = (0..7)
            .map(|i| Rect::new((i % 3) * 10, (i / 3) * 5, 10, 5))
            .collect();
        let kinds: Vec<_> = (0..7).map(|_| folder(0)).collect();
        let fills = run(&kinds, &adjacency(&rects), HueSet::default());
        for (i, near) in adjacency(&rects).iter().enumerate() {
            for &j in near {
                assert_ne!(fills[i], fills[j], "grid tiles {j} and {i}");
            }
        }
    }

    #[test]
    fn folders_avoid_the_hues_of_neighbouring_file_tiles_too() {
        let kinds = [TileKind::File(Hue::Yellow), folder(0)];
        let fills = run(&kinds, &[vec![], vec![0]], HueSet::default());
        assert_eq!(
            fills[1],
            Fill::Hue(Hue::Blue),
            "yellow is taken by the file next to it"
        );
    }

    #[test]
    fn memory_keeps_a_free_hue_that_no_neighbour_uses() {
        let mut memory = HashMap::new();
        memory.insert(NodeId(0), Hue::Vermillion);
        let (fills, after) = assign(&[folder(0)], &[vec![]], HueSet::default(), &memory);
        assert_eq!(fills, [Fill::Hue(Hue::Vermillion)], "not the start hue");
        assert_eq!(after[&NodeId(0)], Hue::Vermillion);
        // Reserved now: it must move, once, and the new hue is remembered.
        let mut reserved = HueSet::default();
        reserved.insert(Hue::Vermillion);
        let (fills, after) = assign(&[folder(0)], &[vec![]], reserved, &memory);
        assert_eq!(fills, [Fill::Hue(Hue::Yellow)]);
        assert_eq!(after[&NodeId(0)], Hue::Yellow);
        // A neighbour already using the remembered hue also forces a move.
        let kinds = [TileKind::File(Hue::Vermillion), folder(0)];
        let (fills, _) = assign(&kinds, &[vec![], vec![0]], HueSet::default(), &memory);
        assert_eq!(fills[1], Fill::Hue(Hue::Yellow));
    }

    #[test]
    fn when_every_free_hue_is_taken_the_least_used_one_wins_lowest_order_on_ties() {
        let mut reserved = HueSet::default();
        for h in &Hue::FOLDER_ORDER[2..] {
            reserved.insert(*h);
        }
        // Free: yellow, blue. Neighbours: two yellow files and one blue file.
        let kinds = [
            TileKind::File(Hue::Yellow),
            TileKind::File(Hue::Yellow),
            TileKind::File(Hue::Blue),
            folder(0),
        ];
        let fills = run(&kinds, &[vec![], vec![], vec![], vec![0, 1, 2]], reserved);
        assert_eq!(fills[3], Fill::Hue(Hue::Blue), "used once versus twice");
        let kinds = [
            TileKind::File(Hue::Yellow),
            TileKind::File(Hue::Blue),
            folder(0),
        ];
        let fills = run(&kinds, &[vec![], vec![], vec![0, 1]], reserved);
        assert_eq!(fills[2], Fill::Hue(Hue::Yellow), "tie: lowest folder order");
    }

    #[test]
    fn with_every_type_reserved_folders_are_unfilled() {
        let mut reserved = HueSet::default();
        for h in Hue::FOLDER_ORDER {
            reserved.insert(h);
        }
        assert_eq!(reserved, {
            let mut all = HueSet::default();
            Hue::FOLDER_ORDER.iter().for_each(|h| all.insert(*h));
            all
        });
        let fills = run(
            &[folder(3), TileKind::File(Hue::Blue)],
            &[vec![], vec![0]],
            reserved,
        );
        assert_eq!(fills, [Fill::Unfilled, Fill::Hue(Hue::Blue)]);
        assert_eq!(ALL_FOLDER_HUES, Hue::FOLDER_ORDER.len());
    }

    #[test]
    fn the_same_input_and_memory_give_the_same_output() {
        let kinds: Vec<_> = (0..12).map(folder).collect();
        let rects: Vec<Rect> = (0..12)
            .map(|i| Rect::new((i % 4) * 8, (i / 4) * 4, 8, 4))
            .collect();
        let adj = adjacency(&rects);
        let a = assign(&kinds, &adj, HueSet::default(), &HashMap::new());
        let b = assign(&kinds, &adj, HueSet::default(), &HashMap::new());
        assert_eq!(a, b);
        // Feeding the memory back changes nothing.
        let c = assign(&kinds, &adj, HueSet::default(), &a.1);
        assert_eq!(a.0, c.0);
    }
}
