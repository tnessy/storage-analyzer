# T019: ETA sources (history, in-session, inodes) and progress in the status line

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T013, T015, T003 (`top_level_progress`).
- **Rev 2 (2026-10-23):** rewritten for ADR-010 / design rev 7 §3.6. The probe is removed. Renamed from `019-probe-history-eta-status.md`.
- **Copy: unblocked (2026-10-23).** UX updated `ux/spec.md` §5.2. **The spec is the source of truth for all status text, including the Indeterminate fields, their formats and the 80-column drop order.** The design's examples in criterion 3 are superseded wherever they differ. Known differences: `K of M top-level done` (shortens to `K/M`), a bytes-seen field, and `ETA unknown (no previous scan)`.
- **Resolved by UX (2026-10-23), same as the former provisional rule:** §5.2's Starting row (`ETA estimating…` for the first 2 s) contradicts the accepted rule that `Estimating` needs a source (design §3.6 rev 8, T013). Until UX answers (Heimdall has flagged it):
  - while `ScanState::Starting`, or for elapsed < 2 s, render `◌ Starting…  00:00 elapsed`;
  - append `  ETA estimating…` **only** if `eta::view` returns `Estimating`;
  - with no source, append nothing.

  Keep the decision in one place (`status.rs`) so the wording can change cheaply.

## Goal
Brief M2 under ADR-010:
- an ETA, labeled with its source, **when a reliable source exists**;
- otherwise indeterminate progress made only of true quantities;
- revised when a source appears mid-scan;
- Done at completion.

The history file is strictly **opt-in**, with zero writes by default.

Acceptance criteria:
1. **Sources**, in priority order (design §3.6). The first one available is used:
   1. **History file** (only with `--history` or the config key `history = true`):
      - read `$XDG_STATE_HOME/<APP_ID>/history.json` (fallback `~/.local/state/<APP_ID>/`) **after frame 0**;
      - on `Done { cancelled: false }` with no `SkippedByUser` issues, write `{ root (canonical), options, entries, dirs, bytes, duration_ms }` atomically (temp file in the same dir + `rename`);
      - keep at most 50 roots;
      - a corrupt file is ignored with a transient message and isn't overwritten until the next successful write.
      - When disabled, **no file is read or written.**
   2. **In-session previous scan** (`SessionPrevious`; new, zero writes):
      - on a complete `Done` of the **root** (not cancelled, no user skips), store `{ root, options, entries }` in `AppState`;
      - when the root is rescanned in the same session (`r` on the root after Done or Stop, T020), use it as the source;
      - subtree rescans don't use it and don't update it;
      - it is never persisted.
   3. **Filesystem inodes** (`FsInodes`): if the root is the mount root (root `st_dev` ≠ parent `st_dev`, or root is `/`) and `statvfs.f_files > 0`, then total = `f_files − f_ffree`. Use read-only `rustix::fs::statvfs` behind a small trait so tests can fake it.
   4. **None** → `eta::view` returns `Indeterminate`.
2. **Wiring:** on each 100 ms data tick, build `EtaInput` from:
   - `Model::totals()` (seen, pending_dirs);
   - `Model::top_level_progress()` (top_done, top_total);
   - `Rate` (T013);
   - the best available source.

   Then call `eta::view`.
3. **Status text** (from `ux/spec.md` §5.2; the examples below are orientation only):
   - **With a source:** `◌ Scanning [bar] ~41%  01:12 elapsed  ETA ~01:44 (estimate, last scan)`. The labels are `last scan` (History and SessionPrevious) and `filesystem` (FsInodes). Estimating → `ETA estimating…`; OverEstimate → `~99% … ETA unknown (past estimate)`.
   - **Indeterminate:** `◌ 01:12 · 1.24M entries · 41k/s · ≥12.4k folders queued · 12/31 top-level done · ETA unknown (no estimate on first scan)`.
     - No bar and no `%` ever appear.
     - The rate is omitted while `rate_eps` is `None`.
     - The top-level count is omitted until `top_total` is known.
     - On narrow widths, drop segments in the order UX specifies.
   - **Done:** `✓ Done in …`. **Stopped:** `■ Stopped at … · partial results · r on the root rescans`.
   - The activity glyph is the spinner (≤ 10 fps, static with `--no-animation`).

## Design references
§3.3 first paint, §3.6 (rev 7), §4 Security (zero writes), §4 Testing (History). ADR-010 (supersedes ADR-005's source list). UX §5.2 (being updated), §11 check 4.

## Files owned
`src/history/mod.rs`, `src/eta/sources.rs`, `src/ui/status.rs`, `src/ui/copy.rs`, `src/app/event_loop.rs` (ETA wiring), `src/app/state.rs` (session-previous record), `tests/app_eta.rs`, `tests/history.rs`.

## Interfaces
**Consumes:** `Rate`/`view`/`EtaInput` (T013), `Model::totals` and `Model::top_level_progress` (T003). **Provides:** `EtaSources` (source selection), used by the event loop.

## Tests to write first
- **Source priority:** with all three available, History wins. Without history, SessionPrevious wins. Without both, FsInodes. With none, the view is Indeterminate.
- **Inodes:** a fake `statvfs` with `f_files = 0` (btrfs) → unavailable. `f_files > 0` at the mount root → the expected total. Not the mount root → unavailable.
- **History zero-write (design §4):** run the app headless with a temp `XDG_STATE_HOME`, without `--history`, through Done → the dir is still empty, and the injected store saw zero read and zero write calls.
- **History on:** the first run writes the file atomically (no temp file left behind). The second run uses source `History`, label `last scan`. Capped at 50 roots. A corrupt file is ignored.
- **In-session:** scan to Done, then `r` on the root → the rescan shows a source-based ETA with `last scan`. A cancelled first scan, or one with user skips, does **not** create the record. A subtree rescan neither uses nor updates it.
- **Mid-scan indeterminate → ETA:** a headless scan with `--history`, where the history store's read is delayed until t = 5 s on a fake clock. The status text is indeterminate (no `%`, no ETA number) before the read, then `ETA estimating…`, then `~NN%  ETA ~MM:SS (estimate, last scan)`. It never returns to indeterminate.
- **Indeterminate content:**
  - with no source, the text never contains `%` or an ETA time;
  - it contains the entries seen, `≥N folders queued` equal to `pending_dirs`, and `K/M top-level done` matching the model;
  - it ends with the "ETA unknown" copy.
- **80-column drop order:** at 80 columns with no prefix, the Indeterminate line equals the spec's 79-column example. Each drop step happens in the spec's order, so test widths that trigger each step. The state glyph and word, elapsed, and `ETA unknown` are never dropped.
- **No-bar rule (spec §11):** in `Indeterminate`, the status line has no `%`, none of the bar glyphs `█▉▊▋▌▍▎▏░`, and no ETA digits. The text after `ETA` matches `^ETA unknown` at 80, 120 and 200 columns.
- **UX §11 check 4** (as updated by UX):
  - no digits after `ETA` unless there is a source;
  - `~` and `estimate` are present whenever a number is shown;
  - `00:00` or `Done in` after Done;
  - `~%` never reads `100` before Done.

Commands: `cargo test --test app_eta --test history`, plus the five standard commands.

## Out of scope
The probe (removed). Delete progress text (T021). The estimator itself (T013).
