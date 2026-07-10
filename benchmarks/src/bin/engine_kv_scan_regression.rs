use std::env;
// Link the benchmark lib for its #[global_allocator] (jemalloc): a bin that
// never references the lib does NOT link it, silently running on glibc
// malloc — whose per-thread arenas fragment unboundedly under multi-GB
// alloc/free churn (T4 RSS attribution, roadmap-v2).
extern crate strata_benchmarks;

use std::path::Path;
use std::process;
use std::time::{Duration, Instant};

use serde::Serialize;
use strata_engine::{
    BranchName, CacheOpenOptions, Database, DurableLocalOpenOptions, EngineResult, KvKey, KvValue,
    ProductSpace,
};

const KEY_SIZE: usize = 24;
const VALUE_SIZE: usize = 150;
const RNG_SEED: u64 = 3;
const DEFAULT_ROWS: usize = 100_000;
const DEFAULT_SCANS: usize = 10_000;
const DEFAULT_LIMIT: usize = 10;
const ENGINE_COMMIT_MUTATIONS: usize = 4094;

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
        .join(".benchmark");
    std::fs::create_dir_all(&root).expect("benchmark directory");

    for mode in config.mode.modes() {
        let tempdir = tempfile::tempdir_in(&root).expect("temporary benchmark database");
        let report = run_mode(&config, mode, tempdir.path())?;
        println!(
            "engine-kv-scan-regression mode={} rows={} scans={} limit={}",
            report.mode, report.rows, report.scans, report.limit
        );
        print_load("load", &report.load);
        print_phase("point reused-service", &report.point_reused_service);
        print_phase("point per-call-service", &report.point_per_call_service);
        print_phase("scan reused-service", &report.scan_reused_service);
        print_phase("scan per-call-service", &report.scan_per_call_service);
        println!("{}", serde_json::to_string(&report).expect("JSON report"));
    }

    Ok(())
}

fn run_mode(config: &Config, mode: BenchMode, path: &Path) -> EngineResult<Report> {
    let mut database = match mode {
        BenchMode::Cache => Database::open_cache(CacheOpenOptions::new())?.into_database(),
        BenchMode::Durable => {
            Database::open_local(path, DurableLocalOpenOptions::new())?.into_database()
        }
    };

    let mut rng = fastrand::Rng::with_seed(RNG_SEED);
    let mut batch = Vec::with_capacity(ENGINE_COMMIT_MUTATIONS);
    let mut keys = Vec::with_capacity(config.rows.min(config.scans.max(1)));
    let load_start = Instant::now();
    for index in 0..config.rows {
        let (key, value) = random_pair(&mut rng);
        let key = KvKey::new(key.to_vec())?;
        if keys.len() < config.scans.max(1) {
            keys.push(key.clone());
        }
        batch.push((key, KvValue::new(value)));
        if batch.len() >= ENGINE_COMMIT_MUTATIONS {
            commit_batch(&mut database, &mut batch)?;
        }
        if index + 1 == config.rows {
            commit_batch(&mut database, &mut batch)?;
        }
    }
    let load = LoadReport::new(config.rows, load_start.elapsed());
    eprintln!(
        "completed mode={} phase=load elapsed_ms={}",
        mode.as_str(),
        load.elapsed_ms
    );

    let point_reused_service = measure_point_reused_service(&mut database, &keys, config.scans)?;
    eprintln!(
        "completed mode={} phase=point_reused_service elapsed_ms={}",
        mode.as_str(),
        point_reused_service.elapsed_ms
    );
    let point_per_call_service =
        measure_point_per_call_service(&mut database, &keys, config.scans)?;
    eprintln!(
        "completed mode={} phase=point_per_call_service elapsed_ms={}",
        mode.as_str(),
        point_per_call_service.elapsed_ms
    );
    let scan_reused_service =
        measure_scan_reused_service(&mut database, &keys, config.scans, config.limit)?;
    eprintln!(
        "completed mode={} phase=scan_reused_service elapsed_ms={}",
        mode.as_str(),
        scan_reused_service.elapsed_ms
    );
    let scan_per_call_service =
        measure_scan_per_call_service(&mut database, &keys, config.scans, config.limit)?;
    eprintln!(
        "completed mode={} phase=scan_per_call_service elapsed_ms={}",
        mode.as_str(),
        scan_per_call_service.elapsed_ms
    );

    Ok(Report {
        benchmark: "engine-kv-scan-regression",
        mode: mode.as_str(),
        rows: config.rows,
        scans: config.scans,
        limit: config.limit,
        load,
        point_reused_service,
        point_per_call_service,
        scan_reused_service,
        scan_per_call_service,
    })
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

fn measure_point_reused_service(
    database: &mut Database,
    keys: &[KvKey],
    operations: usize,
) -> EngineResult<OperationReport> {
    let mut service = database.kv(default_branch(), default_space())?;
    let start = Instant::now();
    let mut checksum = 0u64;
    let mut returned_rows = 0usize;
    for key in keys.iter().cycle().take(operations) {
        if let Some(value) = service.get(key)? {
            returned_rows += 1;
            checksum = checksum.wrapping_add(first_byte(value.as_bytes()));
        }
    }
    Ok(OperationReport::new(
        operations,
        returned_rows,
        checksum,
        start.elapsed(),
    ))
}

fn measure_point_per_call_service(
    database: &mut Database,
    keys: &[KvKey],
    operations: usize,
) -> EngineResult<OperationReport> {
    let start = Instant::now();
    let mut checksum = 0u64;
    let mut returned_rows = 0usize;
    for key in keys.iter().cycle().take(operations) {
        if let Some(value) = database.kv(default_branch(), default_space())?.get(key)? {
            returned_rows += 1;
            checksum = checksum.wrapping_add(first_byte(value.as_bytes()));
        }
    }
    Ok(OperationReport::new(
        operations,
        returned_rows,
        checksum,
        start.elapsed(),
    ))
}

fn measure_scan_reused_service(
    database: &mut Database,
    keys: &[KvKey],
    operations: usize,
    limit: usize,
) -> EngineResult<OperationReport> {
    let mut service = database.kv(default_branch(), default_space())?;
    let start = Instant::now();
    let mut checksum = 0u64;
    let mut returned_rows = 0usize;
    for key in keys.iter().cycle().take(operations) {
        let rows = service.scan(Some(key), Some(limit))?;
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
    ))
}

