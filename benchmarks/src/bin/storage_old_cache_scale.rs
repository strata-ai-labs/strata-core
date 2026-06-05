//! Old engine cache scale benchmark runner.
//!
//! This binary mirrors `storage-next-l9-scale` for cache-mode comparison, but
//! uses the legacy public `stratadb::Strata` API.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use strata_benchmarks::harness::{read_cpu_model, read_total_ram_gb};
use strata_benchmarks::schema::{
    BenchmarkMetrics, BenchmarkReport, BenchmarkResult, HardwareInfo, RunMetadata,
};
use strata_storage::perf_trace as old_perf_trace;
use strata_storage::perf_trace::StoragePerfSnapshot;
use stratadb::{BatchKvEntry, Strata, Value};

const CATEGORY: &str = "storage-old-cache-scale";
const DEFAULT_SCALE: usize = 100_000;
const DEFAULT_VALUE_BYTES: usize = 64;
const DEFAULT_BATCH_SIZE: usize = 1_000;
const DEFAULT_SAMPLES: usize = 10_000;
const DEFAULT_BRANCH_SAMPLES: usize = 100;
const DEFAULT_SCAN_LIMIT: usize = 64;
const DEFAULT_BUCKETS: usize = 4_096;
const DEFAULT_SEED: u64 = 0x5154_5241_5441_2026;

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
        std::process::exit(1);
    }
}

fn run(config: Config) -> Result<(), BenchmarkError> {
    let mut results = Vec::new();

    eprintln!("old engine cache scale benchmark");
    eprintln!("scales: {}", format_list(&config.scales));
    eprintln!("workloads: {}", format_list(&config.workloads));
    eprintln!(
        "value={}B batch={} samples={} branch_samples={} scan_limit={}",
        config.value_bytes,
        config.batch_size,
        config.samples,
        config.branch_samples,
        config.scan_limit
    );
    eprintln!();

    for &scale in &config.scales {
        eprintln!("== scale={} engine=old-cache ==", format_scale(scale));
        let db = Strata::cache()?;

        let mut loaded = false;
        if config.workloads.contains(&Workload::LoadSeq) || config.needs_loaded_data() {
            let result = run_load_seq(&db, scale, &config)?;
            loaded = true;
            print_result(&result);
            if config.workloads.contains(&Workload::LoadSeq) {
                results.push(result.into_benchmark_result(&config));
            }
        }

        if config.workloads.contains(&Workload::PointLatest) {
            ensure_loaded(loaded, Workload::PointLatest);
            let result = run_point_latest(&db, scale, &config)?;
            print_result(&result);
            results.push(result.into_benchmark_result(&config));
        }

        if config.workloads.contains(&Workload::PointLatestThroughput) {
            ensure_loaded(loaded, Workload::PointLatestThroughput);
            let result = run_point_latest_throughput(&db, scale, &config)?;
            print_result(&result);
            results.push(result.into_benchmark_result(&config));
        }

        if config.workloads.contains(&Workload::ScanPrefix) {
            ensure_loaded(loaded, Workload::ScanPrefix);
            let result = run_scan_prefix(&db, scale, &config)?;
            print_result(&result);
            results.push(result.into_benchmark_result(&config));
        }

        if config.workloads.contains(&Workload::ScanRangeThroughput) {
            ensure_loaded(loaded, Workload::ScanRangeThroughput);
            let result = run_scan_range_throughput(&db, scale, &config)?;
            print_result(&result);
            results.push(result.into_benchmark_result(&config));
        }

        if config.workloads.contains(&Workload::BranchForkCurrent) {
            ensure_loaded(loaded, Workload::BranchForkCurrent);
            let result = run_branch_fork_current(&db, scale, &config)?;
            print_result(&result);
            results.push(result.into_benchmark_result(&config));
        }

        eprintln!();
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

fn run_load_seq(db: &Strata, scale: usize, config: &Config) -> Result<RunResult, BenchmarkError> {
    let value = Value::Bytes(vec![0x42; config.value_bytes]);
    let bucket_count = bucket_count(scale);
    let start = Instant::now();
    let mut load_phase = LoadPhaseTrace::default();
    let mut written = 0usize;

    while written < scale {
        let end = written.saturating_add(config.batch_size).min(scale);
        let build_start = Instant::now();
        let entries = (written..end)
            .map(|index| BatchKvEntry {
                key: key_for_index(index, bucket_count),
                value: value.clone(),
            })
            .collect::<Vec<_>>();
        load_phase.record_batch_build(build_start.elapsed());
        let commit_start = Instant::now();
        let summary = db.kv_batch_put(entries)?;
        load_phase.record_commit_call(commit_start.elapsed());
        black_box(summary.len());
        written = end;
        if config.progress && written % progress_step(scale) == 0 {
            eprintln!("  load progress: {}/{}", written, scale);
        }
    }

    Ok(
        RunResult::throughput(Workload::LoadSeq, scale, scale, start.elapsed())
            .with_load_phase_trace(load_phase),
    )
}

fn run_point_latest(
    db: &Strata,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x11);
    let requests = (0..config.samples)
        .map(|_| key_for_index(rng.next_usize(scale), bucket_count))
        .collect::<Vec<_>>();

    let timed = measure_requests(requests.iter(), |key| {
        let value = db.kv_get(key)?;
        if value.is_none() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(value);
        Ok(())
    })?;

    Ok(RunResult::latency(Workload::PointLatest, scale, timed))
}

fn run_point_latest_throughput(
    db: &Strata,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x33);
    let start = Instant::now();

    for _ in 0..config.samples {
        let key = key_for_index(rng.next_usize(scale), bucket_count);
        let value = db.kv_get(&key)?;
        if value.is_none() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(value);
    }

    Ok(RunResult::throughput(
        Workload::PointLatestThroughput,
        scale,
        config.samples,
        start.elapsed(),
    ))
}

