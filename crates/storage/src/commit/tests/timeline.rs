use super::*;
use crate::row::StorageRow;

#[test]
fn timeline_entry_rejects_zero_version() {
    assert_eq!(
        CommitTimelineEntry::new(branch_id(1), CommitVersion::ZERO, Timestamp::EPOCH),
        Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "commit version must be nonzero",
        })
    );
}

#[test]
fn timeline_entry_can_be_derived_from_commit_stamp() {
    let stamp = CommitStamp::new(
        branch_id(1),
        CommitVersion::new(7),
        Timestamp::from_micros(70),
    )
    .expect("commit stamp");
    let entry = CommitTimelineEntry::from_stamp(stamp).expect("timeline entry");

    assert_eq!(entry.branch_id(), stamp.branch_id());
    assert_eq!(entry.commit_version(), stamp.commit_version());
    assert_eq!(entry.commit_timestamp(), stamp.commit_timestamp());
}

#[test]
fn timeline_entry_debug_is_bounded_storage_shaped() {
    let entry = CommitTimelineEntry::new(branch_id(1), CommitVersion::new(1), Timestamp::EPOCH)
        .expect("epoch timestamp is valid");
    let debug = format!("{entry:?}");

    assert!(debug.contains("CommitTimelineEntry"));
    assert_bounded_storage_debug(&debug);
}

#[test]
fn timeline_rows_are_deterministic_storage_owned_puts() {
    let entry = entry(1, 7, 1_000);
    let rows = CommitTimelineRows::from_entry(entry).expect("timeline rows");
    let repeated = CommitTimelineRows::from_entry(entry).expect("repeated timeline rows");

    assert_eq!(rows.entry(), entry);
    assert_eq!(CommitTimelineRows::timeline_row_count(), 2);
    assert_eq!(rows.rows(), repeated.rows());
    for row in rows.rows() {
        assert_eq!(row.physical_key().branch_id(), entry.branch_id());
        assert_eq!(row.physical_key().space(), COMMIT_TIMELINE_SPACE);
        assert_eq!(
            row.physical_key().storage_space_id(),
            StorageSpaceId::COMMIT_TIMELINE
        );
        assert_eq!(row.commit_version(), entry.commit_version());
        assert_eq!(row.commit_timestamp(), entry.commit_timestamp());
        assert_eq!(row.expires_at(), Timestamp::EPOCH);
        assert!(!row.is_tombstone());
        assert_eq!(row.value().len(), 8);
    }
    assert!(rows
        .timestamp_to_version()
        .physical_key()
        .user_key()
        .starts_with(b"ts-v1\0"));
    assert!(rows
        .version_to_timestamp()
        .physical_key()
        .user_key()
        .starts_with(b"ver-v1\0"));
    assert!(
        rows.timestamp_to_version().physical_key().user_key()
            < rows.version_to_timestamp().physical_key().user_key()
    );
}

#[test]
fn timeline_keys_preserve_big_endian_order_and_branch_isolation() {
    let low_timestamp = CommitTimelineRows::from_entry(entry(2, 1, 1_000)).expect("timeline rows");
    let high_timestamp = CommitTimelineRows::from_entry(entry(2, 2, 2_000)).expect("timeline rows");
    let higher_version_same_timestamp =
        CommitTimelineRows::from_entry(entry(2, 3, 2_000)).expect("timeline rows");
    let other_branch = CommitTimelineRows::from_entry(entry(3, 1, 1_000)).expect("timeline rows");

    assert!(
        low_timestamp
            .timestamp_to_version()
            .physical_key()
            .user_key()
            < high_timestamp
                .timestamp_to_version()
                .physical_key()
                .user_key()
    );
    assert!(
        high_timestamp
            .timestamp_to_version()
            .physical_key()
            .user_key()
            < higher_version_same_timestamp
                .timestamp_to_version()
                .physical_key()
                .user_key()
    );
    assert!(
        low_timestamp
            .version_to_timestamp()
            .physical_key()
            .user_key()
            < high_timestamp
                .version_to_timestamp()
                .physical_key()
                .user_key()
    );
    assert_ne!(
        low_timestamp.timestamp_to_version().physical_key(),
        other_branch.timestamp_to_version().physical_key()
    );
    assert_ne!(
        low_timestamp.version_to_timestamp().physical_key(),
        other_branch.version_to_timestamp().physical_key()
    );
}

