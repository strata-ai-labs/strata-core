//! Generated branch-registry and commit-guard contract helpers.

use crate::commit::{
    admit_mutating_commit, execute_read_only_diagnostic, CommitBatch, CommitBatchOptions,
    CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitBranchRegistry, CommitBranchState, CommitMutation, CommitReadOnlyDiagnostics,
    CommitRuntimeConfig, CommitRuntimeError, CommitValidationFacts, VisibleVersionTracker,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion};

use super::TestkitError;

pub(crate) struct CommitRuntimeBranchGuardContract {
    pub(crate) branch_registration_successes: usize,
    pub(crate) duplicate_registration_rejections: usize,
    pub(crate) missing_branch_rejections: usize,
    pub(crate) deleting_branch_rejections: usize,
    pub(crate) generation_exact_matches: usize,
    pub(crate) generation_mismatches: usize,
    pub(crate) generation_not_supplied: usize,
    pub(crate) stale_generation_after_recreate: usize,
    pub(crate) same_branch_guard_contentions: usize,
    pub(crate) different_branch_simultaneous_guards: usize,
    pub(crate) quiesce_start_successes: usize,
    pub(crate) quiesce_rejected_with_active_guards: usize,
    pub(crate) mutating_guard_rejected_during_quiesce: usize,
    pub(crate) read_only_allowed_during_quiesce: usize,
    pub(crate) guard_release_and_reacquire: usize,
}

pub(crate) fn check_commit_runtime_branch_guard_contract(
    script: &[u8],
) -> Result<CommitRuntimeBranchGuardContract, TestkitError> {
    Ok(CommitRuntimeBranchGuardContract {
        branch_registration_successes: check_registration_success(script)?,
        duplicate_registration_rejections: check_duplicate_registration(script)?,
        missing_branch_rejections: check_missing_branch(script)?,
        deleting_branch_rejections: check_deleting_and_deleted(script)?,
        generation_exact_matches: check_generation_exact_match(script)?,
        generation_mismatches: check_generation_mismatch(script)?,
        generation_not_supplied: check_generation_not_supplied(script)?,
        stale_generation_after_recreate: check_stale_generation_after_recreate(script)?,
        same_branch_guard_contentions: check_same_branch_guard_contention(script)?,
        different_branch_simultaneous_guards: check_different_branch_guards(script)?,
        quiesce_start_successes: check_quiesce_success(script)?,
        quiesce_rejected_with_active_guards: check_quiesce_rejects_active_guard(script)?,
        mutating_guard_rejected_during_quiesce: check_mutating_guard_rejected_during_quiesce(
            script,
        )?,
        read_only_allowed_during_quiesce: check_read_only_allowed_during_quiesce(script)?,
        guard_release_and_reacquire: check_guard_release_and_reacquire(script)?,
    })
}

fn check_registration_success(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 49));
    let generation = generation(u64::from(script_byte(script, 50)) + 1)?;
    let mut registry = CommitBranchRegistry::new();
    let descriptor = registry
        .register_active(branch, generation)
        .map_err(|err| TestkitError::new(format!("branch registration failed: {err}")))?;

    if descriptor.branch_id() != branch
        || descriptor.generation() != generation
        || descriptor.state() != CommitBranchState::Active
        || registry.lookup(branch) != Ok(descriptor)
    {
        return Err(TestkitError::new(
            "branch registration did not preserve descriptor facts",
        ));
    }
    Ok(1)
}

fn check_duplicate_registration(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 51));
    let mut registry = registered_registry(branch, 1)?;
    if !matches!(
        registry.register_active(branch, generation(2)?),
        Err(CommitRuntimeError::BranchAlreadyExists { .. })
    ) {
        return Err(TestkitError::new(
            "duplicate branch registration was accepted",
        ));
    }
    Ok(1)
}

fn check_missing_branch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 52));
    let registry = CommitBranchRegistry::new();
    let guard_set = CommitBranchGuardSet::new();
    let batch = mutating_batch(branch)?;
    if !matches!(
        admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        ),
        Err(CommitRuntimeError::BranchNotFound { .. })
    ) || guard_set.active_guard_count().map_err(testkit_error)? != 0
    {
        return Err(TestkitError::new(
            "missing branch did not reject before guard acquisition",
        ));
    }
    Ok(1)
}

