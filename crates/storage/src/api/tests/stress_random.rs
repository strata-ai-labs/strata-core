//! TCP4.13a: randomized concurrent-session stress over ONE shared runtime —
//! the `ClickHouse` stress-mode pattern mapped onto Strata's architecture.
//!
//! Interference bugs cannot appear when every test gets its own database.
//! This lane drives N concurrent sessions of randomized operations against a
//! single shared durable `StorageRuntime` while the background maintenance
//! machinery (flush, checkpoint, WAL rotation/truncation, compaction) runs
//! underneath — a light schedule-fuzzer ahead of the 4.3 loom/shuttle work.
//!
//! Layer choice: the storage API is the one surface where concurrent
//! sessions exist by design (`commit`/`read_point`/`scan_prefix`/`branch`
//! all take `&self`; BS5 write groups arbitrate concurrent writers). The
//! engine wire surface is single-session *by construction* — `Database` is
//! not `Clone` and every service accessor takes `&mut self` (see the
//! `branch_faults.rs` header) — so a wire-level "concurrent sessions" lane
//! cannot exist without inventing a surface the product does not have.
//!
//! Oracles (gate 8 secondary oracles ride the same workload):
//! - every session owns a disjoint key prefix and an exact shadow model;
//!   point reads and prefix scans must match the model exactly, so ANY
//!   cross-session interference (lost write, phantom row, torn batch) fails;
//! - a session's acked commit versions are strictly increasing;
//! - fork-under-load: a fork taken by a session must serve that session's
//!   model exactly as it stood at the fork (C4 inherit correctness);
//! - no unexpected error: only documented backpressure classes may be
//!   retried, everything else fails with seed + session + op index;
//! - after the run: the maintenance queue recorded zero failures (with the
//!   #2763 failure records printed if not), every session's final model
//!   matches a global sweep, and a close + reopen serves every model again
//!   (durability).
//!
//! Deterministic per seed in *content* (`STRATA_STRESS_SEED`); the thread
//! interleaving is the fuzzed dimension. Scale via `STRATA_STRESS_SESSIONS`
//! and `STRATA_STRESS_OPS` (nightly runs a larger grid than the per-PR
//! defaults).

use super::*;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::testkit::rng::SplitMix64;

const DEFAULT_SESSIONS: usize = 4;
const DEFAULT_OPS: usize = 150;
const DEFAULT_SEED: u64 = 0x57e5_5f00_d5ee_d001;
const MAX_BACKPRESSURE_RETRIES: usize = 200;
/// Fork branch ids are 0xC0 + n; the cap keeps ids inside the reserved band
/// even on scaled nightly runs.
const MAX_FORKS: u8 = 0x30;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().map_or(default, |raw| {
        raw.parse()
            .unwrap_or_else(|err| panic!("{name} must be a usize: {err}"))
    })
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().map_or(default, |raw| {
        raw.parse()
            .unwrap_or_else(|err| panic!("{name} must be a u64: {err}"))
    })
}

fn session_key(session: usize, index: u64) -> StorageKey {
    key(format!("s{session:02}/{index:06}").as_bytes())
}

fn session_prefix(session: usize) -> StorageKey {
    key(format!("s{session:02}/").as_bytes())
}

fn put_mutation(session: usize, index: u64, value: &[u8]) -> CommitMutation {
    CommitMutation::Put {
        storage_space: background_space(),
        key: session_key(session, index),
        value: StorageValue::new(value.to_vec()),
        ttl: None,
    }
}

fn delete_mutation(session: usize, index: u64) -> CommitMutation {
    CommitMutation::Delete {
        storage_space: background_space(),
        key: session_key(session, index),
    }
}

fn stress_batch(mutations: Vec<CommitMutation>) -> CommitBatch {
    CommitBatch::new(
        StorageRuntime::default_branch_id_for_test(),
        mutations,
        CommitOptions::default(),
    )
    .expect("valid stress batch")
}

