# UX spec: `<cmd>` (disk-usage TUI), layout A

Status: final draft for build, 2026-10-23. Author: Freya (UX).
Inputs: `docs/brief.md` v0.5, `docs/design.md` rev 3 (§3.6–§3.11, ADR-007, ADR-009), `docs/ux/directions.md` (direction record; A chosen, B/C archived).

This file owns layout, keys, states and copy. If anything here conflicts with `directions.md`, this file wins.

- **Name placeholder:** the command is `<cmd>`, matching design.md. UI code takes the name from the binary name, never from a string literal in the UI.
- **Labels:** **Fact** (from the brief, design or a cited source), **Assumption**, **Open question**.

---

## 1. Users and tasks
- **Who:** one technical Linux user (CachyOS, fish), working in a terminal (brief: Users).
- **Tasks:**
  - T1: start at a root and see where space goes without waiting (G1, M1, M2).
  - T2: explore the space in a tree plus a treemap (G2, M3, M4).
  - T3: find big things across the whole root, by type and as single files (M5, M6).
  - T4: delete with confirmation, including folders that are still being scanned (G3, M7).
  - T5: steer the scan with prioritise, skip and resume (M11).
  - T6: quit safely (M12).

## 2. Design language
**Layout A, Classic split.** The tree runs full width on top and the treemap is a strip below. The visual grammar is ncdu's: a size column, a percent-of-parent bar, and `d` to delete. The panel arrangement follows WinDirStat. The user chose A (brief Decisions v0.5). The comparison and the archived alternatives are in `directions.md` §1–§4. Stack: Rust and ratatui (ADR-001). The spec uses only plain cells, borders, bold, reverse video and up to 24-bit color.

## 3. Screen anatomy (80×24 minimum)

| Rows (at 24) | Region | Content |
|---|---|---|
| 1 | Header | `<cmd>`, view root path, size mode (`Disk usage` / `Apparent size`), `? help  q quit` |
| 2 | View tabs | `[1 Tree]  2 Types  3 Largest  4 Skipped !N`. The active tab is in brackets and bold. |
| 3 | Column header | `SIZE  %  OF PARENT  NAME` |
| 4 … ~60% of body | Main view | The Tree, or the active list view |
| next 1 | Map title rule | `Map: <path>`. During a scan it also shows `(updating)`; after a stop, `(partial)`. The hidden-items notice is right-aligned on this rule (§3.3). |
| ~40% of body | Treemap | Squarified tiles (ADR-006) |
| −2 | Status line | Scan state, progress, elapsed, ETA, transient messages |
| −1 | Key hints | Hints change with focus and context (§6) |

- **Body split:** the main view gets 60% of the rows between the column header and the status line, and the map gets 40%, rounded in the map's favour. The map is never fewer than 6 rows.
- **Wide terminals:** the ≥120-column Types side panel is **deferred** (plan). v1 uses the same layout at every width ≥80×24, and Types is reached with `2`.
- **Too small:** below 80×24 the screen shows only the message in §8. Every key except `q` and `Ctrl-C` is ignored.

### 3.1 Main screen (scanning; NO_COLOR rendering)
```
 <cmd>  /home/nss                              Disk usage   ? help   q quit
 [1 Tree]  2 Types  3 Largest  4 Skipped !3
    SIZE      %  OF PARENT      NAME
  182.4G  100.0  ██████████  ◌  /home/nss
> 64.2G    35.2  ███▌        ◌  ├─▾ .cache/
   31.0G   48.3  ████▊          │  ├── yay/
   14.2G   22.1  ██▏            │  ├── mozilla/
    8.8G   13.7  █▎          ◌  │  ├── pip/
   10.2G   15.9  █▌             │  └── (12 smaller, 10.2G)
   41.8G   22.9  ██▎         ▲  ├─▸ Videos/
  ≥28.1G   15.4  █▌          ⊘  ├─▸ .local/  (scan skipped)
   19.6G   10.7  █              ├─▸ Projects/
   12.3G    6.7  ▋           !  ├─▸ Downloads/  (permission denied)   ↓ 4
 Map: /home/nss (updating) ────────────────────────────────────────────────────
╔ .cache/ 64.2G 35% [A] ══╗┌ Videos/ 41.8G 23% [V]┐┌ .local/ ≥28.1G 15%[B] ─┐
║                         ║│                      ││                        │
║                         ║│                      │└────────────────────────┘
║                         ║│                      │┌ Projects/ 19.6G ┐┌ Dow…┐
║                         ║│                      ││ 11% [C]         ││ 7% !│
╚═════════════════════════╝└──────────────────────┘└─────────────────┘└─────┘
 ◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown
 ↑↓ move  ←→ fold  ⏎ zoom  ⌫ up  Tab map  d delete  p first  x skip  ? help
```

