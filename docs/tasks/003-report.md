# T003 report (engineer-1)

**Status:** DONE

Files: `src/model/{mod,tree,names,hardlink}.rs`, `tests/model_apply.rs`. T005/T011/T012 methods remain `todo!` stubs.

Red first: `cargo test --test model_apply` -> all 14 tests failed, panicking at `Model::new` (`todo!("T003")`). Then implemented; all green on the first run. Fresh: fmt ok, clippy -D warnings clean, `cargo test --all-targets` pass (25 unit, 14 model_apply, 4, 7), doc ok, release build ok.

## Decisions (please check)
- Layout: struct-of-arrays in `tree.rs`; names in one byte arena (`u64` offset + `u16` length, so no 4 GiB cap). Per-node `children: Vec<NodeId>` (24 B). Rough 120+ B/entry; T008 measures, not a gate.
- Placeholders: id blocks can arrive out of order, so gaps hold placeholder slots (no parent, not root). `exists()` is false for them and for `REMOVED`.
- Ignored messages: Listing/DirIssue/Cancelled for a dir that doesn't exist, isn't a dir, or is already `DONE`; also a Listing whose id range overlaps live nodes. T012's rescan must reset `DONE` before re-listing.
- A dir entry is descended (pending) only if `kind == Dir` and `issue.is_none()`. Any entry issue records a `ScanIssue`; flags: `OtherFilesystem` -> `OTHER_FS`, `SkippedByUser` -> none, others -> `ERROR`.
- `OTHER_FS` dirs get own size 0 and are excluded from the `dirs` count (du -x).
- Dir `apparent` is forced to 0 in the model (also for the root).
- `Cancelled`: dir gets `INCOMPLETE|DONE`, so it is complete but not exact (`inexact_dirs` +1 up the chain, itself included).
- `Done{cancelled:true}`: only marks undone dirs `INCOMPLETE`; `pending_dirs` unchanged.
- `HardLinks::get` is `#[cfg(test)]` until T011 needs it (avoids a dead-code warning).
- Accessors index directly: calling them with a never-allocated id panics; check `exists` first.
