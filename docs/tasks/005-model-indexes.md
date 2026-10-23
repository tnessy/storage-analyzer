# T005: Model indexes (extension/category totals, live top-N files)

- **Engineer:** engineer-1. **Wave:** 2. **Depends on:** T003. **Blocks:** T009, T011, T017, T018.

## Goal
Whole-root file-type breakdown (M5) and largest-files list (M6), maintained incrementally in `apply` and correct live during the scan.

Acceptance criteria:
1. `ext_of(name) -> ExtKey` (design §3.4):
   - the lowercased bytes after the last `.`;
   - leading-dot names (`.bashrc`) and names with no dot → `NoExt`;
   - an extension longer than 16 bytes → `Other`;
   - `a.tar.gz` → `gz`;
   - a trailing dot (`a.`) → `NoExt`.
2. Per-ext `{disk_bytes, apparent_bytes, count}` over the whole root, updated in `apply`. Only non-directories count. Duplicate hard links are excluded (consistent with totals), and so are `OTHER_FS` entries. `ext_totals()` returns them sorted by disk bytes desc, then key asc.
3. `category_of(&ExtKey) -> Category` is a static table, in `src/model/category.rs`:
   - Video: mp4 mkv webm avi mov m4v
   - Image: jpg jpeg png gif webp heic svg raw cr2 nef
   - Archive: zip tar gz xz zst bz2 7z rar iso pkg deb rpm
   - CodeText: rs c h cpp py js ts go java md txt json toml yaml yml xml html css sh log csv
   - Binary: so a o bin exe dll elf class wasm, and `NoExt` (common for executables)
   - Documents: pdf doc docx odt xls xlsx ods ppt pptx epub
   - AudioOther: mp3 flac ogg opus wav m4a, plus everything else, including `Other`

   The table contents are owned by UX + engineers (design §3.4). Keep it as one data table so it is easy to edit. `category_of_node(id)`: a file maps by its ext. A dir maps to the category with the largest disk bytes **among its direct file children plus its child dirs' cached category**. Compute it lazily and cache it per dir until that dir's aggregate changes. If that turns out costly, keep it simpler and tell me: the UI only calls it for the ≤ ~60 tiles visible.
4. **Top-N:** `top_files(mode)` returns the N largest non-duplicate files for **each** size mode (two bounded min-heaps, N = `ModelOptions.top_n`, default 100, `--top N`), sorted desc and tie-broken by NodeId. It is updated in `apply` in O(log N) per file. It is live: correct after every applied message. Provide `recompute_top(&mut self)`, an O(files) rebuild that T011 calls after a delete.
5. Per-category totals are **sums of per-ext totals** (no separate index). Expose `category_totals() -> Vec<(Category, ExtTotals)>`. This is a new read method; add it to `src/model/mod.rs`, which you own in this task.

## Design references
§3.4 Indexes (extensions, top files, category), §3.10 "Largest → show in tree", "Types categories". ADR-004.

## Files owned
`src/model/mod.rs` (fill in the T005 stubs, add `category_totals`), `src/model/index.rs`, `src/model/category.rs`, `tests/model_indexes.rs`.

## Interfaces
**Provides:** `ext_totals`, `category_totals`, `category_of`, `category_of_node`, `top_files`, `recompute_top`. Engineer-2 consumes them in T017 and T018. Tell Heimdall when these are merged.

## Tests to write first
- `ext_of` table tests covering every rule above, plus invalid UTF-8 bytes and an uppercase `.JPG`.
- **M5:** a stream with mixed extensions across several dirs. The per-ext and per-category totals (size and count) equal a brute-force recount over the final tree, and are sorted by size. Hard-link duplicates are not double-counted.
- **M5 live:** after each message of the stream, the totals equal the brute-force recount over the nodes applied so far.
- **M6 live:** after each message, `top_files(Disk)` and `top_files(Apparent)` equal a brute-force sort of the files applied so far, truncated to N. This holds for N = 1, 3 and 100.
- `--top` honored: N = 5 returns exactly 5 when ≥ 5 files exist, otherwise all of them.
- `recompute_top` equals the incremental result.

## Out of scope
Updating the indexes on delete (T011 calls `recompute_top` and subtracts ext totals). The views (T018).

## Amendment (2026-10-23, from the T015 review)
- **Fix `Model::path(root)`:** it returns a trailing slash, because `PathBuf::push` of an empty `rel_path` adds one. `path(root)` must equal `root_path` exactly, and `path(child)` must be `root_path/child`. Add a test (red first) to `tests/model_apply.rs`. You own it for this fix.
