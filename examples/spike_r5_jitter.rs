//! Spike R5: how much does a squarified treemap jitter while a scan changes the sizes?
//!
//! `cargo run --release --example spike_r5_jitter -- ROOT [--interval-ms 250,500,1000] [--size 78x6]`

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use storage_analyzer::app::preflight;
use storage_analyzer::layout::{Tile, squarify};
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::{self, RealFs, ScanControl, ScanMsg, ScanOptions};
use storage_analyzer::types::{Kind, NodeId, SizeMode};

const DEFAULT_ASPECT: f32 = 2.0;
const TOP_RANKS: usize = 8;
const SELECT_AT: Duration = Duration::from_secs(2);
const FRAME_EVERY: usize = 8;
const MAX_FRAMES: usize = 12;

/// How two consecutive layouts of the same scope differ.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Diff {
    /// Share of tiles (in either layout) whose rectangle changed or that appeared or vanished.
    changed_fraction: f64,
    /// Mean distance in cells between the centres of tiles present in both.
    mean_displacement: f64,
}

fn diff(prev: &[Tile], next: &[Tile]) -> Diff {
    let mut matched = 0usize;
    let mut changed = 0usize;
    let mut displacement = 0.0;
    for n in next {
        match prev.iter().find(|p| p.id == n.id) {
            Some(p) => {
                matched += 1;
                if p.rect != n.rect {
                    changed += 1;
                }
                displacement += distance(centre(p.rect), centre(n.rect));
            }
            None => changed += 1,
        }
    }
    // Tiles that vanished count as changed too.
    changed += prev
        .iter()
        .filter(|p| !next.iter().any(|n| n.id == p.id))
        .count();
    let total = next.len() + prev.len() - matched;
    if total == 0 {
        return Diff::default();
    }
    Diff {
        changed_fraction: changed as f64 / total as f64,
        mean_displacement: if matched == 0 {
            0.0
        } else {
            displacement / matched as f64
        },
    }
}

fn centre(r: Rect) -> (f64, f64) {
    (
        f64::from(r.x) + f64::from(r.width) / 2.0,
        f64::from(r.y) + f64::from(r.height) / 2.0,
    )
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Pairs among the first `k` of both rankings, present in both, that swapped order.
fn rank_swaps(prev: &[NodeId], next: &[NodeId], k: usize) -> usize {
    let (prev, next) = (&prev[..prev.len().min(k)], &next[..next.len().min(k)]);
    let mut swaps = 0;
    for (i, a) in prev.iter().enumerate() {
        for b in &prev[i + 1..] {
            let (Some(pa), Some(pb)) = (
                next.iter().position(|x| x == a),
                next.iter().position(|x| x == b),
            ) else {
                continue;
            };
            if pa > pb {
                swaps += 1;
            }
        }
    }
    swaps
}

/// `(p50, p95, max)` of the samples (nearest rank); zeros when empty.
fn summary(samples: &[f64]) -> (f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |p: f64| sorted[((p * sorted.len() as f64).ceil() as usize).max(1) - 1];
    (at(0.50), at(0.95), sorted[sorted.len() - 1])
}

/// One scope (a folder's children) being tracked across relayouts.
struct Track {
    scope: Option<NodeId>,
    tiles: Vec<Tile>,
    ranking: Vec<NodeId>,
    changed: Vec<f64>,
    displacement: Vec<f64>,
    swaps: Vec<f64>,
    scope_changes: usize,
}

impl Track {
    fn new() -> Self {
        Self {
            scope: None,
            tiles: Vec::new(),
            ranking: Vec::new(),
            changed: Vec::new(),
            displacement: Vec::new(),
            swaps: Vec::new(),
            scope_changes: 0,
        }
    }

    /// Records one relayout of `scope`'s children.
    fn record(&mut self, scope: NodeId, tiles: Vec<Tile>, ranking: Vec<NodeId>) {
        if self.scope == Some(scope) {
            let d = diff(&self.tiles, &tiles);
            self.changed.push(d.changed_fraction);
            self.displacement.push(d.mean_displacement);
            self.swaps
                .push(rank_swaps(&self.ranking, &ranking, TOP_RANKS) as f64);
        } else if self.scope.is_some() {
            self.scope_changes += 1;
        }
        self.scope = Some(scope);
        self.tiles = tiles;
        self.ranking = ranking;
    }
}

struct Args {
    root: PathBuf,
    intervals: Vec<u64>,
    size: (u16, u16),
}

fn parse_args() -> Option<Args> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next()?);
    let (mut intervals, mut size) = (vec![250, 500, 1000], (78, 6));
    while let Some(flag) = args.next() {
        let value = args.next()?;
        match flag.as_str() {
            "--interval-ms" => {
                intervals = value
                    .split(',')
                    .map(|v| v.parse().ok())
                    .collect::<Option<_>>()?;
            }
            "--size" => {
                let (w, h) = value.split_once('x')?;
                size = (w.parse().ok()?, h.parse().ok()?);
            }
            _ => return None,
        }
    }
    Some(Args {
        root,
        intervals,
        size,
    })
}

