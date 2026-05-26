//! Branch table rewrite scheduling.

use super::{
    telemetry_health_debt, LifecycleError, LifecycleLowerLayer, LifecycleResult, LifecycleStats,
    MaintenanceExecutorStatus, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskScope, RecoveryHealth,
};
use crate::branch::{
    BranchCompactionKind, BranchCompactionOutcome, BranchCompactionPlan, BranchCompactionRecovery,
    BranchCompactionRequest, BranchLevel, BranchLocalState, BranchMaterializationHandle,
    BranchMaterializationIntent, BranchMaterializationOutcome, BranchMaterializationRecovery,
    BranchMaterializationRequest, BranchRuntimeError,
};
use crate::object::ObjectName;
use strata_core_next::BranchId;

const LEVEL_ZERO_COMPACTION_THRESHOLD: usize = 2;
const LEVEL_ZERO_URGENT_COMPACTION_THRESHOLD: usize = 4;
const LEVEL_ZERO_BLOCKING_COMPACTION_THRESHOLD: usize = 8;
const FROZEN_BLOCKING_FLUSH_THRESHOLD: usize = 4;
const PENDING_MAINTENANCE_BLOCKING_THRESHOLD: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleTableRewriteDurability {
    VolatileOnly,
    CheckpointRequiredAfterRewrite,
    DurableTableManifestBacked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCompactionRequest {
    branch_id: BranchId,
    kind: BranchCompactionKind,
    output_identity_seed: String,
    durability: LifecycleTableRewriteDurability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCompactionOutcome {
    status: LifecycleCompactionStatus,
    branch_id: BranchId,
    plan: BranchCompactionPlan,
    branch_outcome: BranchCompactionOutcome,
    checkpoint_required: bool,
    recovery_health: Option<RecoveryHealth>,
    durable_output_objects: Vec<ObjectName>,
    retained_input_objects: Vec<String>,
    failure: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleCompactionStatus {
    Completed,
    CompletedCheckpointRequired,
    CompletedDurable,
    CompletedManifestDebt,
    DeferredNoCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleMaterializationRequest {
    child_branch_id: BranchId,
    layer_index: usize,
    handle: Option<BranchMaterializationHandle>,
    output_identity_prefix: String,
    durability: LifecycleTableRewriteDurability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleMaterializationOutcome {
    status: LifecycleMaterializationStatus,
    child_branch_id: BranchId,
    layer_index: usize,
    intent: Option<BranchMaterializationIntent>,
    branch_outcome: Option<BranchMaterializationOutcome>,
    checkpoint_required: bool,
    recovery_health: Option<RecoveryHealth>,
    durable_output_objects: Vec<ObjectName>,
    failure: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleMaterializationStatus {
    Completed,
    CompletedCheckpointRequired,
    CompletedDurable,
    CompletedManifestDebt,
    AlreadyMaterialized,
    DeferredNoLayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleStoragePressure {
    branch_id: BranchId,
    severity: LifecycleStoragePressureSeverity,
    reason: LifecycleStoragePressureReason,
    suggested_task: Option<MaintenanceTaskRequest>,
    active_rows: usize,
    frozen_tables: usize,
    level_zero_tables: usize,
    owned_tables: usize,
    inherited_layers: usize,
    pending_maintenance: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleStoragePressureSeverity {
    None,
    Background,
    Urgent,
    BlockMutatingAdmission,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleStoragePressureReason {
    None,
    FrozenBacklog,
    LevelZeroTableBacklog,
    InheritedLayerBacklog,
    MaintenanceQueueBacklog,
}

impl LifecycleCompactionRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        kind: BranchCompactionKind,
        output_identity_seed: impl Into<String>,
    ) -> LifecycleResult<Self> {
        let request = Self {
            branch_id,
            kind,
            output_identity_seed: output_identity_seed.into(),
            durability: LifecycleTableRewriteDurability::VolatileOnly,
        };
        request.branch_request()?;
        Ok(request)
    }

    pub(crate) const fn with_durability(
        mut self,
        durability: LifecycleTableRewriteDurability,
    ) -> Self {
        self.durability = durability;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn kind(&self) -> BranchCompactionKind {
        self.kind
    }

    pub(crate) fn output_identity_seed(&self) -> &str {
        &self.output_identity_seed
    }

    pub(crate) const fn durability(&self) -> LifecycleTableRewriteDurability {
        self.durability
    }

    pub(super) fn branch_request(&self) -> LifecycleResult<BranchCompactionRequest> {
        BranchCompactionRequest::new(self.branch_id, self.kind, self.output_identity_seed.clone())
            .map_err(branch_error)
    }
}

impl LifecycleCompactionOutcome {
    pub(super) fn new(
        request: &LifecycleCompactionRequest,
        plan: BranchCompactionPlan,
        branch_outcome: BranchCompactionOutcome,
    ) -> Self {
        let no_candidate = matches!(
            branch_outcome.recovery(),
            BranchCompactionRecovery::NoCandidate { .. }
        );
        let checkpoint_required = !no_candidate
            && request.durability()
                == LifecycleTableRewriteDurability::CheckpointRequiredAfterRewrite;
        let status = if no_candidate {
            LifecycleCompactionStatus::DeferredNoCandidate
        } else if checkpoint_required {
            LifecycleCompactionStatus::CompletedCheckpointRequired
        } else {
            LifecycleCompactionStatus::Completed
        };
        Self {
            status,
            branch_id: branch_outcome.branch_id(),
            plan,
            branch_outcome,
            checkpoint_required,
            recovery_health: None,
            durable_output_objects: Vec::new(),
            retained_input_objects: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn completed_durable(
        plan: BranchCompactionPlan,
        branch_outcome: BranchCompactionOutcome,
        durable_output_objects: Vec<ObjectName>,
        retained_input_objects: Vec<String>,
    ) -> Self {
        Self {
            status: LifecycleCompactionStatus::CompletedDurable,
            branch_id: branch_outcome.branch_id(),
            plan,
            branch_outcome,
            checkpoint_required: false,
            recovery_health: None,
            durable_output_objects,
            retained_input_objects,
            failure: None,
        }
    }

    pub(super) fn manifest_debt(mut self, failure: LifecycleError) -> Self {
        self.status = LifecycleCompactionStatus::CompletedManifestDebt;
        self.checkpoint_required = true;
        self.recovery_health = telemetry_health();
        self.failure = Some(failure);
        self
    }

    pub(crate) const fn status(&self) -> LifecycleCompactionStatus {
        self.status
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn plan(&self) -> &BranchCompactionPlan {
        &self.plan
    }

    pub(crate) const fn branch_outcome(&self) -> &BranchCompactionOutcome {
        &self.branch_outcome
    }

    pub(crate) const fn checkpoint_required(&self) -> bool {
        self.checkpoint_required
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) fn durable_output_objects(&self) -> &[ObjectName] {
        &self.durable_output_objects
    }

    pub(crate) const fn failure(&self) -> Option<&LifecycleError> {
        self.failure.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleCompactionStatus::Completed
            | LifecycleCompactionStatus::CompletedCheckpointRequired
            | LifecycleCompactionStatus::CompletedDurable
            | LifecycleCompactionStatus::CompletedManifestDebt => {
                MaintenanceOutcomeStatus::Completed
            }
            LifecycleCompactionStatus::DeferredNoCandidate => MaintenanceOutcomeStatus::Deferred,
        };
        // Durable rewrite outcomes carry ObjectNames for outputs and identity
        // strings for retained inputs. Volatile/legacy-durable outcomes only
        // have the branch refs, so fall back to table identities then. Either
        // path lists each logical object once.
        let affected_object_names: Vec<String> =
            if self.durable_output_objects.is_empty() && self.retained_input_objects.is_empty() {
                self.branch_outcome
                    .output_refs()
                    .iter()
                    .chain(self.branch_outcome.removed_refs())
                    .map(|table_ref| table_ref.table_identity().as_str().to_owned())
                    .collect()
            } else {
                self.durable_output_objects
                    .iter()
                    .map(|object| object.as_str().to_owned())
                    .chain(self.retained_input_objects.iter().cloned())
                    .collect()
            };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Compaction, status)
            .with_affected_object_names(affected_object_names)
            .with_state_changes(usize::from(!matches!(
                self.status,
                LifecycleCompactionStatus::DeferredNoCandidate
            )))
            .with_checkpoint_required(self.checkpoint_required)
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if matches!(self.status, LifecycleCompactionStatus::DeferredNoCandidate) {
            outcome = outcome.with_reason("compaction has no candidate tables");
        }
        if matches!(
            self.status,
            LifecycleCompactionStatus::CompletedManifestDebt
        ) {
            outcome = outcome.with_reason("table rewrite manifest publication needs recovery");
        }
        if let Some(health) = &self.recovery_health {
            outcome = outcome.with_recovery_health(health.clone());
        }
        if let Some(error) = &self.failure {
            outcome = outcome.with_source_error(error.clone());
        }
        outcome
    }
}

impl LifecycleMaterializationRequest {
    pub(crate) fn new(
        child_branch_id: BranchId,
        layer_index: usize,
        output_identity_prefix: impl Into<String>,
    ) -> LifecycleResult<Self> {
        let request = Self {
            child_branch_id,
            layer_index,
            handle: None,
            output_identity_prefix: output_identity_prefix.into(),
            durability: LifecycleTableRewriteDurability::VolatileOnly,
        };
        request.branch_request()?;
        Ok(request)
    }

    pub(crate) fn from_handle(
        handle: BranchMaterializationHandle,
        output_identity_prefix: impl Into<String>,
    ) -> LifecycleResult<Self> {
        let request = Self {
            child_branch_id: handle.child_branch_id(),
            layer_index: handle.layer_index(),
            handle: Some(handle),
            output_identity_prefix: output_identity_prefix.into(),
            durability: LifecycleTableRewriteDurability::VolatileOnly,
        };
        request.branch_request()?;
        Ok(request)
    }

    pub(crate) const fn with_durability(
        mut self,
        durability: LifecycleTableRewriteDurability,
    ) -> Self {
        self.durability = durability;
        self
    }

    pub(crate) const fn child_branch_id(&self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub(crate) const fn handle(&self) -> Option<BranchMaterializationHandle> {
        self.handle
    }

    pub(crate) fn output_identity_prefix(&self) -> &str {
        &self.output_identity_prefix
    }

    pub(crate) const fn durability(&self) -> LifecycleTableRewriteDurability {
        self.durability
    }

    pub(super) fn branch_request(&self) -> LifecycleResult<BranchMaterializationRequest> {
        match self.handle {
            Some(handle) => BranchMaterializationRequest::from_handle(
                handle,
                self.output_identity_prefix.clone(),
            ),
            None => BranchMaterializationRequest::new(
                self.child_branch_id,
                self.layer_index,
                self.output_identity_prefix.clone(),
            ),
        }
        .map_err(branch_error)
    }
}

impl LifecycleMaterializationOutcome {
    pub(super) fn deferred(request: &LifecycleMaterializationRequest) -> Self {
        Self {
            status: LifecycleMaterializationStatus::DeferredNoLayer,
            child_branch_id: request.child_branch_id(),
            layer_index: request.layer_index(),
            intent: None,
            branch_outcome: None,
            checkpoint_required: false,
            recovery_health: None,
            durable_output_objects: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn completed(
        request: &LifecycleMaterializationRequest,
        intent: BranchMaterializationIntent,
        branch_outcome: BranchMaterializationOutcome,
    ) -> Self {
        let checkpoint_required = request.durability()
            == LifecycleTableRewriteDurability::CheckpointRequiredAfterRewrite
            && !matches!(
                branch_outcome.recovery(),
                BranchMaterializationRecovery::LayerAlreadyMaterialized
            );
        let status = match branch_outcome.recovery() {
            BranchMaterializationRecovery::LayerAlreadyMaterialized => {
                LifecycleMaterializationStatus::AlreadyMaterialized
            }
            BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
            | BranchMaterializationRecovery::ReplacementAlreadyVisibleLayerRemoved => {
                if checkpoint_required {
                    LifecycleMaterializationStatus::CompletedCheckpointRequired
                } else {
                    LifecycleMaterializationStatus::Completed
                }
            }
        };
        Self {
            status,
            child_branch_id: branch_outcome.child_branch_id(),
            layer_index: branch_outcome.layer_index(),
            intent: Some(intent),
            branch_outcome: Some(branch_outcome),
            checkpoint_required,
            recovery_health: None,
            durable_output_objects: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn completed_durable(
        intent: BranchMaterializationIntent,
        branch_outcome: BranchMaterializationOutcome,
        durable_output_objects: Vec<ObjectName>,
    ) -> Self {
        Self {
            status: LifecycleMaterializationStatus::CompletedDurable,
            child_branch_id: branch_outcome.child_branch_id(),
            layer_index: branch_outcome.layer_index(),
            intent: Some(intent),
            branch_outcome: Some(branch_outcome),
            checkpoint_required: false,
            recovery_health: None,
            durable_output_objects,
            failure: None,
        }
    }

    pub(super) fn manifest_debt(mut self, failure: LifecycleError) -> Self {
        self.status = LifecycleMaterializationStatus::CompletedManifestDebt;
        self.checkpoint_required = true;
        self.recovery_health = telemetry_health();
        self.failure = Some(failure);
        self
    }

    pub(crate) const fn status(&self) -> LifecycleMaterializationStatus {
        self.status
    }

    pub(crate) const fn child_branch_id(&self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub(crate) const fn intent(&self) -> Option<&BranchMaterializationIntent> {
        self.intent.as_ref()
    }

    pub(crate) const fn branch_outcome(&self) -> Option<&BranchMaterializationOutcome> {
        self.branch_outcome.as_ref()
    }

    pub(crate) const fn checkpoint_required(&self) -> bool {
        self.checkpoint_required
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = match self.status {
            LifecycleMaterializationStatus::Completed
            | LifecycleMaterializationStatus::CompletedCheckpointRequired
            | LifecycleMaterializationStatus::CompletedDurable
            | LifecycleMaterializationStatus::CompletedManifestDebt
            | LifecycleMaterializationStatus::AlreadyMaterialized => {
                MaintenanceOutcomeStatus::Completed
            }
            LifecycleMaterializationStatus::DeferredNoLayer => MaintenanceOutcomeStatus::Deferred,
        };
        // Durable materialization carries ObjectNames for the published
        // replacements. Volatile materialization only knows the table
        // identities created by the branch runtime. Either path lists each
        // output once.
        let affected_object_names: Vec<String> = if self.durable_output_objects.is_empty() {
            self.branch_outcome
                .as_ref()
                .map_or_else(Vec::new, |outcome| {
                    outcome
                        .created_table_identities()
                        .iter()
                        .map(|identity| identity.as_str().to_owned())
                        .collect()
                })
        } else {
            self.durable_output_objects
                .iter()
                .map(|object| object.as_str().to_owned())
                .collect()
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Materialization, status)
            .with_affected_object_names(affected_object_names)
            .with_state_changes(usize::from(!matches!(
                self.status,
                LifecycleMaterializationStatus::DeferredNoLayer
            )))
            .with_checkpoint_required(self.checkpoint_required)
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if matches!(self.status, LifecycleMaterializationStatus::DeferredNoLayer) {
            outcome = outcome.with_reason("materialization has no inherited layer");
        }
        if matches!(
            self.status,
            LifecycleMaterializationStatus::CompletedManifestDebt
        ) {
            outcome = outcome.with_reason("table rewrite manifest publication needs recovery");
        }
        if let Some(health) = &self.recovery_health {
            outcome = outcome.with_recovery_health(health.clone());
        }
        if let Some(error) = &self.failure {
            outcome = outcome.with_source_error(error.clone());
        }
        outcome
    }
}

impl LifecycleStoragePressure {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn severity(self) -> LifecycleStoragePressureSeverity {
        self.severity
    }

    pub(crate) const fn reason(self) -> LifecycleStoragePressureReason {
        self.reason
    }

    pub(crate) const fn suggested_task(self) -> Option<MaintenanceTaskRequest> {
        self.suggested_task
    }

    pub(crate) const fn active_rows(self) -> usize {
        self.active_rows
    }

    pub(crate) const fn frozen_tables(self) -> usize {
        self.frozen_tables
    }

    pub(crate) const fn level_zero_tables(self) -> usize {
        self.level_zero_tables
    }

    pub(crate) const fn owned_tables(self) -> usize {
        self.owned_tables
    }

    pub(crate) const fn inherited_layers(self) -> usize {
        self.inherited_layers
    }

    pub(crate) const fn pending_maintenance(self) -> usize {
        self.pending_maintenance
    }
}

pub(crate) fn compact_cache_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleCompactionRequest,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    compact_branch(branch, request)
}

pub(crate) fn compact_durable_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleCompactionRequest,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let request = request
        .clone()
        .with_durability(LifecycleTableRewriteDurability::CheckpointRequiredAfterRewrite);
    compact_branch(branch, &request)
}

pub(crate) fn materialize_cache_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleMaterializationRequest,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    materialize_branch(branch, request)
}

pub(crate) fn materialize_durable_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleMaterializationRequest,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    let request = request
        .clone()
        .with_durability(LifecycleTableRewriteDurability::CheckpointRequiredAfterRewrite);
    materialize_branch(branch, &request)
}

pub(crate) fn compaction_request_from_maintenance_task(
    task: &MaintenanceTask,
) -> LifecycleResult<LifecycleCompactionRequest> {
    let MaintenanceTaskScope::TableLevel { branch_id, level } = task.scope() else {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "compaction task must target a table level",
        });
    };
    if task.kind() != MaintenanceTaskKind::Compaction {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task is not a compaction task",
        });
    }
    let kind = if level == 0 {
        BranchCompactionKind::CompactL0ToLevelOne
    } else {
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(level),
            table_index: 0,
        }
    };
    LifecycleCompactionRequest::new(
        branch_id,
        kind,
        format!(
            "maintenance-compaction-{}-{level}",
            branch_component(branch_id)
        ),
    )
}

pub(crate) fn materialization_request_from_maintenance_task(
    task: &MaintenanceTask,
) -> LifecycleResult<LifecycleMaterializationRequest> {
    let MaintenanceTaskScope::InheritedLayer {
        branch_id,
        layer_index,
    } = task.scope()
    else {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task must target an inherited layer",
        });
    };
    if task.kind() != MaintenanceTaskKind::Materialization {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task is not a materialization task",
        });
    }
    let prefix = format!(
        "maintenance-materialization-{}",
        branch_component(branch_id)
    );
    if let Some(handle) = task.materialization_handle() {
        LifecycleMaterializationRequest::from_handle(handle, prefix)
    } else {
        LifecycleMaterializationRequest::new(branch_id, layer_index, prefix)
    }
}

pub(crate) fn bind_materialization_task_for_enqueue(
    branch: &mut BranchLocalState,
    request: MaintenanceTaskRequest,
) -> LifecycleResult<MaintenanceTaskRequest> {
    if request.kind() != MaintenanceTaskKind::Materialization
        || request.materialization_handle().is_some()
    {
        return Ok(request);
    }
    let MaintenanceTaskScope::InheritedLayer {
        branch_id,
        layer_index,
    } = request.scope()
    else {
        return Ok(request);
    };
    if branch.branch_id() != branch_id {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task branch must match runtime branch",
        });
    }
    if branch.inherited_layers().get(layer_index).is_none() {
        return Ok(request);
    }
    let intent = branch
        .mark_inherited_layer_materializing(layer_index)
        .map_err(branch_error)?;
    request.with_materialization_handle(intent.handle())
}

