use super::*;
use crate::branch::{
    BranchHistoryOptions, BranchLocalState, BranchReadBound, BranchReadView, BranchRuntimeConfig,
    BranchScanBounds,
};
use crate::row::StorageRow;

#[test]
fn cache_commit_applies_user_and_timeline_rows_and_publishes_visible_version() {
    let branch = branch_id(21);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let key = physical_key(branch, 0x20, b"alpha".to_vec());
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let outcome = fixture.execute(batch).expect("cache commit succeeds");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Visible);
    assert_eq!(outcome.phase(), CommitPhase::Visible);
    assert_eq!(outcome.durability(), CommitDurabilityClass::NotDurable);
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(1)));
    assert_eq!(
        outcome.commit_timestamp(),
        Some(Timestamp::from_micros(1_000))
    );
    assert_eq!(outcome.mutation_counts().puts(), 1);
    assert_eq!(outcome.mutation_counts().deletes(), 0);
    assert_eq!(
        outcome.mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(
        outcome.visibility_facts(),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            None,
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
        )
        .expect("visible cache facts")
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(1));

    let view = fixture.state.capture_read_view().expect("read view");
    let visible = view.latest(&key).expect("latest read").expect("row");
    assert_eq!(visible.row().commit_version(), CommitVersion::new(1));
    assert_eq!(
        visible.row().commit_timestamp(),
        Timestamp::from_micros(1_000)
    );
    assert_eq!(visible.row().value(), b"value");

    let timeline = timeline_view(&view, branch);
    assert_eq!(
        timeline
            .version_at_or_before(Timestamp::from_micros(1_000))
            .matched_version(),
        Some(CommitVersion::new(1))
    );
    assert_eq!(
        timeline.timestamp_for_version(CommitVersion::new(1)),
        Some(Timestamp::from_micros(1_000))
    );
}

#[test]
fn cache_commit_delete_installs_tombstone_and_hides_latest() {
    let branch = branch_id(34);
    let key = physical_key(branch, 0x20, b"deleted".to_vec());
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1_000),
        Timestamp::EPOCH,
        b"old".to_vec(),
    ));
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(1_000));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::delete(key.clone())],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let outcome = fixture.execute(batch).expect("delete commit succeeds");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(2)));
    let view = fixture.state.capture_read_view().expect("read view");
    assert!(view.latest(&key).expect("latest read").is_none());
    let history = view
        .history(&key, BranchHistoryOptions::all().include_tombstones(true))
        .expect("history");
    assert_eq!(history.len(), 2);
    assert!(history[0].row().is_tombstone());
    assert_eq!(history[0].row().commit_version(), CommitVersion::new(2));
    assert_eq!(history[1].row().value(), b"old");
}

