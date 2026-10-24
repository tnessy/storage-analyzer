//! Spike R2/R4/R8: a headless scan harness (T009).
//!
//! `cargo run --release --example scan_harness -- ROOT [--threads N] [--cross-filesystems]
//!  [--focus-at-ms T PATH [--watch-only]] [--slow-apply-ms N] [--du]`
//! `cargo run --release --example scan_harness -- --bench-rows`    (tree row build, 100 k / 1 M)
//! `cargo run --release --example scan_harness -- --bench-queue [N]` (queue re-key cost)
//!
//! Every number is a target, not a gate: lines starting `TARGET:` are for the report.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use storage_analyzer::app::tree_state::TreeState;
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::queue::{Job, Queue};
use storage_analyzer::scan::{self, RawEntry, RealFs, ScanControl, ScanMsg, ScanOptions};
use storage_analyzer::types::{IssueKind, Kind, NodeId, SizeMode};

/// The time the app spends applying messages per loop iteration (design §3.3).
const APPLY_BUDGET: Duration = Duration::from_millis(8);
const CHANNEL_CAPACITY: usize = 1024;

fn channel_capacity() -> usize {
    std::env::var("HARNESS_CHANNEL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CHANNEL_CAPACITY)
}
const CATEGORY_EVERY: Duration = Duration::from_millis(500);
const CATEGORY_TILES: usize = 60;

#[derive(Clone, Debug)]
struct Opts {
    root: PathBuf,
    threads: usize,
    cross_filesystems: bool,
    focus: Option<(u64, PathBuf)>,
    /// Only watch the folder (no focus): the unfocused baseline for R8.
    watch_only: bool,
    slow_apply_ms: u64,
    walker_only: bool,
    quiet: bool,
}

#[derive(Default)]
struct Report {
    produced: u64,
    applied: u64,
    wall: Duration,
    apply_time: Duration,
    ticks: Vec<Duration>,
    budget_exhausted_ticks: u64,
    max_pending_dirs: u64,
    rss_mib: f64,
    recompute_top: Duration,
    category_batches: Vec<Duration>,
    worker_cpu_percent: Option<f64>,
    focus_start_ms: Option<u64>,
    focus_dir_complete_ms: Option<u64>,
    issues: Vec<(IssueKind, PathBuf)>,
    totals: (u64, u64, u64, u64),
    done: bool,
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn sorted(mut v: Vec<Duration>) -> Vec<Duration> {
    v.sort();
    v
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn rss_mib() -> f64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: f64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0.0);
    pages * 4096.0 / (1024.0 * 1024.0)
}

/// CPU seconds (user + system) used so far by each thread named `scan-*`.
fn scan_thread_cpu() -> Vec<(String, f64)> {
    // USER_HZ is 100 on Linux (`getconf CLK_TCK`).
    let ticks = 100.0;
    let mut out = Vec::new();
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return out;
    };
    for task in tasks.flatten() {
        let comm = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
        let comm = comm.trim();
        if !comm.starts_with("scan-") {
            continue;
        }
        let stat = std::fs::read_to_string(task.path().join("stat")).unwrap_or_default();
        // Fields after the `(comm)`: state is field 3, utime 14, stime 15.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
            continue;
        };
        let f: Vec<&str> = rest.split_whitespace().collect();
        let cpu = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        out.push((comm.to_string(), (cpu(11) + cpu(12)) / ticks));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn root_entry(root: &Path) -> RawEntry {
    let meta = std::fs::symlink_metadata(root).expect("stat root");
    RawEntry {
        name: root
            .file_name()
            .map_or(&b"root"[..], |n| n.as_encoded_bytes())
            .into(),
        kind: Kind::Dir,
        disk_bytes: meta.blocks() * 512,
        apparent_bytes: 0,
        ino: meta.ino(),
        dev: meta.dev(),
        nlink: meta.nlink() as u32,
        issue: None,
    }
}

fn find(model: &Model, rel: &Path) -> Option<NodeId> {
    let mut cur = NodeId::ROOT;
    for part in rel.components() {
        let name = part.as_os_str().as_encoded_bytes();
        cur = *model
            .children(cur)
            .iter()
            .find(|&&c| model.name(c) == name)?;
    }
    Some(cur)
}

/// Times `category_of_node` for the largest children of the root and of its largest folder.
fn time_categories(model: &Model) -> Duration {
    let started = Instant::now();
    let mut biggest_dir = None;
    for (i, id) in model
        .children_sorted(NodeId::ROOT, SizeMode::Disk)
        .into_iter()
        .enumerate()
    {
        if i < CATEGORY_TILES {
            std::hint::black_box(model.category_of_node(id));
        }
        if biggest_dir.is_none() && model.kind(id) == Kind::Dir {
            biggest_dir = Some(id);
        }
    }
    if let Some(dir) = biggest_dir {
        for id in model
            .children_sorted(dir, SizeMode::Disk)
            .into_iter()
            .take(CATEGORY_TILES)
        {
            std::hint::black_box(model.category_of_node(id));
        }
    }
    started.elapsed()
}

fn run_scan(opts: &Opts) -> Report {
    let root_dev = std::fs::metadata(&opts.root).expect("root").dev();
    let control = Arc::new(ScanControl::new());
    let (tx, rx) = sync_channel(channel_capacity());
    let started = Instant::now();
    let handle = scan::start(
        Arc::new(RealFs),
        opts.root.clone(),
        root_dev,
        ScanOptions {
            threads: opts.threads,
            one_filesystem: !opts.cross_filesystems,
        },
        Arc::clone(&control),
        tx,
        1,
    );
    let mut model = Model::new(
        opts.root.clone(),
        root_entry(&opts.root),
        ModelOptions { top_n: 100 },
    );
    let mut report = Report::default();

    let mut watch: Option<NodeId> = None;
    let mut next_category = Instant::now();
    let mut second_start = Instant::now();
    let (mut second_applied, mut second_ticks) = (0u64, Vec::new());
    let mut cpu_window: Option<(Instant, Vec<(String, f64)>)> = None;

    'outer: loop {
        let tick = Instant::now();
        let mut messages = 0;
        let mut exhausted = false;
        loop {
            let msg = match rx.try_recv() {
                Ok(m) => m,
                Err(TryRecvError::Empty) => {
                    if messages > 0 {
                        break;
                    }
                    match rx.recv_timeout(Duration::from_millis(2)) {
                        Ok(m) => m,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => break 'outer,
                    }
                }
                Err(TryRecvError::Disconnected) => break 'outer,
            };
            let finished = matches!(msg, ScanMsg::Done { .. });
            let entries = match &msg {
                ScanMsg::Listing { entries, .. } => entries.len() as u64,
                _ => 0,
            };
            report.produced += entries;
            if !opts.walker_only {
                let t = Instant::now();
                model.apply(msg);
                report.apply_time += t.elapsed();
                report.applied += entries;
                second_applied += entries;
            }
            messages += 1;
            if opts.slow_apply_ms > 0 {
                std::thread::sleep(Duration::from_millis(opts.slow_apply_ms));
            }
            if finished {
                report.done = true;
                break 'outer;
            }
            if tick.elapsed() >= APPLY_BUDGET {
                exhausted = true;
                break;
            }
        }
        if messages > 0 {
            let took = tick.elapsed();
            report.ticks.push(took);
            second_ticks.push(took);
            report.budget_exhausted_ticks += u64::from(exhausted);
        }
        if !opts.walker_only {
            report.max_pending_dirs = report.max_pending_dirs.max(model.totals().pending_dirs);
        }

        let elapsed = started.elapsed();
        if let Some((at_ms, path)) = &opts.focus
            && report.focus_start_ms.is_none()
            && elapsed >= Duration::from_millis(*at_ms)
            && let Some(id) = find(&model, path)
        {
            if !opts.watch_only {
                control.set_focus(Some(model.path(id)));
                model.set_focus(Some(id));
            }
            report.focus_start_ms = Some(elapsed.as_millis() as u64);
            watch = Some(id);
        }
        if let (Some(id), None) = (watch, report.focus_dir_complete_ms)
            && model.is_complete(id)
        {
            report.focus_dir_complete_ms = Some(elapsed.as_millis() as u64);
        }
        if !opts.walker_only && Instant::now() >= next_category {
            report.category_batches.push(time_categories(&model));
            next_category = Instant::now() + CATEGORY_EVERY;
        }

        if opts.slow_apply_ms > 0 {
            if cpu_window.is_none() && elapsed > Duration::from_secs(2) {
                cpu_window = Some((Instant::now(), scan_thread_cpu()));
            } else if let Some((since, before)) = &cpu_window
                && since.elapsed() >= Duration::from_secs(3)
            {
                let after = scan_thread_cpu();
                let used: f64 = after
                    .iter()
                    .map(|(n, c)| c - before.iter().find(|(m, _)| m == n).map_or(0.0, |(_, b)| *b))
                    .sum();
                report.worker_cpu_percent = Some(100.0 * used / since.elapsed().as_secs_f64());
                // Measured; a scan this slow would take hours to finish.
                break 'outer;
            }
        }

        if !opts.quiet && second_start.elapsed() >= Duration::from_secs(1) {
            let t = sorted(std::mem::take(&mut second_ticks));
            println!(
                "t={:>4.0}s applied {:>8}/s  tick p50 {:5.2} ms p95 {:5.2} ms max {:5.2} ms  pending dirs {}",
                elapsed.as_secs_f64(),
                second_applied,
                ms(percentile(&t, 0.5)),
                ms(percentile(&t, 0.95)),
                ms(t.last().copied().unwrap_or_default()),
                model.totals().pending_dirs,
            );
            second_applied = 0;
            second_start = Instant::now();
        }
    }
    report.wall = started.elapsed();
    drop(rx);
    handle.shutdown();

    report.rss_mib = rss_mib();
    if !opts.walker_only {
        let t = Instant::now();
        model.recompute_top();
        report.recompute_top = t.elapsed();
        let totals = model.totals();
        report.totals = (
            totals.files + totals.dirs + 1,
            totals.disk_bytes,
            totals.apparent_bytes,
            totals.pending_dirs,
        );
        report.issues = model
            .issues()
            .iter()
            .map(|i| (i.kind, model.path(i.node)))
            .collect();
        std::hint::black_box(&model);
    }
    report
}

