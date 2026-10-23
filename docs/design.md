# Design: `<cmd>` (disk-usage TUI) v1

> **Name placeholder:** the command name is TBD. Docs use `<cmd>` for the command and `<app-id>` for directory names. Code MUST NOT hardcode it: one constant `APP_ID` (used for XDG paths and config) and the binary name taken from Cargo (`env!("CARGO_BIN_NAME")`) for messages. Renaming then means changing `Cargo.toml` + `APP_ID` only.

Status: **accepted**, rev 9, 2026-10-23. Author: Mimir (architect). Inputs: `docs/brief.md` v0.5, `docs/ux/spec.md` (final, layout A), `docs/ux/directions.md` (§6 accessibility).

Rev 9 (2026-10-23): T002 rework recorded. Variant B fails, R3b is closed, and there is no probe in v1.

Rev 8 (2026-10-23):
- The user approved ADR-010.
- The probe finding is scoped to the variant tested.
- The §3.6 table is made consistent: `Estimating` needs a source; with no source the state is `Indeterminate` from the start.

Rev 7 (2026-10-23): spike R3 failed, so the Knuth probe is removed. The ETA now comes from history (opt-in), an in-session previous scan, or the inode count; otherwise an honest `Indeterminate` progress state (§3.6, ADR-010). `ProbeMsg` and the probe thread are removed (plan D9 retracted).

Rev 6 (2026-10-23): acknowledged plan §5 D1–D3, D7–D9. Changes:
- Aborted deletes trigger a rescan.
- A rescan clears skips inside its folder.
- Status stays Done during a post-scan rescan.
- `ProbeMsg` added.
- §3.10 `DeleteFailKind` aligned to §4.

Rev 5 (2026-10-23):
- Marker semantics aligned with `ux/spec.md` §5.1: `◌` means work pending (`!is_complete`); `≥` means lower bound (`!is_exact`). See §3.11.
- Dialog `p` = "finish scan first" with live numbers (§3.11).
- `r` resumes skipped folders.
- Traceability for brief M11/M12.

Rev 4 (2026-10-23): user chose **UX layout A** (tree on top, treemap strip below). Zoom **and** `p` both prioritise (confirmed). `--max-depth` is rejected (confirmed).

Rev 3 changes (2026-10-23):
- Acting before the scan finishes: options and recommendation (§3.11, ADR-009). Delete of unfinished folders is now allowed with `≥` bounds.
- Quit always needs a second confirmation (§3.8).
- Command name is a placeholder (`<cmd>`).

Rev 2 changes:
- **User decisions:** history file is opt-in; stay on one filesystem by default; build, not adopt.
- **UX backend needs added (§3.10):**
  - per-folder complete flag
  - first paint < 1 s
  - redraw cap
  - hard-link count
  - delete progress
  - skip reasons
- **Aligned with UX:**
  - map scope rule
  - Enter cancels the delete dialog
  - delete blocked on incomplete subtrees (**superseded in rev 3**: unfinished folders can be deleted with `≥` bounds, §3.11)
Labels: **Fact** (read from a file, a tool, or a source this session), **Assumption**, **Open question**.

---

## 1. Problem and constraints

Pick a root folder and get a live, non-blocking view of where the space goes: a tree (root at top, children below, largest first), a treemap of the current directory, a whole-root file-type breakdown and largest-files list, plus permanent delete with confirmation. Interactive TUI on Linux only.

Binding constraints from the brief:
- **M1** The UI is interactive within 1 s and stays responsive while the scan runs. This is the only binding performance requirement. No throughput target until benchmarks (brief, Constraints).
- **M2** Elapsed time plus an ETA that is labeled as an estimate, says "estimating" when it has no basis, and converges to 0 at completion.
- **M8** Totals match `du` in the same mode. Hard links count once. Symlinks are not followed.
- **M9** Nothing is modified except through M7.
- **M10** Permission errors and vanishing files do not crash the scan, and skipped paths are reported.
- The user wants a new tool, not an existing one (brief, Decisions). Single user, no root, no network.

