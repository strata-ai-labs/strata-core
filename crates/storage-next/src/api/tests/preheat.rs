//! C2 cache-preheat suite: the durable runtime re-fills its block cache from
//! live tables when maintenance is otherwise idle.
//!
//! Proofs: a cold reopen (no publishes) preheats via the bootstrap trigger and
//! subsequent point reads hit the cache without touching the source; a
//! re-triggered pass is presence probes only; the `Disabled` policy never
//! preheats (the same reads then miss to disk); and repeated structural
//! triggers coalesce to one pending task. Gated on `localfs` (durable needs a
//! filesystem) + `perf-trace` (the counters).
//!
//! The runtimes under test open with `EvaluateAndEnqueue` scheduling so the
//! preheat runs on the TEST thread via the explicit drain: perf-trace test
//! capture is thread-local, and under the default `Background` policy the
//! workers preheat within milliseconds of reopen on their own threads —
//! correct behavior, but uncapturable counters.

use super::*;

use crate::observability::perf_trace;

/// Reopen with deterministic maintenance (drains run on the caller's thread)
/// and an explicit preheat policy.
fn reopen_evaluate_and_enqueue(
    root: std::path::PathBuf,
    preheat: StorageCachePreheatPolicy,
) -> StorageRuntime<'static> {
    StorageRuntime::open_durable_local_with_options(
        root,
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            )
            .with_cache_preheat_policy(preheat),
    )
    .expect("reopen durable runtime")
    .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid API key")
}

/// Commit `count` distinct puts in one batch so a flush produces one durable
/// table spanning several data blocks.
fn commit_many_puts(runtime: &mut StorageRuntime<'static>, prefix: &str, count: u32, ts: u64) {
    let mutations = (0..count)
        .map(|i| CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(format!("{prefix}-{i:05}").as_bytes()),
            value: StorageValue::new(b"v".to_vec()),
            ttl: None,
        })
        .collect();
    let batch = CommitBatch::new(
        branch(),
        mutations,
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid multi-put batch");
    runtime
        .commit_for_test(&batch, Timestamp::from_micros(ts))
        .expect("commit many puts");
}

fn point_request(key: &[u8]) -> PointReadRequest {
    PointReadRequest::new(branch(), engine_space(), api_key(key), ReadBound::Latest)
}

fn read_value(runtime: &StorageRuntime<'static>, key: &[u8]) -> Vec<u8> {
    runtime
        .read_point(&point_request(key))
        .expect("point read")
        .row()
        .map(|row| row.value().expect("put row").as_bytes().to_vec())
        .expect("row present")
}

/// Seed a multi-block durable table and close with the WAL truncated, so a
/// reopen replays nothing: a recovery flush would rebuild the table and its
/// publish-time warming (W2.4) would pre-fill the cache, leaving the preheat
/// nothing to prove. The tight growth thresholds drive the full checkpoint →
/// flush-watermark → truncation chain through the drain.
fn seed_and_close(root: std::path::PathBuf) {
    let mut runtime = StorageRuntime::open_durable_local_with_options(
        root,
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            )
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(8 * 1024, 2, 3)),
    )
    .expect("open seed runtime")
    .into_runtime();
    commit_many_puts(&mut runtime, "preheat", 2_000, 10);
    runtime
        .flush_default_branch_for_test()
        .expect("flush frozen rows into a durable table");
    let growth = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);
    runtime.maintenance(&growth).expect("evaluate wal growth");
    runtime.drain_maintenance().expect("drain the growth chain");
    runtime.close().expect("clean close");
}

