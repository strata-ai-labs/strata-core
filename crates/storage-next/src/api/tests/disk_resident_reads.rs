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

// ---- BS4.6 exit-gate harness ----
//
// The milestone exit gates for the disk-resident regime: a dataset far larger than the memory budget
// (1) loads and serves, (2) reopens in bounded time, and (5) recovery fully materializes no table.
// The `#[ignore]` `..._100m_on_8gib_budget` test IS gate #1/#2/#5 at the 100M×~1KB (~100GB) / 8GiB /
// ≤1s milestone target (run in the perf environment — see docs/design/performance/bs4-6-exit-runbook.md).
// `..._harness_smoke` runs the same scenario at a tiny scale so the harness logic is exercised on every
// perf-trace test run. Both go through `run_exit_gate_scenario`.

const EXIT_PREFIX: &str = "bs4-6-exit-";

/// Open a durable runtime on `budget_bytes` with inline (deterministic) maintenance: flush and
/// compaction run synchronously on the calling thread (`BackgroundExecutorMode::Inline`). The
/// larger-than-budget load is then driven without background workers, admission stalls, or run-to-run
/// timing races — identical behaviour in debug and release. Used for both the load and the timed reopen
/// so the reopen's perf counters are not perturbed by a background worker.
fn exit_open_options(budget_bytes: u64) -> StorageOpenOptions {
    StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_memory_budget(StorageMemoryBudget::new(budget_bytes).expect("valid memory budget"))
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::DeterministicInline)
}

/// Rows committed between proactive flushes: about half the active memtable pool (≈ `budget/8`, the
/// `from_total_bytes` active fraction), so the memtable is flushed to disk well before it overflows.
/// This is what keeps resident memory within budget while the on-disk dataset grows past it, and keeps
/// a single rotation comfortably inside the frozen pool at every budget tier.
fn exit_flush_interval_rows(budget_bytes: u64, value_bytes: usize) -> usize {
    let half_active_pool_bytes = budget_bytes / 8 / 2;
    usize::try_from(half_active_pool_bytes / value_bytes.max(1) as u64)
        .unwrap_or(usize::MAX)
        .max(1)
}

/// Deterministically load `total_rows` × `value` on a budget smaller than the dataset: commit in
/// batches and flush proactively (`maintenance(Flush)` rotates + flushes to an on-disk L0 table and runs
/// follow-up compaction inline) so the active memtable never overflows the budget-scaled pool — the
/// disk-resident invariant, driven synchronously. A tail flush lands every remaining row, then a
/// checkpoint bounds the reopen's WAL replay (an un-checkpointed WAL replays O(rows), unrelated to the
/// O(tables) manifest path the exit gate measures).
fn load_exit_dataset(
    runtime: &mut StorageRuntime<'static>,
    total_rows: usize,
    batch_rows: usize,
    flush_interval_rows: usize,
    value: &[u8],
) {
    let flush = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));
    let mut start = 0;
    let mut since_flush = 0;
    while start < total_rows {
        let end = (start + batch_rows).min(total_rows);
        runtime
            .commit(&background_put_batch_range(EXIT_PREFIX, start, end, value))
            .expect("exit-gate load commit");
        since_flush += end - start;
        start = end;
        if since_flush >= flush_interval_rows {
            runtime
                .maintenance(&flush)
                .expect("exit-gate periodic flush");
            since_flush = 0;
        }
    }
    runtime.maintenance(&flush).expect("exit-gate tail flush");
    let checkpoint = MaintenanceRequest::new(
        MaintenanceTask::Checkpoint,
        MaintenanceScope::Branch(branch()),
    );
    runtime
        .maintenance(&checkpoint)
        .expect("exit-gate final checkpoint");
}

/// Total size in bytes of every file under `root` — the physical on-disk dataset footprint.
fn dir_size_bytes(root: &std::path::Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(file_type) if file_type.is_dir() => stack.push(path),
                Ok(_) => {
                    if let Ok(meta) = entry.metadata() {
                        total = total.saturating_add(meta.len());
                    }
                }
                Err(_) => {}
            }
        }
    }
    total
}

/// Point-read the first/middle/last loaded keys (a full scan would materialize the whole dataset) and
/// confirm they recovered with the expected value.
fn assert_exit_sampled_reads(runtime: &StorageRuntime<'static>, total_rows: usize, value: &[u8]) {
    for index in [0, total_rows / 2, total_rows - 1] {
        let key_string = format!("{EXIT_PREFIX}{index:08}");
        let point = runtime
            .read_point(&PointReadRequest::new(
                branch(),
                background_space(),
                api_key(key_string.as_bytes()),
                ReadBound::Latest,
            ))
            .unwrap_or_else(|error| panic!("exit sampled read {key_string} failed: {error}"));
        assert_eq!(
            point
                .row()
                .unwrap_or_else(|| panic!("exit sampled read {key_string} missed after reopen"))
                .value()
                .map(StorageValue::as_bytes),
            Some(value),
            "exit sampled read {key_string} recovered the wrong value",
        );
    }
}