fn check_deleting_and_deleted(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 53));
    let guard_set = CommitBranchGuardSet::new();
    let batch = mutating_batch(branch)?;
    let mut registry = registered_registry(branch, 1)?;
    registry
        .mark_deleting(branch)
        .map_err(|err| TestkitError::new(format!("mark deleting failed: {err}")))?;
    if !matches!(
        admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        ),
        Err(CommitRuntimeError::BranchNotWritable { .. })
    ) {
        return Err(TestkitError::new("deleting branch admitted a commit"));
    }

    registry
        .mark_deleted(branch)
        .map_err(|err| TestkitError::new(format!("mark deleted failed: {err}")))?;
    if !matches!(
        admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::not_supplied(),
        ),
        Err(CommitRuntimeError::BranchNotWritable { .. })
    ) {
        return Err(TestkitError::new("deleted branch admitted a commit"));
    }
    Ok(2)
}

fn check_generation_exact_match(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 54));
    let value = u64::from(script_byte(script, 55)) + 1;
    let registry = registered_registry(branch, value)?;
    let guard_set = CommitBranchGuardSet::new();
    let batch = mutating_batch(branch)?;
    let admitted = admit_mutating_commit(
        &registry,
        &guard_set,
        &batch,
        CommitBranchGenerationGuard::exact(generation(value)?),
    )
    .map_err(|err| TestkitError::new(format!("exact generation rejected: {err}")))?;

    if admitted.admission().generation() != generation(value)? {
        return Err(TestkitError::new("exact generation admission lost facts"));
    }
    Ok(1)
}

fn check_generation_mismatch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 56));
    let registry = registered_registry(branch, 5)?;
    let guard_set = CommitBranchGuardSet::new();
    let batch = mutating_batch(branch)?;
    if !matches!(
        admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::exact(generation(4)?),
        ),
        Err(CommitRuntimeError::BranchGenerationMismatch { .. })
    ) || guard_set.active_guard_count().map_err(testkit_error)? != 0
    {
        return Err(TestkitError::new(
            "generation mismatch did not reject before guard acquisition",
        ));
    }
    Ok(1)
}

fn check_generation_not_supplied(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 57));
    let registry = registered_registry(branch, 6)?;
    let guard_set = CommitBranchGuardSet::new();
    let batch = mutating_batch(branch)?;
    let admitted = admit_mutating_commit(
        &registry,
        &guard_set,
        &batch,
        CommitBranchGenerationGuard::not_supplied(),
    )
    .map_err(|err| TestkitError::new(format!("unsupplied generation rejected: {err}")))?;

    if admitted.admission().branch_id() != branch {
        return Err(TestkitError::new(
            "unsupplied generation admission lost branch fact",
        ));
    }
    Ok(1)
}

fn check_stale_generation_after_recreate(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 58));
    let mut registry = registered_registry(branch, 7)?;
    registry
        .mark_deleted(branch)
        .map_err(|err| TestkitError::new(format!("mark deleted failed: {err}")))?;
    registry
        .recreate_active(branch, generation(8)?)
        .map_err(|err| TestkitError::new(format!("recreate failed: {err}")))?;
    let batch = mutating_batch(branch)?;
    let guard_set = CommitBranchGuardSet::new();

    if !matches!(
        admit_mutating_commit(
            &registry,
            &guard_set,
            &batch,
            CommitBranchGenerationGuard::exact(generation(7)?),
        ),
        Err(CommitRuntimeError::BranchGenerationMismatch { .. })
    ) {
        return Err(TestkitError::new(
            "stale generation after recreate was accepted",
        ));
    }
    Ok(1)
}

fn check_same_branch_guard_contention(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 59));
    let guard_set = CommitBranchGuardSet::new();
    let first = guard_set
        .try_acquire_branch_guard(branch)
        .map_err(testkit_error)?;
    if !matches!(
        guard_set.try_acquire_branch_guard(branch),
        Err(CommitRuntimeError::BranchGuardUnavailable { .. })
    ) {
        return Err(TestkitError::new(
            "same branch guard contention was accepted",
        ));
    }
    drop(first);
    Ok(1)
}

