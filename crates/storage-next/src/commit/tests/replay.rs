use super::*;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::read::{BranchReadBound, BranchScanBounds};
use crate::branch::state::BranchLocalState;
use crate::row::StorageRow;
use std::error::Error as _;

#[test]
fn replay_applies_wal_rows_preserves_commit_facts_and_publishes_visible() {
    let branch = branch_id(91);
    let version = CommitVersion::new(7);
    let timestamp = Timestamp::from_micros(7_000);
    let key = physical_key(branch, 0x20, b"replay-put".to_vec());
    let secret_value = b"super-secret-replay-value".to_vec();
    let row = StorageRow::put(
        key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        secret_value.clone(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("replay succeeds");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(report.rows_checked(), 3);
    assert_eq!(report.rows_applied(), 3);
    assert!(!report.gate_cleared());
    assert_eq!(report.outcome().kind(), CommitOutcomeKind::Replay);
    assert_eq!(report.outcome().phase(), CommitPhase::Replay);
    assert_eq!(
        report.outcome().durability(),
        CommitDurabilityClass::Standard
    );
    assert_eq!(report.outcome().commit_version(), Some(version));
    assert_eq!(report.outcome().commit_timestamp(), Some(timestamp));
    assert_eq!(report.outcome().mutation_counts().puts(), 1);
    assert_eq!(report.outcome().mutation_counts().deletes(), 0);
    assert_eq!(
        report.outcome().mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(fixture.visible.visible_version(), version);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        version
    );
    assert_eq!(
        fixture.allocator.timestamp_guard().last_allocated(),
        Some(timestamp)
    );

    let view = fixture.state.capture_read_view().expect("read view");
    let visible = view
        .latest(&key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().commit_version(), version);
    assert_eq!(visible.row().commit_timestamp(), timestamp);
    assert_eq!(visible.row().value(), secret_value);
    assert!(!format!("{report:?}").contains("super-secret-replay-value"));
    assert_eq!(
        timeline_view(&view, branch).timestamp_for_version(version),
        Some(timestamp)
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn replay_classification_perf_trace_counts_rows_and_history_calls() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(121);
    let version = CommitVersion::new(40);
    let timestamp = Timestamp::from_micros(40_000);
    let left = StorageRow::put(
        physical_key(branch, 0x20, b"replay-classify-left".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"left".to_vec(),
    );
    let right = StorageRow::put(
        physical_key(branch, 0x20, b"replay-classify-right".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"right".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![left, right]);
    let expected_rows =
        u64::try_from(record.commit_payload().rows().len()).expect("row count fits u64");
    let mut fixture = ReplayFixture::new(branch);

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("replay succeeds");

    assert_eq!(
        u64::try_from(report.rows_checked()).expect("checked row count fits u64"),
        expected_rows
    );
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_replay_classification_calls(), 1);
    assert_eq!(perf.commit_replay_rows_classified(), expected_rows);
    assert_eq!(perf.commit_replay_history_calls(), expected_rows);
}

#[cfg(feature = "perf-trace")]
#[test]
fn replay_classification_perf_trace_counts_many_rows_without_conflict_validation() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(122);
    let version = CommitVersion::new(41);
    let timestamp = Timestamp::from_micros(41_000);
    let rows = (0..32)
        .map(|index| {
            StorageRow::put(
                physical_key(
                    branch,
                    0x20,
                    format!("replay-classify-many-{index:02}").into_bytes(),
                ),
                version,
                timestamp,
                Timestamp::EPOCH,
                vec![u8::try_from(index).expect("test row index fits u8")],
            )
        })
        .collect::<Vec<_>>();
    let record = replay_record(branch, version, timestamp, rows);
    let expected_rows =
        u64::try_from(record.commit_payload().rows().len()).expect("row count fits u64");
    let mut fixture = ReplayFixture::new(branch);

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("replay succeeds");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(
        u64::try_from(report.rows_checked()).expect("checked row count fits u64"),
        expected_rows
    );
    assert_eq!(
        u64::try_from(report.rows_applied()).expect("applied row count fits u64"),
        expected_rows
    );
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_replay_classification_calls(), 1);
    assert_eq!(perf.commit_replay_rows_classified(), expected_rows);
    assert_eq!(perf.commit_replay_history_calls(), expected_rows);
    assert_eq!(perf.commit_conflict_validation_calls(), 0);
    assert_eq!(perf.conflict_sources_built(), 0);
}

#[test]
fn replay_applies_mixed_put_delete_with_always_durability() {
    let branch = branch_id(107);
    let prior_version = CommitVersion::new(2);
    let version = CommitVersion::new(26);
    let timestamp = Timestamp::from_micros(26_000);
    let put_key = physical_key(branch, 0x20, b"mixed-put".to_vec());
    let delete_key = physical_key(branch, 0x20, b"mixed-delete".to_vec());
    let existing = StorageRow::put(
        delete_key.clone(),
        prior_version,
        Timestamp::from_micros(2_000),
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let put = StorageRow::put(
        put_key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"new".to_vec(),
    );
    let delete = StorageRow::tombstone(delete_key.clone(), version, timestamp);
    let record = replay_record(branch, version, timestamp, vec![put, delete]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .state
        .append_committed_rows_atomically(vec![existing])
        .expect("seed existing row");

    let report = fixture
        .replay(record, CommitDurabilityClass::Always)
        .expect("mixed replay succeeds");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(report.outcome().durability(), CommitDurabilityClass::Always);
    assert_eq!(report.rows_checked(), 4);
    assert_eq!(report.rows_applied(), 4);
    assert_eq!(report.outcome().mutation_counts().puts(), 1);
    assert_eq!(report.outcome().mutation_counts().deletes(), 1);
    let view = fixture.state.capture_read_view().expect("read view");
    assert_eq!(
        view.latest(&put_key)
            .expect("put latest")
            .expect("put row")
            .row()
            .value(),
        b"new"
    );
    assert_eq!(view.latest(&delete_key).expect("delete latest"), None);
    let lookup = timeline_view(&view, branch).version_at_or_before(timestamp);
    assert_eq!(lookup.matched_version(), Some(version));
    assert_eq!(lookup.matched_timestamp(), Some(timestamp));
    assert_eq!(lookup.miss(), CommitTimelineMiss::Matched);
}

#[test]
fn replaying_same_record_twice_does_not_duplicate_rows() {
    let branch = branch_id(108);
    let version = CommitVersion::new(27);
    let timestamp = Timestamp::from_micros(27_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"second-replay".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);

    let first = fixture
        .replay(record.clone(), CommitDurabilityClass::Standard)
        .expect("first replay succeeds");
    let active_rows_after_first = fixture.state.active_row_count();
    let second = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("second replay succeeds");

    assert_eq!(first.action(), CommitReplayAction::Applied);
    assert_eq!(second.action(), CommitReplayAction::AlreadyApplied);
    assert_eq!(second.rows_applied(), 0);
    assert_eq!(fixture.state.active_row_count(), active_rows_after_first);
    assert_eq!(fixture.visible.visible_version(), version);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        version
    );
    assert_eq!(
        fixture.allocator.timestamp_guard().last_allocated(),
        Some(timestamp)
    );
    let view = fixture.state.capture_read_view().expect("read view");
    assert_eq!(
        timeline_view(&view, branch).timestamp_for_version(version),
        Some(timestamp)
    );
}

#[test]
fn replay_below_existing_allocator_floors_does_not_regress_them() {
    let branch = branch_id(112);
    let version = CommitVersion::new(31);
    let timestamp = Timestamp::from_micros(31_000);
    let higher_version = CommitVersion::new(50);
    let higher_timestamp = Timestamp::from_micros(50_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"lower-replay".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .allocator
        .catch_up_to_recovered_version(higher_version);
    fixture
        .allocator
        .catch_up_to_recovered_timestamp(higher_timestamp);

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("lower replay succeeds");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        higher_version
    );
    assert_eq!(
        fixture.allocator.timestamp_guard().last_allocated(),
        Some(higher_timestamp)
    );
    assert_eq!(fixture.visible.visible_version(), version);

    fixture
        .allocator
        .source_mut()
        .set_next_timestamp(Timestamp::from_micros(1));
    let post_replay_batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"post-replay-normal".to_vec()),
            b"next".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid post-replay batch");
    let next_stamp = fixture
        .allocator
        .allocate_for_batch(&post_replay_batch)
        .expect("post-replay allocation")
        .stamp()
        .expect("mutating stamp");
    assert_eq!(next_stamp.commit_version(), CommitVersion::new(51));
    assert_eq!(next_stamp.commit_timestamp(), higher_timestamp);
}

#[test]
fn replay_newer_row_bypasses_stale_read_conflict_checks() {
    let branch = branch_id(113);
    let prior_version = CommitVersion::new(4);
    let version = CommitVersion::new(32);
    let timestamp = Timestamp::from_micros(32_000);
    let key = physical_key(branch, 0x20, b"stale-read-bypass".to_vec());
    let existing = StorageRow::put(
        key.clone(),
        prior_version,
        Timestamp::from_micros(4_000),
        Timestamp::EPOCH,
        b"old".to_vec(),
    );
    let replayed = StorageRow::put(
        key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![replayed]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .state
        .append_committed_rows_atomically(vec![existing])
        .expect("seed stale observed row");

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("replay bypasses normal conflict validation");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(fixture.visible.visible_version(), version);
    let view = fixture.state.capture_read_view().expect("read view");
    assert_eq!(
        view.latest(&key)
            .expect("latest read")
            .expect("visible row")
            .row()
            .value(),
        b"durable"
    );
}

#[test]
fn replay_exact_duplicate_is_idempotent_catches_up_and_clears_matching_gate() {
    let branch = branch_id(92);
    let version = CommitVersion::new(11);
    let timestamp = Timestamp::from_micros(11_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"already-applied".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"present".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);
    fixture.seed_record_rows(&record);
    fixture
        .durable_gate
        .record_unresolved(
            CommitUnresolvedDurable::applied_not_visible(
                CommitStamp::new(branch, version, timestamp).expect("stamp"),
                CommitDurabilityClass::Always,
                "seed applied-not-visible durable fact",
            )
            .expect("unresolved fact"),
        )
        .expect("record unresolved gate");

    let report = fixture
        .replay(record, CommitDurabilityClass::Always)
        .expect("idempotent replay succeeds");

    assert_eq!(report.action(), CommitReplayAction::AlreadyApplied);
    assert_eq!(report.rows_checked(), 3);
    assert_eq!(report.rows_applied(), 0);
    assert!(report.gate_cleared());
    assert_eq!(fixture.visible.visible_version(), version);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        version
    );
    assert_eq!(
        fixture.allocator.timestamp_guard().last_allocated(),
        Some(timestamp)
    );
    assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
}

#[test]
fn replay_rejects_partial_existing_rows_without_catch_up_or_visibility() {
    let branch = branch_id(93);
    let version = CommitVersion::new(12);
    let timestamp = Timestamp::from_micros(12_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"partial".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"partial".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row.clone()]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .state
        .append_committed_rows_atomically(vec![row])
        .expect("seed partial row");

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("partial replay rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "replay rows are only partially installed",
        }
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.allocator.timestamp_guard().last_allocated(), None);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
}

#[test]
fn replay_rejects_timeline_present_user_missing_partial_state() {
    let branch = branch_id(110);
    let version = CommitVersion::new(29);
    let timestamp = Timestamp::from_micros(29_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"timeline-present".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let timeline_rows = record
        .commit_payload()
        .rows()
        .iter()
        .filter(|row| row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE)
        .cloned()
        .collect::<Vec<_>>();
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .state
        .append_committed_rows_atomically(timeline_rows)
        .expect("seed timeline rows only");

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("partial replay rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "replay rows are only partially installed",
        }
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
}

#[test]
fn replay_rejects_duplicate_mismatch_without_mutating_state() {
    let branch = branch_id(94);
    let version = CommitVersion::new(13);
    let timestamp = Timestamp::from_micros(13_000);
    let key = physical_key(branch, 0x20, b"mismatch".to_vec());
    let replay_row = StorageRow::put(
        key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"super-secret-replay-value".to_vec(),
    );
    let existing = StorageRow::put(
        key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"installed-value".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![replay_row]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .state
        .append_committed_rows_atomically(vec![existing])
        .expect("seed mismatch row");

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("mismatch rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "replay row conflicts with installed row facts",
        }
    );
    assert!(!error.to_string().contains("super-secret-replay-value"));
    assert!(!format!("{error:?}").contains("super-secret-replay-value"));
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    let view = fixture.state.capture_read_view().expect("read view");
    let visible = view
        .latest(&key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().value(), b"installed-value");
}

#[test]
fn replay_duplicate_mismatch_checks_timestamp_expiry_and_tombstone() {
    let branch = branch_id(109);
    let version = CommitVersion::new(28);
    let timestamp = Timestamp::from_micros(28_000);
    let key = physical_key(branch, 0x20, b"mismatch-shape".to_vec());
    let replay_put = StorageRow::put(
        key.clone(),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let cases = [
        StorageRow::put(
            key.clone(),
            version,
            Timestamp::from_micros(28_001),
            Timestamp::EPOCH,
            b"durable".to_vec(),
        ),
        StorageRow::put(
            key.clone(),
            version,
            timestamp,
            Timestamp::from_micros(29_000),
            b"durable".to_vec(),
        ),
        StorageRow::tombstone(key, version, timestamp),
    ];

    for existing in cases {
        let record = replay_record(branch, version, timestamp, vec![replay_put.clone()]);
        let mut fixture = ReplayFixture::new(branch);
        fixture
            .state
            .append_committed_rows_atomically(vec![existing])
            .expect("seed mismatched row");

        let error = fixture
            .replay(record, CommitDurabilityClass::Standard)
            .expect_err("mismatch rejects");

        assert_eq!(
            error,
            CommitRuntimeError::InvalidCommitState {
                reason: "replay row conflicts with installed row facts",
            }
        );
        assert_eq!(
            fixture.allocator.version_allocator().last_allocated(),
            CommitVersion::ZERO
        );
        assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
        assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
    }
}

#[test]
fn replay_rejects_record_without_timeline_pair() {
    let branch = branch_id(95);
    let version = CommitVersion::new(14);
    let timestamp = Timestamp::from_micros(14_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"missing-timeline".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let payload = WalCommitPayload::new(vec![row]).expect("payload");
    let record = WalRecord::new(version, branch, timestamp, payload).expect("record");
    let mut fixture = ReplayFixture::new(branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("missing timeline rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidTimelineFact {
            reason: "replay payload is missing commit timeline rows",
        }
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn replay_rejects_timeline_only_payload_without_user_mutation() {
    let branch = branch_id(120);
    let version = CommitVersion::new(39);
    let timestamp = Timestamp::from_micros(39_000);
    let record = replay_record(branch, version, timestamp, Vec::new());
    let mut fixture = ReplayFixture::new(branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("timeline-only replay rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "replay payload is missing user mutation rows",
        }
    );
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
    assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
}

#[test]
fn replay_record_construction_rejects_outer_fact_mismatches() {
    let branch = branch_id(114);
    let version = CommitVersion::new(33);
    let timestamp = Timestamp::from_micros(33_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"outer-facts".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let payload = WalCommitPayload::new(vec![row]).expect("payload");

    assert_eq!(
        WalRecord::new(CommitVersion::new(34), branch, timestamp, payload.clone()),
        Err(FormatError::InvalidValue {
            field: "commit_version",
        })
    );
    assert_eq!(
        WalRecord::new(version, branch_id(115), timestamp, payload.clone()),
        Err(FormatError::InvalidValue { field: "branch_id" })
    );
    assert_eq!(
        WalRecord::new(
            version,
            branch,
            Timestamp::from_micros(timestamp.as_micros() + 1),
            payload,
        ),
        Err(FormatError::InvalidValue {
            field: "commit_timestamp",
        })
    );
}

#[test]
fn replay_empty_payload_is_rejected_by_wal_payload_layer() {
    assert_eq!(
        WalCommitPayload::new(Vec::new()),
        Err(FormatError::InvalidValue { field: "row_count" })
    );
}

#[test]
fn replay_rejects_non_durable_request_before_mutation() {
    let branch = branch_id(101);
    let version = CommitVersion::new(20);
    let timestamp = Timestamp::from_micros(20_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"non-durable-replay".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);

    for durability in [
        CommitDurabilityClass::NotDurable,
        CommitDurabilityClass::Uncertain,
    ] {
        let mut fixture = ReplayFixture::new(branch);
        let error = fixture
            .replay(record.clone(), durability)
            .expect_err("non-durable replay rejects");

        assert_eq!(
            error,
            CommitRuntimeError::DurabilityUnavailable {
                reason: "replay requires a durable WAL record",
            }
        );
        assert_eq!(fixture.state.active_row_count(), 0);
        assert_eq!(
            fixture.allocator.version_allocator().last_allocated(),
            CommitVersion::ZERO
        );
        assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
        assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
    }
}

#[test]
fn replay_rejects_duplicate_internal_rows_before_mutation() {
    let branch = branch_id(102);
    let version = CommitVersion::new(21);
    let timestamp = Timestamp::from_micros(21_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"duplicate-in-payload".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row.clone(), row]);
    let mut fixture = ReplayFixture::new(branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("duplicate internal rows reject");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "replay payload contains duplicate internal rows",
        }
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn replay_rejects_non_timeline_storage_owned_rows_before_mutation() {
    let branch = branch_id(103);
    let version = CommitVersion::new(22);
    let timestamp = Timestamp::from_micros(22_000);
    let reserved_space = StorageSpaceId::from_raw(0x02).expect("reserved storage space");
    let reserved_key =
        PhysicalKey::new(branch, "reserved", reserved_space, b"not-timeline".to_vec())
            .expect("reserved physical key");
    let row = StorageRow::put(
        reserved_key,
        version,
        timestamp,
        Timestamp::EPOCH,
        b"storage-owned".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("reserved storage row rejects");

    assert_eq!(
        error,
        CommitRuntimeError::StorageOwnedMutationSpace {
            space_id: reserved_space,
        }
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn replay_rejects_timeline_pair_that_disagrees_with_wal_facts() {
    let branch = branch_id(104);
    let version = CommitVersion::new(23);
    let timestamp = Timestamp::from_micros(23_000);
    let wrong_stamp =
        CommitStamp::new(branch, version, Timestamp::from_micros(23_001)).expect("wrong stamp");
    let mut rows = vec![StorageRow::put(
        physical_key(branch, 0x20, b"timeline-mismatch".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    )];
    rows.extend(
        CommitTimelineRows::from_entry(
            CommitTimelineEntry::from_stamp(wrong_stamp).expect("wrong timeline entry"),
        )
        .expect("wrong timeline rows")
        .into_rows()
        .into_iter()
        .map(|row| {
            StorageRow::put(
                row.physical_key().clone(),
                version,
                timestamp,
                Timestamp::EPOCH,
                row.value().to_vec(),
            )
        }),
    );
    let payload = WalCommitPayload::new(rows).expect("payload");
    let record = WalRecord::new(version, branch, timestamp, payload).expect("record");
    let mut fixture = ReplayFixture::new(branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("timeline mismatch rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidTimelineFact {
            reason: "timestamp timeline row timestamp disagrees with key",
        }
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(fixture.visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn replay_ignores_current_batch_admission_limits_for_already_durable_rows() {
    let branch = branch_id(105);
    let version = CommitVersion::new(24);
    let timestamp = Timestamp::from_micros(24_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"config-limits".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let config =
        CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled).expect("config");
    let mut fixture = ReplayFixture::new_with_config(branch, config);

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("durable replay is not re-admitted through current batch limits");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert_eq!(report.rows_checked(), 3);
    assert_eq!(report.outcome().mutation_counts().puts(), 1);
    assert_eq!(
        report.outcome().mutation_counts().timeline_rows(),
        CommitTimelineRows::timeline_row_count()
    );
    assert_eq!(fixture.visible.visible_version(), version);
}

#[test]
fn replay_rejects_target_branch_mismatch_before_mutation() {
    let record_branch = branch_id(98);
    let target_branch = branch_id(99);
    let version = CommitVersion::new(17);
    let timestamp = Timestamp::from_micros(17_000);
    let row = StorageRow::put(
        physical_key(record_branch, 0x20, b"wrong-branch".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(record_branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(target_branch);

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("branch mismatch rejects");

    assert_eq!(
        error,
        CommitRuntimeError::BranchMismatch {
            expected: record_branch,
            actual: target_branch,
        }
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
}

#[test]
fn replay_rejects_different_unresolved_gate_before_mutation() {
    let branch = branch_id(100);
    let version = CommitVersion::new(18);
    let timestamp = Timestamp::from_micros(18_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"different-gate".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .durable_gate
        .record_unresolved(
            CommitUnresolvedDurable::durable_not_applied_with_facts(
                CommitStamp::new(
                    branch,
                    CommitVersion::new(99),
                    Timestamp::from_micros(99_000),
                )
                .expect("stamp"),
                CommitDurabilityClass::Standard,
                "other unresolved durable fact",
            )
            .expect("unresolved fact"),
        )
        .expect("seed different gate");

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("different gate rejects");

    assert_eq!(
        error,
        CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: branch,
            commit_version: CommitVersion::new(99),
            reason: "different unresolved durable commit blocks replay",
        }
    );
    assert_eq!(fixture.state.active_row_count(), 0);
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
}

#[test]
fn replay_apply_with_matching_durable_not_applied_gate_clears_after_visibility() {
    let branch = branch_id(116);
    let version = CommitVersion::new(35);
    let timestamp = Timestamp::from_micros(35_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"apply-gate-clear".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new(branch);
    fixture
        .durable_gate
        .record_unresolved(
            CommitUnresolvedDurable::durable_not_applied_with_facts(
                CommitStamp::new(branch, version, timestamp).expect("stamp"),
                CommitDurabilityClass::Standard,
                "seed durable-not-applied fact",
            )
            .expect("unresolved fact"),
        )
        .expect("seed gate");

    let report = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect("replay applies and clears matching gate");

    assert_eq!(report.action(), CommitReplayAction::Applied);
    assert!(report.gate_cleared());
    assert_eq!(fixture.visible.visible_version(), version);
    assert_eq!(fixture.durable_gate.unresolved().expect("gate"), None);
}

#[test]
fn replay_visible_publish_failure_records_applied_not_visible_gate() {
    let branch = branch_id(96);
    let version = CommitVersion::new(15);
    let timestamp = Timestamp::from_micros(15_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"visible-failure".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new_with_visible(branch, FailingReplayVisible::new());

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("visible failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::DurableButNotVisible {
            branch_id: failed_branch,
            commit_version: failed_version,
            reason: "visible publication failed after replay apply",
            ..
        } if failed_branch == branch && failed_version == version
    ));
    assert!(error.source().is_some());
    assert_eq!(
        fixture.allocator.version_allocator().last_allocated(),
        version
    );
    assert_eq!(
        fixture.allocator.timestamp_guard().last_allocated(),
        Some(timestamp)
    );
    let unresolved = fixture
        .durable_gate
        .unresolved()
        .expect("gate")
        .expect("unresolved replay");
    assert_eq!(unresolved.branch_id(), branch);
    assert_eq!(unresolved.commit_version(), version);
    assert_eq!(
        unresolved.kind(),
        CommitUnresolvedDurableKind::AppliedNotVisible
    );
}

#[test]
fn replay_visible_failure_advances_matching_durable_not_applied_gate() {
    let branch = branch_id(97);
    let version = CommitVersion::new(16);
    let timestamp = Timestamp::from_micros(16_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"gate-advance".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let mut fixture = ReplayFixture::new_with_visible(branch, FailingReplayVisible::new());
    fixture
        .durable_gate
        .record_unresolved(
            CommitUnresolvedDurable::durable_not_applied_with_facts(
                CommitStamp::new(branch, version, timestamp).expect("stamp"),
                CommitDurabilityClass::Standard,
                "seed durable-not-applied fact",
            )
            .expect("unresolved fact"),
        )
        .expect("seed gate");

    let error = fixture
        .replay(record, CommitDurabilityClass::Standard)
        .expect_err("visible failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::DurableButNotVisible {
            branch_id: failed_branch,
            commit_version: failed_version,
            ..
        } if failed_branch == branch && failed_version == version
    ));
    let unresolved = fixture
        .durable_gate
        .unresolved()
        .expect("gate")
        .expect("unresolved replay");
    assert_eq!(
        unresolved.kind(),
        CommitUnresolvedDurableKind::AppliedNotVisible
    );
    assert_eq!(unresolved.commit_version(), version);
}

#[test]
fn replay_apply_failure_records_durable_not_applied_gate_without_catch_up() {
    let branch = branch_id(106);
    let version = CommitVersion::new(25);
    let timestamp = Timestamp::from_micros(25_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"apply-failure".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let config = CommitRuntimeConfig::default();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    );
    let mut branch_state = FailingReplayApplyTarget::new(branch);
    let mut visible = VisibleVersionTracker::default();
    let durable_gate = CommitUnresolvedDurableGate::new();

    let error = CommitReplayRuntime::new(
        &config,
        &mut allocator,
        &mut branch_state,
        &mut visible,
        &durable_gate,
    )
    .replay(&CommitReplayRequest::new(
        record,
        CommitDurabilityClass::Standard,
    ))
    .expect_err("apply failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::DurableButNotVisible {
            branch_id: failed_branch,
            commit_version: failed_version,
            reason: "branch state rejected replay rows",
            ..
        } if failed_branch == branch && failed_version == version
    ));
    assert!(error.source().is_some());
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(allocator.timestamp_guard().last_allocated(), None);
    assert_eq!(visible.visible_version(), CommitVersion::ZERO);
    let unresolved = durable_gate
        .unresolved()
        .expect("gate")
        .expect("unresolved replay");
    assert_eq!(
        unresolved.kind(),
        CommitUnresolvedDurableKind::DurableNotApplied
    );
    assert_eq!(unresolved.commit_version(), version);
}

#[test]
fn replay_apply_failure_with_matching_gate_preserves_existing_gate() {
    let branch = branch_id(117);
    let version = CommitVersion::new(36);
    let timestamp = Timestamp::from_micros(36_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"apply-failure-matching-gate".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let config = CommitRuntimeConfig::default();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    );
    let mut branch_state = FailingReplayApplyTarget::new(branch);
    let mut visible = VisibleVersionTracker::default();
    let durable_gate = CommitUnresolvedDurableGate::new();
    let unresolved = CommitUnresolvedDurable::durable_not_applied_with_facts(
        CommitStamp::new(branch, version, timestamp).expect("stamp"),
        CommitDurabilityClass::Standard,
        "seed durable-not-applied fact",
    )
    .expect("unresolved fact");
    durable_gate
        .record_unresolved(unresolved)
        .expect("seed matching gate");

    let error = CommitReplayRuntime::new(
        &config,
        &mut allocator,
        &mut branch_state,
        &mut visible,
        &durable_gate,
    )
    .replay(&CommitReplayRequest::new(
        record,
        CommitDurabilityClass::Standard,
    ))
    .expect_err("apply failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::DurableButNotVisible {
            branch_id: failed_branch,
            commit_version: failed_version,
            ..
        } if failed_branch == branch && failed_version == version
    ));
    assert_eq!(durable_gate.unresolved().expect("gate"), Some(unresolved));
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(visible.visible_version(), CommitVersion::ZERO);
}

#[test]
fn replay_clear_failure_after_visibility_preserves_different_gate_fact() {
    let branch = branch_id(118);
    let version = CommitVersion::new(37);
    let timestamp = Timestamp::from_micros(37_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"clear-failure".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let config = CommitRuntimeConfig::default();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    );
    let mut branch_state =
        BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("branch state");
    branch_state
        .append_committed_rows_atomically(record.commit_payload().rows().to_vec())
        .expect("seed exact duplicate rows");
    let durable_gate = CommitUnresolvedDurableGate::new();
    let original = CommitUnresolvedDurable::applied_not_visible(
        CommitStamp::new(branch, version, timestamp).expect("stamp"),
        CommitDurabilityClass::Standard,
        "seed applied-not-visible fact",
    )
    .expect("original unresolved fact");
    let replacement = CommitUnresolvedDurable::durable_not_applied_with_facts(
        CommitStamp::new(
            branch,
            CommitVersion::new(version.as_u64() + 1),
            Timestamp::from_micros(timestamp.as_micros() + 1),
        )
        .expect("replacement stamp"),
        CommitDurabilityClass::Standard,
        "concurrent gate replacement",
    )
    .expect("replacement unresolved fact");
    durable_gate
        .record_unresolved(original)
        .expect("seed original gate");
    let mut visible = GateReplacingReplayVisible::new(&durable_gate, original, replacement);

    let error = CommitReplayRuntime::new(
        &config,
        &mut allocator,
        &mut branch_state,
        &mut visible,
        &durable_gate,
    )
    .replay(&CommitReplayRequest::new(
        record,
        CommitDurabilityClass::Standard,
    ))
    .expect_err("clear failure rejects");

    assert_eq!(
        error,
        CommitRuntimeError::InvalidCommitState {
            reason: "cannot clear different unresolved durable commit",
        }
    );
    assert_eq!(visible.visible_version(), version);
    assert_eq!(durable_gate.unresolved().expect("gate"), Some(replacement));
}

#[test]
fn replay_read_view_failure_preserves_source_without_mutation() {
    let branch = branch_id(111);
    let version = CommitVersion::new(30);
    let timestamp = Timestamp::from_micros(30_000);
    let row = StorageRow::put(
        physical_key(branch, 0x20, b"read-failure".to_vec()),
        version,
        timestamp,
        Timestamp::EPOCH,
        b"durable".to_vec(),
    );
    let record = replay_record(branch, version, timestamp, vec![row]);
    let config = CommitRuntimeConfig::default();
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    );
    let mut branch_state = FailingReplayReadTarget::new(branch);
    let mut visible = VisibleVersionTracker::default();
    let durable_gate = CommitUnresolvedDurableGate::new();

    let error = CommitReplayRuntime::new(
        &config,
        &mut allocator,
        &mut branch_state,
        &mut visible,
        &durable_gate,
    )
    .replay(&CommitReplayRequest::new(
        record,
        CommitDurabilityClass::Standard,
    ))
    .expect_err("read-view failure rejects");

    assert!(matches!(
        error,
        CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::BranchRuntime,
            reason: "injected replay read-view failure",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::ZERO
    );
    assert_eq!(allocator.timestamp_guard().last_allocated(), None);
    assert_eq!(visible.visible_version(), CommitVersion::ZERO);
    assert_eq!(durable_gate.unresolved().expect("gate"), None);
    assert!(!branch_state.append_attempted);
}

struct ReplayFixture<V = VisibleVersionTracker> {
    config: CommitRuntimeConfig,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    state: BranchLocalState,
    visible: V,
    durable_gate: CommitUnresolvedDurableGate,
}

impl ReplayFixture {
    fn new(branch: BranchId) -> Self {
        Self::new_with_visible(branch, VisibleVersionTracker::default())
    }

    fn new_with_config(branch: BranchId, config: CommitRuntimeConfig) -> Self {
        let mut fixture = Self::new(branch);
        fixture.config = config;
        fixture
    }
}

impl<V: CommitVisiblePublisher> ReplayFixture<V> {
    fn new_with_visible(branch: BranchId, visible: V) -> Self {
        Self {
            config: CommitRuntimeConfig::default(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
            ),
            state: BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state"),
            visible,
            durable_gate: CommitUnresolvedDurableGate::new(),
        }
    }

    fn replay(
        &mut self,
        record: WalRecord,
        durability: CommitDurabilityClass,
    ) -> CommitRuntimeResult<CommitReplayReport> {
        CommitReplayRuntime::new(
            &self.config,
            &mut self.allocator,
            &mut self.state,
            &mut self.visible,
            &self.durable_gate,
        )
        .replay(&CommitReplayRequest::new(record, durability))
    }

    fn seed_record_rows(&mut self, record: &WalRecord) {
        self.state
            .append_committed_rows_atomically(record.commit_payload().rows().to_vec())
            .expect("seed record rows");
    }
}

#[derive(Debug)]
struct FailingReplayVisible;

impl FailingReplayVisible {
    const fn new() -> Self {
        Self
    }
}

impl CommitVisiblePublisher for FailingReplayVisible {
    fn visible_version(&self) -> CommitVersion {
        CommitVersion::ZERO
    }

    fn publish_from_facts(
        &mut self,
        _facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "injected replay visible failure",
        })
    }
}

#[derive(Debug)]
struct GateReplacingReplayVisible<'a> {
    tracker: VisibleVersionTracker,
    gate: &'a CommitUnresolvedDurableGate,
    expected: CommitUnresolvedDurable,
    replacement: CommitUnresolvedDurable,
}

impl<'a> GateReplacingReplayVisible<'a> {
    fn new(
        gate: &'a CommitUnresolvedDurableGate,
        expected: CommitUnresolvedDurable,
        replacement: CommitUnresolvedDurable,
    ) -> Self {
        Self {
            tracker: VisibleVersionTracker::default(),
            gate,
            expected,
            replacement,
        }
    }
}

impl CommitVisiblePublisher for GateReplacingReplayVisible<'_> {
    fn visible_version(&self) -> CommitVersion {
        self.tracker.visible_version()
    }

    fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        let publish = self.tracker.publish_from_facts(facts)?;
        self.gate.replace_exact(self.expected, self.replacement)?;
        Ok(publish)
    }
}

#[derive(Debug)]
struct FailingReplayReadTarget {
    branch: BranchId,
    append_attempted: bool,
}

impl FailingReplayReadTarget {
    const fn new(branch: BranchId) -> Self {
        Self {
            branch,
            append_attempted: false,
        }
    }
}

impl CommitBranchApplyTarget for FailingReplayReadTarget {
    fn branch_id(&self) -> BranchId {
        self.branch
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        None
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<crate::branch::read::BranchReadView> {
        Err(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::BranchRuntime,
            "injected replay read-view failure",
            InjectedReplayReadSource,
        ))
    }

    fn append_committed_rows_atomically(
        &mut self,
        _rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()> {
        self.append_attempted = true;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InjectedReplayReadSource;

impl fmt::Display for InjectedReplayReadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("injected replay read source")
    }
}

impl Error for InjectedReplayReadSource {}

#[derive(Debug)]
struct FailingReplayApplyTarget {
    state: BranchLocalState,
}

impl FailingReplayApplyTarget {
    fn new(branch: BranchId) -> Self {
        Self {
            state: BranchLocalState::new(branch, BranchRuntimeConfig::default()).expect("state"),
        }
    }
}

impl CommitBranchApplyTarget for FailingReplayApplyTarget {
    fn branch_id(&self) -> BranchId {
        self.state.branch_id()
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        self.state.max_commit_version()
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<crate::branch::read::BranchReadView> {
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
        _rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()> {
        Err(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::BranchRuntime,
            "injected replay branch apply failure",
            InjectedReplayApplySource,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InjectedReplayApplySource;

impl fmt::Display for InjectedReplayApplySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("injected replay apply source")
    }
}

impl Error for InjectedReplayApplySource {}

fn replay_record(
    branch: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
    mut user_rows: Vec<StorageRow>,
) -> WalRecord {
    let stamp = CommitStamp::new(branch, version, timestamp).expect("stamp");
    let timeline_rows = CommitTimelineRows::from_entry(
        CommitTimelineEntry::from_stamp(stamp).expect("timeline entry"),
    )
    .expect("timeline rows")
    .into_rows();
    user_rows.extend(timeline_rows);
    let payload = WalCommitPayload::new(user_rows).expect("payload");
    WalRecord::new(version, branch, timestamp, payload).expect("record")
}

fn timeline_view(
    view: &crate::branch::read::BranchReadView,
    branch: BranchId,
) -> CommitTimelineView {
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
        rows.iter().map(crate::branch::read::BranchVisibleRow::row),
    )
    .expect("timeline view")
}
