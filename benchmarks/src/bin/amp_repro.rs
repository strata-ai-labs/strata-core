//! #2524 disk-amplification repro: the GT5 `EnginePageStore` write shape
//! against the engine-next KV surface.
//!
//! Seeds `--commits` batches of `2N+1` rows each — N x `page/<BE u64>`
//! (4 KiB), N x `meta/<BE u64>` (~150 B), plus the 8 B `watermark` row
//! rewritten every commit and the 16 B `manifest` row rewritten via a
//! separate single-row commit (the GT5 store has TWO per-commit hot keys) —
//! then watches the on-disk reclaim curve through a settle window and dumps
//! the compaction/reclaim perf-trace counters.
//!
//! The `--shape` knob removes zones to attribute the gluing:
//!   full        page + meta + watermark + manifest (GT5 exact)
//!   nowatermark page + meta (no hot keys)
//!   pagesonly   page only (true sequential control)
//!
//! Run with `STRATA_TRACE=1` to capture per-pass `compact level= input=
//! overlap=` lines on stderr (the gluing fingerprint).
//!
//! Usage:
//!   amp-repro --commits 2048 --batch 256 --shape full --settle-secs 180

// Link the benchmark lib for its #[global_allocator] (jemalloc) — see
// engine_ycsb.rs for why an unreferenced lib silently drops the allocator.
extern crate strata_benchmarks;

use std::process;
use std::time::Instant;

use strata_engine_next::{
    BranchName, Database, DurableLocalOpenOptions, EngineResult, KvKey, KvValue, ProductSpace,
};

#[allow(dead_code)]
#[path = "../../benches/ycsb_workloads.rs"]
mod ycsb_workloads;

use ycsb_workloads::dir_size_bytes;

const DEFAULT_COMMITS: usize = 2_048;
const DEFAULT_BATCH: usize = 256;
const DEFAULT_PAGE_BYTES: usize = 4_096;
const DEFAULT_META_BYTES: usize = 150;
const DEFAULT_SETTLE_SECS: u64 = 180;
const SETTLE_PROBE_INTERVAL_SECS: u64 = 15;
const SEED_PROBE_EVERY_COMMITS: usize = 256;
const READ_ROUND_PAGES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Full,
    NoWatermark,
    PagesOnly,
}

impl Shape {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "full" => Some(Self::Full),
            "nowatermark" => Some(Self::NoWatermark),
            "pagesonly" => Some(Self::PagesOnly),
            _ => None,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoWatermark => "nowatermark",
            Self::PagesOnly => "pagesonly",
        }
    }
}

struct Config {
    commits: usize,
    batch: usize,
    page_bytes: usize,
    shape: Shape,
    settle_secs: u64,
    memory_budget_bytes: Option<u64>,
    read_rounds: usize,
    /// #2527: time `fork_current` calls after seed+settle (a small commit
    /// precedes each fork so the memtable is dirty — the shape that forced
    /// the O(dataset) eager fork).
    fork_rounds: usize,
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!(
                "usage: amp-repro [--commits N] [--batch N] [--page-bytes N] \
                 [--shape full|nowatermark|pagesonly] [--settle-secs N] \
                 [--memory-budget SIZE] [--reads N]"
            );
            process::exit(2);
        }
    };
    if let Err(error) = run(&config) {
        eprintln!("amp-repro failed: {error}");
        process::exit(1);
    }
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        commits: DEFAULT_COMMITS,
        batch: DEFAULT_BATCH,
        page_bytes: DEFAULT_PAGE_BYTES,
        shape: Shape::Full,
        settle_secs: DEFAULT_SETTLE_SECS,
        memory_budget_bytes: None,
        read_rounds: 64,
        fork_rounds: 0,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match arg.as_str() {
            "--commits" => config.commits = parse_count(&value("--commits")?)?,
            "--batch" => config.batch = parse_count(&value("--batch")?)?,
            "--page-bytes" => config.page_bytes = parse_count(&value("--page-bytes")?)?,
            "--shape" => {
                let raw = value("--shape")?;
                config.shape = Shape::parse(&raw)
                    .ok_or_else(|| format!("unknown shape {raw:?} (full|nowatermark|pagesonly)"))?;
            }
            "--settle-secs" => {
                config.settle_secs = parse_count(&value("--settle-secs")?)? as u64;
            }
            "--memory-budget" => {
                config.memory_budget_bytes = Some(parse_size(&value("--memory-budget")?)?);
            }
            "--reads" => config.read_rounds = parse_count(&value("--reads")?)?,
            "--forks" => config.fork_rounds = parse_count(&value("--forks")?)?,
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if config.commits == 0 || config.batch == 0 {
        return Err("--commits and --batch must be non-zero".to_owned());
    }
    Ok(config)
}

