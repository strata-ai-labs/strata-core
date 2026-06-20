use std::collections::BTreeSet;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use strata_engine_next::testkit::{VectorIndexDiagnostics, VectorIndexPolicy};
use strata_engine_next::{
    BranchName, CacheOpenOptions, Database, DatabaseOpenOutcome, DurableLocalOpenOptions,
    EngineResult, ProductSpace, VectorCollectionName, VectorConfig, VectorDistanceMetric,
    VectorEmbedding, VectorKey, VectorMetadata, VectorMetadataPatch, VectorSearchResult,
    VectorUpsertEntry,
};
use tempfile::TempDir;

const DEFAULT_COLLECTION_SIZE: usize = 10_000;
const DEFAULT_DIMENSION: usize = 64;
const DEFAULT_K: usize = 10;
const DEFAULT_QUERY_SAMPLES: usize = 200;
const DEFAULT_BATCH_SIZE: usize = 512;
const DEFAULT_BRANCH_WRITES: usize = 512;
const DEFAULT_MUTATION_COUNT: usize = 512;

fn main() -> EngineResult<()> {
    let config = BenchConfig::from_env()?;
    let report = run(config)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("benchmark report serializes")
    );
    Ok(())
}

#[derive(Clone, Debug)]
struct BenchConfig {
    collection_size: usize,
    dimension: usize,
    k: usize,
    query_samples: usize,
    batch_size: usize,
    branch_writes: usize,
    mutation_count: usize,
    workload: WorkloadSelection,
    mode: OpenMode,
    metric: MetricSelection,
    durable_path: Option<PathBuf>,
}

