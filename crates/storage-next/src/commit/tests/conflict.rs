use super::*;
use crate::branch::state::BranchLocalState;
use crate::row::StorageRow;
use std::cell::{Cell, RefCell};

#[test]
fn branch_read_view_conflict_source_maps_visible_missing_and_tombstone_rows() {
    let branch = branch_id(121);
    let present = physical_key(branch, 0x20, b"present".to_vec());
    let missing = physical_key(branch, 0x20, b"missing".to_vec());
    let deleted = physical_key(branch, 0x20, b"deleted".to_vec());
    let view = read_view(
        branch,
        vec![
            put_row(present.clone(), 7),
            put_row(deleted.clone(), 3),
            tombstone_row(deleted.clone(), 8),
        ],
    );
    let source = CommitBranchReadViewConflictSource::new(&view);

    assert_eq!(
        source.current_observed_version(&present),
        Ok(CommitObservedVersion::Present(CommitVersion::new(7)))
    );
    assert_eq!(
        source.current_observed_version(&missing),
        Ok(CommitObservedVersion::Missing)
    );
    assert_eq!(
        source.current_observed_version(&deleted),
        Ok(CommitObservedVersion::Missing)
    );
}

#[test]
fn branch_read_view_conflict_source_can_be_capped_at_visible_version() {
    let branch = branch_id(221);
    let key = physical_key(branch, 0x20, b"visible-bound".to_vec());
    let view = read_view(branch, vec![put_row(key.clone(), 7)]);
    let hidden_source =
        CommitBranchReadViewConflictSource::new_at_version(&view, CommitVersion::new(6));
    let visible_source =
        CommitBranchReadViewConflictSource::new_at_version(&view, CommitVersion::new(7));

    assert_eq!(
        hidden_source.current_observed_version(&key),
        Ok(CommitObservedVersion::Missing)
    );
    assert_eq!(
        visible_source.current_observed_version(&key),
        Ok(CommitObservedVersion::Present(CommitVersion::new(7)))
    );
}

#[test]
fn read_set_present_facts_match_or_report_storage_conflict() {
    let branch = branch_id(122);
    let key = physical_key(branch, 0x20, b"read-present".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(9)),
    )]);
    let matching = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key.clone(),
                CommitObservedVersion::Present(CommitVersion::new(9)),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&matching, &source).expect("read set match");
    assert_eq!(report.checked_read_facts(), 1);
    assert_eq!(report.checked_cas_facts(), 0);
    assert!(!report.skipped_validation());

    let stale = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key.clone(),
                CommitObservedVersion::Present(CommitVersion::new(8)),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );
    assert_eq!(
        validate_commit_conflicts(&stale, &source).expect_err("stale read set rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::ReadSet,
                &key,
                CommitObservedVersion::Present(CommitVersion::new(8)),
                CommitObservedVersion::Present(CommitVersion::new(9)),
            ),
        }
    );
}

#[test]
fn read_set_missing_facts_match_missing_and_reject_newly_present_rows() {
    let branch = branch_id(123);
    let missing = physical_key(branch, 0x20, b"still-missing".to_vec());
    let present = physical_key(branch, 0x20, b"now-present".to_vec());
    let source = FakeConflictSource::new(vec![(
        present.clone(),
        CommitObservedVersion::Present(CommitVersion::new(12)),
    )]);
    let still_missing = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                missing.clone(),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&still_missing, &source).expect("missing stays missing");
    assert_eq!(report.checked_read_facts(), 1);

    let became_present = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                present.clone(),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );
    assert_eq!(
        validate_commit_conflicts(&became_present, &source)
            .expect_err("missing-to-present rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::ReadSet,
                &present,
                CommitObservedVersion::Missing,
                CommitObservedVersion::Present(CommitVersion::new(12)),
            ),
        }
    );
}

