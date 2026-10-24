# T027 review (Heimdall, 2026-10-23): APPROVED, committed
**Fresh evidence** (clean worktree: HEAD plus `src/model/{control,mod}.rs`, `src/scan/walker.rs`, `tests/model_controls.rs` with its proptest seed file, `tests/scan_walker.rs`, and **only** the 2-line placeholder arm in `src/app/controls.rs`; engineer-2's in-progress T021 files were excluded):
- fmt ok, whole-tree clippy clean;
- every suite passes, 3 times (model_controls 30, scan_walker 24, …);
- doc tests ok, release build ok;
- `PROPTEST_CASES=2000` release run of model_controls passes.

**Red first** is reported: 4 model_controls tests and 1 scan_walker test. The two "pin existing behaviour" tests passing immediately is acceptable.

**My repro re-run:** a top-level entry with a stat error, rescanned and relisted → no panic, and `top_level_progress` is balanced. An `OTHER_FS` top-level dir → `rescan_plan == OtherFilesystem` ✓.

**Design accepted:**
- **`TOP_COUNTED` flag:** it replaces guessing, and every progress adjustment uses it. A never-counted top-level dir starts counting once a rescan makes it pending.
- **`begin_rescan` on `OTHER_FS`:** a no-op.
- **The walker's rescan identity:** in one-filesystem mode, a dir on another device is sent as `Identity` with an `OtherFilesystem` issue and is not listed. The model converts it so the totals equal a scan that met it as another filesystem from the start.

**Seed file:** `tests/model_controls.proptest-regressions` holds the two shrunk overflow cases. That makes them valid seeds, so they are committed.

**Follow-up:** T020b (engineer-2) replaces the placeholder arm with the UX §5.4 message.
