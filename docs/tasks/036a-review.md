# T036a + T036b review (Heimdall, 2026-10-24): APPROVED, committed together (one hash-checked snapshot; `src/diskio/mod.rs` ties them)
**Fresh evidence** (clean worktree, the snapshot taken after engineer-1's explicit "finished"; the md5s match the reported ones; my earlier snapshot was rejected because it caught a breakage still in place):
- fmt ok, **whole-tree clippy clean**;
- every target passes (diskio_source 22, ui_status 7, app_eta, lib 123, …);
- doc tests ok, release build ok.

**Loops:** diskio_source 20/20; ui_status and app_eta 50/50 each.

**Breakages I re-ran myself** (on the restored `thread.rs`): a 3 s stop wait → `stop_returns_within_250_ms_even_when_a_read_hangs` fails; a 1 ms sleep in `read_bytes` → `reading_the_source_never_blocks_while_the_thread_is_busy` fails. Restored and compared with `cmp`.

**T036a (engineer-2), checked against the rev 12 amendment:**
- **Label:** `copy::DISK_LABEL = "disk"`; the measured zero shows `disk       0 B/s` (UX §5.2 / check 23(e), seven spaces), asserted in `ui_status`.
- **Contract:** `ReadBytesSource` is `read_bytes()` only; no `generation()` anywhere.
- **Meter:** a decrease or `None` gives no value that second, then values resume.
- **Display:** the check-23 strings (70/75 cells), the hidden field when there is no value, only while scanning.
- **Red first:** reported.

**T036b (engineer-1), checked against design §3.12 rev 12:**
- **Thread and source:** the `diskio` thread with atomics; `read_bytes()` is two atomic loads and never blocks.
- **Devices:** the union at or under the root, resolved once.
- **Read errors:** any read or parse error → `None` for good.
- **Lifecycle:** `stop()` ≤ 200 ms, then detach; the thread's panics are tagged `diskio`.
- **Read-only:** only `/proc` and `/sys`; the write guard is green.
- **Breakages, one per criterion (10 total),** including the new `a_source_that_is_not_an_absolute_path_is_never_looked_up` (it closed the tmpfs/NFS gap).

**Process (recorded):** another implementation-before-tests lapse (T036b). It is compensated per the README rule. The breakages run in the shared tree left a mutated file once (a shell was killed); it was caught by my snapshot check before anything was committed.

**Next:** T036c (engineer-2): wiring, help footer, runbook.
