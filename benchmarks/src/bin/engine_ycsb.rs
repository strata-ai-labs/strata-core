//! YCSB benchmark for the engine-next KV surface.
//!
//! Runs the standard YCSB workloads A-F through the public `strata_engine_next`
//! `Database`/`KvService` API with a proper load phase + run phase, the standard
//! Zipfian/Uniform/Latest request distributions, and per-operation-type latency
//! percentiles. Workload/distribution generators are shared with the
//! competitive `ycsb_compare` bench via the `ycsb_workloads` module.
//!
//! Usage:
//!   engine-ycsb                                   # A-F, cache+durable, 100k/100k
//!   engine-ycsb --workload a,c --records 1m
//!   engine-ycsb -q                                # quick: 10k records, 10k ops
//!   engine-ycsb --mode cache --memory-budget 16g

// Link the benchmark lib for its #[global_allocator] (jemalloc): a bin that
// never references the lib does NOT link it, silently running on glibc
// malloc — whose per-thread arenas fragment unboundedly under multi-GB
// alloc/free churn (T4 RSS attribution, roadmap-v2).
extern crate strata_benchmarks;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use strata_engine_next::{
    BranchName, CacheOpenOptions, CachePreheat, Database, DurableLocalOpenOptions, EngineResult,
    KvKey, KvValue, ProductSpace,
};

// Shared YCSB workload definitions and distribution generators (no churn to the
// existing competitive bench — the file is included directly).
#[allow(dead_code)]
#[path = "../../benches/ycsb_workloads.rs"]
mod ycsb_workloads;

use ycsb_workloads::{
    dir_size_bytes, workload_by_label, ycsb_key, ycsb_value, FastRng, KeyChooser, Operation,
    ValueFill, WorkloadSpec,
};

const DEFAULT_RECORDS: usize = 100_000;
const DEFAULT_OPS: usize = 100_000;
const QUICK_RECORDS: usize = 10_000;
const QUICK_OPS: usize = 10_000;
const DEFAULT_VALUE_BYTES: usize = 1_000;
const DEFAULT_SCAN_MAX: usize = 100;
const DEFAULT_MEMORY_BUDGET_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const RUN_SEED: u64 = 0xABCD_2026;

// The load phase is benchmark setup, not the measured YCSB workload: batch its
// inserts so the durable mode does not pay one fsync per record before the run
// phase even starts. The run phase keeps one commit per operation (proper YCSB).
// Override with `--load-batch 1` to measure single-insert load throughput.
const DEFAULT_LOAD_BATCH: usize = 1_000;

/// T4 RSS attribution: jemalloc's own gauges. `allocated` = live bytes the
/// application holds; `resident` = pages jemalloc keeps from the OS (the RSS
/// driver); `retained` = unmapped-but-reusable virtual ranges. app-held vs
/// allocator-retention is `allocated` vs `resident - allocated`.
fn print_jemalloc_split(label: &str) {
    use tikv_jemalloc_ctl::{epoch, stats};
    if epoch::advance().is_err() {
        return;
    }
    let gb = |value: Result<usize, _>| match value {
        Ok(bytes) => format!("{:.2}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0)),
        Err(_) => "?".to_owned(),
    };
    eprintln!(
        "  [jemalloc {label}] allocated={} active={} resident={} retained={} mapped={}",
        gb(stats::allocated::read()),
        gb(stats::active::read()),
        gb(stats::resident::read()),
        gb(stats::retained::read()),
        gb(stats::mapped::read()),
    );
}

fn main() {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            process::exit(2);
        }
    };
    if let Err(error) = run(config) {
        eprintln!("{error:?}");
        process::exit(1);
    }
}

fn run(config: Config) -> EngineResult<()> {
    let root = std::env::current_dir()
        .unwrap_or_else(|_| ".".into())
        .join(".benchmark")
        .join("engine-ycsb");
    std::fs::create_dir_all(&root).expect("benchmark directory");

    println!(
        "engine-ycsb records={} ops={} value={}B fill={} scan_max={} budget={} modes={} workloads={}",
        config.records,
        config.ops,
        config.value_bytes,
        config.value_fill.label(),
        config.scan_max,
        format_bytes(config.memory_budget_bytes),
        config
            .mode
            .modes()
            .iter()
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join("+"),
        config
            .workloads
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(","),
    );

    let mut results = Vec::new();
    for &label in &config.workloads {
        let Some(workload) = workload_by_label(label) else {
            eprintln!("warning: unknown workload `{label}`, skipping");
            continue;
        };
        println!(
            "\n--- Workload {}: {} ({}) ---",
            workload.label.to_ascii_uppercase(),
            workload.name,
            workload.mix_label(),
        );
        for mode in config.mode.modes() {
            let tempdir = tempfile::tempdir_in(&root).expect("temporary benchmark database");
            let result = run_workload(&config, mode, workload, tempdir.path())?;
            print_result(&result);
            results.push(result);
        }
    }

    if !results.is_empty() {
        print_summary(&results);
        write_results(&config, &results);
    }
    Ok(())
}

