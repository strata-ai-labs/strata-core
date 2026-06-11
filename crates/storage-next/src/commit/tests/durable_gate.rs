use super::*;

#[test]
fn unresolved_durable_fact_validates_durable_not_applied_phase() {
    let branch = branch_id(80);
    let fact = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch, 3),
        CommitDurabilityClass::Standard,
        "branch apply failed after WAL append",
    )
    .expect("durable-not-applied fact");

    assert_eq!(fact.branch_id(), branch);
    assert_eq!(fact.commit_version(), CommitVersion::new(3));
    assert_eq!(fact.commit_timestamp(), Timestamp::from_micros(3_000));
    assert_eq!(fact.durability(), CommitDurabilityClass::Standard);
    assert_eq!(fact.kind(), CommitUnresolvedDurableKind::DurableNotApplied);
    assert_eq!(
        fact.visibility_facts(),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(3)),
            Some(CommitVersion::new(3)),
            None,
            None,
            None,
        )
        .expect("facts")
    );
    assert_eq!(fact.reason(), "branch apply failed after WAL append");
}

#[test]
fn unresolved_durable_fact_validates_applied_not_visible_phase() {
    let branch = branch_id(81);
    let fact = CommitUnresolvedDurable::applied_not_visible(
        stamp(branch, 4),
        CommitDurabilityClass::Always,
        "visible publish failed",
    )
    .expect("applied-not-visible fact");

    assert_eq!(fact.branch_id(), branch);
    assert_eq!(fact.commit_version(), CommitVersion::new(4));
    assert_eq!(fact.commit_timestamp(), Timestamp::from_micros(4_000));
    assert_eq!(fact.durability(), CommitDurabilityClass::Always);
    assert_eq!(fact.kind(), CommitUnresolvedDurableKind::AppliedNotVisible);
    assert_eq!(
        fact.visibility_facts(),
        CommitVisibilityFacts::new(
            Some(CommitVersion::new(4)),
            Some(CommitVersion::new(4)),
            Some(CommitVersion::new(4)),
            None,
            Some(CommitVersion::new(4)),
        )
        .expect("facts")
    );
    assert_eq!(fact.reason(), "visible publish failed");
}

#[test]
fn unresolved_durable_fact_rejects_non_durable_durable_not_applied_state() {
    for durability in [
        CommitDurabilityClass::NotDurable,
        CommitDurabilityClass::Uncertain,
    ] {
        assert_eq!(
            CommitUnresolvedDurable::durable_not_applied_with_facts(
                stamp(branch_id(82), 5),
                durability,
                "invalid durability",
            ),
            Err(CommitRuntimeError::InvalidCommitState {
                reason: "durable-not-applied unresolved commit must claim durable WAL success",
            })
        );
    }

    let cache_applied_not_visible = CommitUnresolvedDurable::applied_not_visible(
        stamp(branch_id(82), 5),
        CommitDurabilityClass::NotDurable,
        "cache visible publish failed",
    )
    .expect("not-durable applied-not-visible state");
    assert_eq!(
        cache_applied_not_visible.durability(),
        CommitDurabilityClass::NotDurable
    );

    assert_eq!(
        CommitUnresolvedDurable::applied_not_visible(
            stamp(branch_id(82), 6),
            CommitDurabilityClass::Uncertain,
            "uncertain visible publish failed",
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "applied-not-visible unresolved commit cannot have uncertain durability",
        })
    );
}

#[test]
fn unresolved_durable_fact_rejects_inconsistent_visibility_facts() {
    assert_eq!(
        CommitUnresolvedDurable::new(
            stamp(branch_id(83), 6),
            CommitDurabilityClass::Standard,
            CommitUnresolvedDurableKind::DurableNotApplied,
            CommitVisibilityFacts::new(
                Some(CommitVersion::new(6)),
                Some(CommitVersion::new(6)),
                Some(CommitVersion::new(6)),
                None,
                None,
            )
            .expect("visibility facts"),
            "invalid phase facts",
        ),
        Err(CommitRuntimeError::InvalidVisibilityFacts {
            reason:
                "durable-not-applied unresolved commit must not claim applied or visible progress",
        })
    );
}

