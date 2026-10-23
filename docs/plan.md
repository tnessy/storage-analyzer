# Build plan: `<cmd>` v1

Status: rev 1, 2026-10-23. Author: Heimdall (engineering lead).
Inputs: `docs/brief.md` v0.5, `docs/design.md` rev 6 (accepted), `docs/adr/001–009` (accepted), `docs/ux/spec.md` (layout A).

## 0. Preconditions and caveats
- **Design approval.** Done: `design.md` rev 6 is **accepted**, and all ADRs 001–009 are accepted (2026-10-23).
- **No git repository yet.** `/home/nss/workspace/tools/storage-analyzer` is not a git repo. I will run `git init` when I integrate T001. Engineers do not commit.
- **Commit convention** (none is specified in the design, so I set one): `<type>(<module>): <summary> (T0NN)`, where type is `feat|fix|test|chore|docs|spike`. One commit per approved task.

## 1. Scope decisions applied (from the team lead and Odin's validation)
1. **Targets vs requirements.** The architect's numbers are **targets to measure, not pass/fail gates**, unless the user agrees otherwise: input → render ≤ 50 ms p95, ≤ 120 B/entry, `min(nproc, 8)` threads by default, dialog hard-link check ≤ 50 ms per 1 M nodes, first frame ≤ 200 ms. Tests that measure them **print and record** the value; they do not fail on it. The only timing **gate** is brief M1: the first frame renders within **1 s**.
2. **Deferred:** the ≥120-column Types side panel (`ux/spec.md` §3, "Wide terminals"). This is pending a user decision. No task builds it; T023 makes sure the Tree layout ignores widths ≥ 120 (no panel).
3. **Command name** is a placeholder. `APP_ID` is one constant, and messages use `env!("CARGO_BIN_NAME")`. A test fails if the placeholder appears as a string literal anywhere else (T001).
4. **Odin gap 1: aborted delete of an unfinished folder.** On `Outcome::Aborted`, the model clears `TOMBSTONE`/`DELETING`, the app removes the subtree's cancelled prefix, and the subtree is rescanned (T011 model, T021 wiring). See §5 D1.
5. **Odin gap 2: missing acceptance tests.** Each one is assigned:
   | Req | Test | Task |
   |---|---|---|
   | M3 sort + % of parent | tree rows ordered `(size desc, name asc)`, % of parent at every level after expand/zoom | T016 |
   | M4 tree ↔ map sync | moving in either pane updates the other; the selected tile is double-bordered | T017 |
   | M5 file-type totals | per-ext/per-category size + count over the whole root, sorted by size, live | T005 (model), T018 (view) |
   | M6 live largest files | top-N updates between batches of `ScanMsg`, `--top N` honored | T005 (model), T018 (view) |
   | M11 shallow-first | queue order unit test, plus a walker test that level-1 is listed before any level-3 dir completes | T004, T006 |
   | M1 first frame ≤ 1 s | headless app test, frame 0 drawn before any worker or history read starts and within 1 s | T015 |
6. **One-filesystem hint (new).** When the default one-filesystem mode skips mounts or btrfs subvolumes, the status line shows a hint (for example, scanning `/` skips `/home`). Built in T020, using the UX-approved wording in `ux/spec.md` §5.2.
7. **Spikes R1–R5** are scheduled early: R3 first (T002), R1 right after the model core (T008), R2/R4/R8 right after the walker (T009), and R5 right after layout + walker (T022).