fn run_workload(
    config: &Config,
    mode: BenchMode,
    workload: &WorkloadSpec,
    path: &Path,
) -> EngineResult<WorkloadResult> {
    let mut database = open_database(config, mode, path)?;

    // Load phase: insert `records` keys (user0..user{records-1}).
    let load_start = Instant::now();
    load(&mut database, config)?;
    let load_elapsed = load_start.elapsed();
    if mode == BenchMode::Durable {
        eprintln!(
            "  [probe] on-disk post-load: {}",
            format_bytes(dir_size_bytes(path))
        );
    }

    // Optional settle window: let background maintenance digest the load's
    // L0/compaction backlog before the run phase, so run-phase numbers measure
    // steady state instead of load-tail digestion (the backlog scales with how
    // FAST the load ran, contaminating A/B comparisons across builds).
    if config.settle_secs > 0 {
        eprintln!("  [settle] waiting {}s for background maintenance", config.settle_secs);
        std::thread::sleep(std::time::Duration::from_secs(config.settle_secs));
        if mode == BenchMode::Durable {
            eprintln!(
                "  [probe] on-disk post-settle: {}",
                format_bytes(dir_size_bytes(path))
            );
            {
                // A3 (#2524): did zone/edge cuts fire on this workload?
                let perf = strata_storage_next::perf_trace::snapshot();
                eprintln!(
                    "  [probe] a3: edge_cuts={} zone_cuts={} flush_outputs={}",
                    perf.compaction_input_edge_cuts(),
                    perf.flush_zone_cuts(),
                    perf.flush_zone_output_tables(),
                );
            }
            if config.perf_breakdown {
                // Printed BEFORE the run-phase counter reset: the settle
                // window is where the preheat fill happens.
                print_preheat_probe("post-settle");
            }
        }
    }

    // Run phase: weighted operation mix over the chosen request distribution.
    if config.perf_breakdown {
        print_jemalloc_split("post-load");
        strata_storage_next::perf_trace::reset();
    }
    // C3a (P4): thread-local jemalloc allocation delta across the
    // single-threaded run loop — the per-item gate for the C3b strip.
    let alloc_before = thread_allocated_bytes();
    let run = run_phase(&mut database, config, workload)?;
    if config.perf_breakdown {
        if let (Some(before), Some(after)) = (alloc_before, thread_allocated_bytes()) {
            let delta = after.saturating_sub(before);
            eprintln!(
                "  [probe] alloc: run_bytes={} bytes_per_op={:.1}",
                delta,
                delta as f64 / config.ops.max(1) as f64,
            );
        }
    }
    // Storage-side attribution of the run phase (requires the perf-trace
    // feature, on for benchmarks): separates commit-stage work from runtime-
    // lock wait and from background maintenance lock holds — the numbers that
    // explain durable tail latency.
    if config.perf_breakdown && mode == BenchMode::Durable {
        let perf = strata_storage_next::perf_trace::snapshot();
        eprintln!(
            "  [probe] admission: evals={} clean={} pressure_accepts={} urgent={} pressure_rejects={} retryable_rejects={} wait_attempts={} wait_timeouts={} block_wait_ms={:.1}",
            perf.lifecycle_write_admission_evaluations(),
            perf.lifecycle_write_admission_clean_accepts(),
            perf.lifecycle_write_admission_under_pressure_accepts(),
            perf.lifecycle_write_admission_urgent_accepts(),
            perf.lifecycle_write_admission_pressure_rejects(),
            perf.lifecycle_write_admission_retryable_rejects(),
            perf.lifecycle_write_admission_wait_attempts(),
            perf.lifecycle_write_admission_wait_timeouts(),
            perf.lifecycle_write_admission_block_wait_ns() as f64 / 1e6,
        );
        eprintln!(
            "  [probe] wal/ckpt: checkpoint_enqueues={} checkpoint_coalesced={} post_maint_enq={} post_maint_coalesced={}",
            perf.lifecycle_wal_checkpoint_enqueue_events(),
            perf.lifecycle_wal_checkpoint_coalesced_events(),
            perf.lifecycle_post_commit_maintenance_tasks_enqueued(),
            perf.lifecycle_post_commit_maintenance_tasks_coalesced(),
        );
        eprintln!(
            "  [probe] inline: admission_inline={} urgent_inline={} inline_maint_attempts={} inline_maint_ms={:.1}",
            perf.lifecycle_write_admission_inline_attempts(),
            perf.lifecycle_write_admission_urgent_inline_attempts(),
            perf.lifecycle_inline_maintenance_attempts(),
            perf.lifecycle_inline_maintenance_ns() as f64 / 1e6,
        );
        eprintln!(
            "  [probe] time: api_runtime_total_ms={:.0} stages_ms admit={:.0} stage={:.0} wal_append={:.0} apply={:.0} post_growth={:.0} post_maint={:.0} | fg_lock_wait_ms={:.0} bg_task_ms={:.0} bg_snapshot_lock_ms={:.0}",
            perf.api_commit_runtime_ns() as f64 / 1e6,
            perf.commit_admit_ns() as f64 / 1e6,
            perf.commit_exec_stage_ns() as f64 / 1e6,
            perf.commit_wal_append_ns() as f64 / 1e6,
            perf.commit_exec_apply_ns() as f64 / 1e6,
            perf.commit_post_wal_growth_ns() as f64 / 1e6,
            perf.commit_post_maintenance_ns() as f64 / 1e6,
            perf.lifecycle_foreground_wait_background_lock_ns() as f64 / 1e6,
            perf.lifecycle_background_task_total_ns() as f64 / 1e6,
            perf.lifecycle_background_task_snapshot_lock_ns() as f64 / 1e6,
        );
        eprintln!(
            "  [probe] bg split: rounds={} tasks_completed={} start_lock_ms={:.0} unlocked_build_ms={:.0} publish_lock_ms={:.0}",
            perf.lifecycle_background_drain_rounds(),
            perf.lifecycle_background_tasks_completed(),
            perf.lifecycle_background_task_snapshot_lock_ns() as f64 / 1e6,
            perf.lifecycle_background_task_unlocked_build_ns() as f64 / 1e6,
            perf.lifecycle_background_task_publish_lock_ns() as f64 / 1e6,
        );
        eprintln!(
            "  [probe] low-tier under lock: runs={} total_ms={:.0}",
            perf.lifecycle_background_task_low_tier_runs(),
            perf.lifecycle_background_task_low_tier_ns() as f64 / 1e6,
        );
        // W1.4a: graded pacing attribution — how much wall the token bucket
        // slept, and where the debt-adaptive rate actually sat.
        eprintln!(
            "  [probe] commit path: map_ms={:.0} batch_clone_ms={:.0} dispatch_ms={:.0} notify_ms={:.0} | wal_buf allocs={} reuses={}",
            perf.api_commit_map_ns() as f64 / 1e6,
            perf.commit_api_batch_clone_ns() as f64 / 1e6,
            perf.commit_group_dispatch_ns() as f64 / 1e6,
            perf.commit_drain_notify_ns() as f64 / 1e6,
            perf.commit_wal_encode_buffer_allocations(),
            perf.commit_wal_encode_buffer_reuses(),
        );
        eprintln!(
            "  [probe] wal buffer: appends={} flushes thr={} cap={} dur={} trk={} bytes={}",
            perf.commit_wal_buffered_appends(),
            perf.commit_wal_buffer_flushes_threshold(),
            perf.commit_wal_buffer_flushes_capture(),
            perf.commit_wal_buffer_flushes_durability(),
            perf.commit_wal_buffer_flushes_trickle(),
            perf.commit_wal_buffer_flush_bytes(),
        );
        eprintln!(
            "  [probe] block reads: trusted={} checked={} indexed={} accel_builds={} accel_rebuilds={} warm_rejects={} | api_point_ms={:.0}",
            perf.table_trusted_block_seeks(),
            perf.table_checked_block_seeks(),
            perf.table_indexed_block_seeks(),
            perf.table_accelerator_builds(),
            perf.table_accelerator_rebuilds(),
            perf.table_warm_insert_rejects(),
            perf.api_read_point_runtime_ns() as f64 / 1e6,
        );
        eprintln!(
            "  [probe] pressure collection: calls={} levels={} tables={} total_ms={:.0} sampling_skips={} full_scans={}",
            perf.lifecycle_pressure_collection_calls(),
            perf.lifecycle_pressure_collection_levels_inspected(),
            perf.lifecycle_pressure_collection_tables_inspected(),
            perf.lifecycle_pressure_collection_ns() as f64 / 1e6,
            perf.lifecycle_pressure_collection_sampling_skips(),
            perf.lifecycle_pressure_collection_full_scans(),
        );
        eprintln!(
            "  [probe] pacing: delays={} total_ms={} max_ms={} | rate recomputes={} floor_events={} last_kbps={} debt_max_mb={}",
            perf.lifecycle_graded_throttle_delays(),
            perf.lifecycle_graded_throttle_delay_ms_total(),
            perf.lifecycle_graded_throttle_delay_ms_max(),
            perf.lifecycle_admission_rate_recomputes(),
            perf.lifecycle_admission_rate_floor_events(),
            perf.lifecycle_admission_rate_last() / 1024,
            perf.lifecycle_admission_effective_debt_max() / (1024 * 1024),
        );
        // Read-path attribution (W2): branch-point fan-out is foreground-only;
        // table-level cache/filter/io counters are shared with background
        // maintenance readers — compare table_point vs table_cursor rows to
        // split lookup traffic from compaction/scan traffic.
        eprintln!(
            "  [probe] point: rows_visited={} candidates={} probes act={} frz={} l0={} nz_search={} nz={} inh_l0={} inh_nz={} seeks={} clones={} clone_kb={} | selected act={} frz={} l0={} nz={} inh={}",
            perf.point_rows_visited(),
            perf.point_candidates_materialized(),
            perf.point_active_probes(),
            perf.point_frozen_probes(),
            perf.point_owned_l0_table_probes(),
            perf.point_owned_nonzero_level_searches(),
            perf.point_owned_nonzero_table_probes(),
            perf.point_inherited_l0_table_probes(),
            perf.point_inherited_nonzero_table_probes(),
            perf.point_table_seeks(),
            perf.point_candidate_row_clones(),
            perf.point_candidate_row_clone_bytes() / 1024,
            perf.point_selected_active(),
            perf.point_selected_frozen(),
            perf.point_selected_owned_l0(),
            perf.point_selected_owned_nonzero(),
            perf.point_selected_inherited(),
        );
        eprintln!(
            "  [probe] table read: cache hit={} miss={} ins={} skip={} evict={} warm_full={} | cache_gb={:.2}/{:.2} entries={} | filter neg={} pos={} absent={} | eager neg={} pos={} unavail={} | seeks={} reader_opens={} point_rows={} cursor_rows={}",
            perf.table_cache_hits(),
            perf.table_cache_misses(),
            perf.table_cache_inserts(),
            perf.table_cache_skipped_inserts(),
            perf.table_cache_evictions(),
            perf.table_warm_publish_skipped_full(),
            perf.table_cache_bytes_gauge() as f64 / (1024.0 * 1024.0 * 1024.0),
            perf.table_cache_capacity_gauge() as f64 / (1024.0 * 1024.0 * 1024.0),
            perf.table_cache_entries_gauge(),
            perf.table_filter_negative_probes(),
            perf.table_filter_positive_probes(),
            perf.table_filter_absent_probes(),
            perf.table_eager_filter_negative_probes(),
            perf.table_eager_filter_positive_probes(),
            perf.table_eager_filter_unavailable_probes(),
            perf.table_seeks(),
            perf.table_reader_opens(),
            perf.table_point_rows_visited(),
            perf.table_cursor_rows_visited(),
        );
        eprintln!(
            "  [probe] table io: data_block reads={} read_mb={:.1} decodes={} | lazy scans={} entries={} rows_decoded={} full_decodes_avoided={} | index_mb={:.1} meta_mb={:.1}",
            perf.table_data_block_reads(),
            perf.table_data_block_read_bytes() as f64 / (1024.0 * 1024.0),
            perf.table_data_block_decodes(),
            perf.table_lazy_point_block_scans(),
            perf.table_lazy_point_entries_scanned(),
            perf.table_lazy_point_rows_decoded(),
            perf.table_lazy_point_full_block_decodes_avoided(),
            perf.table_index_read_bytes() as f64 / (1024.0 * 1024.0),
            perf.table_metadata_read_bytes() as f64 / (1024.0 * 1024.0),
        );
        print_preheat_probe("post-run");
        print_jemalloc_split("post-run");
    }

    Ok(WorkloadResult {
        workload: workload.label,
        workload_name: workload.name,
        mode: mode.as_str(),
        distribution: workload.distribution.label(),
        records: config.records,
        ops: config.ops,
        value_bytes: config.value_bytes,
        value_fill: config.value_fill.label(),
        memory_budget_bytes: config.memory_budget_bytes,
        load_ops_per_sec: rate(config.records, load_elapsed),
        load_elapsed_ms: load_elapsed.as_millis(),
        run_ops_per_sec: run.ops_per_sec,
        run_elapsed_ms: run.elapsed_ms,
        operations: run.operations,
    })
}