#[test]
fn unresolved_durable_fact_rejects_every_invalid_phase_shape() {
    let branch = branch_id(87);
    let version = CommitVersion::new(10);
    let cases = [
        (
            CommitUnresolvedDurableKind::DurableNotApplied,
            CommitVisibilityFacts::from_parts_unchecked(
                Some(version),
                Some(version),
                None,
                None,
                Some(version),
            ),
            "timeline version must not exceed applied version",
        ),
        (
            CommitUnresolvedDurableKind::DurableNotApplied,
            CommitVisibilityFacts::from_parts_unchecked(
                Some(version),
                Some(version),
                None,
                Some(version),
                None,
            ),
            "visible version must not exceed applied version",
        ),
        (
            CommitUnresolvedDurableKind::AppliedNotVisible,
            CommitVisibilityFacts::from_parts_unchecked(
                Some(version),
                Some(version),
                None,
                None,
                None,
            ),
            "applied-not-visible unresolved commit must preserve applied version",
        ),
        (
            CommitUnresolvedDurableKind::AppliedNotVisible,
            CommitVisibilityFacts::from_parts_unchecked(
                Some(version),
                Some(version),
                Some(version),
                None,
                None,
            ),
            "applied-not-visible unresolved commit must preserve timeline version",
        ),
        (
            CommitUnresolvedDurableKind::AppliedNotVisible,
            CommitVisibilityFacts::from_parts_unchecked(
                Some(version),
                Some(version),
                Some(version),
                Some(version),
                Some(version),
            ),
            "applied-not-visible unresolved commit must not claim visibility",
        ),
    ];

    for (kind, facts, expected_reason) in cases {
        let error = CommitUnresolvedDurable::new(
            CommitStamp::new(branch, version, Timestamp::from_micros(10_000)).expect("stamp"),
            CommitDurabilityClass::Standard,
            kind,
            facts,
            "invalid phase facts",
        )
        .expect_err("invalid phase shape");
        assert_eq!(
            error,
            CommitRuntimeError::InvalidVisibilityFacts {
                reason: expected_reason,
            }
        );
    }
}

#[test]
fn unresolved_durable_fact_rejects_zero_commit_version_through_stamp() {
    assert_eq!(
        CommitStamp::new(
            branch_id(88),
            CommitVersion::ZERO,
            Timestamp::from_micros(1)
        ),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "commit version must be nonzero",
        })
    );
}

#[test]
fn unresolved_durable_debug_is_bounded_and_does_not_store_value_bytes() {
    let fact = CommitUnresolvedDurable::applied_not_visible(
        stamp(branch_id(89), 12),
        CommitDurabilityClass::Always,
        "visible publish failed",
    )
    .expect("fact");
    let gate = CommitUnresolvedDurableGate::new();
    gate.record_unresolved(fact).expect("record fact");

    let debug = format!("{:?} {:?}", fact, gate.unresolved().expect("gate"));
    assert!(debug.contains("branch_id"));
    assert!(debug.contains("commit_version"));
    assert!(debug.contains("AppliedNotVisible"));
    assert!(!debug.contains("super-secret-value"));
    assert!(!debug.contains("VersionedValue"));
    assert!(!debug.contains("Transaction"));
}

#[test]
fn unresolved_durable_gate_records_idempotently_and_blocks_mutation() {
    let gate = CommitUnresolvedDurableGate::new();
    let fact = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(84), 7),
        CommitDurabilityClass::Standard,
        "apply failed",
    )
    .expect("fact");

    assert_eq!(gate.unresolved().expect("gate read"), None);
    assert!(gate.require_open_for_mutation().is_ok());
    gate.record_unresolved(fact).expect("record fact");
    gate.record_unresolved(fact).expect("record fact again");
    assert_eq!(gate.unresolved().expect("gate read"), Some(fact));
    assert_eq!(
        gate.require_open_for_mutation(),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: fact.branch_id(),
            commit_version: fact.commit_version(),
            reason: "durable commit must be replayed or reconciled first",
        })
    );
}

