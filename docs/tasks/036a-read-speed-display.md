# T036a: Disk read speed: contract, meter and status-line display

- **Engineer:** engineer-2. **Schedule:** now. **Depends on:** T019, T020 (merged).
- **Parallel with:** T036b (engineer-1, the real `/proc` source). Its design is pending the architect.
- **Source:** the user approved new v1 scope (2026-10-24). UX `ux/spec.md` §5.2, §7.1, the §3.1 mockup and check 23. **The spec is the source of truth for every string and width.**

## 1. The contract (create it in this task; then frozen, so changes go through Heimdall)
`src/diskio/mod.rs` (new module; add `pub mod diskio;` to `src/lib.rs`, the only `lib.rs` edit):
```rust
/// Where the read-speed numbers come from. Implemented by T036b for real (/proc, /sys) and by
/// fakes in tests. Must be cheap and non-blocking: it is called from the main loop at most
/// once per second.
pub trait ReadBytesSource {
    /// Cumulative bytes read so far by whatever the source measures, or `None` if the
    /// source can't be read (then, and for the rest of the scan, the field is hidden).
    fn read_bytes(&mut self) -> Option<u64>;
}

/// No source available (the default until T036b lands, and when the platform has none).
pub struct NoSource;
impl ReadBytesSource for NoSource { fn read_bytes(&mut self) -> Option<u64> { None } }
```
**The meter is pure:** `ReadRateMeter::sample(now: Duration, cumulative: Option<u64>)` updates a shown value **at most once per second**, from the bytes per second over the last full one-second window. `rate() -> Option<u64>` (bytes/s):
- `None` until the first full one-second window;
- `None` from the first `None` sample onwards, for the rest of the scan (UX: hidden once the source stops);
- a measured zero is `Some(0)`;
- a counter going backwards (wrap or reset) counts as unavailable.

Put the meter in `src/diskio/meter.rs` (engineer-2 owns it). `App` takes a `Box<dyn ReadBytesSource>` like `Clock` and `EventSource`. The production starter passes `NoSource` until T036b wires the real source.

## 2. Display (UX §5.2 / §7.1)
- **Throughput format** in `ui/format.rs`, following the size rules plus `/s`: `812 B/s`, `812.0 KiB/s`, `64.2 MiB/s`. The value is **right-aligned in 11 cells**, the width of `999.9 MiB/s`. Label `read  `. No glyph. Identical in ASCII.
- **Placement:** just before `ETA`.
  - **First-scan (Indeterminate) line:** joined with ` · `. At 80 columns: `◌ Scanning  01:12 elapsed · 12/31 top · read  64.2 MiB/s · ETA unknown` (70 cells).
  - **With-ETA line:** separated by two spaces. At 80 columns: `◌ Scanning ~41% · 01:12  read  64.2 MiB/s  ETA ~01:44 (estimate, last scan)` (75 cells).
- **Drop order:**
  - **first-scan line:** after the queued count is dropped, `top-level done` shortens to `top`, then the read field goes. The top-level count is dropped last.
  - **with-ETA line:** the progress bar is dropped before the read field; the read field is dropped last.
- **When hidden,** the line is exactly the old form. Every existing status test must still pass.
- **Shown only while scanning.** Never in Starting, Stopping, Stopped, Done, a subtree rescan after Done, or the Deleting line.
- **No fake values:** never `0`, `--` or `…` for "no reading". A measured zero shows `read     0 B/s`.

## Tests (red first; check 23)
- the exact 80-column strings for both lines, in Unicode and ASCII, each ≤ 79 cells;
- **ETA starts at the same column whatever the read value** (try `0 B/s`, `812 B/s`, `999.9 MiB/s`);
- hidden before the first one-second sample, and when the source returns `None` (fake source), with no fake zero;
- a measured zero displays as `read     0 B/s`;
- never shown outside the scanning states listed above;
- **at most one value change per second** under a fake clock with a source changing every 100 ms;
- identical with NO_COLOR;
- meter unit tests: the first window, a counter going backwards, `None` latching.

## Also
Add a runbook row (M2 area, "read speed") and one manual check to `docs/acceptance/runbook.md`: during a cold-ish scan of `$HOME` the `read` field appears after about a second, and it disappears at Done.

## Files owned
`src/diskio/mod.rs`, `src/diskio/meter.rs` (new), the one `src/lib.rs` line, `src/ui/format.rs`, `src/ui/status.rs`, `src/ui/copy.rs`, `src/app/event_loop.rs`, `src/app/state.rs`, `src/app/mod.rs` (the starter passes `NoSource`), `tests/app_eta.rs` (or a new `tests/ui_status.rs`), the affected snapshots, and `docs/acceptance/runbook.md`.

## Amendment (2026-10-24): label `disk` + set generation (architect rev 11, UX update). **Apply now.**
1. **Label is `disk`, not `read`.** The status strings are `disk  64.2 MiB/s`, and a measured zero is `disk     0 B/s`. All check-23 strings (a)–(h) in `ux/spec.md` are updated. The width is unchanged, so the 11-cell value and the 70/75-cell 80-column lines stay the same.
2. **Contract addition, authorized and backward-compatible:** add to `ReadBytesSource`
   ```rust
   /// Changes whenever the measured device set changes (a device added). The meter restarts its
   /// window so a newly added device's lifetime total never shows up as a one-second spike.
   fn generation(&self) -> u64 { 0 }
   ```
   `NoSource` keeps the default.
3. **Meter:**
   - rate = Δbytes ÷ elapsed over the last ~1 s window;
   - **no value that second** if the total went down, or the source returned `None`;
   - **restart the window** (no value until a full second has passed) whenever `generation()` changes.
   - **Note the difference from the earlier text:** `None` and a counter decrease now mean "no value for that second", not a permanent latch. The thread may become available again (for example, after a device is added). Keep the field hidden while there is no value.
   - **Tests:** a counter reset gives no value that second, then values resume; `None` → hidden, then values resume after a full second; a generation change → no spike, the window restarts.
