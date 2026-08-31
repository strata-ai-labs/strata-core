//! TCP4.12a — elle-style history recording + offline lineage/temporal checking.
//!
//! The whole-DB DST harness ([`super::whole_db`]) checks point-in-time oracles
//! against `ExpectedState`; it never records what a client *observed* across a
//! run. This module adds the missing half — the elle model — in two pieces:
//!
//! 1. A **traceability-co-designed** single-session workload whose reads are
//!    recorded into a totally-ordered [`History`]. Every write to a key
//!    *appends* a unique token (`branch:key:seq`) to the key's current value,
//!    so a single latest read recovers the whole append-order, and a fork
//!    child's value must literally *begin with* the source's value at the fork
//!    version — turning lineage transitivity (the #2521 class) into a byte
//!    prefix check on observed rows.
//! 2. An **offline checker** ([`check_history`]) that reconstructs the expected
//!    result of every recorded read from the recorded writes and forks *alone*
//!    — a second, independent oracle in elle's sense (the harness records what
//!    the database returned; the checker computes what it should have returned)
//!    — plus a version-monotonicity secondary oracle (gate 8).
//!
//! Scope of 4.12a: single session, fork-current lineage (chains, incl.
//! fork-of-a-fork), latest and at-version reads, no faults, no maintenance
//! pruning — history stays fully retained so at-version reads are exact.
//! Concurrent isolation histories (Adya inference, the #2682 class) and
//! faulted/pruned trajectories are 4.12b.

use std::collections::BTreeMap;
use std::path::Path;

use strata_core::{BranchId, CommitVersion};

use crate::api::{
    BranchAction, BranchGeneration, BranchRequest, CommitBatch, CommitMutation, CommitOptions,
    PointReadRequest, ReadBound, StorageBackend, StorageDurabilityPolicy, StorageKey,
    StorageRuntime, StorageSpaceId, StorageValue,
};
use crate::testkit::rng::SplitMix64;
use crate::testkit::TestkitError;

/// Bytes per append token: `[branch_tag, key, seq_hi, seq_lo]`. Unique per
/// append, so a value (a concatenation of tokens) records its whole write-order.
const TOKEN_LEN: usize = 4;
/// Distinct keys the workload appends to.
const KEY_COUNT: u8 = 4;
/// The runtime's default branch tag (`default_branch()` is all-`0x01`).
const DEFAULT_TAG: u8 = 0x01;
/// Fork-target branch tags (distinct first bytes, none all-zero or the default).
const FORK_TAGS: [u8; 4] = [0xB0, 0xB1, 0xB2, 0xB3];
/// Decorrelate the lineage-history stream from other seeded streams.
const HISTORY_SALT: u64 = 0x656C_6C65_4831_3261; // "elleH12a"

/// The bound a recorded read was taken at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BoundKind {
    Latest,
    AtVersion(u64),
}

/// One recorded operation in a single-session trajectory, in execution order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum HistoryEvent {
    /// A committed append: `key` on `branch_tag` gained `token`, acked at
    /// `version`. The resulting value is the running concatenation of tokens.
    Append {
        branch_tag: u8,
        key: u8,
        token: [u8; TOKEN_LEN],
        version: u64,
    },
    /// A fork-current: `target_tag` was created from `source_tag` observed as
    /// of `at_version` (the source's last acked version at fork time).
    Fork {
        source_tag: u8,
        target_tag: u8,
        at_version: u64,
    },
    /// A recorded read observation: the raw value bytes the runtime returned
    /// for `key` on `branch_tag` at `bound` (empty when the row was absent).
    Read {
        branch_tag: u8,
        key: u8,
        bound: BoundKind,
        observed: Vec<u8>,
    },
}

