# T029: Escape Unicode bidi and format controls in displayed names

- **Engineer:** engineer-2 (owns `src/ui/sanitize.rs`).
- **Schedule:** after T020b.
- **Depends on:** T021 (merged).
- **Source:** the architect's T021 final review; design §4 "Sanitize file names" (follow-up added).

## Goal
A crafted file name must not be able to display reversed or disguised, in particular in the delete confirmation. `sanitize` currently escapes C0, C1, ESC and DEL, but not the Unicode bidi and format controls.

## Acceptance criteria
1. `sanitize` (and `display_name`) render these code points as `\u{XXXX}`-style escapes. Use the same visible style as the existing `\xNN` escapes; the exact form is `\u{202E}`:
   - U+061C (ARABIC LETTER MARK)
   - U+200E, U+200F (LRM, RLM)
   - U+202A–U+202E (LRE, RLE, PDF, LRO, RLO)
   - U+2066–U+2069 (LRI, RLI, FSI, PDI)
2. **Everywhere `sanitize` is used** (tree, map, lists, dialogs, status, stderr messages), with no other code path that displays a raw name.
3. **Width accounting:** the escape's display width is counted, so truncation stays correct.

## Files owned
`src/ui/sanitize.rs`, `tests/app_delete.rs` (one new test), `tests/ui_tree.rs` (one new test).

## Tests to write first (show them failing)
- **Unit, one case per listed code point:** the output contains the escape and not the raw code point.
- **Dialog level:** a folder named `evil\u{202E}txt.exe/` shows escaped in the delete confirmation's path line, and the buffer contains no U+202E.
- **Tree row:** the same name renders escaped, within its width.

## Commands
The five standard ones.
