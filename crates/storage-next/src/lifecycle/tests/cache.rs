use super::*;
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendFence,
    BackendMetadata, BackendRange, BackendResult, BackendWriterGuard, PublishError, PublishMode,
    PublishOutcome, PublishResult, CACHE_MODE_REQUIREMENTS,
};
use crate::branch::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation,
    CommitObservedVersion, CommitOrigin, CommitReadFact, CommitRetentionHint, CommitRuntimeConfig,
    CommitTimestampPolicy, CommitValidationFacts,
};
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageSpaceId};
use std::sync::atomic::{AtomicUsize, Ordering};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[test]
fn cache_open_builds_volatile_branch_commit_baseline_without_recovery_claims() {
    let branch = branch_id(0x44);
    let backend = MemoryBackend::new();
    let runtime = open_runtime(branch, &backend);

    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.open_plan().storage_mode(), StorageMode::Cache);
    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert_eq!(
        runtime.open_outcome().disposition(),
        StorageOpenDisposition::Created
    );
    assert_eq!(runtime.open_outcome().recovered_visible_version(), None);
    assert!(runtime.open_outcome().recovery_health().is_healthy());
    assert!(runtime.open_outcome().maintenance_ready());
    assert_eq!(
        runtime.open_outcome().backend_capabilities(),
        Some(backend.capabilities())
    );
    assert_eq!(runtime.open_outcome().stats().open_attempts(), 1);
    assert!(runtime.open_outcome().checkpoint().is_none());
    assert!(runtime.open_outcome().bootstrap().is_none());
    assert_eq!(
        runtime.capability_outcome().storage_mode(),
        StorageMode::Cache
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert_eq!(runtime.branch_state().branch_id(), branch);
    assert!(runtime.branch_state().is_empty());
    assert_eq!(
        runtime.unresolved_durable().expect("gate state"),
        None,
        "cache open starts with no unresolved durable gate fact"
    );
}

#[test]
fn cache_open_reports_maintenance_ready_after_executor_attached() {
    let branch = branch_id(0x4f);
    let backend = MemoryBackend::new();
    let runtime = open_runtime(branch, &backend);

    assert!(runtime.open_outcome().maintenance_ready());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(
        runtime.maintenance_status().stats(),
        LifecycleMaintenanceStats::default()
    );
}

#[test]
fn cache_runtime_can_enqueue_and_run_health_collection_maintenance() {
    let branch = branch_id(0x4e);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);

    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue health collection");
    assert_eq!(enqueue.status(), MaintenanceEnqueueStatus::Enqueued);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let mut runner = MaintenanceTestRunner;
    let outcome = runtime
        .run_next_maintenance(&mut runner)
        .expect("run maintenance")
        .expect("task outcome");

    assert_eq!(outcome.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(outcome.task_kind(), MaintenanceTaskKind::HealthCollection);
    assert!(outcome.task_id().is_some());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(runtime.maintenance_status().stats().completed(), 1);
}

#[test]
fn cache_open_rejects_non_cache_plan_before_backend_preflight() {
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    assert!(LifecycleCacheOpenRequest::new(
        open_plan(StorageMode::Cache),
        branch_id(0x45),
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .is_ok());
    for mode in [
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
        StorageMode::ObjectDurableCandidate,
    ] {
        assert_eq!(
            LifecycleCacheOpenRequest::new(
                open_plan(mode),
                branch_id(0x45),
                CommitBranchGeneration::new(1).expect("generation"),
            ),
            Err(LifecycleError::InvalidOpenPlan {
                reason: "cache lifecycle runtime requires cache storage mode",
            })
        );
    }
    assert!(CommitBranchGeneration::new(0).is_err());
    assert_eq!(backend.capability_calls(), 0);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_open_request_validation_rejects_invalid_plan_shapes() {
    assert_eq!(
        LifecycleConfig::new(
            0,
            1,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::Disabled,
        ),
        Err(LifecycleError::InvalidConfig {
            field: "max_maintenance_queue_depth",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        StorageOpenPlan::new(
            StorageMode::Cache,
            LifecycleCodecId::identity(),
            RecoveryStrictness::AllowExplicitLossyFallback,
            LifecycleConfig::new(
                1,
                1,
                LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
                LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
            )
            .expect("valid lossy-enabled config"),
        ),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "cache mode cannot request durable recovery fallback",
        })
    );
}

#[test]
fn cache_open_runs_capability_preflight_without_backend_side_effects() {
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let _runtime = open_runtime(branch_id(0x46), &backend);

    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(
        backend.other_calls(),
        0,
        "cache open must not read, list, write, publish, sync, or lock backend objects"
    );

    let rejected = CountingBackend::new(BackendCapabilities::empty());
    assert!(LifecycleCacheRuntime::open(
        request(branch_id(0x47)),
        &rejected,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .is_err());
    assert_eq!(rejected.capability_calls(), 1);
    assert_eq!(rejected.other_calls(), 0);
}

#[test]
fn cache_runtime_executes_cache_commit_and_reads_through_branch_state() {
    let branch = branch_id(0x48);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"alpha");

    let outcome = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"value".to_vec(),
                Timestamp::from_micros(1_234),
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("cache commit");

    assert_eq!(outcome.branch_id(), branch);
    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(1)));
    assert_eq!(
        outcome.durability(),
        crate::commit::CommitDurabilityClass::NotDurable
    );
    assert_eq!(outcome.mutation_counts().puts(), 1);
    assert_eq!(outcome.mutation_counts().timeline_rows(), 2);
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));
    assert_eq!(
        runtime.branch_state().max_commit_version(),
        Some(CommitVersion::new(1))
    );

    let read_view = runtime.read_view().expect("read view");
    let visible = read_view
        .latest(&key)
        .expect("latest read")
        .expect("visible row");
    assert_eq!(visible.row().value(), b"value");
    assert_eq!(
        visible.row().commit_timestamp(),
        Timestamp::from_micros(1_234)
    );
}