#[test]
fn read_set_facts_validate_against_tombstone_hidden_rows() {
    let branch = branch_id(134);
    let deleted = physical_key(branch, 0x20, b"read-deleted".to_vec());
    let view = read_view(
        branch,
        vec![
            put_row(deleted.clone(), 11),
            tombstone_row(deleted.clone(), 12),
        ],
    );
    let source = CommitBranchReadViewConflictSource::new(&view);

    let missing_still_missing = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                deleted.clone(),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );
    let report = validate_commit_conflicts(&missing_still_missing, &source)
        .expect("tombstone-hidden row is missing");
    assert_eq!(report.checked_read_facts(), 1);

    let present_became_missing = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                deleted.clone(),
                CommitObservedVersion::Present(CommitVersion::new(11)),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );
    assert_eq!(
        validate_commit_conflicts(&present_became_missing, &source)
            .expect_err("present-to-tombstone rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::ReadSet,
                &deleted,
                CommitObservedVersion::Present(CommitVersion::new(11)),
                CommitObservedVersion::Missing,
            ),
        }
    );
}

#[test]
fn cas_present_and_missing_facts_validate_against_current_versions() {
    let branch = branch_id(124);
    let present = physical_key(branch, 0x20, b"cas-present".to_vec());
    let missing = physical_key(branch, 0x20, b"cas-missing".to_vec());
    let source = FakeConflictSource::new(vec![(
        present.clone(),
        CommitObservedVersion::Present(CommitVersion::new(14)),
    )]);
    let matching = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![
                CommitCasFact::new(
                    present.clone(),
                    CommitObservedVersion::Present(CommitVersion::new(14)),
                ),
                CommitCasFact::new(missing.clone(), CommitObservedVersion::Missing),
            ],
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&matching, &source).expect("cas facts match");
    assert_eq!(report.checked_read_facts(), 0);
    assert_eq!(report.checked_cas_facts(), 2);

    let stale = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                present.clone(),
                CommitObservedVersion::Present(CommitVersion::new(13)),
            )],
        ),
        CommitConflictValidationMode::Validate,
    );
    assert_eq!(
        validate_commit_conflicts(&stale, &source).expect_err("stale cas rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::Cas,
                &present,
                CommitObservedVersion::Present(CommitVersion::new(13)),
                CommitObservedVersion::Present(CommitVersion::new(14)),
            ),
        }
    );
}

#[test]
fn cas_facts_validate_against_tombstone_hidden_rows() {
    let branch = branch_id(135);
    let deleted = physical_key(branch, 0x20, b"cas-deleted".to_vec());
    let view = read_view(
        branch,
        vec![
            put_row(deleted.clone(), 15),
            tombstone_row(deleted.clone(), 16),
        ],
    );
    let source = CommitBranchReadViewConflictSource::new(&view);

    let missing_still_missing = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                deleted.clone(),
                CommitObservedVersion::Missing,
            )],
        ),
        CommitConflictValidationMode::Validate,
    );
    let report = validate_commit_conflicts(&missing_still_missing, &source)
        .expect("tombstone-hidden row is missing for cas");
    assert_eq!(report.checked_cas_facts(), 1);

    let present_became_missing = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                deleted.clone(),
                CommitObservedVersion::Present(CommitVersion::new(15)),
            )],
        ),
        CommitConflictValidationMode::Validate,
    );
    assert_eq!(
        validate_commit_conflicts(&present_became_missing, &source)
            .expect_err("cas present-to-tombstone rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::Cas,
                &deleted,
                CommitObservedVersion::Present(CommitVersion::new(15)),
                CommitObservedVersion::Missing,
            ),
        }
    );
}

#[test]
fn read_set_conflict_stops_before_cas_facts_are_checked() {
    let branch = branch_id(125);
    let read_key = physical_key(branch, 0x20, b"read-first".to_vec());
    let cas_key = physical_key(branch, 0x20, b"cas-second".to_vec());
    let source = FakeConflictSource::new(vec![
        (
            read_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(2)),
        ),
        (
            cas_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(7)),
        ),
    ]);
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key.clone(),
                CommitObservedVersion::Present(CommitVersion::new(1)),
            )],
            vec![CommitCasFact::new(
                cas_key,
                CommitObservedVersion::Present(CommitVersion::new(7)),
            )],
        ),
        CommitConflictValidationMode::Validate,
    );

    assert!(matches!(
        validate_commit_conflicts(&batch, &source),
        Err(CommitRuntimeError::CommitConflict { conflict })
            if conflict.kind() == CommitConflictKind::ReadSet
    ));
    assert_eq!(source.read_count(), 1);
}

