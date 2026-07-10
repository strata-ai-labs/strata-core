//! Storage L9 scale benchmark runner.
//!
//! This binary exercises only the public `strata_storage::api` surface.
//! It is intended for large one-shot scale cells where Criterion would be the
//! wrong tool because it repeats setup-heavy workloads.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use strata_benchmarks::harness::{read_cpu_model, read_total_ram_gb};
use strata_benchmarks::schema::{
    BenchmarkMetrics, BenchmarkReport, BenchmarkResult, HardwareInfo, RunMetadata,
};
use strata_storage::api::{
    BranchAction, BranchGeneration, BranchId, BranchRequest, CommitBatch, CommitMutation,
    CommitOptions, DiagnosticsFactState, DiagnosticsRequest, DiagnosticsScope,
    DiagnosticsSourceLayoutReport, MaintenanceQueueSummary, MaintenanceRequest, MaintenanceScope,
    MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask, PointReadRequest,
    PrefixScanReadRequest, ReadBound, ReadLimit, ScanRange, ScanReadOutcome, ScanReadRequest,
    StorageApiError, StorageApiResult, StorageDurabilityPolicy, StorageKey, StorageMemoryBudget,
    StorageOpenOptions, StorageOpenOutcome, StorageRuntime, StorageSpaceId, StorageValue,
};
use strata_storage::perf_trace::{self, StoragePerfSnapshot};
use tempfile::TempDir;

const CATEGORY: &str = "storage-l9-scale";
const DEFAULT_SCALE: usize = 100_000;
const DEFAULT_VALUE_BYTES: usize = 64;
const DEFAULT_BATCH_SIZE: usize = 1_000;
const DEFAULT_SAMPLES: usize = 10_000;
const DEFAULT_BRANCH_SAMPLES: usize = 100;
const DEFAULT_SCAN_LIMIT: usize = 64;
const DEFAULT_BUCKETS: usize = 4_096;
const DEFAULT_SEED: u64 = 0x5154_5241_5441_2026;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);

fn main() {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(CliError::Help) => {
            print_help();
            return;
        }
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!();
            print_help();
            std::process::exit(2);
        }
    };

    if let Err(error) = run(config) {
        eprintln!("benchmark failed: {error}");
        let mut source = std::error::Error::source(&error);
        while let Some(error) = source {
            eprintln!("  caused by: {error}");
            source = error.source();
        }
        std::process::exit(1);
    }
}

fn run(config: Config) -> Result<(), BenchmarkError> {
    std::fs::create_dir_all(&config.root)?;
    let mut results = Vec::new();

    eprintln!("storage L9 scale benchmark");
    eprintln!("root: {}", config.root.display());
    eprintln!("scales: {}", format_list(&config.scales));
    eprintln!("engines: {}", format_list(&config.engines));
    eprintln!("workloads: {}", format_list(&config.workloads));
    eprintln!(
        "value={}B batch={} flush_every={} samples={} branch_samples={} scan_limit={}",
        config.value_bytes,
        config.batch_size,
        config
            .flush_every
            .map_or_else(|| "off".to_string(), |rows| rows.to_string()),
        config.samples,
        config.branch_samples,
        config.scan_limit
    );
    eprintln!(
        "diagnostic_source_shape={} diagnostic_final_drain={}",
        config.diagnostic_source_shape, config.diagnostic_final_drain
    );
    eprintln!(
        "memory_budget={}",
        config
            .memory_budget_bytes
            .map_or_else(|| "default".to_string(), |bytes| bytes.to_string())
    );
    eprintln!();

    for &scale in &config.scales {
        for &engine in &config.engines {
            eprintln!("== scale={} engine={} ==", format_scale(scale), engine);
            let mut open = OpenBenchRuntime::open(engine, scale, &config)?;
            let branch_id = discover_initial_branch(&mut open.runtime)?;

            let mut loaded = false;
            let mut load_phase_context = None;
            let mut source_shape_context = None;
            let mut load_result = None;
            if config.workloads.contains(&Workload::LoadSeq) || config.needs_loaded_data() {
                let result = run_load_seq(&mut open.runtime, branch_id, scale, engine, &config)?;
                loaded = true;
                load_phase_context = result.load_phase_trace;
                load_result = Some(result);
            }

            if loaded && config.should_prepare_loaded_source_shape() {
                source_shape_context = Some(prepare_loaded_source_shape(
                    &mut open.runtime,
                    branch_id,
                    scale,
                    &config,
                )?);
            }

            if let Some(result) = load_result {
                let result = result.with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                if config.workloads.contains(&Workload::LoadSeq) {
                    results.push(result.into_benchmark_result(&config));
                }
            }

            if config.workloads.contains(&Workload::PointLatest) {
                ensure_loaded(loaded, Workload::PointLatest);
                let result = run_point_latest(&open.runtime, branch_id, scale, engine, &config)?
                    .with_load_phase_context(load_phase_context)
                    .with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::PointLatestThroughput) {
                ensure_loaded(loaded, Workload::PointLatestThroughput);
                let result =
                    run_point_latest_throughput(&open.runtime, branch_id, scale, engine, &config)?
                        .with_load_phase_context(load_phase_context)
                        .with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::ScanPrefix) {
                ensure_loaded(loaded, Workload::ScanPrefix);
                let result = run_scan_prefix(&open.runtime, branch_id, scale, engine, &config)?
                    .with_load_phase_context(load_phase_context)
                    .with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::ScanRangeThroughput) {
                ensure_loaded(loaded, Workload::ScanRangeThroughput);
                let result =
                    run_scan_range_throughput(&open.runtime, branch_id, scale, engine, &config)?
                        .with_load_phase_context(load_phase_context)
                        .with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::BranchForkCurrent) {
                ensure_loaded(loaded, Workload::BranchForkCurrent);
                let result =
                    run_branch_fork_current(&mut open.runtime, branch_id, scale, engine, &config)?
                        .with_load_phase_context(load_phase_context)
                        .with_source_shape_context(source_shape_context.clone());
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            let close_start = Instant::now();
            let close = open.runtime.close()?;
            black_box(close);
            eprintln!("close: {}", format_duration(close_start.elapsed()));

            // BS4.6 exit-gate cell: after the load and a clean close, time a cold reopen of the
            // same durable directory (the "DB open ≤ 1 s at 100 M" gate, benchmark-measurable).
            if config.workloads.contains(&Workload::ReopenAfterLoad) {
                ensure_loaded(loaded, Workload::ReopenAfterLoad);
                match open.durable_root() {
                    None => eprintln!(
                        "  reopen-after-load     skipped (cache engine has no durable directory)"
                    ),
                    Some(path) => {
                        let result = run_reopen_after_load(path, engine, scale, &config)?
                            .with_load_phase_context(load_phase_context)
                            .with_source_shape_context(source_shape_context.clone());
                        print_result(&result);
                        results.push(result.into_benchmark_result(&config));
                    }
                }
            }
            eprintln!();
        }
    }

    let report = BenchmarkReport {
        schema_version: 1,
        metadata: run_metadata(),
        results,
    };
    let path = save_report(&config, &report)?;
    eprintln!("results: {}", path.display());
    Ok(())
}

fn run_load_seq(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let storage_space = storage_space()?;
    let value = vec![0x42; config.value_bytes];
    let bucket_count = bucket_count(scale);
    let start = Instant::now();
    let mut load_phase = LoadPhaseTrace::default();
    let mut written = 0usize;
    let mut next_flush_at = config.flush_every;

    while written < scale {
        let end = written.saturating_add(config.batch_size).min(scale);
        let build_start = Instant::now();
        let mut mutations = Vec::with_capacity(end - written);
        for index in written..end {
            mutations.push(CommitMutation::Put {
                storage_space: storage_space.clone(),
                key: storage_key(key_for_index(index, bucket_count))?,
                value: StorageValue::new(value.clone()),
                ttl: None,
            });
        }
        let batch = CommitBatch::new(
            branch_id,
            mutations,
            CommitOptions::default().require_conflict_check(false),
        )?;
        load_phase.record_batch_build(build_start.elapsed());
        let commit_start = Instant::now();
        let summary = runtime.commit(&batch)?;
        load_phase.record_commit_call(commit_start.elapsed());
        black_box(summary.commit_version());
        written = end;
        while let Some(flush_at) = next_flush_at {
            if written < flush_at {
                break;
            }
            let maintenance_start = Instant::now();
            let summary = runtime.maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(branch_id),
            ))?;
            load_phase
                .record_maintenance_call(maintenance_start.elapsed(), summary.rows_processed());
            if summary.status() != MaintenanceSummaryStatus::Completed {
                return Err(BenchmarkError::MaintenanceDidNotComplete {
                    after_rows: written,
                    status: summary.status(),
                    reason: summary.reason(),
                });
            }
            black_box(summary.rows_processed());
            next_flush_at = config
                .flush_every
                .and_then(|flush_every| flush_at.checked_add(flush_every));
        }
        if config.progress && written.is_multiple_of(progress_step(scale)) {
            eprintln!("  load progress: {}/{}", written, scale);
        }
    }

    let elapsed = start.elapsed();
    let perf_trace = perf_trace::snapshot();
    load_phase.record_automatic_maintenance(perf_trace);
    Ok(
        RunResult::throughput(Workload::LoadSeq, engine, scale, scale, elapsed)
            .with_load_phase_trace(load_phase)
            .with_perf_trace(perf_trace),
    )
}

fn prepare_loaded_source_shape(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    config: &Config,
) -> Result<SourceShapeContext, BenchmarkError> {
    if config.diagnostic_final_drain {
        return drain_loaded_source_shape(runtime, branch_id, scale);
    }
    observe_loaded_source_shape(runtime, branch_id, scale)
}

fn observe_loaded_source_shape(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
) -> Result<SourceShapeContext, BenchmarkError> {
    let diagnostics =
        runtime.diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(branch_id)))?;
    let layout = diagnostics.source_layout();
    if layout.state() != DiagnosticsFactState::Known {
        return Err(BenchmarkError::SourceLayoutUnavailable);
    }

    let context = SourceShapeContext::from_observed_report(
        scale,
        SourceShapeCompactionMode::AutomaticScheduling,
        diagnostics.maintenance(),
        layout,
    );
    print_source_shape_context(&context);
    Ok(context)
}

fn drain_loaded_source_shape(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
) -> Result<SourceShapeContext, BenchmarkError> {
    let flush_start = Instant::now();
    let flush = runtime.maintenance(&MaintenanceRequest::new(
        MaintenanceTask::Flush,
        MaintenanceScope::Branch(branch_id),
    ))?;
    let flush_elapsed = flush_start.elapsed();
    require_maintenance_finished(MaintenanceTask::Flush, scale, &flush)?;

    let compact_start = Instant::now();
    let compact = runtime.maintenance(&MaintenanceRequest::new(
        MaintenanceTask::Compact,
        MaintenanceScope::Branch(branch_id),
    ))?;
    let compact_elapsed = compact_start.elapsed();
    require_maintenance_finished(MaintenanceTask::Compact, scale, &compact)?;

    let diagnostics =
        runtime.diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(branch_id)))?;
    let layout = diagnostics.source_layout();
    if layout.state() != DiagnosticsFactState::Known {
        return Err(BenchmarkError::SourceLayoutUnavailable);
    }

    let context = SourceShapeContext::from_report(
        scale,
        flush,
        flush_elapsed,
        compact,
        compact_elapsed,
        SourceShapeCompactionMode::ExplicitFixedPointDrain,
        diagnostics.maintenance(),
        layout,
    );
    print_source_shape_context(&context);
    if !context.source_shape_passed {
        return Err(BenchmarkError::SourceShapeDidNotPass {
            failures: context.failures.clone(),
        });
    }
    Ok(context)
}

fn require_maintenance_finished(
    task: MaintenanceTask,
    after_rows: usize,
    summary: &MaintenanceSummary,
) -> Result<(), BenchmarkError> {
    match summary.status() {
        MaintenanceSummaryStatus::Completed | MaintenanceSummaryStatus::Deferred => Ok(()),
        status => Err(BenchmarkError::MaintenanceTaskDidNotFinish {
            task,
            after_rows,
            status,
            reason: summary.reason(),
        }),
    }
}

fn run_point_latest(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let storage_space = storage_space()?;
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x11);
    let requests = (0..config.samples)
        .map(|_| {
            let index = rng.next_usize(scale);
            Ok(PointReadRequest::new(
                branch_id,
                storage_space.clone(),
                storage_key(key_for_index(index, bucket_count))?,
                ReadBound::Latest,
            ))
        })
        .collect::<Result<Vec<_>, BenchmarkError>>()?;

    let timed = measure_requests(requests.iter(), |request| {
        let outcome = runtime.read_point(request)?;
        if outcome.row().is_none() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(outcome.row().and_then(|row| row.value()));
        Ok(())
    })?;

    Ok(
        RunResult::latency(Workload::PointLatest, engine, scale, timed)
            .with_perf_trace(perf_trace::snapshot()),
    )
}

fn run_point_latest_throughput(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let storage_space = storage_space()?;
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x33);
    let start = Instant::now();

    for _ in 0..config.samples {
        let index = rng.next_usize(scale);
        let request = PointReadRequest::new(
            branch_id,
            storage_space.clone(),
            storage_key(key_for_index(index, bucket_count))?,
            ReadBound::Latest,
        );
        let outcome = runtime.read_point(&request)?;
        if outcome.row().is_none() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(outcome.row().and_then(|row| row.value()));
    }

    Ok(RunResult::throughput(
        Workload::PointLatestThroughput,
        engine,
        scale,
        config.samples,
        start.elapsed(),
    )
    .with_perf_trace(perf_trace::snapshot()))
}

fn run_scan_prefix(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let storage_space = storage_space()?;
    let bucket_count = bucket_count(scale);
    let limit = Some(ReadLimit::new(config.scan_limit)?);
    let mut rng = FastRng::new(config.seed ^ 0x22);
    let requests = (0..config.samples)
        .map(|_| {
            let bucket = rng.next_usize(bucket_count);
            Ok(PrefixScanReadRequest::new(
                branch_id,
                storage_space.clone(),
                storage_key(prefix_for_bucket(bucket))?,
                ReadBound::Latest,
                limit,
            ))
        })
        .collect::<Result<Vec<_>, BenchmarkError>>()?;

    let timed = measure_requests(requests.iter(), |request| {
        let outcome: ScanReadOutcome = runtime.scan_prefix(request)?;
        if outcome.rows().is_empty() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(outcome.rows().len());
        Ok(())
    })?;

    Ok(
        RunResult::latency(Workload::ScanPrefix, engine, scale, timed)
            .with_perf_trace(perf_trace::snapshot()),
    )
}

fn run_scan_range_throughput(
    runtime: &StorageRuntime<'_>,
    branch_id: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let storage_space = storage_space()?;
    let bucket_count = bucket_count(scale);
    let limit = Some(ReadLimit::new(config.scan_limit)?);
    let mut rng = FastRng::new(config.seed ^ 0x44);
    let start = Instant::now();

    for _ in 0..config.samples {
        let index = rng.next_usize(scale);
        let range = ScanRange::new(Some(storage_key(key_for_index(index, bucket_count))?), None)?;
        let request = ScanReadRequest::new(
            branch_id,
            storage_space.clone(),
            range,
            ReadBound::Latest,
            limit,
        );
        let outcome: ScanReadOutcome = runtime.scan_range(&request)?;
        if outcome.rows().is_empty() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(outcome.rows().len());
    }

    Ok(RunResult::throughput(
        Workload::ScanRangeThroughput,
        engine,
        scale,
        config.samples,
        start.elapsed(),
    )
    .with_perf_trace(perf_trace::snapshot()))
}

fn run_branch_fork_current(
    runtime: &mut StorageRuntime<'_>,
    source_branch: BranchId,
    scale: usize,
    engine: Engine,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    perf_trace::reset();
    let requests = (0..config.branch_samples)
        .map(|index| {
            BranchRequest::new(
                branch_id_for_sample(index),
                BranchAction::ForkCurrent {
                    source: source_branch,
                },
                Some(BranchGeneration::new(1)),
            )
        })
        .collect::<Vec<_>>();

    let timed = measure_requests(requests.iter(), |request| {
        let outcome = runtime.branch(request)?;
        black_box(outcome.fork_version());
        Ok(())
    })?;

    Ok(
        RunResult::latency(Workload::BranchForkCurrent, engine, scale, timed)
            .with_perf_trace(perf_trace::snapshot()),
    )
}

/// BS4.6 exit-gate cell: time a cold reopen of the loaded durable directory and capture the
/// fast-open counters. The disk-resident open path is O(tables) — lazy readers built from the
/// manifest, no data decode — so `db_open_after_load_ms` must stay bounded as the scale grows
/// and `table_lazy_full_materializations` must be zero. The reopened runtime is closed without
/// serving reads; the read workloads already ran on the original open.
fn run_reopen_after_load(
    root: PathBuf,
    engine: Engine,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    let policy = engine
        .storage_policy()
        .expect("reopen-after-load requires a durable engine");
    perf_trace::reset();
    let open_start = Instant::now();
    let outcome = open_durable_runtime(root, policy, config)?;
    let open_elapsed = open_start.elapsed();
    let open_trace = perf_trace::snapshot();
    let mut runtime = outcome.into_runtime();
    runtime.close()?;

    Ok(
        RunResult::throughput(Workload::ReopenAfterLoad, engine, scale, 1, open_elapsed)
            .with_perf_trace(open_trace)
            .with_reopen_after_load_context(ReopenAfterLoadContext {
                db_open_after_load_ms: open_elapsed.as_secs_f64() * 1_000.0,
                table_reader_opens: open_trace.table_reader_opens(),
                table_lazy_full_materializations: open_trace.table_lazy_full_materializations(),
                table_data_block_reads: open_trace.table_data_block_reads(),
                replay_rows_classified: open_trace.commit_replay_rows_classified(),
                replay_source_probes: open_trace.commit_replay_source_probes(),
                replay_history_calls: open_trace.commit_replay_history_calls(),
            }),
    )
}

fn measure_requests<'a, I, T, F>(requests: I, mut f: F) -> Result<TimedSamples, BenchmarkError>
where
    I: IntoIterator<Item = &'a T>,
    T: 'a,
    F: FnMut(&T) -> Result<(), BenchmarkError>,
{
    let mut latencies = Vec::new();
    let wall = Instant::now();
    for request in requests {
        let start = Instant::now();
        f(request)?;
        latencies.push(start.elapsed());
    }
    let elapsed = wall.elapsed();
    Ok(TimedSamples::new(latencies, elapsed))
}

fn discover_initial_branch(runtime: &mut StorageRuntime<'_>) -> Result<BranchId, BenchmarkError> {
    let request = BranchRequest::new(DEFAULT_BRANCH_ID, BranchAction::List, None);
    let outcome = runtime.branch(&request)?;
    outcome
        .branches()
        .iter()
        .find(|branch| {
            matches!(
                branch.status(),
                strata_storage::api::BranchStatus::Active
            )
        })
        .map(|branch| branch.branch_id())
        .ok_or(BenchmarkError::MissingInitialBranch)
}

fn storage_space() -> StorageApiResult<StorageSpaceId> {
    StorageSpaceId::new(vec![0x20])
}

fn storage_key(bytes: Vec<u8>) -> StorageApiResult<StorageKey> {
    StorageKey::new(bytes)
}

fn bucket_count(scale: usize) -> usize {
    scale.clamp(1, DEFAULT_BUCKETS)
}

fn key_for_index(index: usize, bucket_count: usize) -> Vec<u8> {
    let bucket = index % bucket_count;
    let ordinal = index / bucket_count;
    let mut key = prefix_for_bucket(bucket);
    key.extend_from_slice(&(ordinal as u64).to_be_bytes());
    key
}

fn prefix_for_bucket(bucket: usize) -> Vec<u8> {
    let bucket = u16::try_from(bucket).expect("bucket count fits u16");
    let mut key = Vec::with_capacity(3);
    key.push(b'k');
    key.extend_from_slice(&bucket.to_be_bytes());
    key
}

fn branch_id_for_sample(index: usize) -> BranchId {
    let mut bytes = [0x42; BranchId::BYTE_LEN];
    bytes[0] = 0x90;
    let ordinal = (index as u64).to_be_bytes();
    let offset = BranchId::BYTE_LEN.saturating_sub(ordinal.len());
    bytes[offset..].copy_from_slice(&ordinal);
    BranchId::from_bytes(bytes)
}

fn progress_step(scale: usize) -> usize {
    (scale / 10).max(1)
}

fn ensure_loaded(loaded: bool, workload: Workload) {
    if !loaded {
        panic!("{workload} requires loaded data");
    }
}

