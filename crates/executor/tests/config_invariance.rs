//! TCP4.5c — the config × capability × operation cross-product (STH-6
//! extended to the wire).
//!
//! The oracle: SEMANTICS ARE CONFIG-INVARIANT. One seeded operation script
//! spanning all five capabilities runs against every configuration in the
//! matrix — cache and durable-local, `Standard` and `Always` durability,
//! default and constrained memory budgets — and every read-back must be
//! identical across configurations after timestamp normalization. Storage
//! durability policy, cache-vs-durable persistence, and budget pressure are
//! allowed to change WHEN data becomes durable and HOW it is laid out —
//! never WHAT any read returns.
//!
//! This is the wire-level extension of the storage-level STH-6 config
//! differential (`testkit/config_differential.rs`), which compares raw KV
//! rows across 6 storage configs; here the full product surface (KV, JSON,
//! events, vectors, graph — including analytics) is compared as the SDK
//! sees it. Commit versions must match exactly across configs (one logical
//! history); wall-clock timestamps are normalized to zero by field name
//! before comparison, and nothing else is masked.
//!
//! Deterministic and seeded; `STRATA_CONFIG_OPS` scales the per-seed script
//! length for deep runs.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use strata_engine::{CacheOpenOptions, Database};
use strata_executor::{
    BatchEventEntry, BatchJsonEntry, BatchKvEntry, Bytes, Command, DurabilityMode,
    DurableLocalOpenOptions, EventRangeDirection, Executor, GraphDirection, VectorDistanceMetric,
};

/// The default "low" budget. Cache mode holds the whole live set in RAM and
/// refuses commits past the budget (`resource_exhausted.engine.persistence_budget`),
/// and the deep-tier scripts are append-heavy (events and graph edges stay
/// live forever) — so the deep soak must raise this in proportion to script
/// volume via `STRATA_CONFIG_LOW_BUDGET_MIB`, while keeping it far below the
/// default budget so pressure-driven maintenance still fires on every run.
const LOW_MEMORY_BUDGET_MIB_DEFAULT: usize = 64;

fn low_memory_budget_bytes() -> u64 {
    (env_usize(
        "STRATA_CONFIG_LOW_BUDGET_MIB",
        LOW_MEMORY_BUDGET_MIB_DEFAULT,
    ) as u64)
        << 20
}
/// One cross-config probe read per this many script ops.
const PROBE_EVERY: usize = 50;
const BASE_SEEDS: [u64; 2] = [0x00C0_FFEE, 0x5EED];

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(default)
}

/// The seed list: the two pinned base seeds, extended deterministically
/// (`SplitMix` over the index) when `STRATA_CONFIG_SEEDS` asks for more —
/// seed DIVERSITY (different op interleavings) finds config-conditional
/// behavior faster than longer scripts alone.
fn seeds(count: usize) -> Vec<u64> {
    let mut list: Vec<u64> = BASE_SEEDS.into_iter().collect();
    let mut derive = SplitMix64(0x5EED_FEED);
    while list.len() < count {
        list.push(derive.next());
    }
    list.truncate(count.max(1));
    list
}

/// The classic `SplitMix64` — deterministic, dependency-free.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

#[derive(Clone, Copy, Debug)]
enum ConfigCase {
    Cache { low_memory: bool },
    Durable { always: bool, low_memory: bool },
}

impl ConfigCase {
    fn label(self) -> String {
        match self {
            Self::Cache { low_memory } => format!("cache/low_mem={low_memory}"),
            Self::Durable { always, low_memory } => {
                format!("durable/always={always}/low_mem={low_memory}")
            }
        }
    }

    fn open(self, dir: &std::path::Path) -> Executor {
        match self {
            Self::Cache { low_memory } => {
                let mut options = CacheOpenOptions::new();
                if low_memory {
                    options = options.with_memory_budget(low_memory_budget_bytes());
                }
                let outcome = Database::open_cache(options).expect("cache opens");
                Executor::from_database(outcome.into_database())
            }
            Self::Durable { always, low_memory } => {
                let mut options = DurableLocalOpenOptions::new();
                if always {
                    options = options.with_durability(DurabilityMode::Always);
                }
                if low_memory {
                    options = options.with_memory_budget(low_memory_budget_bytes());
                }
                Executor::open_durable_local_with_options(dir.join("db"), options)
                    .expect("durable opens")
            }
        }
    }
}

