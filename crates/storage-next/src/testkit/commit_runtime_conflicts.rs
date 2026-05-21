//! Generated commit conflict-validation contract helpers.

use crate::commit::{
    validate_commit_conflicts, CommitBatch, CommitBatchOptions, CommitCasFact, CommitConflictKind,
    CommitConflictReadSource, CommitConflictValidationMode, CommitDuplicateKeyPolicy,
    CommitDurabilityMode, CommitExpiry, CommitLowerLayer, CommitMutation, CommitObservedVersion,
    CommitOrigin, CommitReadFact, CommitRetentionHint, CommitRuntimeConfig, CommitRuntimeError,
    CommitRuntimeResult, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use std::cell::Cell;
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion};

use super::TestkitError;

pub(crate) struct CommitRuntimeConflictContract {
    pub(crate) read_present_matches: usize,
    pub(crate) read_present_mismatches: usize,
    pub(crate) read_present_becomes_missing: usize,
    pub(crate) read_missing_matches: usize,
    pub(crate) read_missing_becomes_present: usize,
    pub(crate) cas_present_matches: usize,
    pub(crate) cas_present_mismatches: usize,
    pub(crate) cas_present_becomes_missing: usize,
    pub(crate) cas_missing_matches: usize,
    pub(crate) cas_missing_becomes_present: usize,
    pub(crate) combined_read_before_cas: usize,
    pub(crate) blind_put_no_conflicts: usize,
    pub(crate) blind_delete_no_conflicts: usize,
    pub(crate) skip_mode_no_reads: usize,
    pub(crate) lower_layer_read_failures: usize,
    pub(crate) conflict_error_vocabulary: usize,
}

pub(crate) fn check_commit_runtime_conflict_contract(
    script: &[u8],
) -> Result<CommitRuntimeConflictContract, TestkitError> {
    Ok(CommitRuntimeConflictContract {
        read_present_matches: check_read_present_match(script)?,
        read_present_mismatches: check_read_present_mismatch(script)?,
        read_present_becomes_missing: check_read_present_becomes_missing(script)?,
        read_missing_matches: check_read_missing_match(script)?,
        read_missing_becomes_present: check_read_missing_becomes_present(script)?,
        cas_present_matches: check_cas_present_match(script)?,
        cas_present_mismatches: check_cas_present_mismatch(script)?,
        cas_present_becomes_missing: check_cas_present_becomes_missing(script)?,
        cas_missing_matches: check_cas_missing_match(script)?,
        cas_missing_becomes_present: check_cas_missing_becomes_present(script)?,
        combined_read_before_cas: check_combined_ordering(script)?,
        blind_put_no_conflicts: check_blind_put(script)?,
        blind_delete_no_conflicts: check_blind_delete(script)?,
        skip_mode_no_reads: check_skip_mode(script)?,
        lower_layer_read_failures: check_lower_layer_failure(script)?,
        conflict_error_vocabulary: check_conflict_error_vocabulary(script)?,
    })
}

fn check_read_present_match(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 66));
    let key = physical_key(
        branch,
        script_byte(script, 67),
        b"read-present-match".to_vec(),
    );
    let version = version(script_byte(script, 68));
    let source =
        FakeConflictSource::new(vec![(key.clone(), CommitObservedVersion::Present(version))]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key,
                CommitObservedVersion::Present(version),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if report.checked_read_facts() != 1 || report.checked_cas_facts() != 0 {
        return Err(TestkitError::new(
            "read present match did not check one fact",
        ));
    }
    Ok(1)
}

fn check_read_present_mismatch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 69));
    let key = physical_key(
        branch,
        script_byte(script, 70),
        b"read-present-mismatch".to_vec(),
    );
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(11)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(10)),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::ReadSet,
    )?;
    Ok(1)
}

fn check_read_present_becomes_missing(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 71));
    let key = physical_key(
        branch,
        script_byte(script, 72),
        b"read-present-missing".to_vec(),
    );
    let source = FakeConflictSource::new(Vec::new());
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(12)),
            )],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::ReadSet,
    )?;
    Ok(1)
}

