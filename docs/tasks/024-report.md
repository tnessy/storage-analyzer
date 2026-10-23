# T024 report (engineer-1)

**Status:** DONE

Files: `tests/write_guard.rs`, `tests/no_name_literal.rs`, `tests/support/src_tree.rs` (new).

## Failing first
New cases added to the T001 versions, run with `cargo test --test write_guard --test no_name_literal`: `newly_covered_forms_are_caught` failed ("not caught: std::fs::File::options()") and in `no_name_literal` `only_the_exact_definition_in_lib_rs_is_exempt`, `slashes_inside_a_string_do_not_hide_the_literal`, `block_comments_and_trailing_comments_pass` failed. Then refactored and hardened; the negative cases (`read_only_open_flags_pass`, `unrelated_write_copy_and_create_names_pass`) passed against both versions.

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` pass (52 unit, 14 delete_executor + 1 ignored, 14 model_apply, 7 no_name_literal, 11 write_guard); doc ok; release build ok.

## Decisions
- One shared `scrub(src, Keep|Blank)` in `support/src_tree.rs` (comments removed; literals kept or blanked; line breaks preserved). It replaces the two ad-hoc comment strippers, so the APP_ID check is now string-aware. Its own cases live in `write_guard.rs` (`scrub_blanks_literals_and_drops_comments`) so they run once.
- `every_forbidden_form_is_caught` now asserts the report names the expected construct, one case per form.
- `OFlags::WRONLY|RDWR|CREATE|TRUNC|APPEND` are flagged only in files that mention `OFlags` (also bare after an import).
- Bare `write(`: flags a `use std::fs::write` / `{…, write}` import, or `use std::fs::*` together with a bare `write(` call. An alias such as `use std::fs as f` is not seen (documented in the header).
- `copy`, `symlink`, `create_dir` etc. match as whole identifiers (so `Copy` and `copy_from_slice` pass); a local variable named `copy` would be reported.
- APP_ID exemption: only the line equal to `pub const APP_ID: &str = "<value>";` in `lib.rs`; the same line in another file fails.
- Header documents the `.write(` false positive in files that mention `OpenOptions`.

## Follow-up (2026-10-23): conflicts with engineer-2's UI code
`repo_sources_pass` started failing on `ui::copy` (the mandated copy module), `ui/*` `copy::…` paths, and a `std::fs::write` inside a `#[cfg(test)]` module in `src/app/mod.rs`. Fixes in `tests/write_guard.rs`:
- `copy` is no longer a bare identifier. Like `write`, it is matched as `fs::copy`, as an import from `std::fs`, or as a bare call after `use std::fs::*` (`COMMON_FS_WRITERS`).
- `#[cfg(test)] mod … { … }` blocks inside `src/` are skipped (they don't ship). Production code next to such a block is still checked. New tests cover both. **Lead: this narrows the brief's guard slightly; say if you want test modules checked too.**

## Round 2 (lead's review)
`strip_test_modules` now strips only `[pub] mod <ident> { … }`; `mod x;`, and `#[cfg(test)]` on fns/uses, are left checked. Red first: `a_file_module_marked_cfg_test_does_not_hide_the_next_item` failed; plus tests for braces ending the stripped module and for cfg(test) on a plain fn.
