# T031: Diagnose (then fix) RSS 4.3 GiB with ≥ 2 threads vs 2.4 GiB with 1

- **Engineer:** engineer-1. **Depends on:** T009. **Schedule:** after T030.
- **Status:** v1 (team lead, per the architect's assessment; design §5 "Process memory").

## Diagnosis, in this order
1. **At the end of a harness run** (`--threads 1` and `--threads 4`, on `$HOME`): call glibc `malloc_stats()` and then `malloc_trim(0)`, and re-read RSS. If "in use" ≈ the model size and the trim frees most of the gap, the gap is **fragmentation, not a leak**. Use `libc` FFI **in the example only**; ask Heimdall to add the `libc` dev-dependency.
2. **Re-run with mimalloc or jemalloc** as the global allocator, **in the example only** (a dev-dependency, via Heimdall), at 1 and 4 threads.
3. **`heaptrack`, only if 1–2 are inconclusive.**

**Likely cause (architect):** with 2+ threads the channel stays full, so many short-lived allocations (per-name heap blocks, entry vectors, child-job paths) interleave with ~2 M long-lived per-folder child lists, which fragments the heap.

## Fix (depends on the result; report before implementing)
- **(a)** Swap the global allocator for the binary: one dependency. **That needs the architect's and owner's OK**, because of ADR-001 and the dependency policy.
- **or (b)** Stop allocating per name in `RawEntry`: one byte buffer plus offsets per listing. That is a **contract change** to `RawEntry`, decided with Heimdall and the architect before you start.

## Deliverables
- `spikes/R31-REPORT.md` with RSS, "in use", and RSS after trim for each configuration.
- The recommended fix; implemented only after approval.
