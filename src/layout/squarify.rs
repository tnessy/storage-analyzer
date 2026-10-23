use ratatui::layout::Rect;

use super::Tile;
use crate::types::NodeId;

/// Cell-space rectangle (x0, y0, x1, y1) of the exact float layout, relative to the area origin.
#[derive(Clone, Copy, Debug)]
struct FRect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

/// Items sorted by (size desc, id asc), zero sizes dropped, each with its area in the
/// aspect-corrected space (cell height = `cell_aspect`). Items under one cell are merged into a
/// trailing aggregate (`None`).
fn prepare(items: &[(NodeId, u64)], area: Rect, cell_aspect: f64) -> Vec<(Option<NodeId>, f64)> {
    let mut sorted: Vec<_> = items.iter().filter(|(_, s)| *s > 0).copied().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: f64 = sorted.iter().map(|(_, s)| *s as f64).sum();
    if total == 0.0 {
        return Vec::new();
    }
    let space = area.width as f64 * area.height as f64 * cell_aspect;
    let scale = space / total;
    let one_cell = cell_aspect;
    let mut out: Vec<_> = sorted
        .iter()
        .map(|&(id, s)| (Some(id), s as f64 * scale))
        .take_while(|&(_, a)| a >= one_cell)
        .collect();
    let aggregate: f64 = sorted[out.len()..]
        .iter()
        .map(|&(_, s)| s as f64 * scale)
        .sum();
    if !sorted[out.len()..].is_empty() {
        out.push((None, aggregate));
    }
    out
}

/// Worst aspect ratio of a row with area sum `sum`, extremes `max`/`min`, along a side of length `side`.
fn worst(sum: f64, max: f64, min: f64, side: f64) -> f64 {
    let s2 = side * side;
    (s2 * max / (sum * sum)).max(sum * sum / (s2 * min))
}

/// Squarified layout in float. Every shared edge is computed once and reused, so neighbours
/// agree bit for bit and rounding them cannot open gaps or overlaps.
fn exact_layout(
    items: &[(NodeId, u64)],
    area: Rect,
    cell_aspect: f32,
) -> Vec<(Option<NodeId>, FRect)> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }
    let aspect = cell_aspect as f64;
    let prepared = prepare(items, area, aspect);
    let (mut x0, mut y0) = (0.0, 0.0);
    let (x1, y1) = (area.width as f64, area.height as f64 * aspect);
    let mut out = Vec::with_capacity(prepared.len());
    let mut i = 0;
    while i < prepared.len() {
        let (rw, rh) = (x1 - x0, y1 - y0);
        let wide = rw >= rh;
        let side = if wide { rh } else { rw };
        let mut end = i + 1;
        let (mut sum, mut max, mut min) = (prepared[i].1, prepared[i].1, prepared[i].1);
        while end < prepared.len() {
            let a = prepared[end].1;
            let (nsum, nmax, nmin) = (sum + a, max.max(a), min.min(a));
            if worst(nsum, nmax, nmin, side) > worst(sum, max, min, side) {
                break;
            }
            (sum, max, min) = (nsum, nmax, nmin);
            end += 1;
        }
        let last_row = end == prepared.len();
        let room = if wide { rw } else { rh };
        let thick = if last_row {
            room
        } else {
            (sum / side).min(room)
        };
        // The strip's far edge on the stacking axis is the remaining rect's edge, exactly.
        let (far_x, far_y) = if wide {
            (x0 + thick, y1)
        } else {
            (x1, y0 + thick)
        };
        let (far_x, far_y) = if last_row {
            (if wide { x1 } else { far_x }, if wide { far_y } else { y1 })
        } else {
            (far_x, far_y)
        };
        let mut pos = if wide { y0 } else { x0 };
        let stop = if wide { far_y } else { far_x };
        for (j, &(id, a)) in prepared[i..end].iter().enumerate() {
            let next = if i + j + 1 == end {
                stop
            } else {
                (pos + a / thick).min(stop)
            };
            let (rx0, ry0, rx1, ry1) = if wide {
                (x0, pos, far_x, next)
            } else {
                (pos, y0, next, far_y)
            };
            out.push((
                id,
                FRect {
                    x0: rx0,
                    y0: ry0 / aspect,
                    x1: rx1,
                    y1: ry1 / aspect,
                },
            ));
            pos = next;
        }
        (x0, y0) = if wide { (far_x, y0) } else { (x0, far_y) };
        i = end;
    }
    out
}

