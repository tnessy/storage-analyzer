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
╔ .cache/ 64.2G 35% ══════╗┌ Videos/ 41.8G 23% ────┐┌ .local/ ≥28.1G 15% ─────┐
║                         ║│                      ││                        │
║                         ║│                      │└────────────────────────┘
║                         ║│                      │┌ Projects/ 19.6G ┐┌ Dow…┐
║                         ║│                      ││ 11%             ││ 7% !│
╚═════════════════════════╝└──────────────────────┘└─────────────────┘└─────┘
 ◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown
 ↑↓ move  ←→ fold  ⏎ zoom  ⌫ up  Tab map  d delete  p first  x skip  ? help
```

### 3.2 Row anatomy (Tree)
`[sel] SIZE  %  BAR  STATE  INDENT+TWISTY NAME  SUFFIX`

| Field | Rule |
|---|---|
| `sel` | `>` on the selected row. The row's full style depends on focus (§3.2.1). |
| SIZE | Size in the active mode, in the **compact** format from §7.1 (`64.2G`, `812.0K`, `0B`), right-aligned in 7 cells. Prefixed `≥` when the size is a lower bound (§5.1). |
| % | Percent of parent, 1 decimal, right-aligned in 5 cells (`100.0`, `35.2`, `0.0`) |
| BAR | 10 cells, eighth-cell steps (`█▉▊▋▌▍▎▏`); the ASCII fallback is `#` |
| STATE | One glyph: `◌` scanning, `▲` prioritised, `⊘` skipped by you, `!` has an issue, or blank. The precedence (§5.1) applies when several are true. |
| NAME | Directories end in `/`. Names are sanitized (design §4 Security) and truncated with `…` by display width. |
| SUFFIX | `(scan skipped)`, `(permission denied)`, `(other filesystem)`, `(deleting…)` |
| `..` row | Present only when zoomed (§3.2.2). |
| Aggregate | `(N smaller, SIZE)`, which can't be deleted or zoomed. An expanded folder shows its 50 largest children, and the rest go into one aggregate row (plan D5; the threshold is a constant). `→` on the aggregate reveals the next 50, and that stays revealed for this folder until it is collapsed. **The selected item is never hidden in an aggregate:** if a live re-sort pushes it below the cut-off, it stays as its own row just above the aggregate until the selection moves away. |

#### 3.2.1 Selected row (revised after build feedback, 2026-10-23)
**Observed in the build** (tmux capture, 80×24, this session):
- The selected row is drawn in reverse video, but its tree-guide characters are also **dimmed** (SGR 2 inside SGR 7), so part of the row loses contrast.
- Selection is otherwise marked only by the `>` in column 1.
- On the map, the selected tile differs from its neighbours only by double versus single border lines, drawn in the same black on the same fill.

**Rule.** Reverse video means "this is where your keys go". It appears in exactly one pane at a time.

| Tree has focus | Map has focus |
|---|---|
| The selected tree row is **reverse video and bold across the full pane width**, every cell from column 1 to the right edge. No cell in that row is dimmed: guides, bar and suffix all take the row style. `>` stays in column 1. | The selected tree row is **bold, with the name underlined**, plus `>`. It is not reversed. |

- **NO_COLOR:** same rule, because reverse, bold and underline are attributes, not colors.
- **Not color-only:** focus is still shown by `*` before the focused pane's title, and the selection by `>` and the attribute.