### 3.2 Row anatomy (Tree)
`[sel] SIZE  %  BAR  STATE  INDENT+TWISTY NAME  SUFFIX`

| Field | Rule |
|---|---|
| `sel` | `>` on the selected row, which is also drawn in reverse video |
| SIZE | Size in the active mode, IEC units with 1 decimal (`64.2G`, `812K`, `0B`), right-aligned in 7 cells. Prefixed `≥` when the size is a lower bound (§5.1). |
| % | Percent of parent, 1 decimal |
| BAR | 10 cells, eighth-cell steps (`█▉▊▋▌▍▎▏`); the ASCII fallback is `#` |
| STATE | One glyph: `◌` scanning, `▲` prioritised, `⊘` skipped by you, `!` has an issue, or blank. The precedence (§5.1) applies when several are true. |
| NAME | Directories end in `/`. Names are sanitized (design §4 Security) and truncated with `…` by display width. |
| SUFFIX | `(scan skipped)`, `(permission denied)`, `(other filesystem)`, `(deleting…)` |
| Aggregate | `(N smaller, SIZE)`, which can't be deleted or zoomed. An expanded folder shows its 50 largest children, and the rest go into one aggregate row (plan D5; the threshold is a constant). `→` on the aggregate reveals the next 50, and that stays revealed for this folder until it is collapsed. **The selected item is never hidden in an aggregate:** if a live re-sort pushes it below the cut-off, it stays as its own row just above the aggregate until the selection moves away. |

### 3.3 Treemap tile anatomy
- **Border:** the selected tile gets a double border (`╔═╗`); others get a single border.
- **Label:** first line is `name size pct% [T]`, and the second line holds whatever didn't fit. Labels truncate with `…`. Tiles narrower than 6 cells get no label, but a `!` or `⊘` marker still shows if one applies.
- **Category tag and fill:** `[V]` video, `[I]` image, `[A]` archive, `[C]` code/text, `[B]` binary, `[D]` documents, `[O]` other/audio. Fill colors are listed in §7.
- **Aggregate tile:** `(+N smaller)`, grey.
- **Hidden-items notice (lead ruling, T017).** Tiles that round to zero cells are dropped.
  - **When it shows:** at least one item with size > 0 is in map scope but has no tile, **and** no `(+N smaller)` tile is drawn. If an aggregate tile is drawn, its N includes all hidden items and there is no notice.
  - **Not counted:** zero-byte items are not counted as hidden. The "all 0 B" case keeps its own message (§5.5).
  - **Placement:** right-aligned on the map title rule, inside the rule, with one `─` before and after. It never takes rows from the map, so it works at 80×24, where the map has its minimum of 6 rows.
  - **Wording, longest first:**
    1. `14 items too small to show (3.1 MiB)`
    2. `14 too small to show`
    3. `+14 hidden`
  - **Selected item hidden:** if the selected item is one of the hidden items, the map has no highlighted tile, so the notice leads with that fact. Forms, longest first:
    1. `selected item too small to show · 14 hidden (3.1 MiB)`
    2. `selected too small · +14 hidden`
    3. `selected hidden`

    `selected hidden` takes priority over the count.
  - **Fit order when the rule is too narrow** (one rule for all forms):
    1. Truncate the path from the left to 24 cells (`…/nss/.cache`).
    2. Shorten the notice to form 2.
    3. Drop `(updating)`.
    4. Truncate the path to 12 cells.
    5. Shorten the notice to form 3.
  - **NO_COLOR:** plain text in the rule's own style, with no color or attribute of its own. It reads the same with color on, because meaning is in the words. ASCII fallback: the `─` becomes `-`.
  - **Count:** N is the number of in-scope items without a tile, and the size is their total in the active mode, with the same unit rules as SIZE. The notice updates on relayout (≤ 2 Hz).
  - **80×24 examples.** The first line is a short path, so form 1 fits. The second is a long path with the selected item hidden; the path is cut to 24 cells and the notice falls back to form 2:
```
 Map: /home/nss/.cache (updating) ─────── 14 items too small to show (3.1 MiB) ─
 Map: …t-portal/node_modules (updating) ────── selected too small · +14 hidden ─
```
- **Scope:** the map shows the children of the selected item's parent. When the zoomed view root itself is selected, the map shows the view root's children (design §3.7).

