# T002 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files (all in `spikes/r3-probe/`): `Cargo.toml`, `src/{main,tree,fs}.rs`, `REPORT.md`. Plus this report. `target/` and `Cargo.lock` were generated there by the build.

Red: both unit tests failed against a stub returning 0 (`left: 0 right: 1364`; `assertion failed: truth > 100.0`). The skewed fixture was too small at first, so I enlarged it. Green: 2 pass.

Fresh runs in the spike crate: `cargo test` 2 passed; `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean; `cargo build --release` ok. The main-crate commands were not run (the spike is not a workspace member).

Result: the probe is unbiased but heavy-tail-skewed. On $HOME it's off 100–1000× in 96–98 % of seeds, and on /usr and a node_modules project 72–92 % of seeds are under 0.5×. ADR-005's revisit trigger fires. Recommendation: don't show a probe ETA or percentage; see `spikes/r3-probe/REPORT.md`.

Notes:
- The spike has its own `[workspace]` table so cargo does not attach it to a parent workspace.
- The project root measured was `~/workspace/minecraft/pollution-craft-docs`.
- Mixed-filesystem mounts are treated as leaf entries (counted, not descended).
- The true-count walk and the probe use the same `Tree` trait, so the rules are identical.
- The ETA error is simulated per the brief, and the report's per-seed ETA table covers seeds 0–4 at K=64 only.
- The $HOME walk took 49 s cold and 6 s warm. The true count drifts between runs because the tree is live.