fn check_read_missing_match(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 73));
    let key = physical_key(
        branch,
        script_byte(script, 74),
        b"read-missing-match".to_vec(),
    );
    let source = FakeConflictSource::new(Vec::new());
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if report.checked_read_facts() != 1 {
        return Err(TestkitError::new(
            "read missing match did not check one fact",
        ));
    }
    Ok(1)
}

fn check_read_missing_becomes_present(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 75));
    let key = physical_key(
        branch,
        script_byte(script, 76),
        b"read-missing-present".to_vec(),
    );
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(13)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::ReadSet,
    )?;
    Ok(1)
}

fn check_cas_present_match(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 77));
    let key = physical_key(
        branch,
        script_byte(script, 78),
        b"cas-present-match".to_vec(),
    );
    let version = CommitVersion::new(14);
    let source =
        FakeConflictSource::new(vec![(key.clone(), CommitObservedVersion::Present(version))]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                key,
                CommitObservedVersion::Present(version),
            )],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if report.checked_cas_facts() != 1 {
        return Err(TestkitError::new(
            "cas present match did not check one fact",
        ));
    }
    Ok(1)
}

fn check_cas_present_mismatch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 66).wrapping_add(1));
    let key = physical_key(
        branch,
        script_byte(script, 67),
        b"cas-present-mismatch".to_vec(),
    );
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(16)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(15)),
            )],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::Cas,
    )?;
    Ok(1)
}

fn check_cas_present_becomes_missing(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 68).wrapping_add(1));
    let key = physical_key(
        branch,
        script_byte(script, 69),
        b"cas-present-missing".to_vec(),
    );
    let source = FakeConflictSource::new(Vec::new());
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(
                key,
                CommitObservedVersion::Present(CommitVersion::new(17)),
            )],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::Cas,
    )?;
    Ok(1)
}

fn check_cas_missing_match(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 70).wrapping_add(1));
    let key = physical_key(
        branch,
        script_byte(script, 71),
        b"cas-missing-match".to_vec(),
    );
    let source = FakeConflictSource::new(Vec::new());
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(key, CommitObservedVersion::Missing)],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if report.checked_cas_facts() != 1 {
        return Err(TestkitError::new(
            "cas missing match did not check one fact",
        ));
    }
    Ok(1)
}

fn check_cas_missing_becomes_present(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 72).wrapping_add(1));
    let key = physical_key(
        branch,
        script_byte(script, 73),
        b"cas-missing-present".to_vec(),
    );
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(18)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            Vec::new(),
            vec![CommitCasFact::new(key, CommitObservedVersion::Missing)],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::Cas,
    )?;
    Ok(1)
}

fn check_combined_ordering(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 74).wrapping_add(1));
    let read_key = physical_key(branch, script_byte(script, 75), b"combined-read".to_vec());
    let cas_key = physical_key(branch, script_byte(script, 76), b"combined-cas".to_vec());
    let source = FakeConflictSource::new(vec![
        (
            read_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(20)),
        ),
        (
            cas_key.clone(),
            CommitObservedVersion::Present(CommitVersion::new(30)),
        ),
    ]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                read_key,
                CommitObservedVersion::Present(CommitVersion::new(19)),
            )],
            vec![CommitCasFact::new(
                cas_key,
                CommitObservedVersion::Present(CommitVersion::new(30)),
            )],
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    expect_conflict(
        validate_commit_conflicts(&batch, &source),
        CommitConflictKind::ReadSet,
    )?;
    if source.read_count() != 1 {
        return Err(TestkitError::new(
            "cas fact was read after read-set conflict",
        ));
    }
    Ok(1)
}

fn check_blind_put(script: &[u8]) -> Result<usize, TestkitError> {
    check_blind_mutation(script, MutationKind::Put)
}

fn check_blind_delete(script: &[u8]) -> Result<usize, TestkitError> {
    check_blind_mutation(script, MutationKind::Delete)
}

