//! Durable `after_version` table-skip scan benchmark.
//!
//! Demonstrates the durable-mode table-level I/O skip. An index-covered query reads only the active
//! delta — rows committed after the manifest watermark — by setting an `after_version` lower bound on
//! the prefix scan. With the skip, every immutable table whose newest commit version is at or below
//! the watermark is dropped before its blocks are read, instead of being opened and discarded.
//!
//! The benchmark builds a durable LSM, seals it, leaves a small active delta, then runs the same
//! full-collection prefix scan twice:
//!   * `after_version = None`  — baseline: reads the whole collection (opens every sealed table).
//!   * `after_version = Some(W)` — skips the sealed tables and reads only the delta.
//!
//! It reports latency percentiles, rows returned, and the number of sealed tables opened per arm.
//! `tables_opened == 0` in the skip arm is the direct proof of the optimization. Storage-layer only:
//! it exercises the public `strata_storage_next::api` surface.

use std::error::Error;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use strata_storage_next::api::{
    BranchAction, BranchId, BranchRequest, BranchStatus, CommitBatch, CommitMutation,
    CommitOptions, CommitVersion, MaintenanceRequest, MaintenanceScope, MaintenanceSummaryStatus,
    MaintenanceTask, PrefixScanReadRequest, ReadBound, StorageApiResult, StorageDurabilityPolicy,
    StorageKey, StorageRuntime, StorageSpaceId, StorageValue,
};
use strata_storage_next::perf_trace;
use tempfile::TempDir;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_RECORDS: usize = 200_000;
const DEFAULT_DELTA: usize = 512;
const DEFAULT_SAMPLES: usize = 50;
const DEFAULT_BATCH: usize = 1_000;
const DEFAULT_FLUSH_EVERY: usize = 20_000;
const VALUE_BYTES: usize = 64;
const KEY_PREFIX: &[u8] = b"vec/";

type BenchResult<T> = Result<T, Box<dyn Error>>;

struct Config {
    records: usize,
    delta: usize,
    samples: usize,
    batch: usize,
    flush_every: usize,
    path: Option<PathBuf>,
}

impl Config {
    fn parse(mut args: impl Iterator<Item = String>) -> BenchResult<Self> {
        let mut config = Self {
            records: DEFAULT_RECORDS,
            delta: DEFAULT_DELTA,
            samples: DEFAULT_SAMPLES,
            batch: DEFAULT_BATCH,
            flush_every: DEFAULT_FLUSH_EVERY,
            path: None,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--records" => config.records = parse_usize(&arg, args.next())?,
                "--delta" => config.delta = parse_usize(&arg, args.next())?,
                "--samples" => config.samples = parse_usize(&arg, args.next())?,
                "--batch" => config.batch = parse_usize(&arg, args.next())?.max(1),
                "--flush-every" => config.flush_every = parse_usize(&arg, args.next())?.max(1),
                "--path" => {
                    let value = args.next().ok_or("--path requires a directory argument")?;
                    config.path = Some(PathBuf::from(value));
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        Ok(config)
    }
}

fn parse_usize(flag: &str, value: Option<String>) -> BenchResult<usize> {
    value
        .ok_or_else(|| format!("{flag} requires a value"))?
        .parse::<usize>()
        .map_err(|error| format!("{flag} expects an integer: {error}").into())
}

fn print_help() {
    eprintln!(
        "storage-next-after-version-scan\n\n\
         Durable benchmark for the after_version table-level I/O skip.\n\n\
         Options:\n  \
         --records N       bulk rows sealed into the LSM (default {DEFAULT_RECORDS})\n  \
         --delta N         fresh rows committed after the watermark (default {DEFAULT_DELTA})\n  \
         --samples N       scans per arm (default {DEFAULT_SAMPLES})\n  \
         --batch N         commit batch size during load (default {DEFAULT_BATCH})\n  \
         --flush-every N   flush+compact cadence in rows during load (default {DEFAULT_FLUSH_EVERY})\n  \
         --path DIR        durable directory (default: a temp dir, removed on exit)\n"
    );
}

fn main() {
    let config = match Config::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            print_help();
            std::process::exit(2);
        }
    };
    if let Err(error) = run(&config) {
        eprintln!("benchmark failed: {error}");
        std::process::exit(1);
    }
}