/// C3a (P4): this thread's cumulative jemalloc allocation counter.
fn thread_allocated_bytes() -> Option<u64> {
    tikv_jemalloc_ctl::thread::allocatedp::mib()
        .and_then(|mib| mib.read())
        .map(|counter| counter.get())
        .ok()
}

fn print_preheat_probe(label: &str) {
    let perf = strata_storage_next::perf_trace::snapshot();
    eprintln!(
        "  [probe] preheat {label}: passes={} admitted={} present={} full={} read_mb={:.1} deferred={} last_pass_blocks={} cache_gb={:.2}/{:.2}",
        perf.table_preheat_passes(),
        perf.table_preheat_blocks_admitted(),
        perf.table_preheat_blocks_skipped_present(),
        perf.table_preheat_blocks_skipped_full(),
        perf.table_preheat_bytes_read() as f64 / (1024.0 * 1024.0),
        perf.table_preheat_deferred(),
        perf.table_preheat_last_pass_blocks(),
        perf.table_cache_bytes_gauge() as f64 / (1024.0 * 1024.0 * 1024.0),
        perf.table_cache_capacity_gauge() as f64 / (1024.0 * 1024.0 * 1024.0),
    );
}

fn open_database(config: &Config, mode: BenchMode, path: &Path) -> EngineResult<Database> {
    let outcome = match mode {
        BenchMode::Cache => Database::open_cache(
            CacheOpenOptions::new().with_memory_budget(config.memory_budget_bytes),
        )?,
        BenchMode::Durable => {
            let mut options = DurableLocalOpenOptions::new()
                .with_memory_budget(config.memory_budget_bytes)
                .with_cache_preheat(config.cache_preheat);
            if let Some(bytes) = config.data_block_bytes {
                options = options.with_data_block_bytes(bytes);
            }
            Database::open_local(path, options)?
        }
    };
    Ok(outcome.into_database())
}