/// Anomalies the offline checker can report. Each is a *correctness* verdict,
/// never an existence check — the elle model derives the expected value and
/// compares bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum HistoryAnomaly {
    /// A read observed a value the recorded writes/forks do not explain. When
    /// `inherited_prefix` is set, the expected value carried a fork-inherited
    /// prefix the observation dropped — the #2521 lineage-break shape.
    ValueMismatch {
        branch_tag: u8,
        key: u8,
        bound: BoundKind,
        expected: Vec<u8>,
        observed: Vec<u8>,
        inherited_prefix: bool,
    },
    /// Acked commit versions were not strictly increasing (the logical clock
    /// must be monotonic) — a secondary oracle riding the same history.
    VersionRegression { previous: u64, next: u64 },
}

/// Non-vacuity counters for one checked history.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct HistoryStats {
    pub(super) appends: usize,
    pub(super) forks: usize,
    pub(super) latest_reads: usize,
    pub(super) as_of_reads: usize,
}

/// Per-key reconstruction state: the current value plus the version timeline
/// (ascending `(version, value-as-of-that-version)` snapshots) so an at-version
/// read resolves to the greatest snapshot `<= v`.
#[derive(Clone, Default)]
struct KeyState {
    current: Vec<u8>,
    timeline: Vec<(u64, Vec<u8>)>,
    /// Bytes inherited from the fork source at creation (a prefix of `current`).
    inherited_prefix_len: usize,
}

impl KeyState {
    fn as_of(&self, version: u64) -> Vec<u8> {
        // Greatest snapshot with commit version <= `version`; empty if none
        // (the key had no value at or before that version on this lineage).
        self.timeline
            .iter()
            .rev()
            .find(|(v, _)| *v <= version)
            .map_or_else(Vec::new, |(_, value)| value.clone())
    }
}

/// The reconstruction model: per `(branch_tag, key)` state.
type Model = BTreeMap<(u8, u8), KeyState>;

/// Reconstruct expected reads from the recorded writes and forks alone, and
/// report the first anomaly. Linear in history length.
pub(super) fn check_history(events: &[HistoryEvent]) -> Result<HistoryStats, HistoryAnomaly> {
    let mut model: Model = BTreeMap::new();
    let mut last_version: Option<u64> = None;
    let mut stats = HistoryStats::default();

    for event in events {
        match event {
            HistoryEvent::Append {
                branch_tag,
                key,
                token,
                version,
            } => {
                if let Some(previous) = last_version {
                    if *version <= previous {
                        return Err(HistoryAnomaly::VersionRegression {
                            previous,
                            next: *version,
                        });
                    }
                }
                last_version = Some(*version);
                let entry = model.entry((*branch_tag, *key)).or_default();
                entry.current.extend_from_slice(token);
                entry.timeline.push((*version, entry.current.clone()));
                stats.appends += 1;
            }
            HistoryEvent::Fork {
                source_tag,
                target_tag,
                at_version,
            } => {
                apply_fork(&mut model, *source_tag, *target_tag, *at_version);
                stats.forks += 1;
            }
            HistoryEvent::Read {
                branch_tag,
                key,
                bound,
                observed,
            } => check_read(&model, *branch_tag, *key, *bound, observed, &mut stats)?,
        }
    }
    Ok(stats)
}

/// A fork-current: the child inherits, per key, the source's value as of the
/// fork version and the source's pre-fork timeline (so a child at-version read
/// below the fork resolves through inheritance — the #2522 boundary).
fn apply_fork(model: &mut Model, source_tag: u8, target_tag: u8, at_version: u64) {
    for key in 0..KEY_COUNT {
        let (current, timeline) = model
            .get(&(source_tag, key))
            .map(|source| {
                let value = source.as_of(at_version);
                let mut timeline: Vec<(u64, Vec<u8>)> = source
                    .timeline
                    .iter()
                    .filter(|(v, _)| *v <= at_version)
                    .cloned()
                    .collect();
                // Collapse to the as-of value at the fork boundary so the
                // child's own future appends extend from it.
                if !value.is_empty() {
                    timeline.push((at_version, value.clone()));
                }
                (value, timeline)
            })
            .unwrap_or_default();
        let inherited_prefix_len = current.len();
        model.insert(
            (target_tag, key),
            KeyState {
                current,
                timeline,
                inherited_prefix_len,
            },
        );
    }
}