#[test]
fn multiple_read_facts_are_checked_in_input_order_and_stop_at_first_conflict() {
    let branch = branch_id(136);
    let first = physical_key(branch, 0x20, b"first-read".to_vec());
    let second = physical_key(branch, 0x20, b"second-read".to_vec());
    let third = physical_key(branch, 0x20, b"third-read".to_vec());
    let source = OrderedConflictSource::new(vec![
        (
            first.clone(),
            CommitObservedVersion::Present(CommitVersion::new(1)),
        ),
        (
            second.clone(),
            CommitObservedVersion::Present(CommitVersion::new(22)),
        ),
        (
            third.clone(),
            CommitObservedVersion::Present(CommitVersion::new(3)),
        ),
    ]);
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![
                CommitReadFact::new(
                    first.clone(),
                    CommitObservedVersion::Present(CommitVersion::new(1)),
                ),
                CommitReadFact::new(
                    second.clone(),
                    CommitObservedVersion::Present(CommitVersion::new(2)),
                ),
                CommitReadFact::new(third, CommitObservedVersion::Present(CommitVersion::new(3))),
            ],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );

    assert_eq!(
        validate_commit_conflicts(&batch, &source).expect_err("second read fact rejects"),
        CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(
                CommitConflictKind::ReadSet,
                &second,
                CommitObservedVersion::Present(CommitVersion::new(2)),
                CommitObservedVersion::Present(CommitVersion::new(22)),
            ),
        }
    );
    assert_eq!(
        source.read_user_keys(),
        vec![b"first-read".to_vec(), b"second-read".to_vec()]
    );
}

#[test]
fn passing_read_set_and_cas_facts_report_both_checked_counts() {
    let branch = branch_id(137);
    let read_key = physical_key(branch, 0x20, b"read-pass".to_vec());
    let cas_key = physical_key(branch, 0x20, b"cas-pass".to_vec());
    let source = FakeConflictSource::new(vec![
        (
            read_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(31)),
        ),
        (
            cas_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(32)),
        ),
    ]);
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key,
                CommitObservedVersion::Present(CommitVersion::new(31)),
            )],
            vec![CommitCasFact::new(
                cas_key,
                CommitObservedVersion::Present(CommitVersion::new(32)),
            )],
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&batch, &source).expect("all facts pass");

    assert_eq!(report.checked_read_facts(), 1);
    assert_eq!(report.checked_cas_facts(), 1);
    assert!(!report.skipped_validation());
    assert_eq!(source.read_count(), 2);
}

#[cfg(feature = "perf-trace")]
#[test]
fn conflict_perf_trace_counts_passing_read_and_cas_facts_once() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(233);
    let read_key = physical_key(branch, 0x20, b"read-pass-perf".to_vec());
    let cas_key = physical_key(branch, 0x20, b"cas-pass-perf".to_vec());
    let source = FakeConflictSource::new(vec![
        (
            read_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(31)),
        ),
        (
            cas_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(32)),
        ),
    ]);
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key,
                CommitObservedVersion::Present(CommitVersion::new(31)),
            )],
            vec![CommitCasFact::new(
                cas_key,
                CommitObservedVersion::Present(CommitVersion::new(32)),
            )],
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&batch, &source).expect("all facts pass");

    assert_eq!(report.checked_read_facts(), 1);
    assert_eq!(report.checked_cas_facts(), 1);
    assert_eq!(source.read_count(), 2);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_conflict_validation_calls(), 1);
    assert_eq!(perf.commit_conflict_validation_skipped(), 0);
    assert_eq!(perf.commit_conflict_validation_without_source(), 0);
    assert_eq!(perf.commit_conflict_validation_with_source(), 1);
    assert_eq!(perf.commit_conflict_read_facts_checked(), 1);
    assert_eq!(perf.commit_conflict_cas_facts_checked(), 1);
    assert_eq!(perf.commit_conflicts_detected(), 0);
}

