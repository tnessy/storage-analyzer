# T023b: Hard-link note in the help footer

- **Engineer:** engineer-2. **Depends on:** T023 (merged, `012f639`). **Schedule:** now, while T020 waits on T012.
- **Source:** UX restored the hard-link attribution note that design §3.4 requires in the UI help (`ux/spec.md` §6 help mockup and "Hard-link footer"; §11 check 19(c)).

## Acceptance criteria
1. The help overlay footer gains the two lines from UX §6, **exactly**:
   - `Hard links count once, in the folder scanned first: totals match du, folder`
   - `sizes may not. Deleting one name of a hard-linked file frees no space.`
2. The overlay is now **20 rows** and still fits 80×24, with `Esc or ? close` on the bottom border, in Unicode and ASCII mode. Every line is the width UX specifies.
3. The delete dialog's own hard-link line (§5.3) is unchanged. It is T021's.

## Files owned
`src/ui/help.rs`, `src/ui/copy.rs` (only the two new constants), `tests/ui_accessibility.rs` (the help tests), and the help snapshots `tests/snapshots/ui_snapshots__help_*.snap`.

## Tests to write first (show them failing)
- **Check 19(c), extended:** at 80×24 the overlay is fully drawn, contains all 18 entries **and** `Hard links count once`, and its bottom border is within row 24, in Unicode and ASCII mode.
- **Regenerate the two help snapshots** and review the diff: only the two new footer lines and the moved bottom border.

**Commands:** the five standard ones, plus `cargo insta review`.

## Out of scope
Any other help or key change. T020's work.