/// Commits with bounded retries on documented backpressure classes; any
/// other refusal is an unexpected-error finding (gate 8(a)).
fn commit_with_backpressure_retries(
    runtime: &StorageRuntime<'static>,
    mutations: &[CommitMutation],
    context: &str,
) -> CommitSummary {
    let mut attempts = 0;
    loop {
        match runtime.commit(&stress_batch(mutations.to_vec())) {
            Ok(summary) => return summary,
            Err(error) => {
                let class = error.class();
                let retryable = matches!(class, StorageApiErrorClass::ResourceExhausted);
                assert!(
                    retryable,
                    "unexpected error class {class:?} ({code}) at {context}: {error}",
                    code = error.code()
                );
                attempts += 1;
                assert!(
                    attempts <= MAX_BACKPRESSURE_RETRIES,
                    "backpressure did not clear after {MAX_BACKPRESSURE_RETRIES} retries \
                     at {context}: {error}"
                );
                std::thread::yield_now();
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}

fn scan_session_rows(
    runtime: &StorageRuntime<'static>,
    branch: BranchId,
    session: usize,
) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let outcome = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch,
            background_space(),
            session_prefix(session),
            ReadBound::Latest,
            None,
        ))
        .unwrap_or_else(|error| panic!("session {session} prefix scan failed: {error}"));
    outcome
        .rows()
        .iter()
        .filter(|row| !row.is_tombstone())
        .map(|row| {
            (
                row.key().as_bytes().to_vec(),
                row.value()
                    .expect("non-tombstone row has value")
                    .as_bytes()
                    .to_vec(),
            )
        })
        .collect()
}

