//! Frozen-state flush orchestration.

use super::{
    require_generated_artifact_budget, require_table_reader_budget, telemetry_health_debt,
    LifecycleError, LifecycleLowerLayer, LifecycleResult, LifecycleStats, MaintenanceOutcome,
    MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskKind, MaintenanceTaskScope,
    RecoveryHealth, StorageBudgetLedger,
};
use crate::backend::{PublishError, PublishFailureKind};
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::BranchOwnedTable;
use crate::branch::state::{
    frozen_rows_match_table, BranchImmutableInstallOutcome, BranchLocalState,
};
use crate::object::ObjectName;
use crate::service::{
    TableObjectFacts, TableObjectReadError, TableObjectReaderService, TableObjectService,
    TableObjectServiceError,
};
use crate::table::{
    FrozenTable, ImmutableTableBuilder, ImmutableTableReader, TableIdentity, TableReaderConfig,
    TableRuntimeFacts, TableSummaryExtras,
};
use strata_core_next::BranchId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FlushFrozenRequest {
    branch_id: BranchId,
    frozen_index: Option<usize>,
    table_identity_seed: FlushTableIdentitySeed,
    table_object_id: FlushTableObjectId,
    target_level: BranchLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FlushTableIdentitySeed(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FlushTableObjectId(String);

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlushFrozenOutcome {
    branch_id: BranchId,
    frozen_index: Option<usize>,
    rows_flushed: u64,
    table_identity: Option<TableIdentity>,
    table_facts: Option<TableRuntimeFacts>,
    table_object: Option<ObjectName>,
    object_facts: Option<TableObjectFacts>,
    install_outcome: Option<BranchImmutableInstallOutcome>,
    failure: Option<LifecycleError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FlushDrainRequest {
    branch_id: BranchId,
    table_identity_seed: FlushTableIdentitySeed,
    table_object_id: FlushTableObjectId,
    freeze_during_drain_retry_limit: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlushDrainOutcome {
    branch_id: BranchId,
    frozen_tables_discovered: usize,
    completed_flushes: usize,
    deferred_flushes: usize,
    failed_flushes: usize,
    skipped_flushes: usize,
    freeze_during_drain_retries: usize,
    post_drain_frozen_tables: usize,
    affected_objects: usize,
    affected_object_names: Vec<String>,
    bytes_reclaimed: u64,
    retryable: bool,
    state_changes: usize,
    source_error: Option<LifecycleError>,
    recovery_health: Option<RecoveryHealth>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedCacheFlush {
    request: FlushFrozenRequest,
    frozen_index: usize,
    table_facts: TableRuntimeFacts,
    table: BranchOwnedTable,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedDurableFlush {
    request: FlushFrozenRequest,
    frozen_index: usize,
    /// `Arc` identity of the sealed memtable this build consumed (BS5.3b):
    /// the install matches by identity in O(1); row equality against this
    /// same sealed input is verified here in the (off-lock) prepare phase.
    frozen_identity: usize,
    table_facts: TableRuntimeFacts,
    object_facts: TableObjectFacts,
    table: Result<BranchOwnedTable, FlushFrozenOutcome>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedDurableFlushDrain {
    request: FlushDrainRequest,
    prepared_flushes: Vec<PreparedDurableFlush>,
}

const DEFAULT_FLUSH_DRAIN_FREEZE_RETRY_LIMIT: usize = 4;
const MEMORY_RELEASE_REEVALUATION_RETAINED_BYTES: u64 = 512 * 1024 * 1024;

impl FlushFrozenRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        frozen_index: Option<usize>,
        table_identity_seed: FlushTableIdentitySeed,
        table_object_id: FlushTableObjectId,
    ) -> LifecycleResult<Self> {
        Self::new_for_level(
            branch_id,
            frozen_index,
            table_identity_seed,
            table_object_id,
            BranchLevel::ZERO,
        )
    }

    pub(crate) fn new_for_level(
        branch_id: BranchId,
        frozen_index: Option<usize>,
        table_identity_seed: FlushTableIdentitySeed,
        table_object_id: FlushTableObjectId,
        target_level: BranchLevel,
    ) -> LifecycleResult<Self> {
        if target_level != BranchLevel::ZERO {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush target level must be zero",
            });
        }
        Ok(Self {
            branch_id,
            frozen_index,
            table_identity_seed,
            table_object_id,
            target_level,
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn frozen_index(&self) -> Option<usize> {
        self.frozen_index
    }

    pub(crate) fn table_identity_seed(&self) -> &FlushTableIdentitySeed {
        &self.table_identity_seed
    }

    pub(crate) fn table_object_id(&self) -> &FlushTableObjectId {
        &self.table_object_id
    }

    pub(crate) const fn target_level(&self) -> BranchLevel {
        self.target_level
    }
}

impl FlushDrainRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        table_identity_seed: FlushTableIdentitySeed,
        table_object_id: FlushTableObjectId,
    ) -> Self {
        Self {
            branch_id,
            table_identity_seed,
            table_object_id,
            freeze_during_drain_retry_limit: DEFAULT_FLUSH_DRAIN_FREEZE_RETRY_LIMIT,
        }
    }

    #[cfg(test)]
    pub(crate) const fn with_freeze_during_drain_retry_limit(mut self, limit: usize) -> Self {
        self.freeze_during_drain_retry_limit = limit;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    const fn freeze_during_drain_retry_limit(&self) -> usize {
        self.freeze_during_drain_retry_limit
    }

    pub(crate) fn flush_request(
        &self,
        operation_index: usize,
    ) -> LifecycleResult<FlushFrozenRequest> {
        FlushFrozenRequest::new(
            self.branch_id,
            None,
            FlushTableIdentitySeed::new(format!(
                "{}-drain-{operation_index}",
                self.table_identity_seed.as_str()
            ))?,
            FlushTableObjectId::new(format!(
                "{}-drain-{operation_index}",
                self.table_object_id.as_str()
            ))?,
        )
    }
}

impl FlushTableIdentitySeed {
    pub(crate) fn new(value: impl Into<String>) -> LifecycleResult<Self> {
        let value = value.into();
        validate_single_component("table identity seed", &value)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FlushTableObjectId {
    pub(crate) fn new(value: impl Into<String>) -> LifecycleResult<Self> {
        let value = value.into();
        validate_single_component("table object id", &value)?;
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FlushFrozenOutcome {
    pub(crate) fn deferred(request: &FlushFrozenRequest) -> Self {
        Self {
            branch_id: request.branch_id(),
            frozen_index: None,
            rows_flushed: 0,
            table_identity: None,
            table_facts: None,
            table_object: None,
            object_facts: None,
            install_outcome: None,
            failure: None,
        }
    }

    fn completed_outcome(
        request: &FlushFrozenRequest,
        frozen_index: usize,
        table_facts: TableRuntimeFacts,
        object_facts: Option<TableObjectFacts>,
        install_outcome: BranchImmutableInstallOutcome,
    ) -> Self {
        let table_identity = table_facts.identity().clone();
        let table_object = object_facts.as_ref().map(|facts| facts.object().clone());
        Self {
            branch_id: request.branch_id(),
            frozen_index: Some(frozen_index),
            rows_flushed: table_facts.row_count(),
            table_identity: Some(table_identity),
            table_facts: Some(table_facts),
            table_object,
            object_facts,
            install_outcome: Some(install_outcome),
            failure: None,
        }
    }

    fn published_not_installed_outcome(
        request: &FlushFrozenRequest,
        frozen_index: usize,
        table_facts: TableRuntimeFacts,
        object_facts: TableObjectFacts,
        failure: LifecycleError,
    ) -> Self {
        let table_identity = table_facts.identity().clone();
        let table_object = object_facts.object().clone();
        let failure = LifecycleError::flush_publication_orphaned_with(
            Some(table_object.as_str().to_owned()),
            "flush published table object before install failed",
            failure,
        );
        Self {
            branch_id: request.branch_id(),
            frozen_index: Some(frozen_index),
            rows_flushed: table_facts.row_count(),
            table_identity: Some(table_identity),
            table_facts: Some(table_facts),
            table_object: Some(table_object),
            object_facts: Some(object_facts),
            install_outcome: None,
            failure: Some(failure),
        }
    }

    fn failed(
        request: &FlushFrozenRequest,
        frozen_index: Option<usize>,
        failure: LifecycleError,
    ) -> Self {
        Self {
            branch_id: request.branch_id(),
            frozen_index,
            rows_flushed: 0,
            table_identity: None,
            table_facts: None,
            table_object: None,
            object_facts: None,
            install_outcome: None,
            failure: Some(failure),
        }
    }

    pub(crate) const fn completed(&self) -> bool {
        self.install_outcome.is_some()
    }

    pub(crate) fn deferred_no_frozen_state(&self) -> bool {
        self.install_outcome.is_none()
            && self.failure.is_none()
            && self.table_identity.is_none()
            && self.table_object.is_none()
    }

    pub(crate) fn failed_before_publication(&self) -> bool {
        self.failure.is_some() && self.table_object.is_none()
    }

    pub(crate) fn published_not_installed(&self) -> bool {
        self.failure.is_some() && self.table_object.is_some() && self.install_outcome.is_none()
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn frozen_index(&self) -> Option<usize> {
        self.frozen_index
    }

    pub(crate) const fn rows_flushed(&self) -> u64 {
        self.rows_flushed
    }

    pub(crate) fn table_identity(&self) -> Option<&TableIdentity> {
        self.table_identity.as_ref()
    }

    pub(crate) fn table_facts(&self) -> Option<&TableRuntimeFacts> {
        self.table_facts.as_ref()
    }

    pub(crate) fn table_object(&self) -> Option<&ObjectName> {
        self.table_object.as_ref()
    }

    pub(crate) fn object_facts(&self) -> Option<&TableObjectFacts> {
        self.object_facts.as_ref()
    }

    pub(crate) const fn install_outcome(&self) -> Option<BranchImmutableInstallOutcome> {
        self.install_outcome
    }

    pub(crate) fn failure(&self) -> Option<&LifecycleError> {
        self.failure.as_ref()
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = if self.completed() {
            MaintenanceOutcomeStatus::Completed
        } else if self.deferred_no_frozen_state() {
            MaintenanceOutcomeStatus::Deferred
        } else {
            MaintenanceOutcomeStatus::Failed
        };
        let affected_objects = usize::from(self.table_object.is_some());
        let retryable = self.published_not_installed()
            && self
                .failure
                .as_ref()
                .is_some_and(published_not_installed_retryable);
        let bytes_reclaimed = if self.completed() {
            self.table_facts
                .as_ref()
                .map_or(0, TableRuntimeFacts::byte_count)
        } else {
            0
        };
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Flush, status)
            .with_effects(affected_objects, bytes_reclaimed, retryable)
            .with_state_changes(usize::from(self.install_outcome.is_some()))
            .with_stats(LifecycleStats::new(0, 0, 1, 0, 0));
        if let Some(object) = &self.table_object {
            outcome = outcome.with_affected_object_names(vec![object.as_str().to_owned()]);
        }
        if self.deferred_no_frozen_state() {
            outcome = outcome.with_reason("flush has no frozen state to publish");
        }
        if self.published_not_installed() {
            outcome = outcome.with_reason("flush published table object before install failed");
        }
        if self.failed_before_publication() {
            outcome = outcome.with_reason("flush failed before table object publication");
        }
        if let Some(error) = &self.failure {
            outcome = outcome.with_source_error(error.clone());
        }
        outcome
    }
}

impl FlushDrainOutcome {
    fn new(branch_id: BranchId, frozen_tables_discovered: usize) -> Self {
        Self {
            branch_id,
            frozen_tables_discovered,
            completed_flushes: 0,
            deferred_flushes: 0,
            failed_flushes: 0,
            skipped_flushes: 0,
            freeze_during_drain_retries: 0,
            post_drain_frozen_tables: 0,
            affected_objects: 0,
            affected_object_names: Vec::new(),
            bytes_reclaimed: 0,
            retryable: false,
            state_changes: 0,
            source_error: None,
            recovery_health: None,
        }
    }

    fn skipped(mut self, post_drain_frozen_tables: usize) -> Self {
        self.skipped_flushes = 1;
        self.post_drain_frozen_tables = post_drain_frozen_tables;
        self
    }

    fn with_post_drain_frozen_tables(mut self, post_drain_frozen_tables: usize) -> Self {
        self.post_drain_frozen_tables = post_drain_frozen_tables;
        if post_drain_frozen_tables > 0 && self.failed_flushes == 0 {
            self.deferred_flushes = self.deferred_flushes.saturating_add(1);
        }
        self
    }

    fn with_freeze_during_drain_retries(mut self, retries: usize) -> Self {
        self.freeze_during_drain_retries = retries;
        self
    }

    fn record_maintenance_outcome(&mut self, outcome: &MaintenanceOutcome) -> bool {
        match outcome.status() {
            MaintenanceOutcomeStatus::Completed => {
                self.completed_flushes = self.completed_flushes.saturating_add(1);
            }
            MaintenanceOutcomeStatus::Deferred | MaintenanceOutcomeStatus::Canceled => {
                self.deferred_flushes = self.deferred_flushes.saturating_add(1);
            }
            MaintenanceOutcomeStatus::Failed => {
                self.failed_flushes = self.failed_flushes.saturating_add(1);
            }
        }
        self.affected_objects = self
            .affected_objects
            .saturating_add(outcome.affected_objects());
        self.affected_object_names
            .extend(outcome.affected_object_names().iter().cloned());
        self.bytes_reclaimed = self
            .bytes_reclaimed
            .saturating_add(outcome.bytes_reclaimed());
        self.retryable |= outcome.retryable();
        self.state_changes = self.state_changes.saturating_add(outcome.state_changes());
        if self.source_error.is_none() {
            self.source_error = outcome.source_error().cloned();
        }
        if self.recovery_health.is_none() {
            self.recovery_health = outcome.recovery_health().cloned();
        }
        matches!(outcome.status(), MaintenanceOutcomeStatus::Completed)
            && outcome.source_error().is_none()
            && outcome.recovery_health().is_none()
    }

    fn record_error(&mut self, error: LifecycleError) {
        self.failed_flushes = self.failed_flushes.saturating_add(1);
        self.retryable = true;
        self.source_error.get_or_insert(error);
    }

    fn status(&self) -> MaintenanceOutcomeStatus {
        if self.failed_flushes > 0 {
            MaintenanceOutcomeStatus::Failed
        } else if self.completed_flushes > 0 && self.post_drain_frozen_tables == 0 {
            MaintenanceOutcomeStatus::Completed
        } else {
            MaintenanceOutcomeStatus::Deferred
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn frozen_tables_discovered(&self) -> usize {
        self.frozen_tables_discovered
    }

    pub(crate) const fn completed_flushes(&self) -> usize {
        self.completed_flushes
    }

    pub(crate) const fn deferred_flushes(&self) -> usize {
        self.deferred_flushes
    }

    pub(crate) const fn failed_flushes(&self) -> usize {
        self.failed_flushes
    }

    pub(crate) const fn skipped_flushes(&self) -> usize {
        self.skipped_flushes
    }

    pub(crate) const fn freeze_during_drain_retries(&self) -> usize {
        self.freeze_during_drain_retries
    }

    pub(crate) const fn post_drain_frozen_tables(&self) -> usize {
        self.post_drain_frozen_tables
    }

    pub(crate) fn maintenance_outcome(&self) -> MaintenanceOutcome {
        let status = self.status();
        let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Flush, status)
            .with_effects(self.affected_objects, self.bytes_reclaimed, self.retryable)
            .with_state_changes(self.state_changes)
            .with_stats(LifecycleStats::new(
                0,
                0,
                self.completed_flushes
                    .saturating_add(self.deferred_flushes)
                    .saturating_add(self.failed_flushes)
                    .saturating_add(self.skipped_flushes),
                0,
                0,
            ));
        if !self.affected_object_names.is_empty() {
            outcome = outcome.with_affected_object_names(self.affected_object_names.clone());
        }
        if self.skipped_flushes > 0 {
            outcome = outcome.with_reason("flush drain has no frozen state to publish");
        } else if self.post_drain_frozen_tables > 0 {
            outcome = outcome.with_reason("flush drain left deferred frozen state");
        } else if self.failed_flushes > 0 {
            outcome = outcome.with_reason("flush drain failed before all frozen state was drained");
        }
        if let Some(error) = &self.source_error {
            outcome = outcome.with_source_error(error.clone());
        }
        if let Some(health) = self.recovery_health.clone() {
            outcome = outcome.with_recovery_health(health);
        } else if (self.completed_flushes > 0 && status != MaintenanceOutcomeStatus::Completed)
            || self.post_drain_frozen_tables > 0
        {
            if let Ok(health) = telemetry_health_debt("flush drain made partial progress") {
                outcome = outcome.with_recovery_health(health);
            }
        }
        outcome
    }
}

pub(crate) fn flush_cache_branch(
    branch: &mut BranchLocalState,
    request: &FlushFrozenRequest,
) -> LifecycleResult<FlushFrozenOutcome> {
    flush_cache_branch_with_budget(branch, request, None)
}

pub(crate) fn flush_cache_branch_with_budget(
    branch: &mut BranchLocalState,
    request: &FlushFrozenRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<FlushFrozenOutcome> {
    let Some(frozen_index) = select_frozen_index(branch, request)? else {
        return Ok(FlushFrozenOutcome::deferred(request));
    };
    let artifact = build_frozen_artifact(branch, request, frozen_index)?;
    require_optional_generated_artifact_budget(
        budget,
        artifact.byte_count(),
        "flush artifact exceeds generated artifact budget",
    )?;
    require_optional_table_reader_budget(
        budget,
        artifact.byte_count(),
        "flush table reader exceeds storage budget",
    )?;
    let identity = artifact.facts().identity().clone();
    let table_facts = artifact.facts().clone();
    let extras = artifact.extras().clone();
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default().with_eager_filter_unavailable(),
    )
    .map_err(table_error)?;
    let table = branch_owned_table(branch.branch_id(), identity, reader, extras)?;
    let install_outcome = match branch.replace_frozen_with_level_zero_table(frozen_index, table) {
        Ok(outcome) => outcome,
        Err(error) => {
            return Ok(FlushFrozenOutcome::failed(
                request,
                Some(frozen_index),
                branch_error(error),
            ));
        }
    };
    Ok(FlushFrozenOutcome::completed_outcome(
        request,
        frozen_index,
        table_facts,
        None,
        install_outcome,
    ))
}

pub(crate) fn prepare_cache_flush_with_budget(
    branch: &BranchLocalState,
    request: &FlushFrozenRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<Option<PreparedCacheFlush>> {
    let Some(frozen_index) = select_frozen_index(branch, request)? else {
        return Ok(None);
    };
    let artifact = build_frozen_artifact(branch, request, frozen_index)?;
    require_optional_generated_artifact_budget(
        budget,
        artifact.byte_count(),
        "flush artifact exceeds generated artifact budget",
    )?;
    require_optional_table_reader_budget(
        budget,
        artifact.byte_count(),
        "flush table reader exceeds storage budget",
    )?;
    let identity = artifact.facts().identity().clone();
    let table_facts = artifact.facts().clone();
    let extras = artifact.extras().clone();
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default().with_eager_filter_unavailable(),
    )
    .map_err(table_error)?;
    let table = branch_owned_table(branch.branch_id(), identity, reader, extras)?;
    Ok(Some(PreparedCacheFlush {
        request: request.clone(),
        frozen_index,
        table_facts,
        table,
    }))
}

pub(crate) fn install_prepared_cache_flush(
    branch: &mut BranchLocalState,
    prepared: PreparedCacheFlush,
) -> FlushFrozenOutcome {
    let PreparedCacheFlush {
        request,
        frozen_index,
        table_facts,
        table,
    } = prepared;
    let install_outcome = match branch.replace_frozen_with_level_zero_table(frozen_index, table) {
        Ok(outcome) => outcome,
        Err(error) => {
            return FlushFrozenOutcome::failed(&request, Some(frozen_index), branch_error(error));
        }
    };
    FlushFrozenOutcome::completed_outcome(
        &request,
        frozen_index,
        table_facts,
        None,
        install_outcome,
    )
}

pub(crate) fn flush_durable_branch(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'static>,
    request: &FlushFrozenRequest,
) -> LifecycleResult<FlushFrozenOutcome> {
    flush_durable_branch_with_budget(branch, table_service, reader_service, request, None)
}

pub(crate) fn flush_durable_branch_with_budget(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'static>,
    request: &FlushFrozenRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<FlushFrozenOutcome> {
    let Some(prepared) =
        prepare_durable_flush_with_budget(branch, table_service, reader_service, request, budget)?
    else {
        return Ok(FlushFrozenOutcome::deferred(request));
    };
    Ok(install_prepared_durable_flush(branch, prepared))
}

pub(crate) fn prepare_durable_flush_with_budget(
    branch: &BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'static>,
    request: &FlushFrozenRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<Option<PreparedDurableFlush>> {
    let Some(frozen_index) = select_frozen_index(branch, request)? else {
        return Ok(None);
    };
    let artifact = build_frozen_artifact(branch, request, frozen_index)?;
    require_optional_generated_artifact_budget(
        budget,
        artifact.byte_count(),
        "flush artifact exceeds generated artifact budget",
    )?;
    // BS4.5a: the installed durable table holds a lazy, disk-resident reader — charge only its
    // metadata-resident footprint (index + properties + filter frame), not the full encoded object.
    // The generated-artifact pool above still accounts the transient in-memory artifact at full size.
    // (Cache-mode flush keeps charging full bytes: those tables install eager, genuinely-resident
    // readers — constraint C2.)
    require_optional_table_reader_budget(
        budget,
        artifact.resident_metadata_bytes(),
        "flush table reader exceeds storage budget",
    )?;
    let identity = artifact.facts().identity().clone();
    let table_facts = artifact.facts().clone();
    let branch_component = request.branch_id().to_string();
    let object_id = derived_object_id(request, &table_facts);
    let object_facts = publish_or_load_existing(
        table_service,
        &branch_component,
        request.target_level().raw().into(),
        &object_id,
        artifact.bytes(),
        &table_facts,
    )?;
    let frozen =
        branch
            .frozen()
            .get(frozen_index)
            .ok_or(LifecycleError::MaintenanceTaskFailed {
                reason: "flush frozen index must exist",
            })?;
    let frozen_identity = frozen.memory_state_identity();
    let reader = match reader_service.open_reader(
        identity.clone(),
        &object_facts,
        TableReaderConfig::default()
            .with_eager_filter_unavailable()
            .deny_runtime_materialization(),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            let outcome = FlushFrozenOutcome::published_not_installed_outcome(
                request,
                frozen_index,
                table_facts.clone(),
                object_facts.clone(),
                table_read_error(error),
            );
            return Ok(Some(PreparedDurableFlush {
                request: request.clone(),
                frozen_index,
                frozen_identity,
                table_facts,
                object_facts,
                table: Err(outcome),
            }));
        }
    };
    let extras = artifact.extras().clone();
    let table = match branch_owned_table(branch.branch_id(), identity, reader, extras) {
        // BS5.3b: the row-equality verification moved OFF-lock from the
        // install (where it walked every row under the runtime lock, ~7.5 ms
        // per flush): verify the built, published table's rows end to end
        // through the reader against the sealed memtable the build consumed.
        // The install then matches that memtable by O(1) identity.
        Ok(table) => {
            if frozen_rows_match_table(&table, frozen) {
                Ok(table)
            } else {
                Err(FlushFrozenOutcome::published_not_installed_outcome(
                    request,
                    frozen_index,
                    table_facts.clone(),
                    object_facts.clone(),
                    LifecycleError::MaintenanceTaskFailed {
                        reason: "flush artifact rows do not match the frozen table",
                    },
                ))
            }
        }
        Err(error) => Err(FlushFrozenOutcome::published_not_installed_outcome(
            request,
            frozen_index,
            table_facts.clone(),
            object_facts.clone(),
            error,
        )),
    };
    Ok(Some(PreparedDurableFlush {
        request: request.clone(),
        frozen_index,
        frozen_identity,
        table_facts,
        object_facts,
        table,
    }))
}

pub(crate) fn install_prepared_durable_flush(
    branch: &mut BranchLocalState,
    prepared: PreparedDurableFlush,
) -> FlushFrozenOutcome {
    let PreparedDurableFlush {
        request,
        frozen_index,
        frozen_identity,
        table_facts,
        object_facts,
        table,
    } = prepared;
    let table = match table {
        Ok(table) => table,
        Err(outcome) => return outcome,
    };
    let install_outcome =
        match branch.replace_frozen_with_level_zero_table_by_identity(frozen_identity, table) {
            Ok(outcome) => outcome,
            Err(error) => {
                return FlushFrozenOutcome::published_not_installed_outcome(
                    &request,
                    frozen_index,
                    table_facts,
                    object_facts,
                    branch_error(error),
                );
            }
        };
    FlushFrozenOutcome::completed_outcome(
        &request,
        frozen_index,
        table_facts,
        Some(object_facts),
        install_outcome,
    )
}

pub(crate) fn prepare_durable_flush_drain_with_budget(
    branch: &BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'static>,
    request: &FlushDrainRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<PreparedDurableFlushDrain> {
    if branch.branch_id() != request.branch_id() {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush drain branch id must match branch state",
        });
    }
    let mut branch_snapshot = branch.clone();
    let frozen_tables_discovered = branch_snapshot.frozen_table_count();
    let operation_limit =
        frozen_tables_discovered.saturating_add(request.freeze_during_drain_retry_limit());
    let mut prepared_flushes = Vec::new();
    let mut simulated_outcome =
        FlushDrainOutcome::new(request.branch_id(), frozen_tables_discovered);
    let mut operation_index = 0usize;
    while branch_snapshot.frozen_table_count() > 0 && operation_index < operation_limit {
        let flush_request = request.flush_request(operation_index)?;
        let Some(prepared) = prepare_durable_flush_with_budget(
            &branch_snapshot,
            table_service,
            reader_service,
            &flush_request,
            budget,
        )?
        else {
            let maintenance = FlushFrozenOutcome::deferred(&flush_request).maintenance_outcome();
            simulated_outcome.record_maintenance_outcome(&maintenance);
            break;
        };
        let maintenance = install_prepared_durable_flush(&mut branch_snapshot, prepared.clone())
            .maintenance_outcome();
        let can_continue = simulated_outcome.record_maintenance_outcome(&maintenance);
        prepared_flushes.push(prepared);
        operation_index = operation_index.saturating_add(1);
        if !can_continue {
            break;
        }
    }
    Ok(PreparedDurableFlushDrain {
        request: request.clone(),
        prepared_flushes,
    })
}

pub(crate) fn install_prepared_durable_flush_drain_with(
    branch: &mut BranchLocalState,
    prepared: PreparedDurableFlushDrain,
    mut install_one: impl FnMut(
        &mut BranchLocalState,
        PreparedDurableFlush,
    ) -> LifecycleResult<MaintenanceOutcome>,
) -> LifecycleResult<MaintenanceOutcome> {
    if branch.branch_id() != prepared.request.branch_id() {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush drain branch id must match branch state",
        });
    }
    let active_bytes_before = branch.active_byte_count();
    let frozen_bytes_before = branch.frozen_byte_count();
    let frozen_tables_discovered = branch.frozen_table_count();
    crate::observability::perf_trace::record_lifecycle_flush_drain_frozen_tables_discovered(
        frozen_tables_discovered,
    );
    if prepared.prepared_flushes.is_empty() {
        let outcome =
            FlushDrainOutcome::new(prepared.request.branch_id(), frozen_tables_discovered)
                .skipped(0);
        record_flush_drain_outcome_counters(&outcome);
        record_flush_memory_retention(
            active_bytes_before,
            frozen_bytes_before,
            branch.active_byte_count(),
            branch.frozen_byte_count(),
        );
        return Ok(outcome.maintenance_outcome());
    }

    let mut outcome =
        FlushDrainOutcome::new(prepared.request.branch_id(), frozen_tables_discovered);
    for prepared_flush in prepared.prepared_flushes {
        match install_one(branch, prepared_flush) {
            Ok(maintenance) => {
                if !outcome.record_maintenance_outcome(&maintenance) {
                    break;
                }
            }
            Err(error) => {
                outcome.record_error(error);
                break;
            }
        }
    }

    let freeze_during_drain_retries = outcome
        .completed_flushes()
        .saturating_sub(frozen_tables_discovered);
    outcome = outcome
        .with_freeze_during_drain_retries(freeze_during_drain_retries)
        .with_post_drain_frozen_tables(branch.frozen_table_count());
    record_flush_drain_outcome_counters(&outcome);
    record_flush_memory_retention(
        active_bytes_before,
        frozen_bytes_before,
        branch.active_byte_count(),
        branch.frozen_byte_count(),
    );
    Ok(outcome.maintenance_outcome())
}

pub(crate) fn flush_branch_drain_with(
    branch: &mut BranchLocalState,
    request: &FlushDrainRequest,
    mut flush_one: impl FnMut(
        &mut BranchLocalState,
        &FlushFrozenRequest,
    ) -> LifecycleResult<MaintenanceOutcome>,
) -> LifecycleResult<FlushDrainOutcome> {
    if branch.branch_id() != request.branch_id() {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush drain branch id must match branch state",
        });
    }
    let active_bytes_before = branch.active_byte_count();
    let frozen_bytes_before = branch.frozen_byte_count();
    let frozen_tables_discovered = branch.frozen_table_count();
    crate::observability::perf_trace::record_lifecycle_flush_drain_frozen_tables_discovered(
        frozen_tables_discovered,
    );
    if frozen_tables_discovered == 0 {
        let outcome = FlushDrainOutcome::new(request.branch_id(), 0).skipped(0);
        record_flush_drain_outcome_counters(&outcome);
        record_flush_memory_retention(
            active_bytes_before,
            frozen_bytes_before,
            branch.active_byte_count(),
            branch.frozen_byte_count(),
        );
        return Ok(outcome);
    }

    let operation_limit =
        frozen_tables_discovered.saturating_add(request.freeze_during_drain_retry_limit());
    let mut outcome = FlushDrainOutcome::new(request.branch_id(), frozen_tables_discovered);
    let mut operation_index = 0usize;
    while branch.frozen_table_count() > 0 {
        if operation_index >= operation_limit {
            break;
        }
        let flush_request = request.flush_request(operation_index)?;
        match flush_one(branch, &flush_request) {
            Ok(maintenance) => {
                let can_continue = outcome.record_maintenance_outcome(&maintenance);
                operation_index = operation_index.saturating_add(1);
                if !can_continue {
                    break;
                }
            }
            Err(error) => {
                outcome.record_error(error);
                break;
            }
        }
    }

    let freeze_during_drain_retries = outcome
        .completed_flushes()
        .saturating_sub(frozen_tables_discovered);
    outcome = outcome
        .with_freeze_during_drain_retries(freeze_during_drain_retries)
        .with_post_drain_frozen_tables(branch.frozen_table_count());
    record_flush_drain_outcome_counters(&outcome);
    record_flush_memory_retention(
        active_bytes_before,
        frozen_bytes_before,
        branch.active_byte_count(),
        branch.frozen_byte_count(),
    );
    Ok(outcome)
}

pub(crate) fn flush_drain_request_from_maintenance_task(
    task: &MaintenanceTask,
) -> LifecycleResult<FlushDrainRequest> {
    let MaintenanceTaskScope::Branch(branch_id) = task.scope() else {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush drain task must target a branch",
        });
    };
    flush_drain_request_for_branch_from_maintenance_task(task, branch_id)
}

pub(crate) fn flush_drain_request_for_branch_from_maintenance_task(
    task: &MaintenanceTask,
    branch_id: BranchId,
) -> LifecycleResult<FlushDrainRequest> {
    if task.kind() != MaintenanceTaskKind::Flush {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task kind is not flush",
        });
    }
    match task.scope() {
        MaintenanceTaskScope::Branch(task_branch_id) if task_branch_id == branch_id => {}
        MaintenanceTaskScope::Branch(_) => {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush drain branch id must match task scope",
            });
        }
        MaintenanceTaskScope::Global => {}
        _ => {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush drain task must target a branch or global scope",
            });
        }
    }
    flush_drain_request_for_branch(branch_id)
}

