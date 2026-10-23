# T024: Harden the write guard and the APP_ID check (T001 follow-up)

- **Engineer:** engineer-1. **Depends on:** T001 (merged). **Blocks:** nothing.
- **Parallel with:** everything. The files are owned only by this task. Fit it in between T003 and T004.

## Goal
Close gaps in the M9 write guard and the APP_ID check that the T001 review found. The guard stays lexical; no parser is needed.

Acceptance criteria:
1. **`tests/write_guard.rs` also flags** the following, outside `src/delete/**` and `src/history/**`:
   - `File::options`, `File::create_new`
   - `create_dir`, `create_dir_all`, `copy`, `hard_link`, `soft_link`, `symlink`, `set_len`, `set_times`, `set_modified`
   - rustix `mkdirat`, `linkat`, `symlinkat`, `fchmod`, `fchmodat`, `chmod`, `ftruncate`, `utimensat`, `futimens`
   - `OFlags::WRONLY`, `OFlags::RDWR`, `OFlags::CREATE`, `OFlags::TRUNC`, `OFlags::APPEND` (also when imported as bare `WRONLY` etc. within a file that mentions `OFlags`)
   - a bare `write(` call when the file imports `std::fs::write` (`use std::fs::write`, or `write` inside a `use std::fs::{…}` list)
2. Each new form has its own `every_forbidden_form_is_caught` case. The existing negative tests still pass. Add a negative test showing that `OFlags::RDONLY | OFlags::DIRECTORY` passes.
3. **`tests/no_name_literal.rs`:**
   - exempt only the exact definition line in `src/lib.rs` (`pub const APP_ID: &str = "…";`);
   - strip `//` comments only outside string literals;
   - add tests: `const APP_ID_X: &str = "tool-x"` fails; `"https://x tool-x"` fails; `const APP_ID` in a file other than `lib.rs` fails.
4. Move the shared helpers (`collect_rs`, `write_tree`) into `tests/support/src_tree.rs`, included with `#[path]`. Document the known false positive (`.write(` in files that mention `OpenOptions`) in the header.

## Files owned
`tests/write_guard.rs`, `tests/no_name_literal.rs`, `tests/support/src_tree.rs`.

## Tests to write first
Write the new cases first and show that they fail against the current guard. Command: `cargo test --test write_guard --test no_name_literal`.

## Out of scope
Parsing Rust or macro expansion. Guarding `examples/` and `spikes/`.
