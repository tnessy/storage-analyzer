# T023 review (Heimdall, 2026-10-23): CHANGES REQUESTED (two items)

**Fresh evidence** (clean worktree: HEAD plus the T023 files; engineer-1's in-progress `src/model/mod.rs` and `tests/model_controls.rs` were excluded):
- fmt ok, whole-tree clippy clean;
- every suite passes, twice: lib 104, ui_accessibility 15, ui_snapshots, …;
- release build ok.

**Red first:** 11 of 15 were red against stubs. For the other 4, one targeted breakage each is reported, which meets the README rule.

**Real terminal** (by me, release binary under a pty at 110×30, on `/usr/share/doc`; keys `? Esc a 2 1 q y`):

| Mode | Result |
|---|---|
| `NO_COLOR=1` | **0** colour SGRs in the whole session ✓. The help overlay showed, and `Showing apparent size.` appeared ✓ |
| `LANG=C LC_ALL=C` | **no non-ASCII character** anywhere in the output ✓ |
| `TERM=xterm-256color`, no `COLORTERM` (clean env) | fills are `48;5;{221,35,74,178,166,175,250}`, matching the §7 xterm-256 column ✓ |
| same, labels | **every label is still `38;2;0;0;0`** ✗ (item 1) |

(R7 informal visual check: done by me in the pty for these three modes.)

**Required changes:**
1. **The 256-colour fallback still emits 24-bit label colours.** `theme.rs` `BLACK`/`WHITE` are `Color::Rgb`, so `Swatch.label` is RGB even in `Indexed256`. A terminal without truecolor may draw those wrong, and that is the case the fallback exists for (UX §7: "the nearest xterm-256 colour with black or white label text").
   - **Fix:** in `Indexed256`, use indexed or ANSI black and white (for example `Color::Indexed(16)` / `Color::Indexed(231)`, or `Color::Black` / `Color::White`). Recheck check 3's 256-colour contrast with the colours actually used.
   - **Test, red first:** render a scanning screen with `ColorDepth::Indexed256`, and assert that **no cell** has any `Color::Rgb` fg or bg.
2. **The help overlay clips at the 80×24 floor** (about 30 lines in 22 rows), so some keys can't be read at the supported minimum (UX §8: everything reachable by keyboard; §6: help lists every key).
   - Make the overlay **scrollable** with `↑↓ j k PgUp PgDn g G`, while it stays open. Show a scroll position cue when it overflows, for example a last-line hint, with the wording from UX.
   - **Test:** at 80×24, scrolling down reaches the last help line, and the first and last lines are each visible at some scroll position.

   Heimdall has asked UX for the cue wording via the team lead. Until it arrives, use `↑↓ scroll · Esc close` from `copy.rs`.

**Not blocking:**
- **ASCII key hints read `E zoom  B up`** (⏎→E, ⌫→B). They work, but the wording needs UX's word: probably `Enter zoom  Bksp up`, which costs width at 80 columns. Asked via the team lead. Implement UX's answer as a one-line copy change when it comes; it doesn't block this commit.
- **Detection only at start-up, no override flag:** accepted. UX §8 says a flag "can" override NO_COLOR, which is optional and not in the T023 brief. It is a candidate for the backlog; the project owner decides.
- **The four T017 map tests now render only the map widgets:** accepted (isolation from header/status changes).
- **Process:** engineer-2 disclosed running `git checkout -q` with no paths. That form changes nothing, but it is the forbidden command family in README rule 5. Thanks for disclosing it; please don't use it again.

T023's other criteria are met:
- the `a` toggle (with `--apparent`);
- `?` help, with the ETA note, the hard-link note and the two map-colour lines;
- the too-small screen (only `q`/Ctrl-C);
- key hints that always keep `? help`;
- NO_COLOR gives no fg/bg anywhere;
- the ASCII glyph set and fold;
- 20 snapshots, Unicode and ASCII;
- the spinner, frozen with `--no-animation`;
- no Types side panel at ≥ 120 columns.

## Update 2026-10-23: UX resolved the open points
UX answered both open points (`ux/spec.md` §6, §7, check 19). **Item 2 changes:** the help overlay becomes a fixed two-column layout that fits 80×24, with **no scrolling**, so ignore the scrolling request. Item 1 (256-colour labels) stands. The ASCII hint wording is settled (letter keys, `Enter`/`Bksp`). All of it is collected as **"Round 2"** in `023-ui-polish-accessibility.md`. T023 will be committed once, after round 2.
