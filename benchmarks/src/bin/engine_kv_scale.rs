//! Engine KV scale benchmark.
//!
//! Drives KV load, point reads, and prefix scans through the public
//! `strata_engine` surface (`Database` + `KvService`) at a configurable
//! scale and storage memory budget. The memory budget is the reason this binary
//! exists separately from `engine-kv-scan-regression`: it exercises the
//! `with_memory_budget` open option so large working sets stay resident instead
//! of tripping the storage frozen-mutable budget.

// Link the benchmark lib for its #[global_allocator] (jemalloc): a bin that
// never references the lib does NOT link it, silently running on glibc
// malloc — whose per-thread arenas fragment unboundedly under multi-GB
// alloc/free churn (T4 RSS attribution, roadmap-v2).
extern crate strata_benchmarks;

use std::env;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use strata_engine::{
    BranchName, CacheOpenOptions, Database, DurableLocalOpenOptions, EngineResult, KvKey, KvValue,
    ProductSpace,
};

const KEY_SIZE: usize = 24;
const RNG_SEED: u64 = 3;
const ENGINE_COMMIT_MUTATIONS: usize = 4094;

const DEFAULT_SCALE: usize = 1_000_000;
const DEFAULT_VALUE_BYTES: usize = 64;
const DEFAULT_MEMORY_BUDGET_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const DEFAULT_READS: usize = 100_000;
const DEFAULT_SCANS: usize = 10_000;
const DEFAULT_SCAN_LIMIT: usize = 64;

fn main() {
    let config = match Config::parse(env::args().skip(1)) {
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
    let root = env::current_dir()
        .unwrap_or_else(|_| ".".into())
        .join(".benchmark")
        .join("engine-kv-scale");
    std::fs::create_dir_all(&root).expect("benchmark directory");

    println!(
        "engine-kv-scale scale={} value={}B budget={} reads={} scans={} scan_limit={}",
        config.scale,
        config.value_bytes,
        format_bytes(config.memory_budget_bytes),
        config.reads,
        config.scans,
        config.scan_limit,
    );

    let mut reports = Vec::new();
    for mode in config.mode.modes() {
        let tempdir = tempfile::tempdir_in(&root).expect("temporary benchmark database");
        let report = run_mode(&config, mode, tempdir.path())?;
        print_report(&report);
        reports.push(report);
    }

    write_results(&config, &reports);
    Ok(())
}

fn run_mode(config: &Config, mode: BenchMode, path: &Path) -> EngineResult<ModeReport> {
    let mut database = match mode {
        BenchMode::Cache => Database::open_cache(
            CacheOpenOptions::new().with_memory_budget(config.memory_budget_bytes),
        )?
        .into_database(),
        BenchMode::Durable => Database::open_local(
            path,
            DurableLocalOpenOptions::new().with_memory_budget(config.memory_budget_bytes),
        )?
        .into_database(),
    };

    let keys = load(&mut database, config, mode)?;
    let point = measure_point_reads(&mut database, &keys, config.reads)?;
    eprintln!(
        "completed mode={} phase=point ops/s={:.0}",
        mode.as_str(),
        point.ops_per_second
    );
    let scan = measure_scans(&mut database, &keys, config.scans, config.scan_limit)?;
    eprintln!(
        "completed mode={} phase=scan ops/s={:.0}",
        mode.as_str(),
        scan.ops_per_second
    );

    Ok(ModeReport {
        mode: mode.as_str(),
        scale: config.scale,
        value_bytes: config.value_bytes,
        memory_budget_bytes: config.memory_budget_bytes,
        load: load_report(config.scale, keys_load_elapsed(&keys)),
        point,
        scan,
    })
}

/// Loaded keys plus the elapsed wall time of the load phase.
struct LoadedKeys {
    keys: Vec<KvKey>,
    elapsed: Duration,
}

fn keys_load_elapsed(loaded: &LoadedKeys) -> Duration {
    loaded.elapsed
}

fn load(database: &mut Database, config: &Config, mode: BenchMode) -> EngineResult<LoadedKeys> {
    let sample_target = config.reads.max(config.scans).max(1);
    let stride = (config.scale / sample_target).max(1);

    let mut rng = fastrand::Rng::with_seed(RNG_SEED);
    let mut batch = Vec::with_capacity(ENGINE_COMMIT_MUTATIONS);
    let mut keys = Vec::with_capacity(sample_target.min(config.scale));

    let start = Instant::now();
    for index in 0..config.scale {
        let (key_bytes, value) = random_pair(&mut rng, config.value_bytes);
        let key = KvKey::new(key_bytes.to_vec())?;
        if index % stride == 0 && keys.len() < sample_target {
            keys.push(key.clone());
        }
        batch.push((key, KvValue::new(value)));
        if batch.len() >= ENGINE_COMMIT_MUTATIONS || index + 1 == config.scale {
            commit_batch(database, &mut batch)?;
        }
    }
    let elapsed = start.elapsed();
    eprintln!(
        "completed mode={} phase=load rows={} elapsed_ms={} ({:.0} rows/s)",
        mode.as_str(),
        config.scale,
        elapsed.as_millis(),
        rate(config.scale, elapsed),
    );
    Ok(LoadedKeys { keys, elapsed })
}

fn commit_batch(database: &mut Database, batch: &mut Vec<(KvKey, KvValue)>) -> EngineResult<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let entries = std::mem::take(batch);
    database
        .kv(default_branch(), default_space())?
        .put_batch(entries)?;
    Ok(())
}

