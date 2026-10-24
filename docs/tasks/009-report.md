# T009 report (engineer-1)

**Status:** DONE. Files: `examples/scan_harness.rs`, `spikes/R2-R4-R8-REPORT.md` (all tables, TARGET lines and findings). No production code changed.

- Tests first: the two smoke tests in the example (`cargo test --example scan_harness`: a 1,000-entry tree reaches Done with applied == produced for 1 and 4 threads; walker-only mode counts without applying) failed against a `todo!()` stub, then passed.
- Headline numbers ($HOME, 19.85 M entries, warm): pure apply 2.7 M entries/s; apply/walker 82 % at 4 threads, 48 % at 8, 39 % at 16 (target >= 70); wall flat from 4 to 16 threads; frontier about 504 k; re-key 32.7 ms at 500 k pending jobs; focus completes a big folder in 56-224 ms vs seconds; worker CPU while the channel is full 1.3-2.7 % of a core (no backoff needed); `category_of_node` p95 0.06 ms; tree row build 5.3 ms at 100 k children but 83.8 ms at 1 M (target 50); btrfs parity byte-exact on /usr, ~/xiv and `/` with -x.
- Problems found: tick spikes of 118-147 ms in `Model::apply` (hypothesis: array growth); `recompute_top` 0.4-0.6 s; process RSS 4.3 GiB vs 2.4 GiB with more than one thread (unexplained, details in the report).
- Not done: a cold-cache run (needs `sudo drop_caches`, which the harness only prints).
