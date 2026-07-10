//! Generated commit-runtime outcome and visibility contract helpers.

use crate::commit::{
    execute_read_only_diagnostic, CommitBatch, CommitBatchOptions, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitExpiry,
    CommitMutation, CommitMutationCounts, CommitOrigin, CommitOutcome, CommitOutcomeKind,
    CommitPhase, CommitReadOnlyDiagnostics, CommitReadSnapshot, CommitRetentionHint,
    CommitRuntimeConfig, CommitRuntimeError, CommitStamp, CommitTimestampPolicy,
    CommitValidationFacts, CommitVisibilityFacts, ValidatedCommitBatch, VisibleVersionPublish,
    VisibleVersionTracker,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

pub(crate) struct CommitRuntimeOutcomeContract {
    pub(crate) read_only_outcomes: usize,
    pub(crate) read_only_disabled_rejections: usize,
    pub(crate) visible_tracker_initializations: usize,
    pub(crate) visible_tracker_monotonic_publishes: usize,
    pub(crate) visible_tracker_regression_rejections: usize,
    pub(crate) invalid_visibility_facts: usize,
    pub(crate) outcome_constructor_rejections: usize,
    pub(crate) mutation_count_facts: usize,
    pub(crate) cross_branch_read_only_facts: usize,
    pub(crate) read_only_no_allocation_proofs: usize,
}

pub(crate) fn check_commit_runtime_outcome_contract(
    script: &[u8],
) -> Result<CommitRuntimeOutcomeContract, TestkitError> {
    Ok(CommitRuntimeOutcomeContract {
        read_only_outcomes: check_read_only_outcome(script)?,
        read_only_disabled_rejections: check_read_only_disabled(script)?,
        visible_tracker_initializations: check_visible_tracker_initialization(script)?,
        visible_tracker_monotonic_publishes: check_visible_tracker_monotonic_publish(script)?,
        visible_tracker_regression_rejections: check_visible_tracker_regression(script)?,
        invalid_visibility_facts: check_invalid_visibility_facts()?,
        outcome_constructor_rejections: check_outcome_constructor_rejections()?,
        mutation_count_facts: check_mutation_counts(script)?,
        cross_branch_read_only_facts: check_cross_branch_read_only_facts(script)?,
        read_only_no_allocation_proofs: check_read_only_no_allocation(script)?,
    })
}

fn check_read_only_outcome(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 40));
    let visible_version = CommitVersion::new(u64::from(script_byte(script, 41)) + 1);
    let tracker = VisibleVersionTracker::new(visible_version);
    let batch = read_only_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Always,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(90)),
            CommitOrigin::Diagnostic,
        ),
    )?;
    let outcome = execute_read_only_diagnostic(&batch, &CommitRuntimeConfig::default(), tracker)
        .map_err(|err| TestkitError::new(format!("read-only outcome failed: {err}")))?;

    if outcome.kind() != CommitOutcomeKind::ReadOnly
        || outcome.branch_id() != branch
        || outcome.commit_version().is_some()
        || outcome.commit_timestamp().is_some()
        || outcome.durability() != CommitDurabilityClass::NotDurable
        || outcome.read_snapshot() != Some(CommitReadSnapshot::new(branch, visible_version))
    {
        return Err(TestkitError::new(
            "read-only outcome did not preserve snapshot facts",
        ));
    }
    Ok(1)
}

fn check_read_only_disabled(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 42));
    let tracker = VisibleVersionTracker::new(CommitVersion::new(7));
    let config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Disabled)
        .map_err(|err| TestkitError::new(format!("disabled config rejected: {err}")))?;
    let batch = read_only_batch(branch, CommitBatchOptions::default())?;

    if !matches!(
        execute_read_only_diagnostic(&batch, &config, tracker),
        Err(CommitRuntimeError::InvalidCommitPhase { .. })
    ) || tracker.visible_version() != CommitVersion::new(7)
    {
        return Err(TestkitError::new(
            "disabled read-only diagnostic was not rejected cleanly",
        ));
    }
    Ok(1)
}