#[test]
fn cache_runtime_generated_timestamp_proves_zero_allocator_and_empty_timestamp_guard() {
    let branch = branch_id(0x4c);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime_with_timestamp(branch, &backend, Timestamp::from_micros(1));
    let key = physical_key(branch, b"generated-timestamp");

    let outcome = runtime
        .execute_cache_commit(
            runtime_generated_put_batch(branch, key.clone(), b"generated".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("runtime-generated cache commit");

    assert_eq!(
        outcome.commit_version(),
        Some(CommitVersion::new(1)),
        "first commit proves the version allocator opened at zero"
    );
    let visible = runtime
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("read")
        .expect("visible row");
    assert_eq!(
        visible.row().commit_timestamp(),
        Timestamp::from_micros(1),
        "first runtime-generated timestamp proves the timestamp guard opened empty"
    );
}

#[test]
fn cache_runtime_rejects_wrong_mode_batch_and_preserves_state() {
    let branch = branch_id(0x49);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"durable");

    let before_visible = runtime.visible_version();
    let before_rows = runtime.branch_state().active_row_count();
    for durability in [CommitDurabilityMode::Standard, CommitDurabilityMode::Always] {
        let error = runtime
            .execute_cache_commit(
                put_batch_with_durability(
                    branch,
                    key.clone(),
                    b"value".to_vec(),
                    Timestamp::from_micros(2_000),
                    durability,
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("durable batch rejected by cache runtime");

        assert_commit_runtime_error(&error);
        assert_eq!(runtime.visible_version(), before_visible);
        assert_eq!(runtime.branch_state().active_row_count(), before_rows);
    }

    let accepted = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-reject"),
                b"accepted".to_vec(),
                Timestamp::from_micros(2_001),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("valid cache commit after rejection");
    assert_eq!(accepted.commit_version(), Some(CommitVersion::new(1)));
}

#[test]
fn cache_runtime_rejects_read_only_wrong_branch_stale_generation_and_conflict() {
    let branch = branch_id(0x4a);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"guarded");

    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                read_only_batch(branch, key.clone()),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("read-only diagnostic rejected by mutating cache executor"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    let other_branch = branch_id(0x4b);
    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                put_batch(
                    other_branch,
                    physical_key(other_branch, b"wrong-branch"),
                    b"value".to_vec(),
                    Timestamp::from_micros(2_100),
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("wrong branch rejected"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    physical_key(branch, b"stale-generation"),
                    b"value".to_vec(),
                    Timestamp::from_micros(2_200),
                ),
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(2).expect("generation"),
                ),
            )
            .expect_err("stale generation rejected"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);

    let first = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"first".to_vec(),
                Timestamp::from_micros(2_300),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("first commit");
    assert_eq!(first.commit_version(), Some(CommitVersion::new(1)));

    assert_commit_runtime_error(
        &runtime
            .execute_cache_commit(
                put_batch_with_validation(
                    branch,
                    key.clone(),
                    b"conflict".to_vec(),
                    Timestamp::from_micros(2_400),
                    CommitValidationFacts::new(
                        vec![CommitReadFact::new(
                            key.clone(),
                            CommitObservedVersion::Missing,
                        )],
                        Vec::new(),
                    ),
                    crate::commit::CommitConflictValidationMode::Validate,
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("stale read fact rejected"),
    );
    assert_eq!(runtime.visible_version(), CommitVersion::new(1));

    let second = runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"after-conflict"),
                b"second".to_vec(),
                Timestamp::from_micros(2_500),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("second accepted commit");
    assert_eq!(second.commit_version(), Some(CommitVersion::new(2)));
}

#[test]
fn cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls() {
    let branch = branch_id(0x4a);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"before-close"),
                b"value".to_vec(),
                Timestamp::from_micros(2_900),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit before close");

    let close = runtime.close().expect("cache close");
    assert_eq!(close.phase(), ClosePhase::Closed);
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.close_fact(), Some(LifecycleCloseFact::Complete));
    assert!(close.commits_quiesced());
    assert!(close.maintenance_drained());
    assert!(!close.durable_synced());
    assert!(close.guards_released());
    assert!(!close.prior_final());
    assert_eq!(close.stats().close_attempts(), 1);
    assert_eq!(runtime.state(), LifecycleState::Closed);

    let second = runtime.close().expect("idempotent close");
    assert_eq!(second.phase(), ClosePhase::Closed);
    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(second.close_fact(), Some(LifecycleCloseFact::AlreadyClosed));
    assert!(second.prior_final());
    assert_eq!(runtime.state(), LifecycleState::Closed);

    assert!(matches!(
        runtime.read_view().expect_err("read after close rejected"),
        LifecycleError::InvalidLifecycleState { .. }
    ));
    let key = physical_key(branch, b"closed");
    assert!(matches!(
        runtime
            .execute_cache_commit(
                put_batch(
                    branch,
                    key,
                    b"value".to_vec(),
                    Timestamp::from_micros(3_000)
                ),
                CommitBranchGenerationGuard::not_supplied(),
            )
            .expect_err("commit after close rejected"),
        LifecycleError::InvalidLifecycleState { .. }
    ));
    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(
        backend.other_calls(),
        0,
        "cache commit and close must not touch durable backend methods"
    );
    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert!(runtime.open_outcome().recovery_health().is_healthy());
}

#[test]
fn cache_close_rejects_pending_drain_required_maintenance_before_transitioning() {
    let branch = branch_id(0x50);
    let backend = MemoryBackend::new();
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain-required health task"),
        )
        .expect("enqueue drain-required task");

    let error = runtime
        .close()
        .expect_err("drain-required task blocks close");

    assert_eq!(
        error,
        LifecycleError::MaintenanceTaskFailed {
            reason: "cache close cannot complete while drain-required maintenance is pending",
        }
    );
    assert_eq!(runtime.state(), LifecycleState::Open);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);

    let mut runner = MaintenanceTestRunner;
    runtime
        .run_next_maintenance(&mut runner)
        .expect("run drain-required task while open")
        .expect("task outcome");
    assert_eq!(
        runtime.close().expect("close after maintenance").status(),
        CloseOutcomeStatus::Complete
    );
}