/// Compare an observed read against the value the model reconstructs for its
/// bound; a divergence is an anomaly (labelled a lineage break when it drops a
/// fork-inherited prefix).
fn check_read(
    model: &Model,
    branch_tag: u8,
    key: u8,
    bound: BoundKind,
    observed: &[u8],
    stats: &mut HistoryStats,
) -> Result<(), HistoryAnomaly> {
    let entry = model.get(&(branch_tag, key));
    let (expected, inherited_len) = match bound {
        BoundKind::Latest => {
            stats.latest_reads += 1;
            entry.map_or_else(
                || (Vec::new(), 0),
                |s| (s.current.clone(), s.inherited_prefix_len),
            )
        }
        BoundKind::AtVersion(version) => {
            stats.as_of_reads += 1;
            entry.map_or_else(
                || (Vec::new(), 0),
                |s| {
                    let value = s.as_of(version);
                    // The inherited prefix is only asserted as such when the
                    // as-of value still contains it.
                    let inherited = value.len().min(s.inherited_prefix_len);
                    (value, inherited)
                },
            )
        }
    };
    if observed != expected.as_slice() {
        let inherited_prefix = inherited_len > 0
            && (observed.len() < inherited_len
                || observed[..inherited_len] != expected[..inherited_len]);
        return Err(HistoryAnomaly::ValueMismatch {
            branch_tag,
            key,
            bound,
            expected,
            observed: observed.to_vec(),
            inherited_prefix,
        });
    }
    Ok(())
}

// --- harness ------------------------------------------------------------

fn history_space() -> StorageSpaceId {
    // A single engine-owned space byte (the storage API rejects multi-byte
    // spaces); distinct from the recovery oracle's own space.
    StorageSpaceId::new(vec![0x21]).expect("valid space")
}

fn history_key(key: u8) -> StorageKey {
    StorageKey::new(vec![key]).expect("valid key")
}

fn branch_for(tag: u8) -> BranchId {
    BranchId::from_bytes([tag; BranchId::BYTE_LEN])
}

/// Harness-side bookkeeping for one live branch.
struct LiveBranch {
    tag: u8,
    branch: BranchId,
    /// The current intended value per key (harness truth, tracked independent
    /// of reads so a read bug cannot corrupt a later write).
    values: BTreeMap<u8, Vec<u8>>,
    /// Per-key next sequence number for unique tokens.
    seqs: BTreeMap<u8, u16>,
    /// Versions this branch has acked (for at-version read targets).
    versions: Vec<u64>,
    last_version: u64,
}

impl LiveBranch {
    fn token(&mut self, key: u8) -> [u8; TOKEN_LEN] {
        let seq = self.seqs.entry(key).or_insert(0);
        let value = *seq;
        *seq += 1;
        [self.tag, key, (value >> 8) as u8, (value & 0xff) as u8]
    }
}

