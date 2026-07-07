//! BS5.0 — concurrent-writer scaling baseline (the milestone's instrument, built first).
//!
//! N writer threads share one `&runtime` (commit is `&self`; the runtime is `Send + Sync`) and
//! issue independent distinct-key commit batches for a fixed window, swept over thread counts.
//! Today every commit serializes on the single runtime mutex — and in `Always` durability each
//! commit additionally pays its own fsync under that lock — so the expected baseline is
//! flat-to-negative scaling (worst in `Always`). BS5.1+ (write groups, commit path off the
//! mutex) are gated on moving these curves; single-thread must never regress.
//!
//! Each measurement point runs against a FRESH runtime (writes mutate state; reusing one
//! database would hand later points a larger dataset and background-maintenance debt).
//!
//! Usage:
//!   cargo run --release --bin storage-next-concurrent-writers -- \
//!     [--engines cache,standard,always] [--branches shared|per-writer] [--readers M]
//!     [--batch-size N] [--value-bytes N] [--window-secs S] [--threads 1,2,4,8]
//!
//! Output: CSV `engine,branches,threads,readers,total_ops,ops_per_sec,min_thread_ops,
//! max_thread_ops` on stdout (one row per point), plus a `BenchmarkReport` JSON under
//! `benchmarks/results/storage-next-concurrent-writers/`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use strata_benchmarks::harness::{read_cpu_model, read_total_ram_gb};
use strata_benchmarks::schema::{
    BenchmarkMetrics, BenchmarkReport, BenchmarkResult, HardwareInfo, RunMetadata,
};
use strata_storage_next::api::{
    BranchAction, BranchGeneration, BranchId, BranchRequest, CommitBatch, CommitMutation,
    CommitOptions, PointReadRequest, ReadBound, StorageApiErrorClass, StorageDurabilityPolicy,
    StorageKey, StorageRuntime, StorageSpaceId, StorageValue,
};
use tempfile::TempDir;

const DEFAULT_THREADS: &[usize] = &[1, 2, 4, 8];
const DEFAULT_BATCH_SIZE: usize = 10;
const DEFAULT_VALUE_BYTES: usize = 16;
const DEFAULT_WINDOW: Duration = Duration::from_secs(3);
const READER_KEYS: usize = 10_000;

const SHARED_BRANCH: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);

fn space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine space")
}

fn writer_branch(index: usize) -> BranchId {
    let mut bytes = [0x70; BranchId::BYTE_LEN];
    bytes[BranchId::BYTE_LEN - 1] = u8::try_from(index).expect("small writer index");
    BranchId::from_bytes(bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Engine {
    Cache,
    DurableStandard,
    DurableAlways,
}

impl Engine {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "cache" => Ok(Self::Cache),
            "standard" => Ok(Self::DurableStandard),
            "always" => Ok(Self::DurableAlways),
            other => Err(format!("unknown engine {other:?}")),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::DurableStandard => "standard",
            Self::DurableAlways => "always",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BranchMode {
    Shared,
    PerWriter,
}

impl BranchMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "shared" => Ok(Self::Shared),
            "per-writer" => Ok(Self::PerWriter),
            other => Err(format!("unknown branch mode {other:?}")),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::PerWriter => "per-writer",
        }
    }
}

struct Config {
    engines: Vec<Engine>,
    branch_modes: Vec<BranchMode>,
    threads: Vec<usize>,
    readers: usize,
    batch_size: usize,
    value_bytes: usize,
    window: Duration,
    root: PathBuf,
    perf_breakdown: bool,
}

fn parse_config() -> Result<Config, String> {
    let mut config = Config {
        engines: vec![
            Engine::Cache,
            Engine::DurableStandard,
            Engine::DurableAlways,
        ],
        branch_modes: vec![BranchMode::Shared],
        threads: DEFAULT_THREADS.to_vec(),
        readers: 0,
        batch_size: DEFAULT_BATCH_SIZE,
        value_bytes: DEFAULT_VALUE_BYTES,
        window: DEFAULT_WINDOW,
        root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".benchmark")
            .join("storage-next-concurrent-writers"),
        perf_breakdown: false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let mut value = |name: &str| -> Result<&str, String> {
            index += 1;
            args.get(index)
                .map(String::as_str)
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match arg {
            "--engines" => {
                config.engines = value("--engines")?
                    .split(',')
                    .map(Engine::parse)
                    .collect::<Result<_, _>>()?;
            }
            "--branches" => {
                config.branch_modes = value("--branches")?
                    .split(',')
                    .map(BranchMode::parse)
                    .collect::<Result<_, _>>()?;
            }
            "--threads" => {
                config.threads = value("--threads")?
                    .split(',')
                    .map(|t| t.parse::<usize>().map_err(|e| e.to_string()))
                    .collect::<Result<_, _>>()?;
            }
            "--readers" => {
                config.readers = value("--readers")?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            }
            "--batch-size" => {
                config.batch_size = value("--batch-size")?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            }
            "--value-bytes" => {
                config.value_bytes = value("--value-bytes")?
                    .parse()
                    .map_err(|e: std::num::ParseIntError| e.to_string())?;
            }
            "--window-secs" => {
                config.window = Duration::from_secs(
                    value("--window-secs")?
                        .parse()
                        .map_err(|e: std::num::ParseIntError| e.to_string())?,
                );
            }
            "--perf-breakdown" => {
                config.perf_breakdown = true;
            }
            other => return Err(format!("unknown flag {other:?}")),
        }
        index += 1;
    }
    if config.batch_size == 0 || config.value_bytes == 0 || config.threads.is_empty() {
        return Err("batch size, value bytes, and threads must be nonzero".into());
    }
    Ok(config)
}

