//! Durable-local lifecycle service assembly.

use super::{
    validate_backend_capabilities_for_open, LifecycleCapabilityOutcome, LifecycleError,
    LifecycleLowerLayer, LifecycleOperationKind, LifecycleRecoveryOutcome, LifecycleResult,
    LifecycleState, LifecycleStateMachine, LifecycleTransitionTrigger, RecoveryHealth, StorageMode,
    StorageOpenDisposition, StorageOpenOutcome, StorageOpenPlan,
};
use crate::backend::{Backend, BackendError, BackendWriterGuard, PublishFailureKind};
use crate::branch::{BranchLocalState, BranchReadView, BranchRuntimeConfig};
use crate::commit::{
    CommitBatch, CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitBranchRegistry, CommitDurabilityClass, CommitDurableRuntime, CommitFactAllocator,
    CommitManualTimestampSource, CommitOutcome, CommitReplayAction, CommitReplayRequest,
    CommitReplayRuntime, CommitRuntimeConfig, CommitRuntimeError, CommitTimestampGuard,
    CommitTimestampSource, CommitUnresolvedDurable, CommitUnresolvedDurableGate,
    CommitVersionAllocator, VisibleVersionPublish, VisibleVersionTracker,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::{DatabaseManifest, WalRecord};
use crate::layout::{LayoutError, ObjectLayout};
use crate::object::ObjectName;
use crate::service::{
    CheckpointService, DatabaseManifestService, ManifestServiceError, QuarantineService,
    SnapshotService, TableManifestService, TableObjectReaderService, TableObjectService,
    WalSegmentMetadataSidecarService, WalService, WalServiceConfig, WalServiceError,
};
use std::fmt;
use strata_core_next::{BranchId, CommitVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleDurableLocalOpenRequest {
    plan: StorageOpenPlan,
    database_id: [u8; 16],
    initial_branch_id: BranchId,
    branch_generation: CommitBranchGeneration,
    branch_config: BranchRuntimeConfig,
    commit_config: CommitRuntimeConfig,
    wal_config: WalServiceConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleDurableAssemblyFacts {
    mode: StorageMode,
    disposition: StorageOpenDisposition,
    database_id: [u8; 16],
    codec_id: String,
    durability_policy: DurabilityPolicy,
    active_wal_segment: u64,
    writer_lock_object: ObjectName,
    manifest_snapshot_watermark: Option<u64>,
    manifest_snapshot_id: Option<u64>,
    manifest_flush_watermark: Option<CommitVersion>,
}

pub(crate) struct LifecycleDurableLocalServices<'a> {
    capability_outcome: LifecycleCapabilityOutcome,
    manifest: DatabaseManifestService<'a>,
    table_manifest: TableManifestService<'a>,
    wal: WalService<'a>,
    wal_sidecar: WalSegmentMetadataSidecarService<'a>,
    snapshot: SnapshotService<'a>,
    table_object: TableObjectService<'a>,
    table_reader: TableObjectReaderService<'a>,
    checkpoint: CheckpointService<'a>,
    quarantine: QuarantineService<'a>,
    assembly_facts: LifecycleDurableAssemblyFacts,
    writer_guard: BackendWriterGuard,
}

pub(crate) struct LifecycleDurableLocalShell<'a, S = CommitManualTimestampSource> {
    state: LifecycleStateMachine,
    open_plan: StorageOpenPlan,
    services: LifecycleDurableLocalServices<'a>,
    branch: BranchLocalState,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<S>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    commit_config: CommitRuntimeConfig,
}

#[derive(Debug)]
pub(crate) struct LifecycleDurableLocalRuntime<'a, S = CommitManualTimestampSource> {
    state: LifecycleStateMachine,
    open_plan: StorageOpenPlan,
    open_outcome: StorageOpenOutcome,
    bootstrap_report: LifecycleRecoveryBootstrapReport,
    services: LifecycleDurableLocalServices<'a>,
    branch: BranchLocalState,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<S>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    commit_config: CommitRuntimeConfig,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveryBootstrapReport {
    records_seen: usize,
    records_applied: usize,
    records_already_applied: usize,
    rows_checked: usize,
    rows_applied: usize,
    gates_cleared: usize,
    checkpoint_visible_publish: Option<VisibleVersionPublish>,
    recovered_visible_version: CommitVersion,
    recovery_health: RecoveryHealth,
}

impl LifecycleDurableLocalOpenRequest {
    pub(crate) fn new(
        plan: StorageOpenPlan,
        database_id: [u8; 16],
        initial_branch_id: BranchId,
        branch_generation: CommitBranchGeneration,
        branch_config: BranchRuntimeConfig,
        commit_config: CommitRuntimeConfig,
        wal_config: WalServiceConfig,
    ) -> LifecycleResult<Self> {
        let request = Self {
            plan,
            database_id,
            initial_branch_id,
            branch_generation,
            branch_config,
            commit_config,
            wal_config,
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn plan(&self) -> &StorageOpenPlan {
        &self.plan
    }

    pub(crate) const fn database_id(&self) -> [u8; 16] {
        self.database_id
    }

    pub(crate) const fn initial_branch_id(&self) -> BranchId {
        self.initial_branch_id
    }

    pub(crate) const fn branch_generation(&self) -> CommitBranchGeneration {
        self.branch_generation
    }

    pub(crate) const fn branch_config(&self) -> BranchRuntimeConfig {
        self.branch_config
    }

    pub(crate) const fn commit_config(&self) -> CommitRuntimeConfig {
        self.commit_config
    }

    pub(crate) const fn wal_config(&self) -> WalServiceConfig {
        self.wal_config
    }

    fn validate(&self) -> LifecycleResult<()> {
        self.plan.validate()?;
        durable_policy_for_mode(self.plan.storage_mode())?;
        if self.plan.codec_id().as_str() != self.wal_config.codec_id() {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "durable open plan codec must match WAL codec",
            });
        }
        self.branch_config.validate().map_err(branch_error)?;
        self.commit_config.validate().map_err(commit_error)?;
        self.wal_config.validate().map_err(wal_error)?;
        Ok(())
    }
}

impl LifecycleDurableAssemblyFacts {
    fn new(
        mode: StorageMode,
        disposition: StorageOpenDisposition,
        manifest: &DatabaseManifest,
        durability_policy: DurabilityPolicy,
        writer_lock_object: ObjectName,
    ) -> Self {
        Self {
            mode,
            disposition,
            database_id: *manifest.database_id(),
            codec_id: manifest.codec_id().to_owned(),
            durability_policy,
            active_wal_segment: manifest.active_wal_segment(),
            writer_lock_object,
            manifest_snapshot_watermark: manifest.snapshot_watermark(),
            manifest_snapshot_id: manifest.snapshot_id(),
            manifest_flush_watermark: manifest.flushed_through_commit_id(),
        }
    }

    pub(crate) const fn mode(&self) -> StorageMode {
        self.mode
    }

    pub(crate) const fn disposition(&self) -> StorageOpenDisposition {
        self.disposition
    }

    pub(crate) const fn database_id(&self) -> &[u8; 16] {
        &self.database_id
    }

    pub(crate) fn codec_id(&self) -> &str {
        &self.codec_id
    }

    pub(crate) const fn durability_policy(&self) -> DurabilityPolicy {
        self.durability_policy
    }

    pub(crate) const fn active_wal_segment(&self) -> u64 {
        self.active_wal_segment
    }

    pub(crate) const fn writer_lock_object(&self) -> &ObjectName {
        &self.writer_lock_object
    }

    pub(crate) const fn manifest_snapshot_watermark(&self) -> Option<u64> {
        self.manifest_snapshot_watermark
    }

    pub(crate) const fn manifest_snapshot_id(&self) -> Option<u64> {
        self.manifest_snapshot_id
    }

    pub(crate) const fn manifest_flush_watermark(&self) -> Option<CommitVersion> {
        self.manifest_flush_watermark
    }
}

impl<'a> LifecycleDurableLocalServices<'a> {
    pub(crate) const fn capability_outcome(&self) -> &LifecycleCapabilityOutcome {
        &self.capability_outcome
    }

    pub(crate) const fn assembly_facts(&self) -> &LifecycleDurableAssemblyFacts {
        &self.assembly_facts
    }

    pub(crate) const fn writer_guard(&self) -> &BackendWriterGuard {
        &self.writer_guard
    }

    pub(crate) const fn wal(&self) -> &WalService<'a> {
        &self.wal
    }

    pub(crate) fn wal_mut(&mut self) -> &mut WalService<'a> {
        &mut self.wal
    }

    pub(crate) const fn manifest(&self) -> &DatabaseManifestService<'a> {
        &self.manifest
    }

    pub(crate) const fn table_manifest(&self) -> &TableManifestService<'a> {
        &self.table_manifest
    }

    pub(crate) const fn wal_sidecar(&self) -> &WalSegmentMetadataSidecarService<'a> {
        &self.wal_sidecar
    }

    pub(crate) const fn snapshot(&self) -> &SnapshotService<'a> {
        &self.snapshot
    }

    pub(crate) const fn table_object(&self) -> &TableObjectService<'a> {
        &self.table_object
    }

    pub(crate) const fn table_reader(&self) -> &TableObjectReaderService<'a> {
        &self.table_reader
    }

    pub(crate) const fn checkpoint(&self) -> &CheckpointService<'a> {
        &self.checkpoint
    }

    pub(crate) const fn quarantine(&self) -> &QuarantineService<'a> {
        &self.quarantine
    }
}

