//! Model-backed API maintenance contracts.

use std::error::Error;
use std::fmt;

use strata_core::BranchId;

use crate::api::{
    CommitBatch, CommitMutation, CommitOptions, MaintenanceRequest, MaintenanceScope,
    MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask as ApiMaintenanceTask,
    MaintenanceWalGrowthStatus, StorageApiError, StorageApiErrorClass, StorageKey,
    StorageMaintenanceSchedulingPolicy, StorageOpenOptions, StorageRuntime, StorageSpaceId,
    StorageValue,
};
use crate::lifecycle::{
    LifecycleError, LifecycleLowerLayer, MaintenanceOutcome, MaintenanceOutcomeStatus,
    MaintenanceTaskKind,
};
use crate::testkit::TestkitError;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const ENGINE_STORAGE_SPACE: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiMaintenanceModelOutcome {
    scripts: usize,
    checkpoints: usize,
    flushes: usize,
    rewrites: usize,
    retention: usize,
    quarantine: usize,
    wal_growth: usize,
    queue_drains: usize,
    scope_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiMaintenanceFaultOutcome {
    snapshot_publish_failures: usize,
    manifest_publish_failures: usize,
    table_publish_failures: usize,
    compaction_failures: usize,
    materialization_failures: usize,
    retention_proof_failures: usize,
    quarantine_inventory_mismatches: usize,
    purge_publish_uncertainties: usize,
    repair_failures: usize,
}

const MAINTENANCE_FAULT_SNAPSHOT_PUBLISH: u8 = 0;
const MAINTENANCE_FAULT_MANIFEST_PUBLISH: u8 = 1;
const MAINTENANCE_FAULT_TABLE_PUBLISH: u8 = 2;
const MAINTENANCE_FAULT_COMPACTION: u8 = 3;
const MAINTENANCE_FAULT_MATERIALIZATION: u8 = 4;
const MAINTENANCE_FAULT_RETENTION_PROOF: u8 = 5;
const MAINTENANCE_FAULT_QUARANTINE_INVENTORY: u8 = 6;
const MAINTENANCE_FAULT_PURGE_PUBLISH: u8 = 7;
const MAINTENANCE_FAULT_REPAIR: u8 = 8;

pub fn check_storage_api_maintenance_model_contract(
    script: &[u8],
) -> Result<StorageApiMaintenanceModelOutcome, TestkitError> {
    let script = non_empty_script(script);
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .map_err(testkit_error)?
    .into_runtime();
    let mut outcome = StorageApiMaintenanceModelOutcome {
        scripts: 1,
        ..StorageApiMaintenanceModelOutcome::default()
    };

    check_checkpoint_route(&mut runtime, &mut outcome)?;
    check_flush_route(&mut runtime, script, &mut outcome)?;
    check_rewrite_routes(&mut runtime, &mut outcome)?;
    check_retention_routes(&mut runtime, &mut outcome)?;
    check_quarantine_routes(&mut runtime, &mut outcome)?;
    check_wal_growth_route(&mut runtime, &mut outcome)?;
    check_queue_route(&mut runtime, script, &mut outcome)?;
    check_scope_rejection(&mut runtime, &mut outcome)?;

    Ok(outcome)
}

pub fn check_storage_api_maintenance_fault_contract(
    script: &[u8],
) -> Result<StorageApiMaintenanceFaultOutcome, TestkitError> {
    let script = non_empty_script(script);
    let mut outcome = StorageApiMaintenanceFaultOutcome::default();
    match route_from_script(script) {
        MAINTENANCE_FAULT_SNAPSHOT_PUBLISH => {
            check_snapshot_publish_fault(&mut outcome)?;
        }
        MAINTENANCE_FAULT_MANIFEST_PUBLISH => {
            check_manifest_publish_fault(&mut outcome)?;
        }
        MAINTENANCE_FAULT_TABLE_PUBLISH => check_table_publish_fault(&mut outcome)?,
        MAINTENANCE_FAULT_COMPACTION => check_compaction_fault(&mut outcome)?,
        MAINTENANCE_FAULT_MATERIALIZATION => {
            check_materialization_fault(&mut outcome)?;
        }
        MAINTENANCE_FAULT_RETENTION_PROOF => check_retention_proof_fault(&mut outcome)?,
        MAINTENANCE_FAULT_QUARANTINE_INVENTORY => {
            check_quarantine_inventory_fault(&mut outcome)?;
        }
        MAINTENANCE_FAULT_PURGE_PUBLISH => check_purge_publish_fault(&mut outcome)?,
        MAINTENANCE_FAULT_REPAIR => check_repair_fault(&mut outcome)?,
        _ => unreachable!("maintenance fault route is normalized by route_from_script"),
    }
    Ok(outcome)
}

impl StorageApiMaintenanceModelOutcome {
    pub const fn scripts(self) -> usize {
        self.scripts
    }

    pub const fn checkpoints(self) -> usize {
        self.checkpoints
    }

    pub const fn flushes(self) -> usize {
        self.flushes
    }

    pub const fn rewrites(self) -> usize {
        self.rewrites
    }

    pub const fn retention(self) -> usize {
        self.retention
    }

    pub const fn quarantine(self) -> usize {
        self.quarantine
    }

    pub const fn wal_growth(self) -> usize {
        self.wal_growth
    }

    pub const fn queue_drains(self) -> usize {
        self.queue_drains
    }

    pub const fn scope_rejections(self) -> usize {
        self.scope_rejections
    }
}

impl StorageApiMaintenanceFaultOutcome {
    pub const fn snapshot_publish_failures(self) -> usize {
        self.snapshot_publish_failures
    }

    pub const fn manifest_publish_failures(self) -> usize {
        self.manifest_publish_failures
    }

    pub const fn table_publish_failures(self) -> usize {
        self.table_publish_failures
    }

    pub const fn compaction_failures(self) -> usize {
        self.compaction_failures
    }

    pub const fn materialization_failures(self) -> usize {
        self.materialization_failures
    }

    pub const fn retention_proof_failures(self) -> usize {
        self.retention_proof_failures
    }

    pub const fn quarantine_inventory_mismatches(self) -> usize {
        self.quarantine_inventory_mismatches
    }

    pub const fn purge_publish_uncertainties(self) -> usize {
        self.purge_publish_uncertainties
    }

    pub const fn repair_failures(self) -> usize {
        self.repair_failures
    }

    pub const fn total_routes(self) -> usize {
        self.snapshot_publish_failures
            + self.manifest_publish_failures
            + self.table_publish_failures
            + self.compaction_failures
            + self.materialization_failures
            + self.retention_proof_failures
            + self.quarantine_inventory_mismatches
            + self.purge_publish_uncertainties
            + self.repair_failures
    }
}

fn check_checkpoint_route(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    let checkpoint = runtime
        .maintenance(&MaintenanceRequest::new(
            ApiMaintenanceTask::Checkpoint,
            MaintenanceScope::Global,
        ))
        .map_err(testkit_error)?;
    require_summary(
        &checkpoint,
        ApiMaintenanceTask::Checkpoint,
        MaintenanceSummaryStatus::Deferred,
        "cache checkpoint did not defer",
    )?;
    outcome.checkpoints += 1;
    Ok(())
}

fn check_flush_route(
    runtime: &mut StorageRuntime<'_>,
    script: &[u8],
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    runtime
        .commit(&put_batch(b"flush", &[script_byte(script, 0)])?)
        .map_err(testkit_error)?;
    let flush = runtime
        .maintenance(&branch_request(ApiMaintenanceTask::Flush))
        .map_err(testkit_error)?;
    require_summary(
        &flush,
        ApiMaintenanceTask::Flush,
        MaintenanceSummaryStatus::Completed,
        "flush maintenance did not complete",
    )?;
    if flush.rows_processed() == 0 || flush.wal_truncated() {
        return Err(TestkitError::new(
            "flush summary lost row count or claimed log truncation",
        ));
    }
    outcome.flushes += 1;
    Ok(())
}

fn check_rewrite_routes(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    for task in [ApiMaintenanceTask::Compact, ApiMaintenanceTask::Materialize] {
        let summary = runtime
            .maintenance(&branch_request(task))
            .map_err(testkit_error)?;
        if summary.task() != task || summary.status() == MaintenanceSummaryStatus::Failed {
            return Err(TestkitError::new(
                "rewrite maintenance did not preserve status",
            ));
        }
        outcome.rewrites += 1;
    }
    Ok(())
}

fn check_retention_routes(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    for request in [
        MaintenanceRequest::new(ApiMaintenanceTask::Retain, MaintenanceScope::Global),
        branch_request(ApiMaintenanceTask::Reclaim),
        MaintenanceRequest::new(
            ApiMaintenanceTask::SnapshotPruning,
            MaintenanceScope::Global,
        ),
    ] {
        let summary = runtime.maintenance(&request).map_err(testkit_error)?;
        if summary.status() == MaintenanceSummaryStatus::Failed {
            return Err(TestkitError::new(
                "retention maintenance failed in cache mode",
            ));
        }
        outcome.retention += 1;
    }
    Ok(())
}

fn check_quarantine_routes(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    for request in [
        MaintenanceRequest::new(ApiMaintenanceTask::Quarantine, MaintenanceScope::Global),
        branch_request(ApiMaintenanceTask::Purge),
        MaintenanceRequest::new(ApiMaintenanceTask::Repair, MaintenanceScope::Global),
    ] {
        let summary = runtime.maintenance(&request).map_err(testkit_error)?;
        if summary.status() == MaintenanceSummaryStatus::Failed {
            return Err(TestkitError::new(
                "quarantine maintenance failed without a durable service",
            ));
        }
        outcome.quarantine += 1;
    }
    Ok(())
}

fn check_wal_growth_route(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    let summary = runtime
        .maintenance(&MaintenanceRequest::new(
            ApiMaintenanceTask::WalGrowth,
            MaintenanceScope::Global,
        ))
        .map_err(testkit_error)?;
    let growth = summary
        .wal_growth()
        .ok_or_else(|| TestkitError::new("WAL growth summary was absent"))?;
    if growth.status() != MaintenanceWalGrowthStatus::NoDurableAction {
        return Err(TestkitError::new("cache WAL growth made a durable claim"));
    }
    outcome.wal_growth += 1;
    Ok(())
}

fn check_queue_route(
    runtime: &mut StorageRuntime<'_>,
    script: &[u8],
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    runtime
        .commit(&put_batch(b"queue", &[script_byte(script, 1)])?)
        .map_err(testkit_error)?;
    runtime
        .enqueue_maintenance(&branch_request(ApiMaintenanceTask::Flush))
        .map_err(testkit_error)?;
    let drain = runtime.drain_maintenance().map_err(testkit_error)?;
    if drain.drained_tasks() != 1 || drain.queue().pending_tasks() != 0 {
        return Err(TestkitError::new("maintenance drain did not empty queue"));
    }
    outcome.queue_drains += 1;
    Ok(())
}

fn check_scope_rejection(
    runtime: &mut StorageRuntime<'_>,
    outcome: &mut StorageApiMaintenanceModelOutcome,
) -> Result<(), TestkitError> {
    let error = runtime
        .maintenance(&MaintenanceRequest::new(
            ApiMaintenanceTask::Flush,
            MaintenanceScope::Global,
        ))
        .expect_err("global flush scope should reject");
    if error.class() != StorageApiErrorClass::InvalidArgument {
        return Err(TestkitError::new("scope rejection used the wrong class"));
    }
    outcome.scope_rejections += 1;
    Ok(())
}

fn check_snapshot_publish_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "snapshot publish failed",
        FaultSource("snapshot service failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Checkpoint,
        MaintenanceTaskKind::Checkpoint,
        error,
        true,
    )?;
    outcome.snapshot_publish_failures += 1;
    Ok(())
}

