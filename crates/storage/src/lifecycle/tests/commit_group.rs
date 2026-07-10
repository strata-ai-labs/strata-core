//! Write-group execution tests (BS5.1).
//!
//! The equivalence anchor: a group of one is byte-identical to a solo commit —
//! same outcome, same visible version, same backend object bytes. Larger
//! groups allocate contiguous versions in member order (WAL order == version
//! order), reject invalid members cleanly without disturbing the rest, and
//! survive mid-group WAL segment rotation.

use super::checkpoint::shared::{
    branch_id, durable_batch, generation_guard, CheckpointTestBackend,
};
use super::*;
use crate::branch::config::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use crate::service::WalServiceConfig;
use strata_core::{BranchId, Timestamp};

const DATABASE_ID: [u8; 16] = [0x5b; 16];

fn open_group_runtime(
    branch: BranchId,
    backend: &'static CheckpointTestBackend,
    mode: StorageMode,
    segment_size: u64,
) -> LifecycleDurableLocalRuntime<'static, CommitManualTimestampSource> {
    let lifecycle_config = LifecycleConfig::default()
        .with_wal_growth_policy(LifecycleWalGrowthPolicy::new(u64::MAX, usize::MAX, None))
        .expect("lifecycle config");
    let request = LifecycleDurableLocalOpenRequest::new(
        StorageOpenPlan::new(
            mode,
            LifecycleCodecId::identity(),
            RecoveryStrictness::Strict,
            lifecycle_config,
        )
        .expect("open plan"),
        DATABASE_ID,
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::new(segment_size),
    )
    .expect("durable request");
    let mut shell = LifecycleDurableLocalShell::assemble(
        request,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(9_000)),
    )
    .expect("durable shell");
    let recovery_request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .expect("recovery outcome");
    shell.complete_recovery(&recovery).expect("runtime")
}

fn leak_backend() -> &'static CheckpointTestBackend {
    Box::leak(Box::new(CheckpointTestBackend::new()))
}

fn group_batch(
    branch: BranchId,
    user_key: Vec<u8>,
    value: Vec<u8>,
    durability: CommitDurabilityMode,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            PhysicalKey::new(
                branch,
                "commit-group",
                StorageSpaceId::engine(0x36).expect("engine storage space"),
                user_key,
            )
            .expect("physical key"),
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            durability,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

#[test]
fn group_of_one_matches_solo_commit_byte_for_byte() {
    let branch = branch_id(0xb1);

    let solo_backend = leak_backend();
    let mut solo_runtime = open_group_runtime(
        branch,
        solo_backend,
        StorageMode::DurableLocalStandard,
        WalServiceConfig::default().segment_size(),
    );
    let solo_outcome = solo_runtime
        .execute_durable_commit(
            durable_batch(branch, b"anchor-key", b"anchor-value"),
            generation_guard(),
        )
        .expect("solo commit");

    let group_backend = leak_backend();
    let mut group_runtime = open_group_runtime(
        branch,
        group_backend,
        StorageMode::DurableLocalStandard,
        WalServiceConfig::default().segment_size(),
    );
    let mut group_results = group_runtime.execute_durable_commit_group(vec![(
        durable_batch(branch, b"anchor-key", b"anchor-value"),
        generation_guard(),
    )]);
    assert_eq!(group_results.len(), 1);
    let group_outcome = group_results
        .pop()
        .expect("one member result")
        .outcome
        .expect("group-of-1 commit");

    assert_eq!(group_outcome.branch_id(), solo_outcome.branch_id());
    assert_eq!(
        group_outcome.commit_version(),
        solo_outcome.commit_version()
    );
    assert_eq!(
        group_outcome.commit_timestamp(),
        solo_outcome.commit_timestamp()
    );
    assert_eq!(group_outcome.durability(), solo_outcome.durability());
    assert_eq!(
        group_outcome.visibility_facts(),
        solo_outcome.visibility_facts()
    );
    assert_eq!(
        group_runtime.visible_version(),
        solo_runtime.visible_version()
    );
    // The strongest anchor: every stored object — WAL segments included — is
    // byte-identical between the solo commit and the group of one.
    assert_eq!(
        group_backend.object_snapshot(),
        solo_backend.object_snapshot()
    );
}

#[test]
fn group_members_commit_contiguous_versions_in_member_order() {
    let branch = branch_id(0xb2);
    let backend = leak_backend();
    let mut runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalStandard,
        WalServiceConfig::default().segment_size(),
    );

    let members = (0..4)
        .map(|index| {
            (
                group_batch(
                    branch,
                    format!("member-{index}").into_bytes(),
                    vec![u8::try_from(index).expect("small index"); 8],
                    CommitDurabilityMode::Standard,
                ),
                generation_guard(),
            )
        })
        .collect();
    let results = runtime.execute_durable_commit_group(members);

    assert_eq!(results.len(), 4);
    let versions: Vec<_> = results
        .iter()
        .map(|result| {
            result
                .outcome
                .as_ref()
                .expect("member commit")
                .commit_version()
                .expect("mutating outcome version")
        })
        .collect();
    // Contiguous block in member order: member order == version order (== WAL
    // append order by construction of the sequential leader loop).
    for pair in versions.windows(2) {
        assert_eq!(pair[1].as_u64(), pair[0].as_u64() + 1);
    }
    // One publish to the group max: the runtime's visible version is the last
    // member's version.
    assert_eq!(runtime.visible_version(), versions[3]);
    // The gate is clean after a successful group.
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());
}