impl<'a, S> LifecycleDurableLocalShell<'a, S> {
    pub(crate) fn assemble(
        request: LifecycleDurableLocalOpenRequest,
        backend: &'a dyn Backend,
        timestamp_source: S,
    ) -> LifecycleResult<Self> {
        request.validate()?;
        let mut state = LifecycleStateMachine::new();
        require_admitted(state, LifecycleOperationKind::Open)?;
        state.transition(LifecycleTransitionTrigger::OpenRequested)?;

        let capability_outcome = validate_backend_capabilities_for_open(request.plan(), backend)?;
        let durability_policy =
            capability_outcome
                .durability_policy()
                .ok_or(LifecycleError::InvalidOpenPlan {
                    reason: "durable local assembly requires durable policy",
                })?;

        let writer_lock_object = ObjectLayout::writer_lock().map_err(layout_error)?;
        let writer_guard = backend
            .acquire_writer_lock(&writer_lock_object)
            .map_err(backend_error)?;

        let manifest_service = DatabaseManifestService::new(backend);
        let (manifest, disposition) = load_or_create_manifest(&manifest_service, &request)?;
        validate_manifest_identity(&manifest, &request)?;

        let wal = WalService::open(
            backend,
            request.database_id(),
            manifest.active_wal_segment(),
            durability_policy,
            request.wal_config(),
        )
        .map_err(wal_error)?;

        let branch = BranchLocalState::new(request.initial_branch_id(), request.branch_config())
            .map_err(branch_error)?;
        let mut registry = CommitBranchRegistry::new();
        registry
            .register_active(request.initial_branch_id(), request.branch_generation())
            .map_err(commit_error)?;

        let assembly_facts = LifecycleDurableAssemblyFacts::new(
            request.plan().storage_mode(),
            disposition,
            &manifest,
            durability_policy,
            writer_lock_object,
        );

        let services = LifecycleDurableLocalServices {
            capability_outcome,
            manifest: manifest_service,
            table_manifest: TableManifestService::new(backend),
            wal,
            wal_sidecar: WalSegmentMetadataSidecarService::new(backend),
            snapshot: SnapshotService::new(backend),
            table_object: TableObjectService::new(backend),
            table_reader: TableObjectReaderService::new(backend),
            checkpoint: CheckpointService::new(backend),
            quarantine: QuarantineService::new(backend),
            assembly_facts,
            writer_guard,
        };

        state.transition(LifecycleTransitionTrigger::DurableRecoveryRequired)?;
        let commit_config = request.commit_config();

        Ok(Self {
            state,
            open_plan: request.plan,
            services,
            branch,
            registry,
            guard_set: CommitBranchGuardSet::new(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                timestamp_source,
            ),
            visible: VisibleVersionTracker::default(),
            durable_gate: CommitUnresolvedDurableGate::new(),
            commit_config,
        })
    }