fn check_manifest_publish_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::table_manifest_publication_failed_with(
        "manifest publish failed",
        FaultSource("manifest service failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Checkpoint,
        MaintenanceTaskKind::Checkpoint,
        error,
        true,
    )?;
    outcome.manifest_publish_failures += 1;
    Ok(())
}

fn check_table_publish_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::flush_publication_orphaned_with(
        Some("tables/fault".to_string()),
        "table publish failed",
        FaultSource("table service failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Flush,
        MaintenanceTaskKind::Flush,
        error,
        true,
    )?;
    outcome.table_publish_failures += 1;
    Ok(())
}

fn check_compaction_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::rewrite_publication_failed_with(
        "compaction publication failed",
        FaultSource("compaction service failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Compact,
        MaintenanceTaskKind::Compaction,
        error,
        true,
    )?;
    outcome.compaction_failures += 1;
    Ok(())
}

fn check_materialization_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::rewrite_publication_uncertain_with_objects(
        vec!["tables/materialized".to_string()],
        "materialization publication uncertain",
        FaultSource("materialization service failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Materialize,
        MaintenanceTaskKind::Materialization,
        error,
        true,
    )?;
    outcome.materialization_failures += 1;
    Ok(())
}

fn check_retention_proof_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "retention proof failed",
        FaultSource("retention proof failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Retain,
        MaintenanceTaskKind::Retention,
        error,
        false,
    )?;
    outcome.retention_proof_failures += 1;
    Ok(())
}