fn measure_point_reads(
    database: &mut Database,
    loaded: &LoadedKeys,
    operations: usize,
) -> EngineResult<OperationReport> {
    let mut service = database.kv(default_branch(), default_space())?;
    let mut latencies = Vec::with_capacity(operations);
    let mut checksum = 0u64;
    let mut hits = 0usize;
    let start = Instant::now();
    for key in loaded.keys.iter().cycle().take(operations) {
        let op_start = Instant::now();
        let found = service.get(key)?;
        latencies.push(op_start.elapsed().as_nanos() as u64);
        if let Some(value) = found {
            hits += 1;
            checksum = checksum.wrapping_add(first_byte(value.as_bytes()));
        }
    }
    Ok(OperationReport::new(
        operations,
        hits,
        checksum,
        start.elapsed(),
        latencies,
    ))
}

fn measure_scans(
    database: &mut Database,
    loaded: &LoadedKeys,
    operations: usize,
    limit: usize,
) -> EngineResult<OperationReport> {
    let mut service = database.kv(default_branch(), default_space())?;
    let mut latencies = Vec::with_capacity(operations);
    let mut checksum = 0u64;
    let mut returned_rows = 0usize;
    let start = Instant::now();
    for key in loaded.keys.iter().cycle().take(operations) {
        let op_start = Instant::now();
        let rows = service.scan(Some(key), Some(limit))?;
        latencies.push(op_start.elapsed().as_nanos() as u64);
        returned_rows += rows.len();
        for row in rows {
            checksum = checksum.wrapping_add(first_byte(row.value().as_bytes()));
        }
    }
    Ok(OperationReport::new(
        operations,
        returned_rows,
        checksum,
        start.elapsed(),
        latencies,
    ))
}

fn random_pair(rng: &mut fastrand::Rng, value_bytes: usize) -> ([u8; KEY_SIZE], Vec<u8>) {
    let mut key = [0u8; KEY_SIZE];
    rng.fill(&mut key);
    let mut value = vec![0u8; value_bytes];
    rng.fill(&mut value);
    (key, value)
}

fn first_byte(bytes: &[u8]) -> u64 {
    bytes.first().copied().unwrap_or_default() as u64
}

fn default_branch() -> BranchName {
    BranchName::new("default").expect("valid branch")
}

fn default_space() -> ProductSpace {
    ProductSpace::new("default").expect("valid product space")
}

fn print_report(report: &ModeReport) {
    println!("== mode={} ==", report.mode);
    println!(
        "  load   {:>12.0} rows/s  elapsed={}ms",
        report.load.rows_per_second, report.load.elapsed_ms
    );
    print_phase("point", &report.point);
    print_phase("scan ", &report.scan);
    println!("{}", serde_json::to_string(report).expect("JSON report"));
}

fn print_phase(label: &str, report: &OperationReport) {
    println!(
        "  {label}  {:>12.0} ops/s  p50={} p95={} p99={} returned_rows={}",
        report.ops_per_second,
        format_nanos(report.p50_ns),
        format_nanos(report.p95_ns),
        format_nanos(report.p99_ns),
        report.returned_rows,
    );
}

