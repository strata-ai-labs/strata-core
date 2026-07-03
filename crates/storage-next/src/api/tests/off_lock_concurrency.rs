//! BS2.4b — S3 concurrency-stress gate: the milestone's assurance-level test.
//!
//! N reader threads share one runtime with a committing writer (enabled by BS2.4b relaxing
//! `commit`/`branch` to `&self`) and must observe an atomic, monotonic, off-lock view: a committed
//! batch is all-or-none, the visible version never regresses per reader, and the writer reads its
//! own writes. Two complementary tests:
//! - `off_lock_readers_race_a_committing_writer` — **cache** mode (immediate visibility, no
//!   durability backpressure) stresses the protocol core — commit atomicity + the visible bound —
//!   at high rate.
//! - `off_lock_fork_and_materialize_under_concurrent_reads` — **`DurableLocal` + Background** (real
//!   worker threads) exercises C4 branching under load: fork mid-stress (O(1), correct inherited
//!   reads), parent/child concurrent reads, and a materialization (inherited→owned structural
//!   install) that must never tear a concurrent child read.

#![cfg(feature = "localfs")]

use super::*;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Barrier;
use std::time::{Duration, Instant};

const STRESS_KEYS: usize = 8;
const STRESS_READERS: usize = 3;
const STRESS_BATCHES: u64 = 400;

/// A `StorageRuntime` must be `Send + Sync` so a shared handle can be driven by reader + writer
/// threads concurrently (the enabling property of BS2.4b's `&self` commit/branch).
#[test]
fn storage_runtime_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<StorageRuntime<'static>>();
}

/// Cache mode: immediate visibility (clean read-your-writes) and no durability backpressure, so the
/// main read/write race stresses the off-lock protocol's core (commit atomicity + the visible bound)
/// at high rate. Structural-change coverage (fork/materialization under load) is the durable C4 test.
fn open_cache() -> StorageRuntime<'static> {
    StorageRuntime::open_ephemeral()
        .expect("open ephemeral cache runtime")
        .into_runtime()
}

fn open_durable_background(name: &str) -> StorageRuntime<'static> {
    let root = temp_dir_for_api_test(name);
    let backend = Box::leak(Box::new(StorageBackend::local_fs(root)));
    StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            // Default (large) budget so commits are not throttled by budget backpressure; the small
            // WAL growth thresholds still force frequent flush/compaction under the write load.
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(8 * 1024, 2, 3))
            .with_wal_segment_size_for_test(2048)
            .with_background_max_tasks_per_wake(64)
            .with_background_max_runtime_per_wake(Duration::from_millis(250)),
        backend,
    )
    .expect("open durable-local background runtime")
    .into_runtime()
}