#[test]
fn cache_close_without_commits_completes_and_preserves_diagnostic_facts() {
    let branch = branch_id(0x4d);
    let backend = CountingBackend::new(BackendCapabilities::from_slice(CACHE_MODE_REQUIREMENTS));
    let mut runtime = open_runtime(branch, &backend);

    let close = runtime.close().expect("cache close without commits");
    assert_eq!(close.phase(), ClosePhase::Closed);
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.open_plan().storage_mode(), StorageMode::Cache);
    assert_eq!(runtime.open_outcome().recovered_visible_version(), None);
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert_eq!(backend.capability_calls(), 1);
    assert_eq!(backend.other_calls(), 0);
}

#[test]
fn cache_reopen_starts_empty_even_when_prior_runtime_committed_rows() {
    let branch = branch_id(0x4b);
    let backend = MemoryBackend::new();
    let mut first = open_runtime(branch, &backend);
    let key = physical_key(branch, b"volatile");

    first
        .execute_cache_commit(
            put_batch(
                branch,
                key.clone(),
                b"ephemeral".to_vec(),
                Timestamp::from_micros(4_000),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit");
    assert!(first
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .is_some());
    first.close().expect("close first runtime");

    let second = open_runtime(branch, &backend);
    assert_eq!(second.visible_version(), CommitVersion::ZERO);
    assert!(second.branch_state().is_empty());
    assert!(second
        .read_view()
        .expect("second view")
        .latest(&key)
        .expect("read")
        .is_none());
    assert_eq!(second.open_outcome().recovered_visible_version(), None);
}

fn open_runtime(
    branch: BranchId,
    backend: &dyn Backend,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    open_runtime_with_timestamp(branch, backend, Timestamp::from_micros(1_000))
}

fn open_runtime_with_timestamp(
    branch: BranchId,
    backend: &dyn Backend,
    next_timestamp: Timestamp,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    LifecycleCacheRuntime::open(
        request(branch),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(next_timestamp),
    )
    .expect("cache runtime opens")
}

fn request(branch: BranchId) -> LifecycleCacheOpenRequest {
    LifecycleCacheOpenRequest::new(
        open_plan(StorageMode::Cache),
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .expect("cache open request")
}

fn open_plan(mode: StorageMode) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("open plan")
}

fn put_batch(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
) -> CommitBatch {
    put_batch_with_durability(branch, key, value, timestamp, CommitDurabilityMode::Cache)
}

fn put_batch_with_durability(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    durability: CommitDurabilityMode,
) -> CommitBatch {
    put_batch_with_options(
        branch,
        key,
        value,
        timestamp,
        durability,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(
                physical_key(branch, b"read-fact"),
                CommitObservedVersion::Missing,
            )],
            Vec::new(),
        ),
        crate::commit::CommitConflictValidationMode::Skip,
    )
}

