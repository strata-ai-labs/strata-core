//! Benchmark-only probes for isolating storage-next hot-path costs.
//!
//! These probes are compiled only through `perf-trace`. They construct internal
//! storage-next data and measure alternative mechanics without changing the
//! production serving path.

use super::perf_trace::{self, StoragePerfSnapshot};
use crate::branch::read::{BranchReadBound, BranchVisibleRow};
use crate::branch::state::BranchLocalState;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use std::hint::black_box;
use std::time::Instant;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DEFAULT_SCALE_KEYS: usize = 100_000;
const DEFAULT_SAMPLES: usize = 1_000;
const DEFAULT_BATCH_SIZE: usize = 1_000;
const DEFAULT_VALUE_BYTES: usize = 150;
const DEFAULT_BUCKETS: usize = 4_096;
const DEFAULT_SEED: u64 = 0x5154_5241_5441_2026;
const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);

/// Inputs for the point-read isolation probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PointReadProbeConfig {
    /// Number of user keys loaded before the point-read cases run.
    pub scale_keys: usize,
    /// Number of point reads measured in each case.
    pub samples: usize,
    /// Synthetic commit batch size used while loading user keys.
    pub batch_size: usize,
    /// Byte length of each loaded user value.
    pub value_bytes: usize,
    /// Number of key-prefix buckets used by the benchmark key generator.
    pub bucket_count: usize,
    /// Seed for reproducible point-read request generation.
    pub seed: u64,
}

impl Default for PointReadProbeConfig {
    fn default() -> Self {
        Self {
            scale_keys: DEFAULT_SCALE_KEYS,
            samples: DEFAULT_SAMPLES,
            batch_size: DEFAULT_BATCH_SIZE,
            value_bytes: DEFAULT_VALUE_BYTES,
            bucket_count: DEFAULT_BUCKETS,
            seed: DEFAULT_SEED,
        }
    }
}

/// Full point-read isolation probe result.
#[derive(Clone, Debug)]
pub struct PointReadProbeReport {
    config: PointReadProbeConfig,
    branch_rows: usize,
    cases: Vec<PointReadProbeCase>,
}

impl PointReadProbeReport {
    /// Probe inputs used for this run.
    pub const fn config(&self) -> PointReadProbeConfig {
        self.config
    }

    /// Total rows loaded in the synthetic branch, including timeline rows.
    pub const fn branch_rows(&self) -> usize {
        self.branch_rows
    }

    /// Per-case measurements.
    pub fn cases(&self) -> &[PointReadProbeCase] {
        &self.cases
    }
}

/// One measured point-read mechanics case.
#[derive(Clone, Debug)]
pub struct PointReadProbeCase {
    name: &'static str,
    elapsed_ns: u128,
    samples: usize,
    found_rows: usize,
    perf: StoragePerfSnapshot,
}

impl PointReadProbeCase {
    /// Stable case identifier.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Total elapsed nanoseconds for this case.
    pub const fn elapsed_ns(&self) -> u128 {
        self.elapsed_ns
    }

    /// Number of point reads measured.
    pub const fn samples(&self) -> usize {
        self.samples
    }

    /// Number of successful row hits.
    pub const fn found_rows(&self) -> usize {
        self.found_rows
    }

    /// Hot-path counters captured for this case.
    pub const fn perf(&self) -> StoragePerfSnapshot {
        self.perf
    }

    /// Throughput for this case.
    #[expect(
        clippy::cast_precision_loss,
        reason = "throughput is a display-only floating point summary"
    )]
    pub fn ops_per_sec(&self) -> f64 {
        self.samples as f64 / (self.elapsed_ns as f64 / 1_000_000_000.0)
    }

    /// Average point-read latency in nanoseconds.
    pub fn avg_ns(&self) -> u128 {
        self.elapsed_ns / self.samples.max(1) as u128
    }
}

/// Run the PERF-P2 point-read isolation spike.
pub fn run_point_read_spike(config: PointReadProbeConfig) -> Result<PointReadProbeReport, String> {
    let config = validate_config(config)?;
    let mut state = build_probe_state(config)?;
    let branch_rows = state.active_row_count();
    let requests = build_requests(config)?;

    let cases = vec![
        measure_current_view_current_scan(&mut state, &requests)?,
        measure_current_view_direct_seek(&mut state, &requests)?,
        measure_borrowed_view_current_scan(&state, &requests)?,
        measure_borrowed_view_direct_seek(&state, &requests)?,
    ];

    Ok(PointReadProbeReport {
        config,
        branch_rows,
        cases,
    })
}