#[test]
fn group_member_rejection_is_per_member_and_clean() {
    let branch = branch_id(0xb3);
    let backend = leak_backend();
    let mut runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalStandard,
        WalServiceConfig::default().segment_size(),
    );

    let stale_guard = CommitBranchGenerationGuard::exact(
        CommitBranchGeneration::new(7).expect("stale generation"),
    );
    let members = vec![
        (
            group_batch(
                branch,
                b"first".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Standard,
            ),
            generation_guard(),
        ),
        (
            group_batch(
                branch,
                b"rejected".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Standard,
            ),
            stale_guard,
        ),
        (
            group_batch(
                branch,
                b"third".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Standard,
            ),
            generation_guard(),
        ),
    ];
    let results = runtime.execute_durable_commit_group(members);

    assert_eq!(results.len(), 3);
    let first = results[0].outcome.as_ref().expect("first member commits");
    assert!(matches!(
        results[1]
            .outcome
            .as_ref()
            .expect_err("stale guard rejected"),
        LifecycleError::BranchGenerationMismatch { .. }
    ));
    let third = results[2].outcome.as_ref().expect("third member commits");

    // The rejection is pre-WAL and clean: the survivors' versions are
    // contiguous (no version burned on the rejected member) and the group
    // published to the last survivor.
    let first_version = first.commit_version().expect("mutating outcome version");
    let third_version = third.commit_version().expect("mutating outcome version");
    assert_eq!(third_version.as_u64(), first_version.as_u64() + 1);
    assert_eq!(runtime.visible_version(), third_version);
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());

    // The runtime stays fully usable: a follow-up solo commit lands at the
    // next version.
    let follow_up = runtime
        .execute_durable_commit(
            durable_batch(branch, b"follow-up", b"value"),
            generation_guard(),
        )
        .expect("follow-up solo commit");
    assert_eq!(
        follow_up
            .commit_version()
            .expect("mutating outcome version")
            .as_u64(),
        third_version.as_u64() + 1
    );
}

#[test]
fn group_with_all_members_rejected_publishes_nothing() {
    let branch = branch_id(0xb4);
    let backend = leak_backend();
    let mut runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalStandard,
        WalServiceConfig::default().segment_size(),
    );
    let visible_before = runtime.visible_version();

    let stale_guard = CommitBranchGenerationGuard::exact(
        CommitBranchGeneration::new(9).expect("stale generation"),
    );
    let results = runtime.execute_durable_commit_group(vec![
        (
            group_batch(
                branch,
                b"rejected-a".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Standard,
            ),
            stale_guard,
        ),
        (
            group_batch(
                branch,
                b"rejected-b".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Standard,
            ),
            stale_guard,
        ),
    ]);

    assert_eq!(results.len(), 2);
    for result in &results {
        assert!(matches!(
            result.outcome.as_ref().expect_err("stale guard rejected"),
            LifecycleError::BranchGenerationMismatch { .. }
        ));
    }
    assert_eq!(runtime.visible_version(), visible_before);
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());

    // An empty (fully rejected) group leaves the gate span released: a solo
    // commit still admits.
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"after-empty-group", b"value"),
            generation_guard(),
        )
        .expect("solo commit after empty group");
}