#[test]
fn timeline_fact_validation_round_trips_both_index_rows() {
    let entry = entry(2, 8, 2_000);
    let rows = CommitTimelineRows::from_entry(entry).expect("timeline rows");

    let timestamp_fact =
        CommitTimelineFact::validate(rows.timestamp_to_version()).expect("timestamp fact");
    let version_fact =
        CommitTimelineFact::validate(rows.version_to_timestamp()).expect("version fact");

    assert_eq!(timestamp_fact.entry(), entry);
    assert_eq!(
        timestamp_fact.kind(),
        CommitTimelineRowKind::TimestampToVersion
    );
    assert_eq!(version_fact.entry(), entry);
    assert_eq!(
        version_fact.kind(),
        CommitTimelineRowKind::VersionToTimestamp
    );
}

#[test]
fn timeline_fact_validation_rejects_malformed_row_shape() {
    let entry = entry(3, 9, 3_000);
    let rows = CommitTimelineRows::from_entry(entry).expect("timeline rows");

    let wrong_space = StorageRow::put(
        physical_key(entry.branch_id(), 0x20, b"ts-v1\0bad".to_vec()),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        9_u64.to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&wrong_space),
        "timeline row uses the wrong storage space",
    );

    let wrong_named_space = StorageRow::put(
        PhysicalKey::new(
            entry.branch_id(),
            "not-timeline",
            StorageSpaceId::COMMIT_TIMELINE,
            rows.timestamp_to_version()
                .physical_key()
                .user_key()
                .to_vec(),
        )
        .expect("wrong named space key"),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&wrong_named_space),
        "timeline row uses the wrong physical key space",
    );

    let unknown_prefix = StorageRow::put(
        storage_owned_key(entry.branch_id(), b"unknown".to_vec()),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        9_u64.to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&unknown_prefix),
        "timeline row has unknown key prefix",
    );

    let expiring = StorageRow::put(
        rows.version_to_timestamp().physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::from_micros(1),
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&expiring),
        "timeline row must not expire",
    );

    let tombstone = StorageRow::tombstone(
        rows.version_to_timestamp().physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&tombstone),
        "timeline row must be a put row",
    );

    let zero_version = StorageRow::put(
        rows.version_to_timestamp().physical_key().clone(),
        CommitVersion::ZERO,
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&zero_version),
        "timeline row version must be nonzero",
    );
}

#[test]
fn timeline_fact_validation_rejects_mismatched_row_facts() {
    let entry = entry(3, 9, 3_000);
    let rows = CommitTimelineRows::from_entry(entry).expect("timeline rows");

    let short_value = StorageRow::put(
        rows.timestamp_to_version().physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        [1, 2, 3],
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&short_value),
        "timeline value has invalid length",
    );

    let mismatched_value = StorageRow::put(
        rows.timestamp_to_version().physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        10_u64.to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&mismatched_value),
        "timestamp timeline key and value disagree",
    );

    let mismatched_timestamp = StorageRow::put(
        rows.timestamp_to_version().physical_key().clone(),
        entry.commit_version(),
        Timestamp::from_micros(3_001),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&mismatched_timestamp),
        "timestamp timeline row timestamp disagrees with key",
    );

    let mismatched_timestamp_row_version = StorageRow::put(
        rows.timestamp_to_version().physical_key().clone(),
        CommitVersion::new(10),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&mismatched_timestamp_row_version),
        "timestamp timeline row version disagrees with key",
    );

    let mismatched_version = StorageRow::put(
        rows.version_to_timestamp().physical_key().clone(),
        CommitVersion::new(10),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&mismatched_version),
        "version timeline row version disagrees with key",
    );

    let mismatched_version_timestamp = StorageRow::put(
        rows.version_to_timestamp().physical_key().clone(),
        entry.commit_version(),
        Timestamp::from_micros(3_001),
        Timestamp::EPOCH,
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&mismatched_version_timestamp),
        "version timeline row timestamp disagrees with value",
    );

    let short_version_value = StorageRow::put(
        rows.version_to_timestamp().physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        [4, 5, 6],
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&short_version_value),
        "timeline value has invalid length",
    );
}