Environment facts (checked on the user's machine):
- Kernel 7.2.9-cachyos, 16 cores, 60 GiB RAM.
- `/home` is **btrfs** (`compress=zstd:1`, `noatime`, subvol `@home`).
- Rust 1.99.0 and Go 1.27.1 are installed. GNU du is 9.12.
- `statvfs` on btrfs returns `f_files = 0`, `f_ffree = 0`, so no inode counts are available. This matters for ETA (§3.6).
- `/` and `/home` have different `st_dev` (36 vs 59). btrfs subvolumes each get their own device ID. This matters for "one filesystem" (§3.5).
- GNU du 9.12 on tmpfs: in `--apparent-size` mode directories contribute 0. In block mode a directory contributes its `st_blocks`. A hard-linked file is charged to the **first path du visits**. Symlinks count their own size (4 B apparent). (Measured in a scratch tree.)

## 2. Prior art

| System | What it does well | Where it falls short for us | What we borrow / where we diverge |
|---|---|---|---|
| **WinDirStat / KDirStat** | A tree list coupled to a treemap. Squarified and cushion treemaps from the Bruls/Huizing/van Wijk papers ([WinDirStat background](https://windirstat.net/background.html)) | GUI, Windows | **Borrow:** the coupled tree and treemap, squarified layout. **Diverge:** no cushion shading (terminal cells), one level at a time |
| **SequoiaView** (TU/e) | Origin of squarified and cushion treemaps ([SequoiaView](https://www.win.tue.nl/sequoiaview/Main_Frame/Info.html)) | GUI | Algorithm lineage only |
| **Squarified treemaps**, Bruls, Huizing, van Wijk, VisSym 2000 ([paper](https://diglib.eg.org/handle/10.2312/VisSym.VisSym00.033-042)) | Near-square rectangles that are easy to compare and select | Layout shifts when sizes change ([Bederson, Shneiderman, Wattenberg 2002](https://umiacs.umd.edu/node/17312)) | **Borrow:** the algorithm. **Mitigate:** instability under live updates by throttling relayout (ADR-006) |
| **Ordered treemaps** (Bederson et al. 2002) | More stable as data changes | Worse aspect ratios. Order is by input, not size | Fallback if the jitter spike fails (R5) |
| **ncdu 2** (Zig) ([design notes](https://dev.yorhel.nl/doc/ncdu2)) | Low memory: separate structs for dir, file and hardlink nodes. Efficient hard-link counting | List-only layout (user's complaint). Single-threaded scan | **Borrow:** compact per-kind node storage, the idea of a hard-link table |
| **gdu** (Go) ([man](https://man.archlinux.org/man/extra/gdu/gdu.1.en)) | Parallel scan aimed at SSDs, `--max-cores` | No treemap. No ETA found in its docs | **Borrow:** parallel by default with a configurable thread cap |
| **dua-cli** (Rust) ([docs](https://docs.rs/crate/dua-cli/2.9.1)) | Parallel jwalk-based scan. Interactive mode with a multi-stage delete guard | No treemap, no ETA | **Borrow:** delete as a deliberate multi-step action, never a default key |
| **jwalk** (Rust walker) | Parallel, rayon-based, streams sorted entries | **Fact:** docs.rs 0.9.0 (2026-08-05) marks it "no longer maintained", use `dua-core` ([docs.rs](https://docs.rs/crate/jwalk/latest)) | Not adopted (ADR-002) |
| **dust** (Rust) ([README](https://github.com/bootandy/dust)) | Fast, multithreaded (`-T`) | Static output, no interaction, no delete. **Fact:** has `-r` "reverse order of output" | See finding below |
| **diskonaut** (Rust) ([repo](https://github.com/imsnif/diskonaut)) | A navigable terminal treemap exists, so the idea works | No tree pane, no ETA. Activity not verified | Confirms the terminal-treemap UX is viable |
| **Rust CVE-2022-21658** ([advisory](https://groups.google.com/g/rustlang-security-announcements/c/R1fZFDhnJVQ)) | Shows how a check-then-delete sequence enables a symlink TOCTOU attack | — | **Borrow the lesson:** delete by file descriptor with no-follow flags, never check-then-act (ADR-007) |
| **Knuth 1975**, "Estimating the efficiency of backtrack programs" ([AMS](https://www.ams.org/journals/mcom/1975-29-129/S0025-5718-1975-0373371-6/home.html)) | An unbiased tree-size estimate from random root-to-leaf probes | High variance on skewed trees | **Tried and rejected** by spike R3: on real heavy-tailed trees it is 100–1000× low in most runs (ADR-010) |

**Finding (closed):** dust has `-r` (reverse output), but no interactive UI, treemap or delete. The user confirmed: build, not adopt (2026-10-23).

## 3. Architecture

### 3.1 Stack (ADR-001)
Rust (stable, 1.99 installed), **ratatui 0.30.x** with the crossterm backend (crossterm 0.29), and **rustix 1.x** for fd-relative syscalls. **Fact:** `rustix::fs` exposes `openat`, `openat2`, `statx`, `unlinkat`, `Dir`/`RawDir`, `AtFlags`. ratatui exposes `TestBackend`. Engineers pin exact versions in `Cargo.lock`. No async runtime: plain OS threads and channels.

### 3.2 Components and boundaries

```
             ┌──────────── main thread (owns everything mutable) ─────────────┐
 keys ─────► │ app: event loop, AppState, keymap                               │
             │   ├─ model: arena tree + indexes (ext, top files, hardlinks)    │
             │   ├─ eta: pure estimator  ◄── estimate sources                  │
             │   ├─ layout: pure squarify                                      │
             │   └─ ui: views/widgets (render only; read-only on model)        │
             └───────▲───────────────────────────▲─────────────────────────────┘
        ScanMsg      │ bounded channel            │ DeleteEvent
             ┌───────┴────────┐          ┌───────┴────────┐   ┌──────────────┐
             │ scan: N worker │          │ delete: 1      │
             │ threads, queue │          │ executor thread│   │ sampler thr. │
             └────────────────┘          └────────────────┘   └──────────────┘
```

| Module | Responsibility | May write to the filesystem? |
|---|---|---|
| `scan` | Parallel walker. Emits `ScanMsg`. Also handles subtree rescans | **No** |
| `model` | Arena tree, aggregates, ext index, top-N files, hard-link table, error list. Applies `ScanMsg` and `DeleteEvent` | No (memory only) |
| `eta` | Pure function from progress to `EtaView`. Estimate sources | No |
| `history` | Reads and writes the prior-scan record (ADR-005). **Opt-in, off by default** | Its own state file, only with `--history` |
| `layout` | Pure squarified layout over cells | No |
| `ui` | Rendering, dialogs, name sanitising | No |
| `delete` | The **only** module allowed to remove anything (M9) | **Yes, deletions only** |
| `app` | Event loop, state machine, CLI parsing | No |

### 3.3 Concurrency model (ADR-003)
- The **main thread owns the model**. Single writer, no locks on the tree. Workers never touch the model.
- Each loop iteration:
  1. poll crossterm with a timeout until the next tick;
  2. handle input **first**;
  3. drain `ScanMsg`/`DeleteEvent` within a time budget (default 8 ms);
  4. render (rules below).
- **Redraw cap:**
  - Data-driven redraws (scan or delete progress, spinner) happen at most every 100 ms (≤ 10 Hz, UX §6 motion rule).
  - Input-driven redraws happen immediately, coalescing all input read in that iteration into one frame.
  - Treemap relayout ≤ 2 Hz while scanning (ADR-006).
  - When nothing changes, the loop blocks on input with no tick.
  - `--no-animation` freezes the spinner glyph.
- **First paint (< 1 s, M1):**
  - Before the TUI starts, `main` only validates PATH (`statx` and an open of the root dir fd). Path errors go to stderr with a non-zero exit (UX §5.4).
  - It then enters the alternate screen and **renders frame 0 (root row, `◌ Starting…`) before spawning the walker or reading the opt-in history file**.
  - No other work happens before frame 0. Target ≤ 200 ms.
- The scan channel is **bounded**. If apply falls behind, workers block (backpressure). The scan slows but the UI never stalls. M1 wins over throughput by design.
- **Ordering contract:** a worker MUST send the `Listing` for directory D before it enqueues D's child jobs. The model can then rely on any listing's parent node already existing.
- **Node ID allocation:** a shared `AtomicU32` hands out a contiguous block per listing (`first_id .. first_id + entries.len()`). The model grows its vectors to fit.
- **Completion:** a shared pending-jobs counter. When it reaches 0, the scan sends `ScanMsg::Done`.
- **Cancel (Should):** a shared `AtomicBool`. Workers stop taking jobs. Unread dirs are marked `INCOMPLETE`. The scan sends `Done { cancelled: true }`.

### 3.4 Data model (ADR-004)
- **Arena:** `Vec`-backed, indexed by `NodeId(u32)`. Root = `NodeId(0)`. Up to 4.29 billion entries.
- **Names:** stored as raw bytes (`OsStr` semantics, not UTF-8) in a byte arena. **Nodes store only their name, never full paths.** Paths are rebuilt by walking parents. This avoids per-node heap strings.
- **Children:** each directory has `Vec<NodeId>`. Visible, expanded directories are sorted lazily at render time by `(size desc, name asc)`. The name tie-break prevents flicker.

Per-node logical fields (the physical layout, such as split file/dir tables, is the engineers' choice within the memory budget, §5):

| Field | Type | Notes |
|---|---|---|
| parent | NodeId | `NONE` for root |
| name | NameRef | into the byte arena |
| kind | File / Dir / Symlink / Other | from `statx`, never followed |
| own_disk, own_apparent | u64 | `st_blocks*512` and `st_size`. **Both 0 if this is a duplicate hard link.** Directory `own_apparent` = 0 to match du (§1) |
| agg_disk, agg_apparent | u64 | dirs only: subtree totals, updated incrementally |
| files, dirs | u64 | dirs only: subtree counts (used by the delete dialog) |
| pending_dirs | u32 | dirs only: discovered-but-not-yet-listed dirs in this subtree, **including itself**. Drives `complete` (§3.10) |
| nlink | u32 | files only. Needed for the hard-link note (§3.10) |
| issue | Option<IssueIdx> | index into the issue list if this node was skipped or errored, so the tree can show `! name (reason)` |
| ino, dev_idx | u64, u16 | identity for the delete TOCTOU check. `dev_idx` indexes a small device table |
| ext | ExtId | files only |
| flags | bitset | `PENDING`, `DONE`, `INCOMPLETE`, `ERROR`, `OTHER_FS`, `HARDLINK_DUP`, `DELETING`, `TOMBSTONE` |

- **Aggregation:** when a listing is applied, each entry's own sizes and counts are added to every ancestor. This costs O(depth) per entry and gives correct live partial totals for M1, M3 and M4.

Indexes, all updated in `apply`:
- **Hard links:** `HashMap<(dev, ino), HardLink { counted: NodeId, others: SmallVec<NodeId> }>`, kept only for `nlink > 1` non-directories. The first occurrence applied is charged, later ones are `HARDLINK_DUP`. **Consequence (fact-based, see §1):** totals match du, but which directory gets charged can differ from du and between runs because parallel order is nondeterministic. This is documented in the UI help.
- **Extensions:** `ext_of(name)` is the lowercased bytes after the last `.`. Leading-dot names (`.bashrc`) and names with no dot map to `(none)`. Extensions longer than 16 bytes map to `(other)`. `.tar.gz` maps to `gz` in v1. Per-ext `{disk, apparent, count}` cover the whole root (M5). Duplicate hard links are excluded, consistent with the totals.
- **Top files:** top-N by the current size mode (default N = 100, `--top N`). It is a bounded min-heap during the scan. After a delete the list must stay correct: recompute with an O(files) pass. If that pass exceeds the frame budget at real scale, move it to a helper thread (R2).
- **Issues (skipped paths):** `Vec<ScanIssue { node: NodeId, op: Open|Read|Stat, kind: IssueKind }>`, where `IssueKind = PermissionDenied | Vanished | OtherFilesystem | Other(errno)`.
  - The path is rebuilt from `node` on display.
  - Issues are counted in the status bar and listed in the Skipped view (M10, UX `4`).
  - UX reason text: `permission denied` / `vanished during scan` / `other filesystem` / `read error: <strerror>`.
- **Category:** `category_of(ext) -> Category { Video, Image, Archive, CodeText, Binary, Documents, AudioOther }`. It is a static table, used by the Types view grouping and the treemap color/letter tag (UX §6). The table contents are owned by UX + engineers. Per-category totals are sums of per-ext totals; no separate index.
- **NodeId stability:** a NodeId never changes or gets reused while its node exists. The UI tracks selection by NodeId, so selection follows the item across live re-sorts (UX F2).

### 3.5 Scan (ADR-002)
- **Workers:** `--threads N`, default `min(available_parallelism, 8)` (**assumption**, tune after benchmarks).
- **Job queue:** one shared queue. It MUST:
  - support many producers, many consumers, and blocking pops with cancellation;
  - be **priority-ordered** by `(in_focus desc, depth asc)` (shallow-first, ADR-009), falling back to deepest-first above 1 M pending jobs;
  - be re-keyable when the focus changes;
  - honor `ScanControl` (§3.11).
- **Job:** `{ dir: NodeId, path: PathBuf (absolute), depth }`. Paths live only in jobs, never in nodes.
- **Per job:**
  1. open the directory `O_RDONLY|O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC`, adding `O_NOATIME`, and retry without it on `EPERM` (M9: no atime writes);
  2. iterate the entries;
  3. for each entry, `statx(dirfd, name, AT_SYMLINK_NOFOLLOW, mask = TYPE|MODE|NLINK|INO|SIZE|BLOCKS)`;
  4. send one `Listing`;
  5. enqueue child dirs.
- **Symlinks:** never followed (M8). They count their own size.
- **One filesystem** (Should, ADR-008):
  - **Default on** (user decision, ADR-008 accepted). `--cross-filesystems` turns it off.
  - An entry directory whose `st_dev` differs from the root's is recorded with `OTHER_FS` plus a `ScanIssue { kind: OtherFilesystem }`, so it appears in the Skipped view. It is not descended into and not counted, matching `du -x` (verified by the M8 test).
  - On btrfs, nested subvolumes (for example snapper `.snapshots`) have their own `st_dev`, so they are skipped under `-x`. This matches du.
- **Errors:**
  - `EACCES`/`EPERM` → `ScanIssue::PermissionDenied`.
  - `ENOENT` on open or stat → `Vanished`.
  - Anything else → `Other`.
  - Never panic, always continue (M10).
  - Errors under a `TOMBSTONE` subtree (being deleted) are discarded, not reported.
- **Subtree rescan:** the same walker, seeded with `{dir: existing NodeId}`. The model first clears that subtree's children and subtracts its aggregates. It is used for post-delete reconciliation and for the Could-item manual rescan (`r`).
- **Fault-injection seam:** the walker reaches the filesystem through a narrow internal trait (`open_dir`, `read_entries`, `stat_at`). Tests can inject `EACCES` or `ENOENT` mid-scan (M10). This seam exists only for testability.

### 3.6 ETA (ADR-005)
- **Work unit:** entries (directory entries `statx`'d), not bytes. Scan cost is per-entry syscalls. Bytes only proxy for time (**assumption**, checked by spike R3).
- **Spike R3 result (fact, `spikes/r3-probe/REPORT.md`):** the probe variant tested is unbiased but unusable on real trees. In that variant, each step picks a random child over *all* entries, so a descent stops at the first file it picks. A subdirectory-descent variant (B) was measured afterwards and also fails (K=256 median ratio 0.07 on $HOME). There is no probe in v1 (ADR-010).
  - On `$HOME` (19.46 M entries), 96–98 % of seeds are below 0.5× the truth; the median ratio is 0.001–0.005.
  - On `/usr` and a node_modules project, 72–92 % of seeds are below 0.5×, with rare overshoots of about 84× in ETA error.
  - The ADR-005 revisit trigger fired. **The probe is removed** (ADR-010 supersedes ADR-005's source list).
- **Principle:** show a number only when it has a source that holds up. Otherwise show honest, *measurable* progress, never a percentage or time.
- **Total-entries sources,** in priority order. The first available source is used, and its name is shown in the UI.
  1. **Prior scan, history file (opt-in only).**
     - **Off by default** (user decision). Enabled with `--history` or config key `history = true`.
     - When enabled, read and written at `$XDG_STATE_HOME/<app-id>/history.json`, falling back to `~/.local/state/…` if unset.
     - The file holds only the last *complete* scan per (canonical root, options): entries, dirs, bytes, duration. It is written atomically (temp + rename) and capped at 50 roots.
     - When disabled, the tool performs **zero writes** outside `delete`.
     - Label: `last scan`.
  2. **Prior scan, this session (new, zero writes).** When the root is rescanned in the same session (`r` on the root, after Done or Stop), the previous *complete* in-memory total for the same root and options is the source. It is kept in `AppState` and never persisted. Label: `last scan`.
  3. **Filesystem inodes:** if root == the mount root and `f_files > 0`, then used inodes = `f_files - f_ffree`. Unavailable on btrfs (fact, §1), but useful on ext4/xfs mount roots. Label: `filesystem`.
  4. **None** → the `Indeterminate` state (below).
- **Consequence for the user's setup:** `/home` is btrfs and history is off by default. So **the first scan of a root has no ETA or percentage**; it shows indeterminate progress. Rescans in the same session, and every scan with `--history`, get a real ETA.
- **Guard:** `est_total = max(source_total, seen + pending_dirs)`. It is never below what is already known.
- **Rate:** an EWMA of entries/s with τ = 5 s. It is shown in every scanning state.
- **Display, when there is a source:**
  - `remaining = (est_total - seen) / rate`.
  - `fraction = seen / est_total`, which drives the UX `~41%` bar. It is clamped to ≤ 0.99 until `Done`, so it reaches 100% only when the scan completes.
- **Indeterminate progress (no source).** Only quantities that are true are shown:
  - elapsed time;
  - entries seen and bytes seen (both grow, and are exact so far);
  - the rate in entries/s;
  - **`≥ N folders queued`**: `pending_dirs` is an exact lower bound on the directories still to read. It rises, peaks, then drains to 0 at Done. That gives a visible "winding down" signal without inventing a total;
  - **top-level progress `K of M top-level folders done`**: M = the number of directory children of the **scan root**, known after the first listing. K = how many of them are `is_complete`. This is a real fraction with a known denominator, but it is not time-proportional: the biggest folder usually finishes last, and shallow-first delays completion. It is therefore shown as a count, never as a bar or `%`;
  - an activity glyph (spinner ≤ 10 fps, static with `--no-animation`). It is **not** a bar, so it can't be read as a percentage;
  - the ETA field reads `ETA unknown (no estimate on first scan)`.
- **Not adopted:**
  - A `seen / (seen + pending_dirs)` percentage. It is an *upper* bound on true progress, so it would always look further along than reality, and it is untested.
  - A pending-dirs × mean-subtree estimator. Under shallow-first scheduling, the subtrees that complete first are the small ones, which biases it low, the same failure mode as the probe. R3b is closed: not pursued (ADR-010).

Displayed states (the pure function `eta::view(EtaInput) -> EtaView`). The status text belongs to UX (`ux/spec.md` §5.2). The source label is appended:

| State | Condition | Example (wording is UX's) |
|---|---|---|
| `Estimating` | **a source exists** but elapsed < 2 s or the rate is not settled | `ETA estimating…` |
| `Eta { remaining, fraction, source }` | a source exists and the rate is settled | `[bar] ~41%  ETA ~01:44 (estimate, last scan)` / `(estimate, filesystem)` |
| `OverEstimate` | seen ≥ source_total × 1.02 | `~99%  ETA unknown (past estimate)` |
| `Indeterminate { rate_eps, pending_dirs, top_done, top_total }` | no source (from frame 0; `Estimating` is never used without a source) | `◌ 01:12 · 1.24M entries · 41k/s · ≥12.4k folders queued · 12/31 top-level done · ETA unknown (no estimate on first scan)` |
| `Done { elapsed }` | `ScanMsg::Done` | `✓ Done in 03:51 …` (ETA 00:00) |
| `Stopped { elapsed }` | cancelled | `■ Stopped at 01:12 · partial results` |

- **Mid-scan source change:** a source can appear mid-scan (for example the history file loads after frame 0). The view then moves `Indeterminate → Estimating → Eta`. That counts as "revised as the scan proceeds".
- **Elapsed** is always shown and updates at ≥ 1 Hz on the 100 ms data tick (M2). Accuracy target: none (brief).
- **M2 gap (flag for project-owner):** on a first scan without history on btrfs, the ETA is honestly "unknown" for the whole scan. The brief requires that it "says so instead of showing a made-up number" and converges to 0/Done at completion, and both still hold. But "revised as the scan proceeds" only holds once a source exists. **Recommendation:** accept this and amend M2 to read "an ETA when a source exists (history, in-session rescan, or inode count), otherwise indeterminate progress". The alternative, history on by default, reverses a user decision.

### 3.7 Treemap (ADR-006)
- **Input (UX map scope rule, F2):** the children of the *selected item's parent*, by the current size mode, with the selected item's tile highlighted. When zoomed, the view root's children are shown if the view root itself is selected.
- **Algorithm:** squarified (Bruls et al.), computed in floating point with **cell-aspect correction**. Terminal cells are about 1:2 (w:h), so the layout runs in a space where height is doubled (`--cell-aspect`, default 2.0).
- **Snapping to cells:**
  - Round each rectangle's **edges**, not its sizes, to integer cells. Adjacent rectangles share rounded edges, which guarantees no gaps and no overlap.
  - Children whose share is below about 1 cell are merged into one `(+N smaller)` aggregate tile. Aggregates can't be deleted (UX §5.3).
  - Contract: each rendered area is within ±1 row/column per edge of exact proportion (M4, "within terminal-cell rounding").
- **Pure interfaces:**
  - `layout::squarify(items: &[(NodeId, u64)], area: Rect, cell_aspect: f32) -> Vec<Tile>`, where `Tile { id: Option<NodeId> /* None = aggregate */, rect: Rect }`.
  - `layout::neighbor(tiles: &[Tile], from: usize, dir: Up|Down|Left|Right) -> Option<usize>`, which picks the tile adjacent in `dir` with the largest overlap on the shared axis, ties nearest by centre. This supports UX map-focus arrow navigation.
  - Both are deterministic. Ties break by `NodeId`.
- **Rendering:** follows UX §6:
  - Category color plus a letter tag.
  - A double border on the selected tile.
  - `!` on tiles with an issue.
  - `NO_COLOR` honored.
  - Labels truncate by display width.
- **Live stability:** while the scan runs, relayout at most every 500 ms. The selection highlight updates every frame. Spike R5 decides whether this is enough.
- **Sync (M4):** one selection (`NodeId`) is shared by tree and map. Moving in either one updates the other.

### 3.8 Delete (ADR-007)
- **Flow (UX F4/§5.3):** `d`/`Delete` → modal dialog → only `y` deletes. `p` in the dialog prioritises the target's scan (§3.11). **`Enter`, `n`, and `Esc` all cancel** (UX decision; cancel is the default). Other keys are ignored.
- **Unfinished targets (rev 3, user feedback): allowed.** The dialog shows lower bounds (`≥`), a "scan not finished" title, and the 3 largest known children. `p` in the dialog prioritises the folder so the numbers become exact. Confirming cancels that subtree's scan. Full contract in §3.11. This supersedes the UX §5.3 "blocked" rule.
- **The dialog shows (M7):**
  - Full path, sanitized and wrapped.
  - Type.
  - Size in the active mode, named.
  - Files and folders counts: exact when `is_exact`, otherwise `≥` (§3.11).
  - The hard-link line when `outside_links > 0` (§3.10).
  - "This cannot be undone. Files are not moved to the trash."
- **Refused targets (UX wording):**
  - The scan root.
  - Aggregate tiles/rows.
  - Any `OTHER_FS` node.
  - A node already `DELETING` or inside one.
  - A second delete while one is running (one at a time in v1).
- **Executor (own thread)** receives `DeleteRequest { node, rel_path (from scan root), expect: (dev, ino), kind, total_entries }`:
  1. Resolve the parent with `openat2(root_fd, parent_rel, O_DIRECTORY|O_NOFOLLOW, RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS|RESOLVE_NO_MAGICLINKS)`. `root_fd` is opened at scan start. This guarantees the target is inside the scanned root and that no symlink was swapped into the path.
  2. `statx(parent_fd, name, AT_SYMLINK_NOFOLLOW)`. If the result does not equal `expect` → **abort** "changed since scan", and nothing is deleted.
  2b. **The target must not be a mount point:** if its (dev, mnt_id) differs from `parent_fd`'s → abort `MountPoint`, and nothing is deleted. This can happen with `--cross-filesystems` or a same-device bind mount (T010 review).
  - **Residual race (accepted):** Linux has no unlink-by-fd, so between `statx` and `unlinkat(name)` an object with the same name could be swapped in. The damage is limited to that one name inside an already-verified directory, and an attacker would need write access to it anyway.
  3. File, symlink, or other → `unlinkat(parent_fd, name, 0)`.
  4. Directory:
     - `openat(parent_fd, name, O_DIRECTORY|O_NOFOLLOW)` and re-verify (dev, ino) with `fstat`.
     - Do an iterative post-order removal using dirfds only.
     - Never descend into an entry whose `st_dev` ≠ the target's `st_dev`, **or whose `stx_mnt_id` differs** when the kernel reports it, so same-device bind mounts are caught (T010, accepted). Leave it, record a failure `OtherFilesystem`, and keep going.
     - Finish with `unlinkat(.., AT_REMOVEDIR)`.
     - Keep at most 64 open dirfds. Deeper levels are handled by closing and reopening via `openat` from the nearest held ancestor with the same no-follow and identity checks.
  5. Emit progress and the outcome (§3.10 delete progress contract).
- **Handle lifecycle (exit must never block):** the main thread is the only receiver of `DeleteEvent`, so it must never block waiting on the executor while it is not draining that channel. Exit order:
  1. set stop;
  2. wait at most 2 s for the executor to go idle, **draining `DeleteEvent` while waiting**;
  3. drop the request sender;
  4. restore the terminal and exit.
  - `Drop` must not join without a bound. If the thread hasn't finished, detach it: process exit ends it, and it has already stopped after the current entry.
  - The executor sends `Finished` with a bounded wait (`send_timeout`, or a `try_send` loop that checks stop). On exit it may drop `Finished`, because the stderr "interrupted: N of M" line comes from the last `Progress` or the shared counters, not from `Finished`.
  - The same rule applies to the scan workers on the bounded `ScanMsg` channel (plan T006 drain on `cancel_all`).
- **Cancellation:** the executor checks a stop flag between entries. It is set only on quit (see below), never by a dialog.
- **Quit (user decision, rev 3; supersedes UX F5 "no prompt"):** `q` (and `Ctrl-C`, which arrives as a key event in raw mode) **always** opens a quit confirmation. Nothing quits on the first press.
  - **Idle or scanning:** `Quit <cmd>? Scan results will be lost.` A second `q`, or `y`, quits. `n`, `Esc` and `Enter` cancel.
  - **During a delete:** `A delete is in progress: <path>, 12,400 of 48,212 files removed. Quitting stops it after the current file and leaves the folder partly deleted.` Only `y` quits here. **Any other key**, including a second `q`, returns to the UI, and the delete continues (brief M12). On `y`: set the executor stop flag, wait at most 2 s for the current entry, restore the terminal, and print `<cmd>: delete of <path> interrupted: N of M removed` to stderr.
  - **Signals:** `SIGTERM`/`SIGHUP` can't prompt. They stop the executor after the current entry, restore the terminal, and print the same message.
- **Reconciliation (model):**
  - Mark the target `TOMBSTONE` when the delete starts.
  - On `Removed`: remove the subtree, subtract its aggregates from ancestors, update the ext and top-N indexes, and re-charge hard links whose counted occurrence was deleted to a surviving `others` entry.
  - On `Partial` **or `Aborted`** (plan D1): clear `TOMBSTONE`/`DELETING`, remove the cancelled prefix, and run a subtree rescan of the target. It shows `◌` until done.
    - The rescan re-`statx`es the target itself and refreshes its `ino`/`dev`/`kind` (needed after `ChangedSinceScan`).
    - If the target is gone (`NotFound`), its node is removed.
  - **Rescan clears inner skips** (plan D2): `begin_rescan(X)` removes every cancelled prefix at or under X. This applies to `r` and to the automatic rescans above.
  - **Status during a rescan after `Done`** (plan D3): the global status stays `✓ Done …` and elapsed is not reset. Affected rows show `◌`. No ETA is shown for subtree rescans.
  - Results are shown in the UX result dialog, which must be dismissed explicitly, and failures are added to the issue list. They are never silent.
- **Invariant (tested):** after any delete, the model totals equal `du` on the remaining tree.

### 3.9 UI
`docs/ux/directions.md` owns layout, keys, states and copy (§5–§6). **Chosen: layout A** (user, 2026-10-23). It needs no backend change.
- **Treemap strip:** the treemap is a wide, short strip, about 78×6 cells at 80×24. Squarified with cell-aspect correction still applies; expect fewer labeled tiles and more `(+N smaller)` aggregation at the 80×24 floor.
- **Map focus:** `layout::neighbor` stays, for UX F2 `Tab` map focus.
- **Tree markers:** the tree's `▲` (prioritised) and `⊘` (skipped) markers come from `ScanControl.focus` and `IssueKind::SkippedByUser` (§3.11). The binding UI constraints on the backend are in §3.10.

### 3.10 Backend contract for UX
| UX need | Contract | Cost |
|---|---|---|
| **Per-folder complete flag** (`◌`, `≥` in dialog; see `is_exact` in §3.11) | `Model::is_complete(id) -> bool` = node `DONE` **and** `pending_dirs == 0`. The rules: when a dir is discovered, it and every ancestor get `pending_dirs += 1`; when its listing (or its open/read error) is applied, it and every ancestor get `-= 1`. An unreadable dir counts as complete with `issue` set (its size is "known so far"). After a stop, unread dirs stay incomplete. Files are always complete. | O(depth) per dir. Same pass as size propagation |
| **First screen < 1 s** | Frame 0 is rendered before any worker starts (§3.3). Root row with `◌`, map `Waiting for first results…` | none |
| **Redraw cap** | Data redraws ≤ 10 Hz, input redraws immediate and coalesced, relayout ≤ 2 Hz, `--no-animation` (§3.3) | none |
| **Hard-link count for the delete note** | Computed when the dialog opens, over the (complete) target subtree: `outside_links` = number of files with `nlink > 1` whose occurrences inside the subtree (from the hard-link table) are fewer than `nlink`. Deleting these files won't free their space. The same pass gives `freed_bytes_upper` (disk size minus those files). Budget: ≤ 50 ms per 1 M subtree nodes on the main thread; R2 measures it. Answer to UX's question: **yes, cheap**. `nlink` is already in `statx` | O(subtree) once per dialog |
| **Delete progress** | `DeleteEvent::Progress { removed_files, removed_dirs, removed_bytes }` at most every 100 ms. The total (`total_files`) comes from the request when the target `is_exact`. This gives `Deleting … 12,400 / 48,212 files`. Otherwise it is `None` and the UI shows `12,400 files removed` | none |
| **Delete result** | `Finished(Outcome)`. `Outcome = Removed { files, dirs, bytes } \| Partial { removed: Counts, failures: Vec<DeleteFailure { rel_path, kind: DeleteFailKind }> } \| Aborted { reason: ChangedSinceScan \| NotFound \| OpenFailed(errno) }`. `DeleteFailKind = PermissionDenied (EACCES/EPERM) \| Busy (EBUSY/ETXTBSY) \| ReadOnlyFs (EROFS) \| OtherFilesystem \| Other(errno)`. ENOENT is not a failure; it is also not counted in `removed` (T010) (§4 is the contract, plan D8). UX groups failures by kind and `v` lists their paths | none |
| **Skip reasons** | `IssueKind = PermissionDenied \| Vanished \| OtherFilesystem \| Other(errno)`, with `op` and `node` (§3.4). Live list, plus node `issue` for `! name (reason)` rows | none |
| **Progress `~%`** | `EtaView::Eta.fraction` (§3.6) | none |
| **Selection follows item** | Stable `NodeId` (§3.4) | none |
| **Largest → show in tree** | Top-N entries carry `NodeId`. The tree expands ancestors via `parent` links | none |
| **Types categories** | `category_of(ext)` static table (§3.4) | none |

### 3.11 Acting before the scan finishes (ADR-009)
**User need (rev 3):** a full-depth scan of a large root can take a long time. The user often recognizes a folder by name (for example `.cache/yay`) and wants to delete it right away, without waiting for the whole scan or even that subtree.

**What is already true:** the root's own listing is the first job. So top-level names appear within the first frames whatever the scan order, and their sizes then grow live. The question is what to do about **sizes and counts that aren't final yet**, and **how fast a folder of interest becomes final**.

**Prior art:**
- WizTree is near-instant on NTFS because it reads the Master File Table directly instead of walking folders, and it generally needs admin rights ([diskanalyzer.com/about](https://diskanalyzer.com/about)).
- The btrfs equivalent, `BTRFS_IOC_TREE_SEARCH`, requires `CAP_SYS_ADMIN` (kernel `fs/btrfs/ioctl.c`; [btrfs-uapi docs](https://docs.rs/btrfs-uapi/0.13.0/btrfs_uapi/)). That conflicts with the brief's "no root required" assumption, so a full walk is unavoidable and scheduling is the lever we have.
- ncdu scans depth-first, single-threaded. `du --max-depth` limits *output*, not counting.

**Options:**

| # | Option | What the user gets | Cost / risk | Verdict |
|---|---|---|---|---|
| O1 | **Shallow-first scheduling.** The job queue is a priority queue keyed by depth (shallow first), instead of approximately-DFS | All levels near the top fill in early and grow evenly. Relative sizes become meaningful sooner. No single subtree is shown as huge while its siblings still read 0 | Pending-job frontier is larger: one `PathBuf` per discovered-unread dir, roughly tens of MB at 500 k dirs (**assumption**, R1). **No subtree completes until late**, so `◌` stays everywhere longer | **Adopt**, with a frontier cap: above 1 M pending jobs, new jobs fall back to deepest-first to bound memory |
| O2 | **Prioritise a subtree.** The user presses `p`, or zooms with `⏎`, and that subtree's queued jobs jump ahead | The folder of interest becomes final in about the time its own size needs. Then the delete dialog shows exact counts | Re-heap of pending jobs on focus change, O(pending), rare. One focus at a time; a new focus replaces the old | **Adopt** (user confirmed both triggers: `p` and zoom). This is the main lever for "I want this one now" |
| O3 | **Delete an unfinished folder.** The confirm shows "at least N files · at least X GiB" plus a clear warning, and the subtree's scan is cancelled | Immediate action. Exactly what the user described | The user consents to lower bounds. M7 says the dialog shows size and counts, and lower bounds that are clearly labeled meet the *letter* of M7, but **project-owner should update the brief to say so**. The hard-link note can't be complete | **Adopt**, with mitigations (below) |
| O4 | **Skip / stop a subtree.** `x` stops scanning that subtree; it stays `◌`, shown as "skipped by you" in Skipped | Avoids wasting time on known-huge, uninteresting trees (e.g. `~/.local/share/Steam`) | Sizes above it are lower bounds, so the UI must show the `◌` up the chain (already does) | **Adopt.** Uses the same cancel mechanism as O3 |
| O5 | **`--max-depth N` scan limit** (rejected, user confirmed) | Faster first result | **Sizes become wrong**: anything deeper isn't counted, which breaks M8 and the tool's purpose. The tree is already expand-on-demand, so a display depth limit adds nothing | **Reject** |
| O6 | **Per-subtree size estimate** (run a Knuth probe per top-level child) | Rough size hints before scanning | Probe estimates entries, not bytes. High variance. Easily misleading next to real numbers | **Defer.** Revisit if O1–O4 aren't enough |

**Recommendation: O1 + O2 + O3 + O4.** O2 lets the user get exact numbers fast. O3 lets them act immediately when they're sure. O4 lets them drop noise.

**O3 mitigations (part of the contract):**
- **Title:** `Delete permanently? (scan of this folder not finished)`.
- **Bounds:** sizes and counts are prefixed `≥` and the line says `at least`.
- **Largest known items:** the dialog lists the 3 largest known children with sizes, so the user can see what is inside, not just a name.
- **Hard-link note:** if the subtree is incomplete, the dialog says `Hard links: not fully known (scan not finished)`.
- **Finish scanning first:** a key `p` in the dialog prioritises this folder (O2). The dialog numbers update live, and the `≥` drops when it completes. The user can press `y` at any time.
- **Confirmation is unchanged:** `y` only; Enter, n and Esc cancel.
- **Safety is unaffected.** The executor never relied on scan data except the (dev, ino) identity check, which is available for any listed node. The scan-root, other-filesystem and symlink rules still apply.

**Mechanics:**
- **`ScanControl` (shared with workers):**
  - `cancel_all: AtomicBool`
  - `cancelled_prefixes: RwLock<Vec<PathBuf>>`
  - `focus: RwLock<Option<PathBuf>>`
  - `generation: AtomicU64`, so workers skip the locks when nothing changed
- **Workers check before each job:**
  - **Cancelled job:** if the job path is under a cancelled prefix, drop it and send `ScanMsg::Cancelled { dir }`.
  - **Focus change:** on a focus change (generation bump), the queue re-keys the pending jobs to `(in_focus desc, depth asc)`.
- **Model on `Cancelled { dir }`:**
  - **For O4 skip:** keep the node with flag `INCOMPLETE` and issue `SkippedByUser`. Decrement `pending_dirs` up the chain, so the parent can complete *as a lower bound*: no `◌`, size `≥`.
  - **`r` resume (brief M11):**
    1. Remove the prefix from `cancelled_prefixes` (bumping `generation`).
    2. Clear the `SkippedByUser` issue and the `inexact_dirs` contribution up the chain.
    3. Run a subtree rescan (§3.5). The folder shows `◌` until complete, then `≥` clears.
  - **For O3 delete:** the subtree is `TOMBSTONE`. It is removed on `Finished`.
- **`is_complete` vs `is_exact`:** the complete flag now has two meanings:
  - `is_complete` means no scan work remains.
  - `is_exact` means complete *and* no `SkippedByUser`/stop-incomplete inside, tracked by a per-dir `inexact_dirs` counter maintained like `pending_dirs`.
  - **Display mapping (owned by `ux/spec.md` §5.1; no backend change):**
    - `◌` ⇔ `!is_complete` (scan work pending). `▲` replaces it on the priority folder.
    - `≥` size prefix ⇔ `!is_exact`, i.e. something inside was skipped (`⊘`) or left unread by a stop.
    - A folder that is still scanning shows `◌` without `≥`; its growing size is implied by `◌`.
    - Skipped folders show `⊘` + `≥` (brief M11).
    - The delete dialog uses `≥` whenever `!is_exact || !is_complete`, because both mean the counts are lower bounds (brief M7).
- **Dialog `p`, "finish scan first" (supported):** `p` sets `ScanControl.focus` to the target (same path as O2), and the dialog stays open.
  - Each frame the dialog re-reads `Model::delete_check(id)`. Sizes and counts update at the normal ≤ 10 Hz data redraw.
  - The O(subtree) hard-link count is recomputed at ≤ 2 Hz while the target is incomplete, and once more when it becomes exact.
  - When `is_complete && is_exact`, the `≥` and the "not finished" title drop.
  - `y` snapshots `DeleteCheck` at that instant into the `DeleteRequest`. The exact totals are passed only if the snapshot is exact.
- **Priority folder done:** the model reports the transition `focus` node `!is_complete → is_complete`, which the UI uses for the `✓ … fully scanned` notice. Then `focus` is cleared. Pressing `p` on the current focus clears it.
- **Delete progress for an unfinished target:** `total_files` is `None`, so the status shows `Deleting … 12,400 files removed` with no denominator.
- **ETA:**
  - **Cancelled subtrees:** cancelling removes unknown remaining work. The estimate guard keeps the total ≥ seen + pending, so the ETA simply finishes early. Done still forces 00:00.
  - **Focus:** focus does not change total work, only the order.

## 4. Conventions

**Naming**
- Crate/binary name: **TBD** (`<cmd>` placeholder; see top note). Only `Cargo.toml` and `APP_ID` change on rename.
- Modules as in §3.2.
- Types are `UpperCamel` and are nouns: `ScanMsg`, `DeleteEvent`.
- Units in names: `*_bytes`, `*_ms`.

**CLI**
- `<cmd> [PATH=.] [-x/--one-file-system | --cross-filesystems] [--apparent] [--threads N] [--top N] [--history] [--no-animation] [--cell-aspect F]`.
- Defaults: one filesystem **on** and history **off**.
- Parse with a standard arg parser (engineers' choice).

**Channels and messages**

These are the shared contracts. Define them first.
```rust
enum ScanMsg {
    Listing { dir: NodeId, first_id: NodeId, entries: Vec<RawEntry> },
    DirIssue { dir: NodeId, op: Op, kind: IssueKind },   // counts as "listed" for pending_dirs
    Cancelled { dir: NodeId },                           // job dropped by ScanControl (skip or delete)
    Identity { node: NodeId, entry: RawEntry },          // plan D11: rescan root re-statx'd (NOFOLLOW) before its Listing.
                                                         // Carries kind + sizes: if the target is no longer a dir
                                                         // (ENOTDIR/ELOOP on open), the walker sends Identity only (no
                                                         // Listing), and the model turns the node into a leaf. ENOENT →
                                                         // DirIssue(Vanished) → node removed when it is the rescan root.
    Done { cancelled: bool },
}
enum IssueKind { PermissionDenied, Vanished, OtherFilesystem, SkippedByUser, Other(i32 /*errno*/) }
struct ScanControl { cancel_all: AtomicBool, cancelled_prefixes: RwLock<Vec<PathBuf>>,
                     focus: RwLock<Option<PathBuf>>, generation: AtomicU64 }
struct RawEntry { name: Box<[u8]>, kind: Kind, disk_bytes: u64, apparent_bytes: u64,
                  ino: u64, dev: u64, nlink: u32, issue: Option<(Op, IssueKind)> }
struct DeleteRequest { node: NodeId, rel_path: PathBuf, expect_dev: u64, expect_ino: u64, kind: Kind,
                       total_files: Option<u64>, total_dirs: Option<u64> }   // None when !is_exact
struct Counts { files: u64, dirs: u64, bytes: u64 }
enum DeleteEvent { Progress { removed_files: u64, removed_dirs: u64, removed_bytes: u64 }, Finished(Outcome) }
enum Outcome { Removed(Counts), Partial { removed: Counts, failures: Vec<DeleteFailure> },
               Aborted { reason: AbortReason } }
struct DeleteFailure { rel_path: PathBuf, kind: DeleteFailKind }
enum DeleteFailKind { PermissionDenied, Busy, ReadOnlyFs, OtherFilesystem, Other(i32) }  // ENOENT = success
enum AbortReason { ChangedSinceScan, NotFound, MountPoint, OpenFailed(i32) }
struct DeleteCheck { exact: bool, largest_known: [Option<NodeId>; 3], files: u64, dirs: u64, size_bytes: u64,
                     outside_links: u64, freed_bytes_upper: u64 }   // Model::delete_check(id)
struct EtaInput { elapsed: Duration, seen: u64, pending_dirs: u64, rate_eps: f64,
                  source: Option<(u64 /*total*/, EtaSource)>, state: ScanState }
enum EtaSource { History, SessionPrevious, FsInodes }   // no Probe (ADR-010)
enum EtaView { Estimating, Eta { remaining: Duration, fraction: f32, source: EtaSource }, OverEstimate,
               Indeterminate { rate_eps: Option<f64>, pending_dirs: u64, top_done: u32, top_total: u32 },
               Done { elapsed: Duration }, Stopped { elapsed: Duration } }
```
(Field sets are the contract. Exact Rust spelling can change if the meaning stays the same.)

**Error handling**
- Core modules (`scan`, `delete`, `model`, `history`) return typed error enums and preserve `errno`.
- No `unwrap`/`expect` on I/O paths.
- A panic is a bug. Install a panic hook that **restores the terminal** before printing, **but only for panics on the main thread**. The hook runs before `catch_unwind` and for every thread, so restoring the terminal on a worker panic would leave the still-running app in cooked mode on the main screen.
  - **Non-main threads:** the hook stores the message in a shared slot and does not print or touch the terminal.
  - **Caught panics** (the delete executor, T010 F2) surface through their `Finished(Partial)` outcome, plus the stored message in the result dialog.
  - **Uncaught worker panics** (scan): the main loop detects them through the channel disconnecting or `JoinHandle::is_finished`. It shows an error dialog and treats the scan as `Stopped`. The user can still quit normally (T015 owns the hook).
- User-facing text is produced only in `ui`.

**State**
- Only the main thread mutates `AppState`/model.
- Other threads talk only through the message types above (`ScanMsg`, `DeleteEvent`) plus the shared `ScanControl`. Supporting types (`ScanState`, `SizeMode`, `Refusal`, `Reconcile`, `ScanHandle`, `DeleteHandle`, `ScanTotals`, `FsAccess`) are defined in plan T001 (D7) and follow §3.3–§3.11.
- The UI renders only from `&Model`.

**Security**
- **Nothing writes** outside `delete` (user data) and `history` (own state file, only with `--history`). By default the tool makes zero writes unless the user confirms a delete. CI enforces this with a grep test that forbids `remove_*`, `unlinkat`, `File::create`, `OpenOptions` with write, and `rename` outside those modules (M9).
- Never follow symlinks, either when scanning or when deleting.
- **Sanitize file names before rendering.** Replace C0/C1 control characters, ESC and DEL with `�` or `\xNN`. Decode invalid UTF-8 lossily for display only. Filesystem operations always use the raw bytes. This prevents terminal escape injection through crafted names.
- Display width is computed with a Unicode width function, so CJK and emoji don't break columns.
- No network, no telemetry, no privilege escalation.

**Testing**
- Unit and property tests:
  - `layout`: no overlap, full coverage, proportionality within ±1 cell per edge, determinism.
  - `eta`: synthetic progress feeds for every state transition, converging to `Done`.
  - `model`: apply and reconcile, hard-link re-charge.
  - `is_complete` across listing, error, stop and rescan.
  - `delete_check.outside_links` with links inside and outside the subtree.
  - `ext_of`, name sanitizing.
- Integration tests (M8): build fixture trees in a temp dir with:
  - hard links inside and across subdirs;
  - symlinks: dangling, loop, and pointing outside;
  - sparse files;
  - `chmod 000` dirs;
  - invalid-UTF-8 and newline names;
  - 200-deep nesting.

  Compare totals to `du -s -B1`, `du -s -B1 --apparent-size`, and `du -sx -B1`. Run on tmpfs and on the user's btrfs.
- Robustness (M10): use the fault-injection seam for `EACCES` and `ENOENT` mid-listing, plus a racing deleter thread. The scan completes and the issues are listed.
- Delete:
  - Decline → the tree hash is unchanged.
  - Confirm → removed, and totals equal du.
  - Swap target for a symlink after the scan → `Aborted`, and the symlink target is untouched.
  - Symlink inside the subtree pointing outside → the outside is untouched.
  - Mount inside the target → not descended. Use `unshare -rm` + tmpfs where user namespaces are available, otherwise skip with a message.
- History: the default run creates no file under `$XDG_STATE_HOME` (assert with a temp `XDG_STATE_HOME`).
- Early delete: an unfinished target shows `≥` bounds. Confirming cancels its scan jobs. The final totals equal du after the scan completes.
- Skip and prioritise: a skipped subtree never gets listed after `Cancelled`. A focused subtree completes before the unfocused siblings in a fixture with a deterministic single-thread queue.
- Quit: the first `q` never exits. During a delete, `q q` does not exit and `y` does. Interrupting reports N of M.
- UI: ratatui `TestBackend` snapshot tests for each view and dialog, at 80×24 (the UX floor). App-level tests feed key events and `ScanMsg` streams with no terminal.
- M1: a headless test floods the channel and asserts that a key event is handled within 50 ms.

## 5. Non-functional budget

| Item | Budget | Basis |
|---|---|---|
| First interactive frame | ≤ 200 ms target, 1 s hard (M1). Frame 0 is drawn before workers start | brief M1, UX F1 |
| Input → rendered response during scan | ≤ 50 ms p95 | M1. The 8 ms apply budget + render |
| Data-driven redraws | ≤ 10 Hz; elapsed ≥ 1 Hz; input redraws immediate | M2, UX §6 motion |
| Treemap relayout while scanning | ≤ 2 Hz | ADR-006 |
| Delete dialog hard-link check | ≤ 50 ms per 1 M subtree nodes | §3.10, R2 |
| Memory | ≤ 120 B/entry average incl. names (**assumption**). That is about 600 MB at 5 M entries, against 60 GiB RAM | ncdu-style compact nodes. Spike R1 |
| Threads | `min(nproc, 8)` walkers + 1 delete + main | gdu/dust precedent. Tune later |
| Throughput | No target in v1 | brief |
| Cost | Local only, $0 | — |

## 6. Traceability

| Req | Design element |
|---|---|
| G1 / M1 Async live scan | §3.3 single-owner loop, input-first, bounded channel; §3.4 incremental aggregation; ADR-003 |
| M2 Elapsed + ETA | §3.6, ADR-005/010 (**partial on first scan without a source; see the §3.6 M2 gap**); `Done`/`Stopped` states; 100 ms tick |
| M3 Tree, root top, largest first, % of parent | §3.4 lazy sort `(size desc, name asc)`, stable NodeId; UX F2 keys |
| M4 Treemap proportional, labels, sync | §3.7, ADR-006 |
| M5 File-type breakdown, whole root | §3.4 ext index, view `2` |
| M6 Largest files, whole root, live, N configurable | §3.4 top-N heap, `--top`, view `3` |
| M7 Delete with confirmation, errors shown | §3.8, ADR-007 |
| M8 Accuracy vs du, hard links once, no symlink follow | §3.4 hard-link table, dir apparent = 0, §3.5 `AT_SYMLINK_NOFOLLOW`, §4 du comparison tests |
| M9 Safe by default | §4 write-module rule + CI grep; `O_NOATIME`; ADR-007 |
| M10 Robustness, skipped reported | §3.5 error mapping, §3.10 `IssueKind` reasons, view `4`, fault-injection seam |
| UX backend needs (complete flag, first paint, redraw cap, hard links, delete progress, skip reasons) | §3.10 |
| Should: one filesystem | §3.5, ADR-008 |
| Should: disk vs apparent toggle | both sizes stored per node; `a` key |
| Should: stop/cancel | §3.3 cancel flag, `Stopped` state; per-subtree skip `x` (§3.11) |
| M7 (v0.5) delete of unfinished folders with `≥` | §3.8, §3.11 O3, ADR-007/009 |
| M11 scan controls `p`/zoom, `x`/`r`, shallow-first | §3.5 queue, §3.11 O1/O2/O4 + `r` resume, ADR-009 |
| M12 quit confirmation | §3.8 Quit |
| Could: rescan subtree | §3.5 subtree rescan (needed anyway for delete reconciliation) |
| Could: export report | **Gap. Not designed in v1** (the brief lists non-interactive output as a non-goal; easy later from the model) |
| Constraint: Linux, no root, no network | ADR-001; §4 security |
| Accessibility | selection not color-only, `NO_COLOR`, Unicode width, `?` help |

**Gaps / flags for project-owner:**
1. ~~History file~~: decided, opt-in (`--history`).
2. ~~One filesystem default~~: decided, on.
3. Export: deferred (Could). Not designed.
4. ~~Delete of unfinished folders~~: allowed with `≥` bounds (§3.11). **Owner action:** update brief M7 to accept clearly labeled lower bounds for unfinished folders.
5. ~~Quit~~: always confirm; during a delete only `y` confirms (§3.8). **UX action:** update F5 and the key table.
6. Command name: TBD, placeholder `<cmd>`.

## 7. Risks and unknowns

| # | Risk | Cheapest spike that retires it |
|---|---|---|
| R1 | Memory per entry exceeds budget at real scale. **Note:** R3 measured 19.46 M entries in the user's `$HOME`, so about 2.3 GB at 120 B/entry | Fill the arena with **20 M** synthetic nodes (realistic name lengths), measure RSS. About half a day |
| R2 | Main-thread apply can't keep up (backlog, slow scan) or the top-N recompute is too slow | Walker + apply headless on `/usr` and `$HOME`. Log produce vs apply rate and per-tick apply time |
| R3 | ~~Knuth probe ETA too noisy~~ **Retired: failed** (`spikes/r3-probe/REPORT.md`); probe removed, ADR-010 | — |
| R3b | ~~Pending-dirs-based estimator~~ **Closed, not pursued.** Variant B also failed (K=256 medians 0.07 / 0.35 / 0.63). `seen + pending_dirs` is only a lower bound (0.12 / 0.51 / 0.90 of truth on $HOME). See ADR-010 | — |
| R4 | btrfs specifics: subvolume `st_dev`, `-x` semantics, `du` parity on compressed files | Run `du -sx` vs `du -s` on `$HOME`. Compare to walker totals |
| R5 | Treemap jitter during live scan is distracting | Record a scan's `ScanMsg` stream, replay it into the treemap at 2 Hz, eyeball it. Fallback: ordered treemap |
| R6 | Delete edge cases (hard links, mounts, deep trees) | Covered by §4 tests. Check that `unshare -rm` works on CachyOS for the mount test |
| R8 | Shallow-first frontier memory, or the re-heap cost on focus change, at real scale | Part of the R1/R2 harness: log the max pending jobs and re-key time on `$HOME` |
| R7 | Terminal compatibility (box-drawing, colors, 256-color contrast per UX open question) in the user's terminal | One render of each view in the user's terminal, in truecolor, 256-color and `NO_COLOR` |

## 8. Implementation guidance for engineering-lead

**Define first (shared contracts, one PR):**
- `NodeId`, `Kind`, `Op`, `IssueKind`
- `ScanMsg`, `RawEntry`
- `DeleteRequest`/`DeleteEvent`/`Outcome`
- `EtaInput`/`EtaView`/`EtaSource`
- `Tile` + the `layout::squarify` / `layout::neighbor` signatures
- `DeleteCheck`, `Model::is_complete`, `Model::is_exact`, `Model::delete_check` signatures (§3.10)
- `ScanControl` (§3.11)
- the walker fault-injection trait

Also add the CI write-guard grep.

**Suggested order:**
1. `scan` + `model` apply + a headless harness + **M8 du-parity tests**. This is the foundation and retires R1, R2 and R4.
2. `app` loop + tree view + status bar (elapsed). This is the first usable build and proves M1.
3. `eta` + opt-in `history` + in-session previous scan + fs-inodes source.
4. `layout` + treemap widget.
5. File types + largest files views.
6. `delete` executor + dialog + reconciliation. Its tests come first.
7. Should items: `-x` flag plumbing, the `a` toggle, `s` stop. Then §3.11: `p` prioritise, `x` skip, early delete. Then the Could item: `r` rescan.

**Parallel seam for two engineers:**
- Engineer A: steps 1, 6 (`scan`, `model`, `delete`). These are filesystem-heavy.
- Engineer B: steps 2–5 (`app`, `ui`, `eta`, `layout`) against the contracts, using synthetic `ScanMsg` streams until A lands.

**Watch:** `delete` is the only place where a bug destroys user data. Require a second review on it and on any change to the write-guard allowlist.

## Sources
- [WinDirStat background](https://windirstat.net/background.html) · [SequoiaView](https://www.win.tue.nl/sequoiaview/Main_Frame/Info.html)
- [Bruls, Huizing, van Wijk, Squarified Treemaps (2000)](https://diglib.eg.org/handle/10.2312/VisSym.VisSym00.033-042)
- [Bederson, Shneiderman, Wattenberg, Ordered and Quantum Treemaps](https://umiacs.umd.edu/node/17312)
- [Knuth 1975, Estimating the efficiency of backtrack programs](https://www.ams.org/journals/mcom/1975-29-129/S0025-5718-1975-0373371-6/home.html)
- [Ncdu 2 design notes](https://dev.yorhel.nl/doc/ncdu2) · [gdu man page](https://man.archlinux.org/man/extra/gdu/gdu.1.en) · [dua-cli](https://docs.rs/crate/dua-cli/2.9.1) · [dust](https://github.com/bootandy/dust) · [diskonaut](https://github.com/imsnif/diskonaut)
- [jwalk on docs.rs](https://docs.rs/crate/jwalk/latest) · [rustix](https://docs.rs/crate/rustix/latest) · [ratatui](https://docs.rs/crate/ratatui/latest) · [crossterm](https://docs.rs/crate/crossterm/latest)
- [Rust CVE-2022-21658 advisory](https://groups.google.com/g/rustlang-security-announcements/c/R1fZFDhnJVQ)