fn check_visible_tracker_initialization(script: &[u8]) -> Result<usize, TestkitError> {
    let visible = CommitVersion::new(u64::from(script_byte(script, 43)) + 1);
    let tracker = VisibleVersionTracker::new(visible);
    if tracker.visible_version() != visible {
        return Err(TestkitError::new(
            "visible tracker did not preserve initial version",
        ));
    }
    Ok(1)
}

fn check_visible_tracker_monotonic_publish(script: &[u8]) -> Result<usize, TestkitError> {
    let first = CommitVersion::new(u64::from(script_byte(script, 44)) + 1);
    let second = CommitVersion::new(first.as_u64().saturating_add(1));
    let mut tracker = VisibleVersionTracker::default();
    if tracker.publish_visible(first)
        != Ok(VisibleVersionPublish::Advanced {
            previous: CommitVersion::ZERO,
            current: first,
        })
        || tracker.publish_visible(first) != Ok(VisibleVersionPublish::Unchanged { current: first })
        || tracker.publish_visible(second)
            != Ok(VisibleVersionPublish::Advanced {
                previous: first,
                current: second,
            })
    {
        return Err(TestkitError::new(
            "visible tracker publish was not monotonic",
        ));
    }
    Ok(3)
}

fn check_visible_tracker_regression(script: &[u8]) -> Result<usize, TestkitError> {
    let floor = CommitVersion::new(u64::from(script_byte(script, 45)) + 2);
    let lower = CommitVersion::new(floor.as_u64() - 1);
    let mut tracker = VisibleVersionTracker::new(floor);
    if !matches!(
        tracker.publish_visible(lower),
        Err(CommitRuntimeError::InvalidVisibilityFacts { .. })
    ) || tracker.visible_version() != floor
    {
        return Err(TestkitError::new(
            "visible tracker accepted a regressing publish",
        ));
    }
    Ok(1)
}

fn check_invalid_visibility_facts() -> Result<usize, TestkitError> {
    let v1 = CommitVersion::new(1);
    let v2 = CommitVersion::new(2);
    let invalid = [
        CommitVisibilityFacts::new(Some(v1), Some(v2), None, None, None),
        CommitVisibilityFacts::new(Some(v1), None, Some(v2), None, None),
        CommitVisibilityFacts::new(Some(v2), None, Some(v1), Some(v2), Some(v2)),
        CommitVisibilityFacts::new(Some(v2), None, Some(v1), None, Some(v2)),
        CommitVisibilityFacts::new(Some(v2), None, Some(v2), Some(v2), Some(v1)),
        CommitVisibilityFacts::new(None, Some(v1), None, None, None),
        CommitVisibilityFacts::new(None, None, Some(v1), None, None),
    ];
    for result in &invalid {
        if result.is_ok() {
            return Err(TestkitError::new("invalid visibility facts were accepted"));
        }
    }
    Ok(invalid.len())
}

fn check_outcome_constructor_rejections() -> Result<usize, TestkitError> {
    let fixtures = OutcomeConstructorFixtures::new()?;
    let mut cases = 0usize;
    cases += check_rejected_read_only_and_not_visible_shapes(&fixtures)?;
    cases += check_rejected_durable_shapes(&fixtures)?;
    cases += check_accepted_non_visible_shapes(&fixtures)?;
    Ok(cases)
}

#[derive(Clone, Copy)]
struct OutcomeConstructorFixtures {
    branch: BranchId,
    stamp: CommitStamp,
    visible_facts: CommitVisibilityFacts,
    durable_facts: CommitVisibilityFacts,
    applied_facts: CommitVisibilityFacts,
    applied_durable_facts: CommitVisibilityFacts,
}