#[test]
fn timeline_fact_validation_rejects_malformed_key_lengths() {
    let entry = entry(3, 9, 3_000);

    let mut short_timestamp_key = b"ts-v1\0".to_vec();
    short_timestamp_key.extend_from_slice(&entry.commit_timestamp().as_micros().to_be_bytes());
    let short_timestamp = StorageRow::put(
        storage_owned_key(entry.branch_id(), short_timestamp_key),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&short_timestamp),
        "timestamp timeline key has invalid length",
    );

    let mut long_version_key = b"ver-v1\0".to_vec();
    long_version_key.extend_from_slice(&entry.commit_version().as_u64().to_be_bytes());
    long_version_key.push(0);
    let long_version = StorageRow::put(
        storage_owned_key(entry.branch_id(), long_version_key),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(
        CommitTimelineFact::validate(&long_version),
        "version timeline key has invalid length",
    );
}

#[test]
fn timeline_view_resolves_timestamp_lookup_with_version_tiebreak() {
    let rows = timeline_rows([
        entry(4, 1, 1_000),
        entry(4, 2, 2_000),
        entry(4, 3, 2_000),
        entry(4, 4, 4_000),
    ]);
    let view = CommitTimelineView::from_rows(branch_id(4), rows.iter()).expect("timeline view");

    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(500)),
        500,
        None,
        None,
        CommitTimelineMiss::BeforeRetainedHistory,
    );
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(1_500)),
        1_500,
        Some(1),
        Some(1_000),
        CommitTimelineMiss::Matched,
    );
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(2_000)),
        2_000,
        Some(3),
        Some(2_000),
        CommitTimelineMiss::Matched,
    );
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(9_000)),
        9_000,
        Some(4),
        Some(4_000),
        CommitTimelineMiss::AfterLatestRetained,
    );
    assert_eq!(
        view.timestamp_for_version(CommitVersion::new(2)),
        Some(Timestamp::from_micros(2_000))
    );
    assert_eq!(view.timestamp_for_version(CommitVersion::new(99)), None);

    let exact = view.version_at_or_before(Timestamp::from_micros(2_000));
    assert_eq!(exact.query_timestamp(), Timestamp::from_micros(2_000));
    assert_eq!(exact.matched_version(), Some(CommitVersion::new(3)));
    assert_eq!(
        exact.matched_timestamp(),
        Some(Timestamp::from_micros(2_000))
    );
    assert_eq!(exact.miss(), CommitTimelineMiss::Matched);
}

#[test]
fn timeline_view_resolves_earliest_and_latest_exact_timestamps() {
    let rows = timeline_rows([
        entry(4, 1, 1_000),
        entry(4, 2, 2_000),
        entry(4, 3, 2_000),
        entry(4, 4, 4_000),
    ]);
    let view = CommitTimelineView::from_rows(branch_id(4), rows.iter()).expect("timeline view");

    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(1_000)),
        1_000,
        Some(1),
        Some(1_000),
        CommitTimelineMiss::Matched,
    );
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(4_000)),
        4_000,
        Some(4),
        Some(4_000),
        CommitTimelineMiss::Matched,
    );
}