pub(crate) fn flush_drain_request_for_branch(
    branch_id: BranchId,
) -> LifecycleResult<FlushDrainRequest> {
    Ok(FlushDrainRequest::new(
        branch_id,
        FlushTableIdentitySeed::new(format!("flush-seed-{branch_id}"))?,
        FlushTableObjectId::new(format!("flush-object-{branch_id}"))?,
    ))
}

pub(crate) fn flush_drain_maintenance_outcome_for_scope(
    outcomes: &[FlushDrainOutcome],
) -> MaintenanceOutcome {
    let mut completed_flushes = 0usize;
    let mut deferred_flushes = 0usize;
    let mut failed_flushes = 0usize;
    let mut skipped_flushes = 0usize;
    let mut post_drain_frozen_tables = 0usize;
    let mut affected_objects = 0usize;
    let mut affected_object_names = Vec::new();
    let mut bytes_reclaimed = 0u64;
    let mut retryable = false;
    let mut state_changes = 0usize;
    let mut source_error = None;
    let mut recovery_health = None;

    for outcome in outcomes {
        completed_flushes = completed_flushes.saturating_add(outcome.completed_flushes);
        deferred_flushes = deferred_flushes.saturating_add(outcome.deferred_flushes);
        failed_flushes = failed_flushes.saturating_add(outcome.failed_flushes);
        skipped_flushes = skipped_flushes.saturating_add(outcome.skipped_flushes);
        post_drain_frozen_tables =
            post_drain_frozen_tables.saturating_add(outcome.post_drain_frozen_tables);
        affected_objects = affected_objects.saturating_add(outcome.affected_objects);
        affected_object_names.extend(outcome.affected_object_names.iter().cloned());
        bytes_reclaimed = bytes_reclaimed.saturating_add(outcome.bytes_reclaimed);
        retryable |= outcome.retryable;
        state_changes = state_changes.saturating_add(outcome.state_changes);
        if source_error.is_none() {
            source_error.clone_from(&outcome.source_error);
        }
        if recovery_health.is_none() {
            recovery_health.clone_from(&outcome.recovery_health);
        }
    }

    let status = if failed_flushes > 0 {
        MaintenanceOutcomeStatus::Failed
    } else if completed_flushes > 0 && post_drain_frozen_tables == 0 {
        MaintenanceOutcomeStatus::Completed
    } else {
        MaintenanceOutcomeStatus::Deferred
    };
    let mut maintenance = MaintenanceOutcome::new(MaintenanceTaskKind::Flush, status)
        .with_effects(affected_objects, bytes_reclaimed, retryable)
        .with_state_changes(state_changes)
        .with_stats(LifecycleStats::new(
            0,
            0,
            completed_flushes
                .saturating_add(deferred_flushes)
                .saturating_add(failed_flushes)
                .saturating_add(skipped_flushes),
            0,
            0,
        ));
    if !affected_object_names.is_empty() {
        maintenance = maintenance.with_affected_object_names(affected_object_names);
    }
    if skipped_flushes > 0 && completed_flushes == 0 && failed_flushes == 0 {
        maintenance = maintenance.with_reason("flush drain has no frozen state to publish");
    } else if post_drain_frozen_tables > 0 {
        maintenance = maintenance.with_reason("flush drain left deferred frozen state");
    } else if failed_flushes > 0 {
        maintenance =
            maintenance.with_reason("flush drain failed before all frozen state was drained");
    }
    if let Some(error) = source_error {
        maintenance = maintenance.with_source_error(error);
    }
    if let Some(health) = recovery_health {
        maintenance = maintenance.with_recovery_health(health);
    } else if (completed_flushes > 0 && status != MaintenanceOutcomeStatus::Completed)
        || post_drain_frozen_tables > 0
    {
        if let Ok(health) = telemetry_health_debt("flush drain made partial progress") {
            maintenance = maintenance.with_recovery_health(health);
        }
    }
    maintenance
}