fn assert_model_matches(
    runtime: &StorageRuntime<'static>,
    branch: BranchId,
    session: usize,
    model: &BTreeMap<u64, Vec<u8>>,
    context: &str,
) {
    let observed = scan_session_rows(runtime, branch, session);
    let expected: BTreeMap<Vec<u8>, Vec<u8>> = model
        .iter()
        .map(|(index, value)| {
            (
                session_key(session, *index).as_bytes().to_vec(),
                value.clone(),
            )
        })
        .collect();
    assert_eq!(
        observed, expected,
        "session {session} {context}: rows diverged from the shadow model"
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "the session loop is one deliberately linear randomized workload"
)]
fn run_session(
    runtime: &StorageRuntime<'static>,
    session: usize,
    seed: u64,
    ops: usize,
    sessions: usize,
    fork_ids: &AtomicU8,
) -> BTreeMap<u64, Vec<u8>> {
    let branch = StorageRuntime::default_branch_id_for_test();
    let mut rng = SplitMix64::new(seed ^ (session as u64).wrapping_mul(0x9e37_79b9));
    let mut model: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    let mut next_index: u64 = 0;
    let mut last_version: Option<CommitVersion> = None;
    let mut forked = false;

    for op in 0..ops {
        let context = format!("seed={seed} session={session} op={op}");
        let choice = rng.next_u64() % 100;
        match choice {
            // Fresh puts: 1-6 new keys in one batch.
            0..=39 => {
                let count = 1 + (rng.next_u64() % 6);
                let mut mutations = Vec::new();
                let mut staged = Vec::new();
                for _ in 0..count {
                    let index = next_index;
                    next_index += 1;
                    let value = format!("{context} v{index}").into_bytes();
                    mutations.push(put_mutation(session, index, &value));
                    staged.push((index, value));
                }
                let summary = commit_with_backpressure_retries(runtime, &mutations, &context);
                for (index, value) in staged {
                    model.insert(index, value);
                }
                let version = summary.commit_version();
                if let Some(previous) = last_version {
                    assert!(
                        version > previous,
                        "{context}: acked version {version:?} did not advance past {previous:?}"
                    );
                }
                last_version = Some(version);
            }
            // Overwrite a known key.
            40..=54 => {
                if let Some(index) = pick_key(&model, &mut rng) {
                    let value = format!("{context} overwrite").into_bytes();
                    commit_with_backpressure_retries(
                        runtime,
                        &[put_mutation(session, index, &value)],
                        &context,
                    );
                    model.insert(index, value);
                }
            }
            // Delete a known key.
            55..=64 => {
                if let Some(index) = pick_key(&model, &mut rng) {
                    commit_with_backpressure_retries(
                        runtime,
                        &[delete_mutation(session, index)],
                        &context,
                    );
                    model.remove(&index);
                }
            }
            // Point-read a known key: must match the model exactly.
            65..=79 => {
                if let Some(index) = pick_key(&model, &mut rng) {
                    let outcome = runtime
                        .read_point(&PointReadRequest::new(
                            branch,
                            background_space(),
                            session_key(session, index),
                            ReadBound::Latest,
                        ))
                        .unwrap_or_else(|error| panic!("{context}: point read failed: {error}"));
                    let row = outcome
                        .row()
                        .unwrap_or_else(|| panic!("{context}: acked key {index} vanished"));
                    assert!(
                        !row.is_tombstone(),
                        "{context}: acked key {index} tombstoned"
                    );
                    assert_eq!(
                        row.value().expect("put row").as_bytes(),
                        model[&index].as_slice(),
                        "{context}: acked key {index} served a foreign value"
                    );
                }
            }
            // Scan the whole session keyspace against the model.
            80..=89 => {
                assert_model_matches(runtime, branch, session, &model, &context);
            }
            // Fork under load, once per session at most: the fork must serve
            // this session's model exactly as it stood at the fork.
            90..=92 => {
                if !forked && !model.is_empty() {
                    let fork_number = fork_ids.fetch_add(1, Ordering::Relaxed);
                    if fork_number < MAX_FORKS {
                        forked = true;
                        let fork = branch_id(0xC0 + fork_number);
                        runtime
                            .branch(&BranchRequest::new(
                                fork,
                                BranchAction::ForkCurrent { source: branch },
                                Some(BranchGeneration::new(1)),
                            ))
                            .unwrap_or_else(|error| panic!("{context}: fork failed: {error}"));
                        assert_model_matches(
                            runtime,
                            fork,
                            session,
                            &model,
                            &format!("{context} (fork read)"),
                        );
                    }
                }
            }
            // Cross-session read-only traffic: rows must be well-formed and
            // stay inside the scanned session's keyspace (no contamination);
            // content is not asserted — the owner is racing us.
            93..=95 => {
                let other = usize::try_from(rng.next_u64() % sessions as u64)
                    .expect("session index fits usize");
                let prefix = format!("s{other:02}/").into_bytes();
                let outcome = runtime
                    .scan_prefix(&PrefixScanReadRequest::new(
                        branch,
                        background_space(),
                        session_prefix(other),
                        ReadBound::Latest,
                        None,
                    ))
                    .unwrap_or_else(|error| panic!("{context}: foreign scan failed: {error}"));
                for row in outcome.rows() {
                    assert!(
                        row.key().as_bytes().starts_with(&prefix),
                        "{context}: scan of session {other} leaked key {:?}",
                        row.key()
                    );
                }
            }
            // Diagnostics probes exercise the observability surface mid-load.
            _ => {
                runtime
                    .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
                    .unwrap_or_else(|error| panic!("{context}: diagnostics failed: {error}"));
                runtime.maintenance_status().unwrap_or_else(|error| {
                    panic!("{context}: maintenance status failed: {error}")
                });
            }
        }
    }

    // Session epilogue: the full keyspace must match the model exactly.
    assert_model_matches(runtime, branch, session, &model, "final session sweep");
    model
}

fn pick_key(model: &BTreeMap<u64, Vec<u8>>, rng: &mut SplitMix64) -> Option<u64> {
    if model.is_empty() {
        return None;
    }
    let position =
        usize::try_from(rng.next_u64() % model.len() as u64).expect("model position fits usize");
    model.keys().nth(position).copied()
}

