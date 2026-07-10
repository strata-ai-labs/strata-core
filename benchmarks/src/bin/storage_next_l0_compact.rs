//! BS3.2 — L0→L1 compaction pipeline decomposition (profile gate).
//!
//! Drives one clean, single durable L0→L1 compaction and reports a *reproducible coarse* stage split
//! from the committed `perf-trace` timers: the lifecycle rewrite wall
//! (`lifecycle_compaction_elapsed_ns` = plan + build/merge + per-artifact publish) minus the inner
//! merge-loop timer (`table_compaction_merge_ns`) leaves plan+finish+publish. This is the
//! control-first metric BS3.3 reuses (compaction wall-time per input MB) and the frame BS3.2's finer,
//! transient `STRATA_TRACE` probes drill into. Machine-dependent *direction*, like the
//! concurrent-reads bin — the ≥500 MB/s budget target is out-of-band.
//!
//! Threading the durable admission needle with public API only: the active memtable rotates to frozen
//! at 64 MiB, the frozen byte budget blocks a 2nd rotation, and L0 auto-compacts at
//! `LEVEL_ZERO_COMPACTION_THRESHOLD` (4). So we flush every ~48 MiB — comfortably under the rotation,
//! so the active never rotates and frozen never accumulates — exactly 3 times, sealing 3 L0 tables,
//! then run one timed `Compact` before a 4th flush would auto-fire the L0→L1. ~3×~47 MiB (~142 MB) is
//! therefore the public-API ceiling for a *single* L0→L1; the production 300 MB / 5-table case raises the
//! publish share (more output tables), so this under-states, never over-states, publish.
//! `Compact` runs to a fixed point, so the single L1 output then metadata-promotes down the empty
//! levels — those relabels are byte-0 / build-0, so they do not perturb the timers used here; only
//! count metrics inflate, so the L0 input-table count is read from diagnostics, not the counters.
//!
//! Two operating points contrast per-row vs per-byte cost at equal bytes:
//!   `--value-bytes 1024`  row-dense  (~129k rows)
//!   `--value-bytes 8192`  byte-dense (~18k rows)
//! Run under `STRATA_TRACE=1` to also capture the per-compaction `WT ... compact level=0 ... ms=...`
//! line (its `ms`/`in_bytes` cross-check the L0→L1 in isolation).
//!
//! Usage: `cargo run --release --bin storage-l0-compact -- --value-bytes 1024`
//!
//! Measured (this dev box, single L0→L1, lifecycle-rewrite elapsed = plan+merge+publish):
//!   value_bytes / rows       merge loop        plan+finish+publish     elapsed  (MB/s)
//!   1024 / 129k              150 ms (36%)      260 ms (63%)            410 ms   (~350)
//!   8192 /  18k              129 ms (34%)      241 ms (65%)            370 ms   (~386)
//! Publish dominates; attributed over the 4 output tables (arm A, stripped STRATA_TRACE probes):
//!   publish_io write+fsync+dirsync ~108 ms · byte_validate (redundant re-read+memcmp) ~67 ms ·
//!   reader_handoff (in-memory reader build) ~67 ms.  The durable fsyncs (~88 ms of the 108),
//!   a redundant re-read of just-written bytes, and the reader build dominate; the data write is
//!   cheap (~20 ms). Merge is byte-bound (36% vs 34% across a 7× row-density change). Full
//!   decomposition + ranked BS3.3 fix list:
//! `docs/design/performance/bs3-compaction-admission-plan.md` (BS3.2 section).

// Link the benchmark lib for its #[global_allocator] (jemalloc): a bin that
// never references the lib does NOT link it, silently running on glibc
// malloc — whose per-thread arenas fragment unboundedly under multi-GB
// alloc/free churn (T4 RSS attribution, roadmap-v2).
extern crate strata_benchmarks;

use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

