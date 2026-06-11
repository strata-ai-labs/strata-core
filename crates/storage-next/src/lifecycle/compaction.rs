//! Branch table rewrite scheduling.

use super::{
    telemetry_health_debt, LifecycleError, LifecycleLowerLayer, LifecycleResult, LifecycleStats,
    MaintenanceExecutorStatus, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskScope, RecoveryHealth,
};
use crate::branch::error::BranchRuntimeError;
use crate::branch::facts::{BranchLevel, BranchReachabilitySnapshot, BranchSourceLayout};
use crate::branch::pruning::BranchCompactionPruningProof;
use crate::branch::state::compaction::{
    BranchCompactionCandidate, BranchCompactionKind, BranchCompactionOperation,
    BranchCompactionOutcome, BranchCompactionPlan, BranchCompactionRequest,
    BranchCompactionRetentionPolicy,
};
use crate::branch::state::materialization::{
    BranchMaterializationHandle, BranchMaterializationOutcome, BranchMaterializationRecovery,
    BranchMaterializationRequest,
};
use crate::branch::state::BranchLocalState;
use crate::object::ObjectName;
use crate::table::TableIdentity;
use strata_core_next::BranchId;

const LEVEL_ZERO_COMPACTION_THRESHOLD: usize = 4;
const LEVEL_ZERO_URGENT_COMPACTION_THRESHOLD: usize = 8;
const LEVEL_ZERO_BLOCKING_COMPACTION_THRESHOLD: usize = 16;
const NONZERO_LEVEL_COMPACTION_THRESHOLD: usize = 4;
const NONZERO_LEVEL_URGENT_COMPACTION_THRESHOLD: usize = 8;
const NONZERO_LEVEL_BLOCKING_COMPACTION_THRESHOLD: usize = 16;
const NONZERO_LEVEL_TARGET_BYTES: u64 = 64 * 1024 * 1024;
const NONZERO_LEVEL_URGENT_BYTES: u64 = NONZERO_LEVEL_TARGET_BYTES * 2;
const NONZERO_LEVEL_BLOCKING_BYTES: u64 = NONZERO_LEVEL_TARGET_BYTES * 4;
const FROZEN_BLOCKING_FLUSH_THRESHOLD: usize = 4;
const PENDING_MAINTENANCE_BLOCKING_THRESHOLD: usize = 16;
const DEFAULT_COMPACTION_DRAIN_PASS_LIMIT: usize = 16;

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
    retention_policy: BranchCompactionRetentionPolicy,
    pruning_proof: Option<BranchCompactionPruningProof>,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCompactionDrainRequest {
    branch_id: BranchId,
    output_identity_prefix: String,
    max_passes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCompactionDrainOutcome {
    branch_id: BranchId,
    operations_attempted: usize,
    operations_installed: usize,
    table_rewrites: usize,
    metadata_promotions: usize,
    levels_touched: Vec<BranchLevel>,
    input_tables_removed: usize,
    output_tables_installed: usize,
    final_source_layout: BranchSourceLayout,
    affected_object_names: Vec<String>,
    checkpoint_required: bool,
    recovery_health: Option<RecoveryHealth>,
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
    materialization_handle: Option<BranchMaterializationHandle>,
    reachability_snapshot: Option<BranchReachabilitySnapshot>,
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
    NonZeroLevelTableBacklog,
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
            retention_policy: BranchCompactionRetentionPolicy::KeepAll,
            pruning_proof: None,
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

    #[allow(
        dead_code,
        reason = "row-pruning lifecycle request accessors are consumed by generated coverage"
    )]
    pub(crate) const fn retention_policy(&self) -> BranchCompactionRetentionPolicy {
        self.retention_policy
    }

    #[allow(
        dead_code,
        reason = "row-pruning lifecycle request accessors are consumed by generated coverage"
    )]
    pub(crate) const fn pruning_proof(&self) -> Option<BranchCompactionPruningProof> {
        self.pruning_proof
    }

    #[allow(
        dead_code,
        reason = "proof-backed lifecycle compaction is exercised by row-pruning callers"
    )]
    pub(crate) fn with_retention_policy(
        mut self,
        retention_policy: BranchCompactionRetentionPolicy,
    ) -> Self {
        self.retention_policy = retention_policy;
        self
    }

    #[allow(
        dead_code,
        reason = "proof-backed lifecycle compaction is exercised by row-pruning callers"
    )]
    pub(crate) fn with_pruning_proof(mut self, proof: BranchCompactionPruningProof) -> Self {
        self.pruning_proof = Some(proof);
        self
    }

    pub(super) fn branch_request(&self) -> LifecycleResult<BranchCompactionRequest> {
        let mut request = BranchCompactionRequest::new(
            self.branch_id,
            self.kind,
            self.output_identity_seed.clone(),
        )
        .map_err(branch_error)?
        .with_retention_policy(self.retention_policy);
        if let Some(proof) = self.pruning_proof {
            request = request.with_pruning_proof(proof);
        }
        Ok(request)
    }
}

impl LifecycleCompactionDrainRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        output_identity_prefix: impl Into<String>,
    ) -> LifecycleResult<Self> {
        let request = Self {
            branch_id,
            output_identity_prefix: output_identity_prefix.into(),
            max_passes: DEFAULT_COMPACTION_DRAIN_PASS_LIMIT,
        };
        TableIdentity::new(request.output_identity_prefix.clone()).map_err(|source| {
            LifecycleError::lower_layer_with(
                LifecycleLowerLayer::TableRuntime,
                "table runtime failed",
                source,
            )
        })?;
        request.compaction_request_for(0, 0, 0)?;
        Ok(request)
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn output_identity_prefix(&self) -> &str {
        &self.output_identity_prefix
    }

    pub(crate) const fn max_passes(&self) -> usize {
        self.max_passes
    }

    #[allow(
        dead_code,
        reason = "pass-limit behavior is exercised by targeted drain tests"
    )]
    pub(crate) const fn with_max_passes(mut self, max_passes: usize) -> Self {
        self.max_passes = max_passes;
        self
    }

    fn compaction_request_for(
        &self,
        pass_index: usize,
        level_index: usize,
        operation_index: usize,
    ) -> LifecycleResult<LifecycleCompactionRequest> {
        let kind = if level_index == 0 {
            BranchCompactionKind::CompactL0ToLevelOne
        } else {
            BranchCompactionKind::CompactLevel {
                level: BranchLevel::new(u8::try_from(level_index).map_err(|_| {
                    LifecycleError::MaintenanceTaskFailed {
                        reason: "compaction drain level must fit in BranchLevel",
                    }
                })?),
                table_index: 0,
            }
        };
        LifecycleCompactionRequest::new(
            self.branch_id,
            kind,
            format!(
                "{}-pass-{pass_index}-level-{level_index}-op-{operation_index}",
                self.output_identity_prefix
            ),
        )
    }
}

impl LifecycleCompactionDrainOutcome {
    fn new(branch_id: BranchId, final_source_layout: BranchSourceLayout) -> Self {
        Self {
            branch_id,
            operations_attempted: 0,
            operations_installed: 0,
            table_rewrites: 0,
            metadata_promotions: 0,
            levels_touched: Vec::new(),
            input_tables_removed: 0,
            output_tables_installed: 0,
            final_source_layout,
            affected_object_names: Vec::new(),
            checkpoint_required: false,
            recovery_health: None,
            failure: None,
        }
    }

    fn record_attempt(&mut self) {
        self.operations_attempted = self.operations_attempted.saturating_add(1);
    }

    fn record_install(&mut self, outcome: &LifecycleCompactionOutcome) {
        self.operations_installed = self.operations_installed.saturating_add(1);
        if let Some(candidate) = outcome.branch_outcome().candidate() {
            self.record_candidate(candidate);
        }
        self.input_tables_removed = self.input_tables_removed.saturating_add(
            outcome
                .branch_outcome()
                .candidate()
                .map_or(0, |candidate| candidate.input_refs().len()),
        );
        self.output_tables_installed = self
            .output_tables_installed
            .saturating_add(outcome.branch_outcome().output_refs().len());
        let maintenance = outcome.maintenance_outcome();
        for object_name in maintenance.affected_object_names() {
            if !self.affected_object_names.contains(object_name) {
                self.affected_object_names.push(object_name.clone());
            }
        }
        self.checkpoint_required |= maintenance.checkpoint_required();
        if self.recovery_health.is_none() {
            self.recovery_health = maintenance.recovery_health().cloned();
        }
        if self.failure.is_none() {
            self.failure = maintenance.source_error().cloned();
        }
    }