/// One batch that overwrites the whole key set: key `stress-{i}` -> value `batch_no || i`, all under
/// a single commit version, so a reader observes the batch all-or-none.
fn stress_batch(branch: BranchId, batch_no: u64, num_keys: usize) -> CommitBatch {
    let mut mutations = Vec::with_capacity(num_keys);
    for i in 0..num_keys {
        let mut value = batch_no.to_be_bytes().to_vec();
        value.extend_from_slice(&(i as u64).to_be_bytes());
        mutations.push(CommitMutation::Put {
            storage_space: background_space(),
            key: key(format!("stress-{i:04}").as_bytes()),
            value: StorageValue::new(value),
            ttl: None,
        });
    }
    CommitBatch::new(
        branch,
        mutations,
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid stress batch")
}

/// Latest-scan the key set and enforce atomicity + completeness. Returns the batch number every row
/// agrees on, or `None` before the first full batch is visible on this branch. Panics on a torn read
/// (mixed batch numbers, or a partial key set) — the core S3 invariant.
fn read_checked(runtime: &StorageRuntime<'_>, branch: BranchId, num_keys: usize) -> Option<u64> {
    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch,
            background_space(),
            key(b"stress-"),
            ReadBound::Latest,
            None,
        ))
        .expect("off-lock latest prefix scan");
    let rows = scan.rows();
    if rows.len() != num_keys {
        // A committed batch writes every key under one version, so a Latest scan sees either 0 rows
        // (before the first batch) or the full set — a partial set is itself a torn read.
        assert!(
            rows.is_empty(),
            "torn read: {} of {num_keys} keys visible",
            rows.len()
        );
        return None;
    }
    let mut batch_no = None;
    let mut indices = Vec::with_capacity(num_keys);
    for row in rows {
        let value = row.value().expect("stress rows are puts").as_bytes();
        assert_eq!(value.len(), 16, "unexpected stress value width");
        let observed = u64::from_be_bytes(value[0..8].try_into().expect("8 bytes"));
        let index = u64::from_be_bytes(value[8..16].try_into().expect("8 bytes"));
        match batch_no {
            None => batch_no = Some(observed),
            Some(seen) => assert_eq!(observed, seen, "torn read: mixed batch numbers in one scan"),
        }
        indices.push(index);
    }
    indices.sort_unstable();
    assert_eq!(
        indices,
        (0..num_keys as u64).collect::<Vec<_>>(),
        "incomplete key set in one scan"
    );
    batch_no
}

fn reader_loop(runtime: &StorageRuntime<'_>, branch: BranchId, done: &AtomicBool, start: &Barrier) {
    start.wait();
    let mut max_batch = 0u64;
    let mut reads = 0u64;
    while !done.load(Ordering::Acquire) {
        if let Some(observed) = read_checked(runtime, branch, STRESS_KEYS) {
            assert!(
                observed >= max_batch,
                "visible version regressed for a reader: {observed} < {max_batch}"
            );
            max_batch = observed;
            reads += 1;
        }
        // Cooperate so the (durable) writer and the maintenance workers make progress on a busy box
        // instead of being starved by a hot read loop.
        std::thread::yield_now();
    }
    assert!(reads > 0, "reader observed no complete batches");
}

/// Cheap point read of the first key — its batch number (`None` before any batch is visible).
fn read_point_batch(runtime: &StorageRuntime<'_>, branch: BranchId) -> Option<u64> {
    let outcome = runtime
        .read_point(&PointReadRequest::new(
            branch,
            background_space(),
            key(b"stress-0000"),
            ReadBound::Latest,
        ))
        .expect("off-lock latest point read");
    let row = outcome.row()?;
    let value = row.value().expect("stress rows are puts").as_bytes();
    Some(u64::from_be_bytes(value[0..8].try_into().expect("8 bytes")))
}

fn writer_loop(runtime: &StorageRuntime<'_>, branch: BranchId, done: &AtomicBool, start: &Barrier) {
    start.wait();
    for batch_no in 1..STRESS_BATCHES {
        runtime
            .commit(&stress_batch(branch, batch_no, STRESS_KEYS))
            .expect("commit stress batch");
        // Read-your-writes: after the ack, the writer must see at least its own batch (cheap point
        // read so the writer isn't itself bottlenecked scanning a growing LSM every iteration).
        let seen = read_point_batch(runtime, branch).expect("writer read-your-writes");
        assert!(
            seen >= batch_no,
            "read-your-writes violated: saw {seen} after committing {batch_no}"
        );
    }
    done.store(true, Ordering::Release);
}