const MATRIX: [ConfigCase; 6] = [
    ConfigCase::Cache { low_memory: false },
    ConfigCase::Cache { low_memory: true },
    ConfigCase::Durable {
        always: false,
        low_memory: false,
    },
    ConfigCase::Durable {
        always: false,
        low_memory: true,
    },
    ConfigCase::Durable {
        always: true,
        low_memory: false,
    },
    ConfigCase::Durable {
        always: true,
        low_memory: true,
    },
];

/// One generated command plus the number of row mutations it applies. The
/// generator is a pure function of the rng, so every config replays the
/// identical script. Batch arms carry up to hundreds of mutations per
/// commit — the lever that takes deep runs into eight-figure mutation
/// counts without an fsync per mutation (and the batch surfaces are
/// coverage in their own right).
fn generated_op(rng: &mut SplitMix64) -> (Command, u64) {
    let key_space = 12;
    match rng.below(16) {
        0..=2 => {
            // One put in eight carries a large (8..40 KiB) value so deep
            // runs cross flush/block boundaries; the fill byte keeps it
            // deterministic.
            let value = if rng.below(8) == 0 {
                let len = 8_192 + usize::try_from(rng.below(32_768)).expect("small");
                let fill = u8::try_from(rng.below(256)).expect("byte");
                vec![fill; len]
            } else {
                format!("value-{}", rng.below(1000)).into_bytes()
            };
            (
                Command::KvPut {
                    branch: None,
                    space: None,
                    key: Bytes::from(format!("k{}", rng.below(key_space)).as_str()),
                    value: Bytes::from(value),
                },
                1,
            )
        }
        3 => (
            Command::KvDelete {
                branch: None,
                space: None,
                key: Bytes::from(format!("k{}", rng.below(key_space)).as_str()),
            },
            1,
        ),
        4..=5 => (
            Command::JsonSet {
                branch: None,
                space: None,
                key: format!("d{}", rng.below(6)),
                path: "$".to_owned(),
                value: json!({"a": rng.below(100), "nested": {"b": [rng.below(9), rng.below(9)]}}),
            },
            1,
        ),
        6 => (
            Command::EventAppend {
                branch: None,
                space: None,
                event_type: format!("t{}", rng.below(3)),
                payload: json!({"seq_hint": rng.below(10_000)}),
            },
            1,
        ),
        7..=8 => (
            Command::VectorUpsert {
                branch: None,
                space: None,
                collection: "vecs".to_owned(),
                key: format!("v{}", rng.below(8)),
                vector: (0..8)
                    .map(|_| f64::from(u32::try_from(rng.below(200)).expect("small")) / 10.0)
                    .collect(),
                metadata: Some(json!({"tag": format!("g{}", rng.below(4))})),
            },
            1,
        ),
        9 => (
            Command::GraphAddEdge {
                branch: None,
                space: None,
                graph: "net".to_owned(),
                src: format!("n{}", rng.below(6)),
                edge_type: "e".to_owned(),
                dst: format!("n{}", rng.below(6)),
                weight: Some(1.0 + f64::from(u32::try_from(rng.below(40)).expect("small")) / 10.0),
                properties: None,
            },
            1,
        ),
        // Batch arms: up to hundreds of mutations per commit over a WIDE
        // key space so batches create and rewrite rows the point ops never
        // touch — and carry the mutation volume that takes the nightly into
        // nine figures.
        10..=12 => generated_kv_batch(rng),
        13..=14 => generated_json_batch(rng),
        _ => generated_event_batch(rng),
    }
}

/// KV batch arm: up to 500 rows per commit. Batches refuse duplicate keys,
/// so draws dedupe (last wins) via an ordered map — still a pure function
/// of the rng.
fn generated_kv_batch(rng: &mut SplitMix64) -> (Command, u64) {
    let mut drawn = std::collections::BTreeMap::new();
    for _ in 0..2 + rng.below(499) {
        drawn.insert(rng.below(16_384), rng.below(100_000));
    }
    let entries: Vec<BatchKvEntry> = drawn
        .into_iter()
        .map(|(key, value)| {
            BatchKvEntry::new(
                Bytes::from(format!("bk{key}").as_str()),
                Bytes::from(format!("bv-{value}").as_str()),
            )
        })
        .collect();
    let count = entries.len() as u64;
    (
        Command::KvBatchPut {
            branch: None,
            space: None,
            entries,
        },
        count,
    )
}

fn generated_json_batch(rng: &mut SplitMix64) -> (Command, u64) {
    let mut drawn = std::collections::BTreeMap::new();
    for _ in 0..2 + rng.below(249) {
        drawn.insert(rng.below(4096), rng.below(100_000));
    }
    let entries: Vec<BatchJsonEntry> = drawn
        .into_iter()
        .map(|(key, value)| BatchJsonEntry::new(format!("bd{key}"), "$", json!({"i": value})))
        .collect();
    let count = entries.len() as u64;
    (
        Command::JsonBatchSet {
            branch: None,
            space: None,
            entries,
        },
        count,
    )
}