    pub(crate) const fn state(&self) -> LifecycleState {
        self.state.state()
    }

    pub(crate) const fn open_plan(&self) -> &StorageOpenPlan {
        &self.open_plan
    }

    pub(crate) const fn services(&self) -> &LifecycleDurableLocalServices<'a> {
        &self.services
    }

    pub(crate) fn services_mut(&mut self) -> &mut LifecycleDurableLocalServices<'a> {
        &mut self.services
    }

    pub(crate) const fn assembly_facts(&self) -> &LifecycleDurableAssemblyFacts {
        self.services.assembly_facts()
    }

    pub(crate) const fn branch_state(&self) -> &BranchLocalState {
        &self.branch
    }

    pub(crate) fn branch_state_mut(&mut self) -> &mut BranchLocalState {
        &mut self.branch
    }

    pub(crate) const fn registry(&self) -> &CommitBranchRegistry {
        &self.registry
    }

    pub(crate) const fn guard_set(&self) -> &CommitBranchGuardSet {
        &self.guard_set
    }

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) const fn durable_gate(&self) -> &CommitUnresolvedDurableGate {
        &self.durable_gate
    }

    pub(crate) const fn commit_config(&self) -> CommitRuntimeConfig {
        self.commit_config
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    pub(crate) fn admit_recovery_step(&self) -> LifecycleResult<()> {
        require_admitted(self.state, LifecycleOperationKind::RecoveryStep)
    }

    pub(crate) fn admit_ordinary_read(&self) -> LifecycleResult<()> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)
    }

    pub(crate) fn admit_commit(&self) -> LifecycleResult<()> {
        require_admitted(self.state, LifecycleOperationKind::Commit)
    }

    pub(crate) fn admit_ordinary_maintenance(&self) -> LifecycleResult<()> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)
    }

    pub(crate) fn admit_health_query(&self) -> LifecycleResult<()> {
        require_admitted(self.state, LifecycleOperationKind::HealthQuery)
    }

    pub(crate) fn complete_recovery(
        mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleDurableLocalRuntime<'a, S>> {
        let report = self.bootstrap_commit_runtime(recovery)?;
        let open_outcome = StorageOpenOutcome::new(
            self.assembly_facts().mode(),
            self.assembly_facts().disposition(),
            Some(report.recovered_visible_version()),
            report.recovery_health().clone(),
            false,
        )?;
        self.state
            .transition(LifecycleTransitionTrigger::RecoveryAccepted)?;
        Ok(LifecycleDurableLocalRuntime {
            state: self.state,
            open_plan: self.open_plan,
            open_outcome,
            bootstrap_report: report,
            services: self.services,
            branch: self.branch,
            registry: self.registry,
            guard_set: self.guard_set,
            allocator: self.allocator,
            visible: self.visible,
            durable_gate: self.durable_gate,
            commit_config: self.commit_config,
        })
    }

    #[cfg(test)]
    pub(crate) fn try_bootstrap_commit_runtime_for_test(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        self.bootstrap_commit_runtime(recovery)
    }

    fn bootstrap_commit_runtime(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        require_admitted(self.state, LifecycleOperationKind::RecoveryStep)?;
        if matches!(recovery.health(), RecoveryHealth::Failed { .. }) {
            return Err(LifecycleError::RecoveryFailed {
                reason: "failed recovery package cannot be opened",
            });
        }
        let durability = commit_durability_class_for_mode(self.assembly_facts().mode())?;
        let checkpoint_watermark = recovery
            .checkpoint()
            .trusted_watermark()
            .unwrap_or(CommitVersion::ZERO);
        validate_recovered_wal_package(
            self.branch.branch_id(),
            checkpoint_watermark.max(recovery.wal().replay_start()),
            recovery.wal().records(),
        )?;

        let mut report = LifecycleRecoveryBootstrapReport::new(recovery.health().clone());
        let mut replayed_max = CommitVersion::ZERO;
        for record in recovery.wal().records() {
            replayed_max = replayed_max.max(record.commit_version());
            let replay = CommitReplayRequest::new(record.clone(), durability);
            let replay_report = CommitReplayRuntime::new(
                &self.commit_config,
                &mut self.allocator,
                &mut self.branch,
                &mut self.visible,
                &self.durable_gate,
            )
            .replay(&replay)
            .map_err(commit_error)?;
            report.record_replay(&replay_report);
        }

        let recovered_visible_version = checkpoint_watermark.max(replayed_max);
        let checkpoint_visible_publish =
            if recovered_visible_version > self.visible.visible_version() {
                Some(
                    self.visible
                        .catch_up_visible_after_replay(recovered_visible_version)
                        .map_err(commit_error)?,
                )
            } else {
                None
            };
        self.allocator
            .catch_up_to_recovered_version(recovered_visible_version);
        report.finish(checkpoint_visible_publish, recovered_visible_version);
        Ok(report)
    }
}

