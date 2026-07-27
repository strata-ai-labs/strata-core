//! Branch lifecycle recovery conformance helper.
//!
//! Exercises the open → commit/fork → close → reopen → verify cycle
//! through the public testkit surface. Lib tests cover the same paths
//! directly; this harness exposes the contract to external integrators.

use super::super::TestkitError;
use super::recovery::{assemble_shell, branch_id, open_plan, RecoveryScriptBackend};
use super::{ensure, script_byte};
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityMode, CommitExpiry,
    CommitMutation, CommitOrigin, CommitRetentionHint, CommitTimestampPolicy,
    CommitValidationFacts,
};
use crate::lifecycle::{LifecycleRecoveryRequest, LifecycleRecoveryRuntime, RecoveryStrictness};
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core::{BranchId, CommitVersion};

/// Coverage counters for branch lifecycle recovery contract checks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_field_names,
    reason = "counter suffix keeps the testkit getter vocabulary explicit"
)]
pub struct LifecycleBranchLifecycleRecoveryContractOutcome {
    branches_recovered: usize,
    forks_recovered: usize,
    rows_recovered: usize,
}

impl LifecycleBranchLifecycleRecoveryContractOutcome {
    pub const fn branches_recovered(&self) -> usize {
        self.branches_recovered
    }

    pub const fn forks_recovered(&self) -> usize {
        self.forks_recovered
    }

    pub const fn rows_recovered(&self) -> usize {
        self.rows_recovered
    }
}

/// Exercise the durable open → commit → fork → close → reopen → verify
/// cycle. Asserts that the catalog descriptors and per-branch row state
/// survive restart on a real in-memory backend.
pub fn check_lifecycle_branch_lifecycle_recovery_contract(
    script: &[u8],
) -> Result<LifecycleBranchLifecycleRecoveryContractOutcome, TestkitError> {
    let mut outcome = LifecycleBranchLifecycleRecoveryContractOutcome::default();
    check_durable_branch_recovery_round_trip(script, &mut outcome)?;
    Ok(outcome)
}

#[allow(
    clippy::too_many_lines,
    reason = "single-pass scenario walks open + create + commit + fork + reopen + verify; splitting would split related assertions"
)]
fn check_durable_branch_recovery_round_trip(
    script: &[u8],
    outcome: &mut LifecycleBranchLifecycleRecoveryContractOutcome,
) -> Result<(), TestkitError> {
    let backend: &'static RecoveryScriptBackend =
        crate::testkit::leak_static(RecoveryScriptBackend::new());
    let initial = branch_id(script_byte(script, 0).wrapping_add(1));
    let extra = branch_id(script_byte(script, 1).wrapping_add(2));
    let child = branch_id(script_byte(script, 2).wrapping_add(3));

    ensure(
        initial != extra && initial != child && extra != child,
        "branch lifecycle recovery script produced colliding branch ids",
    )?;

    let generation_one = CommitBranchGeneration::new(1).map_err(testkit_error)?;
    let guard = CommitBranchGenerationGuard::exact(generation_one);

    {
        let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), initial, backend)?;
        let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
            .map_err(|error| testkit_error(&error))?;
        let recovered = LifecycleRecoveryRuntime::new(&mut shell)
            .recover(&request)
            .map_err(|error| testkit_error(&error))?;
        let mut runtime = shell
            .complete_recovery(&recovered)
            .map_err(|error| testkit_error(&error))?;

        runtime
            .create_branch(extra, generation_one, Some(CommitVersion::new(2)))
            .map_err(|error| testkit_error(&error))?;
        runtime
            .execute_durable_commit(
                durable_batch(initial, b"initial-row", b"initial-value"),
                guard,
            )
            .map_err(|error| testkit_error(&error))?;
        runtime
            .execute_durable_commit(durable_batch(extra, b"extra-row", b"extra-value"), guard)
            .map_err(|error| testkit_error(&error))?;
        // Fork the seeded branch at its visible-history version. The
        // child inherits the row via timeline lookup, not the
        // active-set-must-be-empty fork_current path.
        runtime
            .fork_at_retained_version(
                initial,
                child,
                generation_one,
                CommitVersion::new(1),
                CommitVersion::ZERO,
                None,
            )
            .map_err(|error| testkit_error(&error))?;
    }

    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), initial, backend)?;
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .map_err(|error| testkit_error(&error))?;
    let recovered = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .map_err(|error| testkit_error(&error))?;
    let runtime = shell
        .complete_recovery(&recovered)
        .map_err(|error| testkit_error(&error))?;

    let descriptors = runtime.list_branches(false);
    ensure(
        descriptors.iter().any(|d| d.branch_id() == initial),
        "initial branch not present after recovery",
    )?;
    ensure(
        descriptors.iter().any(|d| d.branch_id() == extra),
        "non-seeded branch not present after recovery",
    )?;
    let child_descriptor = descriptors
        .iter()
        .find(|d| d.branch_id() == child)
        .ok_or_else(|| TestkitError::new("forked child not present after recovery"))?;
    ensure(
        child_descriptor.parent().is_some(),
        "forked child lost parent metadata after recovery",
    )?;

    let initial_state = runtime
        .branch_catalog()
        .branch_state(initial)
        .map_err(|error| testkit_error(&error))?;
    let initial_view = initial_state
        .capture_read_view()
        .map_err(|error| testkit_error_branch(&error))?;
    let initial_row = initial_view
        .latest(&physical_key(initial, b"initial-row"))
        .map_err(|error| testkit_error_branch(&error))?
        .ok_or_else(|| TestkitError::new("initial branch row missing after recovery"))?;
    ensure(
        initial_row.row().value() == b"initial-value",
        "initial branch row value changed after recovery",
    )?;

    let extra_state = runtime
        .branch_catalog()
        .branch_state(extra)
        .map_err(|error| testkit_error(&error))?;
    let extra_view = extra_state
        .capture_read_view()
        .map_err(|error| testkit_error_branch(&error))?;
    let extra_row = extra_view
        .latest(&physical_key(extra, b"extra-row"))
        .map_err(|error| testkit_error_branch(&error))?
        .ok_or_else(|| TestkitError::new("non-seeded branch row missing after recovery"))?;
    ensure(
        extra_row.row().value() == b"extra-value",
        "non-seeded branch row value changed after recovery",
    )?;

    outcome.branches_recovered += 2;
    outcome.forks_recovered += 1;
    outcome.rows_recovered += 2;
    Ok(())
}

fn durable_batch(branch: BranchId, user_key: &'static [u8], value: &'static [u8]) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, user_key),
            value.to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Standard,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "lifecycle",
        StorageSpaceId::engine(0x20).expect("engine space id"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn testkit_error<E: std::fmt::Display>(error: E) -> TestkitError {
    TestkitError::new(error.to_string())
}

fn testkit_error_branch(error: &crate::branch::error::BranchRuntimeError) -> TestkitError {
    TestkitError::new(error.to_string())
}
