# UX directions: storage-analyzer (TUI)

Status: **decided, 2026-10-23. Layout A chosen by the user** (brief v0.5 Decisions). B and C are archived below for the record. The active spec is [`spec.md`](spec.md). It replaces the shared-behaviour sections this file used to contain.
Date: 2026-10-23. Source: `docs/brief.md` v0.3. Author: Freya (UX).

Labels: **Fact** (from the brief or a cited source), **Assumption**, **Open question**.

## 0. How the design language was chosen

1. Project's own system: none. The repo holds only `docs/brief.md`, so there is no code, tokens or TUI style to follow.
2. Framework conventions: none yet. The stack is undecided (product-architect owns that).
3. So I explored. The three candidates below each borrow an existing terminal design language, chosen to fit this product:
   - **A. Classic split.** Curses or ncdu/htop conventions laid out like WinDirStat. Full-width rows, reverse-video selection, column headers, a key-hint footer.
   - **B. Panel workspace.** The lazygit/btop style: titled, bordered panels, number keys to jump between panels, and key hints that change with focus.
   - **C. Map first.** Filelight/baobab put the chart at the centre. Here the treemap fills the screen, chrome is kept minimal in the spirit of clig.dev, and the tree opens on demand.

All mockups are drawn at **80×24**, the floor this tool must support (**Assumption**: the user's real terminal is often larger). They show what the screen looks like with **NO_COLOR** set: every meaning is carried by text, glyphs or borders, and color only adds to it (see `spec.md` §8). All three mockups use the same data and the same moment: a scan of `/home/nss` in progress, with `.cache/` selected and expanded. Numbers are illustrative.

**Command name:** to be decided. `<cmd>` is the placeholder, matching `docs/design.md`.

Glyphs used in every candidate (ASCII fallback in brackets, for terminals or fonts without Unicode):

| Meaning | Glyph | Fallback |
|---|---|---|
| Selected row or tile | `>` plus reverse video; the selected tile gets a double border `╔═╗` | `>` plus `#` border |
| Still scanning (size will grow) | `◌` | `~` |
| Prioritised by the user / scan skipped by the user | `▲` / `⊘` | `^` / `-` |
| Lower bound (size incomplete) | `≥` | `>=` |
| Has skipped or errored paths | `!` | `!` |
| Expanded / collapsed directory | `▾` / `▸` | `-` / `+` |
| Size bar (percent of parent) | `█▉▊▋▌▍▎▏` (eighth-cell steps) | `#` |
| File-type category tag in tiles | `[V]` video, `[I]` image, `[A]` archive or package, `[C]` code or text, `[B]` binary or library, `[D]` documents, `[O]` other | same |

---

## 1. Candidate A (chosen): Classic split (tree on top, treemap below)

**Direction.** This is the closest translation of WinDirStat, which the user named as the reference. WinDirStat puts the directory list top-left, the extension list top-right and the treemap below ([ghacks](https://www.ghacks.net/?p=246), [computerhope](https://computerhope.com/tips/tip225.htm)). Here the tree gets the full width, so long names and the size, percent and bar columns stay readable. The treemap sits underneath as a wide strip. Wide strips suit labels, because terminal labels run horizontally. The visual grammar is ncdu's (size column, percent-of-parent bar, `d` to delete), so the user already knows how to read it.

### A1. Main screen, scanning
```
 <cmd>  /home/nss                           Disk usage   ? help   q quit
 [1 Tree]  2 Types  3 Largest  4 Skipped
    SIZE      %  OF PARENT      NAME
  182.4G  100.0  ██████████  ◌  /home/nss
> 64.2G    35.2  ███▌        ◌  ├─▾ .cache/
   31.0G   48.3  ████▊          │  ├── yay/
   14.2G   22.1  ██▏            │  ├── mozilla/
    8.8G   13.7  █▎          ◌  │  ├── pip/
   10.2G   15.9  █▌             │  └── (12 smaller, 10.2G)
   41.8G   22.9  ██▎         ◌  ├─▸ Videos/
   28.1G   15.4  █▌             ├─▸ .local/
   19.6G   10.7  █              ├─▸ Projects/
   12.3G    6.7  ▋           !  ├─▸ Downloads/                            ↓ 4
 Map: /home/nss ───────────────────────────────────────────────────────────────
╔ .cache/ 64.2G 35% [A] ══╗┌ Videos/ 41.8G 23% [V]┐┌ .local/ 28.1G 15%[B] ──┐
║                         ║│                      ││                        │
║                         ║│                      │└────────────────────────┘
║                         ║│                      │┌ Projects/ 19.6G ┐┌ Dow…┐
║                         ║│                      ││ 11% [C]         ││ 7%  │
╚═════════════════════════╝└──────────────────────┘└─────────────────┘└─────┘
 ◌ Scanning  [██████▋░░░░░░░░░] ~41%   01:12 elapsed   ETA ~01:44 (estimate)
 ↑↓ move  ←→ fold  ⏎ zoom  ⌫ up  Tab map  d delete  a apparent  s stop  ! 3
```

### A2. Wide terminal (≥120 columns, fragment, truncated)
A Types panel opens to the right of the tree, which is where WinDirStat puts its extension list. Largest and Skipped stay as tabs.
```
 [1 Tree] ...............................................│ Types (whole root)
  182.4G 100.0 ██████████ ◌ /home/nss                    │  [V] video   58.1G 31%
> 64.2G   35.2 ███▌       ◌ ├─▾ .cache/                  │  [A] archive 40.3G 22%
 ...                                                     │  [B] binary  33.0G 18%
```

**What it feels like.** Familiar and calm. It reads like ncdu with WinDirStat's treemap underneath.

---

## 2. Candidate B (archived, not chosen): Panel workspace (tree left, treemap right)

**Direction.** This treats the tool as a workspace of panels, like lazygit (number keys jump to a panel and Tab cycles; [computingforgeeks](https://computingforgeeks.com/?p=3280)) or btop. The tree and the treemap sit side by side at full height. A terminal cell is about twice as tall as it is wide (**Assumption**: typical monospace fonts), so a 41×20-cell region is roughly square on screen. That gives the treemap better-shaped tiles than A's strip. The cost is a narrow tree: names truncate sooner at 80 columns.

### B1. Main screen, scanning
```
╭[1]*Tree─/home/nss────────────────╮╭[2]Map─/home/nss──────────────────────────╮
│ 182.4G 100% ◌ /home/nss          │╔ .cache/ 64.2G ════╗┌ Videos/ 41.8G ─────┐│
│> 64.2G  35% ◌ ├▾ .cache/         │║ 35% [A]           ║│ 23% [V]            ││
│  31.0G  48%   │ ├ yay/           │║                   ║│                    ││
│  14.2G  22%   │ ├ mozilla/       │║                   ║│                    ││
│   8.8G  14% ◌ │ ├ pip/           │║                   ║│                    ││
│  10.2G  16%   │ └ (12 smaller)   │║                   ║│                    ││
│  41.8G  23% ◌ ├▸ Videos/         │║                   ║│                    ││
│  28.1G  15%   ├▸ .local/         │║                   ║└────────────────────┘│
│  19.6G  11%   ├▸ Projects/       │║                   ║┌ .local/ 28.1G ─────┐│
│  12.3G   7% ! ├▸ Downloads/      │║                   ║│ 15% [B]            ││
│   9.9G   5%   ├▸ .steam/         │║                   ║└────────────────────┘│
│   3.1G   2%   ├▸ Documents/      │║                   ║┌ Projects/ ─┐┌ Dow…─┐│
│   3.4G   2%   └ (34 smaller)     │║                   ║│ 19.6G [C]  ││ 7%   ││
│                                  │╚═══════════════════╝└────────────┘└──────┘│
╰──────────────────────────────────╯╰──────────────────────────────────────────╯
╭[3]Types [4]Largest [5]Skipped !3─────────────────────────────────────────────╮
│ (collapsed: press 3, 4 or 5 to open over the map)                            │
╰──────────────────────────────────────────────────────────────────────────────╯
 ◌ Scanning [██████▋░░░░░░░░░] ~41%  01:12 elapsed  ETA ~01:44 (estimate)
 ↑↓ move ←→ fold ⏎ zoom ⌫ up Tab/1-5 panel d delete a apparent s stop ? help
```
The focused panel shows `*` after its number (`[1]*Tree`) and is drawn bold, so focus does not depend on color.

**What it feels like.** A cockpit: everything is one keypress away. It is busier, and names are tighter at 80 columns.

---

## 3. Candidate C (archived, not chosen): Map first (treemap full screen, tree on demand)

**Direction.** Here the treemap is the main surface, the way the ring chart is in filelight and baobab, which the user called the closest existing tools. The screen shows a breadcrumb, a large treemap, and a short ranked list of the current level underneath. The full expandable tree (M3) opens over the map when you press `t`. Chrome is kept to a minimum. This is the strongest "see where the space goes at a glance" option, but it is the furthest from WinDirStat's always-visible tree.

### C1. Main screen, scanning
```
 /home/nss ›                                     Disk usage · 182.4G so far ◌
╔ .cache/ 64.2G 35% [A] ═════╗┌ Videos/ 41.8G 23% [V] ┐┌ .local/ 28.1G 15% [B]┐
║                            ║│                       ││                      │
║  yay/ 31.0G                ║│                       ││                      │
║  mozilla/ 14.2G            ║│                       ││                      │
║  pip/ 8.8G ◌               ║│                       │└──────────────────────┘
║                            ║│                       │┌ Projects/ 19.6G [C] ┐
║                            ║│                       ││ 11%                  │
║                            ║│                       ││                      │
║                            ║│                       │└──────────────────────┘
║                            ║│                       │┌ Downloads/ !┐┌.st…┐┌┐
║                            ║│                       ││ 12.3G 7%    ││5%  │││
╚════════════════════════════╝└───────────────────────┘└─────────────┘└────┘└┘
 > 64.2G 35.2% ███▌  ◌ .cache/        41.8G 22.9% ██▎ ◌ Videos/
   28.1G 15.4% █▌      .local/        19.6G 10.7% █     Projects/
   12.3G  6.7% ▋     ! Downloads/      9.9G  5.4% ▌     .steam/  (+36 more)
 ◌ Scanning [██████▋░░░░░░░░░] ~41%  01:12 elapsed  ETA ~01:44 (estimate)
 ←↑↓→ move  ⏎ zoom  ⌫ up  t tree  y types  b largest  d delete  ? help
```
In C the selected tile previews its own children as text inside the tile (depth 2), for tiles large enough to fit them.

**What it feels like.** Visual and immediate. Knowing your exact place in the hierarchy takes an extra key.

---

## 4. Comparison

### 4.1 Rubric (from brief goals G1–G3 and M1–M7)

| # | Criterion (observable) | Why (brief) | A Classic split | B Panels | C Map first |
|---|---|---|---|---|---|
| R1 | Tree and treemap both visible at 80×24 without extra keys | Problem statement, G2 | Yes | Yes | No (tree is an overlay on `t`) |
| R2 | Treemap cells at 80×24 (approx.) | M4 | ~78×6 = ~470 | ~41×14 = ~570 | ~80×12 = ~960 |
| R3 | Tiles with a full label (name, size, %) at 80×24, sample data | M4 labels | 4 | 4 | 5 |
| R4 | Name characters available in the tree at 80 cols, depth 2 | M3 | ~40 | ~16 | n/a (overlay ~50) |
| R5 | Keys to reach Types / Largest from the main screen | M5, M6 | 1 / 1 | 1 / 1 | 1 / 1 |
| R6 | Keys from a selected item to the delete confirmation | M7 | 1 (`d`) | 1 | 1 |
| R7 | Patterns the user already knows | Fact: user knows ncdu, dust, WinDirStat | ncdu + WinDirStat | lazygit/btop (**Assumption**: may be unfamiliar) | filelight/baobab |
| R8 | Every state readable with NO_COLOR (see `spec.md` §11) | Accessibility | Yes | Yes | Yes |
| R9 | Relative build effort (**Assumption**, to be confirmed with the architect) | Constraint: v1 | Low–medium | Medium (panel focus, narrow truncation) | Medium–high (spatial navigation is the main mode, plus the overlay) |
| R10 | Treemap tile shape (closer to square is easier to compare) | M4 | Flat strips | Near square | Good |

### 4.2 Strengths, weaknesses, risks

| | A Classic split | B Panels | C Map first |
|---|---|---|---|
| Strengths | Closest to WinDirStat. Full-width tree with readable columns. Easy to learn. Grows a Types panel on wide terminals. | Best treemap shape at 80×24. Everything one key away. Scales well on wide terminals. | Largest map. Fastest "what's big?" scan. Depth-2 preview in tiles. |
| Weaknesses | Treemap is short at 24 rows: small tiles merge into "smaller" quickly. | Tree names truncate hard at 80 cols. More borders means more visual noise. | Fails R1. Tree context is hidden. Spatial arrow-key navigation is less predictable than list navigation. |
| Accessibility | Linear row order makes reading top to bottom simple. | Focus must be clear across 3 panels (needs the `*` marker). | Spatial focus is harder to track. The bottom list provides a linear fallback. |
| Risk | Treemap feels like an add-on at small sizes. | Feels cramped at 80 cols. | User may feel it is "not WinDirStat". |

### 4.3 Recommendation

**Recommend A (Classic split)**, with the Types side panel at ≥120 columns. A is the only candidate that keeps both R1 (tree and map visible together) and R4 (readable names) at 80×24. It matches the reference the user named, and it is the cheapest to build. Its weak point, a short treemap, only bites at small terminal sizes. The treemap gets more rows as the terminal gets taller (it takes 40% of the body height).

What would change this:
- If the user usually runs a terminal ≥160 columns wide and wants the map to dominate, choose **B**.
- If the user says the treemap is the main tool and the tree is secondary, choose **C**.
- If "WinDirStat-like" means a switchable layout, A can add `L` to toggle to B's arrangement later. I don't recommend that for v1, because it doubles layout testing.

---

## 5. Outcome and change log

- **Chosen:** A (Classic split). The final spec is [`spec.md`](spec.md): layout, keys, states, dialogs, copy, accessibility and acceptance checks.
- **User feedback folded into brief v0.5 and design rev 3, and from there into the spec:**
  - Delete of an unfinished folder is **allowed**. The confirmation shows `≥` lower bounds, a "scan of this folder not finished" title, and the 3 largest known items. `p` in the dialog finishes that folder's scan first, and the dialog then updates in place. This replaces the earlier "blocked" rule and the earlier `f` key.
  - Zooming (`⏎`) and `p` both prioritise a folder's scan. `x` skips a folder's scan and `r` resumes it. The scan runs shallow-first.
  - `--max-depth` was **rejected**, because it undercounts and breaks M8 (ADR-009 O5). It has been removed.
  - Quit always needs confirmation. During a delete, only `y` quits.
  - The command name is a placeholder: `<cmd>`.
- The accessibility rationale and contrast measurements from the exploration (palette, NO_COLOR, glyph fallbacks) carried over into `spec.md` §7–§8.

Sources: [ghacks: WinDirStat](https://www.ghacks.net/?p=246), [computerhope: WinDirStat](https://computerhope.com/tips/tip225.htm), [neowin: WinDirStat 2.7.0 layout chooser](https://www.neowin.net/software/windirstat-270/), [ncdu(1)](https://man.archlinux.org/man/ncdu.1), [computingforgeeks: lazygit](https://computingforgeeks.com/?p=3280), [Okabe-Ito palette, NYU Siegal lab](https://siegal.bio.nyu.edu/color-palette/), [no-color.org](https://www.no-color.org), [clig.dev](https://clig.dev).