fn run(config: &Config) -> BenchResult<()> {
    let _tempdir;
    let root = match &config.path {
        Some(path) => {
            std::fs::create_dir_all(path)?;
            path.clone()
        }
        None => {
            _tempdir = TempDir::new()?;
            _tempdir.path().to_path_buf()
        }
    };

    eprintln!("storage-next after_version table-skip benchmark");
    eprintln!(
        "records={} delta={} samples={} batch={} flush_every={}",
        config.records, config.delta, config.samples, config.batch, config.flush_every
    );

    let mut runtime =
        StorageRuntime::open_durable_local(root, StorageDurabilityPolicy::Standard)?.into_runtime();
    let branch = discover_initial_branch(&mut runtime)?;
    let space = StorageSpaceId::new(vec![0x20])?;

    eprintln!("benchmark progress: loading {} bulk rows", config.records);
    let watermark = load_bulk(&mut runtime, branch, &space, config)?;
    seal(&mut runtime, branch)?;
    eprintln!(
        "benchmark progress: bulk sealed at watermark v{}",
        watermark.as_u64()
    );

    eprintln!("benchmark progress: committing {} delta rows", config.delta);
    commit_delta(&mut runtime, branch, &space, config)?;

    let prefix = StorageKey::new(KEY_PREFIX.to_vec())?;
    let baseline = measure_arm(&runtime, branch, &space, &prefix, config.samples, None)?;
    let skipped = measure_arm(
        &runtime,
        branch,
        &space,
        &prefix,
        config.samples,
        Some(watermark.as_u64()),
    )?;

    report(config, &baseline, &skipped);
    Ok(())
}

fn load_bulk(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    space: &StorageSpaceId,
    config: &Config,
) -> BenchResult<CommitVersion> {
    let value = vec![0x42; VALUE_BYTES];
    let mut written = 0usize;
    let mut next_flush = config.flush_every;
    let mut last_version = None;
    while written < config.records {
        let end = written.saturating_add(config.batch).min(config.records);
        let mutations = (written..end)
            .map(|index| CommitMutation::Put {
                storage_space: space.clone(),
                key: record_key(index),
                value: StorageValue::new(value.clone()),
                ttl: None,
            })
            .collect::<Vec<_>>();
        let batch = CommitBatch::new(
            branch,
            mutations,
            CommitOptions::default().require_conflict_check(false),
        )?;
        last_version = Some(runtime.commit(&batch)?.commit_version());
        written = end;
        if written >= next_flush {
            flush_and_compact(runtime, branch)?;
            next_flush = next_flush.saturating_add(config.flush_every);
        }
    }
    last_version.ok_or_else(|| "no rows loaded; --records must be > 0".into())
}

fn commit_delta(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    space: &StorageSpaceId,
    config: &Config,
) -> BenchResult<()> {
    if config.delta == 0 {
        return Ok(());
    }
    let value = vec![0x43; VALUE_BYTES];
    let mutations = (config.records..config.records.saturating_add(config.delta))
        .map(|index| CommitMutation::Put {
            storage_space: space.clone(),
            key: record_key(index),
            value: StorageValue::new(value.clone()),
            ttl: None,
        })
        .collect::<Vec<_>>();
    let batch = CommitBatch::new(
        branch,
        mutations,
        CommitOptions::default().require_conflict_check(false),
    )?;
    runtime.commit(&batch)?;
    Ok(())
}

struct ArmResult {
    after_version: Option<u64>,
    rows_returned: usize,
    nonzero_tables_opened: u64,
    l0_cursors: u64,
    latencies: Vec<Duration>,
}

fn measure_arm(
    runtime: &StorageRuntime<'_>,
    branch: BranchId,
    space: &StorageSpaceId,
    prefix: &StorageKey,
    samples: usize,
    after_version: Option<u64>,
) -> BenchResult<ArmResult> {
    let mut latencies = Vec::with_capacity(samples);
    let mut rows_returned = 0usize;
    perf_trace::reset();
    for _ in 0..samples {
        let request = scan_request(branch, space, prefix, after_version)?;
        let start = Instant::now();
        let outcome = runtime.scan_prefix(&request)?;
        latencies.push(start.elapsed());
        rows_returned = outcome.rows().len();
        black_box(rows_returned);
    }
    let perf = perf_trace::snapshot();
    Ok(ArmResult {
        after_version,
        rows_returned,
        nonzero_tables_opened: perf.scan_owned_nonzero_table_cursors_opened(),
        l0_cursors: perf.scan_owned_l0_cursors(),
        latencies,
    })
}

fn scan_request(
    branch: BranchId,
    space: &StorageSpaceId,
    prefix: &StorageKey,
    after_version: Option<u64>,
) -> StorageApiResult<PrefixScanReadRequest> {
    let request = PrefixScanReadRequest::new(
        branch,
        space.clone(),
        prefix.clone(),
        ReadBound::Latest,
        None,
    );
    Ok(match after_version {
        Some(version) => request.with_after_version(CommitVersion::new(version)),
        None => request,
    })
}

