# T023 report (engineer-2)

Status: DONE_WITH_CONCERNS

Files: `src/ui/{ascii,help,hints,too_small}.rs` (new), `src/ui/{theme,copy,mod,header,dialog}.rs`, `src/app/{keymap,state,mod}.rs`, tests `tests/ui_accessibility.rs` (15), `tests/ui_snapshots.rs` (1 test, 20 snapshots), `tests/ui_treemap.rs` (four small-screen map tests now draw the map widgets alone, see below).

## Evidence (tests first)
- Against stubs (detection returning fixed values, empty help, `is_too_small` false, no ASCII glyph set, `a` and `?` ignored) 11 of the 15 accessibility tests failed on behaviour; then implemented.
- Tests that passed on the stubs (NO_COLOR cells, contrast, hints keep `? help`, wide layout) got one targeted breakage each, all failing as expected and reverted: Mono theme still returns fills → `without_colour_no_cell_…` fails; hints drop `? help` → `key_hints_always_keep_help…` fails; ASCII fold disabled → `the_ascii_fallback_draws_every_view…` fails. The wide-layout test (no side panel) cannot be broken without building the panel, so it is an assertion only.
- Process slip, no effect: while reverting a breakage I typed `git checkout -q` with no paths. It changes nothing (the working tree and the test results are unchanged), but it is a command on the forbidden list.
- Fresh: fmt, full clippy, `cargo test --all-targets` (accessibility 15, snapshots 1, treemap 37, app_eta 22, rest unchanged), doc tests, release build all pass.

## Behaviour
- `a` toggles the size mode globally (header label, tree, map, Types, Largest) with `Showing apparent size.` / `Showing disk usage.`; `--apparent` sets the start mode.
- `?` opens a help overlay (`Dialog::Help`, wider than other dialogs, `ui::help::lines`): the UX §6 key table, the ETA note, the hard-link note, and T026's two map-colour lines; `Esc` or `?` closes it, other keys are ignored.
- Below 80×24 only `Terminal too small: need 80×24, have WxH. Resize, or press q to quit.` is drawn (it wraps below 71 columns) and every key except `q`/`Ctrl-C` is ignored; `q` still opens the quit dialog over it; resizing restores the UI.
- `ColorDepth::detect` (NO_COLOR non-empty → none, `COLORTERM` truecolor/24bit → truecolor, else xterm-256 unless `TERM` is `dumb`/unset → none) and `Glyphs::detect` (first non-empty of `LC_ALL`, `LC_CTYPE`, `LANG` must name UTF-8) run once in `run_tui`.
- ASCII fallback: `Glyphs::ascii()` per UX §7 (`~ ^ - >= - + # . ...`, ASCII tree guides and spinner), plus `ui::ascii::fold`, a last pass over the drawn buffer that turns leftover box-drawing, arrows, `…`, `·`, `⏎`, `⌫` into ASCII, so no non-ASCII cell can reach a non-UTF-8 terminal.
- Key hints moved to `ui/hints.rs` unchanged; `? help` is kept in every context.

## Concerns
- ASCII folds `⏎` to `E`, `⌫` to `B`, arrows to `^ v < >`, so ASCII hints read `E zoom  B up`. UX should supply real ASCII hint wording if that matters.
- The help overlay is ~30 lines and is clipped at 24 rows (no scrolling); at 80×24 the notes at the bottom are cut off.
- NO_COLOR/ASCII are detected only at start-up; there is no flag or config override yet (UX §8 mentions one).
- The four T017 tests that used 30×14, 40×12 and 80×12 screens now draw only the map title and map, because below 80×24 the full screen shows just the message.
- `NO_COLOR=1 cargo run -- ~` for R7 was not possible here (no tty). The 256-colour pairs are covered by the contrast test; the visual check is still open.
- T020 is not started: it needs T012's model controls (`set_focus`, `take_focus_completed`, `mark_skipped`, `begin_rescan` are still `todo!("T012")`, and `Model::rescan_plan` does not exist).

## Review fixes (tests first)
Written first and seen failing against the T023 code (7 failures): `in_256_colour_mode_no_cell_uses_a_24_bit_colour`, `the_help_overlay_fits_80_by_24_in_two_columns_in_unicode_and_ascii`, `help_keys_esc_and_question_mark_close_q_opens_the_quit_prompt_and_others_are_ignored`, `hint_lines_equal_the_ux_strings_at_80_columns_in_unicode_and_ascii`, `ascii_mode_never_uses_single_letters_as_key_names`, `ascii_copy_spells_out_enter_and_backspace_and_three_dots`, `ascii_done_and_stopped_lines_drop_the_symbol_and_keep_the_word`. All pass now.
1. 256-colour mode: labels are `Color::Indexed(16)`/`Indexed(231)`; no cell carries a `Color::Rgb` (the existing contrast test uses the swatch labels actually emitted).
2. Help is UX's two-column, no-scroll overlay (18 rows at 80×24, `Move and view` / `Act and scan`, 4-line footer, `Esc or ? close` on the bottom border). ASCII key column `Up Dn j k`, `Right l`, `Left h`, `Enter`, `Bksp -`. `Esc`/`?` close; `q`/`Ctrl-C` open the quit prompt; other keys are ignored. The scrolling idea is gone.
3. Exact hint strings per context in Unicode and ASCII (`ui/hints.rs`, `copy.rs`); the ASCII tree line drops `x skip` at 80 columns. `ascii::text` spells `⏎`/`⌫` as `Enter`/`Bksp` and `…`/`·` as `...`/`-` in dialogs, the status line, the map title and empty messages; Done and Stopped lose their symbol in ASCII. The old single-character fold stays only as a safety net (`?` for anything unknown).
4. Snapshots regenerated, plus `help_unicode` / `help_ascii` (22 in all).

Note for UX/lead: UX's help footer dropped the hard-link attribution note that design §3.4 and the T023 brief asked for in the help; I followed UX's 18-row layout, so it is not in the help now. There are 6 spare rows at 24 if it should come back.
