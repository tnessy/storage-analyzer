# T001 review (Heimdall, 2026-10-23): APPROVED, committed 9e3e6c2

**Fresh evidence (run by Heimdall):**
- `cargo fmt --check`: ok.
- `clippy -D warnings`: clean.
- `cargo test --all-targets`: 9 unit + 4 + 7 integration tests pass.
- `cargo test --doc`: ok.
- `cargo build --release`: ok.

**Pass 1, spec:**
- Every contract in the brief is present, with the right field sets.
- Only owned files were touched.
- Placing `ProbeMsg` in `probe`, and `Busy`/`Flags` as written, is fine.

**Engineer's flagged items:**
- `ScanControl::new()` and `is_cancel_all()`: **accepted** as additions to the contract. Workers need the read side. It is also accepted that `is_cancelled` is prefix-only and ignores `cancel_all`, because the queue checks `cancel_all` separately (T004).
- **Lexical write guard:** accepted as the v1 approach (no parser). The forbidden list was incomplete; that was my brief's gap, not the engineer's. It is now follow-up **T024**.
- **APP_ID check edge cases:** these are real but low risk, and are fixed in T024:
  - any line containing `const APP_ID` is exempt, in any file and including `APP_ID_X`;
  - a `//` inside a string literal truncates the check.

**Pass 2, quality notes for T024:**
- `collect_rs`/`write_tree` are duplicated in both test files. Move them to `tests/support/src_tree.rs`.
- In a file that mentions `OpenOptions`, the `.write(` match also hits `RwLock::write()`. That is a conservative false positive and acceptable; document it in the test header.

**Integration edits by Heimdall** (made to unblock integration): `.gitignore` gained `spikes/*/target`. `.claude/` is the user's local tool config; it is left untracked and not ignored.
