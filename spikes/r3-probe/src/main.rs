mod fs;
mod tree;

use rand::{rngs::StdRng, SeedableRng};
use std::path::PathBuf;
use std::time::Instant;

type Probe = fn(&fs::FsTree, &mut StdRng) -> f64;

const KS: [usize; 3] = [16, 64, 256];
const FRACTIONS: [f64; 3] = [0.1, 0.5, 0.9];

/// Relative error of the remaining-time estimate at fraction `f` of the true scan,
/// assuming constant rate and `est_total = max(estimate, seen)`.
fn eta_error(estimate: f64, truth: f64, f: f64) -> f64 {
    let seen = f * truth;
    (estimate.max(seen) - seen) / ((1.0 - f) * truth) - 1.0
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| a.total_cmp(b));
    v
}

fn parse_args() -> Option<(PathBuf, u64)> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next()?);
    let mut seeds = 100u64;
    while let Some(flag) = args.next() {
        let value = args.next()?;
        match flag.as_str() {
            "--seeds" => seeds = value.parse().ok()?,
            _ => return None,
        }
    }
    Some((root, seeds))
}

/// Estimates for K = 16, 64, 256 from one seed's prefix sums, plus wall time per prefix.
fn run_seed(
    tree: &fs::FsTree,
    seed: u64,
    probe: fn(&fs::FsTree, &mut StdRng) -> f64,
) -> Vec<(f64, f64)> {
    let mut rng = StdRng::seed_from_u64(seed);
    let (mut sum, mut next) = (0.0, 0);
    let start = Instant::now();
    let mut out = Vec::new();
    for i in 1..=KS[KS.len() - 1] {
        sum += probe(tree, &mut rng);
        if KS[next] == i {
            out.push((sum / i as f64, start.elapsed().as_secs_f64() * 1000.0));
            next += 1;
        }
    }
    out
}

fn main() {
    let Some((root, seeds)) = parse_args() else {
        eprintln!("usage: r3-probe ROOT [--seeds S=100]");
        std::process::exit(2);
    };
    let tree = match fs::FsTree::new(root.clone()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot open {}: {e}", root.display());
            std::process::exit(1);
        }
    };
    let t0 = Instant::now();
    let truth_n = tree::true_count(&tree);
    let truth = truth_n as f64;
    println!(
        "## {} (true_entries={truth_n}, walk_s={:.1}, seeds={seeds})",
        root.display(),
        t0.elapsed().as_secs_f64()
    );

    println!("\n| variant | K | ratio p10 | ratio p50 | ratio p90 | within 0.5-2x | ETA err@10% p50/p90 | @50% p50/p90 | @90% p50/p90 | mean ms |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    let variants: [(&str, Probe); 2] = [
        ("A (all entries)", |t, r| tree::probe_once(t, r)),
        ("B (subdirs)", |t, r| tree::probe_once_subdirs(t, r)),
    ];
    for (name, probe) in variants {
        let runs: Vec<_> = (0..seeds).map(|s| run_seed(&tree, s, probe)).collect();
        for (ki, k) in KS.iter().enumerate() {
            let est: Vec<f64> = runs.iter().map(|r| r[ki].0).collect();
            let ratios = sorted(est.iter().map(|e| e / truth).collect());
            let within = ratios.iter().filter(|r| (0.5..=2.0).contains(*r)).count();
            let errs: Vec<String> = FRACTIONS
                .iter()
                .map(|&f| {
                    let e = sorted(est.iter().map(|&e| eta_error(e, truth, f)).collect());
                    format!("{:+.2} / {:+.2}", percentile(&e, 0.5), percentile(&e, 0.9))
                })
                .collect();
            let ms = runs.iter().map(|r| r[ki].1).sum::<f64>() / seeds as f64;
            println!(
                "| {name} | {k} | {:.3} | {:.3} | {:.3} | {within}% | {} | {ms:.1} |",
                percentile(&ratios, 0.1),
                percentile(&ratios, 0.5),
                percentile(&ratios, 0.9),
                errs.join(" | "),
            );
        }
    }

    println!("\nShallow-first scan, lower-bound total = seen + pending_dirs:\n");
    println!("| progress | seen | pending_dirs | (seen+pending)/true | ETA err |");
    println!("|---|---|---|---|---|");
    let cps = tree::bfs_checkpoints(&tree, truth_n, &FRACTIONS);
    for (f, (seen, pending)) in FRACTIONS.iter().zip(cps) {
        let bound = (seen + pending) as f64;
        let true_f = seen as f64 / truth;
        println!(
            "| {:.0}% | {seen} | {pending} | {:.3} | {:+.2} |",
            f * 100.0,
            bound / truth,
            eta_error(bound, truth, true_f)
        );
    }
    println!();
}