fn write_results(config: &Config, reports: &[ModeReport]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("results")
        .join("engine-kv-scale");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("warning: could not create results dir: {error}");
        return;
    }
    let stamp = unix_seconds();
    let path = dir.join(format!("engine-kv-scale-{stamp}.json"));
    let payload = ResultsFile {
        benchmark: "engine-kv-scale",
        scale: config.scale,
        value_bytes: config.value_bytes,
        memory_budget_bytes: config.memory_budget_bytes,
        reads: config.reads,
        scans: config.scans,
        scan_limit: config.scan_limit,
        results: reports,
    };
    match serde_json::to_string_pretty(&payload) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                eprintln!("warning: could not write results file: {error}");
            } else {
                println!("results: {}", path.display());
            }
        }
        Err(error) => eprintln!("warning: could not serialize results: {error}"),
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

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
    mode: ModeSelection,
    scale: usize,
    value_bytes: usize,
    memory_budget_bytes: u64,
    reads: usize,
    scans: usize,
    scan_limit: usize,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut mode = ModeSelection::Both;
        let mut scale = DEFAULT_SCALE;
        let mut value_bytes = DEFAULT_VALUE_BYTES;
        let mut memory_budget_bytes = DEFAULT_MEMORY_BUDGET_BYTES;
        let mut reads = DEFAULT_READS;
        let mut scans = DEFAULT_SCANS;
        let mut scan_limit = DEFAULT_SCAN_LIMIT;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mode" => mode = parse_mode(&arg_value(&arg, args.next())?)?,
                "--cache" => mode = ModeSelection::One(BenchMode::Cache),
                "--durable" => mode = ModeSelection::One(BenchMode::Durable),
                "--scale" => scale = parse_scale(&arg, args.next())?,
                "--value-bytes" => value_bytes = parse_positive_usize(&arg, args.next())?,
                "--memory-budget" => memory_budget_bytes = parse_size(&arg, args.next())?,
                "--reads" => reads = parse_positive_usize(&arg, args.next())?,
                "--scans" => scans = parse_positive_usize(&arg, args.next())?,
                "--scan-limit" => scan_limit = parse_positive_usize(&arg, args.next())?,
                "--help" | "-h" => return Err(usage()),
                _ => return Err(format!("unknown argument `{arg}`\n{}", usage())),
            }
        }
        Ok(Self {
            mode,
            scale,
            value_bytes,
            memory_budget_bytes,
            reads,
            scans,
            scan_limit,
        })
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

/// Parses a row count with optional decimal `k`/`m` suffix (e.g. `100k`, `1m`).
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
        .map_err(|_| format!("{flag}: invalid scale `{raw}`"))?;
    let scale = parsed * mult;
    if scale == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(scale)
}

/// Parses a byte size with optional binary `k`/`m`/`g` (`b`) suffix (e.g. `32g`, `512m`).
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
    "usage: engine-kv-scale [--mode cache|durable|both] [--scale N] [--value-bytes N] \
     [--memory-budget SIZE] [--reads N] [--scans N] [--scan-limit N]\n  \
     SIZE accepts k/m/g suffixes (binary); scale accepts k/m suffixes (decimal)."
        .to_owned()
}

#[derive(Serialize)]
struct ResultsFile<'a> {
    benchmark: &'static str,
    scale: usize,
    value_bytes: usize,
    memory_budget_bytes: u64,
    reads: usize,
    scans: usize,
    scan_limit: usize,
    results: &'a [ModeReport],
}

#[derive(Serialize)]
struct ModeReport {
    mode: &'static str,
    scale: usize,
    value_bytes: usize,
    memory_budget_bytes: u64,
    load: LoadReport,
    point: OperationReport,
    scan: OperationReport,
}

#[derive(Serialize)]
struct LoadReport {
    rows: usize,
    elapsed_ms: u128,
    rows_per_second: f64,
}

fn load_report(rows: usize, elapsed: Duration) -> LoadReport {
    LoadReport {
        rows,
        elapsed_ms: elapsed.as_millis(),
        rows_per_second: rate(rows, elapsed),
    }
}

#[derive(Serialize)]
struct OperationReport {
    operations: usize,
    returned_rows: usize,
    checksum: u64,
    elapsed_ms: u128,
    ops_per_second: f64,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
}

impl OperationReport {
    fn new(
        operations: usize,
        returned_rows: usize,
        checksum: u64,
        elapsed: Duration,
        mut latencies: Vec<u64>,
    ) -> Self {
        latencies.sort_unstable();
        Self {
            operations,
            returned_rows,
            checksum,
            elapsed_ms: elapsed.as_millis(),
            ops_per_second: rate(operations, elapsed),
            p50_ns: percentile(&latencies, 50.0),
            p95_ns: percentile(&latencies, 95.0),
            p99_ns: percentile(&latencies, 99.0),
        }
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let index = rank.round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

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
    const KIB: u64 = 1 << 10;
    if bytes >= GIB {
        format!("{:.0}GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.0}MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0}KiB", bytes as f64 / KIB as f64)
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