fn validate_config(mut config: PointReadProbeConfig) -> Result<PointReadProbeConfig, String> {
    if config.scale_keys == 0 {
        return Err("scale_keys must be greater than zero".to_string());
    }
    if config.samples == 0 {
        return Err("samples must be greater than zero".to_string());
    }
    if config.batch_size == 0 {
        return Err("batch_size must be greater than zero".to_string());
    }
    if config.bucket_count == 0 {
        config.bucket_count = config.scale_keys.clamp(1, DEFAULT_BUCKETS);
    }
    Ok(config)
}

fn build_probe_state(config: PointReadProbeConfig) -> Result<BranchLocalState, String> {
    let mut state = BranchLocalState::empty(DEFAULT_BRANCH_ID);
    let value = vec![0x42; config.value_bytes];
    let mut commit = 1u64;
    let mut written = 0usize;
    while written < config.scale_keys {
        let end = written
            .saturating_add(config.batch_size)
            .min(config.scale_keys);
        let commit_version = CommitVersion::new(commit);
        let timestamp = Timestamp::from_micros(commit);
        for index in written..end {
            state
                .append_committed_row(StorageRow::put(
                    engine_physical_key(key_for_index(index, config.bucket_count))?,
                    commit_version,
                    timestamp,
                    Timestamp::EPOCH,
                    value.clone(),
                ))
                .map_err(|error| error.to_string())?;
        }
        append_timeline_rows(&mut state, commit_version, timestamp, commit)?;
        written = end;
        commit = commit.saturating_add(1);
    }
    Ok(state)
}

