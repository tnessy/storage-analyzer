# T023: Size toggle, help, too-small, NO_COLOR/ASCII/contrast, snapshot set

- **Engineer:** engineer-2. **Wave:** 2. **Depends on:** T017, T018 (T020 is recommended first, so that status hints exist). **Blocks:** T021, which reuses the theme variants.

## Goal
The remaining UX §6–§8 behaviour, and the scriptable acceptance checks in UX §11 that aren't owned elsewhere.

Acceptance criteria:
1. **`a`** toggles Disk usage ↔ Apparent size globally:
   - the header label changes;
   - the message is `Showing apparent size.` / `Showing disk usage.`;
   - the tree, map, Types and Largest all switch mode;
   - `--apparent` sets the initial mode.
2. **`?` help overlay:** every key, grouped as in UX §6, plus the hard-link attribution note from design §3.4. `Esc`/`?` close it.
3. **Too small** (< 80×24): only `Terminal too small: need 80×24, have WxH. Resize, or press q to quit.` is shown. Every key except `q`/`Ctrl-C` is ignored, and `q` opens the quit dialog. Resizing back restores the UI.
4. **Key hints by context** (UX §6) are dropped from the right when they don't fit, but `? help` is always kept.
5. **Colour:**
   - `NO_COLOR` set and non-empty → no fg/bg colour at all, while bold and reverse remain.
   - Truecolor when `COLORTERM` is `truecolor` or `24bit`; otherwise the nearest xterm-256 colour, with the black/white label chosen by contrast; otherwise monochrome. Detection is injectable for tests.
6. **ASCII fallback** when the locale isn't UTF-8 (`LC_ALL`/`LC_CTYPE`/`LANG` lacks `UTF-8`, or is `C`/`POSIX`): the `Glyphs` ASCII variant from UX §7 (`◌`→`~`, `▲`→`^`, `⊘`→`-`, `≥`→`>=`, `▾▸`→`-+`, bars→`#`, borders).
7. **Spinner:** a frame replaces `◌` in the status line at ≤ 10 fps. `--no-animation` freezes it.
8. **Deferred:** the ≥ 120-column Types side panel is **not** built. At 160×40 the Tree layout has no side panel. Assert this so nobody adds it by accident.

## Design references
§3.3 (`--no-animation`), §3.4 (hard-link attribution note), §3.9. UX §3 (too small, wide), §6, §7, §8, §11 checks 1–3, 5, 12.

## Files owned
`src/ui/theme.rs`, `src/ui/help.rs`, `src/ui/too_small.rs`, `src/ui/hints.rs`, `src/ui/header.rs`, `src/app/keymap.rs` (`a`, `?`), `src/app/state.rs`, `src/ui/copy.rs`, `tests/ui_accessibility.rs`, `tests/ui_snapshots.rs`.

## Tests to write first
- **UX §11 check 1:** 80×24 snapshots for the §3.1 scanning screen, Done, Stopped, an empty folder, each view, and too-small (79×24). Each is rendered with `NO_COLOR=1` and with ASCII. Dialog snapshots belong to T021.
- **UX §11 check 2:** with NO_COLOR, no cell has a non-default fg/bg. A diff of the colour and NO_COLOR buffers shows each §5.1 state is still distinguishable by glyph or suffix.
- **UX §11 check 3:** `contrast(label, fill) ≥ 4.5` for every §7 fill **and** for every 256-colour mapping. Use the WCAG relative-luminance formula. If a 256-colour mapping fails, report it to Heimdall (it is the UX open question) rather than changing the palette.
- **UX §11 check 12:** the key hints at 80 columns always contain `? help`, in every context.
- **`a` toggle:** the sizes in the tree, map and Largest switch, the header label switches, and the Largest order can change.
- **Too small:** at 64×20 only the message renders. `d`, `x` and `↓` do nothing. `q` opens the quit dialog.
- **No side panel:** a 160×40 Tree render has no Types panel.
- **Spinner:** with `--no-animation`, the status glyph is identical across 20 frames. Without it, it changes at ≤ 10 fps.

Commands: `cargo test --test ui_accessibility --test ui_snapshots`, and `NO_COLOR=1 cargo run -- ~` for a visual check in your terminal (this covers R7 informally; note the result in your hand-off).