impl OutcomeConstructorFixtures {
    fn new() -> Result<Self, TestkitError> {
        let branch = branch_id(44);
        let stamp = CommitStamp::new(branch, CommitVersion::new(8), Timestamp::from_micros(8))
            .map_err(|err| TestkitError::new(format!("stamp rejected: {err}")))?;
        let visible_facts = CommitVisibilityFacts::new(
            Some(stamp.commit_version()),
            None,
            Some(stamp.commit_version()),
            Some(stamp.commit_version()),
            Some(stamp.commit_version()),
        )
        .map_err(|err| TestkitError::new(format!("visible facts rejected: {err}")))?;
        let durable_facts = CommitVisibilityFacts::new(
            Some(stamp.commit_version()),
            Some(stamp.commit_version()),
            None,
            None,
            None,
        )
        .map_err(|err| TestkitError::new(format!("durable facts rejected: {err}")))?;
        let applied_facts = CommitVisibilityFacts::new(
            Some(stamp.commit_version()),
            None,
            Some(stamp.commit_version()),
            None,
            None,
        )
        .map_err(|err| TestkitError::new(format!("applied facts rejected: {err}")))?;
        let applied_durable_facts = CommitVisibilityFacts::new(
            Some(stamp.commit_version()),
            Some(stamp.commit_version()),
            Some(stamp.commit_version()),
            None,
            None,
        )
        .map_err(|err| TestkitError::new(format!("applied durable facts rejected: {err}")))?;
        Ok(Self {
            branch,
            stamp,
            visible_facts,
            durable_facts,
            applied_facts,
            applied_durable_facts,
        })
    }
}

fn check_rejected_read_only_and_not_visible_shapes(
    fixtures: &OutcomeConstructorFixtures,
) -> Result<usize, TestkitError> {
    let branch = fixtures.branch;
    let stamp = fixtures.stamp;
    let rejected = [
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
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.visible_facts,
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AllocatedNotDurable,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            CommitVisibilityFacts::empty(),
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.durable_facts,
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::Replay,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.durable_facts,
            None,
        ),
    ];
    require_all_rejected(&rejected)
}

fn check_rejected_durable_shapes(
    fixtures: &OutcomeConstructorFixtures,
) -> Result<usize, TestkitError> {
    let branch = fixtures.branch;
    let stamp = fixtures.stamp;
    let rejected = [
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::DurableNotApplied,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            CommitVisibilityFacts::empty(),
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::DurableNotApplied,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.applied_durable_facts,
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.durable_facts,
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::Visible,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.durable_facts,
            None,
        ),
    ];
    require_all_rejected(&rejected)
}

fn check_accepted_non_visible_shapes(
    fixtures: &OutcomeConstructorFixtures,
) -> Result<usize, TestkitError> {
    let branch = fixtures.branch;
    let stamp = fixtures.stamp;
    let accepted = [
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::NotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::NotDurable,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.applied_facts,
            None,
        ),
        CommitOutcome::new(
            branch,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::AppliedNotVisible,
            CommitDurabilityClass::Always,
            Some(stamp),
            mutation_counts(1, 0, 0)?,
            fixtures.applied_durable_facts,
            None,
        ),
    ];
    for outcome in &accepted {
        if outcome.is_err() {
            return Err(TestkitError::new(
                "valid non-visible commit outcome shape was rejected",
            ));
        }
    }
    Ok(accepted.len())
}

fn require_all_rejected(
    outcomes: &[Result<CommitOutcome, CommitRuntimeError>],
) -> Result<usize, TestkitError> {
    for outcome in outcomes {
        if outcome.is_ok() {
            return Err(TestkitError::new(
                "invalid commit outcome shape was accepted",
            ));
        }
    }
    Ok(outcomes.len())
}