/// One letter per tile, with `+` on the corners, so neighbours are told apart in a text file.
fn ascii_frame(tiles: &[Tile], area: Rect) -> String {
    let mut grid = vec![vec![' '; usize::from(area.width)]; usize::from(area.height)];
    for (i, t) in tiles.iter().enumerate() {
        let letter = b"abcdefghijklmnopqrstuvwxyz"[i % 26] as char;
        for y in t.rect.y..t.rect.bottom() {
            for x in t.rect.x..t.rect.right() {
                let corner = (x == t.rect.x || x == t.rect.right() - 1)
                    && (y == t.rect.y || y == t.rect.bottom() - 1);
                grid[usize::from(y)][usize::from(x)] = if corner { '+' } else { letter };
            }
        }
    }
    grid.into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

struct Run {
    interval: Duration,
    next_tick: Duration,
    root: Track,
    sub: Track,
    selected: Option<NodeId>,
    selected_rect: Option<Rect>,
    selected_moves: usize,
    selected_samples: usize,
    relayouts: usize,
    frames: Vec<String>,
}

fn items(model: &Model, scope: NodeId) -> Vec<(NodeId, u64)> {
    model
        .children(scope)
        .iter()
        .map(|&c| (c, model.size(c, SizeMode::Disk)))
        .collect()
}

fn ranking(items: &[(NodeId, u64)]) -> Vec<NodeId> {
    let mut v = items.to_vec();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().map(|(id, _)| id).collect()
}

impl Run {
    fn relayout(&mut self, model: &Model, now: Duration, area: Rect) {
        self.relayouts += 1;
        let top = items(model, NodeId::ROOT);
        let tiles = squarify(&top, area, DEFAULT_ASPECT);
        let order = ranking(&top);
        // The currently largest top-level directory is the second tracked scope.
        let biggest_dir = order.iter().copied().find(|&c| model.kind(c) == Kind::Dir);

        if self.selected.is_none() && now >= SELECT_AT {
            self.selected = tiles.iter().find_map(|t| t.id);
        }
        if let Some(sel) = self.selected {
            let rect = tiles.iter().find(|t| t.id == Some(sel)).map(|t| t.rect);
            if self.selected_rect.is_some() {
                self.selected_samples += 1;
                if rect != self.selected_rect {
                    self.selected_moves += 1;
                }
            }
            self.selected_rect = rect;
        }
        if self.relayouts % FRAME_EVERY == 1 && self.frames.len() < MAX_FRAMES {
            self.frames.push(format!(
                "--- interval {} ms, t = {:.1} s, {} items ---\n{}",
                self.interval.as_millis(),
                now.as_secs_f64(),
                top.len(),
                ascii_frame(&tiles, area)
            ));
        }
        self.root.record(NodeId::ROOT, tiles, order);

        if let Some(dir) = biggest_dir {
            let inner = items(model, dir);
            self.sub
                .record(dir, squarify(&inner, area, DEFAULT_ASPECT), ranking(&inner));
        }
    }
}

fn percent(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

fn print_run(run: &Run) {
    println!(
        "### interval {} ms ({} relayouts)\n",
        run.interval.as_millis(),
        run.relayouts
    );
    println!("| scope | metric | p50 | p95 | max |");
    println!("|---|---|---|---|---|");
    for (name, track) in [
        ("root children", &run.root),
        ("largest top-level dir", &run.sub),
    ] {
        let (c50, c95, cmax) = summary(&track.changed);
        let (d50, d95, dmax) = summary(&track.displacement);
        let (s50, s95, smax) = summary(&track.swaps);
        println!(
            "| {name} | tiles changed | {} | {} | {} |",
            percent(c50),
            percent(c95),
            percent(cmax)
        );
        println!("| {name} | mean centroid move (cells) | {d50:.2} | {d95:.2} | {dmax:.2} |");
        println!("| {name} | rank swaps in top {TOP_RANKS} | {s50:.0} | {s95:.0} | {smax:.0} |");
        println!("| {name} | scope changes | {} | | |", track.scope_changes);
    }
    println!(
        "\nSelected tile (largest at {} s) moved in {} of {} relayouts.\n",
        SELECT_AT.as_secs(),
        run.selected_moves,
        run.selected_samples
    );
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        eprintln!("usage: spike_r5_jitter ROOT [--interval-ms 250,500,1000] [--size 78x6|158x16]");
        return ExitCode::from(2);
    };
    let root = match preflight(&args.root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("spike_r5_jitter: {e}");
            return ExitCode::from(2);
        }
    };
    let root_dev = root.entry.dev;
    let label = args.root.display().to_string();
    let mut model = Model::new(root.path.clone(), root.entry, ModelOptions { top_n: 10 });
    let (tx, rx) = sync_channel(1024);
    let control = Arc::new(ScanControl::new());
    let options = ScanOptions {
        threads: ScanOptions::default_threads(),
        one_filesystem: true,
        skip_pseudo: false,
    };
    let handle = scan::start(
        Arc::new(RealFs),
        root.path,
        root_dev,
        options,
        control,
        tx,
        1,
    );

    let area = Rect::new(0, 0, args.size.0, args.size.1);
    let mut runs: Vec<Run> = args
        .intervals
        .iter()
        .map(|&ms| Run {
            interval: Duration::from_millis(ms),
            next_tick: Duration::from_millis(ms),
            root: Track::new(),
            sub: Track::new(),
            selected: None,
            selected_rect: None,
            selected_moves: 0,
            selected_samples: 0,
            relayouts: 0,
            frames: Vec::new(),
        })
        .collect();

    let start = Instant::now();
    let mut done = false;
    while !done {
        let now = start.elapsed();
        let next = runs.iter().map(|r| r.next_tick).min().unwrap_or(now);
        match rx.recv_timeout(next.saturating_sub(now).max(Duration::from_millis(1))) {
            Ok(msg) => {
                done = matches!(msg, ScanMsg::Done { .. });
                model.apply(msg);
                // Apply whatever else is waiting, so a flood does not starve the ticks.
                while let Ok(more) = rx.try_recv() {
                    done |= matches!(more, ScanMsg::Done { .. });
                    model.apply(more);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => done = true,
        }
        let now = start.elapsed();
        for run in &mut runs {
            while run.next_tick <= now {
                run.relayout(&model, run.next_tick, area);
                run.next_tick += run.interval;
            }
        }
    }
    drop(rx);
    handle.shutdown();

    println!(
        "## {label} at {}x{} ({} entries, {:.1} s)\n",
        args.size.0,
        args.size.1,
        model.totals().entries_seen,
        start.elapsed().as_secs_f64()
    );
    for run in &runs {
        print_run(run);
    }
    let name: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let path = PathBuf::from(format!(
        "spikes/r5-frames/{}-{}x{}.txt",
        name.trim_matches('_'),
        args.size.0,
        args.size.1
    ));
    let text: Vec<String> = runs.iter().flat_map(|r| r.frames.iter().cloned()).collect();
    if let Err(e) = std::fs::write(&path, text.join("\n\n")) {
        eprintln!("cannot write {}: {e}", path.display());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(id: u32, x: u16, y: u16, w: u16, h: u16) -> Tile {
        Tile {
            id: Some(NodeId(id)),
            rect: Rect::new(x, y, w, h),
        }
    }

    #[test]
    fn identical_layouts_do_not_differ() {
        let a = [tile(1, 0, 0, 10, 5), tile(2, 10, 0, 10, 5)];
        assert_eq!(diff(&a, &a), Diff::default());
    }

    #[test]
    fn one_tile_moved_three_cells_is_reported() {
        let a = [tile(1, 0, 0, 10, 5), tile(2, 10, 0, 10, 5)];
        let b = [tile(1, 0, 0, 10, 5), tile(2, 13, 0, 10, 5)];
        let d = diff(&a, &b);
        assert_eq!(d.changed_fraction, 0.5);
        assert_eq!(d.mean_displacement, 1.5, "(0 + 3) / 2 tiles");
    }

    #[test]
    fn a_diagonal_move_uses_euclidean_distance() {
        let a = [tile(1, 0, 0, 4, 4)];
        let b = [tile(1, 3, 4, 4, 4)];
        assert_eq!(diff(&a, &b).mean_displacement, 5.0);
    }

    #[test]
    fn tiles_that_appear_or_vanish_count_as_changed_but_not_as_displaced() {
        let a = [tile(1, 0, 0, 10, 5), tile(2, 10, 0, 10, 5)];
        let b = [tile(1, 0, 0, 10, 5), tile(3, 10, 0, 10, 5)];
        let d = diff(&a, &b);
        assert!((d.changed_fraction - 2.0 / 3.0).abs() < 1e-9, "{d:?}");
        assert_eq!(d.mean_displacement, 0.0);
    }

    #[test]
    fn rank_swaps_count_flipped_pairs_among_the_top_k() {
        let ids = |v: &[u32]| v.iter().map(|&n| NodeId(n)).collect::<Vec<_>>();
        assert_eq!(rank_swaps(&ids(&[1, 2, 3]), &ids(&[1, 2, 3]), 8), 0);
        assert_eq!(rank_swaps(&ids(&[1, 2, 3]), &ids(&[2, 1, 3]), 8), 1);
        assert_eq!(rank_swaps(&ids(&[1, 2, 3]), &ids(&[3, 2, 1]), 8), 3);
        // Only the top k are compared; newcomers are not swaps.
        assert_eq!(rank_swaps(&ids(&[1, 2, 3]), &ids(&[2, 1, 3]), 1), 0);
        assert_eq!(rank_swaps(&ids(&[1, 2]), &ids(&[9, 1, 2]), 8), 0);
    }

    #[test]
    fn summary_gives_percentiles_and_the_maximum() {
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(summary(&samples), (50.0, 95.0, 100.0));
        assert_eq!(summary(&[]), (0.0, 0.0, 0.0));
        assert_eq!(summary(&[7.0]), (7.0, 7.0, 7.0));
    }
}