fn generated_event_batch(rng: &mut SplitMix64) -> (Command, u64) {
    let entries: Vec<BatchEventEntry> = (0..2 + rng.below(249))
        .map(|_| {
            BatchEventEntry::new(
                format!("t{}", rng.below(3)),
                json!({"b": rng.below(100_000)}),
            )
        })
        .collect();
    let count = entries.len() as u64;
    (
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries,
        },
        count,
    )
}

/// Fixtures every script assumes: the vector collection and graph nodes.
fn seed_fixtures(executor: &mut Executor) {
    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "vecs".to_owned(),
            dimension: 8,
            metric: VectorDistanceMetric::Cosine,
        })
        .expect("collection creates");
    executor
        .execute(Command::GraphCreate {
            branch: None,
            space: None,
            graph: "net".to_owned(),
        })
        .expect("graph creates");
    for node in 0..6 {
        executor
            .execute(Command::GraphAddNode {
                branch: None,
                space: None,
                graph: "net".to_owned(),
                node_id: format!("n{node}"),
                properties: None,
                binding: None,
                object_type: None,
            })
            .expect("node adds");
    }
}

/// The deterministic read sweep whose outputs are compared across configs.
fn read_sweep() -> Vec<(String, Command)> {
    let mut sweep = Vec::new();
    for key in 0..12 {
        sweep.push((
            format!("kv_get k{key}"),
            Command::KvGet {
                branch: None,
                space: None,
                key: Bytes::from(format!("k{key}").as_str()),
                as_of: None,
                as_of_time: None,
            },
        ));
    }
    sweep.push((
        "kv_list".to_owned(),
        Command::KvList {
            branch: None,
            space: None,
            prefix: None,
            cursor: None,
            limit: Some(100),
            as_of: None,
        },
    ));
    sweep.push((
        "kv_count".to_owned(),
        Command::KvCount {
            branch: None,
            space: None,
            prefix: None,
            as_of: None,
        },
    ));
    for key in ["bk17", "bk255", "bk1023", "bk4095"] {
        sweep.push((
            format!("kv_get {key}"),
            Command::KvGet {
                branch: None,
                space: None,
                key: Bytes::from(key),
                as_of: None,
                as_of_time: None,
            },
        ));
    }
    for doc in ["bd7", "bd511", "bd1023"] {
        sweep.push((
            format!("json_get {doc}"),
            Command::JsonGet {
                branch: None,
                space: None,
                key: (*doc).to_owned(),
                path: "$".to_owned(),
                as_of: None,
            },
        ));
    }
    sweep.push((
        "event_count".to_owned(),
        Command::EventCount {
            branch: None,
            space: None,
            as_of: None,
        },
    ));
    for doc in 0..6 {
        sweep.push((
            format!("json_get d{doc}"),
            Command::JsonGet {
                branch: None,
                space: None,
                key: format!("d{doc}"),
                path: "$".to_owned(),
                as_of: None,
            },
        ));
    }
    for event_type in 0..3 {
        sweep.push((
            format!("event_range t{event_type}"),
            Command::EventRange {
                branch: None,
                space: None,
                start_seq: 0,
                end_seq: None,
                limit: Some(50),
                direction: EventRangeDirection::Forward,
                event_type: Some(format!("t{event_type}")),
            },
        ));
    }
    sweep.extend(vector_and_graph_sweep());
    sweep
}

fn vector_and_graph_sweep() -> Vec<(String, Command)> {
    let mut sweep = Vec::new();
    for key in 0..8 {
        sweep.push((
            format!("vector_get v{key}"),
            Command::VectorGet {
                branch: None,
                space: None,
                collection: "vecs".to_owned(),
                key: format!("v{key}"),
                as_of: None,
            },
        ));
    }
    sweep.push((
        "vector_query".to_owned(),
        Command::VectorQuery {
            branch: None,
            space: None,
            collection: "vecs".to_owned(),
            query: vec![5.0; 8],
            k: 4,
            filter: None,
            as_of: None,
        },
    ));
    for node in 0..6 {
        sweep.push((
            format!("graph_neighbors n{node}"),
            Command::GraphNeighbors {
                branch: None,
                space: None,
                graph: "net".to_owned(),
                node_id: format!("n{node}"),
                direction: GraphDirection::Both,
                edge_type: None,
                cursor: None,
                limit: Some(50),
                as_of: None,
            },
        ));
    }
    sweep.push((
        "graph_wcc".to_owned(),
        Command::GraphWcc {
            branch: None,
            space: None,
            graph: "net".to_owned(),
            budget: None,
            as_of: None,
        },
    ));
    sweep.push((
        "graph_pagerank".to_owned(),
        Command::GraphPagerank {
            branch: None,
            space: None,
            graph: "net".to_owned(),
            damping: None,
            max_iterations: None,
            tolerance: None,
            personalization: None,
            budget: None,
            as_of: None,
        },
    ));
    sweep
}