fn print_report(opts: &Opts, r: &Report, walker_rate: Option<f64>) {
    let wall = r.wall.as_secs_f64();
    let ticks = sorted(r.ticks.clone());
    println!("--- {} threads={} ---", opts.root.display(), opts.threads);
    println!(
        "wall {wall:.2} s, entries produced {} ({:.0}/s), RSS {:.0} MiB",
        r.produced,
        r.produced as f64 / wall,
        r.rss_mib
    );
    if opts.walker_only {
        return;
    }
    let apply_rate = r.applied as f64 / r.apply_time.as_secs_f64().max(1e-9);
    println!(
        "apply: {:.2} s total, {:.2} M entries/s of pure apply time; tick p50 {:.2} p95 {:.2} max {:.2} ms; {} of {} ticks hit the {} ms budget (backlog)",
        r.apply_time.as_secs_f64(),
        apply_rate / 1e6,
        ms(percentile(&ticks, 0.5)),
        ms(percentile(&ticks, 0.95)),
        ms(ticks.last().copied().unwrap_or_default()),
        r.budget_exhausted_ticks,
        r.ticks.len(),
        APPLY_BUDGET.as_millis(),
    );
    println!(
        "frontier (max discovered-but-unlisted dirs, the queue's pending jobs): {}",
        r.max_pending_dirs
    );
    println!(
        "recompute_top on the final model: {:.1} ms",
        ms(r.recompute_top)
    );
    if !r.category_batches.is_empty() {
        let b = sorted(r.category_batches.clone());
        println!(
            "category_of_node for the {CATEGORY_TILES} largest children of the root and of its largest folder: p50 {:.3} ms p95 {:.3} ms ({} samples)",
            ms(percentile(&b, 0.5)),
            ms(percentile(&b, 0.95)),
            b.len()
        );
    }
    if let Some(cpu) = r.worker_cpu_percent {
        println!(
            "TARGET: worker_cpu_percent_while_channel_full={cpu:.1} (threads={}; polling every 500 us; 100 = one core)",
            opts.threads
        );
    }
    if let (Some(start), Some(done)) = (r.focus_start_ms, r.focus_dir_complete_ms) {
        println!(
            "focus set at {start} ms; the folder was complete at {done} ms ({} ms after)",
            done.saturating_sub(start)
        );
    } else if let Some(start) = r.focus_start_ms {
        println!("focus set at {start} ms; the folder never completed before the end");
    }
    if let Some(w) = walker_rate {
        println!(
            "TARGET: apply_vs_walker_rate_percent={:.0} (target >= 70)",
            100.0 * apply_rate / w
        );
    }
    println!(
        "totals: entries {} disk {} apparent {} pending_dirs {}",
        r.totals.0, r.totals.1, r.totals.2, r.totals.3
    );
    let other_fs: Vec<_> = r
        .issues
        .iter()
        .filter(|(k, _)| *k == IssueKind::OtherFilesystem)
        .collect();
    println!(
        "issues: {} total, {} other-filesystem",
        r.issues.len(),
        other_fs.len()
    );
    for (_, path) in other_fs.iter().take(30) {
        println!("  other filesystem: {}", path.display());
    }
}