fn load(database: &mut Database, config: &Config) -> EngineResult<()> {
    let mut service = database.kv(default_branch(), default_space())?;
    let value = |index: usize| {
        KvValue::new(ycsb_value(config.value_fill, 0, index as u64, config.value_bytes))
    };
    if config.load_batch <= 1 {
        for index in 0..config.records {
            service.put(make_key(index), value(index))?;
        }
    } else {
        let mut batch = Vec::with_capacity(config.load_batch);
        for index in 0..config.records {
            batch.push((make_key(index), value(index)));
            if batch.len() >= config.load_batch {
                service.put_batch(std::mem::take(&mut batch))?;
            }
        }
        if !batch.is_empty() {
            service.put_batch(batch)?;
        }
    }
    Ok(())
}

struct RunPhase {
    ops_per_sec: f64,
    elapsed_ms: u128,
    operations: BTreeMap<&'static str, LatencySummary>,
}

fn run_phase(
    database: &mut Database,
    config: &Config,
    workload: &WorkloadSpec,
) -> EngineResult<RunPhase> {
    let mut rng = FastRng::new(RUN_SEED);
    let mut key_chooser = KeyChooser::new(workload.distribution, config.records.max(1));
    let mut insert_counter = config.records;
    let mut update_counter = 0u64;
    let mut update_value = || {
        update_counter += 1;
        KvValue::new(ycsb_value(config.value_fill, 1, update_counter, config.value_bytes))
    };
    let mut stats = OpStatsByType::new();

    let mut service = database.kv(default_branch(), default_space())?;
    let wall_start = Instant::now();
    for _ in 0..config.ops {
        let op = workload.choose_operation(rng.next_f64());
        let op_start = Instant::now();
        match op {
            Operation::Read => {
                let key = make_key(key_chooser.next(&mut rng));
                let _ = service.get(&key)?;
            }
            Operation::Update => {
                let key = make_key(key_chooser.next(&mut rng));
                service.put(key, update_value())?;
            }
            Operation::Insert => {
                let value = KvValue::new(ycsb_value(
                    config.value_fill,
                    0,
                    insert_counter as u64,
                    config.value_bytes,
                ));
                service.put(make_key(insert_counter), value)?;
                insert_counter += 1;
                key_chooser.set_max_key(insert_counter);
            }
            Operation::Scan => {
                let key = make_key(key_chooser.next(&mut rng));
                let len = 1 + rng.next_usize(config.scan_max);
                let _ = service.scan(Some(&key), Some(len))?;
            }
            Operation::ReadModifyWrite => {
                let key = make_key(key_chooser.next(&mut rng));
                let _ = service.get(&key)?;
                service.put(key, update_value())?;
            }
        }
        stats.record(op, op_start.elapsed().as_nanos() as u64);
    }
    let elapsed = wall_start.elapsed();

    Ok(RunPhase {
        ops_per_sec: rate(config.ops, elapsed),
        elapsed_ms: elapsed.as_millis(),
        operations: stats.into_summaries(),
    })
}