fn check_different_branch_guards(script: &[u8]) -> Result<usize, TestkitError> {
    let branch_a = branch_id(script_byte(script, 60));
    let branch_b = branch_id(script_byte(script, 60).wrapping_add(1));
    let guard_set = CommitBranchGuardSet::new();
    let guard_a = guard_set
        .try_acquire_branch_guard(branch_a)
        .map_err(testkit_error)?;
    let guard_b = guard_set
        .try_acquire_branch_guard(branch_b)
        .map_err(testkit_error)?;
    if guard_set.active_guard_count().map_err(testkit_error)? != 2 {
        return Err(TestkitError::new(
            "different branch guards did not stay independent",
        ));
    }
    drop(guard_a);
    drop(guard_b);
    Ok(1)
}

fn check_quiesce_success(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 61));
    let guard_set = CommitBranchGuardSet::new();
    let quiesce = guard_set.try_begin_quiesce().map_err(testkit_error)?;
    if !guard_set.is_quiescing().map_err(testkit_error)? {
        return Err(TestkitError::new("quiesce token did not close guard set"));
    }
    drop(quiesce);
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .map_err(testkit_error)?;
    drop(guard);
    Ok(1)
}

fn check_quiesce_rejects_active_guard(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 62));
    let guard_set = CommitBranchGuardSet::new();
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .map_err(testkit_error)?;
    if !matches!(
        guard_set.try_begin_quiesce(),
        Err(CommitRuntimeError::CommitQuiesceUnavailable { .. })
    ) {
        return Err(TestkitError::new(
            "quiesce started while branch guard was active",
        ));
    }
    drop(guard);
    Ok(1)
}

fn check_mutating_guard_rejected_during_quiesce(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 63));
    let guard_set = CommitBranchGuardSet::new();
    let quiesce = guard_set.try_begin_quiesce().map_err(testkit_error)?;
    if !matches!(
        guard_set.try_acquire_branch_guard(branch),
        Err(CommitRuntimeError::CommitQuiesceUnavailable { .. })
    ) {
        return Err(TestkitError::new("quiesce admitted mutating guard"));
    }
    drop(quiesce);
    Ok(1)
}

fn check_read_only_allowed_during_quiesce(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 64));
    let guard_set = CommitBranchGuardSet::new();
    let quiesce = guard_set.try_begin_quiesce().map_err(testkit_error)?;
    let batch = CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(testkit_error)?;
    let outcome = execute_read_only_diagnostic(
        &batch,
        &CommitRuntimeConfig::default(),
        VisibleVersionTracker::new(CommitVersion::new(1)),
    )
    .map_err(testkit_error)?;
    if outcome.branch_id() != branch
        || guard_set.active_guard_count().map_err(testkit_error)? != 0
        || !guard_set.is_quiescing().map_err(testkit_error)?
    {
        return Err(TestkitError::new(
            "read-only diagnostic mutated guard state during quiesce",
        ));
    }
    drop(quiesce);
    Ok(1)
}

fn check_guard_release_and_reacquire(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 65));
    let guard_set = CommitBranchGuardSet::new();
    {
        let guard = guard_set
            .try_acquire_branch_guard(branch)
            .map_err(testkit_error)?;
        drop(guard);
    }
    let guard = guard_set
        .try_acquire_branch_guard(branch)
        .map_err(testkit_error)?;
    drop(guard);
    if guard_set.active_guard_count().map_err(testkit_error)? != 0 {
        return Err(TestkitError::new("branch guard leaked after drop"));
    }
    Ok(1)
}

fn registered_registry(
    branch: BranchId,
    generation_value: u64,
) -> Result<CommitBranchRegistry, TestkitError> {
    let mut registry = CommitBranchRegistry::new();
    registry
        .register_active(branch, generation(generation_value)?)
        .map_err(testkit_error)?;
    Ok(registry)
}

fn mutating_batch(branch: BranchId) -> Result<crate::commit::ValidatedCommitBatch, TestkitError> {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x20,
            b"guard".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(
        &CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled)
            .map_err(testkit_error)?,
    )
    .map_err(testkit_error)
}

fn generation(value: u64) -> Result<CommitBranchGeneration, TestkitError> {
    CommitBranchGeneration::new(value).map_err(testkit_error)
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

#[expect(clippy::needless_pass_by_value, reason = "used directly with map_err")]
fn testkit_error(error: CommitRuntimeError) -> TestkitError {
    TestkitError::new(error.to_string())
}