## 4. Key flows

**F1 Start (T1).**
1. `<cmd> [PATH]`, where PATH defaults to `.`. Flags are in design §4 CLI.
2. Frame 0 shows within 1 s: the root row with `◌`, and the map reads `Waiting for first results…`.
3. The scan is shallow-first (M11), so the top-level folders appear first and their sizes grow evenly.
4. When the scan finishes, the status line shows the done state (§5.2).

**F2 Explore (T2).**
- Moving is covered by the keys in §6.
- `⏎` zooms into the selected folder: it becomes the view root for both the tree and the map, and the header shows a breadcrumb `/home/nss › .cache`.
- **Zooming also prioritises that folder's scan** (M11). Zooming back up to the scan root clears the priority.
- Live re-sorting never moves the selection to another item: selection follows the node, and the viewport keeps the selected row on the same screen line where it can.

**F3 Whole-root views (T3).**
- `2` **Types:** category rows (sortable by size), each expandable with `→` into extensions. Columns: size, files, % of root.
- `3` **Largest:** the top N files (default and `--top N` from design §4), showing size and full path. `⏎` shows the file in the tree. `d` deletes it.
- `4` **Skipped:** path and reason, newest first. `r` on a "skipped by you" row resumes it.

**F4 Delete (T4).**
1. Select an item and press `d` (or `Delete`). The confirmation dialog in §5.3 opens.
   - If the target is fully scanned (`is_exact`), the dialog shows exact values.
   - Otherwise it uses the unfinished variant.
2. In the dialog, `y` deletes. `p` prioritises the target's scan and leaves the dialog open while its numbers update live. `n`, `Esc` and `Enter` cancel. Every other key is ignored.
3. While deleting, the row shows `(deleting…)` and the status line reports progress (§5.2). The rest of the UI stays usable.
4. When it finishes:
   - On success, a status message shows (§5.4) and all views update.
   - On partial failure or abort, the result dialog (§5.3) opens, and it must be dismissed.

**F5 Steer the scan (T5).**
- `p` puts the selected folder's scan first: `▲` on its row, and the status line shows `▲ First: .cache/`.
  - There is one priority folder at a time; a new `p` or zoom replaces it.
  - Pressing `p` on the current priority folder clears it.
  - When the folder completes, the status line shows `✓ .cache/ fully scanned: 64.9 GiB · 211,004 files` for 5 s, and `▲` disappears.
- `x` skips the selected folder's scan. There is no confirmation, because nothing on disk changes.
  - The row shows `⊘` and `(scan skipped)`.
  - The size gets a `≥` prefix, and so does every ancestor's.
  - It appears in Skipped as `skipped by you`.
- `r` resumes a skipped folder, or rescans any folder (design §3.5 subtree rescan). The row goes back to `◌` and the `≥` clears when it completes.
- `s` stops the whole scan: `■ Stopped at 01:12 · partial results`. Unfinished rows keep `◌`. `r` on the root starts a full rescan.
- `p`, `x` and `r` do nothing on files or aggregate rows. The status line says `p, x and r work on folders.`

**F6 Quit (T6, M12).** `q` or `Ctrl-C` always opens the quit dialog (§5.3); the first press never exits.
- **No delete running:** a second `q` (or `Ctrl-C`), or `y`, quits.
- **Delete running:** only `y` quits. `q` and `Ctrl-C` dismiss the dialog, and the delete continues.
- **SIGTERM/SIGHUP** can't prompt. They quit after the current entry and print the stderr line from design §3.8.

## 5. States, dialogs and copy

### 5.1 Per-folder state (tree row and tile)
| Condition (design §3.10/§3.11) | Glyph | Size prefix | Suffix |
|---|---|---|---|
| `!is_complete` (scan work pending) | `◌` (`▲` if it is the priority folder) | none | — |
| Skipped by the user | `⊘` | `≥` | `(scan skipped)` |
| `is_complete && !is_exact` (something inside was skipped or stopped) | blank | `≥` | — |
| Has an issue (unreadable, etc.) | `!` | none (size known so far) | `(permission denied)` etc. |
| Being deleted | `◌` | none | `(deleting…)` |
| Exact | blank | none | — |

- **Precedence when several apply:** `▲` > `⊘` > `◌` > `!`. An issue inside a subtree is shown only on the row that has it, plus the `!N` badge on the Skipped tab.
- **Note for the architect:** design §3.11 says "UI shows ◌ when !is_exact". This spec splits that in two: `◌` means work is pending, and `≥` means a lower bound with no work pending. Brief M11 requires `≥` for skipped folders. The model already exposes both flags, so no backend change is needed.