use strata_storage::api::{
    BranchAction, BranchId, BranchRequest, BranchStatus, CommitBatch, CommitMutation,
    CommitOptions, DiagnosticsRequest, DiagnosticsScope, MaintenanceRequest, MaintenanceScope,
    MaintenanceSummaryStatus, MaintenanceTask, StorageDurabilityPolicy, StorageKey,
    StorageMaintenanceSchedulingPolicy, StorageOpenOptions, StorageRuntime, StorageSpaceId,
    StorageValue,
};
use strata_storage::perf_trace;
use tempfile::TempDir;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_VALUE_BYTES: usize = 1_024;
const BATCH: usize = 1_000;
/// L0 tables to seal before compacting — kept below the auto-compaction threshold (4).
const TARGET_L0_TABLES: usize = 3;
/// Flush cadence in bytes, comfortably under the 64 MiB active-rotation threshold so the active is
/// sealed before it rotates: frozen never accumulates (which would block admission on the frozen byte
/// budget). Margin matters — the measured per-row overhead is ~116 B, so an aggressive cadence tips
/// the active over 64 MiB and rotates early.
const FLUSH_EVERY_BYTES: usize = 48 * 1024 * 1024;
const ROW_OVERHEAD_BYTES: usize = 128;
const KEY_PREFIX: &[u8] = b"c/";
const MIB: f64 = 1_048_576.0;

type BenchResult<T> = Result<T, Box<dyn Error>>;

struct Config {
    value_bytes: usize,
    path: Option<PathBuf>,
}

