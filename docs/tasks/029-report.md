# T029 report (engineer-2)

- `sanitize` now escapes U+061C, U+200E/F, U+202A-202E and U+2066-2069 as `\u{XXXX}` (uppercase hex, at least 4 digits), next to the existing `\xNN`. `display_name` goes through it. Every place that prints a name or path already calls `sanitize` (checked by grep: tree, map, lists, dialogs, status, stderr messages); the only other `to_string_lossy` is the history key, which is not displayed. The dialog renderer sanitizes again, which is harmless (the escapes contain no control characters).
- Width: sanitizing happens before any width measuring or truncation, so the escape's cells are counted.
- Tests, each seen failing first: unit test for each listed code point (escape present, raw code point absent) and that neighbours (U+200D, U+202F, U+2065) are untouched; `a_bidi_override_in_the_path_is_escaped_in_the_confirmation` (app_delete); `a_name_with_a_bidi_override_shows_escaped_within_its_width` (ui_tree).
- Checks: fmt, clippy (lib, bins, tests; `examples/scan_harness.rs` is someone else's untracked `todo!` file), tests, doc tests, release build.