fn record_flush_drain_outcome_counters(outcome: &FlushDrainOutcome) {
    crate::observability::perf_trace::record_lifecycle_flush_drain_operations_completed(
        outcome.completed_flushes(),
    );
    crate::observability::perf_trace::record_lifecycle_flush_drain_freeze_retries(
        outcome.freeze_during_drain_retries(),
    );
    crate::observability::perf_trace::record_lifecycle_flush_drain_failures(
        outcome.failed_flushes(),
    );
    crate::observability::perf_trace::record_lifecycle_flush_drain_post_drain_frozen_tables(
        outcome.post_drain_frozen_tables(),
    );
}

fn record_flush_memory_retention(
    active_bytes_before: u64,
    frozen_bytes_before: u64,
    active_bytes_after: u64,
    frozen_bytes_after: u64,
) {
    crate::observability::perf_trace::record_lifecycle_flush_memory_retention(
        active_bytes_before,
        frozen_bytes_before,
        active_bytes_after,
        frozen_bytes_after,
        MEMORY_RELEASE_REEVALUATION_RETAINED_BYTES,
    );
}

fn select_frozen_index(
    branch: &BranchLocalState,
    request: &FlushFrozenRequest,
) -> LifecycleResult<Option<usize>> {
    if branch.branch_id() != request.branch_id() {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush branch id must match branch state",
        });
    }
    let frozen_count = branch.frozen_table_count();
    match request.frozen_index() {
        Some(index) if index < frozen_count => Ok(Some(index)),
        Some(_) => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush frozen index must exist",
        }),
        None if frozen_count == 0 => Ok(None),
        None => Ok(Some(frozen_count - 1)),
    }
}