/// A fresh runtime for one measurement point. The tempdir (durable engines) lives as long as
/// the returned guard.
fn open_point_runtime(
    engine: Engine,
    root: &std::path::Path,
) -> (StorageRuntime<'static>, Option<TempDir>) {
    match engine {
        Engine::Cache => (
            StorageRuntime::open_ephemeral()
                .expect("open ephemeral runtime")
                .into_runtime(),
            None,
        ),
        Engine::DurableStandard | Engine::DurableAlways => {
            std::fs::create_dir_all(root).expect("create benchmark root");
            let dir = tempfile::tempdir_in(root).expect("create point tempdir");
            let policy = if engine == Engine::DurableAlways {
                StorageDurabilityPolicy::Always
            } else {
                StorageDurabilityPolicy::Standard
            };
            let runtime = StorageRuntime::open_durable_local(dir.path().to_path_buf(), policy)
                .expect("open durable runtime")
                .into_runtime();
            (runtime, Some(dir))
        }
    }
}

/// The branch each writer commits to under `mode`, creating per-writer branches on demand.
fn setup_branches(
    runtime: &StorageRuntime<'static>,
    mode: BranchMode,
    writers: usize,
) -> Vec<BranchId> {
    match mode {
        BranchMode::Shared => vec![SHARED_BRANCH; writers],
        BranchMode::PerWriter => (0..writers)
            .map(|index| {
                let branch = writer_branch(index);
                runtime
                    .branch(&BranchRequest::new(
                        branch,
                        BranchAction::Create,
                        Some(BranchGeneration::new(1)),
                    ))
                    .expect("create writer branch");
                branch
            })
            .collect(),
    }
}

/// Seed keys for the optional reader threads (written to the shared branch).
fn seed_reader_keys(runtime: &StorageRuntime<'static>, value_bytes: usize) {
    let mut index = 0;
    while index < READER_KEYS {
        let end = (index + 1_000).min(READER_KEYS);
        let mutations: Vec<CommitMutation> = (index..end)
            .map(|i| CommitMutation::Put {
                storage_space: space(),
                key: StorageKey::new(format!("seed-{i:08}").into_bytes()).expect("valid key"),
                value: StorageValue::new(vec![0x5A; value_bytes]),
                ttl: None,
            })
            .collect();
        let batch = CommitBatch::new(
            SHARED_BRANCH,
            mutations,
            CommitOptions::default().require_conflict_check(false),
        )
        .expect("valid seed batch");
        runtime.commit(&batch).expect("seed commit");
        index = end;
    }
}

struct PointOutcome {
    total_ops: u64,
    ops_per_sec: f64,
    min_thread_ops: u64,
    max_thread_ops: u64,
    stalls: u64,
    read_ops: u64,
}

