# T019: Probe module, opt-in history, ETA in the status line

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T013, T015, T006 (`RealFs`), T002 (R3 report).

## Goal
Complete brief M2: an ETA that is shown, labeled as an estimate with its source, revised during the scan, says `estimating…` when it has no basis, and shows 00:00/Done at completion. The prior-scan source is strictly **opt-in** (`--history`), with zero writes by default.

Acceptance criteria:
1. **`probe`** (design §3.6, ADR-005):
   - a read-only thread runs K = 64 random root-to-leaf descents over `FsAccess` (use `RealFs` from T006 in production and `FakeFs` in tests);
   - it estimates N = Σ_d Π_{i≤d} b_i per descent and averages the results;
   - it stays on one filesystem when that mode is on, and never follows symlinks;
   - it sends `ProbeMsg::Estimate { total_entries }` once on a channel owned by `probe` (plan D9);
   - it is spawned **after frame 0**, and `--threads` doesn't count it.
   - If T002's report recommends showing a range or hiding the ETA until N %, **ask Heimdall before implementing it**. The brief will be amended.
2. **Filesystem-inodes source:** if the root is the mount root (root `st_dev` ≠ parent's `st_dev`, or root is `/`) and `statvfs.f_files > 0`, then total = `f_files − f_ffree`. Use `rustix::fs::statvfs`, which is read-only.
3. **History** (only with `--history` or the config key `history = true`):
   - read `$XDG_STATE_HOME/<APP_ID>/history.json` (fallback `~/.local/state/<APP_ID>/`) after frame 0;
   - on `Done { cancelled: false }` with no `SkippedByUser` issues, write `{ root (canonical), options, entries, dirs, bytes, duration_ms }` atomically (temp file in the same dir + `rename`);
   - keep at most 50 roots;
   - a corrupt file is ignored with a transient message and is not overwritten until the next successful write.
   - When disabled, **no file is read or written**.
4. **Source priority:** history, then fs inodes, then probe, then none. The app builds `EtaInput` on each data tick from `Model::totals()`, `Rate` (T013) and the best available source, and calls `eta::view`.
5. **Status line text** (UX §5.2):
   - `◌ Scanning [bar] ~41%  01:12 elapsed  ETA ~01:44 (estimate, rough)`;
   - the source labels are `rough` (Probe), `filesystem` (FsInodes) and `last scan` (History);
   - Estimating → `ETA estimating…`;
   - OverEstimate → `~99% … ETA unknown (past estimate)`;
   - Done → `✓ Done in …`, with ETA 00:00 when it is shown;
   - Stopped → `■ Stopped at … · partial results · r on the root rescans`;
   - the bar is 16 cells from `fraction`, and it is dropped first when space is short.

## Design references
§3.2 probe/history rows, §3.3 first paint, §3.6, §4 Security (zero writes), §4 Testing (History). ADR-005. UX §5.2, §11 check 4.

## Files owned
`src/probe/mod.rs`, `src/history/mod.rs`, `src/eta/sources.rs`, `src/ui/status.rs`, `src/app/event_loop.rs` (ETA wiring), `tests/app_eta.rs`, `tests/history.rs`.

## Interfaces
**Consumes:** `FsAccess`/`RealFs` (T006), `Rate`/`view` (T013), `Model::totals` (T003). **Provides:** `ProbeMsg` (local to engineer-2's modules; Heimdall records it as a contract, D9).

## Tests to write first
- **Probe:** on a uniform `FakeFs` tree it returns the exact count. It never calls `open_dir` on a symlink, and stays within `st_dev` when one-filesystem is on.
- **Inodes:** a fake `statvfs` with `f_files = 0` (btrfs) → unavailable. `f_files > 0` at the mount root → the expected total. Not the mount root → unavailable.
- **History zero-write (design §4):** run the app headless with a temp `XDG_STATE_HOME` and no `--history` through Done → the directory is still empty and no read was attempted (inject the history store behind a trait and count calls).
- **History on:** first run → file written atomically (no temp file left behind). Second run → source `History`, label `last scan`. Capped at 50 roots. A corrupt file is ignored.
- **UX §11 check 4:**
  - no digits after `ETA` while Estimating;
  - `~` and `estimate` are present whenever a number is shown;
  - `00:00` or `Done in` after Done;
  - `~%` never reads `100` before Done.

  Run this over a synthetic 30 s scan with a fake clock.
- **M2 revised:** the ETA text changes when a better source arrives mid-scan (probe → inodes in a fake).

Commands: `cargo test --test app_eta --test history`, and `cargo test probe::`.

## Out of scope
Delete progress text (T021). Changing the estimator (T013).