#### 3.2.2 `..` row (parent shortcut; added after build feedback)
- **When it shows:** only when the view root is not the scan root (after `⏎` zoom). It is **row 0**, above the view root's own row.
- **Look:** the SIZE, %, BAR and STATE columns are blank. NAME is `..`, followed by two spaces and `(up to /home/nss)`, the parent path, truncated from the left with `…`. The `..` row has no guides.
```
 <cmd>  /home/nss › .cache                     Disk usage   ? help   q quit
 [1 Tree]  2 Types  3 Largest  4 Skipped
*   SIZE      %  OF PARENT      NAME
                                ..  (up to /home/nss)
>  64.2G  100.0  ██████████  ◌  .cache/
   31.0G   48.3  ████▊          ├── yay/
```
- **Sorting and grouping:** the row is pinned. It is never sorted, never counted in "50 largest", never put in an aggregate, and never shown in Types, Largest or the map. It scrolls with the list as row 0, and `g` goes to it.
- **Selection:**
  - After a zoom, the selection lands on the **view root row**, not on `..`, so `..` is one `↑` away.
  - `↑` on `..` does nothing.
  - It is selected like any row (§3.2.1).
- **Keys on `..`:**
  - `⏎`, `⌫`, `-`, `←` and `h` go up one level, the same as `⌫` anywhere. After going up, the selection is the folder you came from (this applies to `⌫` everywhere).
  - `→` and `l` do nothing.
  - `d`, `p`, `x` and `r` show `".." is the parent folder. Press ⏎ to go up.` and change nothing.
  - `Tab` moves focus to the map and selects the largest tile.
  - Going up to the scan root clears the zoom priority (F2), as before.
- **Map while `..` is selected:**
  - The map keeps showing the **view root's children**, the same as when the view root row is selected, so it doesn't jump when you move between `..` and the root row.
  - No tile is highlighted.
  - The map title appends ` · ⏎ goes up to /home/nss`, truncated first, before the hidden-items notice.
  - The `selected hidden` notice never fires for `..`.
  - *Alternative considered and rejected:* previewing the parent level with the current folder highlighted. The map would redraw at a different level on every `↑`/`↓` between `..` and the root row, and the highlighted tile would not be the selected item.

### 3.3 Treemap tile anatomy
- **Frame ("grout"):** the border lines and the title row of every tile are drawn on the **terminal's default background and foreground**, not on the tile's fill. Only the interior is filled. Every tile then reads as a separate coloured block with a neutral gap around it, so same-coloured neighbours no longer merge. This works the same with NO_COLOR, where there are borders but no fill.
- **Selected tile (revised):**
  - Double border (`╔═╗`).
  - Its **whole frame** (border cells and title row) is drawn in **reverse video and bold**, so it reads as a solid bright outline.
  - The title starts with `>`.
  - The interior keeps its normal fill (§7), so the tile is still recognisable.
  - This applies whether the tree or the map has focus, because the map tile is the main "where am I" cue. The map pane's `*` shows which pane has keyboard focus.
  - NO_COLOR: the same, using attributes only.
- **Other tiles:** single border in the default foreground.
- **Label:** first line is `name size pct% [T]`, and the second line holds whatever didn't fit. Labels truncate with `…`. Tiles narrower than 6 cells get no label, but a `!` or `⊘` marker still shows if one applies.
- **Category tag:** `[V]` video, `[I]` image, `[A]` archive, `[C]` code/text, `[B]` binary, `[D]` documents, `[O]` other/audio.
  - **File tiles:** `name size pct% [T]`.
  - **Folder tiles:** `name/ size pct%`, with **no tag**. The trailing `/` is the folder marker, and a folder's dominant category is no longer shown on the map; the Types view covers it.
  - This makes the two tile kinds readable without colour.
