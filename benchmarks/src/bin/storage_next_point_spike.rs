//! Storage-next PERF-P2 point-read isolation spike.
//!
//! This binary calls the hidden `perf-trace` probe in storage-next. It compares
//! current point-read mechanics against benchmark-only ordered-key seek and
//! borrowed-source variants without changing production serving behavior.

use std::collections::HashMap;
use std::fmt;

use strata_benchmarks::harness::recorder::ResultRecorder;
use strata_benchmarks::schema::{BenchmarkMetrics, BenchmarkResult};
use strata_storage_next::perf_probe::{run_point_read_spike, PointReadProbeConfig};

const CATEGORY: &str = "storage-next-point-spike";

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

fn run(config: Config) -> Result<(), String> {
    eprintln!("storage-next PERF-P2 point-read spike");
    eprintln!(
        "scale={} samples={} batch={} value={}B buckets={} seed={}",
        config.probe.scale_keys,
        config.probe.samples,
        config.probe.batch_size,
        config.probe.value_bytes,
        config.probe.bucket_count,
        config.probe.seed
    );
    eprintln!();

    let report = run_point_read_spike(config.probe)?;
    eprintln!("branch rows: {}", report.branch_rows());

    let mut recorder = ResultRecorder::new(CATEGORY);
    for case in report.cases() {
        print_case(case);
        recorder.record(BenchmarkResult {
            benchmark: format!("{CATEGORY}/{}", case.name()),
            category: CATEGORY.to_string(),
            parameters: parameters(&report.config(), report.branch_rows(), case),
            metrics: BenchmarkMetrics {
                ops_per_sec: Some(case.ops_per_sec()),
                avg_ns: Some(as_u64(case.avg_ns())),
                samples: Some(as_u64(case.samples())),
                ..Default::default()
            },
        });
    }

    let path = recorder.save().map_err(|error| error.to_string())?;
    eprintln!();
    eprintln!("results: {}", path.display());
    Ok(())
}

fn print_case(case: &strata_storage_next::perf_probe::PointReadProbeCase) {
    let perf = case.perf();
    eprintln!(
        "  {:<28} {:>12.0} ops/s  avg={:>10} ns  found={}",
        case.name(),
        case.ops_per_sec(),
        case.avg_ns(),
        case.found_rows()
    );
    eprintln!(
        "      read_views={} cloned_rows={} point_rows={} candidates={} seeks={}",
        perf.read_view_captures(),
        perf.read_view_rows_cloned(),
        perf.point_rows_visited(),
        perf.point_candidates_materialized(),
        perf.table_seeks()
    );
}

fn parameters(
    config: &PointReadProbeConfig,
    branch_rows: usize,
    case: &strata_storage_next::perf_probe::PointReadProbeCase,
) -> HashMap<String, serde_json::Value> {
    let perf = case.perf();
    let mut parameters = HashMap::new();
    parameters.insert("case".to_string(), serde_json::json!(case.name()));
    parameters.insert("scale_keys".to_string(), serde_json::json!(config.scale_keys));
    parameters.insert("samples".to_string(), serde_json::json!(config.samples));
    parameters.insert("batch_size".to_string(), serde_json::json!(config.batch_size));
    parameters.insert("value_bytes".to_string(), serde_json::json!(config.value_bytes));
    parameters.insert("bucket_count".to_string(), serde_json::json!(config.bucket_count));
    parameters.insert("seed".to_string(), serde_json::json!(config.seed));
    parameters.insert("branch_rows".to_string(), serde_json::json!(branch_rows));
    parameters.insert(
        "elapsed_ns".to_string(),
        serde_json::json!(as_u64(case.elapsed_ns())),
    );
    parameters.insert("found_rows".to_string(), serde_json::json!(case.found_rows()));
    parameters.insert(
        "perf_trace".to_string(),
        serde_json::json!({
            "read_view_captures": perf.read_view_captures(),
            "read_view_rows_cloned": perf.read_view_rows_cloned(),
            "append_staging_clones": perf.append_staging_clones(),
            "append_staging_rows_cloned": perf.append_staging_rows_cloned(),
            "blind_conflict_sources_built": perf.blind_conflict_sources_built(),
            "point_rows_visited": perf.point_rows_visited(),
            "point_candidates_materialized": perf.point_candidates_materialized(),
            "scan_rows_visited": perf.scan_rows_visited(),
            "scan_candidates_materialized": perf.scan_candidates_materialized(),
            "table_seeks": perf.table_seeks(),
        }),
    );
    parameters
}