#[test]
fn skip_mode_and_blind_writes_do_not_read_conflict_source() {
    let branch = branch_id(126);
    let key = physical_key(branch, 0x20, b"skip".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(21)),
    )]);
    let skip = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key.clone(),
                CommitObservedVersion::Missing,
            )],
            vec![CommitCasFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(1)),
            )],
        ),
        CommitConflictValidationMode::Skip,
    );

    let report = validate_commit_conflicts(&skip, &source).expect("skip passes");
    assert!(report.skipped_validation());
    assert_eq!(source.read_count(), 0);

    let blind_put = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::empty(),
        CommitConflictValidationMode::Validate,
    );
    let report = validate_commit_conflicts(&blind_put, &source).expect("blind write passes");
    assert!(!report.skipped_validation());
    assert_eq!(report.checked_read_facts(), 0);
    assert_eq!(report.checked_cas_facts(), 0);
    assert_eq!(source.read_count(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn conflict_perf_trace_counts_skip_and_empty_validation_without_source() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(234);
    let key = physical_key(branch, 0x20, b"skip-empty-perf".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(21)),
    )]);
    let skip = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key.clone(),
                CommitObservedVersion::Missing,
            )],
            vec![CommitCasFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(1)),
            )],
        ),
        CommitConflictValidationMode::Skip,
    );
    let validate_empty = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::empty(),
        CommitConflictValidationMode::Validate,
    );

    assert!(validate_commit_conflicts(&skip, &source)
        .expect("skip passes")
        .skipped_validation());
    assert!(!validate_commit_conflicts(&validate_empty, &source)
        .expect("empty validate passes")
        .skipped_validation());

    assert_eq!(source.read_count(), 0);
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_conflict_validation_calls(), 2);
    assert_eq!(perf.commit_conflict_validation_skipped(), 1);
    assert_eq!(perf.commit_conflict_validation_without_source(), 1);
    assert_eq!(perf.commit_conflict_validation_with_source(), 0);
    assert_eq!(perf.commit_conflict_read_facts_checked(), 0);
    assert_eq!(perf.commit_conflict_cas_facts_checked(), 0);
    assert_eq!(perf.commit_conflicts_detected(), 0);
}

#[test]
fn blind_put_and_delete_over_changed_or_missing_keys_do_not_read_conflict_source() {
    let branch = branch_id(138);
    let changed = physical_key(branch, 0x20, b"changed-blind".to_vec());
    let missing = physical_key(branch, 0x20, b"missing-blind".to_vec());
    let source = FakeConflictSource::new(vec![(
        changed.clone(),
        CommitObservedVersion::Present(CommitVersion::new(41)),
    )]);

    for batch in [
        mutating_batch_for_mutation(
            branch,
            CommitMutation::put(
                changed.clone(),
                b"new-value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitValidationFacts::empty(),
            CommitConflictValidationMode::Validate,
        ),
        mutating_batch_for_mutation(
            branch,
            CommitMutation::delete(changed),
            CommitValidationFacts::empty(),
            CommitConflictValidationMode::Validate,
        ),
        mutating_batch_for_mutation(
            branch,
            CommitMutation::put(
                missing.clone(),
                b"new-value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitValidationFacts::empty(),
            CommitConflictValidationMode::Validate,
        ),
        mutating_batch_for_mutation(
            branch,
            CommitMutation::delete(missing),
            CommitValidationFacts::empty(),
            CommitConflictValidationMode::Validate,
        ),
    ] {
        let report = validate_commit_conflicts(&batch, &source).expect("blind mutation passes");
        assert_eq!(report.checked_read_facts(), 0);
        assert_eq!(report.checked_cas_facts(), 0);
        assert!(!report.skipped_validation());
    }
    assert_eq!(source.read_count(), 0);
}

#[test]
fn validation_facts_are_scoped_by_branch_even_for_identical_space_and_user_key() {
    let branch = branch_id(140);
    let other = branch_id(141);
    let target_key = physical_key(branch, 0x20, b"same-user-key".to_vec());
    let other_branch_key = physical_key(other, 0x20, b"same-user-key".to_vec());
    let source = FakeConflictSource::new(vec![(
        other_branch_key,
        CommitObservedVersion::Present(CommitVersion::new(71)),
    )]);
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                target_key,
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );

    let report = validate_commit_conflicts(&batch, &source)
        .expect("other branch state must not affect target branch");

    assert_eq!(report.checked_read_facts(), 1);
    assert_eq!(source.read_count(), 1);
}

#[test]
fn skip_report_is_distinguishable_from_validate_with_empty_facts() {
    let branch = branch_id(139);
    let source = FakeConflictSource::new(Vec::new());
    let skip = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::empty(),
        CommitConflictValidationMode::Skip,
    );
    let validate_empty = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::empty(),
        CommitConflictValidationMode::Validate,
    );

    let skip_report = validate_commit_conflicts(&skip, &source).expect("skip passes");
    let validate_report =
        validate_commit_conflicts(&validate_empty, &source).expect("empty validate passes");

    assert!(skip_report.skipped_validation());
    assert!(!validate_report.skipped_validation());
    assert_eq!(skip_report.checked_read_facts(), 0);
    assert_eq!(validate_report.checked_read_facts(), 0);
    assert_eq!(source.read_count(), 0);
}