impl BenchConfig {
    fn from_env() -> EngineResult<Self> {
        let mut config = Self {
            collection_size: DEFAULT_COLLECTION_SIZE,
            dimension: DEFAULT_DIMENSION,
            k: DEFAULT_K,
            query_samples: DEFAULT_QUERY_SAMPLES,
            batch_size: DEFAULT_BATCH_SIZE,
            branch_writes: DEFAULT_BRANCH_WRITES,
            mutation_count: DEFAULT_MUTATION_COUNT,
            workload: WorkloadSelection::All,
            mode: OpenMode::Cache,
            metric: MetricSelection::Cosine,
            durable_path: None,
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--records" => {
                    config.collection_size = parse_usize_arg(&arg, args.next())?;
                }
                "--dimension" => {
                    config.dimension = parse_usize_arg(&arg, args.next())?;
                }
                "--k" => {
                    config.k = parse_usize_arg(&arg, args.next())?;
                }
                "--samples" => {
                    config.query_samples = parse_usize_arg(&arg, args.next())?;
                }
                "--batch-size" => {
                    config.batch_size = parse_usize_arg(&arg, args.next())?;
                }
                "--branch-writes" => {
                    config.branch_writes = parse_usize_arg(&arg, args.next())?;
                }
                "--mutations" => {
                    config.mutation_count = parse_usize_arg(&arg, args.next())?;
                }
                "--workload" => {
                    let Some(value) = args.next() else {
                        return Err(strata_engine_next::EngineError::invalid_input(
                            "invalid_argument.benchmark.workload",
                            "--workload requires all, exact, flat, hnsw, write, mutation, branch, or reopen",
                        ));
                    };
                    config.workload = WorkloadSelection::parse(&value)?;
                }
                "--mode" => {
                    config.mode = match args.next().as_deref() {
                        Some("cache") => OpenMode::Cache,
                        Some("durable") => OpenMode::Durable,
                        Some(value) => {
                            return Err(strata_engine_next::EngineError::invalid_input(
                                "invalid_argument.benchmark.mode",
                                format!("unknown mode `{value}`, expected cache or durable"),
                            ));
                        }
                        None => {
                            return Err(strata_engine_next::EngineError::invalid_input(
                                "invalid_argument.benchmark.mode",
                                "--mode requires cache or durable",
                            ));
                        }
                    };
                }
                "--metric" => {
                    let Some(value) = args.next() else {
                        return Err(strata_engine_next::EngineError::invalid_input(
                            "invalid_argument.benchmark.metric",
                            "--metric requires cosine, euclidean, dot_product, or all",
                        ));
                    };
                    config.metric = MetricSelection::parse(&value)?;
                }
                "--path" => {
                    let Some(path) = args.next() else {
                        return Err(strata_engine_next::EngineError::invalid_input(
                            "invalid_argument.benchmark.path",
                            "--path requires a filesystem path",
                        ));
                    };
                    config.durable_path = Some(PathBuf::from(path));
                    config.mode = OpenMode::Durable;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                value => {
                    return Err(strata_engine_next::EngineError::invalid_input(
                        "invalid_argument.benchmark.arg",
                        format!("unknown argument `{value}`"),
                    ));
                }
            }
        }
        if config.collection_size == 0 || config.dimension == 0 || config.k == 0 {
            return Err(strata_engine_next::EngineError::invalid_input(
                "invalid_argument.benchmark.config",
                "records, dimension, and k must be greater than zero",
            ));
        }
        if config.query_samples == 0 || config.batch_size == 0 {
            return Err(strata_engine_next::EngineError::invalid_input(
                "invalid_argument.benchmark.config",
                "samples and batch size must be greater than zero",
            ));
        }
        Ok(config)
    }

    fn serializable(&self) -> SerializableBenchConfig {
        SerializableBenchConfig {
            collection_size: self.collection_size,
            dimension: self.dimension,
            k: self.k,
            query_samples: self.query_samples,
            batch_size: self.batch_size,
            branch_writes: self.branch_writes,
            mutation_count: self.mutation_count,
            workload: self.workload.as_str(),
            mode: self.mode.as_str(),
            metric: self.metric.as_str(),
            durable_path: self
                .durable_path
                .as_ref()
                .map(|path| path.display().to_string()),
        }
    }

    fn with_metric(&self, metric: VectorDistanceMetric) -> Self {
        Self {
            collection_size: self.collection_size,
            dimension: self.dimension,
            k: self.k,
            query_samples: self.query_samples,
            batch_size: self.batch_size,
            branch_writes: self.branch_writes,
            mutation_count: self.mutation_count,
            workload: self.workload,
            mode: self.mode,
            metric: MetricSelection::from_metric(metric),
            durable_path: self.durable_path.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum OpenMode {
    Cache,
    Durable,
}

impl OpenMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Durable => "durable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetricSelection {
    Cosine,
    Euclidean,
    DotProduct,
    All,
}

impl MetricSelection {
    fn parse(value: &str) -> EngineResult<Self> {
        match value {
            "cosine" => Ok(Self::Cosine),
            "euclidean" => Ok(Self::Euclidean),
            "dot" | "dot_product" => Ok(Self::DotProduct),
            "all" => Ok(Self::All),
            _ => Err(strata_engine_next::EngineError::invalid_input(
                "invalid_argument.benchmark.metric",
                format!(
                    "unknown metric `{value}`, expected cosine, euclidean, dot_product, or all"
                ),
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Cosine => "cosine",
            Self::Euclidean => "euclidean",
            Self::DotProduct => "dot_product",
            Self::All => "all",
        }
    }

    fn selected_metrics(self) -> &'static [VectorDistanceMetric] {
        match self {
            Self::Cosine => &[VectorDistanceMetric::Cosine],
            Self::Euclidean => &[VectorDistanceMetric::Euclidean],
            Self::DotProduct => &[VectorDistanceMetric::DotProduct],
            Self::All => &[
                VectorDistanceMetric::Cosine,
                VectorDistanceMetric::Euclidean,
                VectorDistanceMetric::DotProduct,
            ],
        }
    }

    const fn from_metric(metric: VectorDistanceMetric) -> Self {
        match metric {
            VectorDistanceMetric::Cosine => Self::Cosine,
            VectorDistanceMetric::Euclidean => Self::Euclidean,
            VectorDistanceMetric::DotProduct => Self::DotProduct,
        }
    }

    const fn concrete_metric(self) -> VectorDistanceMetric {
        match self {
            Self::Cosine | Self::All => VectorDistanceMetric::Cosine,
            Self::Euclidean => VectorDistanceMetric::Euclidean,
            Self::DotProduct => VectorDistanceMetric::DotProduct,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkloadSelection {
    All,
    Exact,
    Flat,
    Hnsw,
    Write,
    Mutation,
    Branch,
    Reopen,
}

impl WorkloadSelection {
    fn parse(value: &str) -> EngineResult<Self> {
        match value {
            "all" => Ok(Self::All),
            "exact" => Ok(Self::Exact),
            "flat" => Ok(Self::Flat),
            "hnsw" => Ok(Self::Hnsw),
            "write" => Ok(Self::Write),
            "mutation" => Ok(Self::Mutation),
            "branch" => Ok(Self::Branch),
            "reopen" => Ok(Self::Reopen),
            _ => Err(strata_engine_next::EngineError::invalid_input(
                "invalid_argument.benchmark.workload",
                format!(
                    "unknown workload `{value}`, expected all, exact, flat, hnsw, write, mutation, branch, or reopen"
                ),
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Exact => "exact",
            Self::Flat => "flat",
            Self::Hnsw => "hnsw",
            Self::Write => "write",
            Self::Mutation => "mutation",
            Self::Branch => "branch",
            Self::Reopen => "reopen",
        }
    }
}

#[derive(Serialize)]
struct SerializableBenchConfig {
    collection_size: usize,
    dimension: usize,
    k: usize,
    query_samples: usize,
    batch_size: usize,
    branch_writes: usize,
    mutation_count: usize,
    workload: &'static str,
    mode: &'static str,
    metric: &'static str,
    durable_path: Option<String>,
}

#[derive(Serialize)]
struct BenchmarkReport {
    config: SerializableBenchConfig,
    results: Vec<BenchmarkRow>,
}

#[derive(Serialize)]
struct BenchmarkRow {
    workload: &'static str,
    collection_size: usize,
    dimension: usize,
    metric: &'static str,
    runtime_mode: &'static str,
    index_policy: String,
    index_kind_used: String,
    hnsw_memory_budget_bytes: usize,
    manifest_status: String,
    manifest_ref_count: usize,
    inherited_ref_count: usize,
    owned_ref_count: usize,
    active_delta_count: usize,
    indexed_source_count: usize,
    exact_source_count: usize,
    flat_source_count: usize,
    hnsw_source_count: usize,
    active_delta_source_count: usize,
    indexed_vector_count: usize,
    exact_fallback_count: u64,
    build_time_ns: Option<u128>,
    derived_bytes: u64,
    recall_at_k: Option<f64>,
    query_p50_ns: Option<u128>,
    query_p95_ns: Option<u128>,
    query_p99_ns: Option<u128>,
    write_throughput_ops_per_sec: Option<f64>,
    write_p50_ns: Option<u128>,
    write_p95_ns: Option<u128>,
    write_p99_ns: Option<u128>,
    artifact_missing_count: usize,
    artifact_stale_count: usize,
    artifact_corrupt_count: usize,
    artifact_over_budget_count: usize,
    artifact_sources: Vec<ArtifactSourceRow>,
}

#[derive(Serialize)]
struct ArtifactSourceRow {
    artifact_id: String,
    status: String,
    searched: bool,
}

struct OpenedDatabase {
    database: Database,
    _temp_dir: Option<TempDir>,
}

fn run(config: BenchConfig) -> EngineResult<BenchmarkReport> {
    let mut results = Vec::new();
    for metric in config.metric.selected_metrics() {
        let metric_config = config.with_metric(*metric);
        let queries = query_vectors(&metric_config)?;
        run_selected_workloads(&metric_config, &queries, &mut results)?;
    }

    Ok(BenchmarkReport {
        config: config.serializable(),
        results,
    })
}

fn run_selected_workloads(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
    results: &mut Vec<BenchmarkRow>,
) -> EngineResult<()> {
    match config.workload {
        WorkloadSelection::All => {
            results.push(run_query_workload(
                config,
                queries,
                "exact_scan",
                VectorIndexPolicy::exact_only_for_test(),
            )?);
            results.push(run_query_workload(
                config,
                queries,
                "flat_index_query",
                VectorIndexPolicy::flat_only_for_test(),
            )?);
            results.push(run_query_workload(
                config,
                queries,
                "hnsw_index_query",
                VectorIndexPolicy::hnsw_only_for_test(),
            )?);
            results.push(run_with_progress("write_with_index_enabled", || {
                run_write_scenario(config, queries)
            })?);
            results.push(run_with_progress("delete_update_workload", || {
                run_mutation_scenario(config, queries)
            })?);
            results.push(run_with_progress("branch_fork_search", || {
                run_branch_scenario(config, queries)
            })?);
            results.push(run_with_progress("durable_reopen_with_artifacts", || {
                run_durable_reopen_scenario(config, queries)
            })?);
        }
        WorkloadSelection::Exact => {
            results.push(run_query_workload(
                config,
                queries,
                "exact_scan",
                VectorIndexPolicy::exact_only_for_test(),
            )?);
        }
        WorkloadSelection::Flat => {
            results.push(run_query_workload(
                config,
                queries,
                "flat_index_query",
                VectorIndexPolicy::flat_only_for_test(),
            )?);
        }
        WorkloadSelection::Hnsw => {
            results.push(run_query_workload(
                config,
                queries,
                "hnsw_index_query",
                VectorIndexPolicy::hnsw_only_for_test(),
            )?);
        }
        WorkloadSelection::Write => {
            results.push(run_with_progress("write_with_index_enabled", || {
                run_write_scenario(config, queries)
            })?);
        }
        WorkloadSelection::Mutation => {
            results.push(run_with_progress("delete_update_workload", || {
                run_mutation_scenario(config, queries)
            })?);
        }
        WorkloadSelection::Branch => {
            results.push(run_with_progress("branch_fork_search", || {
                run_branch_scenario(config, queries)
            })?);
        }
        WorkloadSelection::Reopen => {
            results.push(run_with_progress("durable_reopen_with_artifacts", || {
                run_durable_reopen_scenario(config, queries)
            })?);
        }
    }
    Ok(())
}

fn run_query_workload(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
    workload: &'static str,
    policy: VectorIndexPolicy,
) -> EngineResult<BenchmarkRow> {
    run_with_progress(workload, || run_query_scenario(config, queries, workload, policy))
}

fn run_with_progress<T>(
    workload: &'static str,
    run_workload: impl FnOnce() -> EngineResult<T>,
) -> EngineResult<T> {
    eprintln!("benchmark progress: start {workload}");
    let started_at = Instant::now();
    let result = run_workload();
    let elapsed = started_at.elapsed();
    match &result {
        Ok(_) => eprintln!(
            "benchmark progress: finish {workload} elapsed_ms={}",
            elapsed.as_millis()
        ),
        Err(error) => eprintln!(
            "benchmark progress: fail {workload} elapsed_ms={} error={error}",
            elapsed.as_millis()
        ),
    }
    result
}

fn run_query_scenario(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
    workload: &'static str,
    policy: VectorIndexPolicy,
) -> EngineResult<BenchmarkRow> {
    let mut opened = open_bench_database(config, workload)?;
    let collection = collection();
    let load_throughput = load_dataset(
        &mut opened.database,
        &collection,
        0,
        config.collection_size,
        config.dimension,
        config.metric.concrete_metric(),
        config.batch_size,
        "base",
    )?;
    let exact = exact_keys(&mut opened.database, &collection, queries, config.k)?;
    measure_query_policy(
        &mut opened.database,
        workload,
        &collection,
        queries,
        &exact,
        config,
        policy,
        Some(load_throughput),
    )
}

fn run_write_scenario(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
) -> EngineResult<BenchmarkRow> {
    let mut opened = open_bench_database(config, "write_with_index_enabled")?;
    let collection = collection();
    load_dataset(
        &mut opened.database,
        &collection,
        0,
        config.collection_size,
        config.dimension,
        config.metric.concrete_metric(),
        config.batch_size,
        "base",
    )?;
    build_policy_once(
        &mut opened.database,
        &collection,
        &queries[0],
        config.k,
        VectorIndexPolicy::hnsw_only_for_test(),
    )?;

    let append_count = config.batch_size.max(config.mutation_count);
    let (write_samples, elapsed) = measure_batch_writes(
        &mut opened.database,
        &collection,
        config.collection_size,
        append_count,
        config.dimension,
        config.batch_size,
        "append",
    )?;
    let exact = exact_keys(&mut opened.database, &collection, queries, config.k)?;
    let mut row = measure_query_policy(
        &mut opened.database,
        "write_with_index_enabled",
        &collection,
        queries,
        &exact,
        config,
        VectorIndexPolicy::hnsw_only_for_test(),
        Some(ops_per_second(append_count, elapsed)),
    )?;
    row.write_p50_ns = Some(write_samples.p50.as_nanos());
    row.write_p95_ns = Some(write_samples.p95.as_nanos());
    row.write_p99_ns = Some(write_samples.p99.as_nanos());
    Ok(row)
}

fn run_mutation_scenario(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
) -> EngineResult<BenchmarkRow> {
    let mut opened = open_bench_database(config, "delete_update_workload")?;
    let collection = collection();
    load_dataset(
        &mut opened.database,
        &collection,
        0,
        config.collection_size,
        config.dimension,
        config.metric.concrete_metric(),
        config.batch_size,
        "base",
    )?;
    build_policy_once(
        &mut opened.database,
        &collection,
        &queries[0],
        config.k,
        VectorIndexPolicy::hnsw_only_for_test(),
    )?;

    let mutation_count = config.mutation_count.min(config.collection_size);
    let start = Instant::now();
    {
        let mut vectors = vector_service(&mut opened.database, "default")?;
        for index in 0..mutation_count {
            let patch = VectorMetadataPatch::new(json!({
                "phase": "updated",
                "updated": true,
            }))?;
            vectors.update_metadata(&collection, vector_key(index)?, &patch)?;
        }
        let delete_start = config.collection_size.saturating_sub(mutation_count);
        let keys = (delete_start..config.collection_size)
            .map(vector_key)
            .collect::<EngineResult<Vec<_>>>()?;
        vectors.batch_delete(&collection, &keys)?;
    }
    let mutation_elapsed = start.elapsed();
    let exact = exact_keys(&mut opened.database, &collection, queries, config.k)?;
    measure_query_policy(
        &mut opened.database,
        "delete_update_workload",
        &collection,
        queries,
        &exact,
        config,
        VectorIndexPolicy::hnsw_only_for_test(),
        Some(ops_per_second(mutation_count.saturating_mul(2), mutation_elapsed)),
    )
}

fn run_branch_scenario(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
) -> EngineResult<BenchmarkRow> {
    let mut opened = open_bench_database(config, "branch_fork_search")?;
    let collection = collection();
    load_dataset(
        &mut opened.database,
        &collection,
        0,
        config.collection_size,
        config.dimension,
        config.metric.concrete_metric(),
        config.batch_size,
        "base",
    )?;
    build_policy_once(
        &mut opened.database,
        &collection,
        &queries[0],
        config.k,
        VectorIndexPolicy::hnsw_only_for_test(),
    )?;
    opened
        .database
        .branches()?
        .fork_current(&branch("default")?, branch("feature")?)?;
    let branch_write_count = config.branch_writes.max(1);
    let start = Instant::now();
    {
        let mut vectors = vector_service(&mut opened.database, "feature")?;
        let entries = (0..branch_write_count)
            .map(|offset| {
                upsert_entry(
                    config.collection_size + offset,
                    config.dimension,
                    "feature",
                )
            })
            .collect::<EngineResult<Vec<_>>>()?;
        vectors.batch_upsert(&collection, &entries)?;
    }
    let write_throughput = ops_per_second(branch_write_count, start.elapsed());
    let exact = exact_keys_on_branch(
        &mut opened.database,
        "feature",
        &collection,
        queries,
        config.k,
    )?;
    measure_query_policy_on_branch(
        &mut opened.database,
        "branch_fork_search",
        "feature",
        &collection,
        queries,
        &exact,
        config.collection_size + branch_write_count,
        config,
        VectorIndexPolicy::hnsw_only_for_test(),
        Some(write_throughput),
    )
}

fn run_durable_reopen_scenario(
    config: &BenchConfig,
    queries: &[VectorEmbedding],
) -> EngineResult<BenchmarkRow> {
    let temp_dir = if config.durable_path.is_none() {
        Some(TempDir::new().map_err(io_engine_error)?)
    } else {
        None
    };
    let path = config
        .durable_path
        .as_ref()
        .map(|base| {
            workload_database_path(
                base,
                "durable_reopen_with_artifacts",
                config.metric.concrete_metric(),
            )
        })
        .unwrap_or_else(|| {
            temp_dir
                .as_ref()
                .expect("temp dir exists without durable path")
                .path()
                .join("vector-indexing-reopen.strata")
        });
    let collection = collection();
    {
        let mut database = open_local(&path)?;
        load_dataset(
            &mut database,
            &collection,
            0,
            config.collection_size,
            config.dimension,
            config.metric.concrete_metric(),
            config.batch_size,
            "base",
        )?;
        build_policy_once(
            &mut database,
            &collection,
            &queries[0],
            config.k,
            VectorIndexPolicy::hnsw_only_for_test(),
        )?;
        database.close()?;
    }
    let mut database = open_local(&path)?;
    let exact = exact_keys(&mut database, &collection, queries, config.k)?;
    let mut row = measure_query_policy(
        &mut database,
        "durable_reopen_with_artifacts",
        &collection,
        queries,
        &exact,
        config,
        VectorIndexPolicy::hnsw_only_for_test(),
        None,
    )?;
    row.manifest_status = format!("{}:reopened", row.manifest_status);
    drop(temp_dir);
    Ok(row)
}

fn measure_query_policy(
    database: &mut Database,
    workload: &'static str,
    collection: &VectorCollectionName,
    queries: &[VectorEmbedding],
    exact: &[Vec<String>],
    config: &BenchConfig,
    policy: VectorIndexPolicy,
    write_throughput: Option<f64>,
) -> EngineResult<BenchmarkRow> {
    measure_query_policy_on_branch(
        database,
        workload,
        "default",
        collection,
        queries,
        exact,
        config.collection_size,
        config,
        policy,
        write_throughput,
    )
}

#[allow(clippy::too_many_arguments)]
fn measure_query_policy_on_branch(
    database: &mut Database,
    workload: &'static str,
    branch_name: &str,
    collection: &VectorCollectionName,
    queries: &[VectorEmbedding],
    exact: &[Vec<String>],
    visible_size: usize,
    config: &BenchConfig,
    policy: VectorIndexPolicy,
    write_throughput: Option<f64>,
) -> EngineResult<BenchmarkRow> {
    let (build_time, _) = build_policy_once_on_branch(
        database,
        branch_name,
        collection,
        &queries[0],
        config.k,
        policy,
    )?;
    let mut samples = Vec::with_capacity(config.query_samples);
    let mut last_diagnostics = None;
    let mut recall_hits = 0usize;
    let mut recall_expected = 0usize;
    for sample in 0..config.query_samples {
        let query_index = sample % queries.len();
        let query = &queries[query_index];
        let start = Instant::now();
        let (result, diagnostics) = vector_service(database, branch_name)?
            .query_with_index_policy_for_test(collection, query, config.k, None, policy)?;
        samples.push(start.elapsed());
        black_box(result.matches().len());
        let actual = result_keys(&result);
        let expected = exact
            .get(query_index)
            .expect("exact query keys exist for sample");
        let (hits, expected_count) = recall_counts(expected, &actual);
        recall_hits = recall_hits.saturating_add(hits);
        recall_expected = recall_expected.saturating_add(expected_count);
        last_diagnostics = Some(diagnostics);
    }
    let percentiles = Percentiles::from_samples(samples);
    let diagnostics = last_diagnostics.expect("at least one query sample");
    Ok(row_from_diagnostics(
        workload,
        visible_size,
        config.dimension,
        config.metric.concrete_metric(),
        config.mode,
        diagnostics,
        Some(build_time),
        Some(recall_from_counts(recall_hits, recall_expected)),
        Some(percentiles),
        write_throughput,
    ))
}

fn build_policy_once(
    database: &mut Database,
    collection: &VectorCollectionName,
    query: &VectorEmbedding,
    k: usize,
    policy: VectorIndexPolicy,
) -> EngineResult<(Duration, VectorIndexDiagnostics)> {
    build_policy_once_on_branch(database, "default", collection, query, k, policy)
}

fn build_policy_once_on_branch(
    database: &mut Database,
    branch_name: &str,
    collection: &VectorCollectionName,
    query: &VectorEmbedding,
    k: usize,
    policy: VectorIndexPolicy,
) -> EngineResult<(Duration, VectorIndexDiagnostics)> {
    let mut vectors = vector_service(database, branch_name)?;
    let build_time = if policy == VectorIndexPolicy::exact_only_for_test() {
        Duration::ZERO
    } else {
        let start = Instant::now();
        vectors.seal_index_artifacts_for_test(collection, policy)?;
        start.elapsed()
    };
    let (_, diagnostics) =
        vectors.query_with_index_policy_for_test(collection, query, k, None, policy)?;
    Ok((build_time, diagnostics))
}

#[allow(clippy::too_many_arguments)]
fn row_from_diagnostics(
    workload: &'static str,
    collection_size: usize,
    dimension: usize,
    metric: VectorDistanceMetric,
    mode: OpenMode,
    diagnostics: VectorIndexDiagnostics,
    build_time: Option<Duration>,
    recall_at_k: Option<f64>,
    query_percentiles: Option<Percentiles>,
    write_throughput: Option<f64>,
) -> BenchmarkRow {
    let artifact_sources = diagnostics
        .artifact_sources()
        .iter()
        .map(|source| ArtifactSourceRow {
            artifact_id: source.artifact_id().to_owned(),
            status: source.status().to_owned(),
            searched: source.searched(),
        })
        .collect();
    let artifact_missing_count = diagnostics
        .artifact_sources()
        .iter()
        .filter(|source| source.status() == "missing")
        .count();
    let artifact_stale_count = diagnostics
        .artifact_sources()
        .iter()
        .filter(|source| source.status() == "stale")
        .count();
    let artifact_corrupt_count = diagnostics
        .artifact_sources()
        .iter()
        .filter(|source| source.status() == "corrupt")
        .count();
    let artifact_over_budget_count = diagnostics
        .artifact_sources()
        .iter()
        .filter(|source| source.status() == "over_budget")
        .count();
    BenchmarkRow {
        workload,
        collection_size,
        dimension,
        metric: metric_name(metric),
        runtime_mode: mode.as_str(),
        index_policy: diagnostics.policy_mode().to_owned(),
        index_kind_used: diagnostics.resolved_index_kind_summary().to_owned(),
        hnsw_memory_budget_bytes: diagnostics.hnsw_memory_budget_bytes(),
        manifest_status: diagnostics.manifest_status().to_owned(),
        manifest_ref_count: diagnostics.manifest_ref_count(),
        inherited_ref_count: diagnostics.manifest_inherited_ref_count(),
        owned_ref_count: diagnostics.manifest_owned_ref_count(),
        active_delta_count: diagnostics.manifest_active_delta_count(),
        indexed_source_count: diagnostics.indexed_source_count(),
        exact_source_count: diagnostics.exact_source_count(),
        flat_source_count: diagnostics.flat_source_count(),
        hnsw_source_count: diagnostics.hnsw_source_count(),
        active_delta_source_count: diagnostics.active_delta_source_count(),
        indexed_vector_count: diagnostics.indexed_vector_count(),
        exact_fallback_count: diagnostics.exact_fallback_count(),
        build_time_ns: build_time.map(|duration| duration.as_nanos()),
        derived_bytes: diagnostics.derived_bytes(),
        recall_at_k,
        query_p50_ns: query_percentiles.as_ref().map(|p| p.p50.as_nanos()),
        query_p95_ns: query_percentiles.as_ref().map(|p| p.p95.as_nanos()),
        query_p99_ns: query_percentiles.as_ref().map(|p| p.p99.as_nanos()),
        write_throughput_ops_per_sec: write_throughput,
        write_p50_ns: None,
        write_p95_ns: None,
        write_p99_ns: None,
        artifact_missing_count,
        artifact_stale_count,
        artifact_corrupt_count,
        artifact_over_budget_count,
        artifact_sources,
    }
}

fn load_dataset(
    database: &mut Database,
    collection: &VectorCollectionName,
    start_index: usize,
    count: usize,
    dimension: usize,
    metric: VectorDistanceMetric,
    batch_size: usize,
    phase: &str,
) -> EngineResult<f64> {
    {
        let mut vectors = vector_service(database, "default")?;
        if start_index == 0 {
            vectors.create_collection(collection.clone(), VectorConfig::new(dimension, metric)?)?;
        }
    }
    let start = Instant::now();
    let mut written = 0usize;
    while written < count {
        let chunk = batch_size.min(count - written);
        let entries = (0..chunk)
            .map(|offset| upsert_entry(start_index + written + offset, dimension, phase))
            .collect::<EngineResult<Vec<_>>>()?;
        vector_service(database, "default")?.batch_upsert(collection, &entries)?;
        written += chunk;
    }
    Ok(ops_per_second(count, start.elapsed()))
}

fn measure_batch_writes(
    database: &mut Database,
    collection: &VectorCollectionName,
    start_index: usize,
    count: usize,
    dimension: usize,
    batch_size: usize,
    phase: &str,
) -> EngineResult<(Percentiles, Duration)> {
    let total_start = Instant::now();
    let mut timings = Vec::new();
    let mut written = 0usize;
    while written < count {
        let chunk = batch_size.min(count - written);
        let entries = (0..chunk)
            .map(|offset| upsert_entry(start_index + written + offset, dimension, phase))
            .collect::<EngineResult<Vec<_>>>()?;
        let start = Instant::now();
        vector_service(database, "default")?.batch_upsert(collection, &entries)?;
        timings.push(start.elapsed());
        written += chunk;
    }
    Ok((Percentiles::from_samples(timings), total_start.elapsed()))
}

fn exact_keys(
    database: &mut Database,
    collection: &VectorCollectionName,
    queries: &[VectorEmbedding],
    k: usize,
) -> EngineResult<Vec<Vec<String>>> {
    exact_keys_on_branch(database, "default", collection, queries, k)
}

fn exact_keys_on_branch(
    database: &mut Database,
    branch_name: &str,
    collection: &VectorCollectionName,
    queries: &[VectorEmbedding],
    k: usize,
) -> EngineResult<Vec<Vec<String>>> {
    queries
        .iter()
        .map(|query| {
            vector_service(database, branch_name)?
                .query_with_index_policy_for_test(
                    collection,
                    query,
                    k,
                    None,
                    VectorIndexPolicy::exact_only_for_test(),
                )
                .map(|(result, _)| result_keys(&result))
        })
        .collect()
}

fn result_keys(result: &VectorSearchResult) -> Vec<String> {
    result
        .matches()
        .iter()
        .map(|item| item.entry().key().as_str().to_owned())
        .collect()
}

fn recall_counts(expected: &[String], actual: &[String]) -> (usize, usize) {
    if expected.is_empty() {
        return (0, 0);
    }
    let actual = actual.iter().collect::<BTreeSet<_>>();
    let hits = expected
        .iter()
        .filter(|key| actual.contains(key))
        .count();
    (hits, expected.len())
}

fn recall_from_counts(hits: usize, expected: usize) -> f64 {
    if expected == 0 {
        return 1.0;
    }
    hits as f64 / expected as f64
}

const fn metric_name(metric: VectorDistanceMetric) -> &'static str {
    match metric {
        VectorDistanceMetric::Cosine => "cosine",
        VectorDistanceMetric::Euclidean => "euclidean",
        VectorDistanceMetric::DotProduct => "dot_product",
    }
}

fn query_vectors(config: &BenchConfig) -> EngineResult<Vec<VectorEmbedding>> {
    let count = config.query_samples.clamp(1, config.collection_size);
    (0..count)
        .map(|sample| {
            let index = sample.saturating_mul(config.collection_size) / count;
            embedding_for_index(index, config.dimension)
        })
        .collect()
}

fn upsert_entry(index: usize, dimension: usize, phase: &str) -> EngineResult<VectorUpsertEntry> {
    Ok(VectorUpsertEntry::new(
        vector_key(index)?,
        embedding_for_index(index, dimension)?,
        Some(metadata(json!({
            "phase": phase,
            "bucket": index % 16,
        }))?),
    ))
}

fn embedding_for_index(index: usize, dimension: usize) -> EngineResult<VectorEmbedding> {
    let mut seed = (index as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(0xD1B5_4A32_D192_ED03);
    let mut values = Vec::with_capacity(dimension);
    for lane in 0..dimension {
        seed ^= (lane as u64).wrapping_mul(0xA24B_AED4_963E_E407);
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let sample = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let unit = ((sample >> 40) as f32) / ((1u64 << 24) as f32);
        values.push(unit.mul_add(2.0, -1.0));
    }
    VectorEmbedding::new(values)
}

fn vector_key(index: usize) -> EngineResult<VectorKey> {
    VectorKey::new(format!("doc-{index:010}"))
}

fn collection() -> VectorCollectionName {
    VectorCollectionName::new("benchmark_vectors").expect("valid collection name")
}

fn metadata(value: Value) -> EngineResult<VectorMetadata> {
    VectorMetadata::new(value)
}

fn vector_service<'a>(
    database: &'a mut Database,
    branch_name: &str,
) -> EngineResult<strata_engine_next::VectorService<'a>> {
    database.vector(branch(branch_name)?, ProductSpace::new("default")?)
}

fn branch(name: &str) -> EngineResult<BranchName> {
    BranchName::new(name)
}

fn open_bench_database(config: &BenchConfig, workload: &'static str) -> EngineResult<OpenedDatabase> {
    let path = config
        .durable_path
        .as_ref()
        .map(|base| workload_database_path(base, workload, config.metric.concrete_metric()));
    open_database(config.mode, path.as_deref())
}

fn workload_database_path(base: &Path, workload: &str, metric: VectorDistanceMetric) -> PathBuf {
    base.join(format!(
        "{workload}-{}-{}.strata",
        metric_name(metric),
        std::process::id()
    ))
}

fn open_database(mode: OpenMode, path: Option<&Path>) -> EngineResult<OpenedDatabase> {
    match mode {
        OpenMode::Cache => Ok(OpenedDatabase {
            database: Database::open_cache(CacheOpenOptions::new())?.into_database(),
            _temp_dir: None,
        }),
        OpenMode::Durable => {
            if let Some(path) = path {
                Ok(OpenedDatabase {
                    database: open_local(path)?,
                    _temp_dir: None,
                })
            } else {
                let temp_dir = TempDir::new().map_err(io_engine_error)?;
                let database = open_local(&temp_dir.path().join("vector-indexing.strata"))?;
                Ok(OpenedDatabase {
                    database,
                    _temp_dir: Some(temp_dir),
                })
            }
        }
    }
}

fn open_local(path: &Path) -> EngineResult<Database> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io_engine_error)?;
    }
    Database::open_local(path, DurableLocalOpenOptions::new()).map(DatabaseOpenOutcome::into_database)
}

fn ops_per_second(count: usize, elapsed: Duration) -> f64 {
    if elapsed.is_zero() {
        return f64::INFINITY;
    }
    count as f64 / elapsed.as_secs_f64()
}

#[derive(Clone)]
struct Percentiles {
    p50: Duration,
    p95: Duration,
    p99: Duration,
}

impl Percentiles {
    fn from_samples(mut samples: Vec<Duration>) -> Self {
        samples.sort_unstable();
        let len = samples.len();
        Self {
            p50: samples[percentile_index(len, 50)],
            p95: samples[percentile_index(len, 95)],
            p99: samples[percentile_index(len, 99)],
        }
    }
}

fn percentile_index(len: usize, percentile: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (len.saturating_mul(percentile) / 100).min(len - 1)
}

fn parse_usize_arg(name: &str, value: Option<String>) -> EngineResult<usize> {
    let Some(value) = value else {
        return Err(strata_engine_next::EngineError::invalid_input(
            "invalid_argument.benchmark.arg",
            format!("{name} requires a numeric value"),
        ));
    };
    value.parse::<usize>().map_err(|error| {
        strata_engine_next::EngineError::invalid_input(
            "invalid_argument.benchmark.arg",
            format!("{name} must be a positive integer: {error}"),
        )
    })
}

fn io_engine_error(error: std::io::Error) -> strata_engine_next::EngineError {
    strata_engine_next::EngineError::invalid_input(
        "io.benchmark.tempdir",
        format!("benchmark filesystem setup failed: {error}"),
    )
}

fn print_help() {
    eprintln!(
        "usage: engine-vector-indexing [--records N] [--dimension N] [--k N] \\
         [--samples N] [--batch-size N] [--branch-writes N] [--mutations N] \\
         [--workload all|exact|flat|hnsw|write|mutation|branch|reopen] \\
         [--mode cache|durable] [--metric cosine|euclidean|dot_product|all] [--path PATH]"
    );
}