fn run_metadata() -> RunMetadata {
    RunMetadata {
        timestamp: iso8601_now(),
        git_commit: git_output(["rev-parse", "--short", "HEAD"]),
        git_branch: git_output(["rev-parse", "--abbrev-ref", "HEAD"]),
        git_dirty: git_output(["status", "--porcelain"]).map(|status| !status.is_empty()),
        sdk: "rust".to_string(),
        sdk_version: env!("CARGO_PKG_VERSION").to_string(),
        hardware: HardwareInfo {
            cpu: read_cpu_model(),
            cores: std::thread::available_parallelism()
                .map(|cores| cores.get())
                .unwrap_or(0),
            ram_gb: read_total_ram_gb(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        },
    }
}

fn save_report(config: &Config, report: &BenchmarkReport) -> Result<PathBuf, BenchmarkError> {
    let dir = config.results_dir.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("results")
            .join("storage-l9")
    });
    std::fs::create_dir_all(&dir)?;
    let commit = report
        .metadata
        .git_commit
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let timestamp = report.metadata.timestamp.replace(':', "-");
    let path = dir.join(format!("{CATEGORY}-{timestamp}-{commit}.json"));
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn iso8601_now() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86_400;
    let time_of_day = secs % 86_400;
    let hours = time_of_day / 3_600;
    let minutes = (time_of_day % 3_600) / 60;
    let seconds = time_of_day % 60;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    days += 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn print_result(result: &RunResult) {
    if let Some(reopen) = result.reopen_after_load_context {
        eprintln!(
            "  {:<20} db_open_after_load_ms={:.3} reader_opens={} lazy_full_materializations={} data_block_reads={} replay_rows={} replay_probes={} replay_history_calls={}",
            result.workload,
            reopen.db_open_after_load_ms,
            reopen.table_reader_opens,
            reopen.table_lazy_full_materializations,
            reopen.table_data_block_reads,
            reopen.replay_rows_classified,
            reopen.replay_source_probes,
            reopen.replay_history_calls,
        );
        return;
    }
    match &result.measurement {
        Measurement::Throughput { elapsed, ops } => {
            eprintln!(
                "  {:<20} {:>12.0} ops/s  elapsed={}",
                result.workload,
                *ops as f64 / elapsed.as_secs_f64(),
                format_duration(*elapsed)
            );
        }
        Measurement::Latency(samples) => {
            eprintln!(
                "  {:<20} p50={} p95={} p99={} samples={} ops/s={:.0}",
                result.workload,
                format_duration(samples.p50),
                format_duration(samples.p95),
                format_duration(samples.p99),
                samples.samples,
                samples.samples as f64 / samples.elapsed.as_secs_f64()
            );
        }
    }
    if let Some(perf_trace) = result.perf_trace {
        eprintln!(
            "    perf-trace api_map_ns={} api_runtime_ns={} api_scan_runtime_ns={} api_scan_map_ns={} api_scan_bounds_ns={} validate_ns={} duplicate_key_checks={} prepare_ns={} append_validate_ns={} append_insert_ns={} absent_key_checks={} mutable_insert_checks={} commit_batches={} user_rows={} timeline_rows={} prepared_rows={} append_rows={} branch_fact_rows={} read_views={} read_view_rows={} read_view_validation_rows={} append_clones={} append_clone_rows={} conflict_sources={} point_rows_visited={} point_candidates={} point_active_probes={} point_frozen_probes={} point_owned_l0_table_probes={} point_owned_nonzero_level_searches={} point_owned_nonzero_table_probes={} point_inherited_layer_searches={} point_inherited_l0_table_probes={} point_inherited_nonzero_level_searches={} point_inherited_nonzero_table_probes={} point_table_seeks={} scan_rows_visited={} scan_candidates={} scan_cursor_seeks={} scan_cursor_rows={} scan_active_cursors={} scan_frozen_cursors={} scan_owned_l0_cursors={} scan_owned_nonzero_level_cursors={} scan_owned_nonzero_table_cursors_opened={} scan_inherited_l0_cursors={} scan_inherited_nonzero_level_cursors={} scan_inherited_nonzero_table_cursors_opened={} scan_source_cursor_seeks={} scan_rows_returned={} history_active_rows_visited={} history_frozen_rows_visited={} history_owned_l0_rows_visited={} history_owned_nonzero_rows_visited={} history_inherited_l0_rows_visited={} history_inherited_nonzero_rows_visited={} history_candidates={} timestamp_active_rows_scanned={} timestamp_frozen_rows_scanned={} timestamp_owned_l0_rows_scanned={} timestamp_owned_nonzero_rows_scanned={} timestamp_inherited_l0_rows_scanned={} timestamp_inherited_nonzero_rows_scanned={} branch_facts_active_rows={} branch_facts_frozen_rows={} branch_facts_owned_l0_rows={} branch_facts_owned_nonzero_rows={} branch_facts_inherited_l0_rows={} branch_facts_inherited_nonzero_rows={} branch_scan_source_setup_ns={} branch_scan_merge_ns={} branch_scan_min_key_ns={} branch_scan_group_key_ns={} branch_scan_candidate_ns={} branch_scan_advance_ns={} branch_scan_select_ns={} branch_scan_emit_ns={} scan_logical_key_encodes={} scan_candidate_row_clones={} scan_candidate_row_clone_bytes={} table_seeks={} table_bound_checks={} table_bound_check_ns={}",
            perf_trace.api_commit_map_ns(),
            perf_trace.api_commit_runtime_ns(),
            perf_trace.api_scan_runtime_ns(),
            perf_trace.api_scan_map_ns(),
            perf_trace.api_scan_bounds_ns(),
            perf_trace.runtime_batch_validate_ns(),
            perf_trace.runtime_duplicate_mutation_key_checks(),
            perf_trace.commit_prepare_rows_ns(),
            perf_trace.append_batch_validate_ns(),
            perf_trace.append_insert_rows_ns(),
            perf_trace.append_absent_internal_key_checks(),
            perf_trace.mutable_insert_duplicate_checks(),
            perf_trace.commit_batches_prepared(),
            perf_trace.commit_user_mutation_rows(),
            perf_trace.commit_timeline_rows_prepared(),
            perf_trace.commit_rows_prepared(),
            perf_trace.append_rows_applied(),
            perf_trace.branch_facts_rows_observed(),
            perf_trace.read_view_captures(),
            perf_trace.read_view_rows_cloned(),
            perf_trace.read_view_validation_rows_scanned(),
            perf_trace.append_staging_clones(),
            perf_trace.append_staging_rows_cloned(),
            perf_trace.conflict_sources_built(),
            perf_trace.point_rows_visited(),
            perf_trace.point_candidates_materialized(),
            perf_trace.point_active_probes(),
            perf_trace.point_frozen_probes(),
            perf_trace.point_owned_l0_table_probes(),
            perf_trace.point_owned_nonzero_level_searches(),
            perf_trace.point_owned_nonzero_table_probes(),
            perf_trace.point_inherited_layer_searches(),
            perf_trace.point_inherited_l0_table_probes(),
            perf_trace.point_inherited_nonzero_level_searches(),
            perf_trace.point_inherited_nonzero_table_probes(),
            perf_trace.point_table_seeks(),
            perf_trace.scan_rows_visited(),
            perf_trace.scan_candidates_materialized(),
            perf_trace.scan_cursor_seeks(),
            perf_trace.scan_cursor_rows_yielded(),
            perf_trace.scan_active_cursors(),
            perf_trace.scan_frozen_cursors(),
            perf_trace.scan_owned_l0_cursors(),
            perf_trace.scan_owned_nonzero_level_cursors(),
            perf_trace.scan_owned_nonzero_table_cursors_opened(),
            perf_trace.scan_inherited_l0_cursors(),
            perf_trace.scan_inherited_nonzero_level_cursors(),
            perf_trace.scan_inherited_nonzero_table_cursors_opened(),
            perf_trace.scan_source_cursor_seeks(),
            perf_trace.scan_rows_returned(),
            perf_trace.history_active_rows_visited(),
            perf_trace.history_frozen_rows_visited(),
            perf_trace.history_owned_l0_rows_visited(),
            perf_trace.history_owned_nonzero_rows_visited(),
            perf_trace.history_inherited_l0_rows_visited(),
            perf_trace.history_inherited_nonzero_rows_visited(),
            perf_trace.history_candidates_materialized(),
            perf_trace.timestamp_active_rows_scanned(),
            perf_trace.timestamp_frozen_rows_scanned(),
            perf_trace.timestamp_owned_l0_rows_scanned(),
            perf_trace.timestamp_owned_nonzero_rows_scanned(),
            perf_trace.timestamp_inherited_l0_rows_scanned(),
            perf_trace.timestamp_inherited_nonzero_rows_scanned(),
            perf_trace.branch_facts_active_rows_observed(),
            perf_trace.branch_facts_frozen_rows_observed(),
            perf_trace.branch_facts_owned_l0_rows_observed(),
            perf_trace.branch_facts_owned_nonzero_rows_observed(),
            perf_trace.branch_facts_inherited_l0_rows_observed(),
            perf_trace.branch_facts_inherited_nonzero_rows_observed(),
            perf_trace.branch_scan_source_setup_ns(),
            perf_trace.branch_scan_merge_ns(),
            perf_trace.branch_scan_min_key_ns(),
            perf_trace.branch_scan_group_key_ns(),
            perf_trace.branch_scan_candidate_ns(),
            perf_trace.branch_scan_advance_ns(),
            perf_trace.branch_scan_select_ns(),
            perf_trace.branch_scan_emit_ns(),
            perf_trace.scan_logical_key_encodes(),
            perf_trace.scan_candidate_row_clones(),
            perf_trace.scan_candidate_row_clone_bytes(),
            perf_trace.table_seeks(),
            perf_trace.table_bound_checks(),
            perf_trace.table_bound_check_ns(),
        );
        eprintln!(
            "    commit-perf wal_build_ns={} wal_records={} wal_record_rows={} wal_record_bytes={} wal_payload_bytes={} wal_row_encode_bytes={} wal_encode_buffer_allocations={} wal_encode_buffer_reuses={} wal_append_ns={} wal_appends={} wal_append_bytes={} post_wal_growth_ns={} wal_growth_facts_ns={} wal_growth_manifest_ns={} post_maintenance_ns={} exec_admission_ns={} exec_conflict_ns={} exec_stage_ns={} exec_apply_ns={} exec_publish_ns={} admit_ns={} setup_ns={} api_batch_clone_ns={} api_post_ns={} visible_publish_attempts={} visible_publish_successes={} visible_publish_failures={} gate_attempts={} gate_acquired={} gate_rejected_unresolved={} gate_rejected_active={} unresolved_records={} unresolved_durable_not_applied_records={} unresolved_applied_not_visible_records={} registry_lookups={} registry_descriptors_scanned={} branch_guard_attempts={} branch_guard_acquired={} branch_guard_rejected={} quiesce_attempts={} quiesce_acquired={} quiesce_rejected={} conflict_validation_calls={} conflict_validation_skipped={} conflict_validation_without_source={} conflict_validation_with_source={} read_facts_checked={} cas_facts_checked={} conflicts_detected={} timeline_view_rows={} timeline_timestamp_facts={} timeline_version_facts={} timeline_reconcile_calls={} timeline_reconcile_timestamp_facts={} timeline_reconcile_version_facts={} timeline_reconcile_entry_checks={} timeline_lookup_calls={} timeline_lookup_entries_scanned={} replay_classification_calls={} replay_rows_classified={} replay_history_calls={} replay_source_probes={}",
            perf_trace.commit_wal_record_build_ns(),
            perf_trace.commit_wal_records_built(),
            perf_trace.commit_wal_record_rows(),
            perf_trace.commit_wal_record_bytes(),
            perf_trace.commit_wal_payload_bytes(),
            perf_trace.commit_wal_row_encode_bytes(),
            perf_trace.commit_wal_encode_buffer_allocations(),
            perf_trace.commit_wal_encode_buffer_reuses(),
            perf_trace.commit_wal_append_ns(),
            perf_trace.commit_wal_appends(),
            perf_trace.commit_wal_append_bytes(),
            perf_trace.commit_post_wal_growth_ns(),
            perf_trace.commit_wal_growth_facts_ns(),
            perf_trace.commit_wal_growth_manifest_ns(),
            perf_trace.commit_post_maintenance_ns(),
            perf_trace.commit_exec_admission_ns(),
            perf_trace.commit_exec_conflict_ns(),
            perf_trace.commit_exec_stage_ns(),
            perf_trace.commit_exec_apply_ns(),
            perf_trace.commit_exec_publish_ns(),
            perf_trace.commit_admit_ns(),
            perf_trace.commit_setup_ns(),
            perf_trace.commit_api_batch_clone_ns(),
            perf_trace.commit_api_post_ns(),
            perf_trace.commit_visible_publish_attempts(),
            perf_trace.commit_visible_publish_successes(),
            perf_trace.commit_visible_publish_failures(),
            perf_trace.commit_unresolved_gate_admission_attempts(),
            perf_trace.commit_unresolved_gate_admission_acquired(),
            perf_trace.commit_unresolved_gate_rejected_unresolved(),
            // gate_rejected_active counter removed in BS5 (write groups made it dead)
            0_u64,
            perf_trace.commit_unresolved_records(),
            perf_trace.commit_unresolved_durable_not_applied_records(),
            perf_trace.commit_unresolved_applied_not_visible_records(),
            perf_trace.commit_branch_registry_lookups(),
            perf_trace.commit_branch_registry_descriptors_scanned(),
            perf_trace.commit_branch_guard_attempts(),
            perf_trace.commit_branch_guard_acquired(),
            perf_trace.commit_branch_guard_rejected(),
            perf_trace.commit_quiesce_attempts(),
            perf_trace.commit_quiesce_acquired(),
            perf_trace.commit_quiesce_rejected(),
            perf_trace.commit_conflict_validation_calls(),
            perf_trace.commit_conflict_validation_skipped(),
            perf_trace.commit_conflict_validation_without_source(),
            perf_trace.commit_conflict_validation_with_source(),
            perf_trace.commit_conflict_read_facts_checked(),
            perf_trace.commit_conflict_cas_facts_checked(),
            perf_trace.commit_conflicts_detected(),
            perf_trace.commit_timeline_view_rows_scanned(),
            perf_trace.commit_timeline_timestamp_facts(),
            perf_trace.commit_timeline_version_facts(),
            perf_trace.commit_timeline_reconcile_calls(),
            perf_trace.commit_timeline_reconcile_timestamp_facts(),
            perf_trace.commit_timeline_reconcile_version_facts(),
            perf_trace.commit_timeline_reconcile_entry_checks(),
            perf_trace.commit_timeline_lookup_calls(),
            perf_trace.commit_timeline_lookup_entries_scanned(),
            perf_trace.commit_replay_classification_calls(),
            perf_trace.commit_replay_rows_classified(),
            perf_trace.commit_replay_history_calls(),
            perf_trace.commit_replay_source_probes(),
        );
        eprintln!(
            "    table-io readers={} metadata_bytes={} index_bytes={} properties_bytes={} data_block_reads={} data_block_read_bytes={} data_block_decodes={} rows_decoded={} point_rows_visited={} cursor_rows_visited={} cache_hits={} cache_misses={} cache_inserts={} cache_skipped_inserts={} filter_probes={} filter_negative={} filter_positive={} filter_absent={}",
            perf_trace.table_reader_opens(),
            perf_trace.table_metadata_read_bytes(),
            perf_trace.table_index_read_bytes(),
            perf_trace.table_properties_read_bytes(),
            perf_trace.table_data_block_reads(),
            perf_trace.table_data_block_read_bytes(),
            perf_trace.table_data_block_decodes(),
            perf_trace.table_rows_decoded(),
            perf_trace.table_point_rows_visited(),
            perf_trace.table_cursor_rows_visited(),
            perf_trace.table_cache_hits(),
            perf_trace.table_cache_misses(),
            perf_trace.table_cache_inserts(),
            perf_trace.table_cache_skipped_inserts(),
            perf_trace.table_filter_probes(),
            perf_trace.table_filter_negative_probes(),
            perf_trace.table_filter_positive_probes(),
            perf_trace.table_filter_absent_probes(),
        );
        eprintln!(
            "    table-compaction merge_cursor_opens={} merge_advances={} merge_ns={} merge_input_rows={} merge_ns_per_input_row={} pre_validation_rows={} row_clones={} heap_key_clones={} source_order_key_clones={} boundary_key_allocations={} boundary_key_buffer_allocations={} boundary_key_buffer_reuses={} previous_key_buffer_allocations={} previous_key_buffer_reuses={} kept_rows={} dropped_rows={} peak_buffered_rows={} output_tables_built={} build_facts_from_streaming_metadata={} redundant_fact_decodes_avoided={} reader_reopens_performed={}",
            perf_trace.table_compaction_merge_cursor_opens(),
            perf_trace.table_compaction_merge_advances(),
            perf_trace.table_compaction_merge_ns(),
            perf_trace.table_compaction_merge_input_rows(),
            perf_trace.table_compaction_merge_ns_per_input_row(),
            perf_trace.table_compaction_pre_validation_rows_scanned(),
            perf_trace.table_compaction_row_clones(),
            perf_trace.table_compaction_heap_key_clones(),
            perf_trace.table_compaction_source_order_key_clones(),
            perf_trace.table_compaction_boundary_key_allocations(),
            perf_trace.table_compaction_boundary_key_buffer_allocations(),
            perf_trace.table_compaction_boundary_key_buffer_reuses(),
            perf_trace.table_compaction_previous_key_buffer_allocations(),
            perf_trace.table_compaction_previous_key_buffer_reuses(),
            perf_trace.table_compaction_kept_rows(),
            perf_trace.table_compaction_dropped_rows(),
            perf_trace.table_compaction_peak_buffered_rows(),
            perf_trace.table_compaction_output_tables_built(),
            perf_trace.table_build_facts_from_streaming_metadata(),
            perf_trace.table_rewrite_redundant_fact_decodes_avoided(),
            perf_trace.table_rewrite_reader_reopens_performed(),
        );
        eprintln!(
            "    lifecycle-compaction ops_completed={} l0={} l0_to_l1={} nonzero={} bottommost={} input_tables={} output_tables={} input_bytes={} output_bytes={} input_rows={} elapsed_ns={} trivial_moves={} selected={} selected_table_count={} selected_byte_count={} selected_target_bytes={} nonzero_input_selections={} nonzero_input_bytes={}",
            perf_trace.lifecycle_compaction_operations_completed(),
            perf_trace.lifecycle_compaction_l0_operations(),
            perf_trace.lifecycle_compaction_l0_to_level_one_operations(),
            perf_trace.lifecycle_compaction_nonzero_operations(),
            perf_trace.lifecycle_compaction_bottommost_operations(),
            perf_trace.lifecycle_compaction_input_tables(),
            perf_trace.lifecycle_compaction_output_tables(),
            perf_trace.lifecycle_compaction_input_bytes(),
            perf_trace.lifecycle_compaction_output_bytes(),
            perf_trace.lifecycle_compaction_input_rows(),
            perf_trace.lifecycle_compaction_elapsed_ns(),
            perf_trace.lifecycle_compaction_trivial_moves(),
            perf_trace.lifecycle_compaction_selected(),
            perf_trace.lifecycle_compaction_selected_table_count(),
            perf_trace.lifecycle_compaction_selected_byte_count(),
            perf_trace.lifecycle_compaction_selected_target_bytes(),
            perf_trace.lifecycle_compaction_nonzero_input_selections(),
            perf_trace.lifecycle_compaction_nonzero_input_bytes(),
        );
    }
    if let Some(load_phase) = result.load_phase_trace {
        eprintln!(
            "    load-phase batch_build_ns={} commit_call_ns={} maintenance_call_ns={} maintenance_runs={} maintenance_rows={} diagnostic_poll_ns={} diagnostic_polls={} automatic_maintenance_ns={} automatic_maintenance_attempts={} inline_maintenance_ns={} inline_maintenance_attempts={} background_maintenance_ns={} background_maintenance_tasks={} foreground_wait_background_lock_ns={} admission_block_wait_ns={} admission_wait_attempts={} admission_wait_timeouts={} maintenance_suggested={} maintenance_scheduled={} maintenance_coalesced={} maintenance_deferred={} wal_retained_bytes_last={} wal_retained_segments_last={} wal_retained_bytes_max={} wal_retained_segments_max={} wal_checkpoint_enqueue_events={} wal_checkpoint_coalesced_events={} checkpoint_executions={} wal_truncation_deleted_segments={} wal_truncation_protected_segments={} wal_truncation_failed_segments={}",
            load_phase.batch_build_ns,
            load_phase.commit_call_ns,
            load_phase.maintenance_call_ns,
            load_phase.maintenance_runs,
            load_phase.maintenance_rows,
            load_phase.diagnostic_poll_ns,
            load_phase.diagnostic_polls,
            load_phase.automatic_maintenance_ns,
            load_phase.automatic_maintenance_attempts,
            load_phase.inline_maintenance_ns,
            load_phase.inline_maintenance_attempts,
            load_phase.background_maintenance_ns,
            load_phase.background_maintenance_tasks,
            load_phase.foreground_wait_background_lock_ns,
            load_phase.admission_block_wait_ns,
            load_phase.admission_wait_attempts,
            load_phase.admission_wait_timeouts,
            load_phase.maintenance_suggested_tasks,
            load_phase.maintenance_scheduled_tasks,
            load_phase.maintenance_coalesced_tasks,
            load_phase.maintenance_deferred_tasks,
            optional_u64(load_phase.wal_retained_bytes_last),
            optional_u64(load_phase.wal_retained_segments_last),
            optional_u64(load_phase.wal_retained_bytes_max),
            optional_u64(load_phase.wal_retained_segments_max),
            load_phase.wal_checkpoint_enqueue_events,
            load_phase.wal_checkpoint_coalesced_events,
            load_phase.checkpoint_executions,
            load_phase.wal_truncation_deleted_segments,
            load_phase.wal_truncation_protected_segments,
            load_phase.wal_truncation_failed_segments,
        );
    }
    if let Some(source_shape) = result.source_shape_context.as_ref() {
        eprintln!(
            "    post-load-source-shape passed={} compaction_mode={} final_l0={} owned_nonzero={} inherited_l0={} inherited_nonzero={} queue_final={} queue_max={} interpretation={}",
            source_shape.source_shape_passed,
            source_shape.compaction_mode.as_str(),
            source_shape.final_layout.owned_l0_tables,
            format_level_counts(&source_shape.final_layout.owned_nonzero_level_table_counts),
            source_shape.final_layout.inherited_l0_tables,
            format_level_counts(&source_shape.final_layout.inherited_nonzero_level_table_counts),
            source_shape
                .maintenance_queue
                .map_or_else(|| "unknown".to_string(), |queue| queue.pending_tasks.to_string()),
            source_shape
                .maintenance_queue
                .map_or_else(|| "unknown".to_string(), |queue| queue.max_pending_tasks.to_string()),
            source_shape
                .source_shape_passed
                .then_some("source-shape-passed-evaluate-filter-cache")
                .unwrap_or("source-shape-failed"),
        );
    }
}

fn format_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();
    if nanos < 1_000 {
        format!("{nanos}ns")
    } else if nanos < 1_000_000 {
        format!("{:.2}us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2}ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2}s", nanos as f64 / 1_000_000_000.0)
    }
}

fn format_scale(scale: usize) -> String {
    if scale >= 1_000_000 {
        format!("{}M", scale / 1_000_000)
    } else if scale >= 1_000 {
        format!("{}K", scale / 1_000)
    } else {
        scale.to_string()
    }
}