#[test]
fn read_only_batches_skip_conflict_validation_without_source_reads() {
    let branch = branch_id(131);
    let key = physical_key(branch, 0x20, b"read-only-conflict".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(41)),
    )]);
    let batch = CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid read-only batch");

    let report = validate_commit_conflicts(&batch, &source).expect("read-only skips");

    assert!(report.skipped_validation());
    assert_eq!(source.read_count(), 0);
}

#[test]
fn lower_layer_read_errors_preserve_source_chain() {
    let branch = branch_id(127);
    let key = physical_key(branch, 0x20, b"fail".to_vec());
    let source = FailingConflictSource::new();
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
    );

    let error = validate_commit_conflicts(&batch, &source).expect_err("source failure");

    assert_eq!(
        error,
        CommitRuntimeError::lower_layer(CommitLowerLayer::BranchRuntime, "test read failure")
    );
    assert!(error.source().is_some());
    assert_eq!(
        error.source().expect("source").to_string(),
        "injected conflict source failure"
    );
}

#[test]
fn lower_layer_failure_after_successful_read_set_check_preserves_source_chain() {
    let branch = branch_id(132);
    let read_key = physical_key(branch, 0x20, b"read-ok".to_vec());
    let cas_key = physical_key(branch, 0x20, b"cas-fails".to_vec());
    let source = PartiallyFailingConflictSource::new(read_key.clone());
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key,
                CommitObservedVersion::Present(CommitVersion::new(51)),
            )],
            vec![CommitCasFact::new(cas_key, CommitObservedVersion::Missing)],
        ),
        CommitConflictValidationMode::Validate,
    );

    let error = validate_commit_conflicts(&batch, &source).expect_err("cas read failure");

    assert_eq!(source.read_count(), 2);
    assert_eq!(
        error,
        CommitRuntimeError::lower_layer(
            CommitLowerLayer::BranchRuntime,
            "partial test read failure"
        )
    );
    assert_eq!(
        error.source().expect("source").to_string(),
        "partial conflict source failure"
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn conflict_perf_trace_counts_attempted_facts_when_source_fails() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let branch = branch_id(232);
    let read_key = physical_key(branch, 0x20, b"read-ok".to_vec());
    let cas_key = physical_key(branch, 0x20, b"cas-source-fails".to_vec());
    let source = PartiallyFailingConflictSource::new(read_key.clone());
    let batch = mutating_batch_with_validation(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key,
                CommitObservedVersion::Present(CommitVersion::new(51)),
            )],
            vec![CommitCasFact::new(cas_key, CommitObservedVersion::Missing)],
        ),
        CommitConflictValidationMode::Validate,
    );

    let error = validate_commit_conflicts(&batch, &source).expect_err("cas read failure");

    assert_eq!(
        error,
        CommitRuntimeError::lower_layer(
            CommitLowerLayer::BranchRuntime,
            "partial test read failure"
        )
    );
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_conflict_validation_calls(), 1);
    assert_eq!(perf.commit_conflict_validation_with_source(), 1);
    assert_eq!(perf.commit_conflict_read_facts_checked(), 1);
    assert_eq!(perf.commit_conflict_cas_facts_checked(), 1);
    assert_eq!(perf.commit_conflicts_detected(), 0);
}

