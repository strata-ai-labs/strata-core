//! Generated cache lifecycle checks.

use super::super::TestkitError;
use super::{ensure, script_byte, LifecycleScaffoldOutcome};
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult,
};
use crate::branch::config::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::lifecycle::{
    CloseOutcomeStatus, ClosePhase, LifecycleCacheOpenRequest, LifecycleCacheRuntime,
    LifecycleCodecId, LifecycleConfig, LifecycleError, LifecycleState, RecoveryStrictness,
    StorageMode, StorageOpenDisposition, StorageOpenPlan,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(super) fn check_lifecycle_cache_contract(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    check_cache_open_accepts(script, outcome)?;
    check_cache_open_rejects(outcome)?;
    check_cache_capability_rejection(script, outcome)?;
    check_cache_baseline(script, outcome)?;
    check_cache_commit_read(script, outcome)?;
    check_cache_close(script, outcome)?;
    check_cache_reopen_empty(script, outcome)?;
    check_input_derived_cache_operation(script, outcome)?;
    Ok(())
}

fn check_cache_open_accepts(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend = MemoryBackend::new();
    let runtime = open_runtime(
        branch_id(script_byte(script, 16)),
        &backend,
        timestamp(script, 17),
    )?;
    ensure(
        runtime.state() == LifecycleState::Open,
        "cache open did not reach Open",
    )?;
    ensure(
        runtime.open_outcome().mode() == StorageMode::Cache,
        "cache open outcome mode drifted",
    )?;
    outcome.cache_open_accepted_cases += 1;
    Ok(())
}

fn check_cache_open_rejects(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let rejected = LifecycleCacheOpenRequest::new(
        open_plan(StorageMode::DurableLocalStandard),
        branch_id(0x71),
        CommitBranchGeneration::new(1).map_err(testkit_error)?,
    );
    ensure(
        matches!(rejected, Err(LifecycleError::InvalidOpenPlan { .. })),
        "cache open request accepted non-cache storage mode",
    )?;
    outcome.cache_open_rejected_cases += 1;
    Ok(())
}

fn check_cache_capability_rejection(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend = NoCapabilitiesBackend;
    let rejected = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            open_plan(StorageMode::Cache),
            branch_id(script_byte(script, 17)),
            CommitBranchGeneration::new(1).map_err(testkit_error)?,
        )
        .map_err(testkit_error)?,
        &backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(timestamp(script, 18)),
    );
    ensure(
        matches!(rejected, Err(LifecycleError::CapabilityMismatch { .. })),
        "cache runtime accepted a backend with missing cache capabilities",
    )?;
    outcome.cache_open_rejected_cases += 1;
    Ok(())
}

fn check_cache_baseline(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend = MemoryBackend::new();
    let runtime = open_runtime(
        branch_id(script_byte(script, 18)),
        &backend,
        timestamp(script, 19),
    )?;
    ensure(
        runtime.open_outcome().disposition() == StorageOpenDisposition::Created,
        "cache open did not report created disposition",
    )?;
    ensure(
        runtime.open_outcome().recovered_visible_version().is_none(),
        "cache open claimed recovered durable visibility",
    )?;
    ensure(
        runtime.open_outcome().recovery_health().is_healthy(),
        "cache open did not report healthy no-recovery state",
    )?;
    ensure(
        runtime.visible_version() == CommitVersion::ZERO,
        "cache open visible version was not zero",
    )?;
    ensure(
        runtime.branch_state().is_empty(),
        "cache open branch state was not empty",
    )?;
    ensure(
        runtime.capability_outcome().durability_policy().is_none(),
        "cache capability outcome reported a durable policy",
    )?;
    ensure(
        runtime
            .unresolved_durable()
            .map_err(testkit_error)?
            .is_none(),
        "cache open started with unresolved durable state",
    )?;
    outcome.cache_baseline_cases += 1;
    outcome.cache_durable_absence_cases += 1;
    Ok(())
}

fn check_cache_commit_read(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 20));
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend, timestamp(script, 21))?;
    let key = physical_key(branch, &[script_byte(script, 22), b'k']);
    let value = vec![script_byte(script, 23), b'v'];
    let commit_timestamp = timestamp(script, 24);
    let commit = runtime
        .execute_cache_commit(
            put_batch(branch, key.clone(), value.clone(), commit_timestamp),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .map_err(testkit_error)?;
    ensure(
        commit.durability() == CommitDurabilityClass::NotDurable,
        "cache commit claimed durable class",
    )?;
    ensure(
        runtime.visible_version() == CommitVersion::new(1),
        "cache commit did not publish visible version",
    )?;
    let row = runtime
        .read_view()
        .map_err(testkit_error)?
        .latest(&key)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("cache read did not find committed row"))?;
    ensure(
        row.row().value() == value,
        "cache read returned wrong value",
    )?;
    outcome.cache_commit_read_cases += 1;
    Ok(())
}