#[derive(Clone, Copy, Debug)]
struct Config {
    probe: PointReadProbeConfig,
}

impl Config {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, CliError> {
        let mut probe = PointReadProbeConfig::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-h" | "--help" => return Err(CliError::Help),
                "--scale" | "--keys" => {
                    probe.scale_keys = parse_scale(next_value(&mut args, &arg)?, &arg)?;
                    if probe.bucket_count == PointReadProbeConfig::default().bucket_count {
                        probe.bucket_count = probe.scale_keys.clamp(1, 4_096);
                    }
                }
                "--samples" => {
                    probe.samples = parse_usize(next_value(&mut args, &arg)?, &arg)?;
                }
                "--batch-size" => {
                    probe.batch_size = parse_usize(next_value(&mut args, &arg)?, &arg)?;
                }
                "--value-bytes" => {
                    probe.value_bytes = parse_usize(next_value(&mut args, &arg)?, &arg)?;
                }
                "--buckets" => {
                    probe.bucket_count = parse_usize(next_value(&mut args, &arg)?, &arg)?;
                }
                "--seed" => {
                    probe.seed = parse_u64(next_value(&mut args, &arg)?, &arg)?;
                }
                other => return Err(CliError::UnknownArg(other.to_string())),
            }
        }
        Ok(Self { probe })
    }
}

fn print_help() {
    eprintln!("Usage: storage-next-point-spike [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --scale <N|100k|1m>       Key count (default 100k)");
    eprintln!("  --samples <N>             Point reads per case (default 1000)");
    eprintln!("  --batch-size <N>          Loaded rows per synthetic commit (default 1000)");
    eprintln!("  --value-bytes <N>         User value size (default 150)");
    eprintln!("  --buckets <N>             Key prefix bucket count (default min(scale, 4096))");
    eprintln!("  --seed <N>                Request RNG seed");
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, CliError> {
    args.next()
        .ok_or_else(|| CliError::MissingValue(flag.to_string()))
}

fn parse_scale(value: String, flag: &str) -> Result<usize, CliError> {
    let lower = value.to_ascii_lowercase();
    let (digits, multiplier) = if let Some(prefix) = lower.strip_suffix('k') {
        (prefix, 1_000usize)
    } else if let Some(prefix) = lower.strip_suffix('m') {
        (prefix, 1_000_000usize)
    } else {
        (lower.as_str(), 1usize)
    };
    let base = digits
        .parse::<usize>()
        .map_err(|_| CliError::InvalidValue {
            flag: flag.to_string(),
            value,
        })?;
    Ok(base.saturating_mul(multiplier))
}

fn parse_usize(value: String, flag: &str) -> Result<usize, CliError> {
    value.parse::<usize>().map_err(|_| CliError::InvalidValue {
        flag: flag.to_string(),
        value,
    })
}

fn parse_u64(value: String, flag: &str) -> Result<u64, CliError> {
    value.parse::<u64>().map_err(|_| CliError::InvalidValue {
        flag: flag.to_string(),
        value,
    })
}

fn as_u64(value: impl TryInto<u64>) -> u64 {
    value.try_into().unwrap_or(u64::MAX)
}

#[derive(Debug)]
enum CliError {
    Help,
    MissingValue(String),
    InvalidValue { flag: String, value: String },
    UnknownArg(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => formatter.write_str("help requested"),
            Self::MissingValue(flag) => write!(formatter, "missing value for {flag}"),
            Self::InvalidValue { flag, value } => {
                write!(formatter, "invalid value for {flag}: {value}")
            }
            Self::UnknownArg(arg) => write!(formatter, "unknown argument {arg}"),
        }
    }
}