#[test]
fn branch_read_view_source_maps_wrong_branch_to_lower_layer_error() {
    let branch = branch_id(128);
    let other = branch_id(129);
    let key = physical_key(other, 0x20, b"wrong-branch".to_vec());
    let view = read_view(branch, Vec::new());
    let source = CommitBranchReadViewConflictSource::new(&view);

    let error = source
        .current_observed_version(&key)
        .expect_err("wrong branch should fail through the branch layer");

    assert_eq!(
        error,
        CommitRuntimeError::lower_layer(
            CommitLowerLayer::BranchRuntime,
            "conflict read view lookup failed"
        )
    );
    assert!(error.source().is_some());
}

#[test]
fn conflict_error_display_is_bounded_and_storage_shaped() {
    let branch = branch_id(130);
    let key = physical_key(branch, 0x20, b"display".to_vec());
    let cases = [
        (
            CommitConflictKind::ReadSet,
            "commit ReadSet conflict",
            CommitObservedVersion::Present(CommitVersion::new(30)),
            CommitObservedVersion::Missing,
        ),
        (
            CommitConflictKind::Cas,
            "commit Cas conflict",
            CommitObservedVersion::Missing,
            CommitObservedVersion::Present(CommitVersion::new(31)),
        ),
    ];

    for (kind, label, expected, actual) in cases {
        let error = CommitRuntimeError::CommitConflict {
            conflict: CommitConflict::new(kind, &key, expected, actual),
        };
        let display = error.to_string();

        assert_bounded_storage_debug(&display);
        assert!(display.contains(label));
        assert!(display.contains("storage space 0x20"));
        assert!(display.contains("key fingerprint 0x"));
        assert!(display.contains(&format!("expected {}", observed_label(expected))));
        assert!(display.contains(&format!("actual {}", observed_label(actual))));
        assert!(!display.contains("display"));
    }
}

#[test]
fn conflict_diagnostics_distinguish_same_length_keys_without_leaking_bytes() {
    let branch = branch_id(133);
    let left = physical_key(branch, 0x20, b"aa".to_vec());
    let right = physical_key(branch, 0x20, b"bb".to_vec());
    let left_conflict = CommitConflict::new(
        CommitConflictKind::ReadSet,
        &left,
        CommitObservedVersion::Missing,
        CommitObservedVersion::Present(CommitVersion::new(61)),
    );
    let right_conflict = CommitConflict::new(
        CommitConflictKind::ReadSet,
        &right,
        CommitObservedVersion::Missing,
        CommitObservedVersion::Present(CommitVersion::new(61)),
    );

    assert_eq!(left_conflict.user_key_len(), right_conflict.user_key_len());
    assert_ne!(
        left_conflict.key_fingerprint(),
        right_conflict.key_fingerprint()
    );
    assert_ne!(left_conflict, right_conflict);

    let display = CommitRuntimeError::CommitConflict {
        conflict: left_conflict,
    }
    .to_string();
    assert!(display.contains("key fingerprint 0x"));
    assert!(!display.contains("aa"));
    assert!(!display.contains("bb"));
}

fn read_view(branch: BranchId, rows: Vec<StorageRow>) -> crate::branch::read::BranchReadView {
    let mut state = BranchLocalState::empty(branch);
    for row in rows {
        state.append_committed_row(row).expect("append row");
    }
    state.capture_read_view().expect("capture read view")
}

fn put_row(key: PhysicalKey, version: u64) -> StorageRow {
    StorageRow::put(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version),
        Timestamp::EPOCH,
        vec![1],
    )
}

