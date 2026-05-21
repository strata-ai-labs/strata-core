use super::*;
use crate::branch::{BranchLocalState, BranchReadBound, BranchRuntimeConfig, BranchScanBounds};
use crate::row::StorageRow;
use std::error::Error as _;

#[test]
fn durable_standard_commit_appends_wal_record_then_applies_rows_and_publishes_visible() {
    let branch = branch_id(51);
    let key = physical_key(branch, 0x20, b"durable-standard".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    let outcome = fixture.execute(batch).expect("standard commit succeeds");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Visible);
    assert_eq!(outcome.phase(), CommitPhase::Visible);
    assert_eq!(outcome.durability(), CommitDurabilityClass::Standard);
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(1)));
    assert_eq!(
        outcome.commit_timestamp(),
        Some(Timestamp::from_micros(1_000))
    );
    assert_eq!(
        outcome.visibility_facts(),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
            Some(CommitVersion::new(1)),
        )
        .expect("durable visible facts")
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(1));
    assert_eq!(fixture.wal.records.len(), 1);
    let record = &fixture.wal.records[0];
    assert_eq!(record.branch_id(), branch);
    assert_eq!(record.commit_version(), CommitVersion::new(1));
    assert_eq!(record.commit_timestamp(), Timestamp::from_micros(1_000));
    assert_eq!(
        record.commit_payload().rows().len(),
        1 + CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(record.commit_payload().rows()[0].physical_key(), &key);
    assert_eq!(
        record.commit_payload().rows()[0].commit_version(),
        CommitVersion::new(1)
    );
    assert_record_rows_share_commit_facts(
        record,
        branch,
        CommitVersion::new(1),
        Timestamp::from_micros(1_000),
    );
    assert_payload_contains_timeline_rows(record);

    let view = fixture.state.capture_read_view().expect("read view");
    assert_eq!(
        view.latest(&key)
            .expect("latest read")
            .expect("row")
            .row()
            .value(),
        b"value"
    );
    let timeline = timeline_view(&view, branch);
    assert_eq!(
        timeline
            .version_at_or_before(Timestamp::from_micros(1_000))
            .matched_version(),
        Some(CommitVersion::new(1))
    );
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn durable_standard_commit_appends_through_real_l4_wal_service() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());
    let branch = branch_id(58);
    let key = physical_key(branch, 0x20, b"real-wal".to_vec());
    let timestamp = Timestamp::from_micros(1_000);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(
            branch,
            CommitBranchGeneration::new(1).expect("branch generation"),
        )
        .expect("register branch");
    let guard_set = CommitBranchGuardSet::new();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(timestamp),
    );
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("branch state");
    let mut visible = VisibleVersionTracker::default();
    let mut wal = crate::service::WalService::open(
        &backend,
        [0x58; 16],
        1,
        DurabilityPolicy::Standard,
        crate::service::WalServiceConfig::default(),
    )
    .expect("open real WAL service");
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    let outcome = CommitDurableRuntime::new(
        &CommitRuntimeConfig::default(),
        &registry,
        &guard_set,
        &mut allocator,
        &mut state,
        &mut visible,
        &mut wal,
    )
    .execute(
        batch,
        CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
    )
    .expect("durable commit through real WAL service");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Visible);
    assert_eq!(visible.visible_version(), CommitVersion::new(1));
    let read = wal.read_all().expect("read real WAL service");
    assert_eq!(read.records().len(), 1);
    assert_record_rows_share_commit_facts(
        &read.records()[0],
        branch,
        CommitVersion::new(1),
        timestamp,
    );
    assert_payload_contains_timeline_rows(&read.records()[0]);
    assert_eq!(
        state
            .capture_read_view()
            .expect("read view")
            .latest(&key)
            .expect("latest read")
            .expect("row")
            .row()
            .value(),
        b"value"
    );
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn durable_always_commit_appends_through_real_l4_wal_service() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = crate::backend::local_fs::LocalFsBackend::new(dir.path());
    let branch = branch_id(59);
    let key = physical_key(branch, 0x20, b"real-wal-always".to_vec());
    let timestamp = Timestamp::from_micros(1_000);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(
            branch,
            CommitBranchGeneration::new(1).expect("branch generation"),
        )
        .expect("register branch");
    let guard_set = CommitBranchGuardSet::new();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(timestamp),
    );
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("branch state");
    let mut visible = VisibleVersionTracker::default();
    let mut wal = crate::service::WalService::open(
        &backend,
        [0x59; 16],
        1,
        DurabilityPolicy::Always,
        crate::service::WalServiceConfig::default(),
    )
    .expect("open always WAL service");
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            key,
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    let outcome = CommitDurableRuntime::new(
        &CommitRuntimeConfig::default(),
        &registry,
        &guard_set,
        &mut allocator,
        &mut state,
        &mut visible,
        &mut wal,
    )
    .execute(
        batch,
        CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
    )
    .expect("always durable commit through real WAL service");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Visible);
    assert_eq!(outcome.durability(), CommitDurabilityClass::Always);
    let read = wal.read_all().expect("read always WAL service");
    assert_eq!(read.records().len(), 1);
    assert_record_rows_share_commit_facts(
        &read.records()[0],
        branch,
        CommitVersion::new(1),
        timestamp,
    );
    assert_payload_contains_timeline_rows(&read.records()[0]);
}