/// Zeroes the run-nondeterministic fields by name, everywhere in the tree:
/// wall-clock timestamps, and the event chain hashes — `compute_event_hash`
/// commits to `timestamp_micros` BY DESIGN (the hash attests the recorded
/// time), so `hash`/`previous_hash` differ between any two runs, same
/// config or not. Nothing else is masked: versions, sequences, cursors,
/// counts, and payloads must all match across configurations.
fn normalize(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                let is_wall_clock = key == "timestamp"
                    || key == "timestamp_micros"
                    || key.ends_with("_at")
                    || key.ends_with("_at_micros");
                let is_time_committed_hash = (key == "hash" || key == "previous_hash")
                    && entry.as_str().is_some_and(|hash| {
                        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
                    });
                if is_wall_clock && entry.is_number() {
                    *entry = Value::from(0u64);
                } else if is_time_committed_hash {
                    *entry = Value::from("0".repeat(64));
                } else {
                    normalize(entry);
                }
            }
        }
        Value::Array(entries) => {
            for entry in entries {
                normalize(entry);
            }
        }
        _ => {}
    }
}

fn run_config(case: ConfigCase, seed: u64, ops: usize) -> BTreeMap<String, Value> {
    let dir = tempfile::tempdir().expect("tmp");
    let mut executor = case.open(dir.path());
    seed_fixtures(&mut executor);

    let mut rng = SplitMix64(seed);
    let mut outputs = BTreeMap::new();
    let mut mutations = 0u64;
    for index in 0..ops {
        let (op, count) = generated_op(&mut rng);
        executor.execute(op.clone()).unwrap_or_else(|err| {
            panic!(
                "{}: generated op {index} refused (script must succeed on \
                 every config): {op:?}: {err:?}",
                case.label()
            )
        });
        mutations += count;
        // Mid-script probes: intermediate states are compared across
        // configs too, so a divergence localizes to the op window that
        // introduced it instead of hiding until the final sweep.
        if index % PROBE_EVERY == PROBE_EVERY - 1 {
            let key = format!("bk{}", rng.below(16_384));
            let output = executor
                .execute(Command::KvGet {
                    branch: None,
                    space: None,
                    key: Bytes::from(key.as_str()),
                    as_of: None,
                    as_of_time: None,
                })
                .unwrap_or_else(|err| panic!("{}: probe read refused: {err:?}", case.label()));
            let mut value = serde_json::to_value(&output).expect("probe serializes");
            normalize(&mut value);
            outputs.insert(format!("probe@{index} {key}"), value);
        }
    }
    outputs.insert("applied_mutations".to_owned(), Value::from(mutations));
    for (label, command) in read_sweep() {
        let output = executor
            .execute(command)
            .unwrap_or_else(|err| panic!("{}: read {label} refused: {err:?}", case.label()));
        let mut value = serde_json::to_value(&output).expect("output serializes");
        normalize(&mut value);
        outputs.insert(label, value);
    }
    // Quiesce background maintenance before the tempdir is removed: a bare
    // `drop` only joins workers for 250ms and then DETACHES stragglers, which
    // then read table objects out of the just-deleted tempdir.
    executor.close().expect("executor closes cleanly");
    outputs
}

fn run_matrix(seed_count: usize, ops: usize) -> u64 {
    let mut total_mutations = 0u64;
    for seed in seeds(seed_count) {
        let reference = run_config(MATRIX[0], seed, ops);
        assert!(
            reference.values().any(|value| *value != Value::Null),
            "seed {seed:#x}: the read sweep must observe real data"
        );
        total_mutations += MATRIX.len() as u64
            * reference
                .get("applied_mutations")
                .and_then(Value::as_u64)
                .expect("mutation count recorded");
        for case in &MATRIX[1..] {
            let observed = run_config(*case, seed, ops);
            assert_eq!(
                observed.len(),
                reference.len(),
                "{}: sweep shape diverged",
                case.label()
            );
            for (label, expected) in &reference {
                assert_eq!(
                    observed.get(label),
                    Some(expected),
                    "seed {seed:#x}, {}: `{label}` diverged from the cache/default \
                     reference — semantics leaked a configuration",
                    case.label()
                );
            }
        }
    }
    total_mutations
}