fn tombstone_row(key: PhysicalKey, version: u64) -> StorageRow {
    StorageRow::tombstone(
        key,
        CommitVersion::new(version),
        Timestamp::from_micros(version),
    )
}

fn observed_label(observed: CommitObservedVersion) -> String {
    match observed {
        CommitObservedVersion::Missing => "missing".to_string(),
        CommitObservedVersion::Present(version) => format!("present({version})"),
    }
}

fn mutating_batch_with_validation(
    branch: BranchId,
    validation: CommitValidationFacts,
    mode: CommitConflictValidationMode,
) -> ValidatedCommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, 0x21, b"mutation".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        validation,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            mode,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch")
}

fn mutating_batch_for_mutation(
    branch: BranchId,
    mutation: CommitMutation,
    validation: CommitValidationFacts,
    mode: CommitConflictValidationMode,
) -> ValidatedCommitBatch {
    CommitBatch::mutating(
        branch,
        vec![mutation],
        validation,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            mode,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch")
}

struct FakeConflictSource {
    versions: Vec<(PhysicalKey, CommitObservedVersion)>,
    read_count: Cell<usize>,
}

impl FakeConflictSource {
    fn new(versions: Vec<(PhysicalKey, CommitObservedVersion)>) -> Self {
        Self {
            versions,
            read_count: Cell::new(0),
        }
    }

    fn read_count(&self) -> usize {
        self.read_count.get()
    }
}

impl CommitConflictReadSource for FakeConflictSource {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        self.read_count.set(self.read_count.get() + 1);
        Ok(self
            .versions
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map_or(CommitObservedVersion::Missing, |(_, observed)| *observed))
    }
}

struct OrderedConflictSource {
    versions: Vec<(PhysicalKey, CommitObservedVersion)>,
    read_user_keys: RefCell<Vec<Vec<u8>>>,
}

impl OrderedConflictSource {
    fn new(versions: Vec<(PhysicalKey, CommitObservedVersion)>) -> Self {
        Self {
            versions,
            read_user_keys: RefCell::new(Vec::new()),
        }
    }

    fn read_user_keys(&self) -> Vec<Vec<u8>> {
        self.read_user_keys.borrow().clone()
    }
}

impl CommitConflictReadSource for OrderedConflictSource {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        self.read_user_keys
            .borrow_mut()
            .push(key.user_key().to_vec());
        Ok(self
            .versions
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map_or(CommitObservedVersion::Missing, |(_, observed)| *observed))
    }
}

struct FailingConflictSource;

impl FailingConflictSource {
    const fn new() -> Self {
        Self
    }
}

impl CommitConflictReadSource for FailingConflictSource {
    fn current_observed_version(
        &self,
        _key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        Err(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::BranchRuntime,
            "test read failure",
            InjectedConflictSourceFailure,
        ))
    }
}

#[derive(Debug)]
struct InjectedConflictSourceFailure;

impl fmt::Display for InjectedConflictSourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("injected conflict source failure")
    }
}

impl Error for InjectedConflictSourceFailure {}

struct PartiallyFailingConflictSource {
    matching_key: PhysicalKey,
    read_count: Cell<usize>,
}

impl PartiallyFailingConflictSource {
    fn new(matching_key: PhysicalKey) -> Self {
        Self {
            matching_key,
            read_count: Cell::new(0),
        }
    }

    fn read_count(&self) -> usize {
        self.read_count.get()
    }
}

impl CommitConflictReadSource for PartiallyFailingConflictSource {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        self.read_count.set(self.read_count.get() + 1);
        if key == &self.matching_key {
            Ok(CommitObservedVersion::Present(CommitVersion::new(51)))
        } else {
            Err(CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "partial test read failure",
                PartialConflictSourceFailure,
            ))
        }
    }
}

#[derive(Debug)]
struct PartialConflictSourceFailure;

impl fmt::Display for PartialConflictSourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("partial conflict source failure")
    }
}

impl Error for PartialConflictSourceFailure {}
