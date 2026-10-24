# T036b review, first pass (Heimdall, 2026-10-24): CHANGES REQUESTED (process + one cleanup). Snapshot verification after T036a is stable.

**Scope check against design §3.12 rev 12, from the report** (code verification follows in the snapshot pass):
- any read or parse error → `None` for the rest of the scan;
- device set = union of block devices at or under the root, resolved once;
- sleeps ≤ 100 ms, `stop()` ≤ 200 ms;
- the `diskio` thread's panic is tagged and only hides the field.

## Required
1. **Remove the dead `DeviceStatSource::generation()`** (rev 12 dropped generations).
2. **README red-first rule.** The implementation came before the tests, so **one targeted breakage per acceptance criterion** is required, each shown failing and then reverted. Five are done (longest prefix, btrfs members, the 1 s cadence, the cross-fs union, an unreadable device). **Still missing, so add a breakage for each:**
   - **LUKS/dm counted at that device only:** for example, count the underlying devices too → the dm test must fail;
   - **tmpfs/NFS → `None`:** for example, treat any source as a block device → those tests must fail;
   - **escaped mount points:** for example, skip unescaping → the escaped-path test must fail;
   - **bounded stop:** for example, make `stop()` join without a bound while the fake `SysView` blocks → the stop test must fail (time out);
   - **`read_bytes()` never blocks while the thread is busy:** for example, have `read_bytes()` take a lock the thread holds during sampling → the non-blocking test must fail.

   If a criterion turns out to have **no** test that would catch its breakage, write that test (red first) rather than skip it.
3. **Report the breakage list** (mutation → failing test → output excerpt) in `036b-report.md`.

## Accepted
- **engineer-1 added the `pub mod resolve; pub mod sys; pub mod thread;` lines to `src/diskio/mod.rs`** (T036a's file). That is the declaration I had said would go in at integration. It is accepted; engineer-2 must keep those lines in their rework.
- The trait's `read_bytes(&mut self)` receiver, as in T036a's contract.