fn parse_count(raw: &str) -> Result<usize, String> {
    let (digits, multiplier) = match raw.as_bytes().last() {
        Some(b'k' | b'K') => (&raw[..raw.len() - 1], 1_000usize),
        Some(b'm' | b'M') => (&raw[..raw.len() - 1], 1_000_000usize),
        _ => (raw, 1usize),
    };
    digits
        .parse::<usize>()
        .map(|value| value * multiplier)
        .map_err(|_| format!("invalid count {raw:?}"))
}

fn parse_size(raw: &str) -> Result<u64, String> {
    let (digits, multiplier) = match raw.as_bytes().last() {
        Some(b'k' | b'K') => (&raw[..raw.len() - 1], 1u64 << 10),
        Some(b'm' | b'M') => (&raw[..raw.len() - 1], 1u64 << 20),
        Some(b'g' | b'G') => (&raw[..raw.len() - 1], 1u64 << 30),
        _ => (raw, 1u64),
    };
    digits
        .parse::<u64>()
        .map(|value| value * multiplier)
        .map_err(|_| format!("invalid size {raw:?}"))
}

fn run(config: &Config) -> EngineResult<()> {
    let root = std::env::current_dir()
        .expect("current dir")
        .join(".benchmark");
    std::fs::create_dir_all(&root).expect("benchmark root");
    let tempdir = tempfile::tempdir_in(&root).expect("temporary repro database");
    let path = tempdir.path();

    let mut options = DurableLocalOpenOptions::new();
    if let Some(budget) = config.memory_budget_bytes {
        options = options.with_memory_budget(budget);
    }
    let mut database = Database::open_local(path, options)?.into_database();
    let branch = database.default_branch().clone();
    let space = ProductSpace::new("tier")?;
    let mut kv = database.kv(branch, space)?;

    let logical = logical_bytes(config);
    println!(
        "[amp-repro] shape={} commits={} batch={} page_bytes={} logical={}",
        config.shape.label(),
        config.commits,
        config.batch,
        config.page_bytes,
        format_bytes(logical),
    );

    strata_storage_next::perf_trace::reset();

    // Seed phase: GT5's commit_batch — 2N+1 rows per put_batch (+ the
    // separate manifest put), sequential ids, no overwrites.
    let seed_start = Instant::now();
    let mut next_id: u64 = 0;
    for commit in 0..config.commits {
        let mut rows: Vec<(KvKey, KvValue)> = Vec::with_capacity(config.batch * 2 + 1);
        for _ in 0..config.batch {
            let id = next_id;
            next_id += 1;
            rows.push((
                page_key(id),
                KvValue::new(page_value(id, config.page_bytes)),
            ));
            if config.shape != Shape::PagesOnly {
                rows.push((meta_key(id), KvValue::new(meta_value(id))));
            }
        }
        if config.shape == Shape::Full {
            rows.push((
                KvKey::new(b"watermark".to_vec()).expect("valid watermark key"),
                KvValue::new(next_id.to_be_bytes().to_vec()),
            ));
        }
        kv.put_batch(rows)?;
        if config.shape == Shape::Full {
            let manifest: Vec<u8> = [next_id.to_be_bytes(), next_id.to_be_bytes()].concat();
            kv.put(
                KvKey::new(b"manifest".to_vec()).expect("valid manifest key"),
                KvValue::new(manifest),
            )?;
        }
        if (commit + 1) % SEED_PROBE_EVERY_COMMITS == 0 {
            let on_disk = dir_size_bytes(path);
            println!(
                "[seed] commits={} elapsed={:.1}s on_disk={} amp={:.1}x",
                commit + 1,
                seed_start.elapsed().as_secs_f64(),
                format_bytes(on_disk),
                amp(on_disk, logical_bytes_at(config, commit + 1)),
            );
        }
    }
    let seed_elapsed = seed_start.elapsed();
    let post_seed = dir_size_bytes(path);
    println!(
        "[post-seed] elapsed={:.1}s on_disk={} amp={:.1}x",
        seed_elapsed.as_secs_f64(),
        format_bytes(post_seed),
        amp(post_seed, logical),
    );
    print_counters("post-seed");

    // Settle: the reclaim curve. Converging amp = healthy sweep; a plateau
    // far above ~1.5x = reclaim starvation (mechanism B).
    let settle_start = Instant::now();
    while settle_start.elapsed().as_secs() < config.settle_secs {
        std::thread::sleep(std::time::Duration::from_secs(SETTLE_PROBE_INTERVAL_SECS));
        let on_disk = dir_size_bytes(path);
        println!(
            "[settle] t={}s on_disk={} amp={:.1}x",
            settle_start.elapsed().as_secs(),
            format_bytes(on_disk),
            amp(on_disk, logical),
        );
    }
    print_counters("post-settle");

    // Read phase: GT5's maintenance round shape — batched point reads over
    // random committed pages (+ their meta rows in the full/nowatermark
    // shapes). ms-scale p50 here is the issue's read symptom.
    if config.read_rounds > 0 {
        let total_ids = (config.commits * config.batch) as u64;
        let mut latencies_us: Vec<u64> = Vec::with_capacity(config.read_rounds);
        let mut rng_state: u64 = 0x2524_2524_2524_2524;
        for _ in 0..config.read_rounds {
            let round_start = Instant::now();
            for _ in 0..READ_ROUND_PAGES {
                rng_state = splitmix64(&mut rng_state);
                let id = rng_state % total_ids;
                let _page = kv.get(&page_key(id))?;
                if config.shape != Shape::PagesOnly {
                    let _meta = kv.get(&meta_key(id))?;
                }
            }
            let elapsed = round_start.elapsed();
            latencies_us.push(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX));
        }
        latencies_us.sort_unstable();
        let p50 = latencies_us[latencies_us.len() / 2];
        let p99 = latencies_us[(latencies_us.len() * 99) / 100];
        println!(
            "[reads] rounds={} pages_per_round={} round_p50={}us round_p99={}us",
            config.read_rounds, READ_ROUND_PAGES, p50, p99,
        );
    }

    // #2527: fork latency + disk growth. One small commit before each fork
    // keeps the memtable dirty — the GT5 rollout shape that forced the
    // eager O(dataset) fork (~1.9s + a full duplicate per fork).
    if config.fork_rounds > 0 {
        let fork_base = dir_size_bytes(path);
        let dirty_branch = database.default_branch().clone();
        let mut fork_ms: Vec<u128> = Vec::with_capacity(config.fork_rounds);
        for round in 0..config.fork_rounds {
            let mut dirty_kv = database.kv(dirty_branch.clone(), ProductSpace::new("tier")?)?;
            dirty_kv.put(
                KvKey::new(format!("fork-dirty-{round}").into_bytes()).expect("valid key"),
                KvValue::new(vec![0xF0; 64]),
            )?;
            drop(dirty_kv);
            let fork_start = Instant::now();
            database.branches()?.fork_current(
                &BranchName::new("default")?,
                BranchName::new(format!("rollout-{round}"))?,
            )?;
            fork_ms.push(fork_start.elapsed().as_millis());
        }
        let grown = dir_size_bytes(path).saturating_sub(fork_base);
        fork_ms.sort_unstable();
        println!(
            "[forks] rounds={} p50_ms={} max_ms={} disk_growth={}",
            config.fork_rounds,
            fork_ms[fork_ms.len() / 2],
            fork_ms.last().copied().unwrap_or(0),
            format_bytes(grown),
        );
    }

    let final_on_disk = dir_size_bytes(path);
    println!(
        "[final] on_disk={} logical={} amp={:.1}x",
        format_bytes(final_on_disk),
        format_bytes(logical),
        amp(final_on_disk, logical),
    );
    drop(database);
    Ok(())
}