- **Fill (option C, chosen by the user 2026-10-23):**
  - **File tiles** use their file-type colour.
  - **Folder tiles** use a colour of their own, from the same palette, but **never a hue that a file type uses in the same view** (§7).
  - Within one map view, a hue therefore means exactly one thing: a file type, or "just this folder".
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
  - **`r` on a folder that is still being scanned** (the scan is running and the folder still has pending work) **does not restart it. It acts as `p`.** The scan is already reading fresh data, so restarting would only repeat disk reads and delay Done. The user's likely intent, "get me current numbers for this folder soon", is best met by moving it to the front.
    - The folder becomes the priority folder: `▲` on its row and `▲ First:` in the status line.
    - **Skipped folders inside are resumed too.** The outermost skipped folders under it go from `⊘` to `◌` and are scanned (prioritised along with the folder). Skips nested inside those are cleared by their own rescan (design D2), so they don't show `⊘` again.
    - Message, when nothing inside was skipped: `Still scanning .cache/: moved it to the front instead of restarting.`
    - Message, when skipped folders were resumed: `Still scanning .cache/: moved it to the front and resumed 2 skipped folders inside.` The number counts the outermost skipped folders only. Use `1 skipped folder` for one.
    - If it is already the priority folder: `Already scanning .cache/ first.` Nothing changes, and `r` never toggles the priority off (only `p` does).
  - **`r` on the scan root while the scan is running:** nothing changes. Message: `Scan in progress. Press s to stop, then r to rescan.`
  - **`r` on a folder with no pending work runs a real rescan.** That covers folders that are complete, complete but marked `≥` because of an inner skip, or left unfinished after `s` (Stopped).
  - This UX holds whichever mechanism the architect picks for O7, because no user-visible restart happens while the folder is still being scanned.
- `s` stops the whole scan: `■ Stopped at 01:12 · partial results`. Unfinished rows keep `◌`. `r` on the root starts a full rescan.
  - **Between `s` and the stop completing** (until `Done { cancelled }` arrives), the status line shows `■ Stopping…  01:12 elapsed`. Elapsed is frozen at the moment `s` was pressed, and there is no ETA.
  - During `Stopping…`, `r`, `p` and `x` do nothing and show `Still stopping the scan. Try again when it says Stopped.` The key is not queued, so nothing starts later by surprise. Navigation, views, `d` and quit work as normal.
  - A second `s` during `Stopping…` does nothing.
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
| Starting, no ETA source (first 2 s) | `◌ Starting…  00:00 elapsed`, with no ETA field. After 2 s it becomes the `Indeterminate` line, which says `ETA unknown`. |
| Starting, ETA source exists (`Estimating`) | `◌ Starting…  00:00 elapsed  ETA estimating…`. `estimating…` appears **only** when a source exists (design rev 8, T013/T019), so it never promises an estimate that won't come. |
| Scanning, **no ETA source** (`Indeterminate`; the default first scan on btrfs) | Full: `◌ Scanning  01:12 elapsed · 12 of 31 top-level done · ≥12.4k folders queued · 1.24M entries · 96.1 GiB · 41.2k/s · ETA unknown (no previous scan)`. **No bar, no `%`, no ETA digits.** See the drop order below. |
| Scanning, ETA source exists (`Eta`) | `◌ Scanning [bar] ~41%  01:12 elapsed  ETA ~01:44 (estimate, last scan)`. The source label is **only** `last scan` (history file or an earlier scan this session) or `filesystem` (inode count), per design §3.6 / ADR-010. |
| Past estimate (`OverEstimate`) | `◌ Scanning [bar] ~99%  04:02 elapsed  ETA unknown (past estimate)` |
| With priority | `▲ First: .cache/` is placed before the scanning text. If it doesn't fit, the bar is dropped first. |
| Done | `✓ Done in 03:51 · 1,204,331 files · 182.9 GiB · 3 skipped (press 4)`. ETA is 00:00. |
| Done with skips by you | `✓ Done in 03:51 · … · sizes marked ≥ are partial` |
| Other filesystems not scanned (plan D4) | Appended to any state when ≥1 `OtherFilesystem` issue exists and `--cross-filesystems` is off: ` · not scanned (other fs): /home`. With several, it names the shallowest path and adds a count: ` · not scanned (other fs): /home +2`. When space is short, drop the bar first, then use ` · 3 other fs not scanned`. The full list is in `4 Skipped`. |
| Stopping (after `s`, before `Done { cancelled }`) | `■ Stopping…  01:12 elapsed` |
| Stopped | `■ Stopped at 01:12 · partial results · r on the root rescans` |
| Deleting (exact) | `Deleting /home/nss/.cache/yay … 12,400 / 48,212 files` |
| Deleting (unfinished) | `Deleting /home/nss/.cache/yay … 12,400 files removed` |
| Transient message | Replaces the state text for 5 s or until the next key, then the state text returns. |