fn format_list<T: fmt::Display>(items: &[T]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn print_help() {
    eprintln!(
        "\
storage L9 scale benchmark

Usage:
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-l9-scale -- [options]

Options:
  --scales LIST          Comma list: 100k,1m,10m,100m. Default: 100k
  --engines LIST         Comma list: cache,standard,always. Default: all
  --workloads LIST       Comma list: load-seq,point-latest,point-throughput,scan-prefix,scan-range-throughput,branch-fork-current,reopen-after-load. Default: all
  --value-bytes N        Value size in bytes. Default: 64
  --batch-size N         Mutations per L9 commit during load. Default: 1000
  --flush-every N        Run public Flush maintenance every N loaded rows. Default: off
  --samples N            Read/scan samples. Default: 10000
  --branch-samples N     Branch fork samples. Default: 100
  --scan-limit N         Prefix scan limit. Default: 64
  --seed N               Deterministic sampling seed.
  --memory-budget SIZE   Storage memory budget, e.g. 48g/512m. Default: storage default profile.
  --root PATH            Benchmark scratch root. Default: benchmarks/.benchmark/storage-l9
  --results-dir PATH     JSON output directory. Default: benchmarks/results/storage-l9
  --diagnostic-source-shape
                         Observe source layout after load-only runs without explicit maintenance.
  --diagnostic-final-drain
                         Run final Flush+Compact before read workloads and report it separately.
  --keep-dir             Keep durable scratch directories after the run.
  --progress             Print load progress.
  -h, --help             Show this help.

Examples:
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-l9-scale -- --scales 100k
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-l9-scale -- --scales 100k,1m,10m,100m --engines standard,always --samples 50000
"
    );
}

#[derive(Clone, Debug)]
struct Config {
    scales: Vec<usize>,
    engines: Vec<Engine>,
    workloads: Vec<Workload>,
    value_bytes: usize,
    batch_size: usize,
    flush_every: Option<usize>,
    samples: usize,
    branch_samples: usize,
    scan_limit: usize,
    seed: u64,
    root: PathBuf,
    results_dir: Option<PathBuf>,
    diagnostic_source_shape: bool,
    diagnostic_final_drain: bool,
    keep_dir: bool,
    progress: bool,
    memory_budget_bytes: Option<u64>,
}

impl Config {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, CliError> {
        let mut config = Self {
            scales: vec![DEFAULT_SCALE],
            engines: Engine::ALL.to_vec(),
            workloads: Workload::ALL.to_vec(),
            value_bytes: DEFAULT_VALUE_BYTES,
            batch_size: DEFAULT_BATCH_SIZE,
            flush_every: None,
            samples: DEFAULT_SAMPLES,
            branch_samples: DEFAULT_BRANCH_SAMPLES,
            scan_limit: DEFAULT_SCAN_LIMIT,
            seed: DEFAULT_SEED,
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(".benchmark")
                .join("storage-l9"),
            results_dir: None,
            diagnostic_source_shape: false,
            diagnostic_final_drain: false,
            keep_dir: false,
            progress: false,
            memory_budget_bytes: None,
        };

        let args = args.collect::<Vec<_>>();
        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "-h" | "--help" => return Err(CliError::Help),
                "--scales" => {
                    index += 1;
                    config.scales = parse_list(args.get(index), parse_scale)?;
                }
                "--engines" => {
                    index += 1;
                    config.engines = parse_list(args.get(index), Engine::parse)?;
                }
                "--workloads" => {
                    index += 1;
                    config.workloads = parse_list(args.get(index), Workload::parse)?;
                }
                "--value-bytes" => {
                    index += 1;
                    config.value_bytes = parse_usize(args.get(index), "--value-bytes")?;
                }
                "--memory-budget" => {
                    index += 1;
                    config.memory_budget_bytes =
                        Some(parse_byte_size(args.get(index), "--memory-budget")?);
                }
                "--batch-size" => {
                    index += 1;
                    config.batch_size = parse_usize(args.get(index), "--batch-size")?;
                }
                "--flush-every" => {
                    index += 1;
                    config.flush_every = Some(parse_usize(args.get(index), "--flush-every")?);
                }
                "--samples" => {
                    index += 1;
                    config.samples = parse_usize(args.get(index), "--samples")?;
                }
                "--branch-samples" => {
                    index += 1;
                    config.branch_samples = parse_usize(args.get(index), "--branch-samples")?;
                }
                "--scan-limit" => {
                    index += 1;
                    config.scan_limit = parse_usize(args.get(index), "--scan-limit")?;
                }
                "--seed" => {
                    index += 1;
                    config.seed = parse_u64(args.get(index), "--seed")?;
                }
                "--root" => {
                    index += 1;
                    config.root = PathBuf::from(value(args.get(index), "--root")?);
                }
                "--results-dir" => {
                    index += 1;
                    config.results_dir =
                        Some(PathBuf::from(value(args.get(index), "--results-dir")?));
                }
                "--diagnostic-source-shape" => config.diagnostic_source_shape = true,
                "--diagnostic-final-drain" => config.diagnostic_final_drain = true,
                "--keep-dir" => config.keep_dir = true,
                "--progress" => config.progress = true,
                other => return Err(CliError::UnknownFlag(other.to_string())),
            }
            index += 1;
        }

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), CliError> {
        if self.scales.is_empty() {
            return Err(CliError::EmptyList("--scales"));
        }
        if self.engines.is_empty() {
            return Err(CliError::EmptyList("--engines"));
        }
        if self.workloads.is_empty() {
            return Err(CliError::EmptyList("--workloads"));
        }
        if self.value_bytes == 0 {
            return Err(CliError::InvalidNumber("--value-bytes"));
        }
        if self.batch_size == 0 {
            return Err(CliError::InvalidNumber("--batch-size"));
        }
        if self.flush_every == Some(0) {
            return Err(CliError::InvalidNumber("--flush-every"));
        }
        if self.samples == 0 {
            return Err(CliError::InvalidNumber("--samples"));
        }
        if self.branch_samples == 0 {
            return Err(CliError::InvalidNumber("--branch-samples"));
        }
        if self.scan_limit == 0 {
            return Err(CliError::InvalidNumber("--scan-limit"));
        }
        Ok(())
    }

    fn needs_loaded_data(&self) -> bool {
        self.workloads
            .iter()
            .any(|workload| workload.requires_loaded_data())
    }

    fn should_prepare_loaded_source_shape(&self) -> bool {
        self.needs_loaded_data() || self.diagnostic_source_shape || self.diagnostic_final_drain
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Engine {
    Cache,
    DurableStandard,
    DurableAlways,
}

impl Engine {
    const ALL: [Self; 3] = [Self::Cache, Self::DurableStandard, Self::DurableAlways];

    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "cache" | "ephemeral" => Ok(Self::Cache),
            "standard" | "durable-standard" => Ok(Self::DurableStandard),
            "always" | "durable-always" => Ok(Self::DurableAlways),
            _ => Err(CliError::InvalidEngine(value.to_string())),
        }
    }

    const fn storage_policy(self) -> Option<StorageDurabilityPolicy> {
        match self {
            Self::Cache => None,
            Self::DurableStandard => Some(StorageDurabilityPolicy::Standard),
            Self::DurableAlways => Some(StorageDurabilityPolicy::Always),
        }
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cache => "cache",
            Self::DurableStandard => "standard",
            Self::DurableAlways => "always",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Workload {
    LoadSeq,
    PointLatest,
    PointLatestThroughput,
    ScanPrefix,
    ScanRangeThroughput,
    BranchForkCurrent,
    ReopenAfterLoad,
}

impl Workload {
    const ALL: [Self; 7] = [
        Self::LoadSeq,
        Self::PointLatest,
        Self::PointLatestThroughput,
        Self::ScanPrefix,
        Self::ScanRangeThroughput,
        Self::BranchForkCurrent,
        Self::ReopenAfterLoad,
    ];

    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "load-seq" => Ok(Self::LoadSeq),
            "point-latest" => Ok(Self::PointLatest),
            "point-throughput" | "point-latest-throughput" | "random-read" => {
                Ok(Self::PointLatestThroughput)
            }
            "scan-prefix" => Ok(Self::ScanPrefix),
            "scan-range-throughput" | "range-scan-throughput" | "range-scan" => {
                Ok(Self::ScanRangeThroughput)
            }
            "branch-fork-current" | "branch-fork" => Ok(Self::BranchForkCurrent),
            "reopen-after-load" | "open-after-load" => Ok(Self::ReopenAfterLoad),
            _ => Err(CliError::InvalidWorkload(value.to_string())),
        }
    }

    const fn requires_loaded_data(self) -> bool {
        !matches!(self, Self::LoadSeq)
    }
}

impl fmt::Display for Workload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LoadSeq => "load-seq",
            Self::PointLatest => "point-latest",
            Self::PointLatestThroughput => "point-latest-throughput",
            Self::ScanPrefix => "scan-prefix",
            Self::ScanRangeThroughput => "scan-range-throughput",
            Self::BranchForkCurrent => "branch-fork-current",
            Self::ReopenAfterLoad => "reopen-after-load",
        })
    }
}

/// Open durable-local storage, applying an explicit `--memory-budget` when set
/// (so the benchmark can reproduce a specific operating point such as the YCSB
/// 48 GiB baseline); otherwise the storage default resource profile is used.
fn open_durable_runtime(
    path: PathBuf,
    policy: StorageDurabilityPolicy,
    config: &Config,
) -> StorageApiResult<StorageOpenOutcome<'static>> {
    match config.memory_budget_bytes {
        Some(bytes) => {
            let options = StorageOpenOptions::durable_local(policy)
                .with_memory_budget(StorageMemoryBudget::new(bytes)?);
            StorageRuntime::open_durable_local_with_options(path, options)
        }
        None => StorageRuntime::open_durable_local(path, policy),
    }
}

struct OpenBenchRuntime {
    runtime: StorageRuntime<'static>,
    _tempdir: Option<TempDir>,
    _kept_dir: Option<PathBuf>,
}

impl OpenBenchRuntime {
    fn open(engine: Engine, scale: usize, config: &Config) -> Result<Self, BenchmarkError> {
        match engine.storage_policy() {
            None => {
                let outcome = StorageRuntime::open_ephemeral()?;
                Ok(Self::from_outcome(outcome, None, None))
            }
            Some(policy) => {
                if config.keep_dir {
                    let path =
                        config
                            .root
                            .join(format!("{}-{}-{}", engine, scale, unix_nanos_now()));
                    std::fs::create_dir_all(&path)?;
                    let outcome = open_durable_runtime(path.clone(), policy, config)?;
                    Ok(Self::from_outcome(outcome, None, Some(path)))
                } else {
                    let tempdir = tempfile::tempdir_in(&config.root)?;
                    let outcome =
                        open_durable_runtime(tempdir.path().to_path_buf(), policy, config)?;
                    Ok(Self::from_outcome(outcome, Some(tempdir), None))
                }
            }
        }
    }

    fn from_outcome(
        outcome: StorageOpenOutcome<'static>,
        tempdir: Option<TempDir>,
        kept_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            runtime: outcome.into_runtime(),
            _tempdir: tempdir,
            _kept_dir: kept_dir,
        }
    }

    /// The on-disk root of a durable engine's database (kept dir or live tempdir), `None` for the
    /// cache engine. Used by the reopen-after-load cell; the tempdir stays alive until this struct
    /// drops, so the returned path outlives the reopen.
    fn durable_root(&self) -> Option<PathBuf> {
        self._kept_dir.clone().or_else(|| {
            self._tempdir
                .as_ref()
                .map(|tempdir| tempdir.path().to_path_buf())
        })
    }
}

/// Fast-open evidence captured by the reopen-after-load cell (BS4.6 exit gate #2/#5): the timed
/// cold open plus the counters proving the open was O(tables) with no full table materialization.
#[derive(Clone, Copy, Debug)]
struct ReopenAfterLoadContext {
    db_open_after_load_ms: f64,
    table_reader_opens: u64,
    table_lazy_full_materializations: u64,
    table_data_block_reads: u64,
    replay_rows_classified: u64,
    replay_source_probes: u64,
    replay_history_calls: u64,
}

#[derive(Debug)]
struct RunResult {
    workload: Workload,
    engine: Engine,
    scale: usize,
    measurement: Measurement,
    perf_trace: Option<StoragePerfSnapshot>,
    load_phase_trace: Option<LoadPhaseTrace>,
    source_shape_context: Option<SourceShapeContext>,
    reopen_after_load_context: Option<ReopenAfterLoadContext>,
}

impl RunResult {
    const fn throughput(
        workload: Workload,
        engine: Engine,
        scale: usize,
        ops: usize,
        elapsed: Duration,
    ) -> Self {
        Self {
            workload,
            engine,
            scale,
            measurement: Measurement::Throughput { elapsed, ops },
            perf_trace: None,
            load_phase_trace: None,
            source_shape_context: None,
            reopen_after_load_context: None,
        }
    }

    const fn latency(
        workload: Workload,
        engine: Engine,
        scale: usize,
        samples: TimedSamples,
    ) -> Self {
        Self {
            workload,
            engine,
            scale,
            measurement: Measurement::Latency(samples),
            perf_trace: None,
            load_phase_trace: None,
            source_shape_context: None,
            reopen_after_load_context: None,
        }
    }

    const fn with_perf_trace(mut self, perf_trace: StoragePerfSnapshot) -> Self {
        self.perf_trace = Some(perf_trace);
        self
    }

    const fn with_reopen_after_load_context(mut self, context: ReopenAfterLoadContext) -> Self {
        self.reopen_after_load_context = Some(context);
        self
    }

    const fn with_load_phase_trace(mut self, load_phase_trace: LoadPhaseTrace) -> Self {
        self.load_phase_trace = Some(load_phase_trace);
        self
    }

    fn with_load_phase_context(mut self, load_phase_trace: Option<LoadPhaseTrace>) -> Self {
        if self.load_phase_trace.is_none() {
            self.load_phase_trace = load_phase_trace;
        }
        self
    }

    fn with_source_shape_context(
        mut self,
        source_shape_context: Option<SourceShapeContext>,
    ) -> Self {
        self.source_shape_context = source_shape_context;
        self
    }

    fn into_benchmark_result(self, config: &Config) -> BenchmarkResult {
        let mut parameters = HashMap::new();
        let operation_count = self.measurement.operation_count();
        let load_phase_trace = self.load_phase_trace;
        let source_shape_context = self.source_shape_context;
        parameters.insert(
            "engine".to_string(),
            serde_json::json!(self.engine.to_string()),
        );
        parameters.insert("scale_keys".to_string(), serde_json::json!(self.scale));
        parameters.insert(
            "value_bytes".to_string(),
            serde_json::json!(config.value_bytes),
        );
        parameters.insert(
            "batch_size".to_string(),
            serde_json::json!(config.batch_size),
        );
        parameters.insert(
            "flush_every".to_string(),
            serde_json::json!(config.flush_every),
        );
        parameters.insert("samples".to_string(), serde_json::json!(config.samples));
        parameters.insert(
            "branch_samples".to_string(),
            serde_json::json!(config.branch_samples),
        );
        parameters.insert(
            "scan_limit".to_string(),
            serde_json::json!(config.scan_limit),
        );
        parameters.insert("seed".to_string(), serde_json::json!(config.seed));
        parameters.insert(
            "diagnostic_source_shape".to_string(),
            serde_json::json!(config.diagnostic_source_shape),
        );
        parameters.insert(
            "diagnostic_final_drain".to_string(),
            serde_json::json!(config.diagnostic_final_drain),
        );
        if let Some(load_phase) = load_phase_trace {
            parameters.insert(
                "load_phase_trace".to_string(),
                serde_json::json!({
                    "batch_build_ns": load_phase.batch_build_ns,
                    "commit_call_ns": load_phase.commit_call_ns,
                    "maintenance_call_ns": load_phase.maintenance_call_ns,
                    "maintenance_runs": load_phase.maintenance_runs,
                    "maintenance_rows": load_phase.maintenance_rows,
                    "diagnostic_poll_ns": load_phase.diagnostic_poll_ns,
                    "diagnostic_polls": load_phase.diagnostic_polls,
                    "automatic_maintenance_ns": load_phase.automatic_maintenance_ns,
                    "automatic_maintenance_attempts": load_phase.automatic_maintenance_attempts,
                    "inline_maintenance_ns": load_phase.inline_maintenance_ns,
                    "inline_maintenance_attempts": load_phase.inline_maintenance_attempts,
                    "background_maintenance_ns": load_phase.background_maintenance_ns,
                    "background_maintenance_tasks": load_phase.background_maintenance_tasks,
                    "foreground_wait_background_lock_ns": load_phase.foreground_wait_background_lock_ns,
                    "admission_block_wait_ns": load_phase.admission_block_wait_ns,
                    "admission_wait_attempts": load_phase.admission_wait_attempts,
                    "admission_wait_timeouts": load_phase.admission_wait_timeouts,
                    "maintenance_suggested_tasks": load_phase.maintenance_suggested_tasks,
                    "maintenance_scheduled_tasks": load_phase.maintenance_scheduled_tasks,
                    "maintenance_coalesced_tasks": load_phase.maintenance_coalesced_tasks,
                    "maintenance_deferred_tasks": load_phase.maintenance_deferred_tasks,
                    "wal_retained_bytes_last": load_phase.wal_retained_bytes_last,
                    "wal_retained_segments_last": load_phase.wal_retained_segments_last,
                    "wal_retained_bytes_max": load_phase.wal_retained_bytes_max,
                    "wal_retained_segments_max": load_phase.wal_retained_segments_max,
                    "wal_commits_since_checkpoint_last": load_phase.wal_commits_since_checkpoint_last,
                    "wal_retention_limit_bytes": load_phase.wal_retention_limit_bytes,
                    "wal_retention_limit_segments": load_phase.wal_retention_limit_segments,
                    "wal_checkpoint_enqueue_events": load_phase.wal_checkpoint_enqueue_events,
                    "wal_checkpoint_coalesced_events": load_phase.wal_checkpoint_coalesced_events,
                    "checkpoint_executions": load_phase.checkpoint_executions,
                    "wal_truncation_deleted_segments": load_phase.wal_truncation_deleted_segments,
                    "wal_truncation_protected_segments": load_phase.wal_truncation_protected_segments,
                    "wal_truncation_failed_segments": load_phase.wal_truncation_failed_segments,
                }),
            );
        }
        if let Some(perf_trace) = self.perf_trace {
            parameters.insert("perf_trace".to_string(), perf_trace_json(perf_trace));
            parameters.insert(
                "source_shape_metrics".to_string(),
                source_shape_metrics_json(
                    self.scale,
                    operation_count,
                    config.value_bytes as u64,
                    perf_trace,
                    load_phase_trace,
                    source_shape_context.as_ref(),
                ),
            );
        }
        if let Some(source_shape) = source_shape_context.as_ref() {
            parameters.insert(
                "post_load_source_shape".to_string(),
                source_shape_context_json(source_shape),
            );
        }
        if let Some(reopen) = self.reopen_after_load_context {
            parameters.insert(
                "db_open_after_load_ms".to_string(),
                serde_json::json!(reopen.db_open_after_load_ms),
            );
            parameters.insert(
                "reopen_table_reader_opens".to_string(),
                serde_json::json!(reopen.table_reader_opens),
            );
            parameters.insert(
                "reopen_table_lazy_full_materializations".to_string(),
                serde_json::json!(reopen.table_lazy_full_materializations),
            );
            parameters.insert(
                "reopen_table_data_block_reads".to_string(),
                serde_json::json!(reopen.table_data_block_reads),
            );
        }

        BenchmarkResult {
            benchmark: format!("storage-l9/{}", self.workload),
            category: CATEGORY.to_string(),
            parameters,
            metrics: self.measurement.into_metrics(),
        }
    }
}

