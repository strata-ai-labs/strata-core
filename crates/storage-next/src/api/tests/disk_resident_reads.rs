//! BS4.4j cold-read suite: durable tables are lazy / disk-resident after reopen.
//!
//! The flip's proof. After a durable flush + close + reopen, an installed `BranchOwnedTable` holds a
//! lazy reader over the on-disk object (not decoded rows). These tests assert reopened reads (present,
//! absent) equal a pre-close capture; that recovery + cold reads never force a full materialization
//! (`table_lazy_full_materializations == 0` — the BS4.4d guard would otherwise trip); and that the
//! block cache is cold after reopen and warms on a repeat read (miss then hit). Gated on `localfs`
//! (durable needs a filesystem) + `perf-trace` (the counters).

use super::*;

use crate::observability::perf_trace;

fn open_durable_runtime(root: std::path::PathBuf) -> StorageRuntime<'static> {
    StorageRuntime::open_local(root)
        .expect("open durable runtime")
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

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid put batch")
}

fn commit_put(runtime: &mut StorageRuntime<'static>, key: &[u8], value: &[u8], ts: u64) {
    runtime
        .commit_for_test(&put_batch(key, value), Timestamp::from_micros(ts))
        .expect("commit put");
}

fn point_request(key: &[u8]) -> PointReadRequest {
    PointReadRequest::new(branch(), engine_space(), api_key(key), ReadBound::Latest)
}

/// The latest visible value for `key`, or `None` if absent.
fn read_latest(runtime: &StorageRuntime<'static>, key: &[u8]) -> Option<Vec<u8>> {
    runtime
        .read_point(&point_request(key))
        .expect("point read")
        .row()
        .map(|row| row.value().expect("put row").as_bytes().to_vec())
}

/// Write two rows, flush them into a single durable L0 table, and close.
fn seed_durable_table(root: std::path::PathBuf) {
    let mut runtime = open_durable_runtime(root);
    commit_put(&mut runtime, b"cold-a", b"alpha", 10);
    commit_put(&mut runtime, b"cold-b", b"bravo", 20);
    runtime
        .flush_default_branch_for_test()
        .expect("flush frozen rows into a durable table");
    assert_eq!(
        runtime
            .branch_source_layout_for_test(branch())
            .expect("durable source layout")
            .owned_l0_tables(),
        1,
        "flush must install exactly one durable L0 table so the reopen exercises a lazy reader"
    );
    runtime.close().expect("close durable runtime");
}

#[test]
fn durable_cold_reads_survive_reopen_and_equal_precapture() {
    let root = temp_dir_for_api_test("disk-resident-cold-read");

    // Capture reads while the table is still resident (pre-close), then reopen cold.
    let present_before;
    let absent_before;
    {
        let mut runtime = open_durable_runtime(root.clone());
        commit_put(&mut runtime, b"cold-a", b"alpha", 10);
        commit_put(&mut runtime, b"cold-b", b"bravo", 20);
        runtime
            .flush_default_branch_for_test()
            .expect("flush into a durable table");
        assert_eq!(
            runtime
                .branch_source_layout_for_test(branch())
                .expect("durable source layout")
                .owned_l0_tables(),
            1,
        );
        present_before = read_latest(&runtime, b"cold-a");
        absent_before = read_latest(&runtime, b"cold-missing");
        runtime.close().expect("close durable runtime");
    }
    assert_eq!(present_before.as_deref(), Some(b"alpha".as_ref()));
    assert_eq!(absent_before, None);

    // Reopen: the durable table is now a lazy reader; reads fetch blocks on demand.
    let runtime = open_durable_runtime(root);
    assert_eq!(
        read_latest(&runtime, b"cold-a"),
        present_before,
        "cold read after reopen must equal the pre-close value"
    );
    assert_eq!(
        read_latest(&runtime, b"cold-b").as_deref(),
        Some(b"bravo".as_ref()),
        "every flushed row must read correctly through the lazy reader"
    );
    assert_eq!(
        read_latest(&runtime, b"cold-missing"),
        None,
        "an absent key must stay absent across reopen"
    );
}

#[test]
fn durable_cold_reads_never_fully_materialize_a_durable_table() {
    let _capture = perf_trace::begin_test_capture();
    let root = temp_dir_for_api_test("disk-resident-no-materialize");
    seed_durable_table(root.clone());

    // Recovery (from_parts, not a row scan) plus the point reads below must never fully materialize a
    // lazy durable table — the BS4.4d DenyRuntime guard bites on such a read, and this counter records
    // any that slip through it.
    let runtime = open_durable_runtime(root);
    assert_eq!(
        read_latest(&runtime, b"cold-a").as_deref(),
        Some(b"alpha".as_ref())
    );
    assert_eq!(
        read_latest(&runtime, b"cold-b").as_deref(),
        Some(b"bravo".as_ref())
    );
    assert_eq!(read_latest(&runtime, b"cold-missing"), None);

    assert_eq!(
        perf_trace::snapshot().table_lazy_full_materializations(),
        0,
        "durable recovery + cold reads must not fully materialize any table"
    );
}

#[test]
fn durable_cold_reads_populate_then_hit_the_block_cache() {
    let _capture = perf_trace::begin_test_capture();
    let root = temp_dir_for_api_test("disk-resident-cache-hit");
    seed_durable_table(root.clone());

    // After reopen the block cache is cold (recovery validation uses a no-fill cursor). The first read
    // of a key fetches and caches its data block (a miss); a repeat read of the same key hits the cache.
    let runtime = open_durable_runtime(root);
    assert_eq!(
        read_latest(&runtime, b"cold-a").as_deref(),
        Some(b"alpha".as_ref())
    );
    let after_first = perf_trace::snapshot();
    assert_eq!(
        read_latest(&runtime, b"cold-a").as_deref(),
        Some(b"alpha".as_ref())
    );
    let after_second = perf_trace::snapshot();

    assert!(
        after_first.table_cache_misses() > 0,
        "the first cold read must miss the block cache and fetch from disk"
    );
    assert!(
        after_second.table_cache_hits() > after_first.table_cache_hits(),
        "a repeat read of the same key must hit the now-warm block cache"
    );
}