### 5.2 Status line
| State | Text |
|---|---|
| Starting (first 2 s, `Estimating`) | `◌ Starting…  00:00 elapsed  ETA estimating…` |
| Scanning, **no ETA source** (`Indeterminate`; the default first scan on btrfs) | Full: `◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · 1.24M entries · 96.1 GiB · 41k/s · ETA unknown (no previous scan)`. **No bar, no `%`, no ETA digits.** See the drop order below. |
| Scanning, ETA source exists (`Eta`) | `◌ Scanning [bar] ~41%  01:12 elapsed  ETA ~01:44 (estimate, last scan)`. The source label is **only** `last scan` (history file or an earlier scan this session) or `filesystem` (inode count), per design §3.6 / ADR-010. |
| Past estimate (`OverEstimate`) | `◌ Scanning [bar] ~99%  04:02 elapsed  ETA unknown (past estimate)` |
| With priority | `▲ First: .cache/` is placed before the scanning text. If it doesn't fit, the bar is dropped first. |
| Done | `✓ Done in 03:51 · 1,204,331 files · 182.9 GiB · 3 skipped (press 4)`. ETA is 00:00. |
| Done with skips by you | `✓ Done in 03:51 · … · sizes marked ≥ are partial` |
| Other filesystems not scanned (plan D4) | Appended to any state when ≥1 `OtherFilesystem` issue exists and `--cross-filesystems` is off: ` · not scanned (other fs): /home`. With several, it names the shallowest path and adds a count: ` · not scanned (other fs): /home +2`. When space is short, drop the bar first, then use ` · 3 other fs not scanned`. The full list is in `4 Skipped`. |
| Stopped | `■ Stopped at 01:12 · partial results · r on the root rescans` |
| Deleting (exact) | `Deleting /home/nss/.cache/yay … 12,400 / 48,212 files` |
| Deleting (unfinished) | `Deleting /home/nss/.cache/yay … 12,400 files removed` |
| Transient message | Replaces the state text for 5 s or until the next key, then the state text returns. |

- **Bar and percent:** shown **only when an ETA source exists** (`Eta`, `OverEstimate`). `~%` comes from `EtaView.fraction`, which never shows 100% before Done. In `Indeterminate`, `K of M top-level done` is a count and never a bar or `%`.
- **Indeterminate fields:**
  - `K of M top-level done` is omitted until the root's first listing arrives, and when the root has no subfolders.
  - `≥N folders queued` shows `pending_dirs` (k/M suffixes with 1 decimal) and drains to 0.
  - Entries and bytes are seen-so-far totals.
  - Rate is the EWMA in entries/s (`41k/s`).
- **Source appears mid-scan:** the line switches `Indeterminate → Estimating → Eta` with no transient message. Its changed form is the signal.
- **80-column drop order (Indeterminate):** fields are removed in this order until the line fits:
  1. rate;
  2. bytes;
  3. entries;
  4. the `(no previous scan)` qualifier;
  5. `folders queued` shortens to `queued`;
  6. `12 of 31` shortens to `12/31`;
  7. the queued field;
  8. the top-level field.

  **Never dropped:** the state glyph and word, elapsed, and `ETA unknown`. At 80 columns with no prefix, the result is `◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown` (79 cols). With the `▲ First: …` prefix, the same order continues.
- **80-column drop order (Eta):**
  1. the bar shrinks from 16 to 10 cells;
  2. `elapsed` shortens to its time only, after a `·` separator;
  3. the bar is removed.

  **Never dropped:** `~%`, `~` before the time, `estimate`, and the source label.
- **Elapsed:** updates at least once per second.
- **Spinner:** a frame that replaces the `◌`, at no more than 10 fps. It is off with `--no-animation`.

### 5.3 Dialogs
All dialogs are centered and 64 columns wide (or the terminal width minus 4, whichever is smaller). They have a double border with the title on it. Paths wrap and are never truncated. Default actions are bracketed. Every key not listed is ignored.

**Delete: exact target**
```
╔═ Delete permanently? ════════════════════════════════════════╗
║  /home/nss/.cache/yay                                        ║
║  Folder · 31.0 GiB (disk usage) · 48,212 files · 1,903       ║
║  folders                                                     ║
║  Space freed may be less: 12 files have other hard links.    ║
║                                                              ║
║  This cannot be undone. Files are not moved to the trash.    ║
║                                                              ║
║        y  Delete             [ n / Esc / Enter  Cancel ]     ║
╚══════════════════════════════════════════════════════════════╝
```
- The hard-link line is shown only when `outside_links > 0`.
- For a file target, the second line reads `File · 4.2 GiB (disk usage)`, and there are no counts.