fn perf_trace_json(perf_trace: StoragePerfSnapshot) -> serde_json::Value {
    let mut trace = serde_json::Map::new();
    macro_rules! field {
        ($name:literal, $value:expr) => {
            trace.insert($name.to_string(), serde_json::json!($value));
        };
    }

    field!("api_commit_map_ns", perf_trace.api_commit_map_ns());
    field!("api_commit_runtime_ns", perf_trace.api_commit_runtime_ns());
    field!("api_scan_runtime_ns", perf_trace.api_scan_runtime_ns());
    field!("api_scan_map_ns", perf_trace.api_scan_map_ns());
    field!("api_scan_bounds_ns", perf_trace.api_scan_bounds_ns());
    field!(
        "runtime_batch_validate_ns",
        perf_trace.runtime_batch_validate_ns()
    );
    field!(
        "runtime_duplicate_mutation_key_checks",
        perf_trace.runtime_duplicate_mutation_key_checks()
    );
    field!(
        "commit_prepare_rows_ns",
        perf_trace.commit_prepare_rows_ns()
    );
    field!(
        "append_batch_validate_ns",
        perf_trace.append_batch_validate_ns()
    );
    field!("append_insert_rows_ns", perf_trace.append_insert_rows_ns());
    field!(
        "append_absent_internal_key_checks",
        perf_trace.append_absent_internal_key_checks()
    );
    field!(
        "mutable_insert_duplicate_checks",
        perf_trace.mutable_insert_duplicate_checks()
    );
    field!(
        "commit_batches_prepared",
        perf_trace.commit_batches_prepared()
    );
    field!(
        "commit_user_mutation_rows",
        perf_trace.commit_user_mutation_rows()
    );
    field!(
        "commit_timeline_rows_prepared",
        perf_trace.commit_timeline_rows_prepared()
    );
    field!("commit_rows_prepared", perf_trace.commit_rows_prepared());
    field!(
        "commit_wal_record_build_ns",
        perf_trace.commit_wal_record_build_ns()
    );
    field!(
        "commit_wal_records_built",
        perf_trace.commit_wal_records_built()
    );
    field!(
        "commit_wal_record_rows",
        perf_trace.commit_wal_record_rows()
    );
    field!(
        "commit_wal_record_bytes",
        perf_trace.commit_wal_record_bytes()
    );
    field!(
        "commit_wal_payload_bytes",
        perf_trace.commit_wal_payload_bytes()
    );
    field!(
        "commit_wal_row_encode_bytes",
        perf_trace.commit_wal_row_encode_bytes()
    );
    field!(
        "commit_wal_encode_buffer_allocations",
        perf_trace.commit_wal_encode_buffer_allocations()
    );
    field!(
        "commit_wal_encode_buffer_reuses",
        perf_trace.commit_wal_encode_buffer_reuses()
    );
    field!("commit_wal_append_ns", perf_trace.commit_wal_append_ns());
    field!("commit_wal_appends", perf_trace.commit_wal_appends());
    field!(
        "commit_wal_append_bytes",
        perf_trace.commit_wal_append_bytes()
    );
    field!(
        "commit_post_wal_growth_ns",
        perf_trace.commit_post_wal_growth_ns()
    );
    field!(
        "commit_wal_growth_facts_ns",
        perf_trace.commit_wal_growth_facts_ns()
    );
    field!(
        "commit_wal_growth_manifest_ns",
        perf_trace.commit_wal_growth_manifest_ns()
    );
    field!(
        "commit_post_maintenance_ns",
        perf_trace.commit_post_maintenance_ns()
    );
    field!(
        "commit_exec_admission_ns",
        perf_trace.commit_exec_admission_ns()
    );
    field!(
        "commit_exec_conflict_ns",
        perf_trace.commit_exec_conflict_ns()
    );
    field!("commit_exec_stage_ns", perf_trace.commit_exec_stage_ns());
    field!("commit_exec_apply_ns", perf_trace.commit_exec_apply_ns());
    field!(
        "commit_exec_publish_ns",
        perf_trace.commit_exec_publish_ns()
    );
    field!("commit_admit_ns", perf_trace.commit_admit_ns());
    field!("commit_setup_ns", perf_trace.commit_setup_ns());
    field!(
        "commit_api_batch_clone_ns",
        perf_trace.commit_api_batch_clone_ns()
    );
    field!("commit_api_post_ns", perf_trace.commit_api_post_ns());
    field!(
        "commit_visible_publish_attempts",
        perf_trace.commit_visible_publish_attempts()
    );
    field!(
        "commit_visible_publish_successes",
        perf_trace.commit_visible_publish_successes()
    );
    field!(
        "commit_visible_publish_failures",
        perf_trace.commit_visible_publish_failures()
    );
    field!(
        "commit_unresolved_gate_admission_attempts",
        perf_trace.commit_unresolved_gate_admission_attempts()
    );
    field!(
        "commit_unresolved_gate_admission_acquired",
        perf_trace.commit_unresolved_gate_admission_acquired()
    );
    field!(
        "commit_unresolved_gate_rejected_unresolved",
        perf_trace.commit_unresolved_gate_rejected_unresolved()
    );
    field!(
        "commit_unresolved_records",
        perf_trace.commit_unresolved_records()
    );
    field!(
        "commit_unresolved_durable_not_applied_records",
        perf_trace.commit_unresolved_durable_not_applied_records()
    );
    field!(
        "commit_unresolved_applied_not_visible_records",
        perf_trace.commit_unresolved_applied_not_visible_records()
    );
    field!(
        "commit_branch_registry_lookups",
        perf_trace.commit_branch_registry_lookups()
    );
    field!(
        "commit_branch_registry_descriptors_scanned",
        perf_trace.commit_branch_registry_descriptors_scanned()
    );
    field!(
        "commit_branch_guard_attempts",
        perf_trace.commit_branch_guard_attempts()
    );
    field!(
        "commit_branch_guard_acquired",
        perf_trace.commit_branch_guard_acquired()
    );
    field!(
        "commit_branch_guard_rejected",
        perf_trace.commit_branch_guard_rejected()
    );
    field!(
        "commit_quiesce_attempts",
        perf_trace.commit_quiesce_attempts()
    );
    field!(
        "commit_quiesce_acquired",
        perf_trace.commit_quiesce_acquired()
    );
    field!(
        "commit_quiesce_rejected",
        perf_trace.commit_quiesce_rejected()
    );
    field!(
        "commit_conflict_validation_calls",
        perf_trace.commit_conflict_validation_calls()
    );
    field!(
        "commit_conflict_validation_skipped",
        perf_trace.commit_conflict_validation_skipped()
    );
    field!(
        "commit_conflict_validation_without_source",
        perf_trace.commit_conflict_validation_without_source()
    );
    field!(
        "commit_conflict_validation_with_source",
        perf_trace.commit_conflict_validation_with_source()
    );
    field!(
        "commit_conflict_read_facts_checked",
        perf_trace.commit_conflict_read_facts_checked()
    );
    field!(
        "commit_conflict_cas_facts_checked",
        perf_trace.commit_conflict_cas_facts_checked()
    );
    field!(
        "commit_conflicts_detected",
        perf_trace.commit_conflicts_detected()
    );
    field!(
        "commit_timeline_view_rows_scanned",
        perf_trace.commit_timeline_view_rows_scanned()
    );
    field!(
        "commit_timeline_timestamp_facts",
        perf_trace.commit_timeline_timestamp_facts()
    );
    field!(
        "commit_timeline_version_facts",
        perf_trace.commit_timeline_version_facts()
    );
    field!(
        "commit_timeline_reconcile_calls",
        perf_trace.commit_timeline_reconcile_calls()
    );
    field!(
        "commit_timeline_reconcile_timestamp_facts",
        perf_trace.commit_timeline_reconcile_timestamp_facts()
    );
    field!(
        "commit_timeline_reconcile_version_facts",
        perf_trace.commit_timeline_reconcile_version_facts()
    );
    field!(
        "commit_timeline_reconcile_entry_checks",
        perf_trace.commit_timeline_reconcile_entry_checks()
    );
    field!(
        "commit_timeline_lookup_calls",
        perf_trace.commit_timeline_lookup_calls()
    );
    field!(
        "commit_timeline_lookup_entries_scanned",
        perf_trace.commit_timeline_lookup_entries_scanned()
    );
    field!(
        "commit_replay_classification_calls",
        perf_trace.commit_replay_classification_calls()
    );
    field!(
        "commit_replay_rows_classified",
        perf_trace.commit_replay_rows_classified()
    );
    field!(
        "commit_replay_history_calls",
        perf_trace.commit_replay_history_calls()
    );
    field!(
        "commit_replay_source_probes",
        perf_trace.commit_replay_source_probes()
    );
    field!(
        "lifecycle_write_admission_evaluations",
        perf_trace.lifecycle_write_admission_evaluations()
    );
    field!(
        "lifecycle_write_admission_clean_accepts",
        perf_trace.lifecycle_write_admission_clean_accepts()
    );
    field!(
        "lifecycle_write_admission_under_pressure_accepts",
        perf_trace.lifecycle_write_admission_under_pressure_accepts()
    );
    field!(
        "lifecycle_write_admission_requires_maintenance",
        perf_trace.lifecycle_write_admission_requires_maintenance()
    );
    field!(
        "lifecycle_write_admission_inline_attempts",
        perf_trace.lifecycle_write_admission_inline_attempts()
    );
    field!(
        "lifecycle_write_admission_pressure_rejects",
        perf_trace.lifecycle_write_admission_pressure_rejects()
    );
    field!(
        "lifecycle_write_admission_retryable_rejects",
        perf_trace.lifecycle_write_admission_retryable_rejects()
    );
    field!(
        "lifecycle_write_admission_pressure_cleared_retries",
        perf_trace.lifecycle_write_admission_pressure_cleared_retries()
    );
    field!(
        "lifecycle_post_commit_maintenance_evaluations",
        perf_trace.lifecycle_post_commit_maintenance_evaluations()
    );
    field!(
        "lifecycle_post_commit_maintenance_disabled",
        perf_trace.lifecycle_post_commit_maintenance_disabled()
    );
    field!(
        "lifecycle_post_commit_maintenance_no_task",
        perf_trace.lifecycle_post_commit_maintenance_no_task()
    );
    field!(
        "lifecycle_post_commit_maintenance_tasks_suggested",
        perf_trace.lifecycle_post_commit_maintenance_tasks_suggested()
    );
    field!(
        "lifecycle_post_commit_maintenance_tasks_enqueued",
        perf_trace.lifecycle_post_commit_maintenance_tasks_enqueued()
    );
    field!(
        "lifecycle_post_commit_maintenance_tasks_coalesced",
        perf_trace.lifecycle_post_commit_maintenance_tasks_coalesced()
    );
    field!(
        "lifecycle_post_commit_maintenance_tasks_deferred",
        perf_trace.lifecycle_post_commit_maintenance_tasks_deferred()
    );
    field!(
        "lifecycle_inline_maintenance_attempts",
        perf_trace.lifecycle_inline_maintenance_attempts()
    );
    field!(
        "lifecycle_inline_maintenance_ns",
        perf_trace.lifecycle_inline_maintenance_ns()
    );
    field!(
        "lifecycle_background_runtimes_created",
        perf_trace.lifecycle_background_runtimes_created()
    );
    field!(
        "lifecycle_background_runtime_workers_created",
        perf_trace.lifecycle_background_runtime_workers_created()
    );
    field!(
        "lifecycle_background_wake_submitted",
        perf_trace.lifecycle_background_wake_submitted()
    );
    field!(
        "lifecycle_background_wake_coalesced",
        perf_trace.lifecycle_background_wake_coalesced()
    );
    field!(
        "lifecycle_background_wake_rejected",
        perf_trace.lifecycle_background_wake_rejected()
    );
    field!(
        "lifecycle_background_stale_wake_noop",
        perf_trace.lifecycle_background_stale_wake_noop()
    );
    field!(
        "lifecycle_background_drain_rounds",
        perf_trace.lifecycle_background_drain_rounds()
    );
    field!(
        "lifecycle_background_tasks_completed",
        perf_trace.lifecycle_background_tasks_completed()
    );
    field!(
        "lifecycle_background_task_snapshot_lock_ns",
        perf_trace.lifecycle_background_task_snapshot_lock_ns()
    );
    field!(
        "lifecycle_background_task_unlocked_build_ns",
        perf_trace.lifecycle_background_task_unlocked_build_ns()
    );
    field!(
        "lifecycle_background_task_publish_lock_ns",
        perf_trace.lifecycle_background_task_publish_lock_ns()
    );
    field!(
        "lifecycle_background_publish_manifest_persist_ns",
        perf_trace.lifecycle_background_publish_manifest_persist_ns()
    );
    field!(
        "lifecycle_background_task_total_ns",
        perf_trace.lifecycle_background_task_total_ns()
    );
    field!(
        "lifecycle_background_candidate_stale_deferred",
        perf_trace.lifecycle_background_candidate_stale_deferred()
    );
    field!(
        "lifecycle_foreground_wait_background_lock_ns",
        perf_trace.lifecycle_foreground_wait_background_lock_ns()
    );
    field!(
        "lifecycle_write_admission_wait_attempts",
        perf_trace.lifecycle_write_admission_wait_attempts()
    );
    field!(
        "lifecycle_write_admission_wait_timeouts",
        perf_trace.lifecycle_write_admission_wait_timeouts()
    );
    field!(
        "lifecycle_write_admission_wait_progress_resets",
        perf_trace.lifecycle_write_admission_wait_progress_resets()
    );
    field!(
        "lifecycle_write_admission_block_wait_ns",
        perf_trace.lifecycle_write_admission_block_wait_ns()
    );
    field!(
        "lifecycle_wal_retention_samples",
        perf_trace.lifecycle_wal_retention_samples()
    );
    field!(
        "lifecycle_wal_retained_bytes_last",
        perf_trace.lifecycle_wal_retained_bytes_last()
    );
    field!(
        "lifecycle_wal_retained_bytes_max",
        perf_trace.lifecycle_wal_retained_bytes_max()
    );
    field!(
        "lifecycle_wal_retained_segments_last",
        perf_trace.lifecycle_wal_retained_segments_last()
    );
    field!(
        "lifecycle_wal_retained_segments_max",
        perf_trace.lifecycle_wal_retained_segments_max()
    );
    field!(
        "lifecycle_wal_checkpoint_enqueue_events",
        perf_trace.lifecycle_wal_checkpoint_enqueue_events()
    );
    field!(
        "lifecycle_wal_checkpoint_coalesced_events",
        perf_trace.lifecycle_wal_checkpoint_coalesced_events()
    );
    field!(
        "lifecycle_checkpoint_executions",
        perf_trace.lifecycle_checkpoint_executions()
    );
    field!(
        "lifecycle_wal_truncation_deleted_segments",
        perf_trace.lifecycle_wal_truncation_deleted_segments()
    );
    field!(
        "lifecycle_wal_truncation_protected_segments",
        perf_trace.lifecycle_wal_truncation_protected_segments()
    );
    field!(
        "lifecycle_wal_truncation_failed_segments",
        perf_trace.lifecycle_wal_truncation_failed_segments()
    );
    field!("append_rows_applied", perf_trace.append_rows_applied());
    field!(
        "branch_facts_rows_observed",
        perf_trace.branch_facts_rows_observed()
    );
    field!("read_view_captures", perf_trace.read_view_captures());
    field!(
        "read_view_source_handles_cloned",
        perf_trace.read_view_source_handles_cloned()
    );
    field!("read_view_rows_cloned", perf_trace.read_view_rows_cloned());
    field!(
        "read_view_row_clone_bytes",
        perf_trace.read_view_row_clone_bytes()
    );
    field!(
        "read_view_validation_rows_scanned",
        perf_trace.read_view_validation_rows_scanned()
    );
    field!(
        "branch_compaction_source_opens",
        perf_trace.branch_compaction_source_opens()
    );
    field!(
        "branch_compaction_peak_buffered_rows",
        perf_trace.branch_compaction_peak_buffered_rows()
    );
    field!(
        "lifecycle_compaction_operations_completed",
        perf_trace.lifecycle_compaction_operations_completed()
    );
    field!(
        "lifecycle_compaction_l0_operations",
        perf_trace.lifecycle_compaction_l0_operations()
    );
    field!(
        "lifecycle_compaction_l0_to_level_one_operations",
        perf_trace.lifecycle_compaction_l0_to_level_one_operations()
    );
    field!(
        "lifecycle_compaction_nonzero_operations",
        perf_trace.lifecycle_compaction_nonzero_operations()
    );
    field!(
        "lifecycle_compaction_bottommost_operations",
        perf_trace.lifecycle_compaction_bottommost_operations()
    );
    field!(
        "lifecycle_compaction_input_tables",
        perf_trace.lifecycle_compaction_input_tables()
    );
    field!(
        "lifecycle_compaction_overlap_tables",
        perf_trace.lifecycle_compaction_overlap_tables()
    );
    field!(
        "lifecycle_compaction_output_tables",
        perf_trace.lifecycle_compaction_output_tables()
    );
    field!(
        "lifecycle_compaction_input_bytes",
        perf_trace.lifecycle_compaction_input_bytes()
    );
    field!(
        "lifecycle_compaction_output_bytes",
        perf_trace.lifecycle_compaction_output_bytes()
    );
    field!(
        "lifecycle_compaction_metadata_bytes_avoided",
        perf_trace.lifecycle_compaction_metadata_bytes_avoided()
    );
    field!(
        "lifecycle_compaction_elapsed_ns",
        perf_trace.lifecycle_compaction_elapsed_ns()
    );
    field!(
        "lifecycle_compaction_input_rows",
        perf_trace.lifecycle_compaction_input_rows()
    );
    field!(
        "lifecycle_compaction_rewrite_bytes_per_row",
        perf_trace.lifecycle_compaction_rewrite_bytes_per_row()
    );
    field!(
        "lifecycle_compaction_io_budget_consumed_bytes",
        perf_trace.lifecycle_compaction_io_budget_consumed_bytes()
    );
    field!(
        "lifecycle_compaction_io_budget_deferrals",
        perf_trace.lifecycle_compaction_io_budget_deferrals()
    );
    field!(
        "lifecycle_compaction_io_budget_deferred_bytes",
        perf_trace.lifecycle_compaction_io_budget_deferred_bytes()
    );
    field!(
        "lifecycle_compaction_io_budget_limit_bytes",
        perf_trace.lifecycle_compaction_io_budget_limit_bytes()
    );
    field!(
        "lifecycle_compaction_flush_preemptions",
        perf_trace.lifecycle_compaction_flush_preemptions()
    );
    field!(
        "lifecycle_compaction_trivial_moves",
        perf_trace.lifecycle_compaction_trivial_moves()
    );
    field!(
        "lifecycle_compaction_selected",
        perf_trace.lifecycle_compaction_selected()
    );
    field!(
        "lifecycle_compaction_selected_level_sum",
        perf_trace.lifecycle_compaction_selected_level_sum()
    );
    field!(
        "lifecycle_compaction_selected_score_sum",
        perf_trace.lifecycle_compaction_selected_score_sum()
    );
    field!(
        "lifecycle_compaction_selected_table_count",
        perf_trace.lifecycle_compaction_selected_table_count()
    );
    field!(
        "lifecycle_compaction_selected_byte_count",
        perf_trace.lifecycle_compaction_selected_byte_count()
    );
    field!(
        "lifecycle_compaction_selected_target_bytes",
        perf_trace.lifecycle_compaction_selected_target_bytes()
    );
    field!(
        "lifecycle_compaction_nonzero_input_selections",
        perf_trace.lifecycle_compaction_nonzero_input_selections()
    );
    field!(
        "lifecycle_compaction_nonzero_input_level_sum",
        perf_trace.lifecycle_compaction_nonzero_input_level_sum()
    );
    field!(
        "lifecycle_compaction_nonzero_input_table_index_sum",
        perf_trace.lifecycle_compaction_nonzero_input_table_index_sum()
    );
    field!(
        "lifecycle_compaction_nonzero_input_bytes",
        perf_trace.lifecycle_compaction_nonzero_input_bytes()
    );
    field!(
        "lifecycle_compaction_nonzero_input_rows",
        perf_trace.lifecycle_compaction_nonzero_input_rows()
    );
    field!(
        "lifecycle_compaction_nonzero_input_pointer_selections",
        perf_trace.lifecycle_compaction_largest_input_selections()
    );
    field!(
        "lifecycle_materialization_score_candidates",
        perf_trace.lifecycle_materialization_score_candidates()
    );
    field!(
        "lifecycle_snapshot_floor_advancements",
        perf_trace.lifecycle_snapshot_floor_advancements()
    );
    field!(
        "lifecycle_snapshot_floor_implicit_rejections",
        perf_trace.lifecycle_snapshot_floor_implicit_rejections()
    );
    field!(
        "lifecycle_snapshot_pruning_with_proof",
        perf_trace.lifecycle_snapshot_pruning_with_proof()
    );
    field!(
        "lifecycle_snapshot_pruning_deleted",
        perf_trace.lifecycle_snapshot_pruning_deleted()
    );
    field!(
        "lifecycle_snapshot_pruning_protected",
        perf_trace.lifecycle_snapshot_pruning_protected()
    );
    field!(
        "lifecycle_snapshot_pruning_failed",
        perf_trace.lifecycle_snapshot_pruning_failed()
    );
    field!(
        "branch_materialization_source_opens",
        perf_trace.branch_materialization_source_opens()
    );
    field!(
        "branch_materialization_rows_rewritten",
        perf_trace.branch_materialization_rows_rewritten()
    );
    field!(
        "branch_materialization_rows_skipped_by_fork",
        perf_trace.branch_materialization_rows_skipped_by_fork()
    );
    field!(
        "branch_materialization_rows_skipped_by_shadowing",
        perf_trace.branch_materialization_rows_skipped_by_shadowing()
    );
    field!(
        "branch_materialization_output_tables",
        perf_trace.branch_materialization_output_tables()
    );
    field!(
        "branch_materialization_peak_buffered_rows",
        perf_trace.branch_materialization_peak_buffered_rows()
    );
    field!(
        "table_compaction_merge_cursor_opens",
        perf_trace.table_compaction_merge_cursor_opens()
    );
    field!(
        "table_compaction_merge_advances",
        perf_trace.table_compaction_merge_advances()
    );
    field!(
        "table_compaction_merge_ns",
        perf_trace.table_compaction_merge_ns()
    );
    field!(
        "table_compaction_merge_input_rows",
        perf_trace.table_compaction_merge_input_rows()
    );
    field!(
        "table_compaction_merge_ns_per_input_row",
        perf_trace.table_compaction_merge_ns_per_input_row()
    );
    field!(
        "table_compaction_pre_validation_rows_scanned",
        perf_trace.table_compaction_pre_validation_rows_scanned()
    );
    field!(
        "table_compaction_row_clones",
        perf_trace.table_compaction_row_clones()
    );
    field!(
        "table_compaction_heap_key_clones",
        perf_trace.table_compaction_heap_key_clones()
    );
    field!(
        "table_compaction_source_order_key_clones",
        perf_trace.table_compaction_source_order_key_clones()
    );
    field!(
        "table_compaction_boundary_key_allocations",
        perf_trace.table_compaction_boundary_key_allocations()
    );
    field!(
        "table_compaction_boundary_key_buffer_allocations",
        perf_trace.table_compaction_boundary_key_buffer_allocations()
    );
    field!(
        "table_compaction_boundary_key_buffer_reuses",
        perf_trace.table_compaction_boundary_key_buffer_reuses()
    );
    field!(
        "table_compaction_previous_key_buffer_allocations",
        perf_trace.table_compaction_previous_key_buffer_allocations()
    );
    field!(
        "table_compaction_previous_key_buffer_reuses",
        perf_trace.table_compaction_previous_key_buffer_reuses()
    );
    field!(
        "table_compaction_kept_rows",
        perf_trace.table_compaction_kept_rows()
    );
    field!(
        "table_compaction_dropped_rows",
        perf_trace.table_compaction_dropped_rows()
    );
    field!(
        "table_compaction_peak_buffered_rows",
        perf_trace.table_compaction_peak_buffered_rows()
    );
    field!(
        "table_compaction_output_tables_built",
        perf_trace.table_compaction_output_tables_built()
    );
    field!(
        "table_build_facts_from_streaming_metadata",
        perf_trace.table_build_facts_from_streaming_metadata()
    );
    field!(
        "table_rewrite_redundant_fact_decodes_avoided",
        perf_trace.table_rewrite_redundant_fact_decodes_avoided()
    );
    field!(
        "table_rewrite_reader_reopens_performed",
        perf_trace.table_rewrite_reader_reopens_performed()
    );
    field!("append_staging_clones", perf_trace.append_staging_clones());
    field!(
        "append_staging_rows_cloned",
        perf_trace.append_staging_rows_cloned()
    );
    field!(
        "conflict_sources_built",
        perf_trace.conflict_sources_built()
    );
    field!("point_rows_visited", perf_trace.point_rows_visited());
    field!(
        "point_candidates_materialized",
        perf_trace.point_candidates_materialized()
    );
    field!("point_active_probes", perf_trace.point_active_probes());
    field!("point_frozen_probes", perf_trace.point_frozen_probes());
    field!(
        "point_owned_l0_table_probes",
        perf_trace.point_owned_l0_table_probes()
    );
    field!(
        "point_owned_nonzero_level_searches",
        perf_trace.point_owned_nonzero_level_searches()
    );
    field!(
        "point_owned_nonzero_table_probes",
        perf_trace.point_owned_nonzero_table_probes()
    );
    field!(
        "point_inherited_layer_searches",
        perf_trace.point_inherited_layer_searches()
    );
    field!(
        "point_inherited_l0_table_probes",
        perf_trace.point_inherited_l0_table_probes()
    );
    field!(
        "point_inherited_nonzero_level_searches",
        perf_trace.point_inherited_nonzero_level_searches()
    );
    field!(
        "point_inherited_nonzero_table_probes",
        perf_trace.point_inherited_nonzero_table_probes()
    );
    field!("point_table_seeks", perf_trace.point_table_seeks());
    field!(
        "point_candidate_row_clones",
        perf_trace.point_candidate_row_clones()
    );
    field!(
        "point_candidate_row_clone_bytes",
        perf_trace.point_candidate_row_clone_bytes()
    );
    field!("point_selected_active", perf_trace.point_selected_active());
    field!("point_selected_frozen", perf_trace.point_selected_frozen());
    field!(
        "point_selected_owned_l0",
        perf_trace.point_selected_owned_l0()
    );
    field!(
        "point_selected_owned_nonzero",
        perf_trace.point_selected_owned_nonzero()
    );
    field!(
        "point_selected_inherited",
        perf_trace.point_selected_inherited()
    );
    field!(
        "point_early_exit_active",
        perf_trace.point_early_exit_active()
    );
    field!(
        "point_early_exit_frozen",
        perf_trace.point_early_exit_frozen()
    );
    field!(
        "point_early_exit_owned_l0",
        perf_trace.point_early_exit_owned_l0()
    );
    field!(
        "point_early_exit_owned_nonzero",
        perf_trace.point_early_exit_owned_nonzero()
    );
    field!(
        "point_early_exit_inherited",
        perf_trace.point_early_exit_inherited()
    );
    field!(
        "point_remaining_source_skips",
        perf_trace.point_remaining_source_skips()
    );
    field!(
        "point_inherited_key_rewrites",
        perf_trace.point_inherited_key_rewrites()
    );
    field!(
        "table_point_lookup_key_builds",
        perf_trace.table_point_lookup_key_builds()
    );
    field!(
        "table_point_lookup_key_reuses",
        perf_trace.table_point_lookup_key_reuses()
    );
    field!(
        "table_eager_filter_probes",
        perf_trace.table_eager_filter_probes()
    );
    field!(
        "table_eager_filter_negative_probes",
        perf_trace.table_eager_filter_negative_probes()
    );
    field!(
        "table_eager_filter_positive_probes",
        perf_trace.table_eager_filter_positive_probes()
    );
    field!(
        "table_eager_filter_unavailable_probes",
        perf_trace.table_eager_filter_unavailable_probes()
    );
    field!("scan_rows_visited", perf_trace.scan_rows_visited());
    field!(
        "scan_candidates_materialized",
        perf_trace.scan_candidates_materialized()
    );
    field!("scan_cursor_seeks", perf_trace.scan_cursor_seeks());
    field!(
        "scan_cursor_rows_yielded",
        perf_trace.scan_cursor_rows_yielded()
    );
    field!("scan_active_cursors", perf_trace.scan_active_cursors());
    field!("scan_frozen_cursors", perf_trace.scan_frozen_cursors());
    field!("scan_owned_l0_cursors", perf_trace.scan_owned_l0_cursors());
    field!(
        "scan_owned_nonzero_level_cursors",
        perf_trace.scan_owned_nonzero_level_cursors()
    );
    field!(
        "scan_owned_nonzero_table_cursors_opened",
        perf_trace.scan_owned_nonzero_table_cursors_opened()
    );
    field!(
        "scan_inherited_l0_cursors",
        perf_trace.scan_inherited_l0_cursors()
    );
    field!(
        "scan_inherited_nonzero_level_cursors",
        perf_trace.scan_inherited_nonzero_level_cursors()
    );
    field!(
        "scan_inherited_nonzero_table_cursors_opened",
        perf_trace.scan_inherited_nonzero_table_cursors_opened()
    );
    field!(
        "scan_source_cursor_seeks",
        perf_trace.scan_source_cursor_seeks()
    );
    field!("scan_rows_returned", perf_trace.scan_rows_returned());
    field!(
        "history_active_rows_visited",
        perf_trace.history_active_rows_visited()
    );
    field!(
        "history_frozen_rows_visited",
        perf_trace.history_frozen_rows_visited()
    );
    field!(
        "history_owned_l0_rows_visited",
        perf_trace.history_owned_l0_rows_visited()
    );
    field!(
        "history_owned_nonzero_rows_visited",
        perf_trace.history_owned_nonzero_rows_visited()
    );
    field!(
        "history_inherited_l0_rows_visited",
        perf_trace.history_inherited_l0_rows_visited()
    );
    field!(
        "history_inherited_nonzero_rows_visited",
        perf_trace.history_inherited_nonzero_rows_visited()
    );
    field!(
        "history_candidates_materialized",
        perf_trace.history_candidates_materialized()
    );
    field!(
        "timestamp_active_rows_scanned",
        perf_trace.timestamp_active_rows_scanned()
    );
    field!(
        "timestamp_frozen_rows_scanned",
        perf_trace.timestamp_frozen_rows_scanned()
    );
    field!(
        "timestamp_owned_l0_rows_scanned",
        perf_trace.timestamp_owned_l0_rows_scanned()
    );
    field!(
        "timestamp_owned_nonzero_rows_scanned",
        perf_trace.timestamp_owned_nonzero_rows_scanned()
    );
    field!(
        "timestamp_inherited_l0_rows_scanned",
        perf_trace.timestamp_inherited_l0_rows_scanned()
    );
    field!(
        "timestamp_inherited_nonzero_rows_scanned",
        perf_trace.timestamp_inherited_nonzero_rows_scanned()
    );
    field!(
        "branch_facts_active_rows_observed",
        perf_trace.branch_facts_active_rows_observed()
    );
    field!(
        "branch_facts_frozen_rows_observed",
        perf_trace.branch_facts_frozen_rows_observed()
    );
    field!(
        "branch_facts_owned_l0_rows_observed",
        perf_trace.branch_facts_owned_l0_rows_observed()
    );
    field!(
        "branch_facts_owned_nonzero_rows_observed",
        perf_trace.branch_facts_owned_nonzero_rows_observed()
    );
    field!(
        "branch_facts_inherited_l0_rows_observed",
        perf_trace.branch_facts_inherited_l0_rows_observed()
    );
    field!(
        "branch_facts_inherited_nonzero_rows_observed",
        perf_trace.branch_facts_inherited_nonzero_rows_observed()
    );
    field!(
        "branch_scan_source_setup_ns",
        perf_trace.branch_scan_source_setup_ns()
    );
    field!("branch_scan_merge_ns", perf_trace.branch_scan_merge_ns());
    field!(
        "branch_scan_min_key_ns",
        perf_trace.branch_scan_min_key_ns()
    );
    field!(
        "branch_scan_group_key_ns",
        perf_trace.branch_scan_group_key_ns()
    );
    field!(
        "branch_scan_candidate_ns",
        perf_trace.branch_scan_candidate_ns()
    );
    field!(
        "branch_scan_advance_ns",
        perf_trace.branch_scan_advance_ns()
    );
    field!("branch_scan_select_ns", perf_trace.branch_scan_select_ns());
    field!("branch_scan_emit_ns", perf_trace.branch_scan_emit_ns());
    field!(
        "scan_logical_key_encodes",
        perf_trace.scan_logical_key_encodes()
    );
    field!(
        "scan_candidate_row_clones",
        perf_trace.scan_candidate_row_clones()
    );
    field!(
        "scan_candidate_row_clone_bytes",
        perf_trace.scan_candidate_row_clone_bytes()
    );
    field!("table_reader_opens", perf_trace.table_reader_opens());
    field!(
        "table_metadata_read_bytes",
        perf_trace.table_metadata_read_bytes()
    );
    field!(
        "table_index_read_bytes",
        perf_trace.table_index_read_bytes()
    );
    field!(
        "table_properties_read_bytes",
        perf_trace.table_properties_read_bytes()
    );
    field!(
        "table_data_block_reads",
        perf_trace.table_data_block_reads()
    );
    field!(
        "table_data_block_read_bytes",
        perf_trace.table_data_block_read_bytes()
    );
    field!(
        "table_data_block_decodes",
        perf_trace.table_data_block_decodes()
    );
    field!("table_rows_decoded", perf_trace.table_rows_decoded());
    field!(
        "table_point_rows_visited",
        perf_trace.table_point_rows_visited()
    );
    field!(
        "table_cursor_rows_visited",
        perf_trace.table_cursor_rows_visited()
    );
    field!("table_cache_hits", perf_trace.table_cache_hits());
    field!("table_cache_misses", perf_trace.table_cache_misses());
    field!("table_cache_inserts", perf_trace.table_cache_inserts());
    field!(
        "table_cache_skipped_inserts",
        perf_trace.table_cache_skipped_inserts()
    );
    field!("table_filter_probes", perf_trace.table_filter_probes());
    field!(
        "table_filter_negative_probes",
        perf_trace.table_filter_negative_probes()
    );
    field!(
        "table_filter_positive_probes",
        perf_trace.table_filter_positive_probes()
    );
    field!(
        "table_filter_absent_probes",
        perf_trace.table_filter_absent_probes()
    );
    field!("table_seeks", perf_trace.table_seeks());
    field!("table_bound_checks", perf_trace.table_bound_checks());
    field!("table_bound_check_ns", perf_trace.table_bound_check_ns());

    serde_json::Value::Object(trace)
}