fn run_scan_prefix(
    db: &Strata,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    old_perf_trace::reset();
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x22);
    let requests = (0..config.samples)
        .map(|_| prefix_for_bucket(rng.next_usize(bucket_count)))
        .collect::<Vec<_>>();

    let timed = measure_requests(requests.iter(), |prefix| {
        let rows = db.kv_scan(Some(prefix), Some(config.scan_limit as u64))?;
        if rows.is_empty() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(rows.len());
        Ok(())
    })?;

    Ok(RunResult::latency(Workload::ScanPrefix, scale, timed)
        .with_perf_trace(old_perf_trace::snapshot()))
}

fn run_scan_range_throughput(
    db: &Strata,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    old_perf_trace::reset();
    let bucket_count = bucket_count(scale);
    let mut rng = FastRng::new(config.seed ^ 0x44);
    let start = Instant::now();

    for _ in 0..config.samples {
        let key = key_for_index(rng.next_usize(scale), bucket_count);
        let rows = db.kv_scan(Some(&key), Some(config.scan_limit as u64))?;
        if rows.is_empty() {
            return Err(BenchmarkError::MissingRow);
        }
        black_box(rows.len());
    }

    Ok(RunResult::throughput(
        Workload::ScanRangeThroughput,
        scale,
        config.samples,
        start.elapsed(),
    )
    .with_perf_trace(old_perf_trace::snapshot()))
}

fn run_branch_fork_current(
    db: &Strata,
    scale: usize,
    config: &Config,
) -> Result<RunResult, BenchmarkError> {
    let requests = (0..config.branch_samples)
        .map(|index| format!("bench-fork-current-{index:08}"))
        .collect::<Vec<_>>();

    let timed = measure_requests(requests.iter(), |branch| {
        let outcome = db.branches().fork("default", branch)?;
        black_box(outcome.fork_version);
        Ok(())
    })?;

    Ok(RunResult::latency(
        Workload::BranchForkCurrent,
        scale,
        timed,
    ))
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
    Ok(TimedSamples::new(latencies, wall.elapsed()))
}

fn bucket_count(scale: usize) -> usize {
    scale.clamp(1, DEFAULT_BUCKETS)
}

fn key_for_index(index: usize, bucket_count: usize) -> String {
    let bucket = index % bucket_count;
    let ordinal = index / bucket_count;
    format!("{}{:016x}", prefix_for_bucket(bucket), ordinal)
}