## Out of scope
The Types side panel. Dialog snapshots (T021).

## Note from T025
Include the 8 **alternate** tile shades (`ux/spec.md` §7) in the 256-colour mapping and in the contrast check. The ASCII fallback must cover the tree guide glyphs and the selected-tile `>` marker. Also confirm with UX the STATE-to-guide spacing nit from the T016 review.

## Note from T026
The alternate shades are gone (T026). Colour depth uses the §7 option-C palette's xterm-256 column, which T026 adds to `theme.rs`. The help overlay renders T026's two map-colour lines from `copy.rs`. The Types swatch must show in 256-colour mode, and be hidden under NO_COLOR.

## Round 2 (2026-10-23): required before T023 is committed
T023 is not committed yet. These items complete it, so it lands once, correct. Tests first; for code written before its tests, give one targeted breakage per criterion (README rule).

### R2-1. 256-colour labels (from `023-review.md` item 1)
In `ColorDepth::Indexed256`, label colours must be indexed or ANSI, never `Color::Rgb` (for example `Color::Indexed(16)` / `Color::Indexed(231)`, or `Color::Black` / `Color::White`).
- Re-check check 3's 256-colour contrast with the label colours actually used.
- **Test:** render a scanning screen in `Indexed256`, and assert that no cell has an RGB fg or bg.

### R2-2. Help overlay: two columns, no scrolling (UX §6, **replaces** `023-review.md` item 2, the scrolling)
- **Layout:** 80×24, centred, inner width 76, two 37-cell columns with a 2-cell gap. Each entry has a 10-cell key column and a description of ≤ 26 cells.
  - "Move and view" on the left, "Act and scan" on the right.
  - A 4-line full-width footer with the map-colour note and the ETA/`--history` note, **exact wording in UX §6**.
  - 18 rows in total, with the bottom border title `Esc or ? close`.
  - The footer replaces the earlier map-colour and ETA constants from T026. Delete those constants; don't leave them unused.
- **ASCII key column:** `Up Dn j k`, `Right l`, `Left h`, `Enter`, `Bksp -`; borders `#`; descriptions unchanged.
- **Keys:** `Esc` and `?` close; `q`/`Ctrl-C` open the quit dialog; every other key is ignored.

### R2-3. ASCII key names and hint strings (UX §6, §7)
- **Hint strings:** exactly the §6 table, in Unicode and ASCII. The drop rule removes whole groups from the right and never drops `? help`.
  - At 80 columns in ASCII, the Tree line drops `x skip` and ends `p first  ? help` (76 cells).
- **ASCII mode, everywhere** (hints, messages, dialogs, map title): `⏎` → `Enter` and `⌫` → `Bksp` as **whole words**. For example, `Press Enter to go up.` and ` · Enter goes up to /home/nss`.
  - Arrows are **never** folded character by character; contexts that name arrows use the letter-key wording.
  - Single letters such as `E` or `B` standing for keys are forbidden.
- **More fallbacks:** `…` → `...`, `·` → `-`, `›` → `>`.
- **Done and Stopped lines:** `✓` and `■` are dropped and the word stays (`Done in 03:51`, `Stopped at 01:12`).

### Tests (check 19, plus R2-1)
- **(a)** At 80 columns, each context's hint line equals the §6 string exactly, in both modes. ASCII Tree ends `p first  ? help`.
- **(b)** In ASCII mode, no rendered buffer matches the regex `(^| )[EB] (zoom|up|show)`. Run it over the Tree, Map, Lists, the `..`-row message and the map title.
- **(c)** At 80×24 the help overlay is fully drawn: the bottom border with `Esc or ? close` is within row 24, and all 18 entries appear, in Unicode and ASCII.
- **(d)** In help, `Esc` and `?` close it, `q` opens the quit dialog, and `j` changes nothing.
- **(e)** In ASCII mode, the Done line contains `Done in` and no `✓`; the Stopped line contains `Stopped at` and no `■`.
- **R2-1:** the Indexed256 test above.

**Files:** the same as T023, plus `src/ui/ascii.rs`, `src/ui/help.rs` and `src/ui/hints.rs` (new in T023), and `tests/ui_accessibility.rs` / `tests/ui_snapshots.rs`. Regenerate and review the affected snapshots.
