//! Frozen-state flush orchestration.

use super::{
    require_generated_artifact_budget, require_table_reader_budget, LifecycleError,
    LifecycleLowerLayer, LifecycleResult, LifecycleStats, MaintenanceOutcome,
    MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskKind, MaintenanceTaskScope,
    StorageBudgetLedger,
};
use crate::backend::{PublishError, PublishFailureKind};
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::BranchOwnedTable;
use crate::branch::state::{BranchImmutableInstallOutcome, BranchLocalState};
use crate::object::ObjectName;
use crate::service::{
    TableObjectFacts, TableObjectReadError, TableObjectReaderService, TableObjectService,
    TableObjectServiceError,
};
use crate::table::{
    FrozenTable, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig, TableIdentity,
    TableReaderConfig, TableRuntimeFacts,
};
use sha2::{Digest, Sha256};
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
    fn deferred(request: &FlushFrozenRequest) -> Self {
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
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .map_err(table_error)?;
    let table = branch_owned_table(branch.branch_id(), identity, reader)?;
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

pub(crate) fn flush_durable_branch(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    request: &FlushFrozenRequest,
) -> LifecycleResult<FlushFrozenOutcome> {
    flush_durable_branch_with_budget(branch, table_service, reader_service, request, None)
}

pub(crate) fn flush_durable_branch_with_budget(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
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
    let branch_component = request.branch_id().to_string();
    let object_id = derived_object_id(request, &table_facts, artifact.bytes());
    let object_facts = publish_or_load_existing(
        table_service,
        &branch_component,
        request.target_level().raw().into(),
        &object_id,
        artifact.bytes(),
        &table_facts,
    )?;
    let reader = match reader_service.open_reader(
        identity.clone(),
        &object_facts,
        TableReaderConfig::default(),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            return Ok(FlushFrozenOutcome::published_not_installed_outcome(
                request,
                frozen_index,
                table_facts,
                object_facts,
                table_read_error(error),
            ));
        }
    };
    let table = match branch_owned_table(branch.branch_id(), identity, reader) {
        Ok(table) => table,
        Err(error) => {
            return Ok(FlushFrozenOutcome::published_not_installed_outcome(
                request,
                frozen_index,
                table_facts,
                object_facts,
                error,
            ));
        }
    };
    let install_outcome = match branch.replace_frozen_with_level_zero_table(frozen_index, table) {
        Ok(outcome) => outcome,
        Err(error) => {
            return Ok(FlushFrozenOutcome::published_not_installed_outcome(
                request,
                frozen_index,
                table_facts,
                object_facts,
                branch_error(error),
            ));
        }
    };
    Ok(FlushFrozenOutcome::completed_outcome(
        request,
        frozen_index,
        table_facts,
        Some(object_facts),
        install_outcome,
    ))
}

pub(crate) fn flush_request_from_maintenance_task(
    task: &MaintenanceTask,
) -> LifecycleResult<FlushFrozenRequest> {
    if task.kind() != MaintenanceTaskKind::Flush {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task kind is not flush",
        });
    }
    let MaintenanceTaskScope::Branch(branch_id) = task.scope() else {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush task must target a branch",
        });
    };
    FlushFrozenRequest::new(
        branch_id,
        None,
        FlushTableIdentitySeed::new(format!("flush-seed-{branch_id}"))?,
        FlushTableObjectId::new(format!("flush-object-{branch_id}"))?,
    )
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
    ImmutableTableBuilder::new(TableBuilderConfig::default())
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
        "{}-{}-frozen-{}-{}-{}",
        request.table_identity_seed().as_str(),
        request.branch_id(),
        facts.row_count(),
        facts
            .max_commit()
            .map_or(0, strata_core_next::CommitVersion::as_u64),
        frozen_digest(frozen),
    ))
    .map_err(table_error)
}

fn derived_object_id(
    request: &FlushFrozenRequest,
    table_facts: &TableRuntimeFacts,
    bytes: &[u8],
) -> String {
    format!(
        "{}-frozen-{}-{}-{}",
        request.table_object_id().as_str(),
        table_facts.row_count(),
        table_facts.commit_range().max().as_u64(),
        digest_hex(bytes),
    )
}

fn frozen_digest(frozen: &FrozenTable) -> String {
    let mut hasher = Sha256::new();
    for row in frozen.iter() {
        hasher.update(row.key().as_slice());
        hasher.update(row.row().value());
        hasher.update(row.commit_version().as_u64().to_be_bytes());
        hasher.update(row.row().commit_timestamp().as_micros().to_be_bytes());
        hasher.update(row.row().expires_at().as_micros().to_be_bytes());
        hasher.update([u8::from(row.row().is_tombstone())]);
    }
    digest_to_hex(hasher.finalize().as_slice())
}

fn digest_hex(bytes: &[u8]) -> String {
    digest_to_hex(Sha256::digest(bytes).as_slice())
}

fn digest_to_hex(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest.iter().copied() {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
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
        Ok(write) => Ok(write.facts().clone()),
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
    reader: ImmutableTableReader,
) -> LifecycleResult<BranchOwnedTable> {
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)
            .map_err(branch_error)?;
    BranchOwnedTable::new(branch_id, descriptor, reader).map_err(branch_error)
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