fn make_key(index: usize) -> KvKey {
    KvKey::new(ycsb_key(index).into_bytes()).expect("valid YCSB key")
}

fn default_branch() -> BranchName {
    BranchName::new("default").expect("valid branch")
}

fn default_space() -> ProductSpace {
    ProductSpace::new("default").expect("valid product space")
}

// ---------------------------------------------------------------------------
// Per-operation-type latency accounting
// ---------------------------------------------------------------------------

struct OpStatsByType {
    read: Vec<u64>,
    update: Vec<u64>,
    insert: Vec<u64>,
    scan: Vec<u64>,
    rmw: Vec<u64>,
}

impl OpStatsByType {
    fn new() -> Self {
        Self {
            read: Vec::new(),
            update: Vec::new(),
            insert: Vec::new(),
            scan: Vec::new(),
            rmw: Vec::new(),
        }
    }

    fn record(&mut self, op: Operation, nanos: u64) {
        match op {
            Operation::Read => self.read.push(nanos),
            Operation::Update => self.update.push(nanos),
            Operation::Insert => self.insert.push(nanos),
            Operation::Scan => self.scan.push(nanos),
            Operation::ReadModifyWrite => self.rmw.push(nanos),
        }
    }

    fn into_summaries(self) -> BTreeMap<&'static str, LatencySummary> {
        let mut map = BTreeMap::new();
        for (label, mut samples) in [
            ("read", self.read),
            ("update", self.update),
            ("insert", self.insert),
            ("scan", self.scan),
            ("rmw", self.rmw),
        ] {
            if let Some(summary) = LatencySummary::from_samples(&mut samples) {
                map.insert(label, summary);
            }
        }
        map
    }
}