- **Bar and percent:** shown **only when an ETA source exists** (`Eta`, `OverEstimate`). `~%` comes from `EtaView.fraction`, which never shows 100% before Done. In `Indeterminate`, `K of M top-level done` is a count and never a bar or `%`.
- **Indeterminate fields:**
  - `K of M top-level done` is omitted until the root's first listing arrives, and when the root has no subfolders.
  - `≥N folders queued` shows `pending_dirs` (count format, §7.1) and drains to 0.
  - Entries and bytes are seen-so-far totals.
  - Rate is the EWMA in entries/s (count format plus `/s`, e.g. `41.2k/s`).
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

  **Never dropped:** the state glyph and word, elapsed, and `ETA unknown`. At 80 columns with no prefix, the result is `◌ Scanning  01:12 elapsed · 12/31 top-level done · ≥12.4k queued · ETA unknown` (78 cells of text; with the 1-cell left margin shown in the §3.1 mockup, the line occupies 79 of 80 columns). With the `▲ First: …` prefix, the same order continues.
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
  - `MountPoint` → `It's a mount point (another filesystem or subvolume is mounted here), so it can't be deleted from here. Unmount it first if you want to remove it.`

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
| `r` on a folder still being scanned (plan O7) | `Still scanning .cache/: moved it to the front instead of restarting.` / `… moved it to the front and resumed N skipped folders inside.` |
| `r`, `p` or `x` while stopping | `Still stopping the scan. Try again when it says Stopped.` |
| `r` on the current priority folder, still scanning | `Already scanning .cache/ first.` |
| `d`, `p`, `x` or `r` on `..` | `".." is the parent folder. Press ⏎ to go up.` |
| `r` on the root while scanning | `Scan in progress. Press s to stop, then r to rescan.` |

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
| `⌫` `-` | Up one level (zoom out); the selection becomes the folder you left. Clears priority when back at the root. | Tree, map |
| `⏎` `←` `h` on `..` | Up one level (same as `⌫`) | Tree `..` row |
| `Tab` / `Shift-Tab` | Focus tree ↔ map | Main |
| `1` `2` `3` `4` | Tree, Types, Largest, Skipped | Global |
| `d` `Delete` | Delete the selected item (dialog) | Tree, map, Largest |
| `p` | Scan this folder first (toggle) | Folder rows and tiles; delete dialog |
| `x` | Skip this folder's scan | Unfinished folder |
| `r` | Resume a skipped folder, or rescan a folder. On a folder still being scanned it acts as `p` (F5). | Folder; Skipped list |
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
- **Selection and focus:** see §3.2.1 (tree row) and §3.3 (map tile). The focused pane's title is bold with `*`. **Rows in reverse video never use DIM.**
- **Tile colours: option C, hybrid (chosen by the user 2026-10-23).** Files are coloured by type and folders by identity. The A/B/C comparison is kept in the change log at the end of this section.

**Palette.** One table serves both roles. The file-type assignment is fixed. Contrast was computed with the WCAG formula. The xterm-256 value is the nearest entry by RGB, with its contrast rechecked.