impl<S> LifecycleDurableLocalRuntime<'_, S> {
    pub(crate) const fn state(&self) -> LifecycleState {
        self.state.state()
    }

    pub(crate) const fn open_plan(&self) -> &StorageOpenPlan {
        &self.open_plan
    }

    pub(crate) const fn open_outcome(&self) -> &StorageOpenOutcome {
        &self.open_outcome
    }

    pub(crate) const fn bootstrap_report(&self) -> &LifecycleRecoveryBootstrapReport {
        &self.bootstrap_report
    }

    pub(crate) const fn services(&self) -> &LifecycleDurableLocalServices<'_> {
        &self.services
    }

    pub(crate) const fn branch_state(&self) -> &BranchLocalState {
        &self.branch
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
    }

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    pub(crate) fn read_view(&self) -> LifecycleResult<BranchReadView> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        self.branch.capture_read_view().map_err(branch_error)
    }
}

impl<S> LifecycleDurableLocalRuntime<'_, S>
where
    S: CommitTimestampSource,
{
    pub(crate) fn execute_durable_commit(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<CommitOutcome> {
        require_admitted(self.state, LifecycleOperationKind::Commit)?;
        CommitDurableRuntime::new(
            &self.commit_config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.branch,
            &mut self.visible,
            &mut self.services.wal,
            &self.durable_gate,
        )
        .execute(batch, generation_guard)
        .map_err(commit_error)
    }
}

impl LifecycleRecoveryBootstrapReport {
    const fn new(recovery_health: RecoveryHealth) -> Self {
        Self {
            records_seen: 0,
            records_applied: 0,
            records_already_applied: 0,
            rows_checked: 0,
            rows_applied: 0,
            gates_cleared: 0,
            checkpoint_visible_publish: None,
            recovered_visible_version: CommitVersion::ZERO,
            recovery_health,
        }
    }

    fn record_replay(&mut self, replay: &crate::commit::CommitReplayReport) {
        self.records_seen = self.records_seen.saturating_add(1);
        match replay.action() {
            CommitReplayAction::Applied => {
                self.records_applied = self.records_applied.saturating_add(1);
            }
            CommitReplayAction::AlreadyApplied => {
                self.records_already_applied = self.records_already_applied.saturating_add(1);
            }
        }
        self.rows_checked = self.rows_checked.saturating_add(replay.rows_checked());
        self.rows_applied = self.rows_applied.saturating_add(replay.rows_applied());
        if replay.gate_cleared() {
            self.gates_cleared = self.gates_cleared.saturating_add(1);
        }
    }

    fn finish(
        &mut self,
        checkpoint_visible_publish: Option<VisibleVersionPublish>,
        recovered_visible_version: CommitVersion,
    ) {
        self.checkpoint_visible_publish = checkpoint_visible_publish;
        self.recovered_visible_version = recovered_visible_version;
    }

    pub(crate) const fn records_seen(&self) -> usize {
        self.records_seen
    }

    pub(crate) const fn records_applied(&self) -> usize {
        self.records_applied
    }

    pub(crate) const fn records_already_applied(&self) -> usize {
        self.records_already_applied
    }

    pub(crate) const fn rows_checked(&self) -> usize {
        self.rows_checked
    }

    pub(crate) const fn rows_applied(&self) -> usize {
        self.rows_applied
    }

    pub(crate) const fn gates_cleared(&self) -> usize {
        self.gates_cleared
    }

    pub(crate) const fn checkpoint_visible_publish(&self) -> Option<VisibleVersionPublish> {
        self.checkpoint_visible_publish
    }

    pub(crate) const fn recovered_visible_version(&self) -> CommitVersion {
        self.recovered_visible_version
    }

    pub(crate) const fn recovery_health(&self) -> &RecoveryHealth {
        &self.recovery_health
    }
}

impl fmt::Debug for LifecycleDurableLocalServices<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LifecycleDurableLocalServices")
            .field("capability_outcome", &self.capability_outcome)
            .field("assembly_facts", &self.assembly_facts)
            .field("writer_guard", &self.writer_guard)
            .field("active_wal_segment", &self.wal.active_segment_id())
            .finish_non_exhaustive()
    }
}

