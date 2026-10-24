# T021 review (Heimdall, 2026-10-23): CHANGES REQUESTED. Architect second review requested in parallel.

**Fresh evidence** (clean worktree: HEAD plus the T021 files):
- fmt ok, whole-tree clippy clean;
- every suite passes **except one intermittent failure** (below);
- `-- --ignored` (delete_executor plus the app_delete mount-point test) passes under `unshare -rm`;
- release build ok.

**Real terminal** (by me, release binary under a pty at 100×30, on a scratch tree: `victim/{big.bin 3 MB, a.txt, sub/c.txt}` and `keep/{k.txt, link_to_a.txt}`, a hard link to `victim/a.txt`):

| Keys | Result |
|---|---|
| `j d n` | the dialog showed, with the "cannot be undone" line and the hard-link line; after `n`, **`victim/` still exists** ✓ |
| `j d y` | **`victim/` is gone**, and `keep/link_to_a.txt` still reads `hi`, so the outside hard link survived ✓; clean exit, no panic |

**Pass 1, spec:** the flow, refusals, unfinished-target live update, `p` in the dialog, progress, `Removed`/`Partial`/`Aborted` (including `MountPoint`), quit-while-deleting (only `y`), the signal path with stderr from the last progress, `last_delete_failures` with `v`, and the help entry are all present. Failure groups are sorted largest first ✓.

## Required changes
1. **Copy must be UX's exact text** (UX §5.3; "all UI text comes from the spec").

   | Constant | Current text | Spec text |
   |---|---|---|
   | `DELETE_UNFINISHED_WARNING` | `…including what is not listed yet.` | `Everything inside will be deleted, including files not counted yet.` |
   | `DELETE_HARD_LINKS_UNKNOWN` | `Hard links to files outside this folder are not fully known yet.` | `Hard links: not fully known (scan not finished).` |
   | hard-link line, exact target | `{n} inside have hard links outside this folder; their data stays on disk. At most {x} is freed.` | `Space freed may be less: N files have other hard links.` |
   | `ABORT_CHANGED` | — | `It changed since it was scanned (it may have been replaced or moved). Rescan with r, then try again.` |
   | `ABORT_NOT_FOUND` | — | `It no longer exists.` |
   | OpenFailed | — | `It couldn't be opened (errno N: <strerror>).` |
   | `DELETE_PRIORITISING` | without the glyph | `▲ Scanning this folder first…` (via `Glyphs`, so ASCII gives `^`) |
   | `FAILED_KEYS` (Unicode) | invented | the mockup's `j/k scroll  Esc close`, the same as ASCII |

   Update the snapshots. If you believe a spec string is wrong, raise it through Heimdall; don't substitute.
2. **ASCII `≥` must become `>=`** (UX §7), not `>`.
   - **Cause:** `ui/ascii.rs:42` folds one character to one character (from T023; Heimdall missed it in that review), and the dialogs contain a literal `≥`.
   - **Fix:** make the fold handle `≥` → `>=` (a string fold), and/or build the dialog's lower bounds from `Glyphs::lower_bound`.
   - **Test:** an unfinished-target dialog in ASCII contains `>= ` and no lone `>` before a size.
3. **Byte-order sort.** `result_dialog.rs:103` compares `PathBuf`s, which is component-wise. UX §5.3 says byte order. Compare `as_os_str().as_bytes()`, with a test where `a-b` sorts before `a/b`.
4. **Flaky test `app_controls::r_after_a_stop_is_a_full_rescan_at_once`** (committed in T020): it failed **3 of 30 runs when run alone** in my worktree.
   - **Cause (test race, not a product bug):** the gate was already opened with `grant(100)`, so the rescan of the small fake folder sometimes finishes before `assert!(rescan_in_flight)` runs. The status line `◌ Resuming a/…` in the failure shows the rescan did start.
   - **Fix:** hold the gate (grant nothing) while asserting `rescan_in_flight` and the message, then grant and wait.
   - **Prove it:** 40 consecutive runs of `cargo test --test app_controls` with 0 failures.
5. **The 3 tests that passed on stubs** (the parent-row hint, the running status, a second delete after the first finished): per the README rule, give one targeted breakage each, shown failing, or strengthen them until they fail against the stubs.