/// One measurement point: `writers` threads commit distinct-key batches on a shared `&runtime`
/// for the window; optional reader threads hammer seeded point reads alongside.
fn run_point(config: &Config, engine: Engine, mode: BranchMode, writers: usize) -> PointOutcome {
    let (runtime, _dir) = open_point_runtime(engine, &config.root);
    let branches = setup_branches(&runtime, mode, writers);
    if config.readers > 0 {
        seed_reader_keys(&runtime, config.value_bytes);
    }
    if config.perf_breakdown {
        strata_storage_next::perf_trace::reset();
    }

    let stop = AtomicBool::new(false);
    let read_total = AtomicU64::new(0);
    let mut per_writer_ops = vec![0u64; writers];
    let mut total_stalls = 0u64;
    let started = Instant::now();
    std::thread::scope(|scope| {
        let mut writer_handles = Vec::with_capacity(writers);
        for (writer, branch) in branches.iter().copied().enumerate() {
            let (runtime, stop) = (&runtime, &stop);
            let (batch_size, value_bytes) = (config.batch_size, config.value_bytes);
            writer_handles.push(scope.spawn(move || {
                let mut committed = 0u64;
                let mut stalled = 0u64;
                let mut next_key = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    // Distinct keys per writer (prefix) and per batch (counter): no conflict
                    // overlap, so the measurement isolates the commit protocol itself.
                    let mutations: Vec<CommitMutation> = (0..batch_size)
                        .map(|offset| CommitMutation::Put {
                            storage_space: space(),
                            key: StorageKey::new(
                                format!("w{writer:02}-{:012}", next_key + offset).into_bytes(),
                            )
                            .expect("valid key"),
                            value: StorageValue::new(vec![0xC3; value_bytes]),
                            ttl: None,
                        })
                        .collect();
                    next_key += batch_size;
                    let batch = CommitBatch::new(
                        branch,
                        mutations,
                        CommitOptions::default().require_conflict_check(false),
                    )
                    .expect("valid writer batch");
                    match runtime.commit(&batch) {
                        Ok(_) => committed += 1,
                        // Saturation is data, not a crash: budget/pressure rejections mean the
                        // writers outran maintenance for this window. Count the stall, back off
                        // briefly, and continue with fresh keys.
                        Err(error)
                            if matches!(
                                error.class(),
                                StorageApiErrorClass::ResourceExhausted
                                    | StorageApiErrorClass::FailedPrecondition
                            ) =>
                        {
                            stalled += 1;
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) => panic!("writer commit failed: {error}"),
                    }
                }
                (committed, stalled)
            }));
        }
        for _ in 0..config.readers {
            let (runtime, stop, read_total) = (&runtime, &stop, &read_total);
            scope.spawn(move || {
                let mut count = 0u64;
                let mut idx = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    let request = PointReadRequest::new(
                        SHARED_BRANCH,
                        space(),
                        StorageKey::new(format!("seed-{:08}", idx % READER_KEYS).into_bytes())
                            .expect("valid key"),
                        ReadBound::Latest,
                    );
                    let outcome = runtime.read_point(&request).expect("reader read");
                    std::hint::black_box(outcome.row().is_some());
                    count += 1;
                    idx += 1;
                }
                read_total.fetch_add(count, Ordering::Relaxed);
            });
        }
        std::thread::sleep(config.window);
        stop.store(true, Ordering::Relaxed);
        for (writer, handle) in writer_handles.into_iter().enumerate() {
            let (committed, stalled) = handle.join().expect("writer thread");
            per_writer_ops[writer] = committed;
            total_stalls += stalled;
        }
    });
    let elapsed = started.elapsed().as_secs_f64();

    let total_ops: u64 = per_writer_ops.iter().sum();
    if config.perf_breakdown && total_ops > 0 {
        let perf = strata_storage_next::perf_trace::snapshot();
        let per = |ns: u64| ns as f64 / total_ops as f64 / 1_000.0;
        eprintln!(
            "[breakdown {} {}T] per-commit us: api_map={:.1} api_clone={:.1} admit={:.1} \
             setup={:.1} exec_admission={:.1} conflict={:.1} stage={:.1} wal_build={:.1} \
             wal_append={:.1} apply={:.1} publish={:.1} post_growth={:.1} post_maint={:.1} \
             api_post={:.1} | api_runtime_total={:.1}",
            engine.name(),
            writers,
            per(perf.api_commit_map_ns()),
            per(perf.commit_api_batch_clone_ns()),
            per(perf.commit_admit_ns()),
            per(perf.commit_setup_ns()),
            per(perf.commit_exec_admission_ns()),
            per(perf.commit_exec_conflict_ns()),
            per(perf.commit_exec_stage_ns()),
            per(perf.commit_wal_record_build_ns()),
            per(perf.commit_wal_append_ns()),
            per(perf.commit_exec_apply_ns()),
            per(perf.commit_exec_publish_ns()),
            per(perf.commit_post_wal_growth_ns()),
            per(perf.commit_post_maintenance_ns()),
            per(perf.commit_api_post_ns()),
            per(perf.api_commit_runtime_ns()),
        );
        eprintln!(
            "[bg {} {}T] rounds={} tasks={} snapshot_lock_total_ms={:.1} task_total_ms={:.1} fg_wait_total_ms={:.1}",
            engine.name(),
            writers,
            perf.lifecycle_background_drain_rounds(),
            perf.lifecycle_background_tasks_completed(),
            perf.lifecycle_background_task_snapshot_lock_ns() as f64 / 1e6,
            perf.lifecycle_background_task_total_ns() as f64 / 1e6,
            perf.lifecycle_foreground_wait_background_lock_ns() as f64 / 1e6,
        );
        eprintln!(
            "[bg2 {} {}T] publish_lock_ms={:.1} unlocked_build_ms={:.1} low_tier_runs={} low_tier_ms={:.1} post_maint_enq={} post_maint_coalesced={}",
            engine.name(),
            writers,
            perf.lifecycle_background_task_publish_lock_ns() as f64 / 1e6,
            perf.lifecycle_background_task_unlocked_build_ns() as f64 / 1e6,
            perf.lifecycle_background_task_low_tier_runs(),
            perf.lifecycle_background_task_low_tier_ns() as f64 / 1e6,
            perf.lifecycle_post_commit_maintenance_tasks_enqueued(),
            perf.lifecycle_post_commit_maintenance_tasks_coalesced(),
        );
        let queue = runtime.maintenance_status().expect("queue status");
        eprintln!("[queue {} {}T] {:?}", engine.name(), writers, queue,);
    }
    PointOutcome {
        total_ops,
        ops_per_sec: total_ops as f64 / elapsed,
        min_thread_ops: per_writer_ops.iter().copied().min().unwrap_or(0),
        max_thread_ops: per_writer_ops.iter().copied().max().unwrap_or(0),
        stalls: total_stalls,
        read_ops: read_total.load(Ordering::Relaxed),
    }
}