#[derive(Serialize)]
struct LatencySummary {
    count: usize,
    avg_ns: u64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
    p999_ns: u64,
    max_ns: u64,
}

impl LatencySummary {
    fn from_samples(samples: &mut [u64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_unstable();
        let sum: u128 = samples.iter().map(|&n| n as u128).sum();
        Some(Self {
            count: samples.len(),
            avg_ns: (sum / samples.len() as u128) as u64,
            p50_ns: percentile(samples, 50.0),
            p95_ns: percentile(samples, 95.0),
            p99_ns: percentile(samples, 99.0),
            p999_ns: percentile(samples, 99.9),
            max_ns: *samples.last().expect("non-empty"),
        })
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    sorted[(rank.round() as usize).min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

fn print_result(result: &WorkloadResult) {
    println!(
        "  [{}] load={:.0} ops/s ({}ms)  run={:.0} ops/s ({}ms)",
        result.mode,
        result.load_ops_per_sec,
        result.load_elapsed_ms,
        result.run_ops_per_sec,
        result.run_elapsed_ms,
    );
    for (label, summary) in &result.operations {
        println!(
            "      {:<7} n={:<8} p50={:>9} p99={:>9} p99.9={:>9} max={:>9}",
            label,
            summary.count,
            format_nanos(summary.p50_ns),
            format_nanos(summary.p99_ns),
            format_nanos(summary.p999_ns),
            format_nanos(summary.max_ns),
        );
    }
}

fn print_summary(results: &[WorkloadResult]) {
    println!("\n=== Summary (run throughput, ops/s) ===");
    println!("  {:<12}  {:>14}  {:>14}", "workload", "cache", "durable");
    println!("  {:<12}  {:>14}  {:>14}", "--------", "-----", "-------");
    let mut labels = Vec::new();
    for r in results {
        if !labels.contains(&r.workload) {
            labels.push(r.workload);
        }
    }
    for label in labels {
        let cache = results
            .iter()
            .find(|r| r.workload == label && r.mode == "cache");
        let durable = results
            .iter()
            .find(|r| r.workload == label && r.mode == "durable");
        println!(
            "  Workload {}   {:>14}  {:>14}",
            label.to_ascii_uppercase(),
            cache.map_or("—".to_string(), |r| format!("{:.0}", r.run_ops_per_sec)),
            durable.map_or("—".to_string(), |r| format!("{:.0}", r.run_ops_per_sec)),
        );
    }
}

fn write_results(config: &Config, results: &[WorkloadResult]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("results")
        .join("engine-ycsb");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("warning: could not create results dir: {error}");
        return;
    }
    let path = dir.join(format!("engine-ycsb-{}.json", unix_seconds()));
    let payload = ResultsFile {
        benchmark: "engine-ycsb",
        records: config.records,
        ops: config.ops,
        value_bytes: config.value_bytes,
        value_fill: config.value_fill.label(),
        scan_max: config.scan_max,
        memory_budget_bytes: config.memory_budget_bytes,
        results,
    };
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => match std::fs::write(&path, json) {
            Ok(()) => println!("\nresults: {}", path.display()),
            Err(error) => eprintln!("warning: could not write results file: {error}"),
        },
        Err(error) => eprintln!("warning: could not serialize results: {error}"),
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModeSelection {
    Both,
    One(BenchMode),
}

impl ModeSelection {
    fn modes(self) -> Vec<BenchMode> {
        match self {
            Self::Both => vec![BenchMode::Cache, BenchMode::Durable],
            Self::One(mode) => vec![mode],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BenchMode {
    Cache,
    Durable,
}

impl BenchMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Durable => "durable",
        }
    }
}

struct Config {
    workloads: Vec<char>,
    mode: ModeSelection,
    records: usize,
    ops: usize,
    value_bytes: usize,
    value_fill: ValueFill,
    scan_max: usize,
    load_batch: usize,
    memory_budget_bytes: u64,
    /// Print the storage perf-trace attribution after each durable run phase.
    perf_breakdown: bool,
    settle_secs: u64,
    data_block_bytes: Option<u32>,
    cache_preheat: CachePreheat,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            workloads: vec!['a', 'b', 'c', 'd', 'e', 'f'],
            mode: ModeSelection::Both,
            records: DEFAULT_RECORDS,
            ops: DEFAULT_OPS,
            value_bytes: DEFAULT_VALUE_BYTES,
            value_fill: ValueFill::Random,
            scan_max: DEFAULT_SCAN_MAX,
            load_batch: DEFAULT_LOAD_BATCH,
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            perf_breakdown: false,
            settle_secs: 0,
            data_block_bytes: None,
            cache_preheat: CachePreheat::WhenIdle,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--workload" | "-w" => {
                    config.workloads = arg_value(&arg, args.next())?
                        .split(',')
                        .filter_map(|s| s.trim().chars().next())
                        .map(|c| c.to_ascii_lowercase())
                        .collect();
                }
                "--mode" => config.mode = parse_mode(&arg_value(&arg, args.next())?)?,
                "--cache" => config.mode = ModeSelection::One(BenchMode::Cache),
                "--durable" => config.mode = ModeSelection::One(BenchMode::Durable),
                "--records" => config.records = parse_scale(&arg, args.next())?,
                "--ops" => config.ops = parse_scale(&arg, args.next())?,
                "--value-bytes" => config.value_bytes = parse_positive_usize(&arg, args.next())?,
                "--value-fill" => {
                    let text = arg_value(&arg, args.next())?;
                    config.value_fill = ValueFill::parse(&text)
                        .ok_or_else(|| format!("`{arg}` takes `constant` or `random`"))?;
                }
                "--scan-max" => config.scan_max = parse_positive_usize(&arg, args.next())?,
                "--load-batch" => config.load_batch = parse_positive_usize(&arg, args.next())?,
                "--memory-budget" => config.memory_budget_bytes = parse_size(&arg, args.next())?,
                "--block-bytes" => {
                    config.data_block_bytes = Some(
                        u32::try_from(parse_size(&arg, args.next())?)
                            .map_err(|_| format!("`{arg}` value too large"))?,
                    );
                }
                "--preheat" => {
                    config.cache_preheat = match arg_value(&arg, args.next())?.as_str() {
                        "on" => CachePreheat::WhenIdle,
                        "off" => CachePreheat::Disabled,
                        _ => return Err(format!("`{arg}` takes `on` or `off`")),
                    };
                }
                "--perf-breakdown" => config.perf_breakdown = true,
                "--settle-secs" => {
                    config.settle_secs = arg_value(&arg, args.next())?
                        .parse()
                        .map_err(|_| format!("`{arg}` takes seconds"))?;
                }
                "-q" | "--quick" => {
                    config.records = QUICK_RECORDS;
                    config.ops = QUICK_OPS;
                }
                "-h" | "--help" => return Err(usage()),
                _ => return Err(format!("unknown argument `{arg}`\n{}", usage())),
            }
        }
        if config.workloads.is_empty() {
            return Err("no workloads selected".to_string());
        }
        Ok(config)
    }
}