#[test]
fn group_spanning_wal_rotation_commits_in_order() {
    let branch = branch_id(0xb5);
    let backend = leak_backend();
    // A segment small enough that the group's appends force at least one
    // rotation mid-group.
    let mut runtime =
        open_group_runtime(branch, backend, StorageMode::DurableLocalStandard, 4 * 1024);

    let members = (0..4)
        .map(|index| {
            (
                group_batch(
                    branch,
                    format!("rotating-{index}").into_bytes(),
                    vec![0xab; 2 * 1024],
                    CommitDurabilityMode::Standard,
                ),
                generation_guard(),
            )
        })
        .collect();
    let results = runtime.execute_durable_commit_group(members);

    let versions: Vec<_> = results
        .iter()
        .map(|result| {
            result
                .outcome
                .as_ref()
                .expect("member commit across rotation")
                .commit_version()
                .expect("mutating outcome version")
        })
        .collect();
    for pair in versions.windows(2) {
        assert_eq!(pair[1].as_u64(), pair[0].as_u64() + 1);
    }
    assert_eq!(runtime.visible_version(), versions[3]);
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());
}

#[test]
fn always_mode_group_of_one_matches_solo_and_groups_ack_always() {
    let branch = branch_id(0xb6);

    let solo_backend = leak_backend();
    let mut solo_runtime = open_group_runtime(
        branch,
        solo_backend,
        StorageMode::DurableLocalAlways,
        WalServiceConfig::default().segment_size(),
    );
    let solo_outcome = solo_runtime
        .execute_durable_commit(
            group_batch(
                branch,
                b"always-anchor".to_vec(),
                b"value".to_vec(),
                CommitDurabilityMode::Always,
            ),
            generation_guard(),
        )
        .expect("solo always commit");
    assert_eq!(solo_outcome.durability(), CommitDurabilityClass::Always);

    let group_backend = leak_backend();
    let mut group_runtime = open_group_runtime(
        branch,
        group_backend,
        StorageMode::DurableLocalAlways,
        WalServiceConfig::default().segment_size(),
    );
    let mut results = group_runtime.execute_durable_commit_group(vec![(
        group_batch(
            branch,
            b"always-anchor".to_vec(),
            b"value".to_vec(),
            CommitDurabilityMode::Always,
        ),
        generation_guard(),
    )]);
    let group_outcome = results
        .pop()
        .expect("one member result")
        .outcome
        .expect("group-of-1 always commit");
    assert_eq!(group_outcome.durability(), CommitDurabilityClass::Always);
    assert_eq!(
        group_outcome.visibility_facts(),
        solo_outcome.visibility_facts()
    );
    assert_eq!(
        group_backend.object_snapshot(),
        solo_backend.object_snapshot()
    );

    // A larger Always group: one covering fsync, every member acked Always,
    // contiguous versions.
    let members = (0..3)
        .map(|index| {
            (
                group_batch(
                    branch,
                    format!("always-{index}").into_bytes(),
                    vec![0x11; 16],
                    CommitDurabilityMode::Always,
                ),
                generation_guard(),
            )
        })
        .collect();
    let group_results = group_runtime.execute_durable_commit_group(members);
    let versions: Vec<_> = group_results
        .iter()
        .map(|result| {
            let outcome = result.outcome.as_ref().expect("always member commit");
            assert_eq!(outcome.durability(), CommitDurabilityClass::Always);
            outcome.commit_version().expect("mutating outcome version")
        })
        .collect();
    for pair in versions.windows(2) {
        assert_eq!(pair[1].as_u64(), pair[0].as_u64() + 1);
    }
    assert_eq!(group_runtime.visible_version(), versions[2]);
}

// ---------------------------------------------------------------------------
// Group-boundary crash sweeps (BS5.2d): the phase split makes each crash
// window deterministically constructible — run phase 1, "crash" (drop the
// runtime without phase 2), reopen over the same backend, and assert replay
// reconciles. Sync failures inject through phase 2's explicit sync result.
// ---------------------------------------------------------------------------

fn group_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "commit-group",
        StorageSpaceId::engine(0x36).expect("engine storage space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn row_present(
    runtime: &LifecycleDurableLocalRuntime<'static, CommitManualTimestampSource>,
    branch: BranchId,
    user_key: &[u8],
) -> bool {
    runtime
        .branch_catalog()
        .branch_state(branch)
        .expect("branch state")
        .capture_read_view()
        .expect("read view")
        .latest(&group_key(branch, user_key))
        .expect("read row")
        .is_some()
}