/// Drive one seeded single-session lineage trajectory against a fresh durable
/// store and record the observed [`History`]. Pure function of `seed` — the
/// event sequence replays bit-exact.
pub(super) fn run_lineage_history(
    root: &Path,
    seed: u64,
    steps: usize,
) -> Result<Vec<HistoryEvent>, TestkitError> {
    let mut rng = SplitMix64::new(seed ^ HISTORY_SALT);
    let durability = if seed & 1 == 0 {
        StorageDurabilityPolicy::Always
    } else {
        StorageDurabilityPolicy::Standard
    };
    let backend = StorageBackend::local_fs(root.to_path_buf());
    let (mut runtime, _summary) = StorageRuntime::open_with_backend(
        super::faults::deterministic_options(durability),
        &backend,
    )
    .map_err(|err| TestkitError::new(format!("[seed={seed}] open: {err:?}")))?
    .into_parts();

    let mut live: Vec<LiveBranch> = vec![LiveBranch {
        tag: DEFAULT_TAG,
        branch: branch_for(DEFAULT_TAG),
        values: BTreeMap::new(),
        seqs: BTreeMap::new(),
        versions: Vec::new(),
        last_version: 0,
    }];
    let mut events: Vec<HistoryEvent> = Vec::new();

    for step in 0..steps {
        // Weighted draw: append (3), read (2), fork (1).
        match rng.next_u64() % 6 {
            0..=2 => append_step(&mut runtime, &mut rng, &mut live, &mut events, seed, step)?,
            3..=4 => read_step(&runtime, &mut rng, &live, &mut events, seed, step)?,
            _ => fork_step(&mut runtime, &mut rng, &mut live, &mut events, seed, step)?,
        }
    }

    drop(runtime);
    Ok(events)
}

fn pick_live_index(rng: &mut SplitMix64, live: &[LiveBranch]) -> usize {
    usize::try_from(rng.next_u64() % live.len() as u64).expect("bounded")
}

fn append_step(
    runtime: &mut StorageRuntime<'_>,
    rng: &mut SplitMix64,
    live: &mut [LiveBranch],
    events: &mut Vec<HistoryEvent>,
    seed: u64,
    step: usize,
) -> Result<(), TestkitError> {
    let index = pick_live_index(rng, live);
    let key = rng.gen_u8_below(KEY_COUNT);
    let branch = live[index].branch;
    let tag = live[index].tag;
    let token = live[index].token(key);
    let mut value = live[index].values.get(&key).cloned().unwrap_or_default();
    value.extend_from_slice(&token);

    let mutation = CommitMutation::Put {
        storage_space: history_space(),
        key: history_key(key),
        value: StorageValue::new(value.clone()),
        ttl: None,
    };
    let batch = CommitBatch::new(branch, vec![mutation], CommitOptions::default())
        .map_err(|err| TestkitError::new(format!("[seed={seed} step={step}] batch: {err:?}")))?;
    let summary = runtime
        .commit(&batch)
        .map_err(|err| TestkitError::new(format!("[seed={seed} step={step}] commit: {err:?}")))?;
    let version = summary.commit_version().as_u64();

    live[index].values.insert(key, value);
    live[index].versions.push(version);
    live[index].last_version = version;
    events.push(HistoryEvent::Append {
        branch_tag: tag,
        key,
        token,
        version,
    });
    Ok(())
}

fn read_step(
    runtime: &StorageRuntime<'_>,
    rng: &mut SplitMix64,
    live: &[LiveBranch],
    events: &mut Vec<HistoryEvent>,
    seed: u64,
    step: usize,
) -> Result<(), TestkitError> {
    let index = pick_live_index(rng, live);
    let key = rng.gen_u8_below(KEY_COUNT);
    let branch = live[index].branch;
    let tag = live[index].tag;

    // Half the reads are at-version when the branch has a version to aim at.
    let bound = if rng.next_u64() % 2 == 0 && !live[index].versions.is_empty() {
        let pick =
            usize::try_from(rng.next_u64() % live[index].versions.len() as u64).expect("bounded");
        BoundKind::AtVersion(live[index].versions[pick])
    } else {
        BoundKind::Latest
    };
    let read_bound = match bound {
        BoundKind::Latest => ReadBound::Latest,
        BoundKind::AtVersion(v) => ReadBound::AtVersion(CommitVersion::new(v)),
    };
    let outcome = runtime
        .read_point(&PointReadRequest::new(
            branch,
            history_space(),
            history_key(key),
            read_bound,
        ))
        .map_err(|err| TestkitError::new(format!("[seed={seed} step={step}] read: {err:?}")))?;
    let observed = outcome
        .row()
        .filter(|row| !row.is_tombstone())
        .and_then(|row| row.value())
        .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
    events.push(HistoryEvent::Read {
        branch_tag: tag,
        key,
        bound,
        observed,
    });
    Ok(())
}