#[test]
fn timeline_view_is_branch_local_and_skips_non_timeline_rows() {
    let target = branch_id(5);
    let other = branch_id(6);
    let mut rows = timeline_rows([entry(5, 1, 10), entry(5, 2, 20), entry(6, 9, 5)]);
    rows.push(StorageRow::put(
        physical_key(target, 0x20, b"user-row".to_vec()),
        CommitVersion::new(10),
        Timestamp::from_micros(10),
        Timestamp::EPOCH,
        b"value",
    ));

    let view = CommitTimelineView::from_rows(target, rows.iter()).expect("timeline view");

    assert_eq!(view.branch_id(), target);
    assert_eq!(view.entries().len(), 2);
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(10)),
        10,
        Some(1),
        Some(10),
        CommitTimelineMiss::Matched,
    );

    let other_view = CommitTimelineView::from_rows(other, rows.iter()).expect("other view");
    assert_eq!(other_view.entries().len(), 1);
    assert_lookup(
        other_view.version_at_or_before(Timestamp::from_micros(10)),
        10,
        Some(9),
        Some(5),
        CommitTimelineMiss::AfterLatestRetained,
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn timeline_perf_trace_counts_view_rows_facts_and_lookup_scans() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let target = branch_id(5);
    let mut rows = timeline_rows([entry(5, 1, 10), entry(5, 2, 20), entry(6, 9, 90)]);
    rows.push(StorageRow::put(
        physical_key(target, 0x20, b"user-row".to_vec()),
        CommitVersion::new(3),
        Timestamp::from_micros(30),
        Timestamp::EPOCH,
        b"value".to_vec(),
    ));
    let supplied_rows = rows.len();

    let view = CommitTimelineView::from_rows(target, rows.iter()).expect("timeline view");
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(15)),
        15,
        Some(1),
        Some(10),
        CommitTimelineMiss::Matched,
    );

    let perf = crate::observability::perf_trace::snapshot();
    let supplied_rows = u64::try_from(supplied_rows).expect("supplied row count fits u64");
    assert_eq!(perf.commit_timeline_view_rows_scanned(), supplied_rows);
    assert_eq!(perf.commit_timeline_timestamp_facts(), 2);
    assert_eq!(perf.commit_timeline_version_facts(), 2);
    assert_eq!(perf.commit_timeline_reconcile_calls(), 1);
    assert_eq!(perf.commit_timeline_reconcile_timestamp_facts(), 2);
    assert_eq!(perf.commit_timeline_reconcile_version_facts(), 2);
    assert_eq!(perf.commit_timeline_reconcile_entry_checks(), 8);
    assert_eq!(perf.commit_timeline_lookup_calls(), 1);
    assert_eq!(perf.commit_timeline_lookup_entries_scanned(), 2);
}

#[cfg(feature = "perf-trace")]
#[test]
fn timeline_perf_trace_bounds_large_lookup_and_reconciliation() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let retained_entries = 128usize;
    let rows = timeline_rows((1..=retained_entries).map(|version| {
        entry(
            12,
            u64::try_from(version).expect("version fits u64"),
            u64::try_from(version * 10).expect("timestamp fits u64"),
        )
    }));

    let view = CommitTimelineView::from_rows(branch_id(12), rows.iter()).expect("timeline view");
    assert_lookup(
        view.version_at_or_before(Timestamp::from_micros(640)),
        640,
        Some(64),
        Some(640),
        CommitTimelineMiss::Matched,
    );
    assert_eq!(
        view.timestamp_for_version(CommitVersion::new(127)),
        Some(Timestamp::from_micros(1_270))
    );

    let perf = crate::observability::perf_trace::snapshot();
    let retained = u64::try_from(retained_entries).expect("retained count fits u64");
    assert_eq!(perf.commit_timeline_timestamp_facts(), retained);
    assert_eq!(perf.commit_timeline_version_facts(), retained);
    assert_eq!(perf.commit_timeline_reconcile_calls(), 1);
    assert_eq!(perf.commit_timeline_reconcile_timestamp_facts(), retained);
    assert_eq!(perf.commit_timeline_reconcile_version_facts(), retained);
    assert_eq!(perf.commit_timeline_reconcile_entry_checks(), retained * 4);
    assert_eq!(perf.commit_timeline_lookup_calls(), 1);
    assert_eq!(perf.commit_timeline_lookup_entries_scanned(), 8);
}

#[test]
fn timeline_version_lookup_is_branch_local() {
    let mut rows = timeline_rows([entry(5, 1, 10), entry(5, 2, 20), entry(6, 1, 90)]);
    rows.push(StorageRow::put(
        physical_key(branch_id(5), 0x20, b"user-row".to_vec()),
        CommitVersion::new(1),
        Timestamp::from_micros(10),
        Timestamp::EPOCH,
        b"value",
    ));

    let target = CommitTimelineView::from_rows(branch_id(5), rows.iter()).expect("timeline view");
    let other = CommitTimelineView::from_rows(branch_id(6), rows.iter()).expect("timeline view");

    assert_eq!(
        target.timestamp_for_version(CommitVersion::new(1)),
        Some(Timestamp::from_micros(10))
    );
    assert_eq!(
        other.timestamp_for_version(CommitVersion::new(1)),
        Some(Timestamp::from_micros(90))
    );
    assert_eq!(other.timestamp_for_version(CommitVersion::new(2)), None);
}