fn main() {
    let config = match parse_config() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(2);
        }
    };
    eprintln!(
        "concurrent-writers: engines={:?} branches={:?} threads={:?} readers={} batch={} value={}B window={:?}",
        config.engines.iter().map(|e| e.name()).collect::<Vec<_>>(),
        config.branch_modes.iter().map(|m| m.name()).collect::<Vec<_>>(),
        config.threads,
        config.readers,
        config.batch_size,
        config.value_bytes,
        config.window,
    );

    let mut results = Vec::new();
    println!("engine,branches,threads,readers,total_ops,ops_per_sec,min_thread_ops,max_thread_ops,stalls");
    for &engine in &config.engines {
        for &mode in &config.branch_modes {
            for &writers in &config.threads {
                let outcome = run_point(&config, engine, mode, writers);
                println!(
                    "{},{},{},{},{},{:.0},{},{},{}",
                    engine.name(),
                    mode.name(),
                    writers,
                    config.readers,
                    outcome.total_ops,
                    outcome.ops_per_sec,
                    outcome.min_thread_ops,
                    outcome.max_thread_ops,
                    outcome.stalls,
                );
                if config.readers > 0 {
                    eprintln!("  (reads alongside: {} ops)", outcome.read_ops);
                }
                let mut parameters = HashMap::new();
                parameters.insert("engine".to_string(), serde_json::json!(engine.name()));
                parameters.insert("branches".to_string(), serde_json::json!(mode.name()));
                parameters.insert("threads".to_string(), serde_json::json!(writers));
                parameters.insert("readers".to_string(), serde_json::json!(config.readers));
                parameters.insert(
                    "batch_size".to_string(),
                    serde_json::json!(config.batch_size),
                );
                parameters.insert(
                    "value_bytes".to_string(),
                    serde_json::json!(config.value_bytes),
                );
                parameters.insert(
                    "window_secs".to_string(),
                    serde_json::json!(config.window.as_secs()),
                );
                parameters.insert(
                    "min_thread_ops".to_string(),
                    serde_json::json!(outcome.min_thread_ops),
                );
                parameters.insert(
                    "max_thread_ops".to_string(),
                    serde_json::json!(outcome.max_thread_ops),
                );
                parameters.insert("stalls".to_string(), serde_json::json!(outcome.stalls));
                results.push(BenchmarkResult {
                    benchmark: format!(
                        "storage-next-concurrent-writers/{}-{}-t{}",
                        engine.name(),
                        mode.name(),
                        writers
                    ),
                    category: "storage-next-concurrent-writers".to_string(),
                    parameters,
                    metrics: BenchmarkMetrics {
                        ops_per_sec: Some(outcome.ops_per_sec),
                        samples: Some(outcome.total_ops),
                        ..Default::default()
                    },
                });
            }
        }
    }

    let report = BenchmarkReport {
        schema_version: 1,
        metadata: run_metadata(),
        results,
    };
    match save_report(&report) {
        Ok(path) => eprintln!("results: {}", path.display()),
        Err(error) => eprintln!("failed to save results: {error}"),
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

fn save_report(report: &BenchmarkReport) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("results")
        .join("storage-next-concurrent-writers");
    std::fs::create_dir_all(&dir)?;
    let commit = report
        .metadata
        .git_commit
        .as_deref()
        .unwrap_or("unknown")
        .to_string();
    let timestamp = report.metadata.timestamp.replace(':', "-");
    let path = dir.join(format!(
        "storage-next-concurrent-writers-{timestamp}-{commit}.json"
    ));
    std::fs::write(&path, serde_json::to_string_pretty(report)?)?;
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
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60
    )
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
    (if m <= 2 { y + 1 } else { y }, m, d)
}