fn fork_step(
    runtime: &mut StorageRuntime<'_>,
    rng: &mut SplitMix64,
    live: &mut Vec<LiveBranch>,
    events: &mut Vec<HistoryEvent>,
    seed: u64,
    step: usize,
) -> Result<(), TestkitError> {
    // A free fork tag not currently live (fork targets are single-use in 4.12a,
    // so no delete/recreate generation bookkeeping is needed).
    let Some(target_tag) = FORK_TAGS
        .iter()
        .copied()
        .find(|tag| !live.iter().any(|branch| branch.tag == *tag))
    else {
        return Ok(()); // pool full this step; seeded no-op
    };
    let source_index = pick_live_index(rng, live);
    let source = live[source_index].branch;
    let source_tag = live[source_index].tag;
    let at_version = live[source_index].last_version;
    let target = branch_for(target_tag);

    runtime
        .branch(&BranchRequest::new(
            target,
            BranchAction::ForkCurrent { source },
            Some(BranchGeneration::new(1)),
        ))
        .map_err(|err| TestkitError::new(format!("[seed={seed} step={step}] fork: {err:?}")))?;

    // The child inherits the source's current intended values (fork-current)
    // and the source's pre-fork versions, so at-version reads on the child can
    // aim below/at the fork point — the inherited-as-of (#2521/#2522) boundary.
    let values = live[source_index].values.clone();
    let versions: Vec<u64> = live[source_index]
        .versions
        .iter()
        .copied()
        .filter(|version| *version <= at_version)
        .collect();
    live.push(LiveBranch {
        tag: target_tag,
        branch: target,
        values,
        seqs: BTreeMap::new(),
        versions,
        last_version: at_version,
    });
    events.push(HistoryEvent::Fork {
        source_tag,
        target_tag,
        at_version,
    });
    Ok(())
}

/// Counters describing a lineage-history sweep (non-vacuity for the checker).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LineageHistoryOutcome {
    seeds_executed: usize,
    forks: usize,
    appends: usize,
    as_of_reads: usize,
}

impl LineageHistoryOutcome {
    #[must_use]
    pub const fn seeds_executed(&self) -> usize {
        self.seeds_executed
    }
    #[must_use]
    pub const fn forks(&self) -> usize {
        self.forks
    }
    #[must_use]
    pub const fn as_of_reads(&self) -> usize {
        self.as_of_reads
    }
}

/// Default per-PR seed count; the nightly soak scales via `case_limit`.
const DEFAULT_SEEDS: usize = 16;
const DEFAULT_STEPS: usize = 60;