#[test]
fn timeline_view_reports_bounds_without_claiming_completeness() {
    let empty = CommitTimelineView::from_rows(branch_id(7), [].iter()).expect("empty view");
    assert_eq!(empty.bounds().min_timestamp(), None);
    assert_eq!(empty.bounds().max_timestamp(), None);
    assert_eq!(empty.bounds().min_version(), None);
    assert_eq!(empty.bounds().max_version(), None);
    assert_lookup(
        empty.version_at_or_before(Timestamp::from_micros(1)),
        1,
        None,
        None,
        CommitTimelineMiss::Empty,
    );

    let single_rows = timeline_rows([entry(7, 2, 20)]);
    let single =
        CommitTimelineView::from_rows(branch_id(7), single_rows.iter()).expect("timeline view");
    assert_eq!(
        single.bounds().min_timestamp(),
        Some(Timestamp::from_micros(20))
    );
    assert_eq!(
        single.bounds().max_timestamp(),
        Some(Timestamp::from_micros(20))
    );
    assert_eq!(single.bounds().min_version(), Some(CommitVersion::new(2)));
    assert_eq!(single.bounds().max_version(), Some(CommitVersion::new(2)));

    let rows = timeline_rows([entry(7, 4, 40), entry(7, 2, 20), entry(7, 6, 20)]);
    let view = CommitTimelineView::from_rows(branch_id(7), rows.iter()).expect("timeline view");

    assert_eq!(
        view.bounds().min_timestamp(),
        Some(Timestamp::from_micros(20))
    );
    assert_eq!(
        view.bounds().max_timestamp(),
        Some(Timestamp::from_micros(40))
    );
    assert_eq!(view.bounds().min_version(), Some(CommitVersion::new(2)));
    assert_eq!(view.bounds().max_version(), Some(CommitVersion::new(6)));
}

#[test]
fn timeline_view_sorts_shuffled_rows_deterministically() {
    let ordered = timeline_rows([
        entry(10, 1, 1_000),
        entry(10, 2, 2_000),
        entry(10, 3, 2_000),
        entry(10, 4, 4_000),
    ]);
    let shuffled = [
        ordered[5].clone(),
        ordered[0].clone(),
        ordered[7].clone(),
        ordered[2].clone(),
        ordered[1].clone(),
        ordered[4].clone(),
        ordered[3].clone(),
        ordered[6].clone(),
    ];

    let ordered_view = CommitTimelineView::from_rows(branch_id(10), ordered.iter())
        .expect("ordered timeline view");
    let shuffled_view = CommitTimelineView::from_rows(branch_id(10), shuffled.iter())
        .expect("shuffled timeline view");

    assert_eq!(ordered_view, shuffled_view);
    assert_eq!(
        shuffled_view
            .entries()
            .iter()
            .map(|entry| (entry.commit_timestamp(), entry.commit_version()))
            .collect::<Vec<_>>(),
        vec![
            (Timestamp::from_micros(1_000), CommitVersion::new(1)),
            (Timestamp::from_micros(2_000), CommitVersion::new(2)),
            (Timestamp::from_micros(2_000), CommitVersion::new(3)),
            (Timestamp::from_micros(4_000), CommitVersion::new(4)),
        ]
    );
}