fn source_shape_metrics_json(
    scale: usize,
    operation_count: u64,
    value_bytes: u64,
    perf_trace: StoragePerfSnapshot,
    load_phase_trace: Option<LoadPhaseTrace>,
    source_shape_context: Option<&SourceShapeContext>,
) -> serde_json::Value {
    let logical_write_rows = scale as u64;
    let logical_write_bytes = logical_write_rows.saturating_mul(value_bytes);
    let compaction_row_amplification = ratio_json(
        perf_trace.lifecycle_compaction_input_rows(),
        logical_write_rows,
    );
    let compaction_byte_amplification = ratio_json(
        perf_trace.lifecycle_compaction_input_bytes(),
        logical_write_bytes,
    );
    let point_source_probes = perf_trace
        .point_active_probes()
        .saturating_add(perf_trace.point_frozen_probes())
        .saturating_add(perf_trace.point_owned_l0_table_probes())
        .saturating_add(perf_trace.point_owned_nonzero_level_searches())
        .saturating_add(perf_trace.point_inherited_layer_searches())
        .saturating_add(perf_trace.point_inherited_l0_table_probes())
        .saturating_add(perf_trace.point_inherited_nonzero_level_searches());
    let point_nonzero_table_probes = perf_trace
        .point_owned_nonzero_table_probes()
        .saturating_add(perf_trace.point_inherited_nonzero_table_probes());
    let scan_source_cursors = perf_trace
        .scan_active_cursors()
        .saturating_add(perf_trace.scan_frozen_cursors())
        .saturating_add(perf_trace.scan_owned_l0_cursors())
        .saturating_add(perf_trace.scan_owned_nonzero_level_cursors())
        .saturating_add(perf_trace.scan_inherited_l0_cursors())
        .saturating_add(perf_trace.scan_inherited_nonzero_level_cursors());
    let scan_table_cursors_opened = perf_trace
        .scan_owned_l0_cursors()
        .saturating_add(perf_trace.scan_owned_nonzero_table_cursors_opened())
        .saturating_add(perf_trace.scan_inherited_l0_cursors())
        .saturating_add(perf_trace.scan_inherited_nonzero_table_cursors_opened());
    let l0_tables_per_million_rows_after_load =
        source_shape_context.map_or(serde_json::Value::Null, |context| {
            ratio_json(
                (context.final_layout.owned_l0_tables as u64).saturating_mul(1_000_000),
                scale as u64,
            )
        });
    let post_load_compaction_mode = source_shape_context
        .map_or(serde_json::Value::Null, |context| {
            serde_json::json!(context.compaction_mode.as_str())
        });
    let maintenance_queue_depth_final = source_shape_context
        .and_then(|context| {
            context
                .maintenance_queue
                .map(|queue| serde_json::json!(queue.pending_tasks))
        })
        .unwrap_or(serde_json::Value::Null);
    let maintenance_queue_depth_max = source_shape_context
        .and_then(|context| {
            context
                .maintenance_queue
                .map(|queue| serde_json::json!(queue.max_pending_tasks))
        })
        .unwrap_or(serde_json::Value::Null);
    let maintenance_queue_deferred_outcomes_per_million_rows = source_shape_context
        .and_then(|context| {
            context.maintenance_queue.map(|queue| {
                ratio_json(
                    (queue.deferred as u64).saturating_mul(1_000_000),
                    scale as u64,
                )
            })
        })
        .unwrap_or(serde_json::Value::Null);
    let load_maintenance_ms_per_million_rows = load_phase_trace
        .map_or(serde_json::Value::Null, |trace| {
            ns_per_row_as_ms_per_million_rows_json(trace.maintenance_call_ns, scale as u64)
        });
    let diagnostic_poll_ns = load_phase_trace.map_or(0, |trace| trace.diagnostic_poll_ns);
    let diagnostic_polls = load_phase_trace.map_or(0, |trace| trace.diagnostic_polls);
    let inline_maintenance_ns = load_phase_trace
        .map_or(perf_trace.lifecycle_inline_maintenance_ns(), |trace| {
            trace.inline_maintenance_ns
        });
    let inline_maintenance_attempts = load_phase_trace.map_or(
        perf_trace.lifecycle_inline_maintenance_attempts(),
        |trace| trace.inline_maintenance_attempts,
    );
    let background_maintenance_ns = load_phase_trace
        .map_or(perf_trace.lifecycle_background_task_total_ns(), |trace| {
            trace.background_maintenance_ns
        });
    let background_maintenance_tasks = load_phase_trace
        .map_or(perf_trace.lifecycle_background_tasks_completed(), |trace| {
            trace.background_maintenance_tasks
        });
    let automatic_maintenance_ns = inline_maintenance_ns.saturating_add(background_maintenance_ns);
    let automatic_maintenance_attempts =
        inline_maintenance_attempts.saturating_add(background_maintenance_tasks);
    let foreground_wait_background_lock_ns = load_phase_trace.map_or(
        perf_trace.lifecycle_foreground_wait_background_lock_ns(),
        |trace| trace.foreground_wait_background_lock_ns,
    );
    let admission_block_wait_ns = load_phase_trace.map_or(
        perf_trace.lifecycle_write_admission_block_wait_ns(),
        |trace| trace.admission_block_wait_ns,
    );
    let admission_wait_attempts = load_phase_trace.map_or(
        perf_trace.lifecycle_write_admission_wait_attempts(),
        |trace| trace.admission_wait_attempts,
    );
    let admission_wait_timeouts = load_phase_trace.map_or(
        perf_trace.lifecycle_write_admission_wait_timeouts(),
        |trace| trace.admission_wait_timeouts,
    );
    let maintenance_suggested_tasks = load_phase_trace.map_or(
        perf_trace.lifecycle_post_commit_maintenance_tasks_suggested(),
        |trace| trace.maintenance_suggested_tasks,
    );
    let maintenance_scheduled_tasks = load_phase_trace.map_or(
        perf_trace.lifecycle_post_commit_maintenance_tasks_enqueued(),
        |trace| trace.maintenance_scheduled_tasks,
    );
    let maintenance_coalesced_tasks = load_phase_trace.map_or(
        perf_trace.lifecycle_post_commit_maintenance_tasks_coalesced(),
        |trace| trace.maintenance_coalesced_tasks,
    );
    let maintenance_deferred_tasks = load_phase_trace.map_or(
        perf_trace.lifecycle_post_commit_maintenance_tasks_deferred(),
        |trace| trace.maintenance_deferred_tasks,
    );
    let wal_retained_bytes_last = load_phase_trace
        .and_then(|trace| trace.wal_retained_bytes_last)
        .or_else(|| {
            (perf_trace.lifecycle_wal_retention_samples() > 0)
                .then(|| perf_trace.lifecycle_wal_retained_bytes_last())
        })
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_retained_segments_last = load_phase_trace
        .and_then(|trace| trace.wal_retained_segments_last)
        .or_else(|| {
            (perf_trace.lifecycle_wal_retention_samples() > 0)
                .then(|| perf_trace.lifecycle_wal_retained_segments_last())
        })
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_retained_bytes_max = load_phase_trace
        .and_then(|trace| trace.wal_retained_bytes_max)
        .or_else(|| {
            (perf_trace.lifecycle_wal_retention_samples() > 0)
                .then(|| perf_trace.lifecycle_wal_retained_bytes_max())
        })
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_retained_segments_max = load_phase_trace
        .and_then(|trace| trace.wal_retained_segments_max)
        .or_else(|| {
            (perf_trace.lifecycle_wal_retention_samples() > 0)
                .then(|| perf_trace.lifecycle_wal_retained_segments_max())
        })
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_commits_since_checkpoint_last = load_phase_trace
        .and_then(|trace| trace.wal_commits_since_checkpoint_last)
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_retention_limit_bytes = load_phase_trace
        .and_then(|trace| trace.wal_retention_limit_bytes)
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_retention_limit_segments = load_phase_trace
        .and_then(|trace| trace.wal_retention_limit_segments)
        .map_or(serde_json::Value::Null, serde_json::Value::from);
    let wal_checkpoint_enqueue_events = load_phase_trace.map_or(
        perf_trace.lifecycle_wal_checkpoint_enqueue_events(),
        |trace| trace.wal_checkpoint_enqueue_events,
    );
    let wal_checkpoint_coalesced_events = load_phase_trace.map_or(
        perf_trace.lifecycle_wal_checkpoint_coalesced_events(),
        |trace| trace.wal_checkpoint_coalesced_events,
    );
    let checkpoint_executions = load_phase_trace
        .map_or(perf_trace.lifecycle_checkpoint_executions(), |trace| {
            trace.checkpoint_executions
        });
    let wal_truncation_deleted_segments = load_phase_trace.map_or(
        perf_trace.lifecycle_wal_truncation_deleted_segments(),
        |trace| trace.wal_truncation_deleted_segments,
    );
    let wal_truncation_protected_segments = load_phase_trace.map_or(
        perf_trace.lifecycle_wal_truncation_protected_segments(),
        |trace| trace.wal_truncation_protected_segments,
    );
    let wal_truncation_failed_segments = load_phase_trace.map_or(
        perf_trace.lifecycle_wal_truncation_failed_segments(),
        |trace| trace.wal_truncation_failed_segments,
    );
    let automatic_maintenance_ms_per_million_rows =
        ns_per_row_as_ms_per_million_rows_json(automatic_maintenance_ns, scale as u64);
    let scheduled_maintenance_tasks_per_explicit_flush =
        load_phase_trace.map_or(serde_json::Value::Null, |trace| {
            ratio_json(
                trace
                    .maintenance_scheduled_tasks
                    .saturating_add(trace.maintenance_coalesced_tasks),
                trace.maintenance_runs,
            )
        });
    let point_shape = source_shape_context.map(|context| {
        point_probe_shape_json(
            operation_count,
            perf_trace,
            context.final_layout.owned_l0_tables as u64,
            context.final_layout.owned_nonzero_level_count(),
            context.final_layout.inherited_layers as u64,
            context.final_layout.inherited_l0_tables as u64,
            context.final_layout.inherited_nonzero_level_count(),
        )
    });
    let throughput_interpretation =
        source_shape_context.map_or("source-shape-unavailable", |context| {
            if !context.source_shape_passed {
                "source-shape-failed"
            } else if point_shape
                .as_ref()
                .and_then(|shape| shape.get("passed"))
                .and_then(serde_json::Value::as_bool)
                == Some(false)
            {
                "point-probe-shape-failed"
            } else {
                "source-shape-passed-evaluate-filter-cache"
            }
        });

    let lifecycle_compaction = serde_json::json!({
        "operations_completed": perf_trace.lifecycle_compaction_operations_completed(),
        "input_tables": perf_trace.lifecycle_compaction_input_tables(),
        "overlap_tables": perf_trace.lifecycle_compaction_overlap_tables(),
        "output_tables": perf_trace.lifecycle_compaction_output_tables(),
        "input_bytes": perf_trace.lifecycle_compaction_input_bytes(),
        "output_bytes": perf_trace.lifecycle_compaction_output_bytes(),
        "metadata_bytes_avoided": perf_trace.lifecycle_compaction_metadata_bytes_avoided(),
        "elapsed_ns": perf_trace.lifecycle_compaction_elapsed_ns(),
        "input_rows": perf_trace.lifecycle_compaction_input_rows(),
        "rewrite_bytes_per_row": perf_trace.lifecycle_compaction_rewrite_bytes_per_row(),
        "io_budget_consumed_bytes": perf_trace.lifecycle_compaction_io_budget_consumed_bytes(),
        "io_budget_deferrals": perf_trace.lifecycle_compaction_io_budget_deferrals(),
        "io_budget_deferred_bytes": perf_trace.lifecycle_compaction_io_budget_deferred_bytes(),
        "io_budget_limit_bytes": perf_trace.lifecycle_compaction_io_budget_limit_bytes(),
        "flush_preemptions": perf_trace.lifecycle_compaction_flush_preemptions(),
        "trivial_moves": perf_trace.lifecycle_compaction_trivial_moves(),
        "selected": {
            "candidates": perf_trace.lifecycle_compaction_selected(),
            "level_sum": perf_trace.lifecycle_compaction_selected_level_sum(),
            "score_sum": perf_trace.lifecycle_compaction_selected_score_sum(),
            "table_count": perf_trace.lifecycle_compaction_selected_table_count(),
            "byte_count": perf_trace.lifecycle_compaction_selected_byte_count(),
            "target_bytes": perf_trace.lifecycle_compaction_selected_target_bytes(),
        },
        "selected_nonzero_input": {
            "selections": perf_trace.lifecycle_compaction_nonzero_input_selections(),
            "level_sum": perf_trace.lifecycle_compaction_nonzero_input_level_sum(),
            "table_index_sum": perf_trace.lifecycle_compaction_nonzero_input_table_index_sum(),
            "bytes": perf_trace.lifecycle_compaction_nonzero_input_bytes(),
            "rows": perf_trace.lifecycle_compaction_nonzero_input_rows(),
            "pointer_selections": perf_trace.lifecycle_compaction_largest_input_selections(),
        },
        "operation_kinds": {
            "l0": perf_trace.lifecycle_compaction_l0_operations(),
            "l0_to_level_one": perf_trace.lifecycle_compaction_l0_to_level_one_operations(),
            "nonzero": perf_trace.lifecycle_compaction_nonzero_operations(),
            "bottommost": perf_trace.lifecycle_compaction_bottommost_operations(),
        },
    });
    let lifecycle_materialization = serde_json::json!({
        "score_candidates": perf_trace.lifecycle_materialization_score_candidates(),
        "score_layer_index_sum": perf_trace.lifecycle_materialization_score_layer_index_sum(),
        "score_table_count": perf_trace.lifecycle_materialization_score_table_count(),
        "score_byte_count": perf_trace.lifecycle_materialization_score_byte_count(),
    });
    let lifecycle_snapshot_pruning = serde_json::json!({
        "floor_advancements": perf_trace.lifecycle_snapshot_floor_advancements(),
        "floor_implicit_rejections": perf_trace.lifecycle_snapshot_floor_implicit_rejections(),
        "with_proof": perf_trace.lifecycle_snapshot_pruning_with_proof(),
        "deleted": perf_trace.lifecycle_snapshot_pruning_deleted(),
        "protected": perf_trace.lifecycle_snapshot_pruning_protected(),
        "failed": perf_trace.lifecycle_snapshot_pruning_failed(),
    });
    let point_read_acceleration_gaps = serde_json::json!({
        "table_data_block_reads": perf_trace.table_data_block_reads(),
        "table_data_block_decodes": perf_trace.table_data_block_decodes(),
        "table_rows_decoded": perf_trace.table_rows_decoded(),
        "table_filter_probes": perf_trace.table_filter_probes(),
        "table_filter_negative_probes": perf_trace.table_filter_negative_probes(),
        "table_filter_positive_probes": perf_trace.table_filter_positive_probes(),
        "table_filter_absent_probes": perf_trace.table_filter_absent_probes(),
        "table_cache_hits": perf_trace.table_cache_hits(),
        "table_cache_misses": perf_trace.table_cache_misses(),
    });
    let assumptions = serde_json::json!({
        "operation_count": operation_count,
        "l0_tables_after_load": source_shape_context.map_or(
            "source-shape unavailable",
            |context| context.compaction_mode.layout_assumption(),
        ),
        "known_compaction_mode_values": source_shape_compaction_modes_json(),
    });

    let mut metrics = serde_json::Map::new();
    macro_rules! field {
        ($key:literal, $value:expr) => {
            metrics.insert($key.to_string(), serde_json::json!($value));
        };
    }

    field!(
        "point_source_probes_per_read",
        ratio_json(point_source_probes, operation_count)
    );
    field!(
        "point_nonzero_table_probes_per_read",
        ratio_json(point_nonzero_table_probes, operation_count)
    );
    field!(
        "point_owned_l0_table_probes",
        perf_trace.point_owned_l0_table_probes()
    );
    field!(
        "point_owned_nonzero_level_searches",
        perf_trace.point_owned_nonzero_level_searches()
    );
    field!(
        "point_owned_nonzero_table_probes",
        perf_trace.point_owned_nonzero_table_probes()
    );
    field!("point_table_seeks", perf_trace.point_table_seeks());
    field!("point_rows_visited", perf_trace.point_rows_visited());
    field!(
        "scan_source_cursors_per_call",
        ratio_json(scan_source_cursors, operation_count)
    );
    field!(
        "scan_table_cursors_opened_per_call",
        ratio_json(scan_table_cursors_opened, operation_count)
    );
    field!(
        "scan_rows_visited_per_row_returned",
        ratio_json(
            perf_trace.scan_rows_visited(),
            perf_trace.scan_rows_returned()
        )
    );
    field!(
        "load_maintenance_ms_per_million_rows",
        load_maintenance_ms_per_million_rows
    );
    field!("automatic_maintenance_ns", automatic_maintenance_ns);
    field!(
        "automatic_maintenance_attempts",
        automatic_maintenance_attempts
    );
    field!("inline_maintenance_ns", inline_maintenance_ns);
    field!("inline_maintenance_attempts", inline_maintenance_attempts);
    field!("diagnostic_poll_ns", diagnostic_poll_ns);
    field!("diagnostic_polls", diagnostic_polls);
    field!("logical_write_rows", logical_write_rows);
    field!("logical_write_bytes", logical_write_bytes);
    field!("compaction_row_amplification", compaction_row_amplification);
    field!(
        "compaction_byte_amplification",
        compaction_byte_amplification
    );
    field!("background_maintenance_ns", background_maintenance_ns);
    field!("background_maintenance_tasks", background_maintenance_tasks);
    field!(
        "foreground_wait_background_lock_ns",
        foreground_wait_background_lock_ns
    );
    field!("admission_block_wait_ns", admission_block_wait_ns);
    field!("admission_wait_attempts", admission_wait_attempts);
    field!("admission_wait_timeouts", admission_wait_timeouts);
    field!(
        "automatic_maintenance_ms_per_million_rows",
        automatic_maintenance_ms_per_million_rows
    );
    field!(
        "l0_tables_per_million_rows_after_load",
        l0_tables_per_million_rows_after_load
    );
    field!(
        "scheduled_maintenance_tasks_per_explicit_flush",
        scheduled_maintenance_tasks_per_explicit_flush
    );
    field!(
        "maintenance_queue_depth_final",
        maintenance_queue_depth_final
    );
    field!("maintenance_queue_depth_max", maintenance_queue_depth_max);
    field!(
        "maintenance_queue_deferred_outcomes_per_million_rows",
        maintenance_queue_deferred_outcomes_per_million_rows
    );
    field!("maintenance_suggested_tasks", maintenance_suggested_tasks);
    field!("maintenance_scheduled_tasks", maintenance_scheduled_tasks);
    field!("maintenance_coalesced_tasks", maintenance_coalesced_tasks);
    field!("maintenance_deferred_tasks", maintenance_deferred_tasks);
    field!("wal_retained_bytes_last", wal_retained_bytes_last);
    field!("wal_retained_segments_last", wal_retained_segments_last);
    field!("wal_retained_bytes_max", wal_retained_bytes_max);
    field!("wal_retained_segments_max", wal_retained_segments_max);
    field!(
        "wal_commits_since_checkpoint_last",
        wal_commits_since_checkpoint_last
    );
    field!("wal_retention_limit_bytes", wal_retention_limit_bytes);
    field!("wal_retention_limit_segments", wal_retention_limit_segments);
    field!(
        "wal_checkpoint_enqueue_events",
        wal_checkpoint_enqueue_events
    );
    field!(
        "wal_checkpoint_coalesced_events",
        wal_checkpoint_coalesced_events
    );
    field!("checkpoint_executions", checkpoint_executions);
    field!(
        "wal_truncation_deleted_segments",
        wal_truncation_deleted_segments
    );
    field!(
        "wal_truncation_protected_segments",
        wal_truncation_protected_segments
    );
    field!(
        "wal_truncation_failed_segments",
        wal_truncation_failed_segments
    );
    field!("lifecycle_compaction", lifecycle_compaction);
    field!("lifecycle_materialization", lifecycle_materialization);
    field!("lifecycle_snapshot_pruning", lifecycle_snapshot_pruning);
    field!("post_load_compaction_mode", post_load_compaction_mode);
    field!(
        "post_load_source_shape",
        source_shape_context.map(source_shape_context_json)
    );
    field!("point_probe_shape", point_shape);
    field!("throughput_interpretation", throughput_interpretation);
    field!("point_read_acceleration_gaps", point_read_acceleration_gaps);
    field!("assumptions", assumptions);

    serde_json::Value::Object(metrics)
}

