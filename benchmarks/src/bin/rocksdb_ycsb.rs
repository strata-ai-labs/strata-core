//! YCSB benchmark for RocksDB — the gold-standard reference, mirroring
//! `engine_ycsb` so the two can be compared apples-to-apples.
//!
//! Identical to `engine_ycsb` in every workload-shaping dimension: the same
//! `ycsb_workloads` generators (A-F, Zipfian/Uniform/Latest), the same key
//! format and value size, the same batched load phase and single-record run
//! phase, and the same per-operation-type latency accounting. The only
//! difference is the engine under test.
//!
//! RocksDB runs with default options (WAL enabled, `WriteOptions::sync = false`),
//! which is comparable to Strata's durable "Standard" mode (WAL on, fsync
//! deferred). This is the standard reference configuration, not a tuned one.
//!
//! Usage:
//!   rocksdb-ycsb --workload a --records 100000 --ops 100000
//!   rocksdb-ycsb -q

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{Duration, Instant, SystemTime};

use rocksdb::{Direction, IteratorMode, Options, WriteBatch, DB};
use serde::Serialize;

// Shared YCSB workload definitions and distribution generators — the same module
// `engine_ycsb` uses, so the workloads are identical.
#[allow(dead_code)]
#[path = "../../benches/ycsb_workloads.rs"]
mod ycsb_workloads;

use ycsb_workloads::{workload_by_label, ycsb_key, FastRng, KeyChooser, Operation, WorkloadSpec};

const DEFAULT_RECORDS: usize = 100_000;
const DEFAULT_OPS: usize = 100_000;
const QUICK_RECORDS: usize = 10_000;
const QUICK_OPS: usize = 10_000;
const DEFAULT_VALUE_BYTES: usize = 1_000;
const DEFAULT_SCAN_MAX: usize = 100;
const DEFAULT_LOAD_BATCH: usize = 1_000;
const RUN_SEED: u64 = 0xABCD_2026;