impl<S> fmt::Debug for LifecycleDurableLocalShell<'_, S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LifecycleDurableLocalShell")
            .field("state", &self.state)
            .field("open_plan", &self.open_plan)
            .field("services", &self.services)
            .field("branch_id", &self.branch.branch_id())
            .field("visible_version", &self.visible.visible_version())
            .finish_non_exhaustive()
    }
}

fn load_or_create_manifest(
    service: &DatabaseManifestService<'_>,
    request: &LifecycleDurableLocalOpenRequest,
) -> LifecycleResult<(DatabaseManifest, StorageOpenDisposition)> {
    if let Some(manifest) = service.load_current().map_err(manifest_error)? {
        return Ok((manifest, StorageOpenDisposition::OpenedExisting));
    }

    match service.create_initial(request.database_id(), request.plan().codec_id().as_str()) {
        Ok(write) => Ok((write.manifest().clone(), StorageOpenDisposition::Created)),
        Err(ManifestServiceError::Publish { source, .. })
            if source.kind() == PublishFailureKind::PreconditionFailed =>
        {
            let manifest = service.load_required().map_err(manifest_error)?;
            Ok((manifest, StorageOpenDisposition::OpenedExisting))
        }
        Err(error) => Err(manifest_error(error)),
    }
}

fn validate_manifest_identity(
    manifest: &DatabaseManifest,
    request: &LifecycleDurableLocalOpenRequest,
) -> LifecycleResult<()> {
    if manifest.database_id() != &request.database_id() {
        return Err(LifecycleError::InvalidOpenPlan {
            reason: "database manifest id does not match durable open request",
        });
    }
    if manifest.codec_id() != request.plan().codec_id().as_str() {
        return Err(LifecycleError::InvalidOpenPlan {
            reason: "database manifest codec does not match durable open request",
        });
    }
    Ok(())
}