#[test]
fn cache_commit_mixed_batch_spans_multiple_storage_spaces_atomically() {
    let branch = branch_id(35);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let alpha = physical_key(branch, 0x20, b"alpha".to_vec());
    let beta = physical_key(branch, 0x21, b"beta".to_vec());
    let gamma = physical_key(branch, 0x22, b"gamma".to_vec());
    let batch = mutating_batch(
        branch,
        vec![
            CommitMutation::put(
                alpha.clone(),
                b"a".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(beta.clone()),
            CommitMutation::put(
                gamma.clone(),
                b"g".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let outcome = fixture.execute(batch).expect("mixed commit succeeds");

    assert_eq!(outcome.mutation_counts().puts(), 2);
    assert_eq!(outcome.mutation_counts().deletes(), 1);
    assert_eq!(fixture.state.active_row_count(), 5);
    let view = fixture.state.capture_read_view().expect("read view");
    assert_eq!(
        view.latest(&alpha)
            .expect("alpha read")
            .expect("alpha")
            .row()
            .value(),
        b"a"
    );
    assert!(view.latest(&beta).expect("beta read").is_none());
    assert_eq!(
        view.latest(&gamma)
            .expect("gamma read")
            .expect("gamma")
            .row()
            .value(),
        b"g"
    );
}

#[test]
fn cache_commit_rejects_durable_modes_before_allocation_or_mutation() {
    for durability in [CommitDurabilityMode::Standard, CommitDurabilityMode::Always] {
        let branch = branch_id(22);
        let options = CommitBatchOptions::new(
            durability,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        );
        let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
        let batch = mutating_batch(
            branch,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"durability".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
            CommitValidationFacts::empty(),
            options,
        );

        assert_eq!(
            fixture.execute(batch),
            Err(CommitRuntimeError::DurabilityUnavailable {
                reason: "cache commit executor requires cache durability mode",
            })
        );
        assert_eq!(
            fixture.allocator.version_allocator().last_allocated(),
            CommitVersion::ZERO
        );
        assert_eq!(fixture.state.active_row_count(), 0);
        assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    }
}

#[test]
fn cache_commit_rejects_missing_deleted_and_stale_generation_before_allocation() {
    let branch = branch_id(36);
    let batch = || {
        mutating_batch(
            branch,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"admission".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        )
    };

    let mut missing = CacheFixture::new(branch, CommitRuntimeConfig::default());
    missing.registry = CommitBranchRegistry::new();
    assert_eq!(
        missing.execute(batch()),
        Err(CommitRuntimeError::BranchNotFound { branch_id: branch })
    );
    assert_eq!(
        missing.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(missing.state.active_row_count(), 0);
    assert_eq!(missing.visible.visible_version(), CommitVersion::ZERO);

    let mut deleted = CacheFixture::new(branch, CommitRuntimeConfig::default());
    deleted.registry.mark_deleted(branch).expect("mark deleted");
    assert_eq!(
        deleted.execute(batch()),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleted",
        })
    );
    assert_eq!(
        deleted.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(deleted.state.active_row_count(), 0);
    assert_eq!(deleted.visible.visible_version(), CommitVersion::ZERO);

    let mut stale = CacheFixture::new(branch, CommitRuntimeConfig::default());
    assert_eq!(
        stale.execute_with_generation(
            batch(),
            CommitBranchGeneration::new(2).expect("stale generation")
        ),
        Err(CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 1,
            actual: 2,
        })
    );
    assert_eq!(
        stale.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(stale.state.active_row_count(), 0);
    assert_eq!(stale.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn cache_commit_rejects_read_only_batch_without_allocation_or_mutation() {
    let branch = branch_id(37);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let batch = CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "cache commit executor requires mutating batch",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
}

#[test]
fn cache_commit_preserves_explicit_timestamp_for_user_and_timeline_rows() {
    let branch = branch_id(38);
    let key = physical_key(branch, 0x20, b"explicit-time".to_vec());
    let timestamp = Timestamp::from_micros(8_888);
    let options = CommitBatchOptions::new(
        CommitDurabilityMode::Cache,
        CommitConflictValidationMode::Validate,
        CommitDuplicateKeyPolicy::Reject,
        CommitTimestampPolicy::Explicit(timestamp),
        CommitOrigin::StorageRuntime,
    );
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        options,
    );

    let outcome = fixture.execute(batch).expect("explicit timestamp commit");

    assert_eq!(outcome.commit_timestamp(), Some(timestamp));
    let view = fixture.state.capture_read_view().expect("read view");
    let row = view.latest(&key).expect("latest read").expect("row");
    assert_eq!(row.row().commit_timestamp(), timestamp);
    let timeline = timeline_view(&view, branch);
    for entry in timeline.entries() {
        assert_eq!(entry.commit_timestamp(), timestamp);
    }
}

#[test]
fn cache_commit_conflict_rejects_before_allocation_or_mutation() {
    let branch = branch_id(23);
    let key = physical_key(branch, 0x20, b"conflict".to_vec());
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture
        .state
        .append_committed_row(StorageRow::put(
            key.clone(),
            CommitVersion::new(7),
            Timestamp::from_micros(700),
            Timestamp::EPOCH,
            b"existing".to_vec(),
        ))
        .expect("seed row");
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(7));
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"new".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key.clone(),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        CommitBatchOptions::default(),
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::CommitConflict { conflict })
            if conflict.kind() == CommitConflictKind::ReadSet
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 1);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(7));
}

#[test]
fn cache_commit_guard_releases_after_conflict_failure() {
    let branch = branch_id(39);
    let key = physical_key(branch, 0x20, b"guard-conflict".to_vec());
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1_000),
        Timestamp::EPOCH,
        b"existing".to_vec(),
    ));
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(1_000));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));
    let conflict = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"new".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitBatchOptions::default(),
    );

    assert!(matches!(
        fixture.execute(conflict),
        Err(CommitRuntimeError::CommitConflict { .. })
    ));
    let retry = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"retry".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );
    assert!(fixture.execute(retry).is_ok());
}