## 2. Ownership map (single writer per file)
| Area | Owner | Paths |
|---|---|---|
| Crate skeleton, shared contracts | engineer-1 (T001), then frozen: changes come through me | `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/types.rs`, `src/*/msg.rs`, `src/*/contract.rs`, `tests/write_guard.rs`, `tests/no_name_literal.rs` |
| scan | engineer-1 | `src/scan/**` (except `msg.rs`) |
| model | engineer-1 | `src/model/**` |
| delete | engineer-1 | `src/delete/**` (except `msg.rs`) |
| scan/model/delete integration tests | engineer-1 | `tests/scan_*.rs`, `tests/delete_*.rs`, `tests/model_*.rs`, `tests/support/fixtures.rs` |
| spikes R1, R2/R4/R8 | engineer-1 | `spikes/r1-memory/**`, `examples/scan_harness.rs` |
| app, ui, eta, layout, history | engineer-2 | `src/main.rs` (after T001), `src/app/**`, `src/ui/**`, `src/eta/**` (`contract.rs` only via the T013 delta), `src/layout/**` (except `contract.rs`), `src/history/**` |
| UI/app tests | engineer-2 | `tests/app_*.rs`, `tests/ui_*.rs`, `tests/snapshots/**`, `tests/support/synthetic.rs` |
| spikes R3, R5 | engineer-2 | `spikes/r3-probe/**`, `examples/spike_r5_jitter.rs` |
| Plan, briefs, review notes | Heimdall | `docs/plan.md`, `docs/tasks/**` |

- **Dependencies (`Cargo.toml`):** T001 adds every crate the plan needs. If an engineer needs another crate, they ask me. I edit `Cargo.toml` at integration. Engineers never edit it after T001.
- **Contracts are frozen after T001.** A change to a `msg.rs`/`contract.rs`/`types.rs` file is a request to me. I decide it once, brief both engineers, and route significant changes to the architect.
- Within one engineer's queue, a later task may take over a file from an earlier one (sequential, never concurrent). Each brief says which files it owns.

## 3. Tasks and dependencies

`→` means "depends on (must be merged first)". Tasks in the same wave on **different** engineers run in parallel.

### Wave 0: contracts
| Task | Engineer | Title | Depends on |
|---|---|---|---|
| T001 | e1 | Crate skeleton, shared contracts, write guard, APP_ID | — |
| T002 | e2 | Spike R3: Knuth probe ETA accuracy (standalone) | — |

### Follow-ups
| Task | Engineer | Title | Depends on |
|---|---|---|---|
| T024 | e1 | Harden the write guard and the APP_ID check (from the T001 review) | T001 |
| T025 | e2 | Selection styling, neighbour tile shades, `..` parent row (user feedback on the first build) | T016, T017, T018 |

### Wave 1: foundations (parallel across engineers)
| Task | Engineer | Title | Depends on |
|---|---|---|---|
| T003 | e1 | Model core: arena, apply, aggregation, completeness, hard links, issues | T001 |
| T008 | e1 | Spike R1: memory per entry | T003 |
| T004 | e1 | Scan job queue: shallow-first, focus re-key, frontier cap | T001 |
| T006 | e1 | Walker: workers, real fs, errors, one-filesystem, ScanControl, rescan | T004 |
| T007 | e1 | M8 du-parity + M10 robustness integration suite | T003, T006 |
| T013 | e2 | ETA estimator (pure) + EWMA rate + ADR-010 contract delta | T001 |
| T014 | e2 | Layout: squarify + neighbor (pure) | T001 |
| T015 | e2 | App skeleton: CLI, pre-TUI checks, terminal, event loop, frame 0, elapsed, idle quit | T001, T003 |
| T016 | e2 | Tree view: rows, sort, %, markers, selection, zoom | T015 |
| T022 | e2 | Spike R5: treemap jitter during a live scan | T003, T014, T006 |

