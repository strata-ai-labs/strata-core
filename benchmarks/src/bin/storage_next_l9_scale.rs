//! Storage-next L9 scale benchmark runner.
//!
//! This binary exercises only the public `strata_storage_next::api` surface.
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
use strata_storage_next::api::{
    BranchAction, BranchGeneration, BranchId, BranchRequest, CommitBatch, CommitMutation,
    CommitOptions, DiagnosticsFactState, DiagnosticsRequest, DiagnosticsScope,
    DiagnosticsSourceLayoutReport, MaintenanceRequest, MaintenanceScope, MaintenanceSummary,
    MaintenanceSummaryStatus, MaintenanceTask, PointReadRequest, PrefixScanReadRequest, ReadBound,
    ReadLimit, ScanRange, ScanReadOutcome, ScanReadRequest, StorageApiError, StorageApiResult,
    StorageDurabilityPolicy, StorageKey, StorageOpenOutcome, StorageRuntime, StorageSpaceId,
    StorageValue,
};
use strata_storage_next::perf_trace::{self, StoragePerfSnapshot};
use tempfile::TempDir;

const CATEGORY: &str = "storage-next-l9-scale";
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

    eprintln!("storage-next L9 scale benchmark");
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
    eprintln!();

    for &scale in &config.scales {
        for &engine in &config.engines {
            eprintln!("== scale={} engine={} ==", format_scale(scale), engine);
            let mut open = OpenBenchRuntime::open(engine, scale, &config)?;
            let branch_id = discover_initial_branch(&mut open.runtime)?;

            let mut loaded = false;
            let mut load_phase_context = None;
            let mut source_shape_context = None;
            if config.workloads.contains(&Workload::LoadSeq) || config.needs_loaded_data() {
                let result = run_load_seq(&mut open.runtime, branch_id, scale, engine, &config)?;
                loaded = true;
                load_phase_context = result.load_phase_trace;
                print_result(&result);
                if config.workloads.contains(&Workload::LoadSeq) {
                    results.push(result.into_benchmark_result(&config));
                }
            }

            if loaded && config.needs_loaded_data() {
                source_shape_context = Some(prepare_loaded_source_shape(
                    &mut open.runtime,
                    branch_id,
                    scale,
                )?);
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
    Ok(
        RunResult::throughput(Workload::LoadSeq, engine, scale, scale, elapsed)
            .with_load_phase_trace(load_phase)
            .with_perf_trace(perf_trace::snapshot()),
    )
}

fn prepare_loaded_source_shape(
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
                strata_storage_next::api::BranchStatus::Active
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
            .join("storage-next-l9")
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
            "    commit-perf wal_build_ns={} wal_records={} wal_record_rows={} wal_append_ns={} wal_appends={} wal_append_bytes={} visible_publish_attempts={} visible_publish_successes={} visible_publish_failures={} gate_attempts={} gate_acquired={} gate_rejected_unresolved={} gate_rejected_active={} unresolved_records={} registry_lookups={} registry_descriptors_scanned={} branch_guard_attempts={} branch_guard_acquired={} branch_guard_rejected={} quiesce_attempts={} quiesce_acquired={} quiesce_rejected={} conflict_validation_calls={} conflict_validation_skipped={} conflict_validation_without_source={} conflict_validation_with_source={} read_facts_checked={} cas_facts_checked={} conflicts_detected={} timeline_view_rows={} timeline_timestamp_facts={} timeline_version_facts={} timeline_reconcile_calls={} timeline_reconcile_timestamp_facts={} timeline_reconcile_version_facts={} timeline_lookup_calls={} timeline_lookup_entries_scanned={}",
            perf_trace.commit_wal_record_build_ns(),
            perf_trace.commit_wal_records_built(),
            perf_trace.commit_wal_record_rows(),
            perf_trace.commit_wal_append_ns(),
            perf_trace.commit_wal_appends(),
            perf_trace.commit_wal_append_bytes(),
            perf_trace.commit_visible_publish_attempts(),
            perf_trace.commit_visible_publish_successes(),
            perf_trace.commit_visible_publish_failures(),
            perf_trace.commit_unresolved_gate_admission_attempts(),
            perf_trace.commit_unresolved_gate_admission_acquired(),
            perf_trace.commit_unresolved_gate_rejected_unresolved(),
            perf_trace.commit_unresolved_gate_rejected_active(),
            perf_trace.commit_unresolved_records(),
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
            perf_trace.commit_timeline_lookup_calls(),
            perf_trace.commit_timeline_lookup_entries_scanned(),
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
            "    table-compaction merge_cursor_opens={} merge_advances={} pre_validation_rows={} row_clones={} heap_key_clones={} source_order_key_clones={} boundary_key_allocations={} kept_rows={} dropped_rows={} peak_buffered_rows={} output_tables_built={}",
            perf_trace.table_compaction_merge_cursor_opens(),
            perf_trace.table_compaction_merge_advances(),
            perf_trace.table_compaction_pre_validation_rows_scanned(),
            perf_trace.table_compaction_row_clones(),
            perf_trace.table_compaction_heap_key_clones(),
            perf_trace.table_compaction_source_order_key_clones(),
            perf_trace.table_compaction_boundary_key_allocations(),
            perf_trace.table_compaction_kept_rows(),
            perf_trace.table_compaction_dropped_rows(),
            perf_trace.table_compaction_peak_buffered_rows(),
            perf_trace.table_compaction_output_tables_built(),
        );
    }
    if let Some(load_phase) = result.load_phase_trace {
        eprintln!(
            "    load-phase batch_build_ns={} commit_call_ns={} maintenance_call_ns={} maintenance_runs={} maintenance_rows={}",
            load_phase.batch_build_ns,
            load_phase.commit_call_ns,
            load_phase.maintenance_call_ns,
            load_phase.maintenance_runs,
            load_phase.maintenance_rows,
        );
    }
    if let Some(source_shape) = result.source_shape_context.as_ref() {
        eprintln!(
            "    post-load-source-shape passed={} compaction_mode={} final_l0={} owned_nonzero={} inherited_l0={} inherited_nonzero={} interpretation={}",
            source_shape.source_shape_passed,
            source_shape.compaction_mode.as_str(),
            source_shape.final_layout.owned_l0_tables,
            format_level_counts(&source_shape.final_layout.owned_nonzero_level_table_counts),
            source_shape.final_layout.inherited_l0_tables,
            format_level_counts(&source_shape.final_layout.inherited_nonzero_level_table_counts),
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
storage-next L9 scale benchmark

Usage:
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- [options]

Options:
  --scales LIST          Comma list: 100k,1m,10m,100m. Default: 100k
  --engines LIST         Comma list: cache,standard,always. Default: all
  --workloads LIST       Comma list: load-seq,point-latest,point-throughput,scan-prefix,scan-range-throughput,branch-fork-current. Default: all
  --value-bytes N        Value size in bytes. Default: 64
  --batch-size N         Mutations per L9 commit during load. Default: 1000
  --flush-every N        Run public Flush maintenance every N loaded rows. Default: off
  --samples N            Read/scan samples. Default: 10000
  --branch-samples N     Branch fork samples. Default: 100
  --scan-limit N         Prefix scan limit. Default: 64
  --seed N               Deterministic sampling seed.
  --root PATH            Benchmark scratch root. Default: benchmarks/.benchmark/storage-next-l9
  --results-dir PATH     JSON output directory. Default: benchmarks/results/storage-next-l9
  --keep-dir             Keep durable scratch directories after the run.
  --progress             Print load progress.
  -h, --help             Show this help.

Examples:
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m,10m,100m --engines standard,always --samples 50000
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
    keep_dir: bool,
    progress: bool,
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
                .join("storage-next-l9"),
            results_dir: None,
            keep_dir: false,
            progress: false,
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
}

impl Workload {
    const ALL: [Self; 6] = [
        Self::LoadSeq,
        Self::PointLatest,
        Self::PointLatestThroughput,
        Self::ScanPrefix,
        Self::ScanRangeThroughput,
        Self::BranchForkCurrent,
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
        })
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
                    let outcome = StorageRuntime::open_durable_local(path.clone(), policy)?;
                    Ok(Self::from_outcome(outcome, None, Some(path)))
                } else {
                    let tempdir = tempfile::tempdir_in(&config.root)?;
                    let outcome =
                        StorageRuntime::open_durable_local(tempdir.path().to_path_buf(), policy)?;
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
        }
    }

    const fn with_perf_trace(mut self, perf_trace: StoragePerfSnapshot) -> Self {
        self.perf_trace = Some(perf_trace);
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
        if let Some(load_phase) = load_phase_trace {
            parameters.insert(
                "load_phase_trace".to_string(),
                serde_json::json!({
                    "batch_build_ns": load_phase.batch_build_ns,
                    "commit_call_ns": load_phase.commit_call_ns,
                    "maintenance_call_ns": load_phase.maintenance_call_ns,
                    "maintenance_runs": load_phase.maintenance_runs,
                    "maintenance_rows": load_phase.maintenance_rows,
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

        BenchmarkResult {
            benchmark: format!("storage-next-l9/{}", self.workload),
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
    field!("commit_wal_append_ns", perf_trace.commit_wal_append_ns());
    field!("commit_wal_appends", perf_trace.commit_wal_appends());
    field!(
        "commit_wal_append_bytes",
        perf_trace.commit_wal_append_bytes()
    );
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
        "commit_unresolved_gate_rejected_active",
        perf_trace.commit_unresolved_gate_rejected_active()
    );
    field!(
        "commit_unresolved_records",
        perf_trace.commit_unresolved_records()
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
    field!("commit_quiesce_acquired", perf_trace.commit_quiesce_acquired());
    field!("commit_quiesce_rejected", perf_trace.commit_quiesce_rejected());
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
        "commit_timeline_lookup_calls",
        perf_trace.commit_timeline_lookup_calls()
    );
    field!(
        "commit_timeline_lookup_entries_scanned",
        perf_trace.commit_timeline_lookup_entries_scanned()
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
    perf_trace: StoragePerfSnapshot,
    _load_phase_trace: Option<LoadPhaseTrace>,
    source_shape_context: Option<&SourceShapeContext>,
) -> serde_json::Value {
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
    let point_shape = source_shape_context.map(|context| {
        point_probe_shape_json(
            operation_count,
            perf_trace,
            context.final_layout.owned_nonzero_level_count(),
            context.final_layout.inherited_layers as u64,
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

    serde_json::json!({
        "point_source_probes_per_read": ratio_json(point_source_probes, operation_count),
        "point_nonzero_table_probes_per_read": ratio_json(point_nonzero_table_probes, operation_count),
        "point_owned_l0_table_probes": perf_trace.point_owned_l0_table_probes(),
        "point_owned_nonzero_level_searches": perf_trace.point_owned_nonzero_level_searches(),
        "point_owned_nonzero_table_probes": perf_trace.point_owned_nonzero_table_probes(),
        "point_table_seeks": perf_trace.point_table_seeks(),
        "point_rows_visited": perf_trace.point_rows_visited(),
        "scan_source_cursors_per_call": ratio_json(scan_source_cursors, operation_count),
        "scan_table_cursors_opened_per_call": ratio_json(scan_table_cursors_opened, operation_count),
        "scan_rows_visited_per_row_returned": ratio_json(
            perf_trace.scan_rows_visited(),
            perf_trace.scan_rows_returned(),
        ),
        "l0_tables_per_million_rows_after_load": l0_tables_per_million_rows_after_load,
        "post_load_compaction_mode": post_load_compaction_mode,
        "post_load_source_shape": source_shape_context.map(source_shape_context_json),
        "point_probe_shape": point_shape,
        "throughput_interpretation": throughput_interpretation,
        "point_read_acceleration_gaps": {
            "table_data_block_reads": perf_trace.table_data_block_reads(),
            "table_data_block_decodes": perf_trace.table_data_block_decodes(),
            "table_rows_decoded": perf_trace.table_rows_decoded(),
            "table_filter_probes": perf_trace.table_filter_probes(),
            "table_filter_negative_probes": perf_trace.table_filter_negative_probes(),
            "table_filter_positive_probes": perf_trace.table_filter_positive_probes(),
            "table_filter_absent_probes": perf_trace.table_filter_absent_probes(),
            "table_cache_hits": perf_trace.table_cache_hits(),
            "table_cache_misses": perf_trace.table_cache_misses(),
        },
        "assumptions": {
            "operation_count": operation_count,
            "l0_tables_after_load": source_shape_context.map_or(
                "source-shape unavailable",
                |context| context.compaction_mode.layout_assumption(),
            ),
            "known_compaction_mode_values": source_shape_compaction_modes_json(),
        },
    })
}

fn point_probe_shape_json(
    operation_count: u64,
    perf_trace: StoragePerfSnapshot,
    owned_nonzero_level_count: u64,
    inherited_layers: u64,
    inherited_nonzero_level_count: u64,
) -> serde_json::Value {
    point_probe_shape_json_from_counts(
        operation_count,
        PointProbeShapeCounters::from_perf_trace(perf_trace),
        owned_nonzero_level_count,
        inherited_layers,
        inherited_nonzero_level_count,
    )
}

fn point_probe_shape_json_from_counts(
    operation_count: u64,
    counters: PointProbeShapeCounters,
    owned_nonzero_level_count: u64,
    inherited_layers: u64,
    inherited_nonzero_level_count: u64,
) -> serde_json::Value {
    if operation_count == 0 {
        return serde_json::Value::Null;
    }

    let max_owned_nonzero_level_searches =
        operation_count.saturating_mul(owned_nonzero_level_count);
    let inherited_level_bound =
        inherited_nonzero_level_count.saturating_mul(if inherited_nonzero_level_count == 0 {
            0
        } else {
            inherited_layers.max(1)
        });
    let max_inherited_nonzero_level_searches =
        operation_count.saturating_mul(inherited_level_bound);
    let mut failures = Vec::new();
    if counters.owned_l0_table_probes != 0 {
        failures.push("owned_l0_table_probes_nonzero");
    }
    if counters.inherited_l0_table_probes != 0 {
        failures.push("inherited_l0_table_probes_nonzero");
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
        "owned_nonzero_level_bound": max_owned_nonzero_level_searches,
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

#[derive(Clone, Debug)]
struct SourceShapeContext {
    scale: usize,
    compaction_mode: SourceShapeCompactionMode,
    flush_status: MaintenanceSummaryStatus,
    flush_rows: u64,
    flush_maintenance_ns: u64,
    compact_status: MaintenanceSummaryStatus,
    compact_state_changes: usize,
    compact_maintenance_ns: u64,
    final_layout: SourceLayoutSnapshot,
    source_shape_passed: bool,
    failures: Vec<String>,
}

impl SourceShapeContext {
    fn from_report(
        scale: usize,
        flush: MaintenanceSummary,
        flush_elapsed: Duration,
        compact: MaintenanceSummary,
        compact_elapsed: Duration,
        compaction_mode: SourceShapeCompactionMode,
        report: &DiagnosticsSourceLayoutReport,
    ) -> Self {
        let final_layout = SourceLayoutSnapshot::from_report(report);
        let failures = final_layout.compacted_shape_failures();
        Self {
            scale,
            compaction_mode,
            flush_status: flush.status(),
            flush_rows: flush.rows_processed(),
            flush_maintenance_ns: nanos_u64(flush_elapsed),
            compact_status: compact.status(),
            compact_state_changes: compact.state_changes(),
            compact_maintenance_ns: nanos_u64(compact_elapsed),
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
                "diagnostics source layout after final flush and automatic compaction scheduling"
            }
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
        "  source-shape         passed={} compaction_mode={} final_l0={} owned_nonzero={} inherited_l0={} inherited_nonzero={} flush_status={} compact_status={} compact_changes={}",
        context.source_shape_passed,
        context.compaction_mode.as_str(),
        context.final_layout.owned_l0_tables,
        format_level_counts(&context.final_layout.owned_nonzero_level_table_counts),
        context.final_layout.inherited_l0_tables,
        format_level_counts(&context.final_layout.inherited_nonzero_level_table_counts),
        format_maintenance_status(context.flush_status),
        format_maintenance_status(context.compact_status),
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
            "status": format_maintenance_status(context.flush_status),
            "rows_processed": context.flush_rows,
            "maintenance_ns": context.flush_maintenance_ns,
        },
        "compact": {
            "mode": context.compaction_mode.as_str(),
            "status": format_maintenance_status(context.compact_status),
            "state_changes": context.compact_state_changes,
            "maintenance_ns": context.compact_maintenance_ns,
        },
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
        assert!(metrics["l0_tables_per_million_rows_after_load"].is_null());
        assert!(metrics["post_load_compaction_mode"].is_null());
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
        let metrics = source_shape_metrics_json(0, 0, perf_trace::snapshot(), None, None);

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
    fn source_shape_metrics_use_final_layout_for_l0_density() {
        perf_trace::reset();
        let source_shape = SourceShapeContext {
            scale: 1_000,
            compaction_mode: SourceShapeCompactionMode::ExplicitFixedPointDrain,
            flush_status: MaintenanceSummaryStatus::Completed,
            flush_rows: 1_000,
            flush_maintenance_ns: 1,
            compact_status: MaintenanceSummaryStatus::Completed,
            compact_state_changes: 1,
            compact_maintenance_ns: 1,
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

        let metrics =
            source_shape_metrics_json(1_000, 1, perf_trace::snapshot(), None, Some(&source_shape));

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
    fn point_probe_shape_uses_layout_bound() {
        perf_trace::reset();
        let shape = point_probe_shape_json(10, perf_trace::snapshot(), 1, 0, 0);

        assert_eq!(shape["passed"].as_bool(), Some(true));
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
            2,
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
            2,
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
}
