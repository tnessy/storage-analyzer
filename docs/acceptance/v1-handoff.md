# v1 hand-off to project-owner (Heimdall, 2026-10-23)

> **ON HOLD (2026-10-24):** the user added v1 scope (disk read speed in the status bar, T036). The acceptance pin below is **superseded** once T036 lands; a new pin and a fresh integration pass will follow.

**Status:** every planned v1 task is implemented, reviewed, and committed. **Acceptance build: code commit `979735e`.** Every later commit changes `docs/` only (`git diff 979735e HEAD -- src tests examples Cargo.toml Cargo.lock` is empty). The final acceptance commit is the one that adds this line (`git log -1 --format=%h -- docs/acceptance/v1-handoff.md`). Ready for acceptance using `docs/acceptance/runbook.md`.

## Integration evidence (fresh, clean worktree at HEAD)
- `cargo fmt --check` ok; `cargo clippy --all-targets --all-features -D warnings` clean; doc tests ok; `cargo build --release` ok.
- **`cargo test --all-targets`: 543 passed, 0 failed, 4 ignored**, in 3 consecutive clean runs.
- **`-- --ignored`** (mount-point tests under `unshare -rm`): all pass.
- **btrfs du parity** (`SA_TEST_BTRFS_DIR=$HOME/tmp/sa-tests`): 30/30.
- **Concurrency-sensitive suites:** app_controls, app_delete, ui_lists, top_reserve and scan_walker, 50/50 each.
- **Real terminal** (release binary, pty 100×30, scratch tree): scan to Done; Types, Largest, map focus, help; delete of the selected folder with confirmation; quit; terminal restored.

## Requirement status (details and checks in the runbook)
| Req | Status |
|---|---|
| M1 live scan | Frame 0 before the scanner starts (test-gated < 1 s; measured 0.1 ms); input handled first (p95 0.3 ms in test); apply tick max 9.2 ms on `$HOME` (was 118–147 ms) |
| M2 ETA | History (opt-in), in-session previous scan, inode count; otherwise **honest indeterminate progress** (ADR-010). **Owner action:** amend M2 ("ETA when a source exists"); see design §3.6 |
| M3 / M4 tree + map | Done, including sort, % of parent, sync, the `..` row, focus-aware selection, option C colours |
| M5 / M6 types + largest | Whole root, live; the largest list keeps a reserve so deletes rarely need a rebuild, with `updating…` when one does |
| M7 delete | Done, with `≥` bounds for unfinished folders. **Owner action:** update brief M7 wording (plan O6) |
| M8 accuracy | Byte-exact against `du` (disk, apparent, `-x`) on tmpfs and btrfs, with hard links, symlinks, sparse files, odd names and deep trees |
| M9 safe | Write guard in CI; no writes without `--history`; atimes unchanged; fd-relative, identity-checked delete (two reviews) |
| M10 robustness | Permission errors, vanishing files and racing deletes; issues listed; hostile file names never rendered raw |
| M11 scan controls | `p`, zoom, `x`, `r` (incl. "still scanning" = prioritise), `s` with the Stopping state; shallow-first |
| M12 quit | Always asks; during a delete only `y`; stderr reports N of M; exit never blocks |

## Decisions for the owner / user
1. ~~Memory budget~~ **Closed by the project owner (2026-10-23): acceptable for v1, no user decision needed.** The non-gating figure is **~4.0 GiB RSS / ~210 B per entry for ~20 M entries** (T031, real data). The R1 spike figures are superseded and are not carried forward. **Tell the user once:** a full `$HOME` scan uses about 4 GiB of memory.
2. **M2 and M7 brief amendments** (above).
3. **Types side panel at ≥ 120 columns (O2):** still deferred and not built.
4. **Measured targets stay non-gating (O3)** unless the user says otherwise:
   - first frame 0.1 ms;
   - input p95 0.3 ms;
   - tick max 9.2 ms;
   - delete check 26 ms per 1 M nodes;
   - one ~80–101 ms model-removal step for a 602 k-entry folder delete (accepted; the outcome is drawn first).

## Release notes: known limits of v1
- **Not built (deferred):** the Types side panel at ≥ 120 columns (UX §3, plan O2). The `2` Types view is the only file-type breakdown.
- **Test terminal is simulated:** the UI tests run on ratatui's `TestBackend` and a headless event loop, so first-frame and input-latency numbers are from that, not from a real terminal. Real-terminal behaviour (alternate screen, raw mode, signals, colour depth, resize) is covered only by manual checks and by the lead's pty smoke runs during the reviews.
- **NO_COLOR:** there are no separate NO_COLOR snapshots per dialog. Snapshots are text-only, and NO_COLOR is verified by attribute tests (no fg/bg colour on any cell) plus manual checks.
- **ETA on a first scan without history** is "unknown" by design (ADR-010).

## Measured footprint (non-gating)
- **Memory:** ~4.0 GiB RSS, ~210 B/entry, at 19.9 M entries on `$HOME` (glibc and mimalloc alike; not fragmentation).

## Backlog (not in v1)
- **T032:** cached sorting for folders with ~1 M children (84 ms per frame today).
- **O8:** remember a `MountPoint` delete refusal.
- **O9:** incremental `remove_subtree` (revisit if a real delete exceeds 250 ms; that is about 1.5 M entries in one folder).
- **Memory reductions:** a dir-table split, a compact hard-link table.
- **Cold-cache thread tuning:** not requested.
- **Export report:** a "Could" item, not designed.

## Command name
Still the placeholder `storage-analyzer` (`APP_ID` plus the Cargo bin name). Renaming touches `Cargo.toml` and `APP_ID` only.