**Delete: unfinished target (M7 v0.5, design §3.11 O3)**
```
╔═ Delete permanently? (scan of this folder not finished) ═════╗
║  /home/nss/.cache/yay                                        ║
║  Folder · ≥ 31.0 GiB (disk usage) · ≥ 48,212 files ·         ║
║  ≥ 1,903 folders   (at least; still counting)                ║
║                                                              ║
║  Largest known inside:                                       ║
║    18.2 GiB  pkg/                                            ║
║     9.6 GiB  cache/                                          ║
║     2.1 GiB  linux-cachyos-7.2.9.pkg.tar.zst                 ║
║                                                              ║
║  ◌ Everything inside will be deleted, including files not    ║
║    counted yet. Hard links: not fully known (scan not        ║
║    finished).                                                ║
║  This cannot be undone. Files are not moved to the trash.    ║
║                                                              ║
║  y Delete now   p Finish scan first   [ n/Esc/Enter  Cancel ]║
╚══════════════════════════════════════════════════════════════╝
```
- **Live numbers:** the bounds update live, at most twice a second, while the dialog is open.
- **After `p`:**
  - The `p` hint becomes `▲ Scanning this folder first…`.
  - When the folder completes, the dialog changes in place to the exact variant. The title becomes `Delete permanently?`, the `≥` prefixes and "still counting" go away, and the hard-link line is recomputed.
  - Focus and the default action stay the same. `y` works at any time.
- **Fewer than 3 known children:** list as many as are known. With none, the dialog says `Nothing listed inside yet.`
- **After `y`:** the subtree's scan is cancelled and the delete starts.

**Delete: refused** (shown in the status line, not as a dialog)
| Target | Text |
|---|---|
| Scan root | `Can't delete the scan root.` |
| Aggregate row or tile | `"12 smaller" is a group, not a file. Expand it to choose an item.` |
| Other filesystem | `/path is on another filesystem and wasn't scanned. Not deleted.` |
| Inside or equal to a running delete | `Already being deleted.` |
| Another delete is running | `One delete at a time. Wait for /path to finish.` |

**Delete: result with failures** (`Partial`)
```
╔═ Delete finished with errors ════════════════════════════════╗
║  /home/nss/.cache/yay                                        ║
║  Deleted 48,100 files · 1,890 folders · freed 30.8 GiB       ║
║  112 items could not be deleted:                             ║
║    ! permission denied        108                            ║
║    ! busy or in use             4                            ║
║  The folder is being rescanned to show what is left.         ║
║                                                              ║
║        v  View failed paths              [ Enter  OK ]       ║
╚══════════════════════════════════════════════════════════════╝
```
- **Failure labels** (from `DeleteFailKind`):
  - `permission denied`
  - `busy or in use`
  - `read-only filesystem`
  - `on another filesystem (not crossed)`
  - `other error (errno N: <strerror>)`
- **`v`** opens a scrollable list of the failed paths, each with its reason. `Esc` returns to the result dialog.

**Delete: aborted, nothing deleted** (`Aborted`)
```
╔═ Not deleted ════════════════════════════════════════════════╗
║  /home/nss/.cache/yay                                        ║
║  <reason>                                                    ║
║  Nothing was deleted.                         [ Enter  OK ]  ║
╚══════════════════════════════════════════════════════════════╝
```
- **Reasons:**
  - `ChangedSinceScan` → `It changed since it was scanned (it may have been replaced or moved). Rescan with r, then try again.`
  - `NotFound` → `It no longer exists.`
  - `OpenFailed` → `It couldn't be opened (errno N: <strerror>).`

**Quit: no delete running**
```
╔═ Quit <cmd>? ════════════════════════════════════════╗
║  Scan results will be lost.                          ║
║                                                      ║
║      q / y  Quit          [ n / Esc / Enter  Stay ]  ║
╚══════════════════════════════════════════════════════╝
```

**Quit: delete running (M12)**
```
╔═ Quit while deleting? ═══════════════════════════════╗
║  A delete is in progress:                            ║
║  /home/nss/.cache/yay                                ║
║  12,400 of 48,212 files removed.                     ║
║                                                      ║
║  Quitting stops it after the current file and leaves ║
║  the folder partly deleted.                          ║
║                                                      ║
║   y  Quit anyway     [ any other key  Keep deleting ]║
╚══════════════════════════════════════════════════════╝
```
- For an unfinished target, the counts line reads `12,400 files removed so far.`
- After `y`, stderr shows `<cmd>: delete of /home/nss/.cache/yay interrupted: 12,410 of 48,212 files removed` (design §3.8).

