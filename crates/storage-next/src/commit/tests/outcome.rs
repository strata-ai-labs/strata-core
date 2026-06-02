use super::*;

#[test]
fn read_snapshot_preserves_branch_and_visible_version() {
    let branch = branch_id(60);
    let snapshot = CommitReadSnapshot::new(branch, CommitVersion::new(11));

    assert_eq!(snapshot.branch_id(), branch);
    assert_eq!(snapshot.visible_version(), CommitVersion::new(11));
}

#[test]
fn read_snapshot_zero_visible_version_is_storage_bounded_debug_fact() {
    let branch = branch_id(60);
    let snapshot = CommitReadSnapshot::new(branch, CommitVersion::ZERO);
    let debug = format!("{snapshot:?}");

    assert_eq!(snapshot.branch_id(), branch);
    assert_eq!(snapshot.visible_version(), CommitVersion::ZERO);
    assert_bounded_storage_debug(&debug);
}

#[test]
fn mutation_counts_are_derived_from_validated_mutating_batches() {
    let branch = branch_id(61);
    let batch = CommitBatch::mutating(
        branch,
        vec![
            CommitMutation::put(
                physical_key(branch, 0x20, b"put-a".to_vec()),
                b"value-a".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(physical_key(branch, 0x21, b"delete".to_vec())),
            CommitMutation::put(
                physical_key(branch, 0x22, b"put-b".to_vec()),
                b"value-b".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch");

    let counts = CommitMutationCounts::from_validated_batch(&batch).expect("mutation counts");

    assert_eq!(counts.puts(), 2);
    assert_eq!(counts.deletes(), 1);
    assert_eq!(counts.timeline_rows(), 0);
}

#[test]
fn mutation_counts_cover_read_only_and_delete_only_batches_without_stamping() {
    let branch = branch_id(61);
    let read_only = read_only_batch(branch, CommitBatchOptions::default());
    let delete_only = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x20,
            b"delete-only".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid delete-only batch");

    assert_eq!(
        CommitMutationCounts::from_validated_batch(&read_only).expect("read-only counts"),
        CommitMutationCounts::read_only()
    );

    let delete_counts =
        CommitMutationCounts::from_validated_batch(&delete_only).expect("delete-only counts");
    assert_eq!(delete_counts.puts(), 0);
    assert_eq!(delete_counts.deletes(), 1);
    assert_eq!(delete_counts.timeline_rows(), 0);

    let rows = delete_only
        .stamp_user_rows(stamp(branch, 91))
        .expect("stamp rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(delete_counts.timeline_rows(), 0);
}

#[test]
fn read_only_diagnostic_returns_snapshot_without_commit_facts() {
    let branch = branch_id(62);
    let visible = VisibleVersionTracker::new(CommitVersion::new(33));
    let batch = read_only_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Always,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(99)),
            CommitOrigin::Diagnostic,
        ),
    );

    let outcome = execute_read_only_diagnostic(&batch, &CommitRuntimeConfig::default(), visible)
        .expect("read-only diagnostic outcome");

    assert_eq!(outcome.kind(), CommitOutcomeKind::ReadOnly);
    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.phase(), CommitPhase::RejectedBeforeAllocation);
    assert_eq!(outcome.durability(), CommitDurabilityClass::NotDurable);
    assert_eq!(outcome.commit_version(), None);
    assert_eq!(outcome.commit_timestamp(), None);
    assert_eq!(outcome.mutation_counts(), CommitMutationCounts::read_only());
    assert_eq!(outcome.visibility_facts(), CommitVisibilityFacts::empty());
    assert_eq!(
        outcome.read_snapshot(),
        Some(CommitReadSnapshot::new(branch, CommitVersion::new(33)))
    );
    assert_bounded_storage_debug(&format!("{outcome:?}"));
}

#[test]
fn read_only_diagnostic_rejects_when_disabled_before_any_runtime_work() {
    let branch = branch_id(63);
    let visible = VisibleVersionTracker::new(CommitVersion::new(7));
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Disabled)
        .expect("valid disabled config");
    let batch = read_only_batch(branch, CommitBatchOptions::default());

    assert_eq!(
        execute_read_only_diagnostic(&batch, &config, visible),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        })
    );
    assert_eq!(visible.visible_version(), CommitVersion::new(7));
}