#[test]
fn durable_standard_delete_appends_tombstone_and_hides_latest() {
    let branch = branch_id(60);
    let key = physical_key(branch, 0x20, b"durable-delete".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(900),
        Timestamp::EPOCH,
        b"old".to_vec(),
    ));
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(900));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));

    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::delete(key.clone())],
        ))
        .expect("durable delete succeeds");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(2)));
    assert_eq!(outcome.mutation_counts().puts(), 0);
    assert_eq!(outcome.mutation_counts().deletes(), 1);
    assert_eq!(
        outcome.mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    let record = &fixture.wal.records[0];
    assert_eq!(record.commit_payload().rows()[0].physical_key(), &key);
    assert!(record.commit_payload().rows()[0].is_tombstone());
    assert_record_rows_share_commit_facts(
        record,
        branch,
        CommitVersion::new(2),
        Timestamp::from_micros(1_000),
    );
    assert!(fixture
        .state
        .capture_read_view()
        .expect("read view")
        .latest(&key)
        .expect("latest read")
        .is_none());
}

#[test]
fn durable_standard_mixed_batch_writes_user_rows_then_timeline_and_counts() {
    let branch = branch_id(61);
    let first = physical_key(branch, 0x20, b"mixed-first".to_vec());
    let deleted = physical_key(branch, 0x21, b"mixed-delete".to_vec());
    let second = physical_key(branch, 0x22, b"mixed-second".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );

    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![
                CommitMutation::put(
                    first.clone(),
                    b"first".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                ),
                CommitMutation::delete(deleted.clone()),
                CommitMutation::put(
                    second.clone(),
                    b"second".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                ),
            ],
        ))
        .expect("mixed durable commit succeeds");

    assert_eq!(outcome.mutation_counts().puts(), 2);
    assert_eq!(outcome.mutation_counts().deletes(), 1);
    assert_eq!(
        outcome.mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    let payload = fixture.wal.records[0].commit_payload().rows();
    assert_eq!(payload.len(), 3 + CommitTimelineRows::timeline_row_count());
    assert_eq!(payload[0].physical_key(), &first);
    assert!(!payload[0].is_tombstone());
    assert_eq!(payload[1].physical_key(), &deleted);
    assert!(payload[1].is_tombstone());
    assert_eq!(payload[2].physical_key(), &second);
    assert!(!payload[2].is_tombstone());
    assert!(payload[3..]
        .iter()
        .all(|row| row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE));
    let view = fixture.state.capture_read_view().expect("read view");
    assert!(view
        .latest(&first)
        .expect("first read")
        .is_some_and(|row| row.row().value() == b"first"));
    assert!(view.latest(&deleted).expect("deleted read").is_none());
    assert!(view
        .latest(&second)
        .expect("second read")
        .is_some_and(|row| row.row().value() == b"second"));
}