fn check_blind_mutation(script: &[u8], kind: MutationKind) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 77).wrapping_add(1));
    let source = FakeConflictSource::new(vec![(
        physical_key(branch, 0x20, b"blind-current".to_vec()),
        CommitObservedVersion::Present(CommitVersion::new(22)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::empty(),
        CommitConflictValidationMode::Validate,
        kind,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if report.checked_read_facts() != 0
        || report.checked_cas_facts() != 0
        || source.read_count() != 0
    {
        return Err(TestkitError::new("blind mutation read conflict source"));
    }
    Ok(1)
}

fn check_skip_mode(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 78).wrapping_add(1));
    let key = physical_key(branch, 0x20, b"skip-conflict".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(23)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Skip,
        MutationKind::Put,
    )?;

    let report = validate_commit_conflicts(&batch, &source).map_err(testkit_error)?;
    if !report.skipped_validation() || source.read_count() != 0 {
        return Err(TestkitError::new("skip mode read conflict source"));
    }
    Ok(1)
}

fn check_lower_layer_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 66).wrapping_add(2));
    let key = physical_key(branch, 0x20, b"lower-layer-failure".to_vec());
    let source = FailingConflictSource;
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;

    match validate_commit_conflicts(&batch, &source) {
        Err(CommitRuntimeError::LowerLayer {
            layer: CommitLowerLayer::BranchRuntime,
            source: Some(_),
            ..
        }) => Ok(1),
        other => Err(TestkitError::new(format!(
            "lower-layer failure was not preserved: {other:?}"
        ))),
    }
}

fn check_conflict_error_vocabulary(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 67).wrapping_add(2));
    let key = physical_key(branch, 0x20, b"secret-user-key".to_vec());
    let source = FakeConflictSource::new(vec![(
        key.clone(),
        CommitObservedVersion::Present(CommitVersion::new(24)),
    )]);
    let batch = mutating_batch(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitConflictValidationMode::Validate,
        MutationKind::Put,
    )?;
    let error = validate_commit_conflicts(&batch, &source).expect_err("conflict");
    let display = error.to_string();

    if display.contains("secret-user-key")
        || display.contains("Transaction")
        || display.contains("rollback")
        || !display.contains("key fingerprint 0x")
        || !display.contains("storage space")
    {
        return Err(TestkitError::new(
            "conflict display leaked forbidden vocabulary",
        ));
    }
    Ok(1)
}

fn expect_conflict(
    result: CommitRuntimeResult<crate::commit::CommitConflictReport>,
    kind: CommitConflictKind,
) -> Result<(), TestkitError> {
    match result {
        Err(CommitRuntimeError::CommitConflict { conflict }) if conflict.kind() == kind => Ok(()),
        other => Err(TestkitError::new(format!(
            "expected {kind:?} conflict, got {other:?}"
        ))),
    }
}

fn mutating_batch(
    branch: BranchId,
    validation: CommitValidationFacts,
    mode: CommitConflictValidationMode,
    kind: MutationKind,
) -> Result<crate::commit::ValidatedCommitBatch, TestkitError> {
    let key = physical_key(branch, 0x21, b"mutation".to_vec());
    let mutation = match kind {
        MutationKind::Put => CommitMutation::put(
            key,
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        ),
        MutationKind::Delete => CommitMutation::delete(key),
    };
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
    .map_err(testkit_error)
}

#[derive(Clone, Copy)]
enum MutationKind {
    Put,
    Delete,
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

struct FailingConflictSource;

impl CommitConflictReadSource for FailingConflictSource {
    fn current_observed_version(
        &self,
        _key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        Err(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::BranchRuntime,
            "generated conflict source failure",
            GeneratedConflictSourceFailure,
        ))
    }
}

#[derive(Debug)]
struct GeneratedConflictSourceFailure;

impl fmt::Display for GeneratedConflictSourceFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("generated conflict source failure")
    }
}

impl Error for GeneratedConflictSourceFailure {}

fn version(byte: u8) -> CommitVersion {
    CommitVersion::new(u64::from(byte) + 1)
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
        StorageSpaceId::engine(storage_space_id.max(0x20)).expect("engine-owned space"),
        user_key,
    )
    .expect("physical key")
}

#[expect(clippy::needless_pass_by_value, reason = "used directly with map_err")]
fn testkit_error(error: CommitRuntimeError) -> TestkitError {
    TestkitError::new(error.to_string())
}
