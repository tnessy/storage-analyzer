# T008 report (engineer-1)

**Status:** DONE. Details and the per-field table are in `spikes/r1-memory/REPORT.md`.

- Files: `spikes/r1-memory/**` (crate with a `Stream` generator, a counting allocator, per-size child processes, `home-histogram.txt`).
- Tests first: 4 stream tests (valid parent-before-child order, root complete, requested 10 % directories, deterministic seed, name-length mean, histogram from a real directory) failed against `todo!()` stubs; a depth test failed (464 levels) before the generator picked pending directories at random. 6 tests pass: `cargo test --manifest-path spikes/r1-memory/Cargo.toml`.
- Result at 20 M entries: `TARGET: model_bytes_per_entry=152.8 (target 120)`, `TARGET: model_rss_gib_at_20M=2.85`. Apply 3.4-4.1 M entries/s.
- Biggest saving: directory-only fields in a dir table, about 54 B/entry (to about 99 B); with `dev_idx` and a 12-byte `NameRef` about 89 B.
- Model code was not changed.