#[test]
fn durable_always_commit_requires_forced_wal_append_fact() {
    let branch = branch_id(52);
    let key = physical_key(branch, 0x20, b"durable-always".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::Succeed {
            forced_durable: true,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            key,
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    let outcome = fixture.execute(batch).expect("always commit succeeds");

    assert_eq!(outcome.durability(), CommitDurabilityClass::Always);
    assert_eq!(fixture.wal.records.len(), 1);
    assert_record_rows_share_commit_facts(
        &fixture.wal.records[0],
        branch,
        CommitVersion::new(1),
        Timestamp::from_micros(1_000),
    );
}

#[test]
fn durable_always_commit_rejects_unforced_success_before_l6_apply() {
    let branch = branch_id(53);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"unforced".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::DurabilityUncertain {
            branch_id: failed_branch,
            commit_version,
            reason: "always durability requires a forced WAL append",
            ..
        }) if failed_branch == branch && commit_version == CommitVersion::new(1)
    ));
    assert_eq!(fixture.wal.records.len(), 1);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    let _guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("guard can be reacquired after unforced durable failure");
}

#[test]
fn durable_read_only_batch_rejects_before_allocation_or_wal_append() {
    let branch = branch_id(62);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );

    assert_eq!(
        fixture.execute(CommitBatch::read_only_diagnostic(
            branch,
            CommitValidationFacts::empty(),
            CommitBatchOptions::new(
                CommitDurabilityMode::Standard,
                CommitConflictValidationMode::Validate,
                CommitDuplicateKeyPolicy::Reject,
                CommitTimestampPolicy::RuntimeGenerated,
                CommitOrigin::StorageRuntime,
            ),
        )),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "durable commit executor requires mutating batch",
        })
    );
    assert_unallocated_unattempted(&fixture);
}

#[test]
fn durable_commit_rejects_cache_mode_before_allocation_or_wal_append() {
    let branch = branch_id(54);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Cache,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"cache".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::DurabilityUnavailable {
            reason: "durable commit executor requires durable mode",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.wal.records.len(), 0);
    assert_eq!(fixture.state.active_row_count(), 0);
}

#[test]
fn durable_branch_mismatch_rejects_before_allocation_or_wal_append() {
    let branch = branch_id(69);
    let other = branch_id(70);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        other,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            physical_key(other, 0x20, b"wrong-branch".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchMismatch {
            expected: other,
            actual: branch,
        })
    );
    assert_unallocated_unattempted(&fixture);
}

#[test]
fn durable_generation_mismatch_rejects_before_allocation_or_wal_append() {
    let branch = branch_id(63);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"stale-generation".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert_eq!(
        fixture.execute_with_generation(
            batch,
            CommitBranchGeneration::new(2).expect("stale generation"),
        ),
        Err(CommitRuntimeError::BranchGenerationMismatch {
            branch_id: branch,
            expected: 1,
            actual: 2,
        })
    );
    assert_unallocated_unattempted(&fixture);
}

#[test]
fn durable_guard_contention_rejects_before_allocation_or_wal_append() {
    let branch = branch_id(64);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let held_guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("held guard");
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"guard-contention".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::BranchGuardUnavailable {
            branch_id: branch,
            reason: "branch commit guard is already active",
        })
    );
    assert_unallocated_unattempted(&fixture);
    drop(held_guard);
}

#[test]
fn durable_timestamp_source_failure_rejects_before_version_allocation_or_wal_append() {
    let branch = branch_id(71);
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(
            branch,
            CommitBranchGeneration::new(1).expect("branch generation"),
        )
        .expect("register branch");
    let guard_set = CommitBranchGuardSet::new();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        FailingTimestampSource,
    );
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("branch state");
    let mut visible = VisibleVersionTracker::default();
    let mut wal = RecordingWalAppender::new(
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );

    assert_eq!(
        CommitDurableRuntime::new(
            &CommitRuntimeConfig::default(),
            &registry,
            &guard_set,
            &mut allocator,
            &mut state,
            &mut visible,
            &mut wal,
        )
        .execute(
            durable_batch(
                branch,
                CommitDurabilityMode::Standard,
                vec![CommitMutation::put(
                    physical_key(branch, 0x20, b"timestamp-failure".to_vec()),
                    b"value".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                )],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        ),
        Err(CommitRuntimeError::TimestampUnavailable {
            reason: "injected timestamp failure",
            source: None,
        })
    );
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(wal.append_attempts, 0);
    assert_eq!(state.active_row_count(), 0);
    assert_eq!(visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn durable_conflict_rejects_before_allocation_or_wal_append() {
    let branch = branch_id(65);
    let key = physical_key(branch, 0x20, b"durable-conflict".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(900),
        Timestamp::EPOCH,
        b"existing".to_vec(),
    ));
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(900));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));
    let batch = durable_batch_with_validation(
        branch,
        CommitDurabilityMode::Standard,
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
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::CommitConflict { conflict })
            if conflict.kind() == CommitConflictKind::ReadSet
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.wal.append_attempts, 0);
    assert_eq!(fixture.state.active_row_count(), 1);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::new(1));
}

