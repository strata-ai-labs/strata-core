//! Read-set and CAS conflict validation for commit batches.

use super::{
    CommitBatchKind, CommitConflictValidationMode, CommitLowerLayer, CommitObservedVersion,
    CommitRuntimeError, CommitRuntimeResult, ValidatedCommitBatch,
};
use crate::branch::read::{BranchReadBound, BranchReadView};
use crate::observability::perf_trace;
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core_next::BranchId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitConflictKind {
    ReadSet,
    Cas,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitConflict {
    kind: CommitConflictKind,
    branch_id: BranchId,
    storage_space_id: StorageSpaceId,
    key_fingerprint: u64,
    user_key_len: usize,
    expected: CommitObservedVersion,
    actual: CommitObservedVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitConflictReport {
    checked_read_facts: usize,
    checked_cas_facts: usize,
    skipped: bool,
}

pub(crate) trait CommitConflictReadSource {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion>;
}

#[derive(Clone, Copy, Debug)]
struct UnusedConflictSource;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommitBranchReadViewConflictSource<'a> {
    read_view: &'a BranchReadView,
    bound: BranchReadBound,
}

impl CommitConflict {
    pub(crate) fn new(
        kind: CommitConflictKind,
        key: &PhysicalKey,
        expected: CommitObservedVersion,
        actual: CommitObservedVersion,
    ) -> Self {
        Self {
            kind,
            branch_id: key.branch_id(),
            storage_space_id: key.storage_space_id(),
            key_fingerprint: physical_key_fingerprint(key),
            user_key_len: key.user_key().len(),
            expected,
            actual,
        }
    }

    pub(crate) const fn kind(self) -> CommitConflictKind {
        self.kind
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn storage_space_id(self) -> StorageSpaceId {
        self.storage_space_id
    }

    pub(crate) const fn key_fingerprint(self) -> u64 {
        self.key_fingerprint
    }

    pub(crate) const fn user_key_len(self) -> usize {
        self.user_key_len
    }

    pub(crate) const fn expected(self) -> CommitObservedVersion {
        self.expected
    }

    pub(crate) const fn actual(self) -> CommitObservedVersion {
        self.actual
    }
}

impl CommitConflictReport {
    const fn checked(checked_read_facts: usize, checked_cas_facts: usize) -> Self {
        Self {
            checked_read_facts,
            checked_cas_facts,
            skipped: false,
        }
    }

    const fn skipped() -> Self {
        Self {
            checked_read_facts: 0,
            checked_cas_facts: 0,
            skipped: true,
        }
    }

    pub(crate) const fn checked_read_facts(self) -> usize {
        self.checked_read_facts
    }

    pub(crate) const fn checked_cas_facts(self) -> usize {
        self.checked_cas_facts
    }

    pub(crate) const fn skipped_validation(self) -> bool {
        self.skipped
    }
}

impl<'a> CommitBranchReadViewConflictSource<'a> {
    /// Builds a conflict source over an already-captured branch read view.
    ///
    /// This type does not provide isolation by itself. Commit executors must
    /// hold the per-branch commit guard from admission through conflict
    /// validation, apply, and visibility publication.
    pub(crate) const fn new(read_view: &'a BranchReadView) -> Self {
        Self {
            read_view,
            bound: BranchReadBound::latest(),
        }
    }

    /// Builds a conflict source capped at the executor's current visible
    /// version so unpublished branch-local rows are not treated as committed
    /// facts.
    pub(crate) const fn new_at_version(
        read_view: &'a BranchReadView,
        visible_version: strata_core_next::CommitVersion,
    ) -> Self {
        Self {
            read_view,
            bound: BranchReadBound::at_version(visible_version),
        }
    }
}

impl CommitConflictReadSource for CommitBranchReadViewConflictSource<'_> {
    fn current_observed_version(
        &self,
        key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        self.read_view
            .read_point(key, self.bound)
            .map(|row| {
                row.map_or(CommitObservedVersion::Missing, |visible| {
                    CommitObservedVersion::Present(visible.row().commit_version())
                })
            })
            .map_err(|source| {
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::BranchRuntime,
                    "conflict read view lookup failed",
                    source,
                )
            })
    }
}