fn print_counters(phase: &str) {
    let perf = strata_storage_next::perf_trace::snapshot();
    println!(
        "[counters {phase}] compaction: input={} output={} metadata_avoided={} \
         trivial_moves={} l0_to_l1_ops={} flush_outputs={} zone_cuts={}",
        format_bytes(perf.lifecycle_compaction_input_bytes()),
        format_bytes(perf.lifecycle_compaction_output_bytes()),
        format_bytes(perf.lifecycle_compaction_metadata_bytes_avoided()),
        perf.lifecycle_compaction_trivial_moves(),
        perf.lifecycle_compaction_l0_to_level_one_operations(),
        perf.flush_zone_output_tables(),
        perf.flush_zone_cuts(),
    );
    println!(
        "[counters {phase}] reclaim: retention_runs={} sweep_runs={} \
         sweep_deferred_builds={} sweep_deferred_readers={} quarantined={} \
         quarantine_bytes={} purge_runs={} purged_bytes={} low_tier_runs={}",
        perf.table_object_retention_runs(),
        perf.table_object_sweep_runs(),
        perf.table_object_sweep_deferred_builds(),
        perf.table_object_sweep_deferred_readers(),
        perf.table_objects_quarantined(),
        format_bytes(perf.table_object_quarantine_bytes()),
        perf.quarantine_purge_runs(),
        format_bytes(perf.quarantine_purge_reclaimed_bytes()),
        perf.lifecycle_background_task_low_tier_runs(),
    );
}

