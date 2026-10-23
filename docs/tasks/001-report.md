# T001 report (engineer-1)

**Status:** DONE

## Files
Cargo.toml, Cargo.lock, rustfmt.toml, .gitignore, src/{lib,main,types}.rs, src/scan/{mod,msg,control,fs}.rs, src/model/mod.rs, src/delete/{mod,msg}.rs, src/eta/{mod,contract}.rs, src/layout/{mod,contract}.rs, src/{probe,history,app,ui}/mod.rs, tests/{write_guard,no_name_literal}.rs. No `clippy.toml` (not needed).

## Failing first
- `cargo test --lib`: 6 `scan::control::tests::*` failed with `not yet implemented` (todo! stubs).
- `cargo test --test no_name_literal`: `planted_literal_fails` and `second_definition_like_line_still_fails` failed, left 0 / right 1 (scanner stub returned nothing).
- `cargo test --test write_guard`: `planted_violation_outside_allowed_modules_fails`, `every_forbidden_form_is_caught`, `comment_marker_inside_string_does_not_hide_a_violation` failed (scanner stub).
- Not red first: `Flags` tests and `default_threads` test (trivial code written with its test).

## Fresh results
`cargo fmt --check` ok; `clippy --all-targets --all-features -D warnings` clean; `cargo test --all-targets` 9 unit + 4 + 7 integration pass; `cargo test --doc` ok (0 tests); `cargo build --release` ok.

## Decisions / deviations
- `ScanControl` fields are private. Added `is_cancel_all()` (workers need it) and `new()`. `is_cancelled` is prefix-only; it ignores `cancel_all`.
- `ProbeMsg` lives in `src/probe/mod.rs`; `Busy` and `DeleteCheck`/`Flags`/`ExtKey` etc. live in `delete/mod.rs` and `model/mod.rs`.
- `Flags` is a hand-written `u16` bitset (no extra dependency), including `REMOVED`.
- `ScanOptions::default_threads` is implemented (trivial), not a stub.
- `ScanHandle`, `DeleteHandle`, `Model`, `RealFs` are shells with a private `_private: ()` field (or unit struct); later tasks replace the fields.
- Stubs use `todo!("T0NN")`. `FsAccess` has no impl yet (RealFs is a bare type, per brief).
- write_guard scanner blanks comments, strings and char literals before matching, so `"http://"` can't hide a violation. Word-matching is on identifiers, so `rename` doesn't match `rename_x`. Limitation: it is lexical, not a parser; macros generating forbidden calls are not seen.
- no_name_literal ignores `//` comment text and the `const APP_ID` line; it flags any non-comment occurrence of the value (even inside a longer string). A `//` inside a string literal truncates that line's check.
- Edition 2024, `rust-version = "1.99"`; lib crate is `storage_analyzer`, binary `storage-analyzer`.

## Review attention
Unused-dependency state: all deps are declared but unused; nothing warns.