### 5.4 Messages (status line, transient)
| Event | Text |
|---|---|
| Delete removed | `Deleted /home/nss/.cache/yay · freed 31.0 GiB (48,212 files)` |
| Delete removed more than was scanned | `Deleted /home/nss/.cache/yay · freed 33.4 GiB (61,880 files, more than were counted)` |
| Priority set / cleared | `▲ Scanning .cache/ first.` / `Priority cleared.` |
| Priority folder finished | `✓ .cache/ fully scanned: 64.9 GiB · 211,004 files` |
| Skip / resume | `⊘ Skipped .local/. Its size is partial (≥). r resumes.` / `◌ Resuming .local/…` |
| Size mode toggled | `Showing apparent size.` / `Showing disk usage.` |
| Key not applicable | `p, x and r work on folders.` / `Nothing to delete here.` |
| `x` on a folder already fully scanned (plan D6) | `Already fully scanned.` |
| `x` on a folder already skipped | `Already skipped. r resumes.` |

### 5.5 View states
| View | Starting | Scanning | Done | Stopped | Empty |
|---|---|---|---|---|---|
| Tree | Root row, `◌` | Rows grow and re-sort | Final | `◌` stays on unfinished rows | `This folder is empty.` |
| Map | `Waiting for first results…` | Relayout at most twice a second, title `(updating)` | Final | Title `(partial)` | `This folder is empty.` / `N items, all 0 B. Nothing to map.` |
| Types | `No files yet.` | Live | Final | Header `partial` | `No files found.` |
| Largest | `No files yet.` | Live | Final | Header `partial` | `No files found.` |
| Skipped | `Nothing skipped.` | Live | Final | Final | `Nothing skipped.` |

**Usage errors (CLI validation, before the TUI starts).** These go to stderr with **exit code 2**. The terminal is not switched to raw mode or the alternate screen. The first line names the flag, the allowed range and the value given. The second line is always `Try '<cmd> --help'.` Messages:
- `<cmd>: --cell-aspect must be a number from 0.5 to 4.0 (got "abc")`. NaN and infinity are rejected with the same message.
- `<cmd>: --threads must be a whole number, 1 or more (got "0")`
- `<cmd>: --top must be a whole number, 1 or more (got "-3")`
- Several invalid flags: report the first one only, in command-line order, so the user fixes one thing at a time.

**Pre-TUI errors** go to stderr with a non-zero exit, and no TUI is drawn:
- `<cmd>: /foo: no such file or directory`
- `<cmd>: /foo: not a directory`
- `<cmd>: /root: permission denied`

## 6. Keybindings
| Key | Action | Where |
|---|---|---|
| `↑↓` `j k`, `PgUp PgDn`, `g G` | Move, page, top/bottom | Tree, lists |
| `→ l` | Expand, or move to the first child | Tree, Types |
| `← h` | Collapse, or move to the parent | Tree, Types |
| arrows `hjkl` | Move to the adjacent tile (`layout::neighbor`) | Map focus |
| `⏎` | Zoom into the folder **and prioritise it** / Largest: show in tree | Tree, map, Largest |
| `⌫` `-` | Up one level (zoom out); clears priority when back at the root | Tree, map |
| `Tab` / `Shift-Tab` | Focus tree ↔ map | Main |
| `1` `2` `3` `4` | Tree, Types, Largest, Skipped | Global |
| `d` `Delete` | Delete the selected item (dialog) | Tree, map, Largest |
| `p` | Scan this folder first (toggle) | Folder rows and tiles; delete dialog |
| `x` | Skip this folder's scan | Unfinished folder |
| `r` | Resume a skipped folder / rescan a folder | Folder; Skipped list |
| `s` | Stop the whole scan | While scanning |
| `a` | Disk usage ↔ apparent size | Global |
| `?` | Help overlay (every key, grouped as in this table) | Global |
| `q` `Ctrl-C` | Quit dialog (§5.3) | Global |
| `Esc` | Close the dialog or overlay; never quits | Dialogs |

**Key hints (last row), by context:**
- Tree: `↑↓ move ←→ fold ⏎ zoom ⌫ up Tab map d delete p first x skip ? help`
- Map: `arrows move ⏎ zoom ⌫ up Tab tree d delete p first ? help`
- Lists: `↑↓ move ⏎ show in tree d delete ? help`
- If the hints don't fit, they are dropped from the right, but `? help` is always kept.