#[test]
fn read_only_diagnostic_disabled_cannot_be_bypassed_by_options() {
    let branch = branch_id(63);
    let visible = VisibleVersionTracker::new(CommitVersion::new(7));
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Disabled)
        .expect("valid disabled config");
    let batch = read_only_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Always,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(77)),
            CommitOrigin::Diagnostic,
        ),
    );

    assert_eq!(
        execute_read_only_diagnostic(&batch, &config, visible),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        })
    );
    assert_eq!(visible.visible_version(), CommitVersion::new(7));
}

#[test]
fn read_only_diagnostic_rejects_mutating_batches() {
    let branch = branch_id(64);
    let visible = VisibleVersionTracker::default();
    let batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x20,
            b"delete".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch");

    assert_eq!(
        execute_read_only_diagnostic(&batch, &CommitRuntimeConfig::default(), visible),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "read-only diagnostic executor requires read-only batch",
        })
    );
}

#[test]
fn read_only_diagnostic_keeps_branch_fact_separate_from_global_visible_version() {
    let branch_a = branch_id(64);
    let branch_b = branch_id(65);
    let visible = VisibleVersionTracker::new(CommitVersion::new(44));

    let outcome_a = execute_read_only_diagnostic(
        &read_only_batch(branch_a, CommitBatchOptions::default()),
        &CommitRuntimeConfig::default(),
        visible,
    )
    .expect("branch A read-only outcome");
    let outcome_b = execute_read_only_diagnostic(
        &read_only_batch(branch_b, CommitBatchOptions::default()),
        &CommitRuntimeConfig::default(),
        visible,
    )
    .expect("branch B read-only outcome");

    assert_ne!(outcome_a.branch_id(), outcome_b.branch_id());
    assert_eq!(
        outcome_a
            .read_snapshot()
            .map(CommitReadSnapshot::visible_version),
        Some(CommitVersion::new(44))
    );
    assert_eq!(
        outcome_b
            .read_snapshot()
            .map(CommitReadSnapshot::visible_version),
        Some(CommitVersion::new(44))
    );
    assert_eq!(visible.visible_version(), CommitVersion::new(44));
}

#[test]
fn outcome_constructors_reject_impossible_read_only_shapes() {
    let branch = branch_id(65);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(1), Timestamp::from_micros(1)).expect("stamp");

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::ReadOnly,
            CommitPhase::RejectedBeforeAllocation,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            CommitMutationCounts::read_only(),
            CommitVisibilityFacts::empty(),
            Some(CommitReadSnapshot::new(branch, CommitVersion::ZERO)),
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "read-only outcome must not carry commit facts",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::ReadOnly,
            CommitPhase::RejectedBeforeAllocation,
            CommitDurabilityClass::Standard,
            None,
            CommitMutationCounts::read_only(),
            CommitVisibilityFacts::empty(),
            Some(CommitReadSnapshot::new(branch, CommitVersion::ZERO)),
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "read-only outcome must not claim durability",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::ReadOnly,
            CommitPhase::RejectedBeforeAllocation,
            CommitDurabilityClass::NotDurable,
            None,
            mutation_counts(1, 0, 0),
            CommitVisibilityFacts::empty(),
            Some(CommitReadSnapshot::new(branch, CommitVersion::ZERO)),
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "read-only outcome must not report mutations",
        })
    );
}

#[test]
fn mutation_count_constructor_rejects_values_above_configured_limits() {
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled)
        .expect("valid small config");

    assert_eq!(
        CommitMutationCounts::new(2, 0, 0, &config),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "mutation count exceeds configured limit",
        })
    );
    assert_eq!(
        CommitMutationCounts::new(1, 0, 1, &config),
        Err(CommitRuntimeError::InvalidBatch {
            reason: "commit row count exceeds configured limit",
        })
    );
}