/// The flagship reopen proof: a cold reopen enqueues the preheat (bootstrap
/// trigger — no publish ever fires), the drain fills the cache from the
/// on-disk tables, and a point read then hits the cache with zero source
/// reads. A re-triggered follow-up pass is presence probes only.
#[test]
fn reopen_preheat_fills_cache_and_reads_hit() {
    let root = temp_dir_for_api_test("preheat-reopen-fills");
    seed_and_close(root.clone());

    let mut runtime = reopen_evaluate_and_enqueue(root, StorageCachePreheatPolicy::WhenIdle);
    let _capture = perf_trace::begin_test_capture();
    runtime.drain_maintenance().expect("drain to idle");

    let after_fill = perf_trace::snapshot();
    assert!(
        after_fill.table_preheat_passes() >= 1,
        "the reopen trigger must run at least one preheat chunk"
    );
    assert!(
        after_fill.table_preheat_blocks_admitted() > 0,
        "a cold reopen preheat must admit blocks from disk"
    );
    assert!(after_fill.table_preheat_bytes_read() > 0);
    assert_eq!(after_fill.table_preheat_blocks_skipped_full(), 0);
    assert_eq!(after_fill.table_warm_insert_rejects(), 0);

    // A cold-key point read is served from the warmed cache: no source read.
    assert_eq!(read_value(&runtime, b"preheat-01234"), b"v".to_vec());
    let after_read = perf_trace::snapshot();
    assert_eq!(
        after_read.table_data_block_reads(),
        after_fill.table_data_block_reads(),
        "a preheated read must not touch the table source"
    );
    assert!(
        after_read.table_cache_hits() > after_fill.table_cache_hits(),
        "a preheated read must hit the block cache"
    );

    // Re-trigger via a structural change (the flush task republishes, which
    // enqueues the preheat): the follow-up pass admits nothing new — the old
    // table is preheated and the fresh flush was publish-warmed.
    commit_many_puts(&mut runtime, "retrigger", 4, 20);
    let before_second = perf_trace::snapshot();
    let flush = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));
    runtime.maintenance(&flush).expect("re-trigger flush");
    runtime.drain_maintenance().expect("drain the re-trigger");
    let after_second = perf_trace::snapshot();
    assert!(
        after_second.table_preheat_passes() > before_second.table_preheat_passes(),
        "the flush install must re-trigger a preheat pass"
    );
    assert!(
        after_second.table_preheat_blocks_skipped_present()
            > before_second.table_preheat_blocks_skipped_present(),
        "the re-triggered pass must skip already-present blocks"
    );
}

/// The A/B knob: a `Disabled` reopen never preheats, and the same cold-key
/// read then misses to the table source.
#[test]
fn disabled_policy_never_preheats() {
    let root = temp_dir_for_api_test("preheat-disabled");
    seed_and_close(root.clone());

    let mut runtime = reopen_evaluate_and_enqueue(root, StorageCachePreheatPolicy::Disabled);
    let _capture = perf_trace::begin_test_capture();
    runtime.drain_maintenance().expect("drain to idle");

    let after_drain = perf_trace::snapshot();
    assert_eq!(after_drain.table_preheat_blocks_admitted(), 0);
    assert_eq!(after_drain.table_preheat_bytes_read(), 0);

    // The cold-key read pays the source read the preheat would have absorbed.
    assert_eq!(read_value(&runtime, b"preheat-01234"), b"v".to_vec());
    let after_read = perf_trace::snapshot();
    assert!(
        after_read.table_data_block_reads() > after_drain.table_data_block_reads(),
        "without preheat the cold read must miss to disk"
    );
}

/// Structural triggers arm a flag, not a queued task: however many installs
/// fire, the maintenance queue never holds a pending preheat (queue-shape
/// assertions and the close drain stay noise-free), and one drain then runs
/// the single armed fill.
#[test]
fn preheat_triggers_never_occupy_the_queue() {
    let root = temp_dir_for_api_test("preheat-flag");
    seed_and_close(root.clone());
    let mut runtime = reopen_evaluate_and_enqueue(root, StorageCachePreheatPolicy::WhenIdle);

    let flush = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));
    commit_many_puts(&mut runtime, "co-a", 600, 20);
    runtime.maintenance(&flush).expect("first flush");
    commit_many_puts(&mut runtime, "co-b", 600, 30);
    runtime.maintenance(&flush).expect("second flush");

    let preheat_pending = runtime
        .pending_lifecycle_maintenance_kinds_for_test()
        .iter()
        .filter(|kind| **kind == crate::lifecycle::MaintenanceTaskKind::CachePreheat)
        .count();
    assert_eq!(
        preheat_pending, 0,
        "triggers arm a flag; the queue must never hold a standing preheat task"
    );

    // The armed flag still yields exactly one fill pass on the next drain.
    let _capture = perf_trace::begin_test_capture();
    runtime.drain_maintenance().expect("drain the armed fill");
    let after = perf_trace::snapshot();
    assert!(after.table_preheat_passes() >= 1);
    assert!(after.table_preheat_blocks_admitted() > 0);
}
