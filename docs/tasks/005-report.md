# T005 report (engineer-1)

**Status:** DONE

Files: `src/model/{mod,index,category}.rs`, `tests/model_indexes.rs`; amendment items: `src/types.rs` (`Kind::from_raw_mode` + unit test), `src/scan/fs.rs` and `src/delete/sys.rs` (private kind mappings removed), `tests/model_apply.rs` (root path test), `Model::path` fix.

## Failing first
- `model_indexes`: all 10 tests failed (`todo!` stubs; `ext_of`, `category_totals`, `recompute_top` stubbed so the file compiled).
- `kind_from_raw_mode_reads_the_file_type_bits_only` failed on a `todo!`; `path_of_the_root_has_no_trailing_slash` failed (`/data/x/` vs `/data/x`). Both then fixed.

## Fresh results
fmt ok; clippy -D warnings clean; `cargo test --all-targets` all pass (87 lib, 17 model_apply, 10 model_indexes, 22 scan_walker, 17+3 ignored delete, the rest unchanged); doc ok; release build ok.

## Decisions
- Counted entries (ext totals, top lists): non-directories without `HARDLINK_DUP`, `ERROR` or `OTHER_FS`. Top lists take regular files only (`Kind::File`); ext totals also count symlinks/other entries that have no issue.
- `TopN` is a bounded min-heap on `(size, Reverse(id))`; O(log N) per file; ties broken by smaller NodeId. `top_n = 0` yields empty lists.
- `ext_of` lowercases ASCII only; raw bytes are kept (no UTF-8 requirement).
- `category_of_node` for a directory: categories of its children weighted by their disk size (child directories use their own cached category); ties go to the lower `Category` order; an empty directory is `AudioOther`. The cache (`Mutex<HashMap>`, so `Model` stays `Sync`) is valid while the directory's disk aggregate is unchanged. First call on a big directory visits its whole subtree once.
- `recompute_top` does not touch the ext totals; T011 must subtract those itself.
- `Category` and `ExtKey` now derive `Ord`/`Hash` (needed for sorting/maps).
- `Model::path(root)` returns `root_path` unchanged; other nodes `root_path.join(rel)`.