#[test]
fn unresolved_durable_gate_availability_probe_rejects_without_admitting() {
    let gate = CommitUnresolvedDurableGate::new();
    assert_eq!(gate.require_admission_available(), Ok(()));
    let active = gate.admit_mutating_commit().expect("active admission");
    assert_eq!(
        gate.require_admission_available(),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable commit admission is already active",
        })
    );
    drop(active);

    let fact = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(85), 8),
        CommitDurabilityClass::Standard,
        "apply failed",
    )
    .expect("fact");
    gate.record_unresolved(fact).expect("record fact");
    assert_eq!(
        gate.require_admission_available(),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: fact.branch_id(),
            commit_version: fact.commit_version(),
            reason: "durable commit must be replayed or reconciled first",
        })
    );
}

#[cfg(feature = "perf-trace")]
#[test]
fn unresolved_durable_gate_availability_probe_counts_only_rejections() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let gate = CommitUnresolvedDurableGate::new();

    gate.require_admission_available()
        .expect("clean availability probe");
    let clean = crate::observability::perf_trace::snapshot();
    assert_eq!(clean.commit_unresolved_gate_admission_attempts(), 0);
    assert_eq!(clean.commit_unresolved_gate_admission_acquired(), 0);
    assert_eq!(clean.commit_unresolved_gate_rejected_active(), 0);
    assert_eq!(clean.commit_unresolved_gate_rejected_unresolved(), 0);

    let active = gate.admit_mutating_commit().expect("active admission");
    crate::observability::perf_trace::reset();
    assert!(matches!(
        gate.require_admission_available(),
        Err(CommitRuntimeError::InvalidCommitState { .. })
    ));
    let active_reject = crate::observability::perf_trace::snapshot();
    assert_eq!(active_reject.commit_unresolved_gate_admission_attempts(), 1);
    assert_eq!(active_reject.commit_unresolved_gate_admission_acquired(), 0);
    assert_eq!(active_reject.commit_unresolved_gate_rejected_active(), 1);
    assert_eq!(
        active_reject.commit_unresolved_gate_rejected_unresolved(),
        0
    );
    drop(active);

    let fact = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(86), 9),
        CommitDurabilityClass::Standard,
        "apply failed",
    )
    .expect("fact");
    gate.record_unresolved(fact).expect("record fact");
    crate::observability::perf_trace::reset();
    assert!(matches!(
        gate.require_admission_available(),
        Err(CommitRuntimeError::UnresolvedDurableCommit { .. })
    ));
    let unresolved_reject = crate::observability::perf_trace::snapshot();
    assert_eq!(
        unresolved_reject.commit_unresolved_gate_admission_attempts(),
        1
    );
    assert_eq!(
        unresolved_reject.commit_unresolved_gate_admission_acquired(),
        0
    );
    assert_eq!(
        unresolved_reject.commit_unresolved_gate_rejected_active(),
        0
    );
    assert_eq!(
        unresolved_reject.commit_unresolved_gate_rejected_unresolved(),
        1
    );
}

#[test]
fn unresolved_durable_gate_instances_are_not_process_global() {
    let first = CommitUnresolvedDurableGate::new();
    let second = CommitUnresolvedDurableGate::new();
    let fact = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(90), 13),
        CommitDurabilityClass::Standard,
        "apply failed",
    )
    .expect("fact");

    first.record_unresolved(fact).expect("record first gate");

    assert_eq!(first.unresolved().expect("first gate"), Some(fact));
    assert_eq!(second.unresolved().expect("second gate"), None);
    assert!(second.require_open_for_mutation().is_ok());
}

#[test]
fn unresolved_durable_gate_serializes_active_empty_admissions() {
    let gate = CommitUnresolvedDurableGate::new();
    let first = gate.admit_mutating_commit().expect("first admission");

    assert_eq!(gate.unresolved().expect("gate read"), None);
    assert!(matches!(
        gate.admit_mutating_commit(),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable commit admission is already active",
        })
    ));
    assert_eq!(
        gate.require_open_for_mutation(),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable commit admission is already active",
        })
    );
    drop(first);
    assert!(gate.require_open_for_mutation().is_ok());
}

