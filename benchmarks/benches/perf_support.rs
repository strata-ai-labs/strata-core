//! Shared fixtures for the TCP5.2 instruction-count benches.
//!
//! Included via `#[path]` from each iai-callgrind bench target (the
//! `ycsb_workloads.rs` precedent). Everything here is deterministic by
//! construction: fixed branch, fixed keys, fixed value fill, no RNG — the
//! gate's honesty rests on run-to-run instruction stability.
//!
//! Deliberately does NOT link the `strata_benchmarks` lib: the lib installs
//! jemalloc as the global allocator for the wall-clock bins, and the
//! instruction-count targets stay on the system allocator so counts are
//! allocator-stable under valgrind.

use strata_storage::api::{
    CommitBatch, CommitMutation, CommitOptions, StorageDurabilityPolicy, StorageKey,
    StorageRuntime, StorageSpaceId, StorageValue,
};
use strata_storage::api::BranchId;
use tempfile::TempDir;

pub const BENCH_BRANCH: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);

pub fn bench_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine space")
}

/// A durable Standard-policy runtime on a fresh tempdir. The tempdir guard
/// must outlive the runtime; callers keep both.
pub fn open_durable_runtime() -> (StorageRuntime<'static>, TempDir) {
    let dir = tempfile::tempdir().expect("bench tempdir");
    let runtime = StorageRuntime::open_durable_local(
        dir.path().to_path_buf(),
        StorageDurabilityPolicy::Standard,
    )
    .expect("open durable runtime")
    .into_runtime();
    (runtime, dir)
}

pub fn put_mutation(index: u64, value_bytes: usize) -> CommitMutation {
    CommitMutation::Put {
        storage_space: bench_space(),
        key: StorageKey::new(format!("bench-{index:08}").into_bytes()).expect("valid key"),
        value: StorageValue::new(vec![0x5A; value_bytes]),
        ttl: None,
    }
}

pub fn batch_of(start: u64, count: u64, value_bytes: usize) -> CommitBatch {
    let mutations = (start..start + count)
        .map(|index| put_mutation(index, value_bytes))
        .collect();
    CommitBatch::new(
        BENCH_BRANCH,
        mutations,
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid bench batch")
}

/// Commit `commits` sequential 3-mutation batches — the fixture filler for
/// the reopen bench and the warm-up for commit benches.
pub fn fill_commits(runtime: &StorageRuntime<'static>, commits: u64) {
    for round in 0..commits {
        let batch = batch_of(round * 3, 3, 64);
        runtime.commit(&batch).expect("fixture commit");
    }
}