impl Config {
    fn parse(mut args: impl Iterator<Item = String>) -> BenchResult<Self> {
        let mut config = Self {
            value_bytes: DEFAULT_VALUE_BYTES,
            path: None,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--value-bytes" => config.value_bytes = parse_usize(&arg, args.next())?.max(1),
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

    /// Rows per L0 table (~48 MiB of row content), rounded to a whole number of commit batches so
    /// each flush lands on a batch boundary with an empty active afterwards.
    fn flush_every_rows(&self) -> usize {
        let rows = FLUSH_EVERY_BYTES / self.value_bytes.saturating_add(ROW_OVERHEAD_BYTES);
        ((rows / BATCH) * BATCH).max(BATCH)
    }

    fn records(&self) -> usize {
        self.flush_every_rows() * TARGET_L0_TABLES
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
        "storage-l0-compact\n\n\
         BS3.2 durable L0->L1 compaction decomposition (coarse perf-trace split).\n\n\
         Options:\n  \
         --value-bytes N   value size per row (default {DEFAULT_VALUE_BYTES}; 8192 = byte-dense arm)\n  \
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

    let records = config.records();
    eprintln!("storage L0->L1 compaction decomposition");
    eprintln!(
        "value_bytes={} records={} flush_every_rows={} target_l0={TARGET_L0_TABLES}",
        config.value_bytes,
        records,
        config.flush_every_rows()
    );

    // `Disabled` scheduling: no background workers, so nothing compacts until the explicit Compact.
    let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_maintenance_scheduling_policy(StorageMaintenanceSchedulingPolicy::Disabled);
    let mut runtime =
        StorageRuntime::open_durable_local_with_options(root, options)?.into_runtime();
    let branch = discover_initial_branch(&mut runtime)?;
    let space = StorageSpaceId::new(vec![0x20])?;

    eprintln!(
        "loading {records} rows, flushing every {} rows",
        config.flush_every_rows()
    );
    load(&mut runtime, branch, &space, config, records)?;

    let l0_tables = owned_l0_tables(&runtime, branch)?;
    eprintln!("L0 tables before compaction: {l0_tables}");
    if l0_tables >= 4 {
        eprintln!(
            "  warning: L0={l0_tables} >= 4 — an L0->L1 already auto-fired during load; the \
             single-op measurement is polluted"
        );
    }

    // Time exactly one Compact (plans L0->L1 with L1 empty; runs inline to a fixed point).
    perf_trace::reset();
    let started = Instant::now();
    run_maintenance(&mut runtime, branch, MaintenanceTask::Compact)?;
    let wall = started.elapsed();
    let perf = perf_trace::snapshot();

    report(config, l0_tables, wall, &perf);
    runtime.close()?;
    Ok(())
}

fn load(
    runtime: &mut StorageRuntime<'_>,
    branch: BranchId,
    space: &StorageSpaceId,
    config: &Config,
    records: usize,
) -> BenchResult<()> {
    let value = vec![0x5A; config.value_bytes];
    let flush_every = config.flush_every_rows();
    let mut written = 0usize;
    let mut next_flush = flush_every;
    while written < records {
        let end = written.saturating_add(BATCH).min(records);
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
        runtime.commit(&batch)?;
        written = end;
        if written >= next_flush {
            // Flush the (sub-rotation) active into one L0 table before it rotates, so frozen never
            // accumulates. Exactly TARGET_L0_TABLES flushes seal TARGET_L0_TABLES L0 tables.
            run_maintenance(runtime, branch, MaintenanceTask::Flush)?;
            next_flush = next_flush.saturating_add(flush_every);
        }
    }
    Ok(())
}

fn owned_l0_tables(runtime: &StorageRuntime<'_>, branch: BranchId) -> BenchResult<usize> {
    let diagnostics =
        runtime.diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(branch)))?;
    Ok(diagnostics.source_layout().owned_l0_tables())
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

fn report(
    config: &Config,
    l0_tables: usize,
    wall: std::time::Duration,
    perf: &strata_storage::perf_trace::StoragePerfSnapshot,
) {
    let elapsed_ns = perf.lifecycle_compaction_elapsed_ns();
    let merge_ns = perf.table_compaction_merge_ns();
    let rest_ns = elapsed_ns.saturating_sub(merge_ns);
    let input_bytes = perf.lifecycle_compaction_input_bytes();
    let output_bytes = perf.lifecycle_compaction_output_bytes();
    let input_mb = input_bytes as f64 / MIB;
    let elapsed_s = elapsed_ns as f64 / 1_000_000_000.0;
    let mb_per_s = if elapsed_s > 0.0 {
        input_mb / elapsed_s
    } else {
        0.0
    };

    eprintln!();
    eprintln!("== L0->L1 compaction decomposition (coarse, perf-trace) ==");
    eprintln!("  value_bytes          {}", config.value_bytes);
    eprintln!("  L0 tables in         {l0_tables}");
    eprintln!(
        "  L0->L1 ops           {}   (all lifecycle ops incl. metadata promotions: {})",
        perf.lifecycle_compaction_l0_to_level_one_operations(),
        perf.lifecycle_compaction_operations_completed()
    );
    eprintln!(
        "  outer wall           {}   (incl. fixed-point promotion manifest fsyncs)",
        fmt_ns(wall.as_nanos() as u64)
    );
    eprintln!(
        "  lifecycle elapsed    {}   (plan + merge + publish; promotions add ~0)",
        fmt_ns(elapsed_ns)
    );
    eprintln!(
        "    merge loop         {}   ({}%)",
        fmt_ns(merge_ns),
        pct(merge_ns, elapsed_ns)
    );
    eprintln!(
        "    plan+finish+pub    {}   ({}%)",
        fmt_ns(rest_ns),
        pct(rest_ns, elapsed_ns)
    );
    eprintln!(
        "  input                {input_mb:.1} MB  ({} rows)",
        perf.lifecycle_compaction_input_rows()
    );
    eprintln!(
        "  output               {:.1} MB  ({} tables)",
        output_bytes as f64 / MIB,
        perf.lifecycle_compaction_output_tables()
    );
    eprintln!(
        "  row clones (H2)      {}",
        perf.table_compaction_row_clones()
    );
    eprintln!("  throughput           {mb_per_s:.0} MB/s  (input / lifecycle elapsed)");

    println!(
        "{{\"benchmark\":\"storage-l0-compact\",\"value_bytes\":{},\"l0_tables\":{},\
         \"outer_wall_ns\":{},\"lifecycle_elapsed_ns\":{},\"merge_ns\":{},\"rest_ns\":{},\
         \"input_bytes\":{},\"output_bytes\":{},\"input_rows\":{},\"row_clones\":{},\
         \"l0_to_l1_ops\":{},\"mb_per_s\":{:.1}}}",
        config.value_bytes,
        l0_tables,
        wall.as_nanos(),
        elapsed_ns,
        merge_ns,
        rest_ns,
        input_bytes,
        output_bytes,
        perf.lifecycle_compaction_input_rows(),
        perf.table_compaction_row_clones(),
        perf.lifecycle_compaction_l0_to_level_one_operations(),
        mb_per_s,
    );
}

fn pct(part: u64, whole: u64) -> u64 {
    if whole == 0 {
        return 0;
    }
    part.saturating_mul(100) / whole
}

fn fmt_ns(nanos: u64) -> String {
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