fn point_probe_shape_json(
    operation_count: u64,
    perf_trace: StoragePerfSnapshot,
    owned_l0_table_count: u64,
    owned_nonzero_level_count: u64,
    inherited_layers: u64,
    inherited_l0_table_count: u64,
    inherited_nonzero_level_count: u64,
) -> serde_json::Value {
    point_probe_shape_json_from_counts(
        operation_count,
        PointProbeShapeCounters::from_perf_trace(perf_trace),
        owned_l0_table_count,
        owned_nonzero_level_count,
        inherited_layers,
        inherited_l0_table_count,
        inherited_nonzero_level_count,
    )
}

fn point_probe_shape_json_from_counts(
    operation_count: u64,
    counters: PointProbeShapeCounters,
    owned_l0_table_count: u64,
    owned_nonzero_level_count: u64,
    inherited_layers: u64,
    inherited_l0_table_count: u64,
    inherited_nonzero_level_count: u64,
) -> serde_json::Value {
    if operation_count == 0 {
        return serde_json::Value::Null;
    }

    let max_owned_l0_table_probes = operation_count.saturating_mul(owned_l0_table_count);
    let max_owned_nonzero_level_searches =
        operation_count.saturating_mul(owned_nonzero_level_count);
    let max_inherited_l0_table_probes = operation_count.saturating_mul(inherited_l0_table_count);
    let inherited_level_bound =
        inherited_nonzero_level_count.saturating_mul(if inherited_nonzero_level_count == 0 {
            0
        } else {
            inherited_layers.max(1)
        });
    let max_inherited_nonzero_level_searches =
        operation_count.saturating_mul(inherited_level_bound);
    let mut failures = Vec::new();
    if counters.owned_l0_table_probes > max_owned_l0_table_probes {
        failures.push("owned_l0_table_probes_exceed_layout_bound");
    }
    if counters.inherited_l0_table_probes > max_inherited_l0_table_probes {
        failures.push("inherited_l0_table_probes_exceed_layout_bound");
    }
    if counters.owned_nonzero_level_searches > max_owned_nonzero_level_searches {
        failures.push("owned_nonzero_level_searches_exceed_layout_bound");
    }
    if counters.inherited_nonzero_level_searches > max_inherited_nonzero_level_searches {
        failures.push("inherited_nonzero_level_searches_exceed_layout_bound");
    }
    if counters.owned_nonzero_table_probes > counters.owned_nonzero_level_searches {
        failures.push("owned_nonzero_table_probes_exceed_level_searches");
    }
    if counters.inherited_nonzero_table_probes > counters.inherited_nonzero_level_searches {
        failures.push("inherited_nonzero_table_probes_exceed_level_searches");
    }

    serde_json::json!({
        "passed": failures.is_empty(),
        "failures": failures,
        "owned_l0_table_bound": max_owned_l0_table_probes,
        "owned_nonzero_level_bound": max_owned_nonzero_level_searches,
        "inherited_l0_table_bound": max_inherited_l0_table_probes,
        "inherited_nonzero_level_bound": max_inherited_nonzero_level_searches,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct PointProbeShapeCounters {
    owned_l0_table_probes: u64,
    owned_nonzero_level_searches: u64,
    owned_nonzero_table_probes: u64,
    inherited_l0_table_probes: u64,
    inherited_nonzero_level_searches: u64,
    inherited_nonzero_table_probes: u64,
}

impl PointProbeShapeCounters {
    fn from_perf_trace(perf_trace: StoragePerfSnapshot) -> Self {
        Self {
            owned_l0_table_probes: perf_trace.point_owned_l0_table_probes(),
            owned_nonzero_level_searches: perf_trace.point_owned_nonzero_level_searches(),
            owned_nonzero_table_probes: perf_trace.point_owned_nonzero_table_probes(),
            inherited_l0_table_probes: perf_trace.point_inherited_l0_table_probes(),
            inherited_nonzero_level_searches: perf_trace.point_inherited_nonzero_level_searches(),
            inherited_nonzero_table_probes: perf_trace.point_inherited_nonzero_table_probes(),
        }
    }
}

fn ratio_json(numerator: u64, denominator: u64) -> serde_json::Value {
    if denominator == 0 {
        serde_json::Value::Null
    } else {
        serde_json::json!(numerator as f64 / denominator as f64)
    }
}

fn ns_per_row_as_ms_per_million_rows_json(nanoseconds: u64, rows: u64) -> serde_json::Value {
    ratio_json(nanoseconds, rows)
}

#[derive(Clone, Debug)]
struct SourceShapeContext {
    scale: usize,
    compaction_mode: SourceShapeCompactionMode,
    flush_status: Option<MaintenanceSummaryStatus>,
    flush_rows: u64,
    flush_maintenance_ns: Option<u64>,
    compact_status: Option<MaintenanceSummaryStatus>,
    compact_state_changes: usize,
    compact_maintenance_ns: Option<u64>,
    maintenance_queue: Option<MaintenanceQueueSnapshot>,
    final_layout: SourceLayoutSnapshot,
    source_shape_passed: bool,
    failures: Vec<String>,
}

impl SourceShapeContext {
    fn from_observed_report(
        scale: usize,
        compaction_mode: SourceShapeCompactionMode,
        queue: Option<MaintenanceQueueSummary>,
        report: &DiagnosticsSourceLayoutReport,
    ) -> Self {
        let final_layout = SourceLayoutSnapshot::from_report(report);
        let failures = compaction_mode.source_shape_failures(&final_layout);
        Self {
            scale,
            compaction_mode,
            flush_status: None,
            flush_rows: 0,
            flush_maintenance_ns: None,
            compact_status: None,
            compact_state_changes: 0,
            compact_maintenance_ns: None,
            maintenance_queue: queue.map(MaintenanceQueueSnapshot::from_summary),
            final_layout,
            source_shape_passed: failures.is_empty(),
            failures,
        }
    }

    fn from_report(
        scale: usize,
        flush: MaintenanceSummary,
        flush_elapsed: Duration,
        compact: MaintenanceSummary,
        compact_elapsed: Duration,
        compaction_mode: SourceShapeCompactionMode,
        queue: Option<MaintenanceQueueSummary>,
        report: &DiagnosticsSourceLayoutReport,
    ) -> Self {
        let final_layout = SourceLayoutSnapshot::from_report(report);
        let failures = compaction_mode.source_shape_failures(&final_layout);
        Self {
            scale,
            compaction_mode,
            flush_status: Some(flush.status()),
            flush_rows: flush.rows_processed(),
            flush_maintenance_ns: Some(nanos_u64(flush_elapsed)),
            compact_status: Some(compact.status()),
            compact_state_changes: compact.state_changes(),
            compact_maintenance_ns: Some(nanos_u64(compact_elapsed)),
            maintenance_queue: queue.map(MaintenanceQueueSnapshot::from_summary),
            final_layout,
            source_shape_passed: failures.is_empty(),
            failures,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SourceShapeCompactionMode {
    SingleSelectedOperation,
    ExplicitFixedPointDrain,
    AutomaticScheduling,
}

impl SourceShapeCompactionMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::SingleSelectedOperation => "single-selected-operation",
            Self::ExplicitFixedPointDrain => "explicit-fixed-point-drain",
            Self::AutomaticScheduling => "automatic-scheduling",
        }
    }

    const fn layout_assumption(self) -> &'static str {
        match self {
            Self::SingleSelectedOperation => {
                "diagnostics source layout after final flush and one selected compaction operation"
            }
            Self::ExplicitFixedPointDrain => {
                "diagnostics source layout after final flush and explicit fixed-point compaction drain"
            }
            Self::AutomaticScheduling => {
                "diagnostics source layout after normal load path and automatic maintenance scheduling"
            }
        }
    }

    fn source_shape_failures(self, layout: &SourceLayoutSnapshot) -> Vec<String> {
        match self {
            Self::SingleSelectedOperation | Self::ExplicitFixedPointDrain => {
                layout.compacted_shape_failures()
            }
            Self::AutomaticScheduling => Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MaintenanceQueueSnapshot {
    pending_tasks: usize,
    active_task: Option<u64>,
    enqueued: usize,
    coalesced: usize,
    max_pending_tasks: usize,
    started: usize,
    completed: usize,
    deferred: usize,
    failed: usize,
    canceled: usize,
    drained: usize,
    queue_full: usize,
}

impl MaintenanceQueueSnapshot {
    fn from_summary(summary: MaintenanceQueueSummary) -> Self {
        Self {
            pending_tasks: summary.pending_tasks(),
            active_task: summary.active_task(),
            enqueued: summary.enqueued(),
            coalesced: summary.coalesced(),
            max_pending_tasks: summary.max_pending_tasks(),
            started: summary.started(),
            completed: summary.completed(),
            deferred: summary.deferred(),
            failed: summary.failed(),
            canceled: summary.canceled(),
            drained: summary.drained(),
            queue_full: summary.queue_full(),
        }
    }
}

const SOURCE_SHAPE_COMPACTION_MODES: [SourceShapeCompactionMode; 3] = [
    SourceShapeCompactionMode::SingleSelectedOperation,
    SourceShapeCompactionMode::ExplicitFixedPointDrain,
    SourceShapeCompactionMode::AutomaticScheduling,
];

fn source_shape_compaction_modes_json() -> Vec<&'static str> {
    SOURCE_SHAPE_COMPACTION_MODES
        .iter()
        .map(|mode| mode.as_str())
        .collect()
}

#[derive(Clone, Debug)]
struct SourceLayoutSnapshot {
    active_rows: usize,
    frozen_table_count: usize,
    frozen_rows: usize,
    owned_l0_tables: usize,
    owned_nonzero_level_table_counts: Vec<LevelTableCountSnapshot>,
    owned_total_tables: usize,
    inherited_layers: usize,
    inherited_l0_tables: usize,
    inherited_nonzero_level_table_counts: Vec<LevelTableCountSnapshot>,
    inherited_total_tables: usize,
}

impl SourceLayoutSnapshot {
    fn from_report(report: &DiagnosticsSourceLayoutReport) -> Self {
        Self {
            active_rows: report.active_rows(),
            frozen_table_count: report.frozen_table_count(),
            frozen_rows: report.frozen_rows(),
            owned_l0_tables: report.owned_l0_tables(),
            owned_nonzero_level_table_counts: report
                .owned_nonzero_level_table_counts()
                .iter()
                .map(|count| LevelTableCountSnapshot {
                    level: count.level(),
                    table_count: count.table_count(),
                })
                .collect(),
            owned_total_tables: report.owned_total_tables(),
            inherited_layers: report.inherited_layers(),
            inherited_l0_tables: report.inherited_l0_tables(),
            inherited_nonzero_level_table_counts: report
                .inherited_nonzero_level_table_counts()
                .iter()
                .map(|count| LevelTableCountSnapshot {
                    level: count.level(),
                    table_count: count.table_count(),
                })
                .collect(),
            inherited_total_tables: report.inherited_total_tables(),
        }
    }

    fn compacted_shape_failures(&self) -> Vec<String> {
        let mut failures = Vec::new();
        if self.active_rows != 0 {
            failures.push("active_rows_nonzero".to_string());
        }
        if self.frozen_table_count != 0 {
            failures.push("frozen_table_count_nonzero".to_string());
        }
        if self.frozen_rows != 0 {
            failures.push("frozen_rows_nonzero".to_string());
        }
        if self.owned_l0_tables != 0 {
            failures.push("owned_l0_tables_nonzero".to_string());
        }
        if self.inherited_l0_tables != 0 {
            failures.push("inherited_l0_tables_nonzero".to_string());
        }
        failures
    }

    fn owned_nonzero_level_count(&self) -> u64 {
        self.owned_nonzero_level_table_counts.len() as u64
    }

    fn inherited_nonzero_level_count(&self) -> u64 {
        self.inherited_nonzero_level_table_counts.len() as u64
    }
}

#[derive(Clone, Debug)]
struct LevelTableCountSnapshot {
    level: u8,
    table_count: usize,
}

fn print_source_shape_context(context: &SourceShapeContext) {
    eprintln!(
        "  source-shape         passed={} compaction_mode={} final_l0={} owned_nonzero={} inherited_l0={} inherited_nonzero={} queue_final={} queue_max={} flush_status={} compact_status={} compact_changes={}",
        context.source_shape_passed,
        context.compaction_mode.as_str(),
        context.final_layout.owned_l0_tables,
        format_level_counts(&context.final_layout.owned_nonzero_level_table_counts),
        context.final_layout.inherited_l0_tables,
        format_level_counts(&context.final_layout.inherited_nonzero_level_table_counts),
        context
            .maintenance_queue
            .map_or_else(|| "unknown".to_string(), |queue| queue.pending_tasks.to_string()),
        context
            .maintenance_queue
            .map_or_else(|| "unknown".to_string(), |queue| queue.max_pending_tasks.to_string()),
        format_optional_maintenance_status(context.flush_status),
        format_optional_maintenance_status(context.compact_status),
        context.compact_state_changes,
    );
    if !context.failures.is_empty() {
        eprintln!("    source-shape failures={}", context.failures.join(","));
    }
}

fn source_shape_context_json(context: &SourceShapeContext) -> serde_json::Value {
    serde_json::json!({
        "scale_keys": context.scale,
        "compaction_mode": context.compaction_mode.as_str(),
        "passed": context.source_shape_passed,
        "failures": &context.failures,
        "final_layout": source_layout_json(&context.final_layout),
        "final_owned_l0_tables": context.final_layout.owned_l0_tables,
        "final_owned_nonzero_level_table_counts": level_counts_json(
            &context.final_layout.owned_nonzero_level_table_counts,
        ),
        "flush": {
            "status": format_optional_maintenance_status(context.flush_status),
            "rows_processed": context.flush_rows,
            "maintenance_ns": context.flush_maintenance_ns,
        },
        "compact": {
            "mode": context.compaction_mode.as_str(),
            "status": format_optional_maintenance_status(context.compact_status),
            "state_changes": context.compact_state_changes,
            "maintenance_ns": context.compact_maintenance_ns,
        },
        "maintenance_queue": context.maintenance_queue.map(maintenance_queue_json),
    })
}

fn maintenance_queue_json(queue: MaintenanceQueueSnapshot) -> serde_json::Value {
    serde_json::json!({
        "pending_tasks": queue.pending_tasks,
        "active_task": queue.active_task,
        "enqueued": queue.enqueued,
        "coalesced": queue.coalesced,
        "max_pending_tasks": queue.max_pending_tasks,
        "started": queue.started,
        "completed": queue.completed,
        "deferred": queue.deferred,
        "failed": queue.failed,
        "canceled": queue.canceled,
        "drained": queue.drained,
        "queue_full": queue.queue_full,
    })
}

fn source_layout_json(layout: &SourceLayoutSnapshot) -> serde_json::Value {
    serde_json::json!({
        "active_rows": layout.active_rows,
        "frozen_table_count": layout.frozen_table_count,
        "frozen_rows": layout.frozen_rows,
        "owned_l0_tables": layout.owned_l0_tables,
        "owned_nonzero_level_table_counts": level_counts_json(
            &layout.owned_nonzero_level_table_counts,
        ),
        "owned_total_tables": layout.owned_total_tables,
        "inherited_layers": layout.inherited_layers,
        "inherited_l0_tables": layout.inherited_l0_tables,
        "inherited_nonzero_level_table_counts": level_counts_json(
            &layout.inherited_nonzero_level_table_counts,
        ),
        "inherited_total_tables": layout.inherited_total_tables,
    })
}

fn level_counts_json(counts: &[LevelTableCountSnapshot]) -> serde_json::Value {
    serde_json::json!(counts
        .iter()
        .map(|count| {
            serde_json::json!({
                "level": count.level,
                "table_count": count.table_count,
            })
        })
        .collect::<Vec<_>>())
}

fn format_level_counts(counts: &[LevelTableCountSnapshot]) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|count| format!("L{}={}", count.level, count.table_count))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_maintenance_status(status: MaintenanceSummaryStatus) -> &'static str {
    match status {
        MaintenanceSummaryStatus::Completed => "completed",
        MaintenanceSummaryStatus::Deferred => "deferred",
        MaintenanceSummaryStatus::Failed => "failed",
        MaintenanceSummaryStatus::Canceled => "canceled",
        _ => "unknown",
    }
}

fn format_optional_maintenance_status(status: Option<MaintenanceSummaryStatus>) -> &'static str {
    status.map_or("not-run", format_maintenance_status)
}

fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

fn nanos_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug, Default)]
struct LoadPhaseTrace {
    batch_build_ns: u64,
    commit_call_ns: u64,
    maintenance_call_ns: u64,
    maintenance_runs: u64,
    maintenance_rows: u64,
    diagnostic_poll_ns: u64,
    diagnostic_polls: u64,
    automatic_maintenance_ns: u64,
    automatic_maintenance_attempts: u64,
    inline_maintenance_ns: u64,
    inline_maintenance_attempts: u64,
    background_maintenance_ns: u64,
    background_maintenance_tasks: u64,
    foreground_wait_background_lock_ns: u64,
    admission_block_wait_ns: u64,
    admission_wait_attempts: u64,
    admission_wait_timeouts: u64,
    maintenance_suggested_tasks: u64,
    maintenance_scheduled_tasks: u64,
    maintenance_coalesced_tasks: u64,
    maintenance_deferred_tasks: u64,
    wal_retained_bytes_last: Option<u64>,
    wal_retained_segments_last: Option<u64>,
    wal_retained_bytes_max: Option<u64>,
    wal_retained_segments_max: Option<u64>,
    wal_commits_since_checkpoint_last: Option<u64>,
    wal_retention_limit_bytes: Option<u64>,
    wal_retention_limit_segments: Option<u64>,
    wal_checkpoint_enqueue_events: u64,
    wal_checkpoint_coalesced_events: u64,
    checkpoint_executions: u64,
    wal_truncation_deleted_segments: u64,
    wal_truncation_protected_segments: u64,
    wal_truncation_failed_segments: u64,
}