#[test]
fn off_lock_readers_race_a_committing_writer() {
    let mut runtime = open_cache();
    let branch = StorageRuntime::default_branch_id_for_test();
    // Seed batch 0 so every reader always observes a complete key set.
    runtime
        .commit(&stress_batch(branch, 0, STRESS_KEYS))
        .expect("seed batch 0");

    let done = AtomicBool::new(false);
    let start = Barrier::new(STRESS_READERS + 1);

    std::thread::scope(|scope| {
        for _ in 0..STRESS_READERS {
            scope.spawn(|| reader_loop(&runtime, branch, &done, &start));
        }
        scope.spawn(|| writer_loop(&runtime, branch, &done, &start));
    });

    // Every reader saw only atomic, monotonic batches; the final visible state is the last commit.
    let final_batch = read_checked(&runtime, branch, STRESS_KEYS).expect("final read");
    assert_eq!(
        final_batch,
        STRESS_BATCHES - 1,
        "final visible batch mismatch"
    );
    runtime.close().expect("close cache runtime");
}

#[test]
fn off_lock_fork_and_materialize_under_concurrent_reads() {
    const SEEDED: u64 = 8;
    let mut runtime = open_durable_background("off-lock-c4");
    let parent = StorageRuntime::default_branch_id_for_test();
    let child = branch_id(0x5C);
    // Seed + flush the parent so the child inherits real (flushed) tables to materialize.
    for batch_no in 0..SEEDED {
        runtime
            .commit(&stress_batch(parent, batch_no, STRESS_KEYS))
            .expect("seed parent");
    }
    runtime.wait_background_idle_for_test();
    let latest = SEEDED - 1;

    let done = AtomicBool::new(false);

    std::thread::scope(|scope| {
        // (vi) Concurrent parent readers — the parent is untouched by the child's fork/materialize.
        for _ in 0..2 {
            scope.spawn(|| {
                while !done.load(Ordering::Acquire) {
                    if let Some(observed) = read_checked(&runtime, parent, STRESS_KEYS) {
                        assert_eq!(observed, latest, "parent tore during child branching");
                    }
                }
            });
        }
        // C4 driver: fork mid-stress, read the child, materialize it while reading.
        scope.spawn(|| {
            // (v) fork is O(1) / latency-bounded (inherit-by-reference, no table copy).
            let fork_start = Instant::now();
            runtime
                .branch(&BranchRequest::new(
                    child,
                    BranchAction::ForkCurrent { source: parent },
                    Some(BranchGeneration::new(1)),
                ))
                .expect("fork child");
            assert!(
                fork_start.elapsed() < Duration::from_secs(1),
                "fork was not O(1): {:?}",
                fork_start.elapsed()
            );
            // The child immediately serves the inherited data correctly.
            let inherited =
                read_checked(&runtime, child, STRESS_KEYS).expect("child inherited read");
            assert_eq!(
                inherited, latest,
                "child did not inherit the parent's latest batch"
            );

            // (vii) materialize the child (inherited -> owned) while reading it; never tears.
            runtime
                .enqueue_maintenance(&MaintenanceRequest::new(
                    MaintenanceTask::Materialize,
                    MaintenanceScope::Branch(child),
                ))
                .expect("enqueue materialize");
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                let seen =
                    read_checked(&runtime, child, STRESS_KEYS).expect("child read while draining");
                assert_eq!(seen, latest, "child read tore during materialization");
                let layout = runtime
                    .branch_source_layout_for_test(child)
                    .expect("child source layout");
                if layout.inherited_total_tables() == 0 {
                    break; // materialization drained
                }
                assert!(
                    Instant::now() < deadline,
                    "materialization did not drain within the deadline"
                );
            }
            done.store(true, Ordering::Release);
        });
    });

    runtime.wait_background_idle_for_test();
    let layout = runtime
        .branch_source_layout_for_test(child)
        .expect("child source layout");
    assert_eq!(
        layout.inherited_total_tables(),
        0,
        "child still inherits after materialization"
    );
    assert!(
        layout.owned_total_tables() > 0,
        "child has no owned tables after materialization"
    );
    // Both branches read consistently at the end.
    assert_eq!(read_checked(&runtime, parent, STRESS_KEYS), Some(latest));
    assert_eq!(read_checked(&runtime, child, STRESS_KEYS), Some(latest));
    runtime.close().expect("close durable runtime");
}