fn always_members(
    branch: BranchId,
    keys: &[&[u8]],
) -> Vec<(CommitBatch, CommitBranchGenerationGuard)> {
    keys.iter()
        .map(|key| {
            (
                group_batch(
                    branch,
                    key.to_vec(),
                    b"crash-sweep-value".to_vec(),
                    CommitDurabilityMode::Always,
                ),
                generation_guard(),
            )
        })
        .collect()
}

#[test]
fn group_crash_before_covering_sync_replays_all_members_on_reopen() {
    let branch = branch_id(0xb7);
    let backend = leak_backend();
    {
        let mut runtime = open_group_runtime(
            branch,
            backend,
            StorageMode::DurableLocalAlways,
            WalServiceConfig::default().segment_size(),
        );
        let in_flight = runtime.execute_durable_commit_group_begin(always_members(
            branch,
            &[b"crash-a", b"crash-b", b"crash-c"],
        ));
        assert!(
            in_flight.ticket().is_some(),
            "Always group captures a ticket"
        );
        // Crash window: appends + applies landed, the covering fsync never
        // ran, phase 2 never published. Nothing was acked.
        assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
        drop(in_flight);
        drop(runtime);
    }

    // Reopen: the appended records replay idempotently and become visible —
    // durable-without-ack is legal; the reverse (acked without durability)
    // is what the protocol forbids, and nothing here was acked.
    let runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalAlways,
        WalServiceConfig::default().segment_size(),
    );
    assert!(row_present(&runtime, branch, b"crash-a"));
    assert!(row_present(&runtime, branch, b"crash-b"));
    assert!(row_present(&runtime, branch, b"crash-c"));
    assert_eq!(runtime.visible_version(), CommitVersion::new(3));
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());
}

#[test]
fn group_crash_with_torn_tail_replays_the_complete_prefix() {
    let branch = branch_id(0xb8);
    let backend = leak_backend();
    {
        let mut runtime = open_lossy_group_runtime(branch, backend);
        let in_flight = runtime.execute_durable_commit_group_begin(always_members(
            branch,
            &[b"torn-a", b"torn-b", b"torn-c"],
        ));
        drop(in_flight);
        drop(runtime);
    }

    // Tear the WAL tail mid-record: the crash happened before any covering
    // fsync, so an arbitrary suffix of the group's appends may be lost.
    let segment = crate::layout::ObjectLayout::wal_segment(1).expect("segment one");
    let backend_dyn: &dyn crate::backend::Backend = backend;
    let bytes = backend_dyn.read_object(&segment).expect("read WAL segment");
    let torn = bytes[..bytes.len() - 9].to_vec();
    backend_dyn
        .write_object(&segment, &torn)
        .expect("write torn WAL segment");

    // Lossy reopen: replay applies every COMPLETE record (all-or-nothing per
    // record via CRC framing) and stops at the torn tail.
    let runtime = open_lossy_group_runtime(branch, backend);
    assert!(row_present(&runtime, branch, b"torn-a"));
    assert!(row_present(&runtime, branch, b"torn-b"));
    assert!(
        !row_present(&runtime, branch, b"torn-c"),
        "the torn final record must not survive"
    );
    assert_eq!(runtime.visible_version(), CommitVersion::new(2));
}

fn open_lossy_group_runtime(
    branch: BranchId,
    backend: &'static CheckpointTestBackend,
) -> LifecycleDurableLocalRuntime<'static, CommitManualTimestampSource> {
    let lifecycle_config = LifecycleConfig::new(
        1024,
        16,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .expect("lossy lifecycle config");
    let request = LifecycleDurableLocalOpenRequest::new(
        StorageOpenPlan::new(
            StorageMode::DurableLocalAlways,
            LifecycleCodecId::identity(),
            RecoveryStrictness::AllowExplicitLossyFallback,
            lifecycle_config,
        )
        .expect("open plan"),
        DATABASE_ID,
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default(),
    )
    .expect("durable request");
    let mut shell = LifecycleDurableLocalShell::assemble(
        request,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(9_000)),
    )
    .expect("durable shell");
    let recovery_request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .expect("recovery outcome");
    shell.complete_recovery(&recovery).expect("runtime")
}

