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

/// Commit `count` distinct puts (`{prefix}-{i:05}` → `v`) in a single batch, so a later flush produces
/// one durable table spanning several data blocks (256 rows/block) without a commit per row.
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

/// Number of durable L0 tables the fast-open regression test seeds and expects to reopen.
const FAST_OPEN_TABLE_COUNT: u64 = 3;

/// BS4.5b: manifest replay is O(tables) and scans no rows. After reopen, recovery builds each durable
/// table's reader from the manifest facts + row-split (metadata only): it opens exactly one lazy reader
/// per manifest table and, in release, visits zero rows through a cursor — the manifest-facts validation,
/// the flush-watermark contiguity check, and the (empty-delta) checkpoint combine are now O(metadata) or
/// debug-only oracles. Before BS4.5b, `validate_manifest_reader_facts` cursor-scanned every row of every
/// table at open (here ~300), so this `cursor_rows_visited == 0` guard catches a regression of the
/// demotion. (Checkpoint-row validation and WAL replay still touch rows via point reads — separate paths
/// outside BS4.5b's scope; the checkpoint below bounds WAL replay to a realistic, empty tail.)
#[test]
fn durable_reopen_opens_o_tables_and_scans_no_rows() {
    let _capture = perf_trace::begin_test_capture();
    let root = temp_dir_for_api_test("disk-resident-fast-open");

    // Seed three durable L0 tables. The first spans multiple data blocks (>256 rows) so the pre-BS4.5b
    // per-row cursor scan at open would have been plainly visible in `cursor_rows_visited`.
    {
        let mut runtime = open_durable_runtime(root.clone());
        commit_many_puts(&mut runtime, "wide", 300, 1000);
        runtime
            .flush_default_branch_for_test()
            .expect("flush the wide (multi-block) L0 table");
        commit_put(&mut runtime, b"t2-a", b"a", 5000);
        runtime
            .flush_default_branch_for_test()
            .expect("flush the second L0 table");
        commit_put(&mut runtime, b"t3-a", b"a", 6000);
        runtime
            .flush_default_branch_for_test()
            .expect("flush the third L0 table");
        assert_eq!(
            u64::try_from(
                runtime
                    .branch_source_layout_for_test(branch())
                    .expect("layout")
                    .owned_l0_tables()
            )
            .expect("table count fits u64"),
            FAST_OPEN_TABLE_COUNT,
        );
        // Checkpoint so the flush watermark is durable and WAL replay on reopen is bounded (the tail is
        // empty — a realistic large DB is checkpointed, not replaying its whole history). Without this,
        // reopen re-applies every committed row through the WAL, which is O(rows) by construction and
        // unrelated to the manifest-replay path BS4.5b makes O(metadata).
        let checkpoint = MaintenanceRequest::new(
            MaintenanceTask::Checkpoint,
            MaintenanceScope::Branch(branch()),
        );
        runtime.maintenance(&checkpoint).expect("checkpoint");
        runtime.close().expect("close durable runtime");
    }

    // Reset so the snapshot reflects only the reopen, not the seeding writes/flushes.
    perf_trace::reset();
    let runtime = open_durable_runtime(root);
    let after_open = perf_trace::snapshot();

    // Open is O(tables): one lazy reader open per durable table, and no full materialization.
    assert_eq!(
        after_open.table_reader_opens(),
        FAST_OPEN_TABLE_COUNT,
        "recovery must open exactly one lazy reader per durable table",
    );
    assert_eq!(after_open.table_lazy_full_materializations(), 0);

    // Release: manifest replay trusts the CRC-protected facts + row-split, so it drives no cursor over
    // any table's rows — independent of the 300+ rows on disk. Debug builds run the O(rows) oracles
    // (materialization cross-check, flush-watermark contiguity), so this is release-only, matching how
    // the repo validates release. This is the direct regression guard for the demoted per-table scan.
    #[cfg(not(debug_assertions))]
    assert_eq!(
        after_open.table_cursor_rows_visited(),
        0,
        "manifest replay must not cursor-scan any table's rows at open",
    );

    // The lazy readers still serve every read correctly after the O(metadata) open.
    assert_eq!(
        read_latest(&runtime, b"wide-00000").as_deref(),
        Some(b"v".as_ref())
    );
    assert_eq!(
        read_latest(&runtime, b"wide-00299").as_deref(),
        Some(b"v".as_ref())
    );
    assert_eq!(
        read_latest(&runtime, b"t3-a").as_deref(),
        Some(b"a".as_ref())
    );
    assert_eq!(read_latest(&runtime, b"wide-99999"), None);
}

/// BS4.4l: a durable COMPACTION output installs a lazy, disk-resident reader (not eager row-reuse). The
/// compaction reads its lazy L0 inputs by cursor and installs the merged output lazily, so it must not
/// fully materialize any table — the memory win that keeps L1+ tables (the bulk of a large dataset) off
/// the heap.
#[test]
fn durable_compacted_output_installs_lazy_and_never_materializes() {
    let _capture = perf_trace::begin_test_capture();
    let root = temp_dir_for_api_test("disk-resident-compact");

    let mut runtime = open_durable_runtime(root);
    // Two separate durable L0 tables so an explicit compaction has something to merge.
    commit_put(&mut runtime, b"cmp-a", b"alpha", 10);
    commit_put(&mut runtime, b"cmp-b", b"bravo", 20);
    runtime
        .flush_default_branch_for_test()
        .expect("flush first L0 table");
    commit_put(&mut runtime, b"cmp-c", b"carol", 30);
    commit_put(&mut runtime, b"cmp-d", b"delta", 40);
    runtime
        .flush_default_branch_for_test()
        .expect("flush second L0 table");
    assert_eq!(
        runtime
            .branch_source_layout_for_test(branch())
            .expect("layout")
            .owned_l0_tables(),
        2,
    );

    // Compact the two L0 tables into one terminal-level output. The output installs lazy (BS4.4l).
    perf_trace::reset();
    let compact =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(branch()));
    let outcome = runtime.maintenance(&compact).expect("durable compaction");
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);

    let compacted = runtime
        .branch_source_layout_for_test(branch())
        .expect("compacted layout");
    assert_eq!(compacted.owned_l0_tables(), 0);
    assert_eq!(
        compacted.owned_total_tables(),
        1,
        "the two L0 tables must merge into a single durable output"
    );

    let perf = perf_trace::snapshot();
    assert!(
        perf.table_rewrite_reader_reopens_performed() >= 1,
        "the compaction output must reopen lazily over the published object"
    );
    assert_eq!(
        perf.table_lazy_full_materializations(),
        0,
        "compaction (lazy inputs by cursor + lazy output install) must not fully materialize a table"
    );

    // Every row still reads correctly through the lazy compacted output.
    assert_eq!(
        read_latest(&runtime, b"cmp-a").as_deref(),
        Some(b"alpha".as_ref())
    );
    assert_eq!(
        read_latest(&runtime, b"cmp-d").as_deref(),
        Some(b"delta".as_ref())
    );
    assert_eq!(read_latest(&runtime, b"cmp-missing"), None);
}
