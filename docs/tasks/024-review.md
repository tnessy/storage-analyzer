# T024 review (Heimdall, 2026-10-23): APPROVED, committed

**Fresh evidence** (clean worktree): `write_guard` 11 pass, `no_name_literal` 7 pass; fmt and clippy clean. The red-first evidence is reported.

**Pass 1:** every form listed in the brief is flagged, including the OFlags write flags, a bare imported `write`, and `File::options`/`create_new`. The APP_ID exemption is limited to the exact `lib.rs` line, and the `//`-in-string case is fixed. Only owned files were touched.

**Pass 2:**
- A single shared `scrub(Keep|Blank)` replaces the two comment strippers, so the duplication is gone.
- Each forbidden-form case asserts that the report names the construct.
- `#![allow(dead_code)]` in the shared test-support module is acceptable, because each test file uses a subset.
- The documented limits are accepted: a `use std::fs as f` alias isn't seen, and a local variable named `copy` is reported.