fn measure_scan_per_call_service(
    database: &mut Database,
    keys: &[KvKey],
    operations: usize,
    limit: usize,
) -> EngineResult<OperationReport> {
    let start = Instant::now();
    let mut checksum = 0u64;
    let mut returned_rows = 0usize;
    for key in keys.iter().cycle().take(operations) {
        let rows = database
            .kv(default_branch(), default_space())?
            .scan(Some(key), Some(limit))?;
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
    ))
}

fn random_pair(rng: &mut fastrand::Rng) -> ([u8; KEY_SIZE], Vec<u8>) {
    let mut key = [0u8; KEY_SIZE];
    rng.fill(&mut key);
    let mut value = vec![0u8; VALUE_SIZE];
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

fn print_load(label: &str, report: &LoadReport) {
    println!(
        "  {label}: {} rows in {}ms ({:.2} rows/sec)",
        report.rows, report.elapsed_ms, report.rows_per_second
    );
}

fn print_phase(label: &str, report: &OperationReport) {
    println!(
        "  {label}: {} ops in {}ms ({:.2} ops/sec, {:.2} rows/sec, returned_rows={}, checksum={})",
        report.operations,
        report.elapsed_ms,
        report.operations_per_second,
        report.rows_per_second,
        report.returned_rows,
        report.checksum
    );
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
    rows: usize,
    scans: usize,
    limit: usize,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut mode = ModeSelection::Both;
        let mut rows = DEFAULT_ROWS;
        let mut scans = DEFAULT_SCANS;
        let mut limit = DEFAULT_LIMIT;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mode" => {
                    let value = args
                        .next()
                        .ok_or("--mode requires cache, durable, or both")?;
                    mode = parse_mode(&value)?;
                }
                "--cache" => mode = ModeSelection::One(BenchMode::Cache),
                "--durable" => mode = ModeSelection::One(BenchMode::Durable),
                "--rows" => rows = parse_positive_usize(&arg, args.next())?,
                "--scans" => scans = parse_positive_usize(&arg, args.next())?,
                "--limit" => limit = parse_positive_usize(&arg, args.next())?,
                "--help" | "-h" => return Err(usage()),
                _ => return Err(format!("unknown argument `{arg}`\n{}", usage())),
            }
        }
        Ok(Self {
            mode,
            rows,
            scans,
            limit,
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

fn parse_positive_usize(flag: &str, value: Option<String>) -> Result<usize, String> {
    let value = value.ok_or_else(|| format!("{flag} requires a positive integer"))?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{flag} requires a positive integer, got `{value}`"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

fn usage() -> String {
    "usage: engine-kv-scan-regression [--mode cache|durable|both] [--rows N] [--scans N] [--limit N]"
        .to_owned()
}

#[derive(Serialize)]
struct Report {
    benchmark: &'static str,
    mode: &'static str,
    rows: usize,
    scans: usize,
    limit: usize,
    load: LoadReport,
    point_reused_service: OperationReport,
    point_per_call_service: OperationReport,
    scan_reused_service: OperationReport,
    scan_per_call_service: OperationReport,
}

#[derive(Serialize)]
struct LoadReport {
    rows: usize,
    elapsed_ms: u128,
    rows_per_second: f64,
}

impl LoadReport {
    fn new(rows: usize, elapsed: Duration) -> Self {
        Self {
            rows,
            elapsed_ms: elapsed.as_millis(),
            rows_per_second: rate(rows, elapsed),
        }
    }
}

#[derive(Serialize)]
struct OperationReport {
    operations: usize,
    returned_rows: usize,
    checksum: u64,
    elapsed_ms: u128,
    operations_per_second: f64,
    rows_per_second: f64,
}

impl OperationReport {
    fn new(operations: usize, returned_rows: usize, checksum: u64, elapsed: Duration) -> Self {
        Self {
            operations,
            returned_rows,
            checksum,
            elapsed_ms: elapsed.as_millis(),
            operations_per_second: rate(operations, elapsed),
            rows_per_second: rate(returned_rows, elapsed),
        }
    }
}

fn rate(count: usize, elapsed: Duration) -> f64 {
    let seconds = elapsed.as_secs_f64();
    if seconds == 0.0 {
        return 0.0;
    }
    count as f64 / seconds
}