impl LoadPhaseTrace {
    fn record_batch_build(&mut self, duration: Duration) {
        self.batch_build_ns = self
            .batch_build_ns
            .saturating_add(u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX));
    }

    fn record_commit_call(&mut self, duration: Duration) {
        self.commit_call_ns = self
            .commit_call_ns
            .saturating_add(u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX));
    }

    fn record_maintenance_call(&mut self, duration: Duration, rows_processed: u64) {
        self.maintenance_call_ns = self
            .maintenance_call_ns
            .saturating_add(u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX));
        self.maintenance_runs = self.maintenance_runs.saturating_add(1);
        self.maintenance_rows = self.maintenance_rows.saturating_add(rows_processed);
    }

    fn record_automatic_maintenance(&mut self, perf_trace: StoragePerfSnapshot) {
        self.inline_maintenance_ns = perf_trace.lifecycle_inline_maintenance_ns();
        self.inline_maintenance_attempts = perf_trace.lifecycle_inline_maintenance_attempts();
        self.background_maintenance_ns = perf_trace.lifecycle_background_task_total_ns();
        self.background_maintenance_tasks = perf_trace.lifecycle_background_tasks_completed();
        self.automatic_maintenance_ns = self
            .inline_maintenance_ns
            .saturating_add(self.background_maintenance_ns);
        self.automatic_maintenance_attempts = self
            .inline_maintenance_attempts
            .saturating_add(self.background_maintenance_tasks);
        self.foreground_wait_background_lock_ns =
            perf_trace.lifecycle_foreground_wait_background_lock_ns();
        self.admission_block_wait_ns = perf_trace.lifecycle_write_admission_block_wait_ns();
        self.admission_wait_attempts = perf_trace.lifecycle_write_admission_wait_attempts();
        self.admission_wait_timeouts = perf_trace.lifecycle_write_admission_wait_timeouts();
        self.maintenance_suggested_tasks =
            perf_trace.lifecycle_post_commit_maintenance_tasks_suggested();
        self.maintenance_scheduled_tasks =
            perf_trace.lifecycle_post_commit_maintenance_tasks_enqueued();
        self.maintenance_coalesced_tasks =
            perf_trace.lifecycle_post_commit_maintenance_tasks_coalesced();
        self.maintenance_deferred_tasks =
            perf_trace.lifecycle_post_commit_maintenance_tasks_deferred();
        if perf_trace.lifecycle_wal_retention_samples() > 0 {
            self.wal_retained_bytes_last = Some(perf_trace.lifecycle_wal_retained_bytes_last());
            self.wal_retained_segments_last =
                Some(perf_trace.lifecycle_wal_retained_segments_last());
            self.wal_retained_bytes_max = Some(perf_trace.lifecycle_wal_retained_bytes_max());
            self.wal_retained_segments_max = Some(perf_trace.lifecycle_wal_retained_segments_max());
        }
        self.wal_checkpoint_enqueue_events = perf_trace.lifecycle_wal_checkpoint_enqueue_events();
        self.wal_checkpoint_coalesced_events =
            perf_trace.lifecycle_wal_checkpoint_coalesced_events();
        self.checkpoint_executions = perf_trace.lifecycle_checkpoint_executions();
        self.wal_truncation_deleted_segments =
            perf_trace.lifecycle_wal_truncation_deleted_segments();
        self.wal_truncation_protected_segments =
            perf_trace.lifecycle_wal_truncation_protected_segments();
        self.wal_truncation_failed_segments = perf_trace.lifecycle_wal_truncation_failed_segments();
    }
}

#[derive(Debug)]
enum Measurement {
    Throughput { elapsed: Duration, ops: usize },
    Latency(TimedSamples),
}

impl Measurement {
    const fn operation_count(&self) -> u64 {
        match self {
            Self::Throughput { ops, .. } => *ops as u64,
            Self::Latency(samples) => samples.samples as u64,
        }
    }

    fn into_metrics(self) -> BenchmarkMetrics {
        match self {
            Self::Throughput { elapsed, ops } => BenchmarkMetrics {
                ops_per_sec: Some(ops as f64 / elapsed.as_secs_f64()),
                avg_ns: Some((elapsed.as_nanos() / ops.max(1) as u128) as u64),
                samples: Some(ops as u64),
                ..Default::default()
            },
            Self::Latency(samples) => BenchmarkMetrics {
                ops_per_sec: Some(samples.samples as f64 / samples.elapsed.as_secs_f64()),
                p50_ns: Some(samples.p50.as_nanos() as u64),
                p95_ns: Some(samples.p95.as_nanos() as u64),
                p99_ns: Some(samples.p99.as_nanos() as u64),
                min_ns: Some(samples.min.as_nanos() as u64),
                max_ns: Some(samples.max.as_nanos() as u64),
                avg_ns: Some(samples.avg.as_nanos() as u64),
                samples: Some(samples.samples as u64),
                ..Default::default()
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TimedSamples {
    samples: usize,
    elapsed: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    min: Duration,
    max: Duration,
    avg: Duration,
}

impl TimedSamples {
    fn new(mut latencies: Vec<Duration>, elapsed: Duration) -> Self {
        latencies.sort_unstable();
        let samples = latencies.len();
        let total_nanos = latencies.iter().map(Duration::as_nanos).sum::<u128>();
        Self {
            samples,
            elapsed,
            p50: percentile(&latencies, 50),
            p95: percentile(&latencies, 95),
            p99: percentile(&latencies, 99),
            min: latencies[0],
            max: latencies[samples - 1],
            avg: Duration::from_nanos((total_nanos / samples as u128) as u64),
        }
    }
}

fn percentile(latencies: &[Duration], percentile: usize) -> Duration {
    let index = (latencies.len() * percentile / 100).min(latencies.len() - 1);
    latencies[index]
}

#[derive(Debug)]
struct FastRng {
    state: u64,
}

impl FastRng {
    const fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x0005_DEEC_E66D,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        (self.next_u64() % upper as u64) as usize
    }
}

fn parse_list<T>(
    value: Option<&String>,
    parse_item: impl Fn(&str) -> Result<T, CliError>,
) -> Result<Vec<T>, CliError>
where
    T: Copy + Ord,
{
    let value = value.ok_or(CliError::MissingValue("list"))?;
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();
    for item in value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let parsed = parse_item(item)?;
        if seen.insert(parsed) {
            items.push(parsed);
        }
    }
    Ok(items)
}

fn parse_scale(value: &str) -> Result<usize, CliError> {
    let lower = value.to_ascii_lowercase();
    let (digits, multiplier) = match lower.strip_suffix('k') {
        Some(digits) => (digits, 1_000usize),
        None => match lower.strip_suffix('m') {
            Some(digits) => (digits, 1_000_000usize),
            None => (lower.as_str(), 1usize),
        },
    };
    digits
        .parse::<usize>()
        .ok()
        .and_then(|raw| raw.checked_mul(multiplier))
        .filter(|scale| *scale > 0)
        .ok_or_else(|| CliError::InvalidScale(value.to_string()))
}

fn parse_usize(value: Option<&String>, flag: &'static str) -> Result<usize, CliError> {
    value
        .ok_or(CliError::MissingValue(flag))?
        .parse::<usize>()
        .map_err(|_| CliError::InvalidNumber(flag))
}

fn parse_u64(value: Option<&String>, flag: &'static str) -> Result<u64, CliError> {
    value
        .ok_or(CliError::MissingValue(flag))?
        .parse::<u64>()
        .map_err(|_| CliError::InvalidNumber(flag))
}

/// Parse a binary byte size with an optional `k`/`m`/`g` suffix (1024-based),
/// e.g. `48g`, `512m`, `65536`. Used for `--memory-budget`.
fn parse_byte_size(value: Option<&String>, flag: &'static str) -> Result<u64, CliError> {
    let raw = value.ok_or(CliError::MissingValue(flag))?;
    let lower = raw.to_ascii_lowercase();
    let (digits, multiplier) = if let Some(digits) = lower.strip_suffix('g') {
        (digits, 1024u64 * 1024 * 1024)
    } else if let Some(digits) = lower.strip_suffix('m') {
        (digits, 1024u64 * 1024)
    } else if let Some(digits) = lower.strip_suffix('k') {
        (digits, 1024u64)
    } else {
        (lower.as_str(), 1u64)
    };
    digits
        .trim()
        .parse::<u64>()
        .ok()
        .and_then(|raw| raw.checked_mul(multiplier))
        .filter(|bytes| *bytes > 0)
        .ok_or(CliError::InvalidNumber(flag))
}

fn value(value: Option<&String>, flag: &'static str) -> Result<String, CliError> {
    value.cloned().ok_or(CliError::MissingValue(flag))
}

fn unix_nanos_now() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[derive(Debug)]
enum CliError {
    Help,
    MissingValue(&'static str),
    EmptyList(&'static str),
    UnknownFlag(String),
    InvalidScale(String),
    InvalidEngine(String),
    InvalidWorkload(String),
    InvalidNumber(&'static str),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => f.write_str("help requested"),
            Self::MissingValue(flag) => write!(f, "missing value for {flag}"),
            Self::EmptyList(flag) => write!(f, "{flag} must not be empty"),
            Self::UnknownFlag(flag) => write!(f, "unknown flag {flag}"),
            Self::InvalidScale(value) => write!(f, "invalid scale {value}"),
            Self::InvalidEngine(value) => write!(f, "invalid engine {value}"),
            Self::InvalidWorkload(value) => write!(f, "invalid workload {value}"),
            Self::InvalidNumber(flag) => write!(f, "invalid numeric value for {flag}"),
        }
    }
}

#[derive(Debug)]
enum BenchmarkError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Storage(StorageApiError),
    MaintenanceDidNotComplete {
        after_rows: usize,
        status: MaintenanceSummaryStatus,
        reason: Option<&'static str>,
    },
    MaintenanceTaskDidNotFinish {
        task: MaintenanceTask,
        after_rows: usize,
        status: MaintenanceSummaryStatus,
        reason: Option<&'static str>,
    },
    SourceLayoutUnavailable,
    SourceShapeDidNotPass {
        failures: Vec<String>,
    },
    MissingInitialBranch,
    MissingRow,
}

impl From<std::io::Error> for BenchmarkError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for BenchmarkError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<StorageApiError> for BenchmarkError {
    fn from(error: StorageApiError) -> Self {
        Self::Storage(error)
    }
}

impl fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Json(error) => write!(f, "json error: {error}"),
            Self::Storage(error) => write!(f, "storage API error: {error}"),
            Self::MaintenanceDidNotComplete {
                after_rows,
                status,
                reason,
            } => write!(
                f,
                "flush maintenance after {after_rows} loaded rows did not complete: status={status:?} reason={}",
                reason.unwrap_or("none")
            ),
            Self::MaintenanceTaskDidNotFinish {
                task,
                after_rows,
                status,
                reason,
            } => write!(
                f,
                "{task:?} maintenance after {after_rows} loaded rows did not finish: status={status:?} reason={}",
                reason.unwrap_or("none")
            ),
            Self::SourceLayoutUnavailable => {
                f.write_str("diagnostics did not report a known branch source layout")
            }
            Self::SourceShapeDidNotPass { failures } => write!(
                f,
                "post-load source shape did not pass: {}",
                failures.join(",")
            ),
            Self::MissingInitialBranch => {
                f.write_str("storage runtime did not list an active branch")
            }
            Self::MissingRow => f.write_str("benchmark read expected a loaded row"),
        }
    }
}