#[test]
fn read_only_diagnostic_ignores_unresolved_durable_gate_and_reports_current_visible() {
    let branch = branch_id(91);
    let gate = CommitUnresolvedDurableGate::new();
    gate.record_unresolved(
        CommitUnresolvedDurable::durable_not_applied_with_facts(
            stamp(branch, 50),
            CommitDurabilityClass::Standard,
            "apply failed",
        )
        .expect("fact"),
    )
    .expect("record gate");
    let batch = read_only_batch(branch, CommitBatchOptions::default());

    let outcome = execute_read_only_diagnostic(
        &batch,
        &CommitRuntimeConfig::default(),
        VisibleVersionTracker::new(CommitVersion::new(7)),
    )
    .expect("read-only diagnostic");

    assert_eq!(outcome.kind(), CommitOutcomeKind::ReadOnly);
    assert_eq!(
        outcome.read_snapshot(),
        Some(CommitReadSnapshot::new(branch, CommitVersion::new(7)))
    );
    assert_eq!(
        gate.require_open_for_mutation(),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: branch,
            commit_version: CommitVersion::new(50),
            reason: "durable commit must be replayed or reconciled first",
        })
    );
}

#[test]
fn unresolved_durable_admission_records_fact_through_gate() {
    let gate = CommitUnresolvedDurableGate::new();
    let fact = CommitUnresolvedDurable::applied_not_visible(
        stamp(branch_id(85), 8),
        CommitDurabilityClass::Always,
        "visible publish failed",
    )
    .expect("fact");

    let mut admission = gate.admit_mutating_commit().expect("admit");
    admission
        .record_unresolved(fact)
        .expect("record unresolved through admission");

    assert_eq!(gate.unresolved().expect("gate read"), Some(fact));
    assert_eq!(
        gate.require_open_for_mutation(),
        Err(CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: fact.branch_id(),
            commit_version: fact.commit_version(),
            reason: "durable commit must be replayed or reconciled first",
        })
    );
}

#[test]
fn unresolved_durable_gate_rejects_different_fact_and_exact_clear() {
    let gate = CommitUnresolvedDurableGate::new();
    let first = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(86), 8),
        CommitDurabilityClass::Standard,
        "first",
    )
    .expect("first");
    let second = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch_id(86), 9),
        CommitDurabilityClass::Standard,
        "second",
    )
    .expect("second");

    gate.record_unresolved(first).expect("record first");
    assert_eq!(
        gate.record_unresolved(second),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "different unresolved durable commit is already recorded",
        })
    );
    assert_eq!(
        gate.clear_exact(second),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "cannot clear different unresolved durable commit",
        })
    );
    gate.clear_exact(first).expect("clear first");
    assert_eq!(gate.unresolved().expect("gate read"), None);
    assert_eq!(
        gate.clear_exact(first),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "cannot clear empty unresolved durable gate",
        })
    );
}

#[test]
fn unresolved_durable_gate_replaces_only_exact_existing_fact() {
    let gate = CommitUnresolvedDurableGate::new();
    let branch = branch_id(87);
    let original = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch, 8),
        CommitDurabilityClass::Standard,
        "apply failed",
    )
    .expect("original");
    let replacement = CommitUnresolvedDurable::applied_not_visible(
        stamp(branch, 8),
        CommitDurabilityClass::Standard,
        "visible publish failed",
    )
    .expect("replacement");
    let different = CommitUnresolvedDurable::durable_not_applied_with_facts(
        stamp(branch, 9),
        CommitDurabilityClass::Standard,
        "different",
    )
    .expect("different");

    assert_eq!(
        gate.replace_exact(original, replacement),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "cannot replace empty unresolved durable gate",
        })
    );
    gate.record_unresolved(original).expect("record original");
    assert_eq!(
        gate.replace_exact(different, replacement),
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "cannot replace different unresolved durable commit",
        })
    );

    gate.replace_exact(original, replacement)
        .expect("replace original");
    assert_eq!(gate.unresolved().expect("gate read"), Some(replacement));
}

fn stamp(branch: BranchId, version: u64) -> CommitStamp {
    CommitStamp::new(
        branch,
        CommitVersion::new(version),
        Timestamp::from_micros(version.saturating_mul(1_000)),
    )
    .expect("stamp")
}