fn parse_mode(value: &str) -> Result<ModeSelection, String> {
    match value {
        "both" => Ok(ModeSelection::Both),
        "cache" => Ok(ModeSelection::One(BenchMode::Cache)),
        "durable" => Ok(ModeSelection::One(BenchMode::Durable)),
        _ => Err(format!("unknown mode `{value}`\n{}", usage())),
    }
}

fn arg_value(flag: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive_usize(flag: &str, value: Option<String>) -> Result<usize, String> {
    let value = arg_value(flag, value)?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{flag} requires a positive integer, got `{value}`"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

/// Parses a count with optional decimal `k`/`m` suffix (e.g. `100k`, `1m`).
fn parse_scale(flag: &str, value: Option<String>) -> Result<usize, String> {
    let raw = arg_value(flag, value)?;
    let lower = raw.trim().to_ascii_lowercase();
    let (digits, mult): (&str, usize) = if let Some(d) = lower.strip_suffix('m') {
        (d, 1_000_000)
    } else if let Some(d) = lower.strip_suffix('k') {
        (d, 1_000)
    } else {
        (lower.as_str(), 1)
    };
    let parsed = digits
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{flag}: invalid count `{raw}`"))?;
    let scaled = parsed * mult;
    if scaled == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(scaled)
}

/// Parses a byte size with optional binary `k`/`m`/`g` (`b`) suffix (e.g. `8g`).
fn parse_size(flag: &str, value: Option<String>) -> Result<u64, String> {
    let raw = arg_value(flag, value)?;
    let lower = raw.trim().to_ascii_lowercase();
    let lower = lower.strip_suffix('b').unwrap_or(&lower);
    let (digits, mult): (&str, u64) = if let Some(d) = lower.strip_suffix('g') {
        (d, 1 << 30)
    } else if let Some(d) = lower.strip_suffix('m') {
        (d, 1 << 20)
    } else if let Some(d) = lower.strip_suffix('k') {
        (d, 1 << 10)
    } else {
        (lower, 1)
    };
    let parsed = digits
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("{flag}: invalid size `{raw}`"))?;
    let bytes = parsed.saturating_mul(mult);
    if bytes == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(bytes)
}

fn usage() -> String {
    "usage: engine-ycsb [--workload a,b,c,d,e,f] [--mode cache|durable|both] \
     [--records N] [--ops N] [--value-bytes N] [--value-fill constant|random] \
     [--scan-max N] [--load-batch N] \
     [--memory-budget SIZE] [--preheat on|off] [--perf-breakdown] [-q]\n  \
     records/ops accept k/m suffixes (decimal); SIZE accepts k/m/g (binary)."
        .to_owned()
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ResultsFile<'a> {
    benchmark: &'static str,
    records: usize,
    ops: usize,
    value_bytes: usize,
    value_fill: &'static str,
    scan_max: usize,
    memory_budget_bytes: u64,
    results: &'a [WorkloadResult],
}

#[derive(Serialize)]
struct WorkloadResult {
    workload: char,
    workload_name: &'static str,
    mode: &'static str,
    distribution: &'static str,
    records: usize,
    ops: usize,
    value_bytes: usize,
    value_fill: &'static str,
    memory_budget_bytes: u64,
    load_ops_per_sec: f64,
    load_elapsed_ms: u128,
    run_ops_per_sec: f64,
    run_elapsed_ms: u128,
    operations: BTreeMap<&'static str, LatencySummary>,
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn rate(count: usize, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds == 0.0 {
        return 0.0;
    }
    count as f64 / seconds
}

fn format_bytes(bytes: u64) -> String {
    const GIB: u64 = 1 << 30;
    const MIB: u64 = 1 << 20;
    if bytes >= GIB {
        format!("{:.0}GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.0}MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{bytes}B")
    }
}

fn format_nanos(ns: u64) -> String {
    if ns >= 1_000_000 {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.2}us", ns as f64 / 1_000.0)
    } else {
        format!("{ns}ns")
    }
}