#[test]
fn cache_commit_branch_admission_failures_reject_before_allocation() {
    let branch = branch_id(31);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture
        .registry
        .mark_deleting(branch)
        .expect("mark branch deleting");
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"deleting".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchNotWritable {
            branch_id: branch,
            reason: "branch is deleting",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn cache_commit_guard_contention_rejects_before_allocation() {
    let branch = branch_id(32);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let held_guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("held guard");
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"guard".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    drop(held_guard);

    let retry = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"guard".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );
    assert!(fixture.execute(retry).is_ok());
}

#[test]
fn cache_commit_guard_contention_serializes_conflict_validation_window() {
    let branch = branch_id(40);
    let key = physical_key(branch, 0x20, b"guarded-conflict-window".to_vec());
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1_000),
        Timestamp::EPOCH,
        b"existing".to_vec(),
    ));
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(1_000));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));
    let held_guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("held guard");
    let stale_validation = CommitValidationFacts::new(
        vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
        Vec::new(),
    );
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"new".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        stale_validation,
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.state.active_row_count(), 1);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(1));
    drop(held_guard);
}

#[test]
fn cache_commit_row_limit_counts_timeline_rows_after_allocation_without_apply() {
    let branch = branch_id(24);
    let config =
        CommitRuntimeConfig::new(1, 1, 2, CommitReadOnlyDiagnostics::Enabled).expect("config");
    let mut fixture = CacheFixture::new(branch, config);
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"limit".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "commit row count exceeds configured limit",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn cache_commit_l6_apply_failure_releases_guard_without_visible_publication() {
    let branch = branch_id(37);
    let (registry, guard_set, mut allocator, durable_gate) = injected_cache_runtime_parts(branch);
    let mut state = FailingCacheApplyTarget::new(branch, true);
    let mut visible = FailingCacheVisiblePublisher::new(false);
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"cache-apply-failure".to_vec()),
            b"super-secret-cache-apply-value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let error = execute_injected_cache_commit(
        &registry,
        &guard_set,
        &mut allocator,
        &mut state,
        &mut visible,
        &durable_gate,
        batch,
    )
    .expect_err("cache apply failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::BranchRuntime,
            reason: "injected cache branch apply failure",
            ..
        }
    ));
    assert!(!format!("{error:?}").contains("super-secret-cache-apply-value"));
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(state.state.active_row_count(), 0);
    assert_eq!(visible.publish_attempts, 0);
    assert_eq!(visible.tracker.visible_version(), CommitVersion::ZERO);
    assert_eq!(durable_gate.unresolved().expect("gate read"), None);
    let branch_guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("branch guard released after cache apply failure");
    drop(branch_guard);
}

#[test]
fn cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard() {
    let branch = branch_id(38);
    let key = physical_key(branch, 0x20, b"cache-visible-failure".to_vec());
    let (registry, guard_set, mut allocator, durable_gate) = injected_cache_runtime_parts(branch);
    let mut state = FailingCacheApplyTarget::new(branch, false);
    let mut visible = FailingCacheVisiblePublisher::new(true);
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"super-secret-cache-visible-value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let error = execute_injected_cache_commit(
        &registry,
        &guard_set,
        &mut allocator,
        &mut state,
        &mut visible,
        &durable_gate,
        batch,
    )
    .expect_err("cache visible failure rejects");

    assert_eq!(
        error,
        CommitRuntimeError::AppliedButNotVisible {
            branch_id: branch,
            commit_version: CommitVersion::new(1),
            reason: "injected cache visible publication failure",
        }
    );
    assert!(!format!("{error:?}").contains("super-secret-cache-visible-value"));
    assert_eq!(
        state.state.active_row_count(),
        1 + CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(visible.publish_attempts, 1);
    assert_eq!(visible.tracker.visible_version(), CommitVersion::ZERO);
    assert_eq!(durable_gate.unresolved().expect("gate read"), None);
    assert_eq!(
        state
            .state
            .capture_read_view()
            .expect("read view")
            .latest(&key)
            .expect("latest read")
            .expect("applied row")
            .row()
            .value(),
        b"super-secret-cache-visible-value"
    );
    let branch_guard = guard_set
        .try_acquire_branch_guard(branch)
        .expect("branch guard released after cache visible failure");
    drop(branch_guard);

    visible.fail_publish = false;
    let follow_on = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"cache-visible-follow-on".to_vec()),
            b"follow-on".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );
    let follow_on_error = execute_injected_cache_commit(
        &registry,
        &guard_set,
        &mut allocator,
        &mut state,
        &mut visible,
        &durable_gate,
        follow_on,
    )
    .expect_err("applied-above-visible branch rejects follow-on");

    assert_eq!(
        follow_on_error,
        CommitRuntimeError::InvalidCommitState {
            reason: "branch has applied rows above current visible version",
        }
    );
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(
        state.state.active_row_count(),
        1 + CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(visible.publish_attempts, 1);
}