| Hue | Truecolor | xterm-256 | Label | Contrast (truecolor / 256) | File type (fixed) | Folder order |
|---|---|---|---|---|---|---|
| yellow | #F0E442 | 221 (#FFD75F) | black | 15.88 / 15.14 | `[C]` code/text | 0 |
| blue | #0072B2 | 25 (#005FAF) | **white** | 5.19 / 6.45 | `[B]` binary | 1 |
| orange | #E69F00 | 178 (#D7AF00) | black | 9.32 / 10.02 | `[V]` video | 2 |
| bluish green | #009E73 | 35 (#00AF5F) | black | 6.14 / 7.30 | `[A]` archive | 3 |
| sky blue | #56B4E9 | 74 (#5FAFD7) | black | 9.10 / 8.60 | `[I]` image | 4 |
| vermillion | #D55E00 | 166 (#D75F00) | black | 5.43 / 5.53 | `[D]` documents | 5 |
| reddish purple | #CC79A7 | 175 (#D787AF) | black | 6.86 / 7.97 | `[O]` other/audio | 6 |
| grey (reserved) | #BBBBBB | 250 (#BCBCBC) | black | 10.94 / 11.06 | aggregate `(+N smaller)` only | never |

All pairs are ≥ 5.19:1, which meets ≥ 4.5:1. The folder order alternates light and dark, so neighbouring folders differ in lightness as well as hue, and lightness survives colour-vision deficiency.

**Avoiding the ambiguity (an orange folder next to an orange video file).**

The Okabe-Ito set has only 7 hues, and file types use all 7. So no *global* split exists that keeps every file-type colour and still leaves hues for folders. The split is therefore made **per view**:

- **Reserved set R(view).** The hues of every file-type category that has at least one *direct file child* in the map's scope folder (the folder whose children the map shows).
  - R includes small files that got no tile. This keeps it stable, because adding a tile never changes R.
  - R only grows during a session. A delete does not shrink it; a rescan of that folder rebuilds it.
- **Folder tiles may use only the hues not in R.** File tiles always use their type hue.
- **Result:** in any one view, no folder shares a hue with any file type present, so colour sameness never suggests a false relation.
- **Typical case:** a scope folder's direct files cover 0–2 types, which leaves folders 5–7 hues.
- **Second cue** (also for NO_COLOR and for colour-vision deficiency): folders show a trailing `/` and no tag; files show a `[T]` tag and no `/`.

**Assignment (deterministic, sticky, per relayout ≤ 2 Hz).** Layout order is size descending, ties broken by NodeId. The *neighbours* of a tile are the tiles already processed in this pass that share a border segment with it, files included.
1. **File tile:** its type hue. **Aggregate:** grey.
2. **Folder tile:** the free hues are the folder-order hues not in R.
   1. **Memory:** `AppState` keeps a map from `NodeId` to hue for the session. A folder's tile only ever appears in its parent's view, so a remembered hue is always judged against the same R.
   2. **Keep:** if the remembered hue is free and no neighbour uses it, keep it.
   3. **Pick:** otherwise, take the first free hue in folder order, starting at `NodeId mod 7` and skipping hues in R, that no neighbour uses. Remember it.
   4. **All free hues used by neighbours:** take the free hue used least among the neighbours, lowest order on a tie. The grout frame still separates the tiles.
   5. **No free hues** (direct files of all 7 types): draw folder tiles **unfilled**, with frame and label on the default background, as under NO_COLOR. "Unfilled" then also reads as "folder".
3. **When colours change:** a folder only changes colour when its hue is newly added to R, which happens once, when the first file of that type is seen in the scope folder, or when it would otherwise touch a same-coloured neighbour. Size changes and re-ranking alone never recolour.

**Pure function:** `assign(tiles, adjacency, kinds, reserved, memory) -> (fills, memory')`, where `fills` is a hue or `Unfilled` for each tile. It is unit-tested without a terminal.

**Other rules:**
- **NO_COLOR and monochrome:** no fills. The grout frames separate the tiles; `/` versus `[T]` tells folders from files. R and the memory are still computed, but unused.
- **Legend:**
  - The Types view shows a 2-cell swatch before each category row in truecolor and 256-colour. With NO_COLOR it shows only the tag.
  - The help overlay adds two lines:
    - `Map: files are coloured by type (tag shows it, e.g. [V] video). Folders end in / and get their own colour,`
    - `never one used by a file type in the same view.`