pub(crate) fn collect_storage_pressure(
    branch: &BranchLocalState,
    maintenance: MaintenanceExecutorStatus,
) -> LifecycleStoragePressure {
    let branch_id = branch.branch_id();
    let active_rows = branch.active_row_count();
    let frozen_tables = branch.frozen_table_count();
    let level_zero_tables = branch
        .owned_levels()
        .get(usize::from(BranchLevel::ZERO.raw()))
        .map_or(0, std::vec::Vec::len);
    let owned_tables = branch.owned_table_count();
    let inherited_layers = branch.inherited_layer_count();
    let pending_maintenance = maintenance.pending_tasks();

    let (severity, reason, suggested_task) = if frozen_tables >= FROZEN_BLOCKING_FLUSH_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::FrozenBacklog,
            Some(MaintenanceTaskRequest::flush(branch_id)),
        )
    } else if level_zero_tables >= LEVEL_ZERO_BLOCKING_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if pending_maintenance >= PENDING_MAINTENANCE_BLOCKING_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::MaintenanceQueueBacklog,
            None,
        )
    } else if frozen_tables > 0 {
        (
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureReason::FrozenBacklog,
            Some(MaintenanceTaskRequest::flush(branch_id)),
        )
    } else if level_zero_tables >= LEVEL_ZERO_URGENT_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if level_zero_tables >= LEVEL_ZERO_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if inherited_layers > 0 {
        (
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureReason::InheritedLayerBacklog,
            Some(MaintenanceTaskRequest::materialization(branch_id)),
        )
    } else if pending_maintenance > 0 {
        (
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureReason::MaintenanceQueueBacklog,
            None,
        )
    } else {
        (
            LifecycleStoragePressureSeverity::None,
            LifecycleStoragePressureReason::None,
            None,
        )
    };

    LifecycleStoragePressure {
        branch_id,
        severity,
        reason,
        suggested_task,
        active_rows,
        frozen_tables,
        level_zero_tables,
        owned_tables,
        inherited_layers,
        pending_maintenance,
    }
}