fn append_timeline_rows(
    state: &mut BranchLocalState,
    commit_version: CommitVersion,
    timestamp: Timestamp,
    commit: u64,
) -> Result<(), String> {
    for suffix in [0u8, 1] {
        let mut user_key = b"commit-timeline/".to_vec();
        user_key.extend_from_slice(&commit.to_be_bytes());
        user_key.push(suffix);
        state
            .append_committed_row(StorageRow::put(
                PhysicalKey::new(
                    DEFAULT_BRANCH_ID,
                    "commit",
                    StorageSpaceId::COMMIT_TIMELINE,
                    user_key,
                )
                .map_err(|error| error.to_string())?,
                commit_version,
                timestamp,
                Timestamp::EPOCH,
                commit.to_be_bytes(),
            ))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn build_requests(config: PointReadProbeConfig) -> Result<Vec<PhysicalKey>, String> {
    let mut rng = FastRng::new(config.seed ^ 0x33);
    let mut requests = Vec::with_capacity(config.samples);
    for _ in 0..config.samples {
        let index = rng.next_usize(config.scale_keys);
        requests.push(engine_physical_key(key_for_index(
            index,
            config.bucket_count,
        ))?);
    }
    Ok(requests)
}

fn measure_current_view_current_scan(
    state: &mut BranchLocalState,
    requests: &[PhysicalKey],
) -> Result<PointReadProbeCase, String> {
    measure_case("current-view/current-scan", requests.len(), || {
        let mut found = 0usize;
        for key in requests {
            let view = state
                .capture_read_view()
                .map_err(|error| error.to_string())?;
            let row = view
                .read_point(key, BranchReadBound::latest())
                .map_err(|error| error.to_string())?;
            found = found.saturating_add(usize::from(require_visible(row, key)?));
        }
        Ok(found)
    })
}

fn measure_current_view_direct_seek(
    state: &mut BranchLocalState,
    requests: &[PhysicalKey],
) -> Result<PointReadProbeCase, String> {
    measure_case("current-view/direct-seek", requests.len(), || {
        let mut found = 0usize;
        for key in requests {
            let view = state
                .capture_read_view()
                .map_err(|error| error.to_string())?;
            black_box(view.active_row_count());
            let row = seek_state_latest(state, key);
            found = found.saturating_add(require_row(row, key)?);
        }
        Ok(found)
    })
}

fn measure_borrowed_view_current_scan(
    state: &BranchLocalState,
    requests: &[PhysicalKey],
) -> Result<PointReadProbeCase, String> {
    measure_case("borrowed-view/current-scan", requests.len(), || {
        let mut found = 0usize;
        for key in requests {
            let row = scan_state_latest(state, key);
            found = found.saturating_add(require_row(row, key)?);
        }
        Ok(found)
    })
}

fn measure_borrowed_view_direct_seek(
    state: &BranchLocalState,
    requests: &[PhysicalKey],
) -> Result<PointReadProbeCase, String> {
    measure_case("borrowed-view/direct-seek", requests.len(), || {
        let mut found = 0usize;
        for key in requests {
            let row = seek_state_latest(state, key);
            found = found.saturating_add(require_row(row, key)?);
        }
        Ok(found)
    })
}

fn measure_case(
    name: &'static str,
    samples: usize,
    mut run: impl FnMut() -> Result<usize, String>,
) -> Result<PointReadProbeCase, String> {
    perf_trace::reset();
    let start = Instant::now();
    let found_rows = run()?;
    let elapsed_ns = start.elapsed().as_nanos();
    let perf = perf_trace::snapshot();
    Ok(PointReadProbeCase {
        name,
        elapsed_ns,
        samples,
        found_rows,
        perf,
    })
}

fn scan_state_latest(state: &BranchLocalState, key: &PhysicalKey) -> Option<StorageRow> {
    let mut rows_visited = state.active().len();
    let mut candidates = Vec::new();
    for row in state
        .active()
        .iter()
        .filter(|row| row.physical_key() == key)
    {
        candidates.push(row.row().clone());
    }
    for table in state.frozen() {
        rows_visited = rows_visited.saturating_add(table.len());
        for row in table.iter().filter(|row| row.physical_key() == key) {
            candidates.push(row.row().clone());
        }
    }
    perf_trace::record_point_candidate_collection(rows_visited, candidates.len());
    candidates
        .into_iter()
        .max_by_key(|row| row.commit_version().as_u64())
}

fn seek_state_latest(state: &BranchLocalState, key: &PhysicalKey) -> Option<StorageRow> {
    let mut rows_visited = 0usize;
    let mut candidates = 0usize;

    let (row, visited) = state.active().perf_seek_physical_key_latest(key);
    rows_visited = rows_visited.saturating_add(visited);
    if let Some(row) = row {
        candidates = candidates.saturating_add(1);
        perf_trace::record_point_candidate_collection(rows_visited, candidates);
        return Some(row.row().clone());
    }

    for table in state.frozen() {
        let (row, visited) = table.perf_seek_physical_key_latest(key);
        rows_visited = rows_visited.saturating_add(visited);
        if let Some(row) = row {
            candidates = candidates.saturating_add(1);
            perf_trace::record_point_candidate_collection(rows_visited, candidates);
            return Some(row.row().clone());
        }
    }

    perf_trace::record_point_candidate_collection(rows_visited, candidates);
    None
}

fn require_visible(row: Option<BranchVisibleRow>, key: &PhysicalKey) -> Result<bool, String> {
    let Some(row) = row else {
        return Err(format!(
            "missing row for key length {}",
            key.user_key().len()
        ));
    };
    black_box(row.row().value().len());
    Ok(true)
}

fn require_row(row: Option<StorageRow>, key: &PhysicalKey) -> Result<usize, String> {
    let Some(row) = row else {
        return Err(format!(
            "missing row for key length {}",
            key.user_key().len()
        ));
    };
    black_box(row.value().len());
    Ok(1)
}

fn engine_physical_key(user_key: Vec<u8>) -> Result<PhysicalKey, String> {
    PhysicalKey::new(
        DEFAULT_BRANCH_ID,
        "default",
        StorageSpaceId::engine(0x20).map_err(|error| error.to_string())?,
        user_key,
    )
    .map_err(|error| error.to_string())
}

fn key_for_index(index: usize, bucket_count: usize) -> Vec<u8> {
    let bucket = index % bucket_count;
    let ordinal = index / bucket_count;
    let mut key = prefix_for_bucket(bucket);
    key.extend_from_slice(&(ordinal as u64).to_be_bytes());
    key
}

fn prefix_for_bucket(bucket: usize) -> Vec<u8> {
    let bucket = u16::try_from(bucket).unwrap_or(u16::MAX);
    let mut key = Vec::with_capacity(3);
    key.push(b'k');
    key.extend_from_slice(&bucket.to_be_bytes());
    key
}

struct FastRng {
    state: u64,
}

impl FastRng {
    const fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x0005_DEEC_E66D,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        assert!(upper > 0, "upper bound must be positive");
        let upper = u64::try_from(upper).expect("upper bound must fit in u64");
        let index = self.next_u64() % upper;
        usize::try_from(index).expect("modulo result is less than upper")
    }
}