- **Not used:**
  - hatching or pattern fills (they hurt label legibility);
  - hues outside Okabe-Ito (not validated as colour-blind safe);
  - same-hue alternate shades (rejected by the user as too similar);
  - pale tints of file-type hues for folders (they look related to the file type, which is the ambiguity being removed).

**Change log:**
- T025 used same-hue shades. The user found neighbours too similar.
- Three options were compared:
  - A: identity colours for every tile;
  - B: alternates taken from another category's hue (misleading);
  - C: the hybrid.
- The user chose C.

- **Color depth:** truecolor if `COLORTERM` is `truecolor` or `24bit`; otherwise the xterm-256 column of the §7 palette; otherwise monochrome.
- **ASCII fallback** (no Unicode or `LANG=C`): `◌`→`~`, `▲`→`^`, `⊘`→`-`, `≥`→`>=`, `▾▸`→`-+`, `█…`→`#`, double border→`#`, single border→`+-|`.
- **Motion:** spinner at no more than 10 fps; data redraws at no more than 10 Hz; map relayout at no more than 2 Hz (design §5). Nothing blinks.

### 7.1 Number formats (exact; one function each, tested)

**Compact size**, used in tree SIZE, map tile labels, and the Types and Largest columns:
- Units are base 1024: `B K M G T P E`.
- Below 1024 bytes: an integer followed by `B`, with no decimal. Examples: `0B`, `812B`, `1023B`.
- Otherwise: divide by 1024^k for the largest k where the value is ≥ 1, then print **always 1 decimal** followed by the unit letter, with no space. Examples: `1.0K`, `812.0K`, `64.2G`.
- If rounding to 1 decimal gives `1000.0` or more, move up one unit and recompute. So 1,023.96 KiB prints as `1.0M`, never `1024.0K`.
- The longest output is 6 cells (`999.9G`). With `≥` it is 7, which is the SIZE column width.
- Rounding is to nearest, as Rust's `{:.1}` does. Tests should avoid exact ties.

**Long size**, used in dialogs, status messages, the Done line and the Indeterminate line:
- The same number as the compact form, then a space and the IEC unit: `0 B`, `812 B`, `812.0 KiB`, `31.0 GiB`.
- A lower bound is `≥ 31.0 GiB`, with a space after `≥`.

**Count, exact**, used in dialogs, the Done line and messages:
- An integer with `,` thousands separators: `48,212`, `1,204,331`. The separator does not depend on locale.

**Count, compact**, used in the Indeterminate line (entries, queued, rate):
- Base 1000. Below 1000, an integer (`812`).
- Otherwise, 3 significant digits with a lowercase `k`, or uppercase `M`, `G`: `1.24k`, `12.4k`, `412k`, `1.24M`.
- If rounding reaches 1000, move up one unit: 999,960 prints as `1.00M`.

**Percent:**
- Tree % column: 1 decimal, width 5.
- Map tile labels: an integer followed by `%`. Use `<1%` when the value is above 0 and would round to 0.
- Never show `100%` on a tile unless it is the only child.

**Time:**
- Elapsed and ETA print as `MM:SS`, or `H:MM:SS` at 1 hour or more.
- An ETA always has `~` in front.

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
3. **Contrast:** for each of the 8 fills in the §7 palette, in both the truecolor and xterm-256 columns, `contrast(label, fill) ≥ 4.5`.
4. **ETA:**
   - It never shows a number while `Estimating`.
   - `estimating` never appears when there is no ETA source. In that case the Starting line has no `ETA` text at all.
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
7. **Refusals:** the scan root, an aggregate, `OTHER_FS`, and a second delete each produce the §5.3 text and no request. An `Aborted { MountPoint }` outcome opens the "Not deleted" dialog with the `MountPoint` text and `Nothing was deleted.`, and the model is unchanged.
8. **Quit:**
   - The first `q` or `Ctrl-C` never exits. With no delete running, `q q` exits.
   - With a delete running, `q q` and `Ctrl-C Ctrl-C` do not exit, the delete continues, and `y` exits with the stderr line.