#[test]
fn visible_outcome_constructor_validates_visible_shape() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let counts = mutation_counts(1, 1, 0);
    let visible_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("visible facts");
    let outcome = CommitOutcome::visible(
        branch,
        stamp,
        CommitDurabilityClass::NotDurable,
        counts,
        visible_facts,
    )
    .expect("visible outcome");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Visible);
    assert_eq!(outcome.commit_version(), Some(stamp.commit_version()));
    assert_eq!(outcome.commit_timestamp(), Some(stamp.commit_timestamp()));
    assert_bounded_storage_debug(&format!("{outcome:?}"));

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::Visible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            visible_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "visible outcome must use visible phase",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::Visible,
            CommitPhase::Visible,
            CommitDurabilityClass::NotDurable,
            None,
            counts,
            visible_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "visible outcome must carry commit facts",
        })
    );
}

#[test]
fn visible_outcome_constructor_rejects_durability_fact_mismatches() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let counts = mutation_counts(1, 1, 0);
    let cache_visible_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("cache visible facts");
    let durable_visible_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("durable visible facts");

    assert_eq!(
        CommitOutcome::visible(
            branch,
            stamp,
            CommitDurabilityClass::Standard,
            counts,
            cache_visible_facts,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable outcome must preserve durable version",
        })
    );
    assert_eq!(
        CommitOutcome::visible(
            branch,
            stamp,
            CommitDurabilityClass::NotDurable,
            counts,
            durable_visible_facts,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "non-durable outcome must not preserve durable version",
        })
    );
    assert_eq!(
        CommitOutcome::visible(
            branch,
            stamp,
            CommitDurabilityClass::Uncertain,
            counts,
            durable_visible_facts,
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "outcome must not claim uncertain durability",
        })
    );
}

#[test]
fn outcome_constructor_validates_visibility_facts_before_outcome_shape() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(1), Timestamp::from_micros(10)).expect("stamp");
    let invalid_facts = CommitVisibilityFacts::from_parts_unchecked(
        Some(CommitVersion::new(1)),
        None,
        Some(CommitVersion::new(2)),
        Some(CommitVersion::new(2)),
        Some(CommitVersion::new(2)),
    );

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::Visible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0),
            invalid_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied version must not exceed allocated version",
        })
    );
}

#[test]
fn not_visible_outcome_constructor_accepts_valid_progress_facts() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let counts = mutation_counts(1, 1, 0);
    let allocated_facts =
        CommitVisibilityFacts::new(Some(stamp.commit_version()), None, None, None, None)
            .expect("allocated facts");
    let applied_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        None,
        None,
    )
    .expect("applied facts");
    let allocated_not_visible = CommitOutcome::new(
        branch,
        CommitOutcomeKind::NotVisible,
        CommitPhase::AllocatedNotDurable,
        CommitDurabilityClass::NotDurable,
        Some(stamp),
        counts,
        allocated_facts,
        None,
    )
    .expect("allocated not-visible outcome");
    let applied_not_visible = CommitOutcome::new(
        branch,
        CommitOutcomeKind::NotVisible,
        CommitPhase::AppliedNotVisible,
        CommitDurabilityClass::NotDurable,
        Some(stamp),
        counts,
        applied_facts,
        None,
    )
    .expect("applied not-visible outcome");

    assert_eq!(allocated_not_visible.kind(), CommitOutcomeKind::NotVisible);
    assert_eq!(
        allocated_not_visible.visibility_facts().allocated_version(),
        Some(stamp.commit_version())
    );
    assert_eq!(
        allocated_not_visible.visibility_facts().applied_version(),
        None
    );
    assert_eq!(applied_not_visible.kind(), CommitOutcomeKind::NotVisible);
    assert_eq!(
        applied_not_visible.visibility_facts().applied_version(),
        Some(stamp.commit_version())
    );
}

#[test]
fn not_visible_outcome_constructor_rejects_inconsistent_progress_facts() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let counts = mutation_counts(1, 1, 0);
    let allocated_facts =
        CommitVisibilityFacts::new(Some(stamp.commit_version()), None, None, None, None)
            .expect("allocated facts");
    let visible_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("visible facts");

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            visible_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "not-visible outcome must not publish its commit version",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            CommitVisibilityFacts::empty(),
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "not-visible outcome must preserve allocated version",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            allocated_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied not-visible outcome must preserve applied version",
        })
    );
}