fn compact_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleCompactionRequest,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let branch_request = request.branch_request()?;
    let plan = branch
        .plan_branch_compaction(&branch_request)
        .map_err(branch_error)?;
    let branch_outcome = branch
        .install_branch_compaction_plan(&branch_request, &plan)
        .map_err(branch_error)?;
    Ok(LifecycleCompactionOutcome::new(
        request,
        plan,
        branch_outcome,
    ))
}

fn materialize_branch(
    branch: &mut BranchLocalState,
    request: &LifecycleMaterializationRequest,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    if branch.branch_id() != request.child_branch_id() {
        return Err(branch_error(BranchRuntimeError::InvalidBranchState {
            reason: "materialization request branch id must match branch state",
        }));
    }
    if branch
        .inherited_layers()
        .get(request.layer_index())
        .is_none()
        && request.handle().is_none()
    {
        return Ok(LifecycleMaterializationOutcome::deferred(request));
    }
    let (intent, branch_request) = if let Some(handle) = request.handle() {
        if let Some(layer_index) = materialization_layer_index_for_handle(branch, handle) {
            let intent = branch
                .mark_inherited_layer_materializing(layer_index)
                .map_err(branch_error)?;
            if intent.handle().child_branch_id() != handle.child_branch_id()
                || intent.handle().source_branch_id() != handle.source_branch_id()
                || intent.handle().fork_version() != handle.fork_version()
            {
                return Err(branch_error(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "materialization handle must match target layer",
                }));
            }
            let branch_request = BranchMaterializationRequest::from_handle(
                intent.handle(),
                request.output_identity_prefix().to_owned(),
            )
            .map_err(branch_error)?;
            (intent, branch_request)
        } else {
            let snapshot = branch.reachability_snapshot().map_err(branch_error)?;
            (
                BranchMaterializationIntent::new(handle, snapshot),
                request.branch_request()?,
            )
        }
    } else {
        let intent = branch
            .mark_inherited_layer_materializing(request.layer_index())
            .map_err(branch_error)?;
        let branch_request = BranchMaterializationRequest::from_handle(
            intent.handle(),
            request.output_identity_prefix().to_owned(),
        )
        .map_err(branch_error)?;
        (intent, branch_request)
    };
    let branch_outcome = branch
        .materialize_inherited_layer(&branch_request)
        .map_err(branch_error)?;
    Ok(LifecycleMaterializationOutcome::completed(
        request,
        intent,
        branch_outcome,
    ))
}

fn materialization_layer_index_for_handle(
    branch: &BranchLocalState,
    handle: BranchMaterializationHandle,
) -> Option<usize> {
    branch.inherited_layers().iter().position(|layer| {
        layer.source_branch_id() == handle.source_branch_id()
            && layer.fork_version() == handle.fork_version()
    })
}

pub(super) fn branch_error(
    error: impl std::error::Error + Send + Sync + 'static,
) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "branch runtime failed",
        error,
    )
}

fn telemetry_health() -> Option<RecoveryHealth> {
    telemetry_health_debt("table rewrite publication needs recovery").ok()
}

fn branch_component(branch_id: BranchId) -> String {
    let mut component = String::with_capacity(BranchId::BYTE_LEN * 2);
    for byte in branch_id.as_bytes() {
        component.push(hex_digit(byte >> 4));
        component.push(hex_digit(byte & 0x0f));
    }
    component
}

const fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + nibble - 10) as char,
        _ => unreachable!(),
    }
}