### Wave 2: features
| Task | Engineer | Title | Depends on |
|---|---|---|---|
| T005 | e1 | Model indexes: ext/category totals, live top-N | T003 |
| T009 | e1 | Spike R2/R4/R8: headless scan harness | T005, T006 |
| T010 | e1 | Delete executor (safety-critical, second review) | T001 |
| T012 | e1 | Model scan controls: skip, resume, rescan, focus, stop | T003, T005, T006 |
| T011 | e1 | Model delete: delete_check, refusals, reconciliation incl. aborted-unfinished | T005, T010, T012 |
| T017 | e2 | Treemap widget, map focus, tree ↔ map sync | T014, T016, T005 |
| T018 | e2 | Types, Largest, Skipped views | T016, T005 |
| T019 | e2 | ETA sources (history, in-session, inodes) + progress status | T013, T015, T003 |
| T020 | e2 | Scan-control keys (p/zoom/x/r/s), messages, one-filesystem hint | T016, T018, T012 |
| T021 | e2 | Delete, result and quit-during-delete dialogs + delete flow | T010, T011, T018, T020, T023 |
| T023 | e2 | Size toggle, help, too-small, NO_COLOR/ASCII/contrast, snapshot set | T017, T018 |

### Suggested order per engineer
- **engineer-1:** T001 → T003 → T024 → T008 → T004 → T006 → T007 → T005 → T009 → T010 → T012 → T011
- **engineer-2:** T002 → T013 → T014 → T015 → T016 → **T017 → T018 → T025** → T022 → T019 → T020 → T023 → T021. The user approved moving T017/T018 ahead of T022/T019 on 2026-10-23. T022 and T019 are on hold until T017 and T018 are done. T017 therefore lands before R5: if R5 later recommends a different relayout policy, it is a follow-up to T017.

Waiting points: T015 waits for T003. T022 waits for T006. T017/T018 wait for T005. T020 waits for T012. T021 waits for T011. If engineer-2 is blocked, the next unblocked task in their queue goes first. T023 can move ahead of T019 or T020.

### Critical path
T001 → T003 → T004 → T006 → T005 → T010 → T012 → T011 → T021. The delete executor (T010) has only T001 as a dependency. If engineer-1 is ahead, they may pull it earlier, to leave time for the second review.

**ETA revision (2026-10-23):** the user approved ADR-010 / design rev 7. The probe is removed. T013 and T019 were rewritten, and the hold is lifted. T019's status copy waits for UX's §5.2 update. T003 gains `top_level_progress()`, which T011 and T012 keep correct. R1 (T008) now measures 20 M entries.

## 4. Integration checkpoints
1. **After T001:** `cargo build`, and the write-guard test passes. Contracts are frozen.
2. **After T007 + T015:** first usable build. `<cmd> ~` shows a live tree with elapsed time. M8 parity is green on tmpfs and btrfs.
3. **After T017–T020:** full exploration and scan control. Spike reports R1–R5 are reviewed. The architect gets the R-numbers for any ADR revisit.
4. **After T011 + T021:** delete end-to-end. The second review of `src/delete/**` and `src/model/delete.rs` is done. Every acceptance check in `ux/spec.md` §11 passes, except the deferred side panel.
5. **Acceptance hand-off to project-owner:** brief M1–M12 traceability table with the test names, and the measured (non-gating) targets.

## 5. Engineering decisions taken here
The design left these open. I made them so the briefs are complete. Each one has a single seam so it can change cheaply.