#[test]
fn not_visible_outcome_constructor_rejects_durable_facts_or_class() {
    let branch = branch_id(66);
    let stamp =
        CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let counts = mutation_counts(1, 1, 0);
    let allocated_facts =
        CommitVisibilityFacts::new(Some(stamp.commit_version()), None, None, None, None)
            .expect("allocated facts");
    let durable_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        None,
        None,
        None,
    )
    .expect("durable facts");

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::Standard,
            Some(stamp),
            counts,
            allocated_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "not-visible outcome must not claim durability",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            durable_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "not-visible outcome must not preserve durable version",
        })
    );
}

#[test]
fn outcome_constructor_rejects_stamp_branch_mismatch() {
    let branch = branch_id(69);
    let other = branch_id(70);
    let stamp =
        CommitStamp::new(other, CommitVersion::new(8), Timestamp::from_micros(80)).expect("stamp");
    let facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("visible facts");

    assert_eq!(
        CommitOutcome::visible(
            branch,
            stamp,
            CommitDurabilityClass::NotDurable,
            mutation_counts(1, 0, 0),
            facts,
        ),
        Err(CommitRuntimeError::BranchMismatch {
            expected: branch,
            actual: other,
        })
    );
}

#[test]
fn durable_but_not_visible_outcome_preserves_durable_facts() {
    let branch = branch_id(67);
    let stamp = CommitStamp::new(branch, CommitVersion::new(12), Timestamp::from_micros(120))
        .expect("stamp");
    let counts = mutation_counts(1, 0, 0);
    let durable_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        None,
        None,
        None,
    )
    .expect("durable facts");
    let applied_durable_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        None,
        None,
    )
    .expect("applied durable facts");

    let outcome = CommitOutcome::durable_but_not_visible(
        branch,
        stamp,
        CommitDurabilityClass::Always,
        counts,
        durable_facts,
    )
    .expect("durable not visible outcome");

    assert_eq!(outcome.kind(), CommitOutcomeKind::DurableButNotVisible);
    assert_eq!(outcome.durability(), CommitDurabilityClass::Always);
    assert_eq!(
        outcome.visibility_facts().durable_version(),
        Some(stamp.commit_version())
    );
    assert_eq!(
        outcome.visibility_facts().allocated_version(),
        Some(stamp.commit_version())
    );
    assert_eq!(outcome.visibility_facts().visible_version(), None);

    let applied_outcome = CommitOutcome::new(
        branch,
        CommitOutcomeKind::DurableButNotVisible,
        CommitPhase::AppliedNotVisible,
        CommitDurabilityClass::Always,
        Some(stamp),
        counts,
        applied_durable_facts,
        None,
    )
    .expect("applied durable not-visible outcome");
    assert_eq!(
        applied_outcome.kind(),
        CommitOutcomeKind::DurableButNotVisible
    );
    assert_eq!(
        applied_outcome.visibility_facts().applied_version(),
        Some(stamp.commit_version())
    );
}

#[test]
fn durable_but_not_visible_outcome_rejects_inconsistent_progress_facts() {
    let branch = branch_id(67);
    let stamp = CommitStamp::new(branch, CommitVersion::new(12), Timestamp::from_micros(120))
        .expect("stamp");
    let counts = mutation_counts(1, 0, 0);
    let durable_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        None,
        None,
        None,
    )
    .expect("durable facts");

    assert_eq!(
        CommitOutcome::durable_but_not_visible(
            branch,
            stamp,
            CommitDurabilityClass::NotDurable,
            counts,
            durable_facts,
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable-but-not-visible outcome must claim durable WAL success",
        })
    );
    assert_eq!(
        CommitOutcome::durable_but_not_visible(
            branch,
            stamp,
            CommitDurabilityClass::Uncertain,
            counts,
            durable_facts,
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable-but-not-visible outcome must claim durable WAL success",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::DurableNotApplied,
            CommitDurabilityClass::Always,
            Some(stamp),
            counts,
            CommitVisibilityFacts::new(
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                None,
                None,
            )
            .expect("applied durable facts"),
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable-not-applied outcome must not preserve applied version",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::Always,
            Some(stamp),
            counts,
            durable_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "applied durable outcome must preserve applied version",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::DurableNotApplied,
            CommitDurabilityClass::Always,
            Some(stamp),
            counts,
            CommitVisibilityFacts::empty(),
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable-but-not-visible outcome must preserve allocated version",
        })
    );
}

