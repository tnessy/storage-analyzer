//! `scan_harness` with mimalloc as the global allocator (T031 measurement only).
//!
//! `cargo run --release --example scan_harness_mimalloc -- ROOT --threads 4`

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[allow(dead_code)]
#[path = "scan_harness.rs"]
mod harness;

fn main() {
    let _ = harness::ALLOCATOR.set("mimalloc");
    harness::main();
}