fn du(root: &Path, args: &[&str]) -> Option<u64> {
    let out = Command::new("du")
        .env("LC_ALL", "C")
        .args(["-s", "-B1"])
        .args(args)
        .arg(root)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn print_du(root: &Path, r: &Report, one_filesystem: bool) {
    let (disk, apparent) = (r.totals.1, r.totals.2);
    println!(
        "du comparison (harness one_filesystem={one_filesystem}): harness disk {disk} apparent {apparent}"
    );
    let rows = [
        ("du -s -B1", du(root, &[]), disk),
        ("du -sx -B1", du(root, &["-x"]), disk),
        (
            "du -s -B1 --apparent-size",
            du(root, &["--apparent-size"]),
            apparent,
        ),
    ];
    for (label, value, mine) in rows {
        match value {
            Some(v) => println!(
                "  {label}: {v} (harness {:+} bytes, {:+.4} %)",
                mine as i128 - v as i128,
                100.0 * (mine as f64 - v as f64) / v as f64
            ),
            None => println!("  {label}: not available"),
        }
    }
}

fn synthetic_root() -> RawEntry {
    RawEntry {
        name: b"synthetic".as_slice().into(),
        kind: Kind::Dir,
        disk_bytes: 4096,
        apparent_bytes: 0,
        ino: 1,
        dev: 1,
        nlink: 1,
        issue: None,
    }
}

fn bench_rows() {
    for n in [100_000u32, 1_000_000] {
        let mut model = Model::new(
            PathBuf::from("/synthetic"),
            synthetic_root(),
            ModelOptions { top_n: 100 },
        );
        let entries: Vec<RawEntry> = (0..n)
            .map(|i| RawEntry {
                name: format!("file-{i:08}.dat").into_bytes().into(),
                kind: Kind::File,
                disk_bytes: 4096 * (1 + u64::from(i % 997)),
                apparent_bytes: 100 + u64::from(i),
                ino: 0,
                dev: 1,
                nlink: 1,
                issue: None,
            })
            .collect();
        model.apply(ScanMsg::Listing {
            dir: NodeId::ROOT,
            first_id: NodeId(1),
            entries,
        });
        let mut tree = TreeState::new();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            tree.prepare(&model, SizeMode::Disk, 40);
            samples.push(t.elapsed());
        }
        let first = samples[0];
        let steady = sorted(samples[1..].to_vec());
        println!(
            "children {n}: rows {} first prepare {:.1} ms, steady p50 {:.1} ms max {:.1} ms",
            tree.rows().len(),
            ms(first),
            ms(percentile(&steady, 0.5)),
            ms(*steady.last().unwrap()),
        );
        println!(
            "TARGET: tree_row_build_ms_{}k={:.1} (target 50)",
            n / 1000,
            ms(percentile(&steady, 0.5))
        );
    }
}