    fn record_candidate(&mut self, candidate: &BranchCompactionCandidate) {
        match candidate.operation() {
            BranchCompactionOperation::TableRewrite => {
                self.table_rewrites = self.table_rewrites.saturating_add(1);
            }
            BranchCompactionOperation::MetadataPromotion => {
                self.metadata_promotions = self.metadata_promotions.saturating_add(1);
            }
        }
        for table_ref in candidate
            .input_refs()
            .iter()
            .chain(candidate.overlap_refs().iter())
        {
            self.record_touched_level(table_ref.level());
        }
        self.record_touched_level(candidate.output_level());
    }

    fn record_touched_level(&mut self, level: BranchLevel) {
        if !self.levels_touched.contains(&level) {
            self.levels_touched.push(level);
        }
    }

    fn with_final_source_layout(mut self, final_source_layout: BranchSourceLayout) -> Self {
        self.final_source_layout = final_source_layout;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn operations_attempted(&self) -> usize {
        self.operations_attempted
    }

    pub(crate) const fn operations_installed(&self) -> usize {
        self.operations_installed
    }

    pub(crate) const fn table_rewrites(&self) -> usize {
        self.table_rewrites
    }

    pub(crate) const fn metadata_promotions(&self) -> usize {
        self.metadata_promotions
    }

    pub(crate) fn levels_touched(&self) -> &[BranchLevel] {
        &self.levels_touched
    }

    pub(crate) const fn input_tables_removed(&self) -> usize {
        self.input_tables_removed
    }

    pub(crate) const fn output_tables_installed(&self) -> usize {
        self.output_tables_installed
    }

    pub(crate) const fn final_source_layout(&self) -> &BranchSourceLayout {
        &self.final_source_layout
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = if self.operations_installed == 0 {
            MaintenanceOutcomeStatus::Deferred
        } else {
            MaintenanceOutcomeStatus::Completed
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Compaction, status)
            .with_affected_object_names(self.affected_object_names.clone())
            .with_state_changes(self.operations_installed)
            .with_checkpoint_required(self.checkpoint_required)
            .with_stats(LifecycleStats::new(0, 0, self.operations_attempted, 0, 0));
        if self.operations_installed == 0 {
            outcome = outcome.with_reason("compaction drain has no candidate tables");
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

impl LifecycleCompactionOutcome {
    pub(super) fn new(
        request: &LifecycleCompactionRequest,
        plan: BranchCompactionPlan,
        branch_outcome: BranchCompactionOutcome,
    ) -> Self {
        let no_candidate = branch_outcome.noop_reason().is_some();
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
            materialization_handle: None,
            reachability_snapshot: None,
            branch_outcome: None,
            checkpoint_required: false,
            recovery_health: None,
            durable_output_objects: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn completed(
        request: &LifecycleMaterializationRequest,
        materialization_handle: BranchMaterializationHandle,
        reachability_snapshot: BranchReachabilitySnapshot,
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
            materialization_handle: Some(materialization_handle),
            reachability_snapshot: Some(reachability_snapshot),
            branch_outcome: Some(branch_outcome),
            checkpoint_required,
            recovery_health: None,
            durable_output_objects: Vec::new(),
            failure: None,
        }
    }

    pub(super) fn completed_durable(
        materialization_handle: BranchMaterializationHandle,
        reachability_snapshot: BranchReachabilitySnapshot,
        branch_outcome: BranchMaterializationOutcome,
        durable_output_objects: Vec<ObjectName>,
    ) -> Self {
        Self {
            status: LifecycleMaterializationStatus::CompletedDurable,
            child_branch_id: branch_outcome.child_branch_id(),
            layer_index: branch_outcome.layer_index(),
            materialization_handle: Some(materialization_handle),
            reachability_snapshot: Some(reachability_snapshot),
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

    pub(crate) const fn materialization_handle(&self) -> Option<BranchMaterializationHandle> {
        self.materialization_handle
    }

    pub(crate) const fn reachability_snapshot(&self) -> Option<&BranchReachabilitySnapshot> {
        self.reachability_snapshot.as_ref()
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

pub(crate) fn compact_cache_branch_to_fixed_point(
    branch: &mut BranchLocalState,
    request: &LifecycleCompactionDrainRequest,
) -> LifecycleResult<LifecycleCompactionDrainOutcome> {
    compact_branch_to_fixed_point_with(branch, request, compact_branch)
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
    let (handle, _) = branch
        .mark_inherited_layer_materializing(layer_index)
        .map_err(branch_error)?;
    request.with_materialization_handle(handle)
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
    let nonzero_level_pressure = selected_nonzero_level_pressure(branch);
    let owned_tables = branch.owned_table_count();
    let inherited_layers = branch.inherited_layer_count();
    let pending_maintenance = maintenance.pending_tasks();

    let (severity, reason, suggested_task) = if frozen_tables >= FROZEN_BLOCKING_FLUSH_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::FrozenBacklog,
            Some(MaintenanceTaskRequest::flush(branch_id)),
        )
    } else if frozen_tables > 0 {
        (
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureReason::FrozenBacklog,
            Some(MaintenanceTaskRequest::flush(branch_id)),
        )
    } else if level_zero_tables >= LEVEL_ZERO_BLOCKING_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if nonzero_level_pressure.is_some_and(NonZeroLevelPressure::is_blocking) {
        let level = nonzero_level_pressure
            .expect("checked nonzero level pressure")
            .level;
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::NonZeroLevelTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, level)),
        )
    } else if pending_maintenance >= PENDING_MAINTENANCE_BLOCKING_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            LifecycleStoragePressureReason::MaintenanceQueueBacklog,
            None,
        )
    } else if level_zero_tables >= LEVEL_ZERO_URGENT_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if nonzero_level_pressure.is_some_and(NonZeroLevelPressure::is_urgent) {
        let level = nonzero_level_pressure
            .expect("checked nonzero level pressure")
            .level;
        (
            LifecycleStoragePressureSeverity::Urgent,
            LifecycleStoragePressureReason::NonZeroLevelTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, level)),
        )
    } else if level_zero_tables >= LEVEL_ZERO_COMPACTION_THRESHOLD {
        (
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureReason::LevelZeroTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, 0)),
        )
    } else if nonzero_level_pressure.is_some() {
        let level = nonzero_level_pressure
            .expect("checked nonzero level pressure")
            .level;
        (
            LifecycleStoragePressureSeverity::Background,
            LifecycleStoragePressureReason::NonZeroLevelTableBacklog,
            Some(MaintenanceTaskRequest::compaction(branch_id, level)),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NonZeroLevelPressure {
    level: u8,
    table_count: usize,
    byte_count: u64,
}

impl NonZeroLevelPressure {
    const fn is_background(self) -> bool {
        self.table_count >= NONZERO_LEVEL_COMPACTION_THRESHOLD
            || self.byte_count >= NONZERO_LEVEL_TARGET_BYTES
    }

    const fn is_urgent(self) -> bool {
        self.table_count >= NONZERO_LEVEL_URGENT_COMPACTION_THRESHOLD
            || self.byte_count >= NONZERO_LEVEL_URGENT_BYTES
    }

    const fn is_blocking(self) -> bool {
        self.table_count >= NONZERO_LEVEL_BLOCKING_COMPACTION_THRESHOLD
            || self.byte_count >= NONZERO_LEVEL_BLOCKING_BYTES
    }
}

fn selected_nonzero_level_pressure(branch: &BranchLocalState) -> Option<NonZeroLevelPressure> {
    let terminal_level_index = branch.owned_levels().len().saturating_sub(1);
    branch
        .owned_levels()
        .iter()
        .enumerate()
        .skip(usize::from(BranchLevel::ZERO.raw()) + 1)
        .filter_map(|(level_index, tables)| {
            if level_index >= terminal_level_index {
                return None;
            }
            let level = u8::try_from(level_index).ok()?;
            let byte_count = tables.iter().fold(0u64, |total, table| {
                total.saturating_add(table.facts().byte_count())
            });
            Some(NonZeroLevelPressure {
                level,
                table_count: tables.len(),
                byte_count,
            })
        })
        .filter(|pressure| pressure.is_background())
        .max_by_key(|pressure| {
            (
                pressure.is_blocking(),
                pressure.is_urgent(),
                pressure.table_count,
                pressure.byte_count,
                std::cmp::Reverse(pressure.level),
            )
        })
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

pub(crate) fn compact_branch_to_fixed_point_with<F>(
    branch: &mut BranchLocalState,
    request: &LifecycleCompactionDrainRequest,
    mut compact_once: F,
) -> LifecycleResult<LifecycleCompactionDrainOutcome>
where
    F: FnMut(
        &mut BranchLocalState,
        &LifecycleCompactionRequest,
    ) -> LifecycleResult<LifecycleCompactionOutcome>,
{
    if branch.branch_id() != request.branch_id() {
        return Err(branch_error(BranchRuntimeError::InvalidBranchState {
            reason: "compaction drain request branch id must match branch state",
        }));
    }
    let mut outcome =
        LifecycleCompactionDrainOutcome::new(branch.branch_id(), branch.source_layout());
    if branch_compaction_drain_is_stable(branch) {
        return Ok(outcome);
    }

    for pass_index in 0..request.max_passes() {
        let installed_before_pass = outcome.operations_installed();
        let compactable_level_count = branch.config().max_level_count().saturating_sub(1);
        for level_index in 0..compactable_level_count {
            while branch_owned_level_table_count(branch, level_index) > 0 {
                let input_tables_before = branch_owned_level_table_count(branch, level_index);
                let request_for_level = request.compaction_request_for(
                    pass_index,
                    level_index,
                    outcome.operations_attempted(),
                )?;
                outcome.record_attempt();
                let compaction = compact_once(branch, &request_for_level)?;
                if compaction.branch_outcome().noop_reason().is_some() {
                    break;
                }
                let input_tables_after = branch_owned_level_table_count(branch, level_index);
                if input_tables_after >= input_tables_before {
                    return Err(LifecycleError::MaintenanceTaskFailed {
                        reason: "compaction drain operation made no input-level progress",
                    });
                }
                outcome.record_install(&compaction);
                if compaction.failure().is_some() {
                    return Ok(outcome.with_final_source_layout(branch.source_layout()));
                }
            }
        }
        if branch_compaction_drain_is_stable(branch) {
            return Ok(outcome.with_final_source_layout(branch.source_layout()));
        }
        if outcome.operations_installed() == installed_before_pass {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "compaction drain found no progress with remaining compactable tables",
            });
        }
    }

    Err(LifecycleError::MaintenanceTaskFailed {
        reason: "compaction drain exceeded pass limit",
    })
}

fn branch_compaction_drain_is_stable(branch: &BranchLocalState) -> bool {
    branch
        .owned_levels()
        .iter()
        .take(branch.config().max_level_count().saturating_sub(1))
        .all(Vec::is_empty)
}

fn branch_owned_level_table_count(branch: &BranchLocalState, level_index: usize) -> usize {
    branch.owned_levels().get(level_index).map_or(0, Vec::len)
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
    let (materialization_handle, reachability_snapshot, branch_request) =
        if let Some(handle) = request.handle() {
            if let Some(layer_index) = materialization_layer_index_for_handle(branch, handle) {
                let (bound_handle, snapshot) = branch
                    .mark_inherited_layer_materializing(layer_index)
                    .map_err(branch_error)?;
                if bound_handle.child_branch_id() != handle.child_branch_id()
                    || bound_handle.source_branch_id() != handle.source_branch_id()
                    || bound_handle.fork_version() != handle.fork_version()
                {
                    return Err(branch_error(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "materialization handle must match target layer",
                    }));
                }
                let branch_request = BranchMaterializationRequest::from_handle(
                    bound_handle,
                    request.output_identity_prefix().to_owned(),
                )
                .map_err(branch_error)?;
                (bound_handle, snapshot, branch_request)
            } else {
                let snapshot = branch.reachability_snapshot().map_err(branch_error)?;
                (handle, snapshot, request.branch_request()?)
            }
        } else {
            let (handle, snapshot) = branch
                .mark_inherited_layer_materializing(request.layer_index())
                .map_err(branch_error)?;
            let branch_request = BranchMaterializationRequest::from_handle(
                handle,
                request.output_identity_prefix().to_owned(),
            )
            .map_err(branch_error)?;
            (handle, snapshot, branch_request)
        };
    let branch_outcome = branch
        .materialize_inherited_layer(&branch_request)
        .map_err(branch_error)?;
    Ok(LifecycleMaterializationOutcome::completed(
        request,
        materialization_handle,
        reachability_snapshot,
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
