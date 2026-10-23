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