fn main() {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            process::exit(2);
        }
    };
    if let Err(error) = run(config) {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run(config: Config) -> Result<(), String> {
    let root = std::env::current_dir()
        .unwrap_or_else(|_| ".".into())
        .join(".benchmark")
        .join("rocksdb-ycsb");
    std::fs::create_dir_all(&root).map_err(|e| format!("create benchmark dir: {e}"))?;

    println!(
        "rocksdb-ycsb records={} ops={} value={}B scan_max={} load_batch={} workloads={}",
        config.records,
        config.ops,
        config.value_bytes,
        config.scan_max,
        config.load_batch,
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
        let tempdir = tempfile::tempdir_in(&root).map_err(|e| format!("tempdir: {e}"))?;
        let result = run_workload(&config, workload, tempdir.path())?;
        print_result(&result);
        results.push(result);
    }

    if !results.is_empty() {
        write_results(&config, &results);
    }
    Ok(())
}

fn run_workload(config: &Config, workload: &WorkloadSpec, path: &Path) -> Result<WorkloadResult, String> {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let db = DB::open(&opts, path).map_err(|e| format!("open rocksdb: {e}"))?;

    let value = vec![0x42u8; config.value_bytes];
    let load_start = Instant::now();
    load(&db, config, &value)?;
    let load_elapsed = load_start.elapsed();

    let run = run_phase(&db, config, workload, &value)?;

    Ok(WorkloadResult {
        engine: "rocksdb",
        workload: workload.label,
        workload_name: workload.name,
        distribution: workload.distribution.label(),
        records: config.records,
        ops: config.ops,
        value_bytes: config.value_bytes,
        load_ops_per_sec: rate(config.records, load_elapsed),
        load_elapsed_ms: load_elapsed.as_millis(),
        run_ops_per_sec: run.ops_per_sec,
        run_elapsed_ms: run.elapsed_ms,
        operations: run.operations,
    })
}

fn load(db: &DB, config: &Config, value: &[u8]) -> Result<(), String> {
    let batch_size = config.load_batch.max(1);
    let mut batch = WriteBatch::default();
    let mut pending = 0usize;
    for index in 0..config.records {
        batch.put(ycsb_key(index).as_bytes(), value);
        pending += 1;
        if pending >= batch_size {
            db.write(std::mem::take(&mut batch))
                .map_err(|e| format!("load write: {e}"))?;
            pending = 0;
        }
    }
    if pending > 0 {
        db.write(batch).map_err(|e| format!("load write: {e}"))?;
    }
    Ok(())
}

struct RunPhase {
    ops_per_sec: f64,
    elapsed_ms: u128,
    operations: BTreeMap<&'static str, LatencySummary>,
}

fn run_phase(
    db: &DB,
    config: &Config,
    workload: &WorkloadSpec,
    value: &[u8],
) -> Result<RunPhase, String> {
    let update_value = vec![0x43u8; config.value_bytes];
    let mut rng = FastRng::new(RUN_SEED);
    let mut key_chooser = KeyChooser::new(workload.distribution, config.records.max(1));
    let mut insert_counter = config.records;
    let mut stats = OpStatsByType::new();
    let mut checksum = 0u64;

    let wall_start = Instant::now();
    for _ in 0..config.ops {
        let op = workload.choose_operation(rng.next_f64());
        let op_start = Instant::now();
        match op {
            Operation::Read => {
                let key = ycsb_key(key_chooser.next(&mut rng));
                if let Some(v) = db.get(key.as_bytes()).map_err(|e| format!("get: {e}"))? {
                    checksum = checksum.wrapping_add(first_byte(&v));
                }
            }
            Operation::Update => {
                let key = ycsb_key(key_chooser.next(&mut rng));
                db.put(key.as_bytes(), &update_value)
                    .map_err(|e| format!("put: {e}"))?;
            }
            Operation::Insert => {
                let key = ycsb_key(insert_counter);
                insert_counter += 1;
                key_chooser.set_max_key(insert_counter);
                db.put(key.as_bytes(), value).map_err(|e| format!("put: {e}"))?;
            }
            Operation::Scan => {
                let key = ycsb_key(key_chooser.next(&mut rng));
                let len = 1 + rng.next_usize(config.scan_max);
                let iter = db.iterator(IteratorMode::From(key.as_bytes(), Direction::Forward));
                for row in iter.take(len) {
                    let (_k, v) = row.map_err(|e| format!("scan: {e}"))?;
                    checksum = checksum.wrapping_add(first_byte(&v));
                }
            }
            Operation::ReadModifyWrite => {
                let key = ycsb_key(key_chooser.next(&mut rng));
                if let Some(v) = db.get(key.as_bytes()).map_err(|e| format!("get: {e}"))? {
                    checksum = checksum.wrapping_add(first_byte(&v));
                }
                db.put(key.as_bytes(), &update_value)
                    .map_err(|e| format!("put: {e}"))?;
            }
        }
        stats.record(op, op_start.elapsed().as_nanos() as u64);
    }
    let elapsed = wall_start.elapsed();
    // Keep the checksum observable so reads/scans are not optimized away.
    std::hint::black_box(checksum);

    Ok(RunPhase {
        ops_per_sec: rate(config.ops, elapsed),
        elapsed_ms: elapsed.as_millis(),
        operations: stats.into_summaries(),
    })
}

fn first_byte(bytes: &[u8]) -> u64 {
    bytes.first().copied().unwrap_or_default() as u64
}

// ---------------------------------------------------------------------------
// Per-operation-type latency accounting (identical shape to engine_ycsb)
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
        "  [rocksdb] load={:.0} ops/s ({}ms)  run={:.0} ops/s ({}ms)",
        result.load_ops_per_sec, result.load_elapsed_ms, result.run_ops_per_sec, result.run_elapsed_ms,
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

fn write_results(config: &Config, results: &[WorkloadResult]) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("results")
        .join("rocksdb-ycsb");
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("warning: could not create results dir: {error}");
        return;
    }
    let path = dir.join(format!("rocksdb-ycsb-{}.json", unix_seconds()));
    let payload = ResultsFile {
        benchmark: "rocksdb-ycsb",
        records: config.records,
        ops: config.ops,
        value_bytes: config.value_bytes,
        scan_max: config.scan_max,
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

struct Config {
    workloads: Vec<char>,
    records: usize,
    ops: usize,
    value_bytes: usize,
    scan_max: usize,
    load_batch: usize,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            workloads: vec!['a', 'b', 'c', 'd', 'e', 'f'],
            records: DEFAULT_RECORDS,
            ops: DEFAULT_OPS,
            value_bytes: DEFAULT_VALUE_BYTES,
            scan_max: DEFAULT_SCAN_MAX,
            load_batch: DEFAULT_LOAD_BATCH,
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
                "--records" => config.records = parse_scale(&arg, args.next())?,
                "--ops" => config.ops = parse_scale(&arg, args.next())?,
                "--value-bytes" => config.value_bytes = parse_positive_usize(&arg, args.next())?,
                "--scan-max" => config.scan_max = parse_positive_usize(&arg, args.next())?,
                "--load-batch" => config.load_batch = parse_positive_usize(&arg, args.next())?,
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

fn usage() -> String {
    "usage: rocksdb-ycsb [--workload a,b,c,d,e,f] [--records N] [--ops N] \
     [--value-bytes N] [--scan-max N] [--load-batch N] [-q]\n  \
     records/ops accept k/m suffixes (decimal). RocksDB runs with default options."
        .to_owned()
}

// ---------------------------------------------------------------------------
// Result types (mirror engine_ycsb; `engine` field distinguishes the source)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ResultsFile<'a> {
    benchmark: &'static str,
    records: usize,
    ops: usize,
    value_bytes: usize,
    scan_max: usize,
    results: &'a [WorkloadResult],
}

#[derive(Serialize)]
struct WorkloadResult {
    engine: &'static str,
    workload: char,
    workload_name: &'static str,
    distribution: &'static str,
    records: usize,
    ops: usize,
    value_bytes: usize,
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

fn format_nanos(ns: u64) -> String {
    if ns >= 1_000_000 {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.2}us", ns as f64 / 1_000.0)
    } else {
        format!("{ns}ns")
    }
}