**Status (2026-10-23):**
- The architect accepted D1–D3 and D7–D9, recorded in design rev 6.
- UX accepted D5 and D6.
- UX re-worded D4 as below (`ux/spec.md` §5.2). T020 uses the UX wording.
- UX also added two rules: §3.2, the selected item is never hidden in the aggregate row (applied in T016), and §5.4, `x` on an already-skipped folder (applied in T020).
- **D1 (architect): aborted delete of an unfinished folder (Odin gap 1).** The design's reconciliation covers `Removed` and `Partial` only. On `Aborted`, the model clears `TOMBSTONE`/`DELETING` on the subtree and returns `Reconcile::Rescan(id)`. The app removes the cancelled prefix for that path, calls `Model::begin_rescan(id)` and `ScanHandle::rescan`. This is the same path as `Partial`. Rationale: confirming cancelled the subtree's pending jobs, so without a rescan the folder stays incomplete forever.
- **D2 (architect): rescan clears inner skips.** `begin_rescan(X)` removes every cancelled prefix at or under X's path. Otherwise a recreated skipped child would get a new NodeId without its `SkippedByUser` mark.
- **D3 (architect/UX): global status during a subtree rescan after Done.** The status line stays `✓ Done …`. Affected rows show `◌`. A later `ScanMsg::Done` does not reset elapsed time.
- **D4 (UX), superseded by the UX wording:** ` · not scanned (other fs): /home`, ` … /home +N`, shortened to ` · K other fs not scanned`. Original proposal: when ≥ 1 `OtherFilesystem` issue exists, the status line appends ` · skips /home` (the shallowest skipped path), or ` · skips /home +2 more` for several. When space runs out, the bar is dropped first, then the hint shortens to ` · 3 other fs skipped`. The hint never shows with `--cross-filesystems`. The copy lives in one constant in `src/ui/copy.rs`.
- **D5 (UX): tree aggregate rule.** `ux/spec.md` shows `(N smaller, SIZE)` rows but doesn't say when to group. Proposed: an expanded folder shows its 50 largest children, and the rest become one aggregate row (matching "50 rows at a time" on expand). The threshold is a constant.
- **D6 (UX): `x` on a folder that is already complete** does nothing and shows `Already fully scanned.` (constant).
- **D7 (contracts):** some types the design implies but doesn't spell out are defined in T001: `ScanState`, `SizeMode`, `Refusal`, `Reconcile`, `ScanHandle`, `DeleteHandle`, `ScanTotals`, `FsAccess`. Their meaning follows design §3.3–§3.11.
- **D11 (accepted by the architect with a change; design §4): `ScanMsg::Identity { node, entry: RawEntry }`**, a no-follow stat of the rescan target itself. If the target became a file or symlink, the walker sends only `Identity`, and the model converts the node, so a delete retry no longer loops on a kind mismatch. `Vanished` removes a node only when it is the rescan root. Superseded first draft: Design §3.8 now requires a rescan to re-statx the target and refresh its identity, and to remove the node if it is gone, but the `ScanMsg` contract had no way to carry that. A rescan job `fstat`s its opened dir and sends `Identity` before its `Listing`. ENOENT gives `DirIssue(Vanished)`, which removes the node when it is a rescan root (T006, T012).
- **D10 (architect FYI): the delete executor also compares `stx_mnt_id`**, so a same-device bind mount inside a delete target is not descended, and is reported as `on another filesystem (not crossed)`. The scanner's one-filesystem check stays st_dev-only (ADR-008). Stricter than ADR-007's st_dev rule, and safer.
- **D9: RETRACTED** (ADR-010 removed the probe). Original text: `ProbeMsg::Estimate { total_entries }` is a channel from the probe thread to the main thread. Design §4 lists four message types; the probe estimate is a fifth that the design implies (§3.2 diagram) but doesn't define. It is local to engineer-2's modules (T019).
- **D8 (contracts):** `DeleteFailKind` follows design §4 (no `Vanished`; ENOENT counts as success), not §3.10. §4 is the contract.

## 6. Open items needing someone else
| # | Item | Owner |
|---|---|---|
| O2 | Types side panel ≥120 columns: build it or drop it | user via project-owner |
| O3 | Do any measured targets become gates? (50 ms p95, 120 B/entry, thread default) | user via project-owner |
| O6 | Brief M7 update for `≥` lower bounds (design §6 gap 4) | project-owner |
| ~~O7~~ | **Closed (architect, design §3.11):** `r` on a still-scanning folder = `p` plus resume of the skipped folders inside it. No per-rescan generation (revisit only for v2 file watching). The rule lives in `Model::rescan_plan` (T012), and T020 executes it. The status text is pending from UX | — |