fn prefix_for_bucket(bucket: usize) -> String {
    format!("k{bucket:04x}")
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
            .join("storage-old-cache")
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
    if let Some(load_phase) = result.load_phase_trace {
        eprintln!(
            "    load-phase batch_build_ns={} commit_call_ns={}",
            load_phase.batch_build_ns, load_phase.commit_call_ns
        );
    }
    if let Some(perf_trace) = result.perf_trace {
        eprintln!(
            "    perf-trace old_scan_calls={} old_scan_rows={} iterator_seeks={} pipeline_builds={} iterator_rows_yielded={} kv_scan_iter_create_ns={} kv_scan_seek_ns={} kv_scan_next_ns={} kv_scan_map_ns={}",
            perf_trace.kv_scan_calls(),
            perf_trace.kv_scan_rows_returned(),
            perf_trace.storage_iterator_seeks(),
            perf_trace.storage_iterator_pipeline_builds(),
            perf_trace.storage_iterator_rows_yielded(),
            perf_trace.kv_scan_iter_create_ns(),
            perf_trace.kv_scan_seek_ns(),
            perf_trace.kv_scan_next_ns(),
            perf_trace.kv_scan_map_ns(),
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
old engine cache scale benchmark

Usage:
  cargo run --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- [options]

Options:
  --scales LIST          Comma list: 100k,1m,10m,100m. Default: 100k
  --workloads LIST       Comma list: load-seq,point-latest,point-throughput,scan-prefix,scan-range-throughput,branch-fork-current. Default: all
  --value-bytes N        Value size in bytes. Default: 64
  --batch-size N         Entries per kv_batch_put during load. Default: 1000
  --samples N            Read/scan samples. Default: 10000
  --branch-samples N     Branch fork samples. Default: 100
  --scan-limit N         Scan limit. Default: 64
  --seed N               Deterministic sampling seed.
  --results-dir PATH     JSON output directory. Default: benchmarks/results/storage-old-cache
  --progress             Print load progress.
  -h, --help             Show this help.
"
    );
}

#[derive(Clone, Debug)]
struct Config {
    scales: Vec<usize>,
    workloads: Vec<Workload>,
    value_bytes: usize,
    batch_size: usize,
    samples: usize,
    branch_samples: usize,
    scan_limit: usize,
    seed: u64,
    results_dir: Option<PathBuf>,
    progress: bool,
}

impl Config {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, CliError> {
        let mut config = Self {
            scales: vec![DEFAULT_SCALE],
            workloads: Workload::ALL.to_vec(),
            value_bytes: DEFAULT_VALUE_BYTES,
            batch_size: DEFAULT_BATCH_SIZE,
            samples: DEFAULT_SAMPLES,
            branch_samples: DEFAULT_BRANCH_SAMPLES,
            scan_limit: DEFAULT_SCAN_LIMIT,
            seed: DEFAULT_SEED,
            results_dir: None,
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
                "--results-dir" => {
                    index += 1;
                    config.results_dir =
                        Some(PathBuf::from(value(args.get(index), "--results-dir")?));
                }
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
        if self.workloads.is_empty() {
            return Err(CliError::EmptyList("--workloads"));
        }
        if self.value_bytes == 0 {
            return Err(CliError::InvalidNumber("--value-bytes"));
        }
        if self.batch_size == 0 {
            return Err(CliError::InvalidNumber("--batch-size"));
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

#[derive(Debug)]
struct RunResult {
    workload: Workload,
    scale: usize,
    measurement: Measurement,
    load_phase_trace: Option<LoadPhaseTrace>,
    perf_trace: Option<StoragePerfSnapshot>,
}

impl RunResult {
    const fn throughput(workload: Workload, scale: usize, ops: usize, elapsed: Duration) -> Self {
        Self {
            workload,
            scale,
            measurement: Measurement::Throughput { elapsed, ops },
            load_phase_trace: None,
            perf_trace: None,
        }
    }

    const fn latency(workload: Workload, scale: usize, samples: TimedSamples) -> Self {
        Self {
            workload,
            scale,
            measurement: Measurement::Latency(samples),
            load_phase_trace: None,
            perf_trace: None,
        }
    }

    const fn with_load_phase_trace(mut self, load_phase_trace: LoadPhaseTrace) -> Self {
        self.load_phase_trace = Some(load_phase_trace);
        self
    }

    const fn with_perf_trace(mut self, perf_trace: StoragePerfSnapshot) -> Self {
        self.perf_trace = Some(perf_trace);
        self
    }

    fn into_benchmark_result(self, config: &Config) -> BenchmarkResult {
        let mut parameters = HashMap::new();
        parameters.insert("engine".to_string(), serde_json::json!("old-cache"));
        parameters.insert("scale_keys".to_string(), serde_json::json!(self.scale));
        parameters.insert(
            "value_bytes".to_string(),
            serde_json::json!(config.value_bytes),
        );
        parameters.insert(
            "batch_size".to_string(),
            serde_json::json!(config.batch_size),
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
                }),
            );
        }
        if let Some(perf_trace) = self.perf_trace {
            parameters.insert(
                "perf_trace".to_string(),
                serde_json::json!({
                    "storage_iterator_seeks": perf_trace.storage_iterator_seeks(),
                    "storage_iterator_pipeline_builds": perf_trace.storage_iterator_pipeline_builds(),
                    "storage_iterator_rows_yielded": perf_trace.storage_iterator_rows_yielded(),
                    "kv_scan_calls": perf_trace.kv_scan_calls(),
                    "kv_scan_rows_returned": perf_trace.kv_scan_rows_returned(),
                    "kv_scan_iter_create_ns": perf_trace.kv_scan_iter_create_ns(),
                    "kv_scan_seek_ns": perf_trace.kv_scan_seek_ns(),
                    "kv_scan_next_ns": perf_trace.kv_scan_next_ns(),
                    "kv_scan_map_ns": perf_trace.kv_scan_map_ns(),
                }),
            );
        }

        BenchmarkResult {
            benchmark: format!("storage-old-cache/{}", self.workload),
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

#[derive(Debug)]
enum CliError {
    Help,
    MissingValue(&'static str),
    EmptyList(&'static str),
    UnknownFlag(String),
    InvalidScale(String),
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
            Self::InvalidWorkload(value) => write!(f, "invalid workload {value}"),
            Self::InvalidNumber(flag) => write!(f, "invalid numeric value for {flag}"),
        }
    }
}

#[derive(Debug)]
enum BenchmarkError {
    Api(stratadb::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingRow,
}

impl From<stratadb::Error> for BenchmarkError {
    fn from(error: stratadb::Error) -> Self {
        Self::Api(error)
    }
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

impl fmt::Display for BenchmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(error) => write!(f, "old storage API error: {error}"),
            Self::Io(error) => write!(f, "io error: {error}"),
            Self::Json(error) => write!(f, "json error: {error}"),
            Self::MissingRow => f.write_str("benchmark read expected a loaded row"),
        }
    }
}

impl std::error::Error for BenchmarkError {}
