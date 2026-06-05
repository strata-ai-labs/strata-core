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
    CommitOptions, MaintenanceRequest, MaintenanceScope, MaintenanceSummaryStatus, MaintenanceTask,
    PointReadRequest, PrefixScanReadRequest, ReadBound, ReadLimit, ScanRange, ScanReadOutcome,
    ScanReadRequest, StorageApiError, StorageApiResult, StorageDurabilityPolicy, StorageKey,
    StorageOpenOutcome, StorageRuntime, StorageSpaceId, StorageValue,
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
            if config.workloads.contains(&Workload::LoadSeq) || config.needs_loaded_data() {
                let result = run_load_seq(&mut open.runtime, branch_id, scale, engine, &config)?;
                loaded = true;
                print_result(&result);
                if config.workloads.contains(&Workload::LoadSeq) {
                    results.push(result.into_benchmark_result(&config));
                }
            }

            if config.workloads.contains(&Workload::PointLatest) {
                ensure_loaded(loaded, Workload::PointLatest);
                let result = run_point_latest(&open.runtime, branch_id, scale, engine, &config)?;
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::PointLatestThroughput) {
                ensure_loaded(loaded, Workload::PointLatestThroughput);
                let result =
                    run_point_latest_throughput(&open.runtime, branch_id, scale, engine, &config)?;
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::ScanPrefix) {
                ensure_loaded(loaded, Workload::ScanPrefix);
                let result = run_scan_prefix(&open.runtime, branch_id, scale, engine, &config)?;
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::ScanRangeThroughput) {
                ensure_loaded(loaded, Workload::ScanRangeThroughput);
                let result =
                    run_scan_range_throughput(&open.runtime, branch_id, scale, engine, &config)?;
                print_result(&result);
                results.push(result.into_benchmark_result(&config));
            }

            if config.workloads.contains(&Workload::BranchForkCurrent) {
                ensure_loaded(loaded, Workload::BranchForkCurrent);
                let result =
                    run_branch_fork_current(&mut open.runtime, branch_id, scale, engine, &config)?;
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
        if config.progress && written % progress_step(scale) == 0 {
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
            "    perf-trace api_map_ns={} api_runtime_ns={} api_scan_runtime_ns={} api_scan_map_ns={} validate_ns={} duplicate_key_checks={} prepare_ns={} append_validate_ns={} append_insert_ns={} absent_key_checks={} mutable_insert_checks={} commit_batches={} user_rows={} timeline_rows={} prepared_rows={} append_rows={} branch_fact_rows={} read_views={} read_view_rows={} read_view_validation_rows={} append_clones={} append_clone_rows={} conflict_sources={} point_rows_visited={} point_candidates={} scan_rows_visited={} scan_candidates={} scan_cursor_seeks={} scan_cursor_rows={} branch_scan_source_setup_ns={} branch_scan_merge_ns={} branch_scan_min_key_ns={} branch_scan_group_key_ns={} branch_scan_candidate_ns={} branch_scan_advance_ns={} branch_scan_select_ns={} scan_logical_key_encodes={} scan_candidate_row_clones={} scan_candidate_row_clone_bytes={} table_seeks={}",
            perf_trace.api_commit_map_ns(),
            perf_trace.api_commit_runtime_ns(),
            perf_trace.api_scan_runtime_ns(),
            perf_trace.api_scan_map_ns(),
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
            perf_trace.scan_rows_visited(),
            perf_trace.scan_candidates_materialized(),
            perf_trace.scan_cursor_seeks(),
            perf_trace.scan_cursor_rows_yielded(),
            perf_trace.branch_scan_source_setup_ns(),
            perf_trace.branch_scan_merge_ns(),
            perf_trace.branch_scan_min_key_ns(),
            perf_trace.branch_scan_group_key_ns(),
            perf_trace.branch_scan_candidate_ns(),
            perf_trace.branch_scan_advance_ns(),
            perf_trace.branch_scan_select_ns(),
            perf_trace.scan_logical_key_encodes(),
            perf_trace.scan_candidate_row_clones(),
            perf_trace.scan_candidate_row_clone_bytes(),
            perf_trace.table_seeks(),
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

    fn into_benchmark_result(self, config: &Config) -> BenchmarkResult {
        let mut parameters = HashMap::new();
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
        if let Some(load_phase) = self.load_phase_trace {
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
            parameters.insert(
                "perf_trace".to_string(),
                serde_json::json!({
                    "api_commit_map_ns": perf_trace.api_commit_map_ns(),
                    "api_commit_runtime_ns": perf_trace.api_commit_runtime_ns(),
                    "api_scan_runtime_ns": perf_trace.api_scan_runtime_ns(),
                    "api_scan_map_ns": perf_trace.api_scan_map_ns(),
                    "runtime_batch_validate_ns": perf_trace.runtime_batch_validate_ns(),
                    "runtime_duplicate_mutation_key_checks": perf_trace.runtime_duplicate_mutation_key_checks(),
                    "commit_prepare_rows_ns": perf_trace.commit_prepare_rows_ns(),
                    "append_batch_validate_ns": perf_trace.append_batch_validate_ns(),
                    "append_insert_rows_ns": perf_trace.append_insert_rows_ns(),
                    "append_absent_internal_key_checks": perf_trace.append_absent_internal_key_checks(),
                    "mutable_insert_duplicate_checks": perf_trace.mutable_insert_duplicate_checks(),
                    "commit_batches_prepared": perf_trace.commit_batches_prepared(),
                    "commit_user_mutation_rows": perf_trace.commit_user_mutation_rows(),
                    "commit_timeline_rows_prepared": perf_trace.commit_timeline_rows_prepared(),
                    "commit_rows_prepared": perf_trace.commit_rows_prepared(),
                    "append_rows_applied": perf_trace.append_rows_applied(),
                    "branch_facts_rows_observed": perf_trace.branch_facts_rows_observed(),
                    "read_view_captures": perf_trace.read_view_captures(),
                    "read_view_rows_cloned": perf_trace.read_view_rows_cloned(),
                    "read_view_validation_rows_scanned": perf_trace.read_view_validation_rows_scanned(),
                    "append_staging_clones": perf_trace.append_staging_clones(),
                    "append_staging_rows_cloned": perf_trace.append_staging_rows_cloned(),
                    "conflict_sources_built": perf_trace.conflict_sources_built(),
                    "point_rows_visited": perf_trace.point_rows_visited(),
                    "point_candidates_materialized": perf_trace.point_candidates_materialized(),
                    "scan_rows_visited": perf_trace.scan_rows_visited(),
                    "scan_candidates_materialized": perf_trace.scan_candidates_materialized(),
                    "scan_cursor_seeks": perf_trace.scan_cursor_seeks(),
                    "scan_cursor_rows_yielded": perf_trace.scan_cursor_rows_yielded(),
                    "branch_scan_source_setup_ns": perf_trace.branch_scan_source_setup_ns(),
                    "branch_scan_merge_ns": perf_trace.branch_scan_merge_ns(),
                    "branch_scan_min_key_ns": perf_trace.branch_scan_min_key_ns(),
                    "branch_scan_group_key_ns": perf_trace.branch_scan_group_key_ns(),
                    "branch_scan_candidate_ns": perf_trace.branch_scan_candidate_ns(),
                    "branch_scan_advance_ns": perf_trace.branch_scan_advance_ns(),
                    "branch_scan_select_ns": perf_trace.branch_scan_select_ns(),
                    "scan_logical_key_encodes": perf_trace.scan_logical_key_encodes(),
                    "scan_candidate_row_clones": perf_trace.scan_candidate_row_clones(),
                    "scan_candidate_row_clone_bytes": perf_trace.scan_candidate_row_clone_bytes(),
                    "table_seeks": perf_trace.table_seeks(),
                }),
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
            state: seed ^ 0x5DEE_CE66_D,
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
            | Self::MissingInitialBranch
            | Self::MissingRow => None,
        }
    }
}
