//! TCP5.2: instruction-count gates over the storage commit path.
//!
//! Setup (runtime open, tempdir, warm-up commits) runs outside the measured
//! section; each measured body is exactly the shipping-path operation.

#[path = "perf_support.rs"]
mod perf_support;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use perf_support::{batch_of, fill_commits, open_durable_runtime};
use strata_storage::api::StorageRuntime;
use tempfile::TempDir;

struct Warmed {
    runtime: StorageRuntime<'static>,
    _dir: TempDir,
}

/// A durable runtime with 8 warm-up commits so the measured commit hits the
/// steady-state path (WAL open, memtable live), not first-commit setup.
fn warmed_runtime() -> Warmed {
    let (runtime, dir) = open_durable_runtime();
    fill_commits(&runtime, 8);
    Warmed { runtime, _dir: dir }
}

#[library_benchmark]
#[bench::steady(setup = warmed_runtime)]
fn commit_small_batch(warmed: Warmed) -> Warmed {
    let batch = batch_of(1_000, 3, 64);
    warmed.runtime.commit(&batch).expect("small commit");
    warmed
}

#[library_benchmark]
#[bench::steady(setup = warmed_runtime)]
fn commit_medium_batch(warmed: Warmed) -> Warmed {
    let batch = batch_of(2_000, 64, 64);
    warmed.runtime.commit(&batch).expect("medium commit");
    warmed
}

#[library_benchmark]
#[bench::steady(setup = warmed_runtime)]
fn wal_append_burst(warmed: Warmed) -> Warmed {
    for round in 0..32u64 {
        let batch = batch_of(10_000 + round * 3, 3, 64);
        warmed.runtime.commit(&batch).expect("burst commit");
    }
    warmed
}

library_benchmark_group!(
    name = storage_commit;
    benchmarks = commit_small_batch, commit_medium_batch, wal_append_burst
);
main!(library_benchmark_groups = storage_commit);