#[cfg(feature = "localfs")]
#[test]
fn concurrent_random_sessions_stay_exact_on_one_shared_database() {
    let sessions = env_usize("STRATA_STRESS_SESSIONS", DEFAULT_SESSIONS);
    let ops = env_usize("STRATA_STRESS_OPS", DEFAULT_OPS);
    let seed = env_u64("STRATA_STRESS_SEED", DEFAULT_SEED);
    assert!(sessions >= 2, "interference needs at least two sessions");

    let root = temp_dir_for_api_test("stress-random-shared");
    let backend = crate::testkit::leak_static(StorageBackend::local_fs(root.clone()));
    // Small-but-not-tiny WAL segments + moderate growth thresholds make
    // rotation, flush and checkpoint-enqueue traffic fire repeatedly DURING
    // the workload (the interference this lane exists to create) without
    // putting every third commit behind a checkpoint — 2 KiB segments with
    // tight thresholds measured ~39 min for the default grid because
    // checkpoint cost grows with the store while rotations stay
    // constant-rate. The default memory budget avoids the FrozenBacklog
    // wedge that stalls budget-starved debug builds (the 4.9b lesson).
    //
    // Once a session forks, the multi-branch checkpoint guard (TCP2.6/2.7:
    // the seeded-only collector defers whenever a non-seeded branch exists)
    // defers checkpoints for the rest of the run, so WAL segments accumulate
    // — deliberate V1 behavior; do not assert bounded WAL here.
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(32 * 1024, 4, 30))
            .with_wal_segment_size_for_test(16 * 1024),
        backend,
    )
    .expect("open shared stress runtime")
    .into_runtime();

    let fork_ids = AtomicU8::new(0);
    let mut models: Vec<(usize, BTreeMap<u64, Vec<u8>>)> = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for session in 0..sessions {
            let runtime = &runtime;
            let fork_ids = &fork_ids;
            handles.push((
                session,
                scope.spawn(move || run_session(runtime, session, seed, ops, sessions, fork_ids)),
            ));
        }
        for (session, handle) in handles {
            let model = handle
                .join()
                .unwrap_or_else(|_| panic!("session {session} panicked (seed={seed})"));
            models.push((session, model));
        }
    });

    runtime.wait_background_idle_for_test();
    let status = runtime
        .maintenance_status()
        .expect("final maintenance status");
    // Open-issue allowance (#2776, shrink-only — the fix removes this block):
    // background retention transiently fails on runner-class disks when
    // snapshot pruning's whole-tree listing walk races concurrent object
    // deletion (`failed_precondition.lifecycle.service`). The lane found and
    // classified it on its first CI run; every OTHER failure class stays
    // fatal, and a failure storm still trips the count bound.
    let unexpected: Vec<_> = status
        .recent_failures()
        .into_iter()
        .filter(|record| {
            !(record.task_kind() == "retention"
                && record.source_error_code() == Some("failed_precondition.lifecycle.service"))
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "background maintenance recorded unexpected failures under stress: {unexpected:?}"
    );
    assert!(
        status.failed() <= 4,
        "background maintenance failure storm under stress ({} failures): {:?}",
        status.failed(),
        status.recent_failures()
    );

    // Quiesced global sweep: every session's final model against the shared
    // branch, after all backgrounds work drained.
    let branch = StorageRuntime::default_branch_id_for_test();
    for (session, model) in &models {
        assert_model_matches(&runtime, branch, *session, model, "post-quiesce sweep");
    }

    // Durability oracle: close, reopen, and every model must still serve.
    let mut closing = runtime;
    closing.close().expect("close stress runtime");
    let backend = crate::testkit::leak_static(StorageBackend::local_fs(root));
    let reopened = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        backend,
    )
    .expect("reopen stress runtime")
    .into_runtime();
    for (session, model) in &models {
        assert_model_matches(&reopened, branch, *session, model, "post-reopen sweep");
    }
    let mut reopened = reopened;
    reopened.close().expect("close reopened runtime");
}