/// Sweep seeded lineage-history trajectories, checking each recorded history
/// offline. Any anomaly fails loudly with the seed for replay.
pub fn run_lineage_history_harness(
    root: &Path,
    case_limit: Option<usize>,
) -> Result<LineageHistoryOutcome, TestkitError> {
    let seeds = case_limit.unwrap_or(DEFAULT_SEEDS);
    let mut outcome = LineageHistoryOutcome::default();
    for seed in 0..seeds as u64 {
        let dir = root.join(format!("lineage-{seed}"));
        std::fs::create_dir_all(&dir)
            .map_err(|err| TestkitError::new(format!("[seed={seed}] mkdir: {err}")))?;
        let events = run_lineage_history(&dir, seed, DEFAULT_STEPS)?;
        let stats = check_history(&events).map_err(|anomaly| {
            TestkitError::new(format!(
                "[seed={seed}] history anomaly (replay: run_lineage_history(seed={seed})): {anomaly:?}"
            ))
        })?;
        outcome.seeds_executed += 1;
        outcome.forks += stats.forks;
        outcome.appends += stats.appends;
        outcome.as_of_reads += stats.as_of_reads;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(tag: u8, key: u8, seq: u16) -> [u8; TOKEN_LEN] {
        [tag, key, (seq >> 8) as u8, (seq & 0xff) as u8]
    }

    #[test]
    fn checker_accepts_a_faithful_lineage_history() {
        // default appends t0,t1 to key 0; fork B at v2; B appends t2; reads
        // observe exactly the reconstructed values.
        let t0 = token(0x00, 0, 0);
        let t1 = token(0x00, 0, 1);
        let t2 = token(0xB0, 0, 0);
        let mut default_value = t0.to_vec();
        default_value.extend_from_slice(&t1);
        let mut child_value = default_value.clone();
        child_value.extend_from_slice(&t2);
        let events = vec![
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: t0,
                version: 1,
            },
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: t1,
                version: 2,
            },
            HistoryEvent::Fork {
                source_tag: 0x00,
                target_tag: 0xB0,
                at_version: 2,
            },
            HistoryEvent::Append {
                branch_tag: 0xB0,
                key: 0,
                token: t2,
                version: 3,
            },
            // child latest sees the inherited prefix plus its own append
            HistoryEvent::Read {
                branch_tag: 0xB0,
                key: 0,
                bound: BoundKind::Latest,
                observed: child_value,
            },
            // default latest is unaffected by the child's write (fork isolation)
            HistoryEvent::Read {
                branch_tag: 0x00,
                key: 0,
                bound: BoundKind::Latest,
                observed: default_value.clone(),
            },
            // child as-of the fork version sees exactly the inherited value
            HistoryEvent::Read {
                branch_tag: 0xB0,
                key: 0,
                bound: BoundKind::AtVersion(2),
                observed: default_value,
            },
        ];
        let stats = check_history(&events).expect("faithful history is clean");
        assert_eq!(stats.forks, 1);
        assert_eq!(stats.appends, 3);
        assert_eq!(stats.latest_reads, 2);
        assert_eq!(stats.as_of_reads, 1);
    }

    #[test]
    fn checker_catches_a_dropped_fork_inheritance() {
        // The #2521 shape: default has a value, B forks it, but B's read drops
        // the inherited state (observes empty). The checker must flag it as an
        // inherited-prefix lineage break.
        let t0 = token(0x00, 0, 0);
        let events = vec![
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: t0,
                version: 1,
            },
            HistoryEvent::Fork {
                source_tag: 0x00,
                target_tag: 0xB0,
                at_version: 1,
            },
            HistoryEvent::Read {
                branch_tag: 0xB0,
                key: 0,
                bound: BoundKind::Latest,
                observed: Vec::new(), // dropped the inherited t0
            },
        ];
        let anomaly = check_history(&events).expect_err("dropped inheritance is an anomaly");
        match anomaly {
            HistoryAnomaly::ValueMismatch {
                branch_tag,
                inherited_prefix,
                expected,
                ..
            } => {
                assert_eq!(branch_tag, 0xB0);
                assert!(inherited_prefix, "must classify as a lineage break");
                assert_eq!(expected, t0.to_vec());
            }
            other @ HistoryAnomaly::VersionRegression { .. } => {
                panic!("expected ValueMismatch, got {other:?}")
            }
        }
    }

    #[test]
    fn checker_catches_a_non_inherited_value_mismatch() {
        // A plain read-your-writes break (no fork): observed differs, but it is
        // NOT an inherited-prefix break — direction control on the #2521 label.
        let t0 = token(0x00, 0, 0);
        let events = vec![
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: t0,
                version: 1,
            },
            HistoryEvent::Read {
                branch_tag: 0x00,
                key: 0,
                bound: BoundKind::Latest,
                observed: vec![0xde_u8], // arbitrary wrong byte, not the real token
            },
        ];
        match check_history(&events).expect_err("mismatch is an anomaly") {
            HistoryAnomaly::ValueMismatch {
                inherited_prefix, ..
            } => assert!(!inherited_prefix, "no fork ⇒ not a lineage break"),
            other @ HistoryAnomaly::VersionRegression { .. } => {
                panic!("expected ValueMismatch, got {other:?}")
            }
        }
    }

    #[test]
    fn checker_catches_a_wrong_as_of_snapshot() {
        // An at-version read must resolve to that version's snapshot, not the
        // latest value — direction control on the temporal (as-of) arm.
        let t0 = token(0x01, 0, 0);
        let t1 = token(0x01, 0, 1);
        let mut latest = t0.to_vec();
        latest.extend_from_slice(&t1);
        let events = vec![
            HistoryEvent::Append {
                branch_tag: 0x01,
                key: 0,
                token: t0,
                version: 1,
            },
            HistoryEvent::Append {
                branch_tag: 0x01,
                key: 0,
                token: t1,
                version: 2,
            },
            HistoryEvent::Read {
                branch_tag: 0x01,
                key: 0,
                bound: BoundKind::AtVersion(1),
                observed: latest, // observed latest, but v1 snapshot is just t0
            },
        ];
        match check_history(&events).expect_err("wrong as-of snapshot is an anomaly") {
            HistoryAnomaly::ValueMismatch {
                bound, expected, ..
            } => {
                assert_eq!(bound, BoundKind::AtVersion(1));
                assert_eq!(expected, t0.to_vec(), "as-of v1 is exactly t0");
            }
            other @ HistoryAnomaly::VersionRegression { .. } => {
                panic!("expected ValueMismatch, got {other:?}")
            }
        }
    }

    #[test]
    #[ignore = "nightly soak; scale seed count via STRATA_LINEAGE_SEEDS"]
    fn lineage_history_soak() {
        let seeds = std::env::var("STRATA_LINEAGE_SEEDS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(200);
        let dir = tempfile::tempdir().expect("tmp");
        let outcome =
            run_lineage_history_harness(dir.path(), Some(seeds)).expect("soak sweep is clean");
        assert_eq!(outcome.seeds_executed(), seeds);
        assert!(
            outcome.forks() > 0 && outcome.as_of_reads() > 0,
            "soak must exercise forks and at-version reads"
        );
    }

    #[test]
    fn lineage_history_sweep_is_clean_and_non_vacuous() {
        // The offline checker validates the LIVE database's real lineage/temporal
        // behavior across seeded trajectories — clean, and non-vacuously so.
        let dir = tempfile::tempdir().expect("tmp");
        let outcome = run_lineage_history_harness(dir.path(), Some(8)).expect("sweep is clean");
        assert_eq!(outcome.seeds_executed(), 8);
        assert!(
            outcome.forks() > 0,
            "some seed must fork (lineage coverage)"
        );
        assert!(
            outcome.as_of_reads() > 0,
            "some seed must read at-version (temporal coverage)"
        );
    }

    #[test]
    fn lineage_history_replays_bit_exact() {
        let dir_a = tempfile::tempdir().expect("tmp");
        let dir_b = tempfile::tempdir().expect("tmp");
        let first = run_lineage_history(dir_a.path(), 7, 60).expect("first run");
        let second = run_lineage_history(dir_b.path(), 7, 60).expect("second run");
        assert_eq!(first, second, "same seed produced divergent histories");
    }

    #[test]
    fn checker_catches_a_version_regression() {
        let events = vec![
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: token(0x00, 0, 0),
                version: 5,
            },
            HistoryEvent::Append {
                branch_tag: 0x00,
                key: 0,
                token: token(0x00, 0, 1),
                version: 5, // not strictly increasing
            },
        ];
        assert_eq!(
            check_history(&events).expect_err("regression is an anomaly"),
            HistoryAnomaly::VersionRegression {
                previous: 5,
                next: 5
            }
        );
    }
}