fn build_frozen_artifact(
    branch: &BranchLocalState,
    request: &FlushFrozenRequest,
    frozen_index: usize,
) -> LifecycleResult<crate::table::BuiltTableArtifact> {
    let frozen =
        branch
            .frozen()
            .get(frozen_index)
            .ok_or(LifecycleError::MaintenanceTaskFailed {
                reason: "flush frozen index must exist",
            })?;
    let identity = derived_table_identity(request, frozen)?;
    ImmutableTableBuilder::new(super::compaction::lifecycle_table_builder_config())
        .map_err(table_error)?
        .build_from_frozen(identity, frozen)
        .map_err(table_error)
}

fn derived_table_identity(
    request: &FlushFrozenRequest,
    frozen: &FrozenTable,
) -> LifecycleResult<TableIdentity> {
    let facts = frozen.facts();
    TableIdentity::new(format!(
        "{}-{}-frozen-{}-{}-{}-{:016x}",
        request.table_identity_seed().as_str(),
        request.branch_id(),
        facts.row_count(),
        facts
            .min_commit()
            .map_or(0, strata_core_next::CommitVersion::as_u64),
        facts
            .max_commit()
            .map_or(0, strata_core_next::CommitVersion::as_u64),
        frozen_key_span_digest(frozen),
    ))
    .map_err(table_error)
}

