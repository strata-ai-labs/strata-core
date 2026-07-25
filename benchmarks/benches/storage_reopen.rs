//! TCP5.2: instruction-count gate over recovery — reopening a store whose
//! WAL holds ~200 committed batches. Setup builds and drops the store; the
//! measured body is the reopen (WAL replay + assembly).

#[path = "perf_support.rs"]
mod perf_support;

use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use perf_support::{fill_commits, open_durable_runtime};
use strata_storage::api::{StorageDurabilityPolicy, StorageRuntime};
use tempfile::TempDir;

/// Build a ~200-commit store, close it cleanly-by-drop (Standard's lossy
/// abandon keeps replay honest: the WAL tail is what recovery reads), and
/// hand the directory to the measured reopen.
fn staged_store() -> TempDir {
    let (runtime, dir) = open_durable_runtime();
    fill_commits(&runtime, 200);
    drop(runtime);
    dir
}

#[library_benchmark]
#[bench::two_hundred_commits(setup = staged_store)]
fn recovery_reopen(dir: TempDir) -> TempDir {
    let runtime = StorageRuntime::open_durable_local(
        dir.path().to_path_buf(),
        StorageDurabilityPolicy::Standard,
    )
    .expect("reopen staged store")
    .into_runtime();
    drop(runtime);
    dir
}

library_benchmark_group!(
    name = storage_reopen;
    benchmarks = recovery_reopen
);
main!(library_benchmark_groups = storage_reopen);