/// Lays out `items` (one level) inside `area` as a squarified treemap.
///
/// - Items are sorted by (size desc, `NodeId` asc); input order is irrelevant.
/// - `cell_aspect` (cell height / width, default 2.0) must be finite and positive; the CLI
///   validates it.
/// - Items whose exact share is under one cell are merged into one trailing aggregate tile
///   (`id: None`). Zero-size items take no area.
/// - Tile edges are the float layout's edges rounded to cells, so the tiles cover `area` exactly
///   with no overlap, and each edge is within ±1 cell of the exact one.
/// - A tile whose snapped rect is empty is dropped (including a sub-cell aggregate). Such items
///   are "not shown"; callers can count them as `items.len()` minus tiles with `Some(id)`.
/// - Empty input, all-zero sizes or an empty `area` give no tiles.
pub fn squarify(items: &[(NodeId, u64)], area: Rect, cell_aspect: f32) -> Vec<Tile> {
    debug_assert!(cell_aspect.is_finite() && cell_aspect > 0.0);
    exact_layout(items, area, cell_aspect)
        .into_iter()
        .filter_map(|(id, f)| {
            let edge = |v: f64| v.round() as u16;
            let (x0, y0, x1, y1) = (edge(f.x0), edge(f.y0), edge(f.x1), edge(f.y1));
            (x1 > x0 && y1 > y0).then(|| Tile {
                id,
                rect: Rect::new(area.x + x0, area.y + y0, x1 - x0, y1 - y0),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ids(sizes: &[u64]) -> Vec<(NodeId, u64)> {
        sizes
            .iter()
            .enumerate()
            .map(|(i, &s)| (NodeId(i as u32), s))
            .collect()
    }

    fn area_of(r: Rect) -> u32 {
        r.width as u32 * r.height as u32
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    #[test]
    fn empty_and_all_zero_input_give_no_tiles() {
        let area = Rect::new(0, 0, 40, 10);
        assert!(squarify(&[], area, 2.0).is_empty());
        assert!(squarify(&ids(&[0, 0, 0]), area, 2.0).is_empty());
    }

    #[test]
    fn single_item_fills_the_area() {
        let area = Rect::new(3, 2, 40, 10);
        let tiles = squarify(&ids(&[7]), area, 2.0);
        assert_eq!(
            tiles,
            vec![Tile {
                id: Some(NodeId(0)),
                rect: area
            }]
        );
    }

    #[test]
    fn proportional_within_rounding() {
        let area = Rect::new(0, 0, 78, 6);
        let tiles = squarify(&ids(&[50, 25, 15, 10]), area, 2.0);
        assert_eq!(tiles.len(), 4);
        let total = (78 * 6) as f64;
        for (tile, share) in tiles.iter().zip([0.5, 0.25, 0.15, 0.10]) {
            let r = tile.rect;
            let bound = 2.0 * (r.width + r.height) as f64 + 4.0;
            assert!(
                (area_of(r) as f64 - share * total).abs() <= bound,
                "{r:?} share {share}"
            );
        }
        assert_eq!(tiles.iter().map(|t| area_of(t.rect)).sum::<u32>(), 78 * 6);
    }

    #[test]
    fn tiny_items_merge_into_one_trailing_aggregate() {
        let mut sizes = vec![1_000_000u64];
        sizes.extend(std::iter::repeat_n(100, 200));
        let tiles = squarify(&ids(&sizes), Rect::new(0, 0, 80, 24), 2.0);
        assert_eq!(tiles.len(), 2);
        assert_eq!(tiles[0].id, Some(NodeId(0)));
        assert_eq!(tiles[1].id, None);
    }

    #[test]
    fn input_order_does_not_matter_and_ties_break_by_id() {
        let items = vec![
            (NodeId(5), 10),
            (NodeId(2), 10),
            (NodeId(9), 30),
            (NodeId(1), 10),
        ];
        let mut rev = items.clone();
        rev.reverse();
        let area = Rect::new(0, 0, 60, 12);
        let a = squarify(&items, area, 2.0);
        assert_eq!(a, squarify(&rev, area, 2.0));
        let order: Vec<_> = a.iter().map(|t| t.id.unwrap().0).collect();
        assert_eq!(order, vec![9, 1, 2, 5]);
    }

    #[test]
    fn degenerate_area_gives_no_tiles() {
        assert!(squarify(&ids(&[1, 2]), Rect::new(0, 0, 0, 5), 2.0).is_empty());
        assert!(squarify(&ids(&[1, 2]), Rect::new(0, 0, 5, 0), 2.0).is_empty());
    }

    #[test]
    fn five_thousand_items_timing() {
        let sizes: Vec<u64> = (0..5000u64)
            .map(|i| (i * 2654435761 % 1_000_003) + 1)
            .collect();
        let items = ids(&sizes);
        let area = Rect::new(0, 0, 160, 20);
        let start = std::time::Instant::now();
        let runs = 20;
        for _ in 0..runs {
            std::hint::black_box(squarify(&items, area, 2.0));
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        println!("TARGET: squarify_5000_items_ms={ms:.3} (target 1.0)");
    }

    proptest! {
        #[test]
        fn layout_contract(
            sizes in prop::collection::vec(0u64..1_000_000_000_000, 1..500),
            w in 1u16..200, h in 1u16..60,
            aspect in 1.0f32..3.0,
        ) {
            let items = ids(&sizes);
            let area = Rect::new(2, 1, w, h);
            let tiles = squarify(&items, area, aspect);
            let mut rev = items.clone();
            rev.reverse();
            prop_assert_eq!(&tiles, &squarify(&rev, area, aspect));

            if sizes.iter().all(|&s| s == 0) {
                prop_assert!(tiles.is_empty());
                return Ok(());
            }
            prop_assert!(tiles.iter().filter(|t| t.id.is_none()).count() <= 1);
            if let Some(pos) = tiles.iter().position(|t| t.id.is_none()) {
                prop_assert_eq!(pos, tiles.len() - 1);
            }
            for t in &tiles {
                prop_assert!(area.contains(t.rect.as_position()));
                prop_assert!(t.rect.right() <= area.right() && t.rect.bottom() <= area.bottom());
                prop_assert!(area_of(t.rect) > 0);
            }
            for (i, a) in tiles.iter().enumerate() {
                for b in &tiles[i + 1..] {
                    prop_assert!(!overlaps(a.rect, b.rect));
                }
            }
            prop_assert_eq!(tiles.iter().map(|t| area_of(t.rect)).sum::<u32>(), w as u32 * h as u32);

            let exact = exact_layout(&items, area, aspect);

            // Independent of the snapping: float areas must match each item's share.
            let total: f64 = sizes.iter().map(|&s| s as f64).sum();
            let space = w as f64 * h as f64 * aspect as f64;
            let close = |got: f64, want: f64| (got - want).abs() <= 1e-9 + 1e-6 * want;
            let mut shown = 0.0;
            for (id, f) in &exact {
                let got = (f.x1 - f.x0) * (f.y1 - f.y0) * aspect as f64;
                let want = match id {
                    Some(id) => sizes[id.0 as usize] as f64 / total * space,
                    None => {
                        let members: f64 = items
                            .iter()
                            .filter(|(i, _)| !exact.iter().any(|(e, _)| *e == Some(*i)))
                            .map(|&(_, s)| s as f64)
                            .sum();
                        members / total * space
                    }
                };
                prop_assert!(close(got, want), "id {id:?}: area {got} want {want}");
                shown += got;
            }
            prop_assert!(close(shown, space));
            for tile in &tiles {
                let (_, f) = exact.iter().find(|(id, _)| *id == tile.id).unwrap();
                let r = tile.rect;
                prop_assert!((r.x as f64 - (area.x as f64 + f.x0)).abs() <= 1.0);
                prop_assert!((r.y as f64 - (area.y as f64 + f.y0)).abs() <= 1.0);
                prop_assert!((r.right() as f64 - (area.x as f64 + f.x1)).abs() <= 1.0);
                prop_assert!((r.bottom() as f64 - (area.y as f64 + f.y1)).abs() <= 1.0);
            }
        }
    }
}
