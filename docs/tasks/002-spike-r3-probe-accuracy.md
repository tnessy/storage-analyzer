# T002: Spike R3, Knuth probe ETA accuracy (standalone)

- **Engineer:** engineer-2. **Wave:** 0. **Depends on:** none. **Parallel with:** T001.

## Goal
Retire risk R3 (design §7): is the Knuth random-probe estimate of total entries good enough on the user's real trees? This decides whether ADR-005's "revisit if probe error > 2x at 50 % progress" fires.

Acceptance criteria:
1. A standalone crate at `spikes/r3-probe/`, with its own `Cargo.toml` and **not** a member of the main crate, takes `ROOT [--probes K=64] [--seeds S=5]`.
2. It computes the probe estimate N = mean over K descents of Σ_d Π_{i≤d} b_i (design §3.6). Descents read one directory per level, never follow symlinks, and stay on one filesystem (`st_dev` = root's).
3. It counts the true number of entries with a full read-only walk under the same rules.
4. It reports, for each root and seed, the ratio estimate/true for K = 16, 64 and 256. It also reports the error of the ETA a user would see at 10 %, 50 % and 90 % of the true scan. Simulate this: at fraction f of the true count, the scan has seen f·true entries and `pending_dirs` is unknown, so use `est_total = max(probe, seen)` and assume a constant rate.
5. Roots to run: `$HOME`, `/usr`, and one `node_modules`- or `.git`-heavy project directory under `$HOME` (pick one and name it in the report).
6. `spikes/r3-probe/REPORT.md` holds a table of the results, the probe's wall time, and a recommendation: keep it, show it as a range, or hide it until N %.

## Design references
§3.6 ETA, ADR-005, §7 R3.

## Files owned
`spikes/r3-probe/**` only.

## Constraints
- **Read-only.** The spike opens directories `O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_NOATIME` (retry without `O_NOATIME` on EPERM) and never writes outside `spikes/r3-probe/` (the report). std plus `rustix` and `rand` are allowed in this crate only.
- Permission errors count the entry and skip its contents. They must not crash the spike.

## Tests to write first
- A unit test on a synthetic, perfectly uniform tree (branching b, depth d). The probe returns the exact count for any seed.
- A unit test on a skewed synthetic tree shows the estimate is unbiased: the mean over 2,000 seeds is within 5 % of the truth.
- Commands: `cargo test --manifest-path spikes/r3-probe/Cargo.toml`, then `cargo run --release --manifest-path spikes/r3-probe/Cargo.toml -- $HOME`.

## Out of scope
The production `probe` module (T019). Byte-based estimates.