fn check_mutation_counts(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 46));
    let mixed_batch = CommitBatch::mutating(
        branch,
        vec![
            CommitMutation::put(
                physical_key(branch, 0x20, b"put".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(physical_key(branch, 0x21, b"delete".to_vec())),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("mutating batch rejected: {err}")))?;
    let mixed_counts = CommitMutationCounts::from_validated_batch(&mixed_batch)
        .map_err(|err| TestkitError::new(format!("mutation counts failed: {err}")))?;
    if mixed_counts.puts() != 1 || mixed_counts.deletes() != 1 || mixed_counts.timeline_rows() != 0
    {
        return Err(TestkitError::new("mutation counts were incorrect"));
    }
    let delete_only = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x22,
            b"delete-only".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("delete-only batch rejected: {err}")))?;
    let delete_counts = CommitMutationCounts::from_validated_batch(&delete_only)
        .map_err(|err| TestkitError::new(format!("delete counts failed: {err}")))?;
    if delete_counts.puts() != 0
        || delete_counts.deletes() != 1
        || delete_counts.timeline_rows() != 0
    {
        return Err(TestkitError::new(
            "delete-only mutation counts were incorrect",
        ));
    }
    let read_only = read_only_batch(branch, CommitBatchOptions::default())?;
    if CommitMutationCounts::from_validated_batch(&read_only)
        .map_err(|err| TestkitError::new(format!("read-only counts failed: {err}")))?
        != CommitMutationCounts::read_only()
    {
        return Err(TestkitError::new("read-only counts were not zero"));
    }
    let small_config = CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled)
        .map_err(|err| TestkitError::new(format!("small config rejected: {err}")))?;
    if CommitMutationCounts::new(2, 0, 0, &small_config).is_ok()
        || CommitMutationCounts::new(1, 0, 1, &small_config).is_ok()
    {
        return Err(TestkitError::new(
            "mutation count constructor accepted configured overrun",
        ));
    }
    Ok(5)
}

fn check_cross_branch_read_only_facts(script: &[u8]) -> Result<usize, TestkitError> {
    let first_branch = branch_id(script_byte(script, 47));
    let second_branch = branch_id(script_byte(script, 47).wrapping_add(1));
    let tracker = VisibleVersionTracker::new(CommitVersion::new(12));
    let first = execute_read_only_diagnostic(
        &read_only_batch(first_branch, CommitBatchOptions::default())?,
        &CommitRuntimeConfig::default(),
        tracker,
    )
    .map_err(|err| TestkitError::new(format!("first read-only outcome failed: {err}")))?;
    let second = execute_read_only_diagnostic(
        &read_only_batch(second_branch, CommitBatchOptions::default())?,
        &CommitRuntimeConfig::default(),
        tracker,
    )
    .map_err(|err| TestkitError::new(format!("second read-only outcome failed: {err}")))?;

    if first.branch_id() == second.branch_id()
        || first
            .read_snapshot()
            .map(CommitReadSnapshot::visible_version)
            != second
                .read_snapshot()
                .map(CommitReadSnapshot::visible_version)
    {
        return Err(TestkitError::new(
            "cross-branch read-only facts lost branch isolation or global visibility",
        ));
    }
    Ok(1)
}

fn check_read_only_no_allocation(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 48));
    let tracker = VisibleVersionTracker::new(CommitVersion::new(3));
    let outcome = execute_read_only_diagnostic(
        &read_only_batch(branch, CommitBatchOptions::default())?,
        &CommitRuntimeConfig::default(),
        tracker,
    )
    .map_err(|err| TestkitError::new(format!("read-only outcome failed: {err}")))?;
    if outcome.commit_version().is_some()
        || outcome.commit_timestamp().is_some()
        || outcome.mutation_counts() != CommitMutationCounts::read_only()
        || outcome.visibility_facts() != CommitVisibilityFacts::empty()
    {
        return Err(TestkitError::new(
            "read-only diagnostic produced allocation or visibility facts",
        ));
    }
    Ok(1)
}

fn read_only_batch(
    branch: BranchId,
    options: CommitBatchOptions,
) -> Result<ValidatedCommitBatch, TestkitError> {
    CommitBatch::read_only_diagnostic(branch, CommitValidationFacts::empty(), options)
        .validate(&CommitRuntimeConfig::default())
        .map_err(|err| TestkitError::new(format!("read-only batch rejected: {err}")))
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_id: BranchId, storage_space_id: u8, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "default",
        StorageSpaceId::engine(storage_space_id).expect("engine-owned space"),
        user_key,
    )
    .expect("physical key")
}

fn mutation_counts(
    puts: usize,
    deletes: usize,
    timeline_rows: usize,
) -> Result<CommitMutationCounts, TestkitError> {
    CommitMutationCounts::new(
        puts,
        deletes,
        timeline_rows,
        &CommitRuntimeConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("mutation counts rejected: {err}")))
}