#[test]
fn cache_commit_rejects_unpublished_branch_rows_before_allocation() {
    let branch = branch_id(33);
    let hidden_key = physical_key(branch, 0x20, b"hidden".to_vec());
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture
        .state
        .append_committed_row(StorageRow::put(
            hidden_key.clone(),
            CommitVersion::new(2),
            Timestamp::from_micros(2_000),
            Timestamp::EPOCH,
            b"hidden".to_vec(),
        ))
        .expect("seed unpublished row");
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"new".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "branch has applied rows above current visible version",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    assert!(fixture
        .state
        .capture_read_view()
        .expect("read view")
        .latest(&hidden_key)
        .expect("latest read")
        .is_some());
}

#[test]
fn cache_commit_rejects_unresolved_durable_gate_before_allocation() {
    let branch = branch_id(34);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture
        .durable_gate
        .record_unresolved(unresolved_durable_fact(branch, CommitVersion::new(7)))
        .expect("record unresolved durable fact");
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"blocked-by-durable".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: blocked_branch,
            commit_version,
            ..
        }) if blocked_branch == branch && commit_version == CommitVersion::new(7)
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    let _guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("branch guard released after blocked cache commit");
}

#[test]
fn cache_commit_rejects_any_unresolved_durable_gate_before_allocation() {
    let blocked_branch = branch_id(35);
    let target_branch = branch_id(36);
    let mut fixture = CacheFixture::new(target_branch, CommitRuntimeConfig::default());
    fixture
        .durable_gate
        .record_unresolved(unresolved_durable_fact(
            blocked_branch,
            CommitVersion::new(8),
        ))
        .expect("record unresolved durable fact");
    let batch = mutating_batch(
        target_branch,
        vec![CommitMutation::put(
            physical_key(target_branch, 0x20, b"blocked-by-other-branch".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id,
            commit_version,
            ..
        }) if branch_id == blocked_branch && commit_version == CommitVersion::new(8)
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    let _guard = fixture
        .guard_set
        .try_acquire_branch_guard(target_branch)
        .expect("target branch guard was never retained");
}

#[test]
fn cache_commit_rejects_allocator_visible_mismatch_before_apply() {
    let branch = branch_id(25);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(9));
    let key = physical_key(branch, 0x20, b"visible-regression".to_vec());
    let batch = mutating_batch(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "allocated commit version must be greater than current visible version",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(9));
    assert_eq!(fixture.state.active_row_count(), 0);
    let branch_guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("branch guard released after visible mismatch");
    drop(branch_guard);
    assert!(fixture
        .state
        .capture_read_view()
        .expect("read view")
        .latest(&key)
        .expect("latest read")
        .is_none());
}

#[test]
fn cache_commit_rejects_branch_state_mismatch_before_allocation() {
    let branch = branch_id(26);
    let other = branch_id(27);
    let mut fixture = CacheFixture::new(branch, CommitRuntimeConfig::default());
    let batch = mutating_batch(
        other,
        vec![CommitMutation::put(
            physical_key(other, 0x20, b"wrong-branch".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchMismatch {
            expected: other,
            actual: branch,
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.state.active_row_count(), 0);
}

#[test]
fn cache_commit_row_preparation_uses_one_stamp_for_user_and_timeline_rows() {
    let branch = branch_id(28);
    let batch = mutating_batch(
        branch,
        vec![
            CommitMutation::put(
                physical_key(branch, 0x20, b"a".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(physical_key(branch, 0x20, b"b".to_vec())),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("validated batch");
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(5), Timestamp::from_micros(55)).expect("stamp");

    let rows = CacheCommitRows::prepare(&batch, stamp, &CommitRuntimeConfig::default())
        .expect("cache rows");

    assert_eq!(rows.stamp(), stamp);
    assert_eq!(rows.user_rows().rows().len(), 2);
    assert_eq!(
        rows.timeline_rows().entry(),
        CommitTimelineEntry::from_stamp(stamp).expect("timeline entry")
    );
    assert_eq!(rows.mutation_counts().puts(), 1);
    assert_eq!(rows.mutation_counts().deletes(), 1);
    assert_eq!(
        rows.mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    for row in rows.combined_rows() {
        assert_eq!(row.commit_version(), stamp.commit_version());
        assert_eq!(row.commit_timestamp(), stamp.commit_timestamp());
    }
}

#[test]
fn branch_atomic_append_rejects_partial_duplicate_batch_without_mutating_state() {
    let branch = branch_id(29);
    let mut state = BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state");
    let key = physical_key(branch, 0x20, b"duplicate".to_vec());
    let row = StorageRow::put(
        key,
        CommitVersion::new(3),
        Timestamp::from_micros(30),
        Timestamp::EPOCH,
        b"value".to_vec(),
    );

    assert!(matches!(
        state.append_committed_rows_atomically(vec![row.clone(), row]),
        Err(crate::branch::BranchRuntimeError::TableRuntime { .. })
    ));
    assert_eq!(state.active_row_count(), 0);
    assert_eq!(state.max_commit_version(), None);
}

#[test]
fn branch_atomic_append_reports_batch_state_after_success() {
    let branch = branch_id(30);
    let mut state = BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state");
    let first = StorageRow::put(
        physical_key(branch, 0x20, b"first".to_vec()),
        CommitVersion::new(3),
        Timestamp::from_micros(30),
        Timestamp::EPOCH,
        b"first".to_vec(),
    );
    let second = StorageRow::tombstone(
        physical_key(branch, 0x20, b"second".to_vec()),
        CommitVersion::new(4),
        Timestamp::from_micros(40),
    );

    let outcome = state
        .append_committed_rows_atomically(vec![first, second])
        .expect("atomic append");

    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.appended_rows(), 2);
    assert_eq!(outcome.active_rows(), 2);
    assert!(outcome.approximate_active_bytes() > 0);
    assert_eq!(outcome.max_commit_version(), Some(CommitVersion::new(4)));
    assert_eq!(state.active_row_count(), 2);
}

struct CacheFixture {
    config: CommitRuntimeConfig,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    state: BranchLocalState,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
}

#[derive(Debug)]
struct FailingCacheApplyTarget {
    state: BranchLocalState,
    fail_append: bool,
}

impl FailingCacheApplyTarget {
    fn new(branch: BranchId, fail_append: bool) -> Self {
        Self {
            state: BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state"),
            fail_append,
        }
    }
}

impl CommitBranchApplyTarget for FailingCacheApplyTarget {
    fn branch_id(&self) -> BranchId {
        self.state.branch_id()
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        self.state.max_commit_version()
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<BranchReadView> {
        self.state.capture_read_view().map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "branch read view capture failed",
                source,
            )
        })
    }

    fn append_committed_rows_atomically(
        &mut self,
        rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()> {
        if self.fail_append {
            return Err(CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "injected cache branch apply failure",
                InjectedCacheApplySource,
            ));
        }
        self.state
            .append_committed_rows_atomically(rows)
            .map(|_| ())
            .map_err(|source| {
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::BranchRuntime,
                    "branch state rejected commit rows",
                    source,
                )
            })
    }
}

#[derive(Debug)]
struct FailingCacheVisiblePublisher {
    tracker: VisibleVersionTracker,
    fail_publish: bool,
    publish_attempts: usize,
}

impl FailingCacheVisiblePublisher {
    const fn new(fail_publish: bool) -> Self {
        Self {
            tracker: VisibleVersionTracker::new(CommitVersion::ZERO),
            fail_publish,
            publish_attempts: 0,
        }
    }
}

impl CommitVisiblePublisher for FailingCacheVisiblePublisher {
    fn visible_version(&self) -> CommitVersion {
        self.tracker.visible_version()
    }

    fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        self.publish_attempts = self.publish_attempts.saturating_add(1);
        if self.fail_publish {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "injected cache visible publication failure",
            });
        }
        self.tracker.publish_from_facts(facts)
    }
}

#[derive(Debug)]
struct InjectedCacheApplySource;

impl std::fmt::Display for InjectedCacheApplySource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("injected cache apply source")
    }
}

