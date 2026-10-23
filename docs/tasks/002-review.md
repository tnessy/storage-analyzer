# T002 review (Heimdall, 2026-10-23): CHANGES REQUESTED (not committed)

**Fresh evidence:** I read every file in `spikes/r3-probe/src` and `REPORT.md`. The tests are as reported. **I did not rerun the measurements.**

**Main finding: the conclusion covers only one estimator variant.**
- `tree.rs:17–34` picks uniformly among **all** entries, so a descent ends the first time it picks a file. With typical file:dir ratios, most descents stop at depth 1–2. That alone explains the "100–1000× low" medians on `$HOME`.
- That follows the formula in the brief and the design literally, so it is **not an implementation bug**. But "probe unusable" holds only for this variant.

**Required changes:**
1. **Add variant B** (subdirectory descent): at dir v, with e(v) entries and s(v) child dirs on the same filesystem, pick a child dir uniformly, and use X(v) = e(v) + s(v)·X(child), where X = e at a dir with no subdirs. It is unbiased: E[X(v)] = e(v) + Σ_c E[X(c)] = T(v). It still reads one dir per level, and never dies at a file.
   - Add the same uniform-exact and skewed-unbiased tests for B.
   - Rerun the same roots and table for A and B side by side: K = 16, 64, 256, 100 seeds.
   - Report the ETA error at 10/50/90 % as p50/p90 over all seeds, not 5 single seeds.
2. **Fix a factual error in REPORT.md line 3:** "`/` and `/home` share one device here" is wrong. `stat` gives `/` = 36 and `/home` = 59 (checked today). It doesn't affect the `$HOME` runs, but correct it.
3. Optional, and wanted if it's cheap: simulate a shallow-first (BFS) scan and report how well `seen + pending_dirs` does as a lower-bound total at 10/50/90 %. The design's guard depends on it.

Then update the recommendation. Until this lands, the architect's ADR-005 revision should not treat "the probe is unusable" as settled.

**Files:** `spikes/r3-probe/**` only, as before.

## Update 2026-10-23: verdict revised after ADR-010
The user approved ADR-010 (probe removed) using these results. So variant B no longer gates any decision.
- **Still required:** fix the device-ID statement on `REPORT.md` line 3. After that, T002 is **approved** and committed as the R3 record.
- **Now optional:** item 1 (variant B, subdirectory descent) and item 3 (BFS `seen + pending_dirs`). Item 3 is the architect's optional R3b. Variant B is input for an ADR-010 revisit, not v1 work.
- **Flagged to the architect via the team lead:**
  1. Design §3.6 and ADR-010 say "the Knuth probe is unbiased but unusable on real trees". That holds for the all-children variant measured here, not for every probe estimator. Scope the wording to that variant.
  2. The design §3.6 table's `Indeterminate` row says "before 2 s, Estimating", while the `Estimating` row requires a source. Per the team lead, T013 returns `Estimating` only when a source exists. Please make the table consistent.

## Final verdict 2026-10-23: APPROVED, committed
- The rework is verified. I re-ran `cargo test` (5 pass), clippy and fmt in `spikes/r3-probe`, and read `tree.rs` (`probe_once_subdirs`, `bfs_checkpoints`) and the revised `REPORT.md`.
- The device-ID statement is fixed.
- B and the BFS simulation are measured, and ADR-010 / design rev 9 record the result.
- Process note: the variant-B tests weren't seen failing first. That is acceptable for throwaway spike code, but not for production tasks.