#[test]
fn semantics_are_config_invariant_across_the_matrix() {
    run_matrix(
        env_usize("STRATA_CONFIG_SEEDS", 2),
        env_usize("STRATA_CONFIG_OPS", 120),
    );
}

/// The nightly soak tier: many seeds × long scripts (the env knobs), so the
/// cross-product crosses flush and block boundaries and rare op mixes the
/// per-PR shape cannot reach (deep BUDGET-pressure equivalence is the
/// storage STH-6 lane's job). A failure names its seed.
///
/// The defaults are the nightly shape: 48 seeds × 5000 ops × 6 configs at
/// ~70 mutations/op ≈ 1.0e8 applied mutations, ~60 min in release at 28K
/// mutations/sec (measured 2026-09-02). Requires
/// `STRATA_CONFIG_LOW_BUDGET_MIB=256` — deep append-heavy scripts grow the
/// live set past the 64MiB default and cache mode refuses commits past its
/// budget by contract. The manual monster tier is the same run at
/// `STRATA_CONFIG_SEEDS=480` / `STRATA_CONFIG_MUTATION_FLOOR=1000000000`
/// for ~1.0e9 mutations in ~10 h.
#[test]
#[ignore = "soak tier — run via the nightly config-invariance step"]
fn config_invariance_soak() {
    // Secondary oracle (debug-assert tier): a background maintenance thread
    // that panics — a real object-lifetime regression, or the teardown race
    // where a not-yet-quiesced worker reads a removed table object — is
    // otherwise SWALLOWED, because a panic on a detached worker thread does
    // not fail the test process. Count those panics via the panic hook and
    // assert none fired, so the soak goes red on a background fault instead
    // of reporting a false `ok`. `run_config` closes every executor before
    // its tempdir is dropped, so a clean run counts zero.
    let bg_panics = install_background_panic_counter();

    let mutations = run_matrix(
        env_usize("STRATA_CONFIG_SEEDS", 48),
        env_usize("STRATA_CONFIG_OPS", 5000),
    );
    eprintln!("config-invariance soak applied {mutations} mutations across the matrix");
    let floor = env_usize("STRATA_CONFIG_MUTATION_FLOOR", 100_000_000) as u64;
    assert!(
        mutations >= floor,
        "soak applied {mutations} mutations — below the {floor} floor; raise \
         seeds/ops or lower STRATA_CONFIG_MUTATION_FLOOR deliberately"
    );

    let panics = bg_panics.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        panics, 0,
        "{panics} background maintenance thread(s) panicked during the soak — \
         a swallowed background fault (see stderr for the panic site)"
    );
}

/// Install a panic hook that counts panics originating on storage background
/// maintenance threads (`strata-storage-maint-bg-*`), chaining to the prior
/// hook so the panic still prints. Returns the shared counter. The soak runs
/// `--ignored` in isolation, so the process-global hook affects no other test.
fn install_background_panic_counter() -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let counter = Arc::new(AtomicUsize::new(0));
    let hook_counter = Arc::clone(&counter);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current()
            .name()
            .is_some_and(|name| name.contains("maint-bg"))
        {
            hook_counter.fetch_add(1, Ordering::SeqCst);
        }
        previous(info);
    }));
    counter
}

/// Deterministic proof the soak's secondary oracle is not vacuous: a panic on
/// a `maint-bg`-named thread is counted, and a panic on any other thread is
/// not. Guards against a future refactor (thread-name change, hook removal)
/// silently turning the soak's background-fault detection into a no-op — the
/// teardown race that motivated the oracle is timing-dependent and does not
/// reproduce on every run, so a live-fault test would be flaky.
#[test]
fn background_panic_counter_counts_only_maintenance_threads() {
    use std::sync::atomic::Ordering;

    let counter = install_background_panic_counter();

    // A panic on a maintenance-named thread is counted.
    let maint = std::thread::Builder::new()
        .name("strata-storage-maint-bg-9".to_owned())
        .spawn(|| panic!("simulated background maintenance fault"))
        .expect("spawn maint thread");
    assert!(maint.join().is_err(), "the maint thread must have panicked");

    // A panic on an unrelated thread is not counted (direction control).
    let other = std::thread::Builder::new()
        .name("unrelated-worker".to_owned())
        .spawn(|| panic!("unrelated panic"))
        .expect("spawn other thread");
    assert!(other.join().is_err(), "the other thread must have panicked");

    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "the counter must catch the maint-bg panic and ignore the unrelated one"
    );
}