fn bench_queue(n: u32) {
    let control = Arc::new(ScanControl::new());
    let queue = Queue::new(Arc::clone(&control));
    for i in 0..n {
        queue.push(Job {
            dir: NodeId(i + 1),
            path: PathBuf::from(format!("/r/d{}/e{}/f{}", i % 97, i % 1013, i)),
            depth: 1 + i % 12,
        });
    }
    control.set_focus(Some(PathBuf::from("/r/d5")));
    let started = Instant::now();
    let _ = queue.pop();
    println!(
        "queue with {n} pending jobs: focus change then first pop {:.1} ms (last_rekey_ms {:.1}), max_pending {}",
        ms(started.elapsed()),
        queue.last_rekey_ms(),
        queue.max_pending()
    );
    println!("TARGET: queue_rekey_ms_at_{n}={:.1}", queue.last_rekey_ms());
    let started = Instant::now();
    control.set_focus(None);
    let _ = queue.pop();
    println!("clearing the focus: {:.1} ms", ms(started.elapsed()));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    if args.iter().any(|a| a == "--bench-rows") {
        return bench_rows();
    }
    if args.iter().any(|a| a == "--bench-queue") {
        let n = value("--bench-queue")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1_000_000);
        return bench_queue(n);
    }
    let Some(root) = args
        .iter()
        .find(|a| !a.starts_with("--") && Path::new(a).is_dir())
    else {
        eprintln!(
            "usage: scan_harness ROOT [--threads N] [--cross-filesystems] [--focus-at-ms T PATH] [--slow-apply-ms N] [--du]"
        );
        std::process::exit(2);
    };
    let focus = args.iter().position(|a| a == "--focus-at-ms").map(|i| {
        let at: u64 = args[i + 1].parse().expect("--focus-at-ms T PATH");
        let rel = Path::new(&args[i + 2]);
        (at, rel.strip_prefix(root).unwrap_or(rel).to_path_buf())
    });
    let opts = Opts {
        root: PathBuf::from(root),
        threads: value("--threads")
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(ScanOptions::default_threads),
        cross_filesystems: args.iter().any(|a| a == "--cross-filesystems"),
        focus,
        watch_only: args.iter().any(|a| a == "--watch-only"),
        slow_apply_ms: value("--slow-apply-ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        walker_only: false,
        quiet: false,
    };
    println!(
        "cold run: first `sync; echo 3 | sudo tee /proc/sys/vm/drop_caches` yourself (never done here)"
    );
    // One throw-away pass first so the measured pass is as warm as the applied one.
    let _ = run_scan(&Opts {
        walker_only: true,
        quiet: true,
        focus: None,
        slow_apply_ms: 0,
        ..opts.clone()
    });
    let walker_rate = {
        let w = run_scan(&Opts {
            walker_only: true,
            quiet: true,
            focus: None,
            slow_apply_ms: 0,
            ..opts.clone()
        });
        w.produced as f64 / w.wall.as_secs_f64()
    };
    println!("walker alone (messages dropped on arrival): {walker_rate:.0} entries/s");
    let report = run_scan(&opts);
    print_report(&opts, &report, Some(walker_rate));
    if args.iter().any(|a| a == "--du") {
        print_du(&opts.root, &report, !opts.cross_filesystems);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_harness_loop_reaches_done_with_every_entry_applied() {
        let tmp = tempfile::tempdir().unwrap();
        let mut expected = 0u64;
        for d in 0..10 {
            std::fs::create_dir_all(tmp.path().join(format!("d{d}"))).unwrap();
            expected += 1;
            for f in 0..99 {
                std::fs::write(tmp.path().join(format!("d{d}/f{f}")), b"x").unwrap();
                expected += 1;
            }
        }
        for threads in [1, 4] {
            let opts = Opts {
                root: tmp.path().to_path_buf(),
                threads,
                cross_filesystems: false,
                focus: None,
                watch_only: false,
                slow_apply_ms: 0,
                walker_only: false,
                quiet: true,
            };
            let report = run_scan(&opts);
            assert!(report.done);
            assert_eq!(report.produced, expected);
            assert_eq!(report.applied, report.produced);
            assert!(!report.ticks.is_empty());
        }
    }

    #[test]
    fn walker_only_counts_entries_without_applying_them() {
        let tmp = tempfile::tempdir().unwrap();
        for f in 0..50 {
            std::fs::write(tmp.path().join(format!("f{f}")), b"x").unwrap();
        }
        let opts = Opts {
            root: tmp.path().to_path_buf(),
            threads: 2,
            cross_filesystems: false,
            focus: None,
            watch_only: false,
            slow_apply_ms: 0,
            walker_only: true,
            quiet: true,
        };
        let report = run_scan(&opts);
        assert!(report.done);
        assert_eq!((report.produced, report.applied), (50, 0));
    }
}
