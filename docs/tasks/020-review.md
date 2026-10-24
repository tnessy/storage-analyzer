# T020 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree: HEAD plus the T020 files):
- fmt ok, whole-tree clippy clean;
- every suite passes, 3 times (app_controls 24, lib 105, …);
- release build ok.

**Red first:** 23 of 24 failed on behaviour first. The 24th only checks that two copy constants exist. The node-keying change was mutation-checked (the old position scheme fails the held-cursor test).

**Real terminal** (by me, release binary under a pty at 120×30, on `$HOME`, a 19.6 M-entry live scan; keys `j p j x j r j r s r … q y`):
- `▲ First: …` and `Resuming` showed;
- `r` on a still-scanning folder gave `Still scanning xiv/: moved it to the front instead of restarting.`;
- `s` gave `■ Stopping…  00:06 elapsed`, then `■ Stopped at … · partial results · r …`;
- the skip / `partial` text showed;
- no panic, no `Scan error`, exit 0.

The "Still stopping" message didn't trigger because the stop completed before my `r`; the tests cover it.

**Pass 1, spec:** every T020 criterion and amendment is met:
- `p` toggles; zoom prioritises, and back to the root clears;
- the priority-done notice, with `▲` cleared;
- `x` with its `Already skipped` / `Already fully scanned` messages;
- `r` via `rescan_plan`, with the UX 0/1/N messages, root-while-scanning, and Skipped-row rescans;
- `p, x and r work on folders.`;
- the `..` interception stays first;
- **Stopping:** elapsed frozen, no ETA, `r`/`p`/`x` only show the message and are never replayed, a second `s` does nothing, an internal rescan is queued once until Done;
- one shared `AppState::rescan` (`remove_prefixes_under`, `forget_reserved`, `begin_rescan`, `ScanHandle::rescan`);
- the root restart uses `use_session_previous()` and updates the history;
- the one-filesystem hint in every state, with the narrow fallback;
- Skipped rows keyed by `NodeId` (D12).

**Model bug found by engineer-2, reproduced by me:** `begin_rescan` on a top-level dir whose entry carries an issue underflows `top_done` (debug panic at `control.rs:271`). Plus a gap I found: `r` on an `OTHER_FS` node would rescan another filesystem (ADR-008), because `rescan_plan` returns `Full` and the walker's rescan path doesn't check `dev`. Both are in **T027 (engineer-1, before T007)**.

**Concerns ruled:**
- `◌` staying on unfinished rows after `s` isn't asserted at the app level: accepted, the model tests cover it.
- `p`/`x`/`r` are ignored in Types and Largest: accepted. Those views list file types and files, and UX scopes `p`/`x`/`r` to folders in the Tree, map and Skipped.
- `ScanHandle::rescan` resets cancel-all: accepted. It is safe because T020 gates every rescan on Stopping being finished (`Done { cancelled: true }` applied).
