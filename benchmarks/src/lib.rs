//! Benchmark harness. All bins inherit jemalloc as the global allocator —
//! the pre-V1 numbers were measured under it (via the old `stratadb` root
//! package), so the instrument keeps it for comparability.
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub mod harness;
pub mod schema;
