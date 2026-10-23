use super::{Direction, Tile};

/// The tile adjacent to `tiles[from]` in `dir` with the largest overlap on the shared axis.
/// Ties go to the nearest centre, then the lowest index.
pub fn neighbor(tiles: &[Tile], from: usize, dir: Direction) -> Option<usize> {
    let src = tiles.get(from)?.rect;
    // Work in (along, across) coordinates: `along` is the axis of travel.
    let (src_edge_far, src_span, src_centre2) = match dir {
        Direction::Right => (
            src.right(),
            span(src.y, src.height),
            2 * src.y as u32 + src.height as u32,
        ),
        Direction::Left => (
            src.x,
            span(src.y, src.height),
            2 * src.y as u32 + src.height as u32,
        ),
        Direction::Down => (
            src.bottom(),
            span(src.x, src.width),
            2 * src.x as u32 + src.width as u32,
        ),
        Direction::Up => (
            src.y,
            span(src.x, src.width),
            2 * src.x as u32 + src.width as u32,
        ),
    };
    tiles
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != from)
        .filter_map(|(i, t)| {
            let r = t.rect;
            let (adjacent, cand_span, cand_centre2) = match dir {
                Direction::Right => (
                    r.x == src_edge_far,
                    span(r.y, r.height),
                    2 * r.y as u32 + r.height as u32,
                ),
                Direction::Left => (
                    r.right() == src_edge_far,
                    span(r.y, r.height),
                    2 * r.y as u32 + r.height as u32,
                ),
                Direction::Down => (
                    r.y == src_edge_far,
                    span(r.x, r.width),
                    2 * r.x as u32 + r.width as u32,
                ),
                Direction::Up => (
                    r.bottom() == src_edge_far,
                    span(r.x, r.width),
                    2 * r.x as u32 + r.width as u32,
                ),
            };
            let overlap = overlap(src_span, cand_span);
            (adjacent && overlap > 0).then(|| (overlap, src_centre2.abs_diff(cand_centre2), i))
        })
        .min_by_key(|&(overlap, dist, i)| (std::cmp::Reverse(overlap), dist, i))
        .map(|(_, _, i)| i)
}

fn span(start: u16, len: u16) -> (u32, u32) {
    (start as u32, start as u32 + len as u32)
}

fn overlap(a: (u32, u32), b: (u32, u32)) -> u32 {
    a.1.min(b.1).saturating_sub(a.0.max(b.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeId;
    use ratatui::layout::Rect;

    fn t(n: u32, x: u16, y: u16, w: u16, h: u16) -> Tile {
        Tile {
            id: Some(NodeId(n)),
            rect: Rect::new(x, y, w, h),
        }
    }

    #[test]
    fn two_by_two_grid() {
        let tiles = [
            t(0, 0, 0, 5, 5),
            t(1, 5, 0, 5, 5),
            t(2, 0, 5, 5, 5),
            t(3, 5, 5, 5, 5),
        ];
        assert_eq!(neighbor(&tiles, 0, Direction::Right), Some(1));
        assert_eq!(neighbor(&tiles, 0, Direction::Down), Some(2));
        assert_eq!(neighbor(&tiles, 3, Direction::Left), Some(2));
        assert_eq!(neighbor(&tiles, 3, Direction::Up), Some(1));
        assert_eq!(neighbor(&tiles, 0, Direction::Left), None);
        assert_eq!(neighbor(&tiles, 0, Direction::Up), None);
        assert_eq!(neighbor(&tiles, 3, Direction::Right), None);
        assert_eq!(neighbor(&tiles, 3, Direction::Down), None);
    }

    #[test]
    fn l_shaped_layout_picks_largest_overlap() {
        // 0: tall left column. 1: top right (short). 2: bottom right (tall).
        let tiles = [t(0, 0, 0, 4, 10), t(1, 4, 0, 6, 3), t(2, 4, 3, 6, 7)];
        // From the tall column, the right neighbour overlapping most is tile 2 (7 rows vs 3).
        assert_eq!(neighbor(&tiles, 0, Direction::Right), Some(2));
        assert_eq!(neighbor(&tiles, 1, Direction::Left), Some(0));
        assert_eq!(neighbor(&tiles, 2, Direction::Left), Some(0));
        assert_eq!(neighbor(&tiles, 1, Direction::Down), Some(2));
        assert_eq!(neighbor(&tiles, 2, Direction::Up), Some(1));
        assert_eq!(neighbor(&tiles, 1, Direction::Right), None);
    }

    #[test]
    fn equal_overlap_prefers_nearest_centre() {
        // Source rows 0..6 (centre 3). Right neighbours overlap 2 rows each; tile 2 is centred on 3.
        let tiles = [
            t(0, 0, 0, 4, 6),
            t(1, 4, 0, 4, 2),
            t(2, 4, 2, 4, 2),
            t(3, 4, 4, 4, 2),
        ];
        assert_eq!(neighbor(&tiles, 0, Direction::Right), Some(2));
    }

    #[test]
    fn equal_overlap_and_distance_prefers_lowest_index() {
        // Source rows 2..8 (centre 5); candidates rows 0..5 and 5..10 overlap 3 each, centres 2.5 and 7.5.
        let tiles = [t(0, 0, 2, 4, 6), t(2, 4, 5, 4, 5), t(1, 4, 0, 4, 5)];
        assert_eq!(neighbor(&tiles, 0, Direction::Right), Some(1));
    }

    #[test]
    fn out_of_range_index_is_none() {
        assert_eq!(neighbor(&[], 0, Direction::Up), None);
    }
}