fn durable_policy_for_mode(mode: StorageMode) -> LifecycleResult<DurabilityPolicy> {
    match mode {
        StorageMode::DurableLocalStandard => Ok(DurabilityPolicy::Standard),
        StorageMode::DurableLocalAlways => Ok(DurabilityPolicy::Always),
        StorageMode::Cache | StorageMode::ObjectDurableCandidate => {
            Err(LifecycleError::InvalidOpenPlan {
                reason: "durable local assembly requires durable local storage mode",
            })
        }
    }
}

fn commit_durability_class_for_mode(mode: StorageMode) -> LifecycleResult<CommitDurabilityClass> {
    match mode {
        StorageMode::DurableLocalStandard => Ok(CommitDurabilityClass::Standard),
        StorageMode::DurableLocalAlways => Ok(CommitDurabilityClass::Always),
        StorageMode::Cache | StorageMode::ObjectDurableCandidate => {
            Err(LifecycleError::InvalidOpenPlan {
                reason: "commit recovery bootstrap requires durable local storage mode",
            })
        }
    }
}

fn validate_recovered_wal_package(
    branch_id: BranchId,
    replay_lower_bound: CommitVersion,
    records: &[WalRecord],
) -> LifecycleResult<()> {
    let mut previous = replay_lower_bound;
    for record in records {
        if record.branch_id() != branch_id {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovered WAL package contains an unopened branch",
            });
        }
        if record.commit_version() <= previous {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovered WAL package must be strictly ordered after replay start",
            });
        }
        previous = record.commit_version();
    }
    Ok(())
}

