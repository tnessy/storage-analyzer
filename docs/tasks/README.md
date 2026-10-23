# Common rules for every task brief

Every brief in this folder includes these rules by reference. Read them once.

## Source documents
- Requirements: `docs/brief.md` v0.5 (M1–M12).
- Architecture: `docs/design.md` rev 6 (accepted). Its §4 Conventions are binding.
- Decisions: `docs/adr/001–009`.
- Layout, keys, states and copy: `docs/ux/spec.md` (layout A). For UI text, the spec wins over everything else.
- Plan, ownership and lead decisions D1–D9: `docs/plan.md`.

## Workflow (iron laws)
1. **Test first.** Write the tests listed in the brief, run them, and **see them fail** for the right reason. Keep the failing output: your hand-off must include the command and a short excerpt of the failure.
2. Implement until they pass. No production code without a test that would fail if the behavior broke. Do not write tests that assert on mocks instead of behavior.
3. **Touch only the files your brief owns.** If you need a change anywhere else (a contract, `Cargo.toml`, another engineer's module), stop and ask Heimdall. Don't do cleanups you happened to notice; list them in your hand-off instead.
4. **Do not commit.** Heimdall reviews, commits and integrates.
5. Questions about another engineer's area go to Heimdall, not to the other engineer.

## Commands (run all of them before hand-off; Heimdall re-runs them)
```
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo test --doc
cargo build --release
```
Task-specific commands (single test, ignored or privileged tests) are in each brief.

## Code conventions (summary of design §4; the design wins if they differ)
- Rust stable 1.99. ratatui 0.30, crossterm 0.29, rustix 1.x. Plain std threads and channels; no async runtime.
- Types are `UpperCamel` nouns. Units go in names: `*_bytes`, `*_ms`.
- Core modules (`scan`, `delete`, `model`, `history`) return typed error enums that keep the `errno`. No `unwrap`/`expect` on I/O paths. `unwrap` in tests is fine.
- Only the main thread mutates the model/`AppState`. Other threads talk only through `ScanMsg`, `DeleteEvent`, `DeleteRequest`, the probe estimate, and `ScanControl`.
- **Only `delete` (user data) and `history` (own state file, opt-in) may write to the filesystem.** `tests/write_guard.rs` enforces this.
- Never follow symlinks. Use raw bytes for filesystem operations; decode lossily **only for display**, and sanitize names before rendering.
- User-facing text lives only in `src/ui/` (copy constants in `src/ui/copy.rs`). The command name is never a string literal: use `APP_ID` or `env!("CARGO_BIN_NAME")`.
- Comments only where intent isn't obvious. No dead code, and no `todo!()` left in code you own when you hand off (contract stubs owned by another task are the exception).

## Measured targets are not gates
50 ms p95 input latency, 120 B/entry, the `min(nproc, 8)` thread default, 200 ms first paint, and 50 ms per 1 M nodes for the hard-link check are **targets**. Tests that measure them print the value with the prefix `TARGET:`, for example `TARGET: input_p95_ms=12.4 (target 50)`, and do **not** assert on it. The only timing gate is brief M1: **first frame ≤ 1 s**.

## Hand-off checklist (reply to Heimdall)
- The list of files changed (it must be a subset of the brief's owned files).
- The failing-first evidence: command plus output excerpt for each new test group.
- The output of the five commands above, run fresh.
- Any `TARGET:` lines you measured.
- Anything you couldn't do, and any questions.