/// FNV-1a-64 over the frozen table's physical key span (first key, a separator, then last key).
///
/// The flush object id derives from the table identity (`derived_object_id`), and
/// `publish_or_load_existing` treats an id that already exists as the same content (idempotent
/// retry). So two frozen tables that share a row count and commit range but cover different keys
/// MUST NOT share an identity — otherwise the second flush would load the first's stale object,
/// whose rows would not match this table's summary (extras). Production keeps flush identities
/// distinct through monotonic commit versions; this span digest keeps them distinct even when the
/// row count and commit range collide (e.g. raw same-version rows). It is idempotent — the same
/// frozen span yields the same digest — so retry-load stays sound.
fn frozen_key_span_digest(frozen: &FrozenTable) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    fn mix(mut hash: u64, bytes: &[u8]) -> u64 {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }
    let mut hash = FNV_OFFSET;
    if let Some(first) = frozen.first_key() {
        hash = mix(hash, first.as_slice());
    }
    // Unit separator so an empty last key cannot alias a first key that ends where last begins.
    hash = mix(hash, b"\x1f");
    if let Some(last) = frozen.last_key() {
        hash = mix(hash, last.as_slice());
    }
    hash
}

fn derived_object_id(request: &FlushFrozenRequest, table_facts: &TableRuntimeFacts) -> String {
    format!(
        "{}-{}",
        request.table_object_id().as_str(),
        table_facts.identity().as_str(),
    )
}