fn require_admitted(
    state: LifecycleStateMachine,
    operation: LifecycleOperationKind,
) -> LifecycleResult<()> {
    let admission = state.admit(operation);
    if admission.is_allowed() {
        Ok(())
    } else {
        Err(LifecycleError::InvalidLifecycleState {
            reason: admission
                .rejection_reason()
                .unwrap_or("operation is not admitted in current lifecycle state"),
        })
    }
}

fn backend_error(error: BackendError) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Backend, "backend failed", error)
}

fn layout_error(error: LayoutError) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Layout, "layout failed", error)
}

fn manifest_error(error: ManifestServiceError) -> LifecycleError {
    let reason = match &error {
        ManifestServiceError::Publish { source, .. } => match source.kind() {
            PublishFailureKind::VisibilityUnknown => "database manifest publish visibility unknown",
            PublishFailureKind::VisibleDurabilityUnconfirmed => {
                "database manifest publish durability unconfirmed"
            }
            PublishFailureKind::FailedBeforeVisibility => {
                "database manifest publish failed before visibility"
            }
            PublishFailureKind::PreconditionFailed => {
                "database manifest create precondition failed"
            }
            PublishFailureKind::Unsupported => "database manifest publish unsupported",
        },
        ManifestServiceError::Missing { .. } => "database manifest missing",
        ManifestServiceError::CodecMismatch { .. } => "database manifest codec mismatch",
        ManifestServiceError::Decode { .. } => "database manifest decode failed",
        ManifestServiceError::Read { .. } => "database manifest read failed",
        ManifestServiceError::Layout { .. } => "database manifest layout failed",
        ManifestServiceError::Encode { .. } => "database manifest encode failed",
        ManifestServiceError::InvalidPublishMetadata { .. } => {
            "database manifest publish metadata invalid"
        }
        ManifestServiceError::InvalidRecoveryFact { .. } => {
            "database manifest recovery fact invalid"
        }
    };
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, reason, error)
}

fn wal_error(error: WalServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, "WAL service failed", error)
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "branch runtime failed",
        error,
    )
}

fn commit_error(error: CommitRuntimeError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::CommitRuntime,
        "commit runtime failed",
        error,
    )
}