## Accepted
- NO_COLOR variants aren't separately snapshotted. Snapshots are text-only; T023's attribute tests cover NO_COLOR.
- Wrapped dialog lines lose their indent (existing `dialog.rs` behaviour). Non-blocking; noted for UX polish.
- Four `pub(super)` changes in `controls.rs`, `PanicSlot::take_if` in `terminal.rs`, and the `delete: None` line in `tests/app_eta.rs`.

## Second review
Requested from the architect (via the team lead) for `src/app/delete_flow.rs` and the quit/signal paths in `event_loop.rs`/`terminal.rs`, against ADR-007 and design §3.8. Items 1–3 don't touch that logic, so the review can run in parallel.

## Update 2026-10-23: UX text review + architect second review → Round 2 (supersedes the "Required changes" above)
**Correction (Heimdall):** item 4's diagnosis was **wrong**. The flaky `r_after_a_stop_is_a_full_rescan_at_once` is a **real race in the walker (T006)**, not a test race. The architect traced it: a worker in the cancel branch drains a rescan job pushed just after `Done`. It is moved to **T028 (engineer-1)**. **Do not change that test in T021.**

**UX ruled on items 1–3:** some of my required items are withdrawn, and UX's wording is final in `ux/spec.md` §5.3/§7/§7.1.
- **Withdrawn:** byte-order sorting of failed paths (accepted as built); ~~the Unicode `FAILED_KEYS` hint (accepted)~~ **reinstated: see item 9**; the `▲` on the prioritising hint (accepted as built, along with the other dialog text).
- **Wrapped dialog lines** returning to the 2-cell inner margin is now the stated rule. List items are indented 2 more cells, never wrap, and truncate with `…`.

**Round 2 (engineer-2, tests first), all in `ui/copy.rs` unless noted:**
1. **Hard-link line, fully scanned target:**
   - `Space freed may be less: N files also have hard links outside this folder, so at most X is freed.`
   - singular: `…1 file also has hard links outside this folder, so at most X is freed.`
2. **Unfinished target:**
   - the warning is `Everything inside will be deleted, including files not counted yet.`, with **no glyph** before it;
   - then, as its own paragraph, `Hard links: not fully known (scan not finished).`
3. **"Not deleted" reasons:**
   - `It changed since it was scanned (it may have been replaced or moved). Rescan with r, then try again.`
   - `It no longer exists.`
   - `It couldn't be opened (errno N: <strerror>).`
4. **ASCII `≥` → `>=`, never a lone `>`** (a lone `>` clashes with the selection marker). The fold in `ui/ascii.rs` must output the two-character string. **The ASCII SIZE column becomes 8 cells** instead of 7, so names get one cell less (UX §7/§7.1). Update the tree and list layouts and the ASCII snapshots.
5. **Architect T021 low #1, exit wait ~4 s instead of 2 s:** `stop_delete` waits up to 2 s, then dropping the handle waits up to 2 s more. Skip Drop's wait when `stop_delete` already waited, for example with a `DeleteHandle::detach()` that the quit path calls. **Test:** with an executor that never finishes, `q y` returns within 2.5 s.
6. **Architect T021 low #2, wrong exit message:** if the delete finishes during that wait, the message still says `interrupted: N of M`, because the drain loop stops at `Finished` and ignores it. Use the real outcome: print nothing for `Removed`, and report `Partial`/`Aborted` honestly. **Test:** an executor that finishes during the wait → no "interrupted" line.
7. **Architect T021 minor #3, silent close:** if the open confirmation's target becomes refused while the dialog is open (for example, it is now inside a running delete or gone), the dialog currently closes **without a message**. Show the refusal text (§5.3). **Test:** make the target refused while the dialog is open; the dialog closes and the status shows the refusal.
9. **Failure-list footer:** the spec mockup (§5.3) shows `j/k scroll  Esc close` in **both** Unicode and ASCII. Use it; the spec wins over the earlier "accepted" note. Update the snapshot.
8. **Prior item 5 still stands:** the 3 tests that passed on stubs need one targeted breakage each, shown failing.

**After round 2:**
- the five commands;
- the pty delete re-run, by me;
- I commit T021 once round 2 is done. T028 lands separately, and I'll re-run the app_controls loop after both.