#[test]
fn group_sync_failure_records_range_fact_and_reopen_reconciles() {
    let branch = branch_id(0xb9);
    let backend = leak_backend();
    {
        let mut runtime = open_group_runtime(
            branch,
            backend,
            StorageMode::DurableLocalAlways,
            WalServiceConfig::default().segment_size(),
        );
        let in_flight =
            runtime.execute_durable_commit_group_begin(always_members(branch, &[b"sf-a", b"sf-b"]));
        let results = runtime.execute_durable_commit_group_finish(
            in_flight,
            Some(Err(crate::service::WalServiceError::InvalidConfig {
                field: "segment_size",
            })),
        );

        // Every member reports durability-uncertain; nothing published; the
        // widened range fact gates further commits.
        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(matches!(
                result.outcome.as_ref().expect_err("member fails"),
                LifecycleError::LowerLayer { .. } | LifecycleError::InvalidLifecycleState { .. }
            ));
        }
        assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
        let fact = runtime
            .unresolved_durable()
            .expect("gate readable")
            .expect("range fact recorded");
        assert_eq!(fact.first_commit_version(), CommitVersion::new(1));
        assert_eq!(fact.commit_version(), CommitVersion::new(2));
        let follow_up = runtime
            .execute_durable_commit(
                group_batch(
                    branch,
                    b"blocked".to_vec(),
                    b"value".to_vec(),
                    CommitDurabilityMode::Always,
                ),
                generation_guard(),
            )
            .expect_err("gate blocks commits until reconciled");
        assert!(matches!(
            follow_up,
            LifecycleError::LowerLayer { .. } | LifecycleError::TimelineRecoveryMismatch { .. }
        ));
        drop(runtime);
    }

    // Reopen: the records were appended (the memory backend retains them);
    // replay reconciles the whole range and the rows become visible.
    let runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalAlways,
        WalServiceConfig::default().segment_size(),
    );
    assert!(row_present(&runtime, branch, b"sf-a"));
    assert!(row_present(&runtime, branch, b"sf-b"));
    assert_eq!(runtime.visible_version(), CommitVersion::new(2));
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());
}

#[test]
fn pipelined_groups_publish_out_of_order_and_fatal_above_does_not_block_below() {
    let branch = branch_id(0xba);
    let backend = leak_backend();
    let mut runtime = open_group_runtime(
        branch,
        backend,
        StorageMode::DurableLocalAlways,
        WalServiceConfig::default().segment_size(),
    );

    // Two groups in flight at once (multi-admission): A holds versions 1-2,
    // B holds versions 3-4.
    let group_a =
        runtime.execute_durable_commit_group_begin(always_members(branch, &[b"pa-1", b"pa-2"]));
    let group_b =
        runtime.execute_durable_commit_group_begin(always_members(branch, &[b"pb-1", b"pb-2"]));

    // B settles FIRST (out of order): its publish exposes A's applied rows —
    // safe, because B's covering sync also covered A's earlier appends.
    let results_b = runtime.execute_durable_commit_group_finish(group_b, Some(Ok(())));
    assert!(results_b.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(runtime.visible_version(), CommitVersion::new(4));

    // A then settles below the already-published bound: a no-op publish, not
    // a regression, and its members ack normally.
    let results_a = runtime.execute_durable_commit_group_finish(group_a, Some(Ok(())));
    assert!(results_a.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(runtime.visible_version(), CommitVersion::new(4));
    assert!(runtime
        .unresolved_durable()
        .expect("gate readable")
        .is_none());

    // Now the fatal-ordering rule: C (5-6) and D (7-8) in flight; D fails its
    // sync and records a fact at 7-8. C's range ends BELOW the fact, so C may
    // still publish; commits after the fact are blocked.
    let group_c =
        runtime.execute_durable_commit_group_begin(always_members(branch, &[b"pc-1", b"pc-2"]));
    let group_d =
        runtime.execute_durable_commit_group_begin(always_members(branch, &[b"pd-1", b"pd-2"]));
    let results_d = runtime.execute_durable_commit_group_finish(
        group_d,
        Some(Err(crate::service::WalServiceError::InvalidConfig {
            field: "segment_size",
        })),
    );
    assert!(results_d.iter().all(|result| result.outcome.is_err()));
    let fact = runtime
        .unresolved_durable()
        .expect("gate readable")
        .expect("fact for D");
    assert_eq!(fact.first_commit_version(), CommitVersion::new(7));

    let results_c = runtime.execute_durable_commit_group_finish(group_c, Some(Ok(())));
    assert!(
        results_c.iter().all(|result| result.outcome.is_ok()),
        "a fact strictly above C's range must not fail C"
    );
    assert_eq!(runtime.visible_version(), CommitVersion::new(6));
}