#[test]
fn durable_but_not_visible_outcome_rejects_visible_publication() {
    let branch = branch_id(67);
    let stamp = CommitStamp::new(branch, CommitVersion::new(12), Timestamp::from_micros(120))
        .expect("stamp");
    let visible_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("visible facts");

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0),
            visible_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable-but-not-visible outcome must not publish visibility",
        })
    );
}

#[test]
fn outcome_constructor_rejects_kind_phase_mismatches() {
    let branch = branch_id(71);
    let stamp = CommitStamp::new(branch, CommitVersion::new(18), Timestamp::from_micros(180))
        .expect("stamp");
    let counts = mutation_counts(1, 0, 0);
    let allocated_facts =
        CommitVisibilityFacts::new(Some(stamp.commit_version()), None, None, None, None)
            .expect("allocated facts");
    let durable_facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        None,
        None,
        None,
    )
    .expect("durable facts");

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::ReadOnly,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            None,
            CommitMutationCounts::read_only(),
            CommitVisibilityFacts::empty(),
            Some(CommitReadSnapshot::new(branch, CommitVersion::ZERO)),
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only outcome must use rejected-before-allocation phase",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::Replay,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            counts,
            allocated_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "not-visible outcome must use an allocated or applied non-visible phase",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::DurableNotApplied,
            CommitDurabilityClass::Standard,
            Some(stamp),
            counts,
            durable_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "not-visible outcome must use an allocated or applied non-visible phase",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::Visible,
            CommitDurabilityClass::Always,
            Some(stamp),
            counts,
            durable_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "durable-but-not-visible outcome must use a durable non-visible phase",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::Always,
            Some(stamp),
            counts,
            durable_facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "durable-but-not-visible outcome must use a durable non-visible phase",
        })
    );
}

#[test]
fn replay_outcome_preserves_commit_facts_without_read_snapshot() {
    let branch = branch_id(68);
    let stamp = CommitStamp::new(branch, CommitVersion::new(14), Timestamp::from_micros(140))
        .expect("stamp");
    let facts = CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
    .expect("replay visible facts");

    let outcome = CommitOutcome::new(
        branch,
        CommitOutcomeKind::Replay,
        CommitPhase::Replay,
        CommitDurabilityClass::Always,
        Some(stamp),
        mutation_counts(1, 0, 0),
        facts,
        None,
    )
    .expect("replay outcome");

    assert_eq!(outcome.kind(), CommitOutcomeKind::Replay);
    assert_eq!(outcome.commit_version(), Some(stamp.commit_version()));

    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::Replay,
            CommitPhase::Replay,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0),
            facts,
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "non-durable outcome must not preserve durable version",
        })
    );
    assert_eq!(
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::Replay,
            CommitPhase::Replay,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0),
            CommitVisibilityFacts::new(
                Some(stamp.commit_version()),
                None,
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
            )
            .expect("cache replay facts"),
            None,
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason: "durable outcome must preserve durable version",
        })
    );
}

fn stamp(branch: BranchId, version: u64) -> CommitStamp {
    CommitStamp::new(
        branch,
        CommitVersion::new(version),
        Timestamp::from_micros(version * 10),
    )
    .expect("commit stamp")
}

fn read_only_batch(branch: BranchId, options: CommitBatchOptions) -> ValidatedCommitBatch {
    CommitBatch::read_only_diagnostic(branch, CommitValidationFacts::empty(), options)
        .validate(&CommitRuntimeConfig::default())
        .expect("valid read-only batch")
}

fn mutation_counts(puts: usize, deletes: usize, timeline_rows: usize) -> CommitMutationCounts {
    CommitMutationCounts::new(
        puts,
        deletes,
        timeline_rows,
        &CommitRuntimeConfig::default(),
    )
    .expect("valid mutation counts")
}

fn assert_bounded_storage_debug(debug: &str) {
    for forbidden in [
        "Transaction",
        "VersionedValue",
        "Value {",
        "payload",
        concat!("w", "al"),
        "backend",
        "Engine",
    ] {
        assert!(
            !debug.contains(forbidden),
            "debug output leaked forbidden vocabulary {forbidden}: {debug}"
        );
    }
}