fn publish_or_load_existing(
    table_service: &TableObjectService<'_>,
    branch_component: &str,
    level: u32,
    object_id: &str,
    bytes: &[u8],
    table_facts: &TableRuntimeFacts,
) -> LifecycleResult<TableObjectFacts> {
    match table_service.publish_create(branch_component, level, object_id, bytes) {
        Ok(facts) => Ok(facts),
        Err(TableObjectServiceError::Publish { source, .. })
            if source.kind() == PublishFailureKind::PreconditionFailed =>
        {
            TableObjectService::facts_for_table(branch_component, level, object_id, table_facts)
                .map_err(table_service_error)
        }
        Err(error) => Err(table_service_error(error)),
    }
}

fn branch_owned_table(
    branch_id: BranchId,
    identity: TableIdentity,
    reader: ImmutableTableReader<'static>,
    extras: TableSummaryExtras,
) -> LifecycleResult<BranchOwnedTable> {
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)
            .map_err(branch_error)?;
    BranchOwnedTable::new(branch_id, descriptor, reader, extras).map_err(branch_error)
}

fn validate_single_component(field: &'static str, value: &str) -> LifecycleResult<()> {
    if value.is_empty() {
        return Err(LifecycleError::InvalidConfig {
            field,
            reason: "flush component must not be empty",
        });
    }
    if value.as_bytes().contains(&0) || value.contains('/') {
        return Err(LifecycleError::InvalidConfig {
            field,
            reason: "flush component must be a single object component",
        });
    }
    ObjectName::new(value).map_err(|_| LifecycleError::InvalidConfig {
        field,
        reason: "flush component must be a valid object name",
    })?;
    Ok(())
}

