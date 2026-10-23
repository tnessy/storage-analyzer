# T024 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree): `write_guard` 11 pass, `no_name_literal` 7 pass; fmt and clippy clean. The red-first evidence is reported.

**Pass 1:** every form listed in the brief is flagged, including the OFlags write flags, a bare imported `write`, and `File::options`/`create_new`. The APP_ID exemption is limited to the exact `lib.rs` line, and the `//`-in-string case is fixed. Only owned files were touched.

**Pass 2:**
- A single shared `scrub(Keep|Blank)` replaces the two comment strippers, so the duplication is gone.
- Each forbidden-form case asserts that the report names the construct.
- `#![allow(dead_code)]` in the shared test-support module is acceptable, because each test file uses a subset.
- The documented limits are accepted: a `use std::fs as f` alias isn't seen, and a local variable named `copy` is reported.

## Follow-up change by engineer-1 (2026-10-23): CHANGES REQUESTED
1. **`copy` narrowed to `fs::copy`, an import from `std::fs`, or a bare call after `use std::fs::*`:** **accepted.** `src/ui/copy.rs` is a mandated module name, and the narrowed match still catches every real `std::fs::copy` form.
2. **Skipping `#[cfg(test)] mod … { … }` blocks:** **accepted in principle**, because test modules don't ship. **But the implementation can hide production code.** For `#[cfg(test)] mod tests;` (a file module, no brace), `strip_test_modules` takes the *next* `{` anywhere after `mod`, and deletes the following production item. I verified it: a planted `src/scan/zz_probe.rs` containing `#[cfg(test)]\nmod tests;\n\npub fn evil(p) { std::fs::remove_file(p) }` **passes** `repo_sources_pass`.
   - **Fix:** strip only when the tokens are exactly `mod <ident> {` (optionally preceded by `pub`). A `mod <ident>;` is left alone.
   - **Tests, red first:** (a) the planted case above is flagged; (b) `#[cfg(test)] mod t { fn f() { std::fs::write(..) } }` passes; (c) a violation right after a stripped test module is still flagged.