impl std::error::Error for BenchmarkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::MaintenanceDidNotComplete { .. }
            | Self::MaintenanceTaskDidNotFinish { .. }
            | Self::SourceLayoutUnavailable
            | Self::SourceShapeDidNotPass { .. }
            | Self::MissingInitialBranch
            | Self::MissingRow => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_final_drain_is_opt_in() {
        let default_config = Config::parse(std::iter::empty()).expect("default config");
        assert!(!default_config.diagnostic_source_shape);
        assert!(!default_config.diagnostic_final_drain);

        let load_only_config =
            Config::parse(["--workloads".to_string(), "load-seq".to_string()].into_iter())
                .expect("load-only config");
        assert!(!load_only_config.diagnostic_source_shape);
        assert!(!load_only_config.diagnostic_final_drain);
        assert!(!load_only_config.should_prepare_loaded_source_shape());

        let observing_config = Config::parse(
            [
                "--workloads".to_string(),
                "load-seq".to_string(),
                "--diagnostic-source-shape".to_string(),
            ]
            .into_iter(),
        )
        .expect("diagnostic source-shape config");
        assert!(observing_config.diagnostic_source_shape);
        assert!(observing_config.should_prepare_loaded_source_shape());

        let draining_config = Config::parse(
            [
                "--workloads".to_string(),
                "load-seq".to_string(),
                "--diagnostic-final-drain".to_string(),
            ]
            .into_iter(),
        )
        .expect("diagnostic drain config");
        assert!(draining_config.diagnostic_final_drain);
        assert!(draining_config.should_prepare_loaded_source_shape());
    }

    #[test]
    fn benchmark_normal_path_uses_automatic_source_shape_observation() {
        let source = include_str!("storage_next_l9_scale.rs");
        let prepare_source = source
            .split("fn prepare_loaded_source_shape")
            .nth(1)
            .expect("prepare_loaded_source_shape is present")
            .split("fn observe_loaded_source_shape")
            .next()
            .expect("prepare function precedes observe helper");
        assert!(
            prepare_source.contains("if config.diagnostic_final_drain"),
            "benchmark source-shape preparation must gate explicit drains behind the diagnostic flag"
        );
        assert!(
            prepare_source.contains("return drain_loaded_source_shape"),
            "diagnostic final drain must stay opt-in and explicit"
        );
        assert!(
            prepare_source.contains("observe_loaded_source_shape(runtime, branch_id, scale)"),
            "normal benchmark path must observe automatic source shape"
        );

        let load_source = source
            .split("fn run_load_seq")
            .nth(1)
            .expect("run_load_seq is present")
            .split("fn prepare_loaded_source_shape")
            .next()
            .expect("load path precedes source-shape preparation");
        assert!(
            !load_source.contains("drain_loaded_source_shape"),
            "load path must not call explicit source-shape drain"
        );
        assert!(
            !load_source.contains("MaintenanceTask::Compact"),
            "load path must not force compaction to make later reads possible"
        );
        assert!(
            !load_source.contains("drain_maintenance"),
            "load path must not call public fixed-point maintenance drain"
        );
        assert!(
            !load_source.contains("runtime.diagnostics"),
            "load path must not poll global diagnostics while measuring foreground writes"
        );
        assert!(
            !load_source.contains("DiagnosticsRequest::new(DiagnosticsScope::Global)"),
            "load path must not collect source-shape/WAL facts by polling global diagnostics"
        );

        let run_source = source
            .split("fn run(config: Config)")
            .nth(1)
            .expect("run function is present")
            .split("fn run_load_seq")
            .next()
            .expect("run orchestration precedes load helper");
        assert!(
            run_source.contains("let mut load_result = None"),
            "load result must be held until post-load diagnostics can be attached"
        );
        let load_index = run_source
            .find("run_load_seq")
            .expect("run orchestration calls load helper");
        let source_shape_index = run_source
            .find("config.should_prepare_loaded_source_shape()")
            .expect("run orchestration checks post-load source-shape opt-in");
        let print_index = run_source
            .find("print_result(&result)")
            .expect("run orchestration prints the load result");
        assert!(
            load_index < source_shape_index && source_shape_index < print_index,
            "load-only source-shape diagnostics must run after timed load and before result serialization"
        );
    }

    #[test]
    fn benchmark_result_records_source_shape_metrics_with_load_context() {
        perf_trace::reset();
        let config = Config::parse(std::iter::empty()).expect("default config");
        let load_phase = LoadPhaseTrace {
            maintenance_runs: 2,
            maintenance_rows: 1_000,
            ..LoadPhaseTrace::default()
        };

        let result = RunResult::throughput(
            Workload::PointLatestThroughput,
            Engine::Cache,
            1_000,
            5,
            Duration::from_secs(1),
        )
        .with_load_phase_context(Some(load_phase))
        .with_perf_trace(perf_trace::snapshot())
        .into_benchmark_result(&config);

        let load_trace = result
            .parameters
            .get("load_phase_trace")
            .expect("load phase trace");
        assert_eq!(load_trace["maintenance_runs"].as_u64(), Some(2));
        assert_eq!(load_trace["maintenance_rows"].as_u64(), Some(1_000));

        let metrics = result
            .parameters
            .get("source_shape_metrics")
            .expect("source shape metrics");
        assert_eq!(metrics["point_source_probes_per_read"].as_f64(), Some(0.0));
        assert_eq!(
            metrics["point_nonzero_table_probes_per_read"].as_f64(),
            Some(0.0)
        );
        assert_eq!(metrics["scan_source_cursors_per_call"].as_f64(), Some(0.0));
        assert_eq!(
            metrics["scan_table_cursors_opened_per_call"].as_f64(),
            Some(0.0)
        );
        assert!(metrics["scan_rows_visited_per_row_returned"].is_null());
        assert_eq!(
            metrics["load_maintenance_ms_per_million_rows"].as_f64(),
            Some(0.0)
        );
        assert_eq!(metrics["automatic_maintenance_ns"].as_u64(), Some(0));
        assert_eq!(metrics["automatic_maintenance_attempts"].as_u64(), Some(0));
        assert_eq!(metrics["inline_maintenance_ns"].as_u64(), Some(0));
        assert_eq!(metrics["inline_maintenance_attempts"].as_u64(), Some(0));
        assert_eq!(metrics["background_maintenance_ns"].as_u64(), Some(0));
        assert_eq!(metrics["background_maintenance_tasks"].as_u64(), Some(0));
        assert_eq!(
            metrics["foreground_wait_background_lock_ns"].as_u64(),
            Some(0)
        );
        assert_eq!(metrics["admission_block_wait_ns"].as_u64(), Some(0));
        assert_eq!(metrics["admission_wait_attempts"].as_u64(), Some(0));
        assert_eq!(metrics["admission_wait_timeouts"].as_u64(), Some(0));
        assert_eq!(
            metrics["automatic_maintenance_ms_per_million_rows"].as_f64(),
            Some(0.0)
        );
        assert!(metrics["l0_tables_per_million_rows_after_load"].is_null());
        assert!(metrics["maintenance_queue_depth_final"].is_null());
        assert!(metrics["maintenance_queue_depth_max"].is_null());
        assert!(metrics["post_load_compaction_mode"].is_null());
        assert_eq!(metrics["logical_write_rows"].as_u64(), Some(1_000));
        assert_eq!(metrics["logical_write_bytes"].as_u64(), Some(64_000));
        assert_eq!(metrics["compaction_row_amplification"].as_f64(), Some(0.0));
        assert_eq!(metrics["compaction_byte_amplification"].as_f64(), Some(0.0));
        assert_eq!(metrics["assumptions"]["operation_count"].as_u64(), Some(5));
        assert_eq!(
            metrics["assumptions"]["known_compaction_mode_values"]
                .as_array()
                .expect("known mode values")
                .len(),
            3
        );
    }

    #[test]
    fn source_shape_metrics_use_null_for_unavailable_denominators() {
        perf_trace::reset();
        let metrics = source_shape_metrics_json(0, 0, 64, perf_trace::snapshot(), None, None);

        assert!(metrics["point_source_probes_per_read"].is_null());
        assert!(metrics["point_nonzero_table_probes_per_read"].is_null());
        assert!(metrics["scan_source_cursors_per_call"].is_null());
        assert!(metrics["scan_table_cursors_opened_per_call"].is_null());
        assert!(metrics["scan_rows_visited_per_row_returned"].is_null());
        assert!(metrics["l0_tables_per_million_rows_after_load"].is_null());
        assert!(metrics["post_load_compaction_mode"].is_null());

        let load_metrics = source_shape_metrics_json(
            0,
            0,
            64,
            perf_trace::snapshot(),
            Some(LoadPhaseTrace {
                maintenance_runs: 1,
                ..LoadPhaseTrace::default()
            }),
            None,
        );
        assert!(load_metrics["l0_tables_per_million_rows_after_load"].is_null());
    }

    #[test]
    fn source_shape_metrics_preserve_load_maintenance_after_read_trace_reset() {
        perf_trace::reset();
        let metrics = source_shape_metrics_json(
            1_000,
            10,
            64,
            perf_trace::snapshot(),
            Some(LoadPhaseTrace {
                maintenance_runs: 2,
                diagnostic_poll_ns: 123,
                diagnostic_polls: 4,
                automatic_maintenance_ns: 4_000,
                automatic_maintenance_attempts: 3,
                inline_maintenance_ns: 1_000,
                inline_maintenance_attempts: 1,
                background_maintenance_ns: 3_000,
                background_maintenance_tasks: 2,
                foreground_wait_background_lock_ns: 50,
                admission_block_wait_ns: 70,
                admission_wait_attempts: 3,
                admission_wait_timeouts: 1,
                wal_retained_bytes_last: Some(90),
                wal_retained_segments_last: Some(4),
                wal_retained_bytes_max: Some(120),
                wal_retained_segments_max: Some(5),
                wal_commits_since_checkpoint_last: Some(6),
                wal_retention_limit_bytes: Some(1_000),
                wal_retention_limit_segments: Some(8),
                wal_checkpoint_enqueue_events: 1,
                wal_checkpoint_coalesced_events: 2,
                checkpoint_executions: 3,
                wal_truncation_deleted_segments: 4,
                wal_truncation_protected_segments: 5,
                wal_truncation_failed_segments: 6,
                maintenance_suggested_tasks: 5,
                maintenance_scheduled_tasks: 2,
                maintenance_coalesced_tasks: 1,
                maintenance_deferred_tasks: 1,
                ..LoadPhaseTrace::default()
            }),
            None,
        );

        assert_eq!(metrics["automatic_maintenance_ns"].as_u64(), Some(4_000));
        assert_eq!(metrics["diagnostic_poll_ns"].as_u64(), Some(123));
        assert_eq!(metrics["diagnostic_polls"].as_u64(), Some(4));
        assert_eq!(metrics["automatic_maintenance_attempts"].as_u64(), Some(3));
        assert_eq!(metrics["inline_maintenance_ns"].as_u64(), Some(1_000));
        assert_eq!(metrics["inline_maintenance_attempts"].as_u64(), Some(1));
        assert_eq!(metrics["background_maintenance_ns"].as_u64(), Some(3_000));
        assert_eq!(metrics["background_maintenance_tasks"].as_u64(), Some(2));
        assert_eq!(
            metrics["foreground_wait_background_lock_ns"].as_u64(),
            Some(50)
        );
        assert_eq!(metrics["admission_block_wait_ns"].as_u64(), Some(70));
        assert_eq!(metrics["admission_wait_attempts"].as_u64(), Some(3));
        assert_eq!(metrics["admission_wait_timeouts"].as_u64(), Some(1));
        assert_eq!(metrics["wal_retained_bytes_last"].as_u64(), Some(90));
        assert_eq!(metrics["wal_retained_segments_last"].as_u64(), Some(4));
        assert_eq!(metrics["wal_retained_bytes_max"].as_u64(), Some(120));
        assert_eq!(metrics["wal_retained_segments_max"].as_u64(), Some(5));
        assert_eq!(
            metrics["wal_commits_since_checkpoint_last"].as_u64(),
            Some(6)
        );
        assert_eq!(metrics["wal_retention_limit_bytes"].as_u64(), Some(1_000));
        assert_eq!(metrics["wal_retention_limit_segments"].as_u64(), Some(8));
        assert_eq!(metrics["wal_checkpoint_enqueue_events"].as_u64(), Some(1));
        assert_eq!(metrics["wal_checkpoint_coalesced_events"].as_u64(), Some(2));
        assert_eq!(metrics["checkpoint_executions"].as_u64(), Some(3));
        assert_eq!(metrics["wal_truncation_deleted_segments"].as_u64(), Some(4));
        assert_eq!(
            metrics["wal_truncation_protected_segments"].as_u64(),
            Some(5)
        );
        assert_eq!(metrics["wal_truncation_failed_segments"].as_u64(), Some(6));
        assert_eq!(
            metrics["automatic_maintenance_ms_per_million_rows"].as_f64(),
            Some(4.0)
        );
        assert_eq!(metrics["maintenance_suggested_tasks"].as_u64(), Some(5));
        assert_eq!(metrics["maintenance_scheduled_tasks"].as_u64(), Some(2));
        assert_eq!(metrics["maintenance_coalesced_tasks"].as_u64(), Some(1));
        assert_eq!(metrics["maintenance_deferred_tasks"].as_u64(), Some(1));
        assert_eq!(
            metrics["scheduled_maintenance_tasks_per_explicit_flush"].as_f64(),
            Some(1.5)
        );
    }

    #[test]
    fn benchmark_result_records_diagnostic_drain_and_scheduler_metadata() {
        perf_trace::reset();
        let config = Config::parse(["--diagnostic-final-drain".to_string()].into_iter())
            .expect("diagnostic drain config");
        let load_phase = LoadPhaseTrace {
            batch_build_ns: 11,
            commit_call_ns: 22,
            maintenance_call_ns: 33,
            maintenance_runs: 4,
            maintenance_rows: 800,
            diagnostic_poll_ns: 99,
            diagnostic_polls: 5,
            automatic_maintenance_ns: 5_000,
            automatic_maintenance_attempts: 6,
            inline_maintenance_ns: 2_000,
            inline_maintenance_attempts: 2,
            background_maintenance_ns: 3_000,
            background_maintenance_tasks: 4,
            foreground_wait_background_lock_ns: 44,
            admission_block_wait_ns: 66,
            admission_wait_attempts: 3,
            admission_wait_timeouts: 1,
            maintenance_suggested_tasks: 7,
            maintenance_scheduled_tasks: 3,
            maintenance_coalesced_tasks: 2,
            maintenance_deferred_tasks: 1,
            wal_retained_bytes_last: Some(128),
            wal_retained_segments_last: Some(2),
            wal_retained_bytes_max: Some(256),
            wal_retained_segments_max: Some(3),
            wal_commits_since_checkpoint_last: Some(9),
            wal_retention_limit_bytes: Some(512),
            wal_retention_limit_segments: Some(4),
            wal_checkpoint_enqueue_events: 2,
            wal_checkpoint_coalesced_events: 3,
            checkpoint_executions: 4,
            wal_truncation_deleted_segments: 5,
            wal_truncation_protected_segments: 6,
            wal_truncation_failed_segments: 7,
            ..LoadPhaseTrace::default()
        };

        let result = RunResult::throughput(
            Workload::PointLatestThroughput,
            Engine::Cache,
            2_000,
            8,
            Duration::from_secs(1),
        )
        .with_load_phase_trace(load_phase)
        .with_perf_trace(perf_trace::snapshot())
        .into_benchmark_result(&config);

        assert_eq!(
            result.parameters["diagnostic_source_shape"].as_bool(),
            Some(false)
        );
        assert_eq!(
            result.parameters["diagnostic_final_drain"].as_bool(),
            Some(true)
        );
        let trace = &result.parameters["load_phase_trace"];
        assert_eq!(trace["batch_build_ns"].as_u64(), Some(11));
        assert_eq!(trace["commit_call_ns"].as_u64(), Some(22));
        assert_eq!(trace["maintenance_call_ns"].as_u64(), Some(33));
        assert_eq!(trace["maintenance_runs"].as_u64(), Some(4));
        assert_eq!(trace["maintenance_rows"].as_u64(), Some(800));
        assert_eq!(trace["diagnostic_poll_ns"].as_u64(), Some(99));
        assert_eq!(trace["diagnostic_polls"].as_u64(), Some(5));
        assert_eq!(trace["automatic_maintenance_ns"].as_u64(), Some(5_000));
        assert_eq!(trace["automatic_maintenance_attempts"].as_u64(), Some(6));
        assert_eq!(trace["inline_maintenance_ns"].as_u64(), Some(2_000));
        assert_eq!(trace["inline_maintenance_attempts"].as_u64(), Some(2));
        assert_eq!(trace["background_maintenance_ns"].as_u64(), Some(3_000));
        assert_eq!(trace["background_maintenance_tasks"].as_u64(), Some(4));
        assert_eq!(
            trace["foreground_wait_background_lock_ns"].as_u64(),
            Some(44)
        );
        assert_eq!(trace["admission_block_wait_ns"].as_u64(), Some(66));
        assert_eq!(trace["admission_wait_attempts"].as_u64(), Some(3));
        assert_eq!(trace["admission_wait_timeouts"].as_u64(), Some(1));
        assert_eq!(trace["maintenance_suggested_tasks"].as_u64(), Some(7));
        assert_eq!(trace["maintenance_scheduled_tasks"].as_u64(), Some(3));
        assert_eq!(trace["maintenance_coalesced_tasks"].as_u64(), Some(2));
        assert_eq!(trace["maintenance_deferred_tasks"].as_u64(), Some(1));
        assert_eq!(trace["wal_retained_bytes_last"].as_u64(), Some(128));
        assert_eq!(trace["wal_retained_segments_last"].as_u64(), Some(2));
        assert_eq!(trace["wal_retained_bytes_max"].as_u64(), Some(256));
        assert_eq!(trace["wal_retained_segments_max"].as_u64(), Some(3));
        assert_eq!(trace["wal_commits_since_checkpoint_last"].as_u64(), Some(9));
        assert_eq!(trace["wal_retention_limit_bytes"].as_u64(), Some(512));
        assert_eq!(trace["wal_retention_limit_segments"].as_u64(), Some(4));
        assert_eq!(trace["wal_checkpoint_enqueue_events"].as_u64(), Some(2));
        assert_eq!(trace["wal_checkpoint_coalesced_events"].as_u64(), Some(3));
        assert_eq!(trace["checkpoint_executions"].as_u64(), Some(4));
        assert_eq!(trace["wal_truncation_deleted_segments"].as_u64(), Some(5));
        assert_eq!(trace["wal_truncation_protected_segments"].as_u64(), Some(6));
        assert_eq!(trace["wal_truncation_failed_segments"].as_u64(), Some(7));

        let perf_trace = &result.parameters["perf_trace"];
        for field in [
            "lifecycle_compaction_operations_completed",
            "lifecycle_compaction_l0_operations",
            "lifecycle_compaction_l0_to_level_one_operations",
            "lifecycle_compaction_nonzero_operations",
            "lifecycle_compaction_bottommost_operations",
            "lifecycle_compaction_input_tables",
            "lifecycle_compaction_overlap_tables",
            "lifecycle_compaction_output_tables",
            "lifecycle_compaction_selected",
            "lifecycle_compaction_selected_level_sum",
            "lifecycle_compaction_selected_score_sum",
            "lifecycle_compaction_selected_table_count",
            "lifecycle_compaction_selected_byte_count",
            "lifecycle_compaction_selected_target_bytes",
            "lifecycle_compaction_nonzero_input_selections",
            "lifecycle_compaction_nonzero_input_level_sum",
            "lifecycle_compaction_nonzero_input_table_index_sum",
            "lifecycle_compaction_nonzero_input_bytes",
            "lifecycle_compaction_nonzero_input_rows",
            "lifecycle_compaction_nonzero_input_pointer_selections",
            "table_compaction_merge_ns",
            "table_compaction_merge_input_rows",
            "table_compaction_merge_ns_per_input_row",
            "table_compaction_boundary_key_buffer_allocations",
            "table_compaction_boundary_key_buffer_reuses",
            "table_compaction_previous_key_buffer_allocations",
            "table_compaction_previous_key_buffer_reuses",
            "table_build_facts_from_streaming_metadata",
            "table_rewrite_redundant_fact_decodes_avoided",
            "table_rewrite_reader_reopens_performed",
        ] {
            assert_eq!(
                perf_trace[field].as_u64(),
                Some(0),
                "perf trace must expose merge-cost counter {field}"
            );
        }

        let metrics = &result.parameters["source_shape_metrics"];
        assert_eq!(metrics["automatic_maintenance_ns"].as_u64(), Some(5_000));
        assert_eq!(metrics["diagnostic_poll_ns"].as_u64(), Some(99));
        assert_eq!(metrics["diagnostic_polls"].as_u64(), Some(5));
        assert_eq!(metrics["automatic_maintenance_attempts"].as_u64(), Some(6));
        assert_eq!(metrics["inline_maintenance_ns"].as_u64(), Some(2_000));
        assert_eq!(metrics["inline_maintenance_attempts"].as_u64(), Some(2));
        assert_eq!(metrics["background_maintenance_ns"].as_u64(), Some(3_000));
        assert_eq!(metrics["background_maintenance_tasks"].as_u64(), Some(4));
        assert_eq!(
            metrics["foreground_wait_background_lock_ns"].as_u64(),
            Some(44)
        );
        assert_eq!(metrics["admission_block_wait_ns"].as_u64(), Some(66));
        assert_eq!(metrics["admission_wait_attempts"].as_u64(), Some(3));
        assert_eq!(metrics["admission_wait_timeouts"].as_u64(), Some(1));
        assert_eq!(metrics["wal_retained_bytes_last"].as_u64(), Some(128));
        assert_eq!(metrics["wal_retained_segments_last"].as_u64(), Some(2));
        assert_eq!(metrics["wal_retained_bytes_max"].as_u64(), Some(256));
        assert_eq!(metrics["wal_retained_segments_max"].as_u64(), Some(3));
        assert_eq!(
            metrics["wal_commits_since_checkpoint_last"].as_u64(),
            Some(9)
        );
        assert_eq!(metrics["wal_retention_limit_bytes"].as_u64(), Some(512));
        assert_eq!(metrics["wal_retention_limit_segments"].as_u64(), Some(4));
        assert_eq!(metrics["wal_checkpoint_enqueue_events"].as_u64(), Some(2));
        assert_eq!(metrics["wal_checkpoint_coalesced_events"].as_u64(), Some(3));
        assert_eq!(metrics["checkpoint_executions"].as_u64(), Some(4));
        assert_eq!(metrics["wal_truncation_deleted_segments"].as_u64(), Some(5));
        assert_eq!(
            metrics["wal_truncation_protected_segments"].as_u64(),
            Some(6)
        );
        assert_eq!(metrics["wal_truncation_failed_segments"].as_u64(), Some(7));
        assert_eq!(
            metrics["automatic_maintenance_ms_per_million_rows"].as_f64(),
            Some(2.5)
        );
        assert_eq!(metrics["maintenance_suggested_tasks"].as_u64(), Some(7));
        assert_eq!(metrics["maintenance_scheduled_tasks"].as_u64(), Some(3));
        assert_eq!(metrics["maintenance_coalesced_tasks"].as_u64(), Some(2));
        assert_eq!(metrics["maintenance_deferred_tasks"].as_u64(), Some(1));
        assert_eq!(
            metrics["scheduled_maintenance_tasks_per_explicit_flush"].as_f64(),
            Some(1.25)
        );
    }

    #[test]
    fn source_shape_metrics_use_final_layout_for_l0_density() {
        perf_trace::reset();
        let source_shape = SourceShapeContext {
            scale: 1_000,
            compaction_mode: SourceShapeCompactionMode::ExplicitFixedPointDrain,
            flush_status: Some(MaintenanceSummaryStatus::Completed),
            flush_rows: 1_000,
            flush_maintenance_ns: Some(1),
            compact_status: Some(MaintenanceSummaryStatus::Completed),
            compact_state_changes: 1,
            compact_maintenance_ns: Some(1),
            maintenance_queue: Some(MaintenanceQueueSnapshot {
                pending_tasks: 0,
                active_task: None,
                enqueued: 2,
                coalesced: 1,
                max_pending_tasks: 3,
                started: 2,
                completed: 2,
                deferred: 2,
                failed: 0,
                canceled: 0,
                drained: 0,
                queue_full: 0,
            }),
            final_layout: SourceLayoutSnapshot {
                active_rows: 0,
                frozen_table_count: 0,
                frozen_rows: 0,
                owned_l0_tables: 2,
                owned_nonzero_level_table_counts: Vec::new(),
                owned_total_tables: 2,
                inherited_layers: 0,
                inherited_l0_tables: 0,
                inherited_nonzero_level_table_counts: Vec::new(),
                inherited_total_tables: 0,
            },
            source_shape_passed: false,
            failures: vec!["owned_l0_tables_nonzero".to_string()],
        };

        let metrics = source_shape_metrics_json(
            1_000,
            1,
            64,
            perf_trace::snapshot(),
            None,
            Some(&source_shape),
        );

        assert_eq!(
            metrics["l0_tables_per_million_rows_after_load"].as_f64(),
            Some(2_000.0)
        );
        assert_eq!(
            metrics["throughput_interpretation"].as_str(),
            Some("source-shape-failed")
        );
        assert_eq!(
            metrics["post_load_compaction_mode"].as_str(),
            Some("explicit-fixed-point-drain")
        );
        assert_eq!(
            metrics["post_load_source_shape"]["compact"]["mode"].as_str(),
            Some("explicit-fixed-point-drain")
        );
        assert_eq!(metrics["maintenance_queue_depth_final"].as_u64(), Some(0));
        assert_eq!(metrics["maintenance_queue_depth_max"].as_u64(), Some(3));
        assert_eq!(
            metrics["maintenance_queue_deferred_outcomes_per_million_rows"].as_f64(),
            Some(2_000.0)
        );
        assert_eq!(
            metrics["assumptions"]["l0_tables_after_load"].as_str(),
            Some(
                "diagnostics source layout after final flush and explicit fixed-point compaction drain"
            )
        );
    }

    #[test]
    fn source_layout_snapshot_marks_uncompacted_sources_as_failed() {
        let layout = SourceLayoutSnapshot {
            active_rows: 1,
            frozen_table_count: 1,
            frozen_rows: 1,
            owned_l0_tables: 1,
            owned_nonzero_level_table_counts: vec![LevelTableCountSnapshot {
                level: 7,
                table_count: 1,
            }],
            owned_total_tables: 2,
            inherited_layers: 0,
            inherited_l0_tables: 0,
            inherited_nonzero_level_table_counts: Vec::new(),
            inherited_total_tables: 0,
        };

        let failures = layout.compacted_shape_failures();

        assert!(failures.contains(&"active_rows_nonzero".to_string()));
        assert!(failures.contains(&"frozen_table_count_nonzero".to_string()));
        assert!(failures.contains(&"owned_l0_tables_nonzero".to_string()));
    }

    #[test]
    fn automatic_source_shape_observation_does_not_require_compacted_sources() {
        let layout = SourceLayoutSnapshot {
            active_rows: 2,
            frozen_table_count: 1,
            frozen_rows: 3,
            owned_l0_tables: 4,
            owned_nonzero_level_table_counts: vec![LevelTableCountSnapshot {
                level: 2,
                table_count: 1,
            }],
            owned_total_tables: 5,
            inherited_layers: 1,
            inherited_l0_tables: 1,
            inherited_nonzero_level_table_counts: Vec::new(),
            inherited_total_tables: 1,
        };

        assert!(SourceShapeCompactionMode::AutomaticScheduling
            .source_shape_failures(&layout)
            .is_empty());

        let explicit_failures =
            SourceShapeCompactionMode::ExplicitFixedPointDrain.source_shape_failures(&layout);
        assert!(explicit_failures.contains(&"active_rows_nonzero".to_string()));
        assert!(explicit_failures.contains(&"frozen_table_count_nonzero".to_string()));
        assert!(explicit_failures.contains(&"frozen_rows_nonzero".to_string()));
        assert!(explicit_failures.contains(&"owned_l0_tables_nonzero".to_string()));
        assert!(explicit_failures.contains(&"inherited_l0_tables_nonzero".to_string()));
    }

    #[test]
    fn source_shape_context_json_separates_observation_from_diagnostic_drain() {
        let automatic_context = SourceShapeContext {
            scale: 1_000,
            compaction_mode: SourceShapeCompactionMode::AutomaticScheduling,
            flush_status: None,
            flush_rows: 0,
            flush_maintenance_ns: None,
            compact_status: None,
            compact_state_changes: 0,
            compact_maintenance_ns: None,
            maintenance_queue: None,
            final_layout: SourceLayoutSnapshot {
                active_rows: 1,
                frozen_table_count: 0,
                frozen_rows: 0,
                owned_l0_tables: 1,
                owned_nonzero_level_table_counts: Vec::new(),
                owned_total_tables: 1,
                inherited_layers: 0,
                inherited_l0_tables: 0,
                inherited_nonzero_level_table_counts: Vec::new(),
                inherited_total_tables: 0,
            },
            source_shape_passed: true,
            failures: Vec::new(),
        };
        let automatic_json = source_shape_context_json(&automatic_context);

        assert_eq!(
            automatic_json["compaction_mode"].as_str(),
            Some("automatic-scheduling")
        );
        assert_eq!(automatic_json["flush"]["status"].as_str(), Some("not-run"));
        assert!(automatic_json["flush"]["maintenance_ns"].is_null());
        assert_eq!(
            automatic_json["compact"]["status"].as_str(),
            Some("not-run")
        );
        assert_eq!(
            automatic_json["compact"]["mode"].as_str(),
            Some("automatic-scheduling")
        );
        assert!(automatic_json["compact"]["maintenance_ns"].is_null());
        assert!(automatic_json["maintenance_queue"].is_null());

        let drained_context = SourceShapeContext {
            scale: 1_000,
            compaction_mode: SourceShapeCompactionMode::ExplicitFixedPointDrain,
            flush_status: Some(MaintenanceSummaryStatus::Completed),
            flush_rows: 1_000,
            flush_maintenance_ns: Some(12),
            compact_status: Some(MaintenanceSummaryStatus::Completed),
            compact_state_changes: 3,
            compact_maintenance_ns: Some(34),
            maintenance_queue: Some(MaintenanceQueueSnapshot {
                pending_tasks: 1,
                active_task: Some(42),
                enqueued: 5,
                coalesced: 2,
                max_pending_tasks: 4,
                started: 3,
                completed: 2,
                deferred: 1,
                failed: 0,
                canceled: 0,
                drained: 1,
                queue_full: 0,
            }),
            final_layout: SourceLayoutSnapshot {
                active_rows: 0,
                frozen_table_count: 0,
                frozen_rows: 0,
                owned_l0_tables: 0,
                owned_nonzero_level_table_counts: vec![LevelTableCountSnapshot {
                    level: 7,
                    table_count: 2,
                }],
                owned_total_tables: 2,
                inherited_layers: 0,
                inherited_l0_tables: 0,
                inherited_nonzero_level_table_counts: Vec::new(),
                inherited_total_tables: 0,
            },
            source_shape_passed: true,
            failures: Vec::new(),
        };
        let drained_json = source_shape_context_json(&drained_context);

        assert_eq!(
            drained_json["compaction_mode"].as_str(),
            Some("explicit-fixed-point-drain")
        );
        assert_eq!(drained_json["flush"]["status"].as_str(), Some("completed"));
        assert_eq!(
            drained_json["flush"]["rows_processed"].as_u64(),
            Some(1_000)
        );
        assert_eq!(drained_json["flush"]["maintenance_ns"].as_u64(), Some(12));
        assert_eq!(
            drained_json["compact"]["status"].as_str(),
            Some("completed")
        );
        assert_eq!(
            drained_json["compact"]["mode"].as_str(),
            Some("explicit-fixed-point-drain")
        );
        assert_eq!(drained_json["compact"]["state_changes"].as_u64(), Some(3));
        assert_eq!(drained_json["compact"]["maintenance_ns"].as_u64(), Some(34));
        assert_eq!(
            drained_json["maintenance_queue"]["pending_tasks"].as_u64(),
            Some(1)
        );
        assert_eq!(
            drained_json["maintenance_queue"]["active_task"].as_u64(),
            Some(42)
        );
        assert_eq!(
            drained_json["maintenance_queue"]["max_pending_tasks"].as_u64(),
            Some(4)
        );
    }

    #[test]
    fn point_probe_shape_uses_layout_bound() {
        perf_trace::reset();
        let shape = point_probe_shape_json(10, perf_trace::snapshot(), 0, 1, 0, 0, 0);

        assert_eq!(shape["passed"].as_bool(), Some(true));
        assert_eq!(shape["owned_l0_table_bound"].as_u64(), Some(0));
        assert_eq!(shape["owned_nonzero_level_bound"].as_u64(), Some(10));
    }

    #[test]
    fn point_probe_shape_allows_inherited_search_per_layer() {
        let shape = point_probe_shape_json_from_counts(
            10,
            PointProbeShapeCounters {
                inherited_nonzero_level_searches: 20,
                inherited_nonzero_table_probes: 20,
                ..PointProbeShapeCounters::default()
            },
            0,
            0,
            2,
            0,
            1,
        );

        assert_eq!(shape["passed"].as_bool(), Some(true));
        assert_eq!(shape["inherited_nonzero_level_bound"].as_u64(), Some(20));

        let failed = point_probe_shape_json_from_counts(
            10,
            PointProbeShapeCounters {
                inherited_nonzero_level_searches: 21,
                inherited_nonzero_table_probes: 21,
                ..PointProbeShapeCounters::default()
            },
            0,
            0,
            2,
            0,
            1,
        );

        assert_eq!(failed["passed"].as_bool(), Some(false));
        assert!(failed["failures"]
            .as_array()
            .expect("failures")
            .iter()
            .any(|failure| failure.as_str()
                == Some("inherited_nonzero_level_searches_exceed_layout_bound")));
    }

    #[test]
    fn point_probe_shape_allows_l0_probes_with_observed_l0_tables() {
        let shape = point_probe_shape_json_from_counts(
            10,
            PointProbeShapeCounters {
                owned_l0_table_probes: 20,
                inherited_l0_table_probes: 10,
                ..PointProbeShapeCounters::default()
            },
            2,
            0,
            1,
            1,
            0,
        );

        assert_eq!(shape["passed"].as_bool(), Some(true));
        assert_eq!(shape["owned_l0_table_bound"].as_u64(), Some(20));
        assert_eq!(shape["inherited_l0_table_bound"].as_u64(), Some(10));

        let failed = point_probe_shape_json_from_counts(
            10,
            PointProbeShapeCounters {
                owned_l0_table_probes: 21,
                ..PointProbeShapeCounters::default()
            },
            2,
            0,
            0,
            0,
            0,
        );

        assert_eq!(failed["passed"].as_bool(), Some(false));
        assert!(failed["failures"]
            .as_array()
            .expect("failures")
            .iter()
            .any(|failure| failure.as_str() == Some("owned_l0_table_probes_exceed_layout_bound")));
    }
}