9. **Zoom** (`⏎`) on a folder sets the `ScanControl` focus to that path. Zooming back up to the root clears it. `p` toggles it.
   - **`r` on a still-scanning, non-root folder** sets focus to it and starts no new jobs for it: the count of queued jobs for that path doesn't rise. It shows the §5.4 message. A second `r` leaves focus set.
   - **`r` on the root while scanning** changes nothing.
   - **`r` after Done or Stopped** starts a subtree rescan.
   - **`r` on a still-scanning folder that contains two outermost skipped folders and one nested skip:** both outer folders show `◌` and are scanned, the message says `resumed 2 skipped folders`, and after completion no `⊘` remains anywhere in that subtree.
   - **Keys after `s`, before `Done { cancelled }`:** the status line contains `Stopping…`. `r`, `p` and `x` produce the stopping message and change no `ScanControl` state. Once Stopped, `r` works.
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
15. **Number formats (§7.1)**, as unit tests over the formatting functions:
    - **Compact size:**

      | Input bytes | Output |
      |---|---|
      | `0` | `0B` |
      | `1023` | `1023B` |
      | `1024` | `1.0K` |
      | `831488` (812 KiB) | `812.0K` |
      | `1048535` (≈1023.96 KiB) | `1.0M` |
      | `68932140237` | `64.2G` |

      No output is longer than 6 cells for any `u64`.
    - **Long size:** `33285996544` → `31.0 GiB`, and `0` → `0 B`.
    - **Compact count:** `812` → `812`, `12400` → `12.4k`, `999960` → `1.00M`, `1240000` → `1.24M`.
    - **Exact count:** `1204331` → `1,204,331`.
16. **Selected state (§3.2.1, §3.3):**
    - Tree focused: every cell of the selected row (column 0 to the pane edge) has REVERSED and BOLD, and none has DIM.
    - Map focused: the selected row has BOLD, has UNDERLINED on the name cells, and has no REVERSED.
    - In both cases, every frame cell of the selected tile has REVERSED.
    - All of this holds identically with `NO_COLOR=1`.
17. **Tile colours (§7, option C):** unit tests on `assign`, plus a TestBackend check.
    - **(a) File tiles:** each file tile's fill equals its type hue.
    - **(b) No overlap with file types.** Scope folder with direct `.mp4` and `.zip` files and 6 subfolders: no folder tile uses orange or bluish green, in truecolor or 256-colour.
    - **(c) No touching tiles share a fill:**
      - 6 same-type sibling folders in one row;
      - a 7-tile grid.
      With 5 or more free hues, no two edge-sharing tiles have the same fill.
    - **(d) Stability:** 20 relayouts with changing sizes and unchanged adjacency recolour nothing. Adding a file of a new type to the scope folder recolours only the folders that used that hue, once.
    - **(e) Zoom:** zooming in and out keeps every folder's colour.
    - **(f) All 7 types present:** folder tiles are unfilled and have default bg.
    - **(g) Aggregate:** the aggregate tile is grey #BBBBBB / 250, and no other tile is.
    - **(h) Labels:** folder labels end in `/` and contain no `[T]` tag; file labels contain a tag.
    - **(i) Frames:** every frame cell has the default bg.
    - **(j) NO_COLOR:** no fills anywhere, and (h) still holds.
    - **(k) Determinism:** the same input and memory give the same output.
18. **`..` row:**
    - It is absent at the scan root. After `⏎` on a folder, row 0 is `..`, and the selection is on row 1 (the view root).
    - `⏎` on `..` gives the same state as `⌫`, and the selection becomes the folder you left.
    - `d`, `p`, `x` and `r` on `..` produce the §3.2.2 message, no `DeleteRequest`, and no `ScanControl` change.
    - With 60 children, `..` is not inside the aggregate.
    - With `..` selected, the map shows the same tiles as with the view root row selected, and no tile is highlighted.