fn require_optional_generated_artifact_budget(
    budget: Option<&StorageBudgetLedger>,
    bytes: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    if let Some(budget) = budget {
        require_generated_artifact_budget(budget, bytes, reason)?;
    }
    Ok(())
}

fn require_optional_table_reader_budget(
    budget: Option<&StorageBudgetLedger>,
    bytes: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    if let Some(budget) = budget {
        require_table_reader_budget(budget, bytes, reason)?;
    }
    Ok(())
}

fn table_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::TableRuntime,
        "table runtime failed",
        error,
    )
}

fn table_service_error(error: TableObjectServiceError) -> LifecycleError {
    if let TableObjectServiceError::Publish { source, .. } = &error {
        if matches!(
            source.kind(),
            PublishFailureKind::VisibilityUnknown
                | PublishFailureKind::VisibleDurabilityUnconfirmed
        ) {
            return LifecycleError::flush_publication_uncertain_with(
                table_publish_reason(source),
                error,
            );
        }
    }
    let reason = match &error {
        TableObjectServiceError::Layout { .. } => "table object layout failed",
        TableObjectServiceError::List { .. } => "table object list failed",
        TableObjectServiceError::Metadata { .. } => "table object metadata failed",
        TableObjectServiceError::Decode { .. } => "table object decode failed",
        TableObjectServiceError::Publish { source, .. } => table_publish_reason(source),
        TableObjectServiceError::InvalidPublishMetadata { .. } => {
            "table object publish metadata invalid"
        }
    };
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, reason, error)
}

fn table_publish_reason(error: &PublishError) -> &'static str {
    match error.kind() {
        PublishFailureKind::Unsupported => "table object publish unsupported",
        PublishFailureKind::PreconditionFailed => "table object already exists",
        PublishFailureKind::FailedBeforeVisibility => {
            "table object publish failed before visibility"
        }
        PublishFailureKind::VisibilityUnknown => "table object publish visibility unknown",
        PublishFailureKind::VisibleDurabilityUnconfirmed => {
            "table object publish durability unconfirmed"
        }
    }
}

fn published_not_installed_retryable(error: &LifecycleError) -> bool {
    match error {
        LifecycleError::FlushPublicationUncertain { .. } => true,
        LifecycleError::FlushPublicationOrphaned {
            source: Some(source),
            ..
        } => source
            .downcast_ref::<LifecycleError>()
            .is_some_and(published_not_installed_retryable),
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service | LifecycleLowerLayer::Backend,
            reason,
            ..
        } => !matches!(
            *reason,
            "table object already exists" | "table object publish metadata invalid"
        ),
        _ => false,
    }
}

fn table_read_error(error: TableObjectReadError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "table object read failed",
        error,
    )
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "branch runtime failed",
        error,
    )
}
