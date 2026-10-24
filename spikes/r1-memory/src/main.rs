//! Spike R1: memory per entry of the real model, fed by a synthetic scan stream.
//!
//! `r1-memory` runs 1 M, 5 M, 10 M and 20 M entries, each in its own process so that
//! one run's freed memory cannot hide in the next one's RSS. `--entries N` runs one size.

use std::alloc::{GlobalAlloc, Layout, System};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use r1_memory::{root_entry, NameHist, Shape, Stream};
use storage_analyzer::model::{Model, ModelOptions};
use storage_analyzer::scan::ScanMsg;
use storage_analyzer::types::NodeId;

/// Counts live and peak heap bytes (what was asked of the allocator, not RSS).
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            if new_size >= layout.size() {
                let now = LIVE.fetch_add(new_size - layout.size(), Ordering::Relaxed)
                    + (new_size - layout.size());
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        p
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Resident set size in bytes, from `/proc/self/statm`.
fn rss_bytes() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    pages * 4096
}

/// Bytes the kernel has pushed to swap (`VmSwap`). On a busy machine the RSS alone
/// under-reports the footprint, so the headline number is RSS + swap.
fn swap_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmSwap:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kib| kib * 1024)
}

fn live() -> u64 {
    LIVE.load(Ordering::Relaxed) as u64
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// `--names builtin`, or the length histogram of names under `$HOME`, cached in
/// `--hist-file` so every run uses the same one.
fn histogram(args: &[String]) -> NameHist {
    if arg_value(args, "--names").as_deref() == Some("builtin") {
        return NameHist::builtin();
    }
    let cache = PathBuf::from(
        arg_value(args, "--hist-file").unwrap_or_else(|| "home-histogram.txt".into()),
    );
    if let Some(hist) = std::fs::read_to_string(&cache)
        .ok()
        .and_then(|t| NameHist::from_text(&t))
    {
        return hist;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let hist = NameHist::from_dir(&home, 300_000);
    let _ = std::fs::write(&cache, hist.to_text());
    hist
}

fn run_one(entries: u64, args: &[String]) {
    let hist = histogram(args);
    let shape = Shape {
        hardlink_fraction: arg_value(args, "--hardlinks")
            .and_then(|v| v.parse().ok())
            .unwrap_or(Shape::default().hardlink_fraction),
        ..Shape::default()
    };
    let mean_name = hist.mean_len();
    let mut stream = Stream::new(entries, shape, hist, 0x5eed);

    let (rss0, live0) = (rss_bytes() + swap_bytes(), live());
    let mut model = Model::new(
        PathBuf::from("/synthetic"),
        root_entry(),
        ModelOptions { top_n: 100 },
    );
    let mut apply_time = Duration::ZERO;
    let started = Instant::now();
    for msg in stream.by_ref() {
        let t = Instant::now();
        model.apply(msg);
        apply_time += t.elapsed();
    }
    let wall = started.elapsed();
    let (rss1, swap1) = (rss_bytes(), swap_bytes());
    let (live1, peak) = (live(), PEAK.load(Ordering::Relaxed) as u64);
    let rss1 = rss1 + swap1;
    let t = stream.totals();
    let n = entries as f64;
    assert_eq!(model.totals().entries_seen, entries);
    assert!(model.is_complete(NodeId::ROOT));

    let rss_per = (rss1 - rss0) as f64 / n;
    let heap_per = (live1 - live0) as f64 / n;
    println!(
        "== N = {entries} ({} dirs = {:.1} %, {} hard-link pairs) ==",
        t.dirs,
        100.0 * t.dirs as f64 / n,
        t.hardlink_pairs
    );
    println!(
        "mean name length: {mean_name:.1} B (generated: {:.1} B)",
        t.name_bytes as f64 / n
    );
    println!("RSS+swap before {:8.1} MiB   after {:8.1} MiB   delta {:8.1} MiB   = {rss_per:6.1} B/entry", mib(rss0), mib(rss1), mib(rss1 - rss0));
    println!("heap     before {:8.1} MiB   after {:8.1} MiB   delta {:8.1} MiB   = {heap_per:6.1} B/entry (peak {:.1} MiB)", mib(live0), mib(live1), mib(live1 - live0), mib(peak - live0));
    println!("swapped out at the end: {:.1} MiB", mib(swap1));
    println!(
        "apply    {:.2} s of apply calls ({:.2} M entries/s), {:.2} s including generation",
        apply_time.as_secs_f64(),
        n / apply_time.as_secs_f64() / 1e6,
        wall.as_secs_f64()
    );
    println!("TARGET: model_bytes_per_entry={rss_per:.1} (target 120)");
    println!("TARGET: model_heap_bytes_per_entry={heap_per:.1} (target 120)");
    if entries == 20_000_000 {
        let gib = (rss1 - rss0) as f64 / (1024.0 * 1024.0 * 1024.0);
        println!("TARGET: model_rss_gib_at_20M={gib:.2} (about 2.3 at the target)");
    }
    // Keep the model alive until the numbers are printed.
    std::hint::black_box(&model);
    let _ = ScanMsg::Done { cancelled: false };
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(n) = arg_value(&args, "--entries") {
        run_one(n.parse().expect("--entries N"), &args);
        return;
    }
    let exe = std::env::current_exe().expect("current exe");
    for n in [1_000_000u64, 5_000_000, 10_000_000, 20_000_000] {
        let status = Command::new(&exe)
            .args(["--entries", &n.to_string()])
            .args(&args)
            .status()
            .expect("spawn");
        assert!(status.success());
    }
}