#[test]
fn timeline_view_rejects_missing_or_conflicting_index_pairs() {
    let base_entry = entry(8, 10, 10_000);
    let rows = CommitTimelineRows::from_entry(base_entry).expect("timeline rows");

    expect_invalid_timeline_fact(
        CommitTimelineView::from_rows(branch_id(8), [rows.timestamp_to_version()]),
        "timestamp timeline fact is missing version index",
    );
    expect_invalid_timeline_fact(
        CommitTimelineView::from_rows(branch_id(8), [rows.version_to_timestamp()]),
        "version timeline fact is missing timestamp index",
    );

    let duplicate_idempotent = CommitTimelineView::from_rows(
        branch_id(8),
        [
            rows.timestamp_to_version(),
            rows.timestamp_to_version(),
            rows.version_to_timestamp(),
            rows.version_to_timestamp(),
        ],
    )
    .expect("duplicate identical timeline rows are idempotent");
    assert_eq!(duplicate_idempotent.entries().len(), 1);

    let disagreeing_pair =
        CommitTimelineRows::from_entry(entry(8, 10, 10_001)).expect("disagreeing timeline rows");
    expect_timeline_conflict(
        CommitTimelineView::from_rows(
            branch_id(8),
            [
                rows.timestamp_to_version(),
                disagreeing_pair.version_to_timestamp(),
            ],
        ),
        "timeline indexes disagree for a commit version",
    );

    let conflicting_version =
        CommitTimelineRows::from_entry(entry(8, 10, 10_001)).expect("conflicting timeline rows");
    expect_timeline_conflict(
        CommitTimelineView::from_rows(
            branch_id(8),
            [
                rows.timestamp_to_version(),
                rows.version_to_timestamp(),
                conflicting_version.version_to_timestamp(),
            ],
        ),
        "conflicting duplicate version timeline fact",
    );
}

#[test]
fn caller_storage_owned_timeline_mutations_remain_rejected() {
    let branch = branch_id(9);
    let batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            storage_owned_key(branch, b"caller-forged-timeline".to_vec()),
            b"forged",
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    assert_eq!(
        batch.validate(&CommitRuntimeConfig::default()),
        Err(CommitRuntimeError::StorageOwnedMutationSpace {
            space_id: StorageSpaceId::COMMIT_TIMELINE,
        })
    );

    let delete_batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(storage_owned_key(
            branch,
            b"caller-delete-timeline".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );
    assert_eq!(
        delete_batch.validate(&CommitRuntimeConfig::default()),
        Err(CommitRuntimeError::StorageOwnedMutationSpace {
            space_id: StorageSpaceId::COMMIT_TIMELINE,
        })
    );
}

#[test]
fn timeline_error_display_stays_storage_shaped() {
    let display = CommitRuntimeError::InvalidTimelineFact {
        reason: "timeline row must be a put row",
    }
    .to_string();
    assert!(display.contains("commit timeline fact"));
    assert_bounded_storage_debug(&display);
    assert!(!display.contains("as_of"));
    assert!(!display.contains("JSON"));
}

fn entry(branch: u8, version: u64, timestamp: u64) -> CommitTimelineEntry {
    CommitTimelineEntry::new(
        branch_id(branch),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    )
    .expect("timeline entry")
}

fn timeline_rows(entries: impl IntoIterator<Item = CommitTimelineEntry>) -> Vec<StorageRow> {
    entries
        .into_iter()
        .flat_map(|entry| {
            CommitTimelineRows::from_entry(entry)
                .expect("timeline rows")
                .into_rows()
        })
        .collect()
}

fn assert_lookup(
    actual: CommitTimelineLookup,
    query_timestamp: u64,
    matched_version: Option<u64>,
    matched_timestamp: Option<u64>,
    miss: CommitTimelineMiss,
) {
    assert_eq!(
        actual.query_timestamp(),
        Timestamp::from_micros(query_timestamp)
    );
    assert_eq!(
        actual.matched_version(),
        matched_version.map(CommitVersion::new)
    );
    assert_eq!(
        actual.matched_timestamp(),
        matched_timestamp.map(Timestamp::from_micros)
    );
    assert_eq!(actual.miss(), miss);
}

fn expect_invalid_timeline_fact<T>(result: CommitRuntimeResult<T>, expected_reason: &'static str) {
    assert_eq!(
        result.err(),
        Some(CommitRuntimeError::InvalidTimelineFact {
            reason: expected_reason,
        })
    );
}

fn expect_timeline_conflict<T>(result: CommitRuntimeResult<T>, expected_reason: &'static str) {
    assert_eq!(
        result.err(),
        Some(CommitRuntimeError::TimelineConflict {
            reason: expected_reason,
        })
    );
}