/// Load `total_rows` × `value_bytes` on a `budget_bytes` memory budget (dataset ≫ budget), checkpoint,
/// close, then time a cold reopen on the same budget and assert the disk-resident exit gates:
/// #1 dataset > budget loads and serves; #2 open ≤ `open_limit_ms`; #5 `lazy_full_materialization == 0`.
fn run_exit_gate_scenario(
    root: std::path::PathBuf,
    total_rows: usize,
    value_bytes: usize,
    budget_bytes: u64,
    batch_rows: usize,
    open_limit_ms: u64,
) {
    let _capture = perf_trace::begin_test_capture();
    let value = vec![0xAB_u8; value_bytes];

    // Load on the small budget, flushing proactively so the dataset lands on disk while resident memory
    // stays within budget, then checkpoint and close.
    {
        let mut runtime = StorageRuntime::open_durable_local_with_options(
            root.clone(),
            exit_open_options(budget_bytes),
        )
        .expect("open durable exit load runtime")
        .into_runtime();
        let flush_interval_rows = exit_flush_interval_rows(budget_bytes, value_bytes);
        load_exit_dataset(
            &mut runtime,
            total_rows,
            batch_rows,
            flush_interval_rows,
            &value,
        );
        runtime.close().expect("close durable exit load runtime");
    }

    // Gate #1 precondition: the physical on-disk dataset exceeds the memory budget it loaded under.
    let dataset_bytes = dir_size_bytes(&root);
    assert!(
        dataset_bytes > budget_bytes,
        "exit gate #1: on-disk dataset {dataset_bytes} B must exceed the {budget_bytes} B budget to \
         prove disk-residency (dropped to {total_rows} rows × {value_bytes} B?)",
    );

    // Gate #2: time a cold reopen on the same budget; capture only the reopen's perf counters.
    perf_trace::reset();
    let open_start = std::time::Instant::now();
    let reopened =
        StorageRuntime::open_durable_local_with_options(root, exit_open_options(budget_bytes))
            .expect("reopen exit database");
    let open_elapsed = open_start.elapsed();
    let after_open = perf_trace::snapshot();
    assert_eq!(
        reopened.summary().disposition(),
        StorageOpenDisposition::OpenedExisting,
        "the exit reopen must recover the existing durable database",
    );
    let runtime = reopened.into_runtime();

    // Gate #5: recovery opened lazy readers from the manifest (O(tables)) and materialized no table.
    assert!(
        after_open.table_reader_opens() > 0,
        "exit open must build lazy readers from the manifest",
    );
    assert_eq!(
        after_open.table_lazy_full_materializations(),
        0,
        "exit open must not fully materialize any durable table",
    );

    // Gate #2: bounded open latency (≤ 1 s at the 100M tier; generous at smoke scale).
    assert!(
        open_elapsed <= std::time::Duration::from_millis(open_limit_ms),
        "exit gate #2: DB open took {open_elapsed:?}, over the {open_limit_ms} ms limit",
    );

    // Gate #1: the reopened lazy database serves reads correctly.
    assert_exit_sampled_reads(&runtime, total_rows, &value);
}

/// Exercises the BS4.6 exit-gate scenario at a tiny scale on every perf-trace test run: ~10 MB of rows
/// on an 8 MiB budget forces disk-resident L0 tables + compaction and a bounded reopen, so a regression
/// in the harness (or in the disk-resident open path) is caught without a 100 GB load. The open-latency
/// limit is deliberately generous — this validates the harness, not open performance.
#[test]
fn durable_exit_gate_harness_smoke() {
    let root = temp_dir_for_api_test("bs4-6-exit-smoke");
    // ~10 MB of rows on an 8 MiB budget (dataset > budget), driven by proactive inline flushes so the
    // memtable stays within budget. Generous open limit — validates the harness, not open performance.
    run_exit_gate_scenario(root, 10_000, 1_024, 8 * 1024 * 1024, 100, 60_000);
}

/// BS4.6 milestone exit gate — 100M × ~1 KB (~100 GB) loads and serves on an 8 GiB budget, DB open
/// ≤ 1 s, `lazy_full_materialization == 0`. Perf environment only (loads ~100 GB): run with
/// `--ignored --release`; see docs/design/performance/bs4-6-exit-runbook.md.
#[ignore = "BS4.6 exit gate: loads ~100 GB on an 8 GiB budget; run with --ignored --release in the perf env"]
#[test]
fn durable_exit_gate_100m_on_8gib_budget() {
    let root = temp_dir_for_api_test("bs4-6-exit-100m");
    run_exit_gate_scenario(
        root,
        100_000_000,
        1_024,
        8 * 1024 * 1024 * 1024,
        1_000,
        1_000,
    );
}