fn flush_and_compact(runtime: &mut StorageRuntime<'_>, branch: BranchId) -> BenchResult<()> {
    run_maintenance(runtime, branch, MaintenanceTask::Flush)?;
    run_maintenance(runtime, branch, MaintenanceTask::Compact)?;
    Ok(())
}

fn seal(runtime: &mut StorageRuntime<'_>, branch: BranchId) -> BenchResult<()> {
    flush_and_compact(runtime, branch)
}

fn run_maintenance(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    task: MaintenanceTask,
) -> BenchResult<()> {
    let summary = runtime.maintenance(&MaintenanceRequest::new(
        task,
        MaintenanceScope::Branch(branch),
    ))?;
    if summary.status() != MaintenanceSummaryStatus::Completed {
        return Err(format!(
            "{task:?} maintenance did not complete: {:?}",
            summary.status()
        )
        .into());
    }
    Ok(())
}

fn discover_initial_branch(runtime: &mut StorageRuntime<'_>) -> BenchResult<BranchId> {
    let outcome = runtime.branch(&BranchRequest::new(
        DEFAULT_BRANCH_ID,
        BranchAction::List,
        None,
    ))?;
    outcome
        .branches()
        .iter()
        .find(|branch| matches!(branch.status(), BranchStatus::Active))
        .map(|branch| branch.branch_id())
        .ok_or_else(|| "no active branch found in durable runtime".into())
}

fn record_key(index: usize) -> StorageKey {
    let mut key = KEY_PREFIX.to_vec();
    key.extend_from_slice(&(index as u64).to_be_bytes());
    StorageKey::new(key).expect("record key")
}

fn report(config: &Config, baseline: &ArmResult, skipped: &ArmResult) {
    eprintln!();
    print_arm("baseline (after_version=None)", baseline);
    print_arm("skip     (after_version=W)   ", skipped);
    let speedup = ratio(
        percentile(&baseline.latencies, 50),
        percentile(&skipped.latencies, 50),
    );
    eprintln!();
    eprintln!(
        "p50 speedup: {speedup:.1}x   sealed nonzero tables opened: {} -> {}",
        baseline.nonzero_tables_opened, skipped.nonzero_tables_opened
    );
    println!("{}", json_report(config, baseline, skipped));
}

fn print_arm(label: &str, arm: &ArmResult) {
    eprintln!(
        "  {label}  p50={:<10} p95={:<10} p99={:<10} rows={:<8} nonzero_tables_opened={} l0_cursors={}",
        fmt_duration(percentile(&arm.latencies, 50)),
        fmt_duration(percentile(&arm.latencies, 95)),
        fmt_duration(percentile(&arm.latencies, 99)),
        arm.rows_returned,
        arm.nonzero_tables_opened,
        arm.l0_cursors,
    );
}

fn json_report(config: &Config, baseline: &ArmResult, skipped: &ArmResult) -> String {
    format!(
        "{{\"benchmark\":\"storage-next-after-version-scan\",\"records\":{},\"delta\":{},\"samples\":{},\
         \"baseline\":{},\"skip\":{}}}",
        config.records,
        config.delta,
        config.samples,
        arm_json(baseline),
        arm_json(skipped),
    )
}

fn arm_json(arm: &ArmResult) -> String {
    format!(
        "{{\"after_version\":{},\"rows_returned\":{},\"nonzero_tables_opened\":{},\"l0_cursors\":{},\
         \"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{}}}",
        arm.after_version
            .map_or_else(|| "null".to_string(), |version| version.to_string()),
        arm.rows_returned,
        arm.nonzero_tables_opened,
        arm.l0_cursors,
        percentile(&arm.latencies, 50).as_nanos(),
        percentile(&arm.latencies, 95).as_nanos(),
        percentile(&arm.latencies, 99).as_nanos(),
    )
}

fn percentile(latencies: &[Duration], percentile: usize) -> Duration {
    if latencies.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len().saturating_mul(percentile) / 100).min(sorted.len() - 1);
    sorted[index]
}

fn ratio(baseline: Duration, skipped: Duration) -> f64 {
    if skipped.is_zero() {
        return f64::INFINITY;
    }
    baseline.as_secs_f64() / skipped.as_secs_f64()
}

fn fmt_duration(duration: Duration) -> String {
    let nanos = duration.as_nanos();
    if nanos < 1_000 {
        format!("{nanos} ns")
    } else if nanos < 1_000_000 {
        format!("{:.2} us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}