impl std::error::Error for InjectedCacheApplySource {}

fn injected_cache_runtime_parts(
    branch: BranchId,
) -> (
    CommitBranchRegistry,
    CommitBranchGuardSet,
    CommitFactAllocator<CommitManualTimestampSource>,
    CommitUnresolvedDurableGate,
) {
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(
            branch,
            CommitBranchGeneration::new(1).expect("branch generation"),
        )
        .expect("register branch");
    let allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    );

    (
        registry,
        CommitBranchGuardSet::new(),
        allocator,
        CommitUnresolvedDurableGate::new(),
    )
}

fn execute_injected_cache_commit(
    registry: &CommitBranchRegistry,
    guard_set: &CommitBranchGuardSet,
    allocator: &mut CommitFactAllocator<CommitManualTimestampSource>,
    state: &mut FailingCacheApplyTarget,
    visible: &mut FailingCacheVisiblePublisher,
    durable_gate: &CommitUnresolvedDurableGate,
    batch: CommitBatch,
) -> CommitRuntimeResult<CommitOutcome> {
    CommitCacheRuntime::new(
        &CommitRuntimeConfig::default(),
        registry,
        guard_set,
        allocator,
        state,
        visible,
        durable_gate,
    )
    .execute(
        batch,
        CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
    )
}

