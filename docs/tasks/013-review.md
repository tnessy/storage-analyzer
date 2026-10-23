# T013 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree: HEAD plus only the T013 files):
- fmt ok, clippy clean;
- `cargo test --all-targets`: 24 lib tests (15 eta) + 4 + 7 pass;
- doc tests ok, release build ok;
- no `probe`/`Probe` left in `src/`.

**Pass 1, spec:**
- Part A matches the authorized delta exactly, and only owned files were touched.
- Part B follows the brief's precedence: Done/Stopped, then Indeterminate when there is no source, then Estimating, OverEstimate, Eta.
- The guard, the 0.99 clamp and the 2 s / settled gate are correct.

**Pass 2, quality:**
- Small, pure and readable. The tests check behavior, and the proptest covers finiteness.
- **Decisions accepted:** a non-finite or zero rate, or an unrepresentable `remaining`, gives `Estimating`, so `Eta` is always finite. The first `Rate` sample seeds the EWMA.
- **Non-blocking nit:** in `eta/mod.rs`, the test `source_arriving_mid_scan…` has `let seen = ms * 1000 / 1000;`, which is a no-op expression. Tidy it the next time the file is touched.

**Regression-file ruling:** `proptest-regressions/layout/squarify.txt` comes from T014's red phase. **Keep it and commit it as a seed**, as proptest recommends. It is committed with T014. `tests/model_apply.proptest-regressions` belongs to engineer-1's T003 and is handled in that review.

**Integration incident (Heimdall):** commit `4e3419b` (docs) accidentally included engineer-2's pre-staged deletion of `src/probe/mod.rs`. Because `lib.rs` still declared the module, HEAD did not build from `4e3419b` until this commit. Fixed forward; the history is local and unpushed. From now on I check `git diff --cached` before every commit, and engineers must not stage files (`git add`/`git rm`).