**Help overlay (`?`), ETA note** (shown at the bottom of the overlay, one line, always present):
`ETA appears when a previous complete scan of this root is known. Run with --history to remember scans between sessions (stores one small file under ~/.local/state).`

## 7. Visual system
- **Text and chrome:** the terminal's default foreground and background. Dim text is used only for decoration such as tree guides, never for information.
- **Selection and focus:** reverse video plus `>` marks selection. The focused pane's title is bold with `*`.
- **Category fills** (Okabe-Ito; black label text unless noted; WCAG contrast computed 2026-10-23 with the relative-luminance formula):

| Category | Hex | Label text | Contrast |
|---|---|---|---|
| `[V]` video | #E69F00 | black | 9.32:1 |
| `[I]` image | #56B4E9 | black | 9.10:1 |
| `[A]` archive | #009E73 | black | 6.14:1 |
| `[C]` code/text | #F0E442 | black | 15.88:1 |
| `[B]` binary | #0072B2 | **white** | 5.19:1 |
| `[D]` documents | #D55E00 | black | 5.43:1 |
| `[O]` other/audio | #CC79A7 | black | 6.86:1 |
| aggregate or mixed | #BBBBBB | black | 10.94:1 |

- **Color depth:** truecolor if `COLORTERM` is `truecolor` or `24bit`; otherwise the nearest xterm-256 color with black or white label text chosen per the contrast table (**Open question:** recheck the 256-color contrast in build review); otherwise monochrome.
- **ASCII fallback** (no Unicode or `LANG=C`): `◌`→`~`, `▲`→`^`, `⊘`→`-`, `≥`→`>=`, `▾▸`→`-+`, `█…`→`#`, double border→`#`, single border→`+-|`.
- **Motion:** spinner at no more than 10 fps; data redraws at no more than 10 Hz; map relayout at no more than 2 Hz (design §5). Nothing blinks.

## 8. Accessibility
Target: the intent of WCAG 2.2 AA where it maps to a TUI (1.4.1, 1.4.3, 2.1.1, 2.3.1, 2.4.7), plus NO_COLOR and clig.dev conventions.
- **No color-only meaning.** Every state in §5.1 has a glyph and/or words. Categories carry a letter tag. Selection has `>`. Focus has `*`. Bars always have a number next to them.
- **`NO_COLOR`:** when set and non-empty, no color is emitted, but bold and reverse still are. A flag or config can override it (no-color.org).
- **Keyboard:** every function is reachable by keyboard. Dialogs trap focus. In every dialog, the safe action is the default for `Enter`.
- **Reduced motion:** `--no-animation` gives a static `◌` and no spinner.
- **Screen readers:** a known TUI limitation. Status text and dialogs use full words.
- **Terminal size:** 80×24 is the floor. Below it: `Terminal too small: need 80×24, have 64×20. Resize, or press q to quit.` (`q` opens the quit dialog as usual.)

## 9. Traceability
| Req | Satisfied by |
|---|---|
| M1 | F1 frame 0, §5.5 Starting; input never waits for the scan (design §3.3) |
| M2 | §5.2 status states: estimating; `~ETA (estimate, source)` when a source exists; `Indeterminate` with `ETA unknown (no previous scan)` otherwise; past estimate; Done with 00:00. **Gap (design §3.6):** on a first scan without a source, the ETA is not "revised as the scan proceeds". project-owner is amending M2. |
| M3 | §3.1–3.2 tree, root at top, largest first, % of parent, F2 |
| M4 | §3.3 map scope, ADR-006 proportionality, shared selection |
| M5 / M6 | F3 Types and Largest, whole root, live |
| M7 | F4, §5.3 exact and unfinished dialogs, refusals, result and abort dialogs |
| M8 | Backend (design §4 tests); UI shows the model's sizes only |
| M9 | Only `y` in the delete dialog modifies disk; `x`, `p`, `r` and `s` don't touch the disk |
| M10 | §5.1 `!` rows, Skipped view, `!N` badge |
| M11 | F5, `⏎` and `p` prioritise, `x` and `r`, `≥` and `⊘` |
| M12 | F6, §5.3 quit dialogs |
| Should: one-filesystem default | `(other filesystem)` suffix, Skipped reason |
| Should: size-mode toggle | `a`, header label |
| Should: stop | `s`, Stopped state |
| Could: rescan | `r` |

