# T013: ETA estimator (pure), EWMA rate, ADR-010 contract delta

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T001 (merged). **Hold lifted:** ADR-010 accepted by the user. **Blocks:** T019.
- **Parallel with:** engineer-1's T003, T004 and T006.
- **Rev 2 (2026-10-23):** rewritten for ADR-010 / design rev 7 §3.6. The probe is removed, and a no-source state is added.

## Goal
`eta::view(&EtaInput) -> EtaView` and a rate tracker for brief M2 under ADR-010:
- a labeled ETA **only when a reliable source exists** (history, in-session previous scan, inode count);
- otherwise an honest **indeterminate** progress state with true quantities only;
- `Done` at completion.

## Part A: contract delta (authorized by Heimdall; contracts are otherwise frozen)
T001 is merged with the old ETA contract. Make exactly these changes in this task:
1. In `src/eta/contract.rs`:
   - `EtaSource { History, SessionPrevious, FsInodes }`, with `Probe` removed.
   - `EtaInput` gains `top_done: u32` and `top_total: u32`. Change `rate_eps: f64` to `rate_eps: Option<f64>` (`None` = not settled).
   - `EtaView` gains `Indeterminate { rate_eps: Option<f64>, pending_dirs: u64, top_done: u32, top_total: u32 }`.
2. Delete `src/probe/mod.rs` and the `pub mod probe;` line in `src/lib.rs` (that one line is the only `lib.rs` edit allowed). `ProbeMsg` is gone; plan D9 is retracted.
3. `cargo build` and the write-guard/name tests stay green.

## Part B: behavior
Acceptance criteria:
1. `Rate`, an EWMA of entries/s with τ = 5 s: `Rate::new(tau)`, `rate.sample(now: Duration, seen: u64)` (fed on the 100 ms data tick), and `rate.eps() -> Option<f64>`, which is `None` until **settled** (at least 2 s of samples and at least 3 samples).
2. `view()` follows design §3.6 rev 7. The precedence, from first to last:
   1. `state == Done` → `Done { elapsed }`. `state == Stopped` → `Stopped { elapsed }`.
   2. **No source** → `Indeterminate { rate_eps, pending_dirs, top_done, top_total }`. **`Estimating` is never returned without a source.** This is the team lead's ruling. Design §3.6's table row says "before 2 s, Estimating"; Heimdall has flagged that to the architect, and the ruling holds until the architect answers. In the first seconds, `rate_eps` is simply `None`.
   3. A source exists but elapsed < 2 s, or the rate is not settled → `Estimating`.
   4. `seen ≥ source_total × 1.02` → `OverEstimate`.
   5. Otherwise `Eta`, with:
      - `est_total = max(source_total, seen + pending_dirs)`;
      - `remaining = (est_total − seen) / rate`;
      - `fraction = seen / est_total`, clamped to `[0, 0.99]`;
      - `source` passed through.
3. It never returns a non-finite `remaining` or `fraction`.
4. Pure and deterministic: no clock reads, no I/O.

## Design references
§3.6 (rev 7, full), §4 contracts. ADR-010 (supersedes ADR-005's source list). UX spec §5.2 and §11 check 4. The text belongs to T019.

## Files owned
`src/eta/contract.rs`, `src/eta/mod.rs`, `src/eta/rate.rs`, `src/probe/mod.rs` (delete), and the one `pub mod probe;` line in `src/lib.rs`. Unit tests go in the eta files.

## Interfaces
- **Consumes:** `ScanState` (T001).
- **Provides:** `view`, `Rate`, and the revised `EtaInput`/`EtaView`/`EtaSource` to T019.
- `top_done`/`top_total` come from `Model::top_level_progress()` (T003 amendment); T019 wires them.

## Tests to write first
- **State precedence:**
  - no source at elapsed 0.5 s and at 30 s → `Indeterminate`, never `Estimating`;
  - a source at 0.5 s → `Estimating`;
  - a source with the rate settled → `Eta`;
  - seen passes 1.02× the source → `OverEstimate`;
  - Done and Stopped take priority over everything else.
- **Indeterminate passes values through:** `pending_dirs`, `top_done`/`top_total` and `rate_eps` (including `None`) are copied unchanged.
- **Mid-scan source arrival:** the same feed with the source set to `None` until t = 10 s, then `Some`. The output goes `Indeterminate → Estimating` (if the rate is not settled) `→ Eta`, never back to `Indeterminate` while the source remains.
- **Guard:** with a source total below `seen + pending_dirs`, `fraction` < 1 and `est_total` uses the guard.
- `fraction` ≤ 0.99 before Done, even when `seen == source_total`.
- **M2 convergence with a source:** a constant-rate synthetic scan whose source equals the truth. `remaining` decreases (±5 % EWMA noise) and ends at `Done`.
- `Rate`: a constant input gives `eps` within 1 % after 5τ, and `None` before it settles.
- Property test: random inputs never produce NaN or infinity, and never `fraction > 0.99` unless Done.

Command: `cargo test eta::`, plus the five standard commands.

## Out of scope
Sources (history, session, inodes) and status-line text (T019). The probe (removed by ADR-010).