fn check_cache_close(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(
        branch_id(script_byte(script, 25)),
        &backend,
        timestamp(script, 26),
    )?;
    let close = runtime.close().map_err(testkit_error)?;
    ensure(
        close.status() == CloseOutcomeStatus::Complete,
        "cache close did not complete",
    )?;
    ensure(
        runtime.state() == LifecycleState::Closed,
        "cache close did not reach Closed",
    )?;
    let second = runtime.close().map_err(testkit_error)?;
    ensure(
        second.phase() == ClosePhase::Closed,
        "idempotent cache close did not report closed phase",
    )?;
    ensure(
        runtime.read_view().is_err(),
        "cache runtime admitted read after close",
    )?;
    let branch = branch_id(script_byte(script, 25));
    let key = physical_key(branch, &[script_byte(script, 27), b'c']);
    ensure(
        runtime
            .execute_cache_commit(
                put_batch(branch, key, b"closed".to_vec(), timestamp(script, 28)),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .is_err(),
        "cache runtime admitted commit after close",
    )?;
    outcome.cache_close_cases += 1;
    outcome.cache_close_idempotence_cases += 1;
    outcome.cache_commit_after_close_rejected_cases += 1;
    Ok(())
}

fn check_cache_reopen_empty(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 27));
    let backend = MemoryBackend::new();
    let key = physical_key(branch, &[script_byte(script, 28), b'r']);
    let mut first = open_runtime(branch, &backend, timestamp(script, 29))?;
    first
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"volatile".to_vec(),
                timestamp(script, 30),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .map_err(testkit_error)?;
    first.close().map_err(testkit_error)?;

    let second = open_runtime(branch, &backend, timestamp(script, 31))?;
    ensure(
        second.visible_version() == CommitVersion::ZERO,
        "cache reopen preserved prior visible version",
    )?;
    ensure(
        second.branch_state().is_empty(),
        "cache reopen preserved volatile branch state",
    )?;
    ensure(
        second
            .read_view()
            .map_err(testkit_error)?
            .latest(&key)
            .map_err(testkit_error)?
            .is_none(),
        "cache reopen recovered prior volatile row",
    )?;
    outcome.cache_reopen_empty_cases += 1;
    Ok(())
}

fn check_input_derived_cache_operation(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 32));
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend, timestamp(script, 33))?;
    match script_byte(script, 34) % 3 {
        0 => {
            let key = physical_key(branch, &[script_byte(script, 35), b'i']);
            runtime
                .execute_cache_commit(
                    put_batch(branch, key, b"input".to_vec(), timestamp(script, 36)),
                    CommitBranchGenerationGuard::not_supplied(),
                )
                .map_err(testkit_error)?;
        }
        1 => {
            runtime.read_view().map_err(testkit_error)?;
        }
        _ => {
            runtime.close().map_err(testkit_error)?;
        }
    }
    outcome.input_derived_cache_cases += 1;
    Ok(())
}

fn open_runtime(
    branch: BranchId,
    backend: &dyn crate::backend::Backend,
    next_timestamp: Timestamp,
) -> Result<LifecycleCacheRuntime<CommitManualTimestampSource>, TestkitError> {
    LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            open_plan(StorageMode::Cache),
            branch,
            CommitBranchGeneration::new(1).map_err(testkit_error)?,
        )
        .map_err(testkit_error)?,
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(next_timestamp),
    )
    .map_err(testkit_error)
}

fn open_plan(mode: StorageMode) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("valid open plan")
}

fn put_batch(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Skip,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(timestamp),
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "cache",
        StorageSpaceId::engine(0x20).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte.max(1); BranchId::BYTE_LEN])
}

fn timestamp(script: &[u8], offset: usize) -> Timestamp {
    Timestamp::from_micros(u64::from(script_byte(script, offset)).saturating_add(1_000))
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}

struct NoCapabilitiesBackend;

impl Backend for NoCapabilitiesBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::empty()
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(unsupported_backend_call())
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(unsupported_backend_call())
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(unsupported_backend_call())
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(unsupported_backend_call())
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Err(unsupported_backend_call())
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(unsupported_backend_call())
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        let _ = name;
        Err(unsupported_backend_call())
    }

    fn append_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendAppend> {
        Err(unsupported_backend_call())
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(unsupported_backend_call())
    }

    fn conditional_create(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(unsupported_backend_call())
    }

    fn conditional_update(
        &self,
        _name: &ObjectName,
        _expected: &BackendFence,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(unsupported_backend_call())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        Err(PublishError::unsupported(name, unsupported_backend_call()))
    }
}

fn unsupported_backend_call() -> BackendError {
    BackendError::new(
        BackendErrorKind::UnsupportedOperation,
        "cache capability rejection must not call backend data methods",
    )
}
