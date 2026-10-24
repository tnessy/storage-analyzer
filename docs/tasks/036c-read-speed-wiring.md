# T036c: Disk read speed: wiring, label and help line

- **Engineer:** engineer-2. **Depends on:** T036a and T036b merged, plus **UX's label decision**.
- **Schedule:** after both merge.

1. **Production wiring:** `app/mod.rs::production_starter` uses `DiskSource::resolve_for(root, cross)` (T036b), resolved **after the first frame**, instead of `NoSource`.
2. **Label:** the UX spec currently says `read  `; the architect's source is device-wide, so it proposes `disk`. **UX decides** (brief M13: the label must say what is measured). Change the one copy constant, keep the 11-cell value alignment, and update the check-23 strings and snapshots.
3. **Help line** (wording from UX): the speed is device-wide, so other programs' reads count too.
4. **Rescans:** UX §5.2 says the field is **not** shown during a subtree rescan after Done. The architect's "including rescans" is superseded by UX (it owns the surface). Keep T036a's rule.
5. **Tests:**
   - with the fake `SysView` from T036b, a full app run shows `disk …/s` after one second of fake-clock time and hides it at Done;
   - the label and help strings match UX;
   - every check-23 test still passes with the new label.
6. **Runbook:** update the read-speed row and the manual check: on `$HOME` the field appears within about 1 s of a cold-ish scan; on `/tmp` (tmpfs) it never appears.