pub(crate) fn validate_commit_conflicts(
    batch: &ValidatedCommitBatch,
    source: &impl CommitConflictReadSource,
) -> CommitRuntimeResult<CommitConflictReport> {
    perf_trace::record_commit_conflict_validation_call();
    let batch = batch.batch();
    if batch.kind() == CommitBatchKind::ReadOnlyDiagnostic
        || batch.options().conflict_validation() == CommitConflictValidationMode::Skip
    {
        perf_trace::record_commit_conflict_validation_skipped();
        return Ok(CommitConflictReport::skipped());
    }

    let validation = batch.validation();
    if validation.read_set().is_empty() && validation.cas_set().is_empty() {
        perf_trace::record_commit_conflict_validation_without_source();
        return Ok(CommitConflictReport::checked(0, 0));
    }

    let mut checked_read_facts = 0;
    for fact in validation.read_set() {
        checked_read_facts += 1;
        let actual = match source.current_observed_version(fact.physical_key()) {
            Ok(actual) => actual,
            Err(error) => {
                perf_trace::record_commit_conflict_validation_with_source(checked_read_facts, 0);
                return Err(error);
            }
        };
        if actual != fact.observed() {
            perf_trace::record_commit_conflict_validation_with_source(checked_read_facts, 0);
            perf_trace::record_commit_conflict_detected();
            return Err(CommitRuntimeError::CommitConflict {
                conflict: CommitConflict::new(
                    CommitConflictKind::ReadSet,
                    fact.physical_key(),
                    fact.observed(),
                    actual,
                ),
            });
        }
    }

    let mut checked_cas_facts = 0;
    for fact in validation.cas_set() {
        checked_cas_facts += 1;
        let actual = match source.current_observed_version(fact.physical_key()) {
            Ok(actual) => actual,
            Err(error) => {
                perf_trace::record_commit_conflict_validation_with_source(
                    checked_read_facts,
                    checked_cas_facts,
                );
                return Err(error);
            }
        };
        if actual != fact.expected() {
            perf_trace::record_commit_conflict_validation_with_source(
                checked_read_facts,
                checked_cas_facts,
            );
            perf_trace::record_commit_conflict_detected();
            return Err(CommitRuntimeError::CommitConflict {
                conflict: CommitConflict::new(
                    CommitConflictKind::Cas,
                    fact.physical_key(),
                    fact.expected(),
                    actual,
                ),
            });
        }
    }

    perf_trace::record_commit_conflict_validation_with_source(
        checked_read_facts,
        checked_cas_facts,
    );
    Ok(CommitConflictReport::checked(
        checked_read_facts,
        checked_cas_facts,
    ))
}

pub(crate) fn commit_conflict_validation_needs_source(batch: &ValidatedCommitBatch) -> bool {
    let batch = batch.batch();
    if batch.kind() == CommitBatchKind::ReadOnlyDiagnostic
        || batch.options().conflict_validation() == CommitConflictValidationMode::Skip
    {
        return false;
    }

    let validation = batch.validation();
    !validation.read_set().is_empty() || !validation.cas_set().is_empty()
}

pub(crate) fn validate_commit_conflicts_without_source(
    batch: &ValidatedCommitBatch,
) -> CommitRuntimeResult<CommitConflictReport> {
    debug_assert!(
        !commit_conflict_validation_needs_source(batch),
        "source-backed conflict validation requires a read source"
    );
    validate_commit_conflicts(batch, &UnusedConflictSource)
}

impl CommitConflictReadSource for UnusedConflictSource {
    fn current_observed_version(
        &self,
        _key: &PhysicalKey,
    ) -> CommitRuntimeResult<CommitObservedVersion> {
        Err(CommitRuntimeError::InvalidCommitState {
            reason: "conflict validation source was unexpectedly read",
        })
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn physical_key_fingerprint(key: &PhysicalKey) -> u64 {
    // Diagnostic-only fingerprint: stable and cheap, but not an identity or a
    // collision-resistant hash. Conflict correctness always compares full keys.
    let mut hash = FNV_OFFSET_BASIS;
    hash = hash_len_and_bytes(hash, key.branch_id().as_bytes());
    hash = hash_byte(hash, key.storage_space_id().raw());
    hash = hash_len_and_bytes(hash, key.space().as_bytes());
    hash_len_and_bytes(hash, key.user_key())
}

fn hash_len_and_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    let len = u64::try_from(bytes.len()).expect("slice length fits into u64");
    for byte in len.to_le_bytes() {
        hash = hash_byte(hash, byte);
    }
    for byte in bytes {
        hash = hash_byte(hash, *byte);
    }
    hash
}

fn hash_byte(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
}