#[test]
fn durable_row_limit_failure_after_allocation_leaves_version_gap_without_wal_append() {
    let branch = branch_id(66);
    let config =
        CommitRuntimeConfig::new(1, 1, 2, CommitReadOnlyDiagnostics::Enabled).expect("config");
    let mut fixture = DurableFixture::new(
        branch,
        config,
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let key = physical_key(branch, 0x20, b"row-limit".to_vec());

    assert_eq!(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                key.clone(),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "commit row count exceeds configured limit",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.wal.append_attempts, 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);

    fixture.config = CommitRuntimeConfig::default();
    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                key,
                b"retry".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        ))
        .expect("retry after allocated row-limit failure succeeds");
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(2)));
}

#[test]
fn durable_version_overflow_rejects_before_wal_append() {
    let branch = branch_id(67);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    fixture.catch_up_to(CommitVersion::MAX, Timestamp::from_micros(1_000));

    assert_eq!(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"overflow".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        Err(CommitRuntimeError::VersionAllocatorOverflow {
            last_allocated: CommitVersion::MAX,
        })
    );
    assert_eq!(fixture.wal.append_attempts, 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn durable_commit_rejects_policy_mismatch_before_allocation_or_wal_append() {
    let branch = branch_id(55);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"mismatch".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert_eq!(
        fixture.execute(batch),
        Err(CommitRuntimeError::DurabilityUnavailable {
            reason: "durable commit executor requires matching WAL durability policy",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.wal.records.len(), 0);
    assert_eq!(fixture.state.active_row_count(), 0);
}

#[test]
fn durable_clean_wal_failure_leaves_no_visible_rows_but_allocation_may_gap() {
    let branch = branch_id(56);
    let key = physical_key(branch, 0x20, b"clean-wal-failure".to_vec());
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::CleanFailure,
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            key.clone(),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::WalService,
            reason: "injected clean WAL failure",
            ..
        })
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.wal.records.len(), 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    assert!(fixture
        .state
        .capture_read_view()
        .expect("read view")
        .latest(&key)
        .expect("latest read")
        .is_none());

    fixture.wal.mode = FakeWalMode::Succeed {
        forced_durable: false,
    };
    let retry = durable_batch(
        branch,
        CommitDurabilityMode::Standard,
        vec![CommitMutation::put(
            key,
            b"retry".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );
    let outcome = fixture.execute(retry).expect("retry succeeds");
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(2)));
    assert_eq!(fixture.wal.records.len(), 1);
}

#[test]
fn durable_clean_wal_failure_preserves_source_chain() {
    let branch = branch_id(68);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::CleanFailureWithSource,
    );

    let error = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"sourced-failure".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        ))
        .expect_err("sourced WAL failure");

    assert!(matches!(
        error,
        CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::WalService,
            reason: "injected sourced WAL failure",
            ..
        }
    ));
    assert_eq!(
        error.source().map(ToString::to_string),
        Some("injected WAL source".to_string())
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn durable_writer_halted_failure_is_clean_durability_unavailable() {
    let branch = branch_id(72);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::WriterHalted,
    );

    assert_eq!(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"writer-halted".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        Err(CommitRuntimeError::DurabilityUnavailable {
            reason: "WAL writer is halted; reopen before appending",
        })
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.wal.append_attempts, 1);
    assert_eq!(fixture.wal.records.len(), 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn durable_uncertain_wal_failure_is_distinct_and_leaves_no_visible_rows() {
    let branch = branch_id(57);
    let mut fixture = DurableFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::UncertainFailure,
    );
    let batch = durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"uncertain".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    );

    assert!(matches!(
        fixture.execute(batch),
        Err(CommitRuntimeError::DurabilityUncertain {
            branch_id: failed_branch,
            commit_version,
            reason: "WAL append durability is uncertain",
            ..
        }) if failed_branch == branch && commit_version == CommitVersion::new(1)
    ));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(fixture.wal.records.len(), 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    let _guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .expect("guard can be reacquired after uncertain WAL failure");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeWalMode {
    Succeed { forced_durable: bool },
    CleanFailure,
    CleanFailureWithSource,
    WriterHalted,
    UncertainFailure,
}

#[derive(Debug)]
struct RecordingWalAppender {
    policy: DurabilityPolicy,
    mode: FakeWalMode,
    records: Vec<WalRecord>,
    append_attempts: usize,
}

impl RecordingWalAppender {
    fn new(policy: DurabilityPolicy, mode: FakeWalMode) -> Self {
        Self {
            policy,
            mode,
            records: Vec::new(),
            append_attempts: 0,
        }
    }
}

impl CommitWalAppender for RecordingWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy {
        self.policy
    }

    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError> {
        self.append_attempts = self.append_attempts.saturating_add(1);
        match self.mode {
            FakeWalMode::Succeed { forced_durable } => {
                self.records.push(record.clone());
                Ok(CommitWalAppendFacts::new(0, 36, 128, 128, forced_durable))
            }
            FakeWalMode::CleanFailure => Err(CommitWalAppendError::clean(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "injected clean WAL failure",
                ),
            )),
            FakeWalMode::CleanFailureWithSource => Err(CommitWalAppendError::clean(
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::WalService,
                    "injected sourced WAL failure",
                    InjectedWalSource,
                ),
            )),
            FakeWalMode::WriterHalted => Err(CommitWalAppendError::clean(
                CommitRuntimeError::DurabilityUnavailable {
                    reason: "WAL writer is halted; reopen before appending",
                },
            )),
            FakeWalMode::UncertainFailure => Err(CommitWalAppendError::uncertain(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "injected uncertain WAL failure",
                ),
            )),
        }
    }
}