**Gaps:** export (Could) is not specified. The only UI for the opt-in history is the ETA source label and the help line in §6. Both are intentional for v1.

## 10. Usability risks and cheapest tests
| Risk | Test |
|---|---|
| Live re-sorting causes the wrong item to be selected | TestBackend: feed a shuffling `ScanMsg` stream, press `↓` 20 times, then assert the selected NodeId follows the expected sequence. |
| `≥` deletes surprise the user | Show the user §5.3's unfinished dialog and ask what will be deleted. They pass if they say "the whole folder, possibly more than shown". |
| Zoom silently changing scan priority confuses the user | During the first real use, check whether `▲ First:` in the status line is noticed. Fallback: show the transient message on the first zoom only. |
| Map strip too short at 24 rows | Render the user's home at 80×24 and 120×40, then count labelled tiles (≥4 at 80×24). |
| Quit dialog during delete dismissed by habit | Covered by design: only `y` quits, so a habitual `q q` keeps the delete running. |

## 11. Acceptance checks (scriptable; ratatui `TestBackend` unless noted)
1. **80×24 snapshots** exist for: §3.1 scanning; Done; Stopped; empty folder; each view; each dialog in §5.3; too small (79×24). Each one is also rendered with `NO_COLOR=1` and with the ASCII fallback.
2. **NO_COLOR** set: the rendered buffer has no cell with a non-default fg or bg color. Each §5.1 state is still distinguishable by its glyph or suffix text, and a test diffs the two buffers.
3. **Contrast:** for each fill in §7 (and each 256-color mapping), `contrast(label, fill) ≥ 4.5`.
4. **ETA:**
   - It never shows a number while `Estimating`.
   - **In `Indeterminate`, the status line contains no `%`, no bar glyphs (`█▉▊▋▌▍▎▏░`), and no ETA digits.** The text after `ETA` must match `^ETA unknown` at 80, 120 and 200 columns.
   - When it is a number, it always contains `~` and `estimate`.
   - The source label is only `last scan` or `filesystem`, and never `rough`.
   - After `Done`, it shows `00:00`, or `Done in`.
   - `~%` is never `100` before Done.
   - At 80 columns, the Indeterminate line still contains `Scanning`, the elapsed time and `ETA unknown`.
5. **Elapsed** text changes at least once in each 1,000 ms window of a synthetic run.
6. **Delete dialog:**
   - `Enter`, `n` and `Esc` produce no `DeleteRequest`.
   - Other keys are ignored.
   - `y` produces exactly one request.
   - When the target is `!is_exact`, the title contains `scan of this folder not finished`, every count has `≥`, and up to 3 known children are listed.
   - `p` sets the focus and the dialog stays open.
   - When the target completes, the title loses `not finished` and the `≥` prefixes disappear.
7. **Refusals:** the scan root, an aggregate, `OTHER_FS`, and a second delete each produce the §5.3 text and no request.
8. **Quit:**
   - The first `q` or `Ctrl-C` never exits. With no delete running, `q q` exits.
   - With a delete running, `q q` and `Ctrl-C Ctrl-C` do not exit, the delete continues, and `y` exits with the stderr line.
9. **Zoom** (`⏎`) on a folder sets the `ScanControl` focus to that path. Zooming back up to the root clears it. `p` toggles it.
10. **`x`:** the row shows `⊘`, `(scan skipped)` and a `≥` size, and every ancestor shows `≥`. After `r` and completion, all of those marks clear.
11. **Label truncation:** with a 200-character name and a name containing ESC, the row stays within its width and no raw control byte reaches the buffer.
12. **Key hints** at 80 columns always contain `? help`.
13. **Hidden-items notice:**
    - In a fixture with 1 huge child and 14 tiny children, where layout produces no aggregate tile, the map title row at 80×24 contains `too small to show` or `hidden` together with `14`.
    - When an aggregate tile is drawn, the row contains neither.
    - When the selected item is hidden, the row contains `selected`.
    - Under `NO_COLOR=1`, the notice cells have the default fg and bg.
    - The map keeps ≥ 6 rows.
14. **CLI validation:** each of these exits with code 2, prints the matching §5.5 line plus `Try '<cmd> --help'.` to stderr, and writes nothing to stdout:
    - `--cell-aspect` with `0.49`, `4.01`, `NaN`, `inf` or `abc`
    - `--threads 0`
    - `--top 0` or `--top -3`

    The run never enters the alternate screen; check that stdout contains no `\x1b[?1049h`. `--cell-aspect 0.5`, `--cell-aspect 4.0`, `--threads 1` and `--top 1` are accepted.
