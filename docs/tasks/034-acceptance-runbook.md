# T034: Acceptance runbook for brief M1–M12 (docs only)

- **Engineer:** engineer-2. **Schedule:** now, while T030b waits for T030 round 2. **Depends on:** nothing new.
- **Purpose:** give the project owner (Odin) and the user one document that shows how each requirement is verified: by which automated tests, and by which short manual check in a real terminal.

## Deliverable: `docs/acceptance/runbook.md`
For each of **M1–M12** in `docs/brief.md` v0.5, plus the "Should" items that were built (one filesystem by default, the size toggle, stop) and the UX §11 checks:
1. **The requirement**, quoted briefly.
2. **Automated evidence:** the exact test names (`file::test_name`) that cover it, and the command to run just those (`cargo test --test X -- name`). Include the opt-in commands: `SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests` for btrfs parity, and `-- --ignored` for the `unshare` mount tests.
3. **Manual check** (≤ 5 steps) in a real terminal, on the user's machine, using **only a scratch directory** for anything that deletes. For example: create a tree with `mkdir`/`head -c` under `/tmp/sa-accept`, run `cargo run --release -- /tmp/sa-accept`, press keys, and say what should be seen.
4. **Known limits and measured targets** where relevant: the R1/R2 numbers, and "ETA unknown on a first scan without a source" (M2 per ADR-010; the M2 amendment is with the project owner).

**Also:**
- **A short "full run" section:** the five README commands, plus the btrfs and `--ignored` variants, with expected test counts at the time of writing.
- **A traceability table** at the top: requirement → tests → manual check id.

## Rules
- **Docs only.** No source or test changes. If you find a requirement with **no** automated test, list it under "Gaps" and tell Heimdall; don't write the test here.
- Every test name must exist: run `cargo test -- --list` and check each name you cite.
- Manual checks must never touch anything outside a scratch dir. Deletes only under `/tmp/sa-accept`.

## Files owned
`docs/acceptance/runbook.md` (new).