struct DurableFixture {
    config: CommitRuntimeConfig,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    state: BranchLocalState,
    visible: VisibleVersionTracker,
    wal: RecordingWalAppender,
}

impl DurableFixture {
    fn new(
        branch: BranchId,
        config: CommitRuntimeConfig,
        policy: DurabilityPolicy,
        wal_mode: FakeWalMode,
    ) -> Self {
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
            wal: RecordingWalAppender::new(policy, wal_mode),
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
        CommitDurableRuntime::new(
            &self.config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.state,
            &mut self.visible,
            &mut self.wal,
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

fn durable_batch(
    branch: BranchId,
    durability: CommitDurabilityMode,
    mutations: Vec<CommitMutation>,
) -> CommitBatch {
    durable_batch_with_validation(
        branch,
        durability,
        mutations,
        CommitValidationFacts::empty(),
    )
}

fn durable_batch_with_validation(
    branch: BranchId,
    durability: CommitDurabilityMode,
    mutations: Vec<CommitMutation>,
    validation: CommitValidationFacts,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        mutations,
        validation,
        CommitBatchOptions::new(
            durability,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn assert_unallocated_unattempted(fixture: &DurableFixture) {
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.wal.append_attempts, 0);
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
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

fn assert_record_rows_share_commit_facts(
    record: &WalRecord,
    branch: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
) {
    assert_eq!(record.branch_id(), branch);
    assert_eq!(record.commit_version(), version);
    assert_eq!(record.commit_timestamp(), timestamp);
    for row in record.commit_payload().rows() {
        assert_eq!(row.physical_key().branch_id(), branch);
        assert_eq!(row.commit_version(), version);
        assert_eq!(row.commit_timestamp(), timestamp);
    }
}

fn assert_payload_contains_timeline_rows(record: &WalRecord) {
    let timeline_rows = record
        .commit_payload()
        .rows()
        .iter()
        .filter(|row| row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE)
        .count();
    assert_eq!(timeline_rows, CommitTimelineRows::timeline_row_count());
}

#[derive(Debug)]
struct InjectedWalSource;

impl fmt::Display for InjectedWalSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("injected WAL source")
    }
}

impl std::error::Error for InjectedWalSource {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailingTimestampSource;

impl CommitTimestampSource for FailingTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        Err(CommitRuntimeError::timestamp_unavailable(
            "injected timestamp failure",
        ))
    }
}