fn check_quarantine_inventory_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::quarantine_inventory_mismatch_with(
        "quarantine inventory mismatch",
        FaultSource("inventory mismatch"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Quarantine,
        MaintenanceTaskKind::Quarantine,
        error,
        false,
    )?;
    outcome.quarantine_inventory_mismatches += 1;
    Ok(())
}

fn check_purge_publish_fault(
    outcome: &mut StorageApiMaintenanceFaultOutcome,
) -> Result<(), TestkitError> {
    let error = LifecycleError::quarantine_publication_uncertain_with(
        "purge publication uncertain",
        FaultSource("purge publish uncertain"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Purge,
        MaintenanceTaskKind::Purge,
        error,
        true,
    )?;
    outcome.purge_publish_uncertainties += 1;
    Ok(())
}

fn check_repair_fault(outcome: &mut StorageApiMaintenanceFaultOutcome) -> Result<(), TestkitError> {
    let error = LifecycleError::quarantine_repair_inconclusive_with(
        "quarantine repair inconclusive",
        FaultSource("repair failed"),
    );
    check_fault_surface(
        ApiMaintenanceTask::Repair,
        MaintenanceTaskKind::Repair,
        error,
        true,
    )?;
    outcome.repair_failures += 1;
    Ok(())
}

fn check_fault_surface(
    api_task: ApiMaintenanceTask,
    lifecycle_kind: MaintenanceTaskKind,
    error: LifecycleError,
    retryable: bool,
) -> Result<(), TestkitError> {
    let mapped = crate::api::map_lifecycle_error_for_test(error.clone());
    require_source(&mapped)?;
    let summary = crate::api::map_maintenance_outcome_for_test(
        MaintenanceRequest::new(api_task, fault_scope(api_task)),
        &MaintenanceOutcome::new(lifecycle_kind, MaintenanceOutcomeStatus::Failed)
            .with_effects(1, 0, retryable)
            .with_reason("scripted maintenance failure")
            .with_source_error(error),
    );
    if summary.status() != MaintenanceSummaryStatus::Failed
        || summary.source_error_code().is_none()
        || summary.source_error_class().is_none()
        || summary.retryable() != retryable
    {
        return Err(TestkitError::new(
            "maintenance fault summary did not preserve source facts",
        ));
    }
    Ok(())
}

fn require_source(error: &StorageApiError) -> Result<(), TestkitError> {
    error
        .source()
        .ok_or_else(|| TestkitError::new("fault mapping lost source"))?;
    Ok(())
}

fn route_from_script(script: &[u8]) -> u8 {
    let label = String::from_utf8_lossy(script).to_ascii_lowercase();
    if label.contains("snapshot") {
        MAINTENANCE_FAULT_SNAPSHOT_PUBLISH
    } else if label.contains("manifest") {
        MAINTENANCE_FAULT_MANIFEST_PUBLISH
    } else if label.contains("table") {
        MAINTENANCE_FAULT_TABLE_PUBLISH
    } else if label.contains("compaction") {
        MAINTENANCE_FAULT_COMPACTION
    } else if label.contains("material") {
        MAINTENANCE_FAULT_MATERIALIZATION
    } else if label.contains("retention") {
        MAINTENANCE_FAULT_RETENTION_PROOF
    } else if label.contains("inventory") {
        MAINTENANCE_FAULT_QUARANTINE_INVENTORY
    } else if label.contains("purge") {
        MAINTENANCE_FAULT_PURGE_PUBLISH
    } else if label.contains("repair") {
        MAINTENANCE_FAULT_REPAIR
    } else {
        script[0] % 9
    }
}

fn require_summary(
    summary: &MaintenanceSummary,
    task: ApiMaintenanceTask,
    status: MaintenanceSummaryStatus,
    message: &'static str,
) -> Result<(), TestkitError> {
    if summary.task() != task || summary.status() != status {
        return Err(TestkitError::new(message));
    }
    Ok(())
}

fn branch_request(task: ApiMaintenanceTask) -> MaintenanceRequest {
    MaintenanceRequest::new(task, MaintenanceScope::Branch(DEFAULT_BRANCH_ID))
}

fn fault_scope(task: ApiMaintenanceTask) -> MaintenanceScope {
    match task {
        ApiMaintenanceTask::Checkpoint
        | ApiMaintenanceTask::Retain
        | ApiMaintenanceTask::SnapshotPruning
        | ApiMaintenanceTask::Quarantine
        | ApiMaintenanceTask::Repair
        | ApiMaintenanceTask::WalGrowth => MaintenanceScope::Global,
        ApiMaintenanceTask::Flush
        | ApiMaintenanceTask::Compact
        | ApiMaintenanceTask::Materialize
        | ApiMaintenanceTask::Reclaim
        | ApiMaintenanceTask::Purge => MaintenanceScope::Branch(DEFAULT_BRANCH_ID),
    }
}

fn put_batch(key: &[u8], value: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Put {
            storage_space: engine_space()?,
            key: api_key(key)?,
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .map_err(testkit_error)
}

fn engine_space() -> Result<StorageSpaceId, TestkitError> {
    StorageSpaceId::new(vec![ENGINE_STORAGE_SPACE]).map_err(testkit_error)
}

fn api_key(key: &[u8]) -> Result<StorageKey, TestkitError> {
    StorageKey::new(key.to_vec()).map_err(testkit_error)
}

fn non_empty_script(script: &[u8]) -> &[u8] {
    if script.is_empty() {
        &[0]
    } else {
        script
    }
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script[index % script.len()]
}

fn testkit_error(error: impl fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}

#[derive(Debug)]
struct FaultSource(&'static str);

impl fmt::Display for FaultSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for FaultSource {}
