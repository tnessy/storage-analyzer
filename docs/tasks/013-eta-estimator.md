# T013: ETA estimator (pure) and EWMA rate

- **Engineer:** engineer-2. **Wave:** 1. **Depends on:** T001. **Blocks:** T019.
- **Parallel with:** engineer-1's T003, T004 and T006.

## Goal
`eta::view(&EtaInput) -> EtaView` and a rate tracker that satisfy brief M2: present, labeled as an estimate, revised during the scan, honest when unknown, and converging to 0 at completion.

Acceptance criteria:
1. `Rate`, an EWMA of entries/s with τ = 5 s. Its API is `Rate::new(tau)` and `rate.sample(now: Duration, seen: u64)`, and it is fed on the 100 ms data tick. `rate.eps()` returns `Option<f64>`, which is `None` until it is **settled**: at least 2 s of samples and at least 3 samples.
2. `view()` implements the design §3.6 table exactly:
   - **`Estimating`:** when elapsed < 2 s, or there is no source, or the rate is not settled (`rate_eps` ≤ 0 or NaN).
   - **Guard:** `est_total = max(source_total, seen + pending_dirs)`.
   - **`OverEstimate`:** when `seen ≥ source_total × 1.02`.
   - **`Eta`:**
     - `remaining = (est_total − seen) / rate`;
     - `fraction = seen / est_total`, clamped to `[0, 0.99]`;
     - `source` is passed through.
   - **`Done { elapsed }`** when `state == Done`, and **`Stopped { elapsed }`** when `state == Stopped`. These take priority over everything else.
3. It never returns a non-finite `remaining` or `fraction`.
4. Pure and deterministic: no clock reads inside `eta`.

## Design references
§3.6 (full), §4 `EtaInput`/`EtaView`. ADR-005. UX spec §5.2 and §11 check 4 (the text is T019's job).

## Files owned
`src/eta/mod.rs` (taken over from T001), `src/eta/rate.rs`. Unit tests live in those files.

## Interfaces
**Consumes:** `EtaInput`, `EtaView`, `EtaSource`, `ScanState` (T001). **Provides:** `view`, `Rate` to T019.

## Tests to write first
- Each state transition with a synthetic feed:
  - Starting → Estimating (elapsed < 2 s);
  - Estimating (no source);
  - Eta;
  - OverEstimate (seen passes 1.02×);
  - Done;
  - Stopped.
- The guard: with a source total below `seen + pending_dirs`, `est_total` uses the guard and `fraction` < 1.
- `fraction` is never > 0.99 before Done, even when `seen == source_total`.
- **M2 convergence:** a synthetic constant-rate scan whose source equals the truth. `remaining` decreases monotonically (within EWMA noise, ±5 %) and is `Done` with zero remaining at the end.
- **Revision:** when the source total changes mid-scan, `remaining` changes on the next view.
- `Rate`: a constant input gives `eps` within 1 % of the true rate after 5τ. It is `None` before it settles.
- Property test: random inputs never produce NaN or infinity and never `fraction > 0.99` unless the state is Done.

Command: `cargo test eta::`.

## Out of scope
Estimate sources (probe, fs inodes, history) and status-line text (T019).