fn put_batch_with_validation(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    validation: CommitValidationFacts,
    conflict_validation: crate::commit::CommitConflictValidationMode,
) -> CommitBatch {
    put_batch_with_options(
        branch,
        key,
        value,
        timestamp,
        CommitDurabilityMode::Cache,
        validation,
        conflict_validation,
    )
}

fn put_batch_with_options(
    branch: BranchId,
    key: PhysicalKey,
    value: Vec<u8>,
    timestamp: Timestamp,
    durability: CommitDurabilityMode,
    validation: CommitValidationFacts,
    conflict_validation: crate::commit::CommitConflictValidationMode,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        validation,
        CommitBatchOptions::new(
            durability,
            conflict_validation,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(timestamp),
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn read_only_batch(branch: BranchId, key: PhysicalKey) -> CommitBatch {
    CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Validate,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(2_050)),
            CommitOrigin::Diagnostic,
        ),
    )
}

fn runtime_generated_put_batch(branch: BranchId, key: PhysicalKey, value: Vec<u8>) -> CommitBatch {
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
            crate::commit::CommitConflictValidationMode::Skip,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

struct MaintenanceTestRunner;

impl MaintenanceTaskRunner for MaintenanceTestRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(MaintenanceOutcome::new(
            task.kind(),
            MaintenanceOutcomeStatus::Completed,
        ))
    }
}

fn assert_commit_runtime_error(error: &LifecycleError) {
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::CommitRuntime,
            ..
        }
    ));
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
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

struct CountingBackend {
    capabilities: BackendCapabilities,
    capability_calls: AtomicUsize,
    other_calls: AtomicUsize,
}

impl CountingBackend {
    fn new(capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            capability_calls: AtomicUsize::new(0),
            other_calls: AtomicUsize::new(0),
        }
    }

    fn capability_calls(&self) -> usize {
        self.capability_calls.load(Ordering::SeqCst)
    }

    fn other_calls(&self) -> usize {
        self.other_calls.load(Ordering::SeqCst)
    }

    fn record_other(&self) {
        self.other_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn unsupported(&self) -> BackendError {
        self.record_other();
        BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "unexpected backend call",
        )
    }
}

impl Backend for CountingBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capability_calls.fetch_add(1, Ordering::SeqCst);
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(self.unsupported())
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(self.unsupported())
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(self.unsupported())
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Err(self.unsupported())
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        self.record_other();
        Ok(BackendWriterGuard::new(name.clone(), ()))
    }

    fn append_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendAppend> {
        Err(self.unsupported())
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(self.unsupported())
    }

    fn conditional_create(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn conditional_update(
        &self,
        _name: &ObjectName,
        _expected: &BackendFence,
        _bytes: &[u8],
    ) -> BackendResult<BackendMetadata> {
        Err(self.unsupported())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        Err(PublishError::unsupported(name, self.unsupported()))
    }
}