impl CacheFixture {
    fn new(branch: BranchId, config: CommitRuntimeConfig) -> Self {
        let mut registry = CommitBranchRegistry::new();
        registry
            .register_active(
                branch,
                CommitBranchGeneration::new(1).expect("branch generation"),
            )
            .expect("register branch");
        Self {
            config,
            registry,
            guard_set: CommitBranchGuardSet::new(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
            ),
            state: BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state"),
            visible: VisibleVersionTracker::default(),
            durable_gate: CommitUnresolvedDurableGate::new(),
        }
    }

    fn execute(&mut self, batch: CommitBatch) -> CommitRuntimeResult<CommitOutcome> {
        self.execute_with_generation(batch, CommitBranchGeneration::new(1).expect("generation"))
    }

    fn execute_with_generation(
        &mut self,
        batch: CommitBatch,
        generation: CommitBranchGeneration,
    ) -> CommitRuntimeResult<CommitOutcome> {
        CommitCacheRuntime::new(
            &self.config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.state,
            &mut self.visible,
            &self.durable_gate,
        )
        .execute(batch, CommitBranchGenerationGuard::exact(generation))
    }

    fn seed_visible_row(&mut self, row: StorageRow) {
        self.state.append_committed_row(row).expect("seed row");
    }

    fn catch_up_to(&mut self, version: CommitVersion, timestamp: Timestamp) {
        self.allocator.catch_up_to_recovered_version(version);
        self.allocator.catch_up_to_recovered_timestamp(timestamp);
    }
}

fn mutating_batch(
    branch: BranchId,
    mutations: Vec<CommitMutation>,
    validation: CommitValidationFacts,
    options: CommitBatchOptions,
) -> CommitBatch {
    CommitBatch::mutating(branch, mutations, validation, options)
}

fn timeline_view(view: &crate::branch::BranchReadView, branch: BranchId) -> CommitTimelineView {
    let bounds = BranchScanBounds::unbounded(
        branch,
        COMMIT_TIMELINE_SPACE,
        StorageSpaceId::COMMIT_TIMELINE,
    )
    .expect("timeline scan bounds");
    let rows = view
        .scan_range(&bounds, BranchReadBound::latest())
        .expect("timeline scan");
    CommitTimelineView::from_rows(
        branch,
        rows.iter().map(crate::branch::BranchVisibleRow::row),
    )
    .expect("timeline view")
}

fn unresolved_durable_fact(branch: BranchId, version: CommitVersion) -> CommitUnresolvedDurable {
    CommitUnresolvedDurable::durable_not_applied_with_facts(
        CommitStamp::new(branch, version, Timestamp::from_micros(7_000)).expect("stamp"),
        CommitDurabilityClass::Standard,
        "seed unresolved durable fact",
    )
    .expect("unresolved durable fact")
}