fn logical_bytes(config: &Config) -> u64 {
    logical_bytes_at(config, config.commits)
}

fn logical_bytes_at(config: &Config, commits: usize) -> u64 {
    let per_id = config.page_bytes
        + match config.shape {
            Shape::PagesOnly => 0,
            _ => DEFAULT_META_BYTES,
        };
    (commits * config.batch * per_id) as u64
}

fn amp(on_disk: u64, logical: u64) -> f64 {
    if logical == 0 {
        return 0.0;
    }
    on_disk as f64 / logical as f64
}

fn page_key(id: u64) -> KvKey {
    let mut key = b"page/".to_vec();
    key.extend_from_slice(&id.to_be_bytes());
    KvKey::new(key).expect("valid page key")
}

fn meta_key(id: u64) -> KvKey {
    let mut key = b"meta/".to_vec();
    key.extend_from_slice(&id.to_be_bytes());
    KvKey::new(key).expect("valid meta key")
}

/// Incompressible page payload (unique per id): splitmix64 stream, the same
/// honesty rule the YCSB harness adopted in C1 — compressible filler would
/// understate on-disk bytes if a compressed format ever lands.
fn page_value(id: u64, len: usize) -> Vec<u8> {
    let mut state = id ^ 0x9E37_79B9_7F4A_7C15;
    let mut value = Vec::with_capacity(len);
    while value.len() < len {
        let word = splitmix64(&mut state);
        let take = usize::min(8, len - value.len());
        value.extend_from_slice(&word.to_le_bytes()[..take]);
    }
    value
}

fn meta_value(id: u64) -> Vec<u8> {
    let mut state = id ^ 0x2545_F491_4F6C_DD1D;
    let mut value = Vec::with_capacity(DEFAULT_META_BYTES);
    while value.len() < DEFAULT_META_BYTES {
        let word = splitmix64(&mut state);
        let take = usize::min(8, DEFAULT_META_BYTES - value.len());
        value.extend_from_slice(&word.to_le_bytes()[..take]);
    }
    value
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.2}{}", UNITS[unit])
    }
}
