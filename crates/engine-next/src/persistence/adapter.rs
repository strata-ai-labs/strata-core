//! Adapter from engine persistence plans to the storage crate.

use std::path::PathBuf;

use strata_core_next::BranchId;
use strata_core_next::{CommitVersion, Timestamp};
use strata_storage_next::api::{
    BranchAction, BranchGeneration as StorageBranchGeneration, BranchRequest, CommitBatch,
    CommitDurabilitySummary, CommitMutation, CommitOptions, HistoryReadRequest, PointReadRequest,
    PrefixScanReadRequest, ReadBound, ReadLimit, ScanRange, ScanReadRequest, StorageApiError,
    StorageApiErrorClass, StorageCloseSummary, StorageKey, StorageOpenDisposition, StorageReadRow,
    StorageRuntime, StorageRuntimeState, StorageSpaceId, StorageValue,
};

use crate::branch::catalog::{DEFAULT_BRANCH_GENERATION, SYSTEM_BRANCH_ID};
use crate::commit::CommitOutcome;
use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};

use super::{CommitPlan, ReadSelector, RowAddress, RowClass, RowMutation};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PersistenceOpenTarget {
    Cache,
    DurableLocal(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PersistenceOpenSummary {
    created: bool,
    durable: bool,
}

impl PersistenceOpenSummary {
    #[must_use]
    pub(crate) const fn created(self) -> bool {
        self.created
    }

    #[must_use]
    pub(crate) const fn durable(self) -> bool {
        self.durable
    }
}

pub(crate) struct StoragePersistence {
    runtime: StorageRuntime<'static>,
    durable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistenceReadRow {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    tombstone: bool,
}

impl PersistenceReadRow {
    fn from_storage(row: &StorageReadRow) -> Self {
        Self {
            key: row.key().as_bytes().to_vec(),
            value: row.value().map(|value| value.as_bytes().to_vec()),
            commit_version: row.commit_version(),
            commit_timestamp: row.commit_timestamp(),
            tombstone: row.is_tombstone(),
        }
    }

    pub(crate) fn key(&self) -> &[u8] {
        &self.key
    }

    pub(crate) fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

impl StoragePersistence {
    pub(crate) fn open(
        target: PersistenceOpenTarget,
    ) -> EngineResult<(Self, PersistenceOpenSummary)> {
        let (runtime, summary, durable) = match target {
            PersistenceOpenTarget::Cache => {
                let outcome = StorageRuntime::open_cache().map_err(map_storage_error)?;
                let (runtime, summary) = outcome.into_parts();
                (runtime, summary, false)
            }
            PersistenceOpenTarget::DurableLocal(path) => {
                let outcome = StorageRuntime::open_local(path).map_err(map_storage_error)?;
                let (runtime, summary) = outcome.into_parts();
                (runtime, summary, true)
            }
        };
        let created = matches!(summary.disposition(), StorageOpenDisposition::Created);
        Ok((
            Self { runtime, durable },
            PersistenceOpenSummary { created, durable },
        ))
    }

    pub(crate) fn create_system_branch_for_new_database(&mut self) -> EngineResult<()> {
        self.ensure_branch_created(SYSTEM_BRANCH_ID, DEFAULT_BRANCH_GENERATION)
    }

    pub(crate) fn ensure_branch_created(
        &mut self,
        branch_id: BranchId,
        generation: u64,
    ) -> EngineResult<()> {
        if self.branch_exists(branch_id)? {
            return Ok(());
        }
        let request = BranchRequest::new(
            branch_id,
            BranchAction::Create,
            Some(StorageBranchGeneration::new(generation)),
        );
        match self.runtime.branch(&request) {
            Ok(_) | Err(StorageApiError::BranchAlreadyExists { .. }) => Ok(()),
            Err(error) => Err(map_storage_error(error)),
        }
    }

    pub(crate) fn branch_exists(&mut self, branch_id: BranchId) -> EngineResult<bool> {
        let request = BranchRequest::new(branch_id, BranchAction::Describe, None);
        match self.runtime.branch(&request) {
            Ok(_) => Ok(true),
            Err(StorageApiError::BranchNotFound { .. }) => Ok(false),
            Err(error) => Err(map_storage_error(error)),
        }
    }

    pub(crate) fn fork_branch_current(
        &mut self,
        branch_id: BranchId,
        source: BranchId,
        generation: u64,
    ) -> EngineResult<()> {
        let request = BranchRequest::new(
            branch_id,
            BranchAction::ForkCurrent { source },
            Some(StorageBranchGeneration::new(generation)),
        );
        self.runtime
            .branch(&request)
            .map(|_| ())
            .map_err(map_storage_error)
    }

    pub(crate) fn commit(&mut self, plan: &CommitPlan) -> EngineResult<CommitOutcome> {
        let mut mutations = Vec::with_capacity(plan.mutations().len());
        for mutation in plan.mutations() {
            mutations.push(to_storage_mutation(mutation)?);
        }
        let mut options = CommitOptions::default();
        if let Some(generation) = plan.expected_generation() {
            options = options.with_expected_generation(StorageBranchGeneration::new(generation));
        }
        let batch =
            CommitBatch::new(plan.branch_id(), mutations, options).map_err(map_storage_error)?;
        let summary = self.runtime.commit(&batch).map_err(map_storage_error)?;
        Ok(CommitOutcome::new(
            summary.commit_version(),
            summary.commit_timestamp(),
            summary.put_count(),
            summary.delete_count(),
            durable_commit_summary(summary.durability()),
        ))
    }

    pub(crate) fn read(
        &mut self,
        address: &RowAddress,
        selector: ReadSelector,
    ) -> EngineResult<Option<Vec<u8>>> {
        Ok(self
            .read_row(address, selector)?
            .and_then(|row| (!row.is_tombstone()).then_some(row))
            .and_then(|row| row.value().map(<[u8]>::to_vec)))
    }

    pub(crate) fn read_row(
        &mut self,
        address: &RowAddress,
        selector: ReadSelector,
    ) -> EngineResult<Option<PersistenceReadRow>> {
        let outcome = match self
            .runtime
            .read_point(&point_read_request(address, selector)?)
        {
            Ok(outcome) => outcome,
            Err(error) if is_before_retained_timestamp_history(&error) => return Ok(None),
            Err(error) if should_fall_back_to_latest(selector, &error) => self
                .runtime
                .read_point(&point_read_request(address, ReadSelector::Latest)?)
                .map_err(map_storage_error)?,
            Err(error) => return Err(map_storage_error(error)),
        };
        Ok(outcome.row().map(PersistenceReadRow::from_storage))
    }

    pub(crate) fn read_history(
        &mut self,
        address: &RowAddress,
        include_tombstones: bool,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        let request = HistoryReadRequest::new(
            address.branch_id(),
            storage_space(address)?,
            storage_key(address)?,
        )
        .include_tombstones(include_tombstones);
        let outcome = self
            .runtime
            .read_history(&request)
            .map_err(map_storage_error)?;
        Ok(outcome
            .rows()
            .iter()
            .map(PersistenceReadRow::from_storage)
            .collect())
    }

    pub(crate) fn scan_prefix(
        &mut self,
        branch_id: BranchId,
        row_class: RowClass,
        prefix: Vec<u8>,
        selector: ReadSelector,
        limit: Option<usize>,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        if limit == Some(0) {
            return Ok(Vec::new());
        }
        let limit = read_limit(limit)?;
        let outcome = match self.runtime.scan_prefix(&prefix_scan_request(
            branch_id,
            row_class,
            prefix.clone(),
            selector,
            limit,
        )?) {
            Ok(outcome) => outcome,
            Err(error) if is_before_retained_timestamp_history(&error) => return Ok(Vec::new()),
            Err(error) if should_fall_back_to_latest(selector, &error) => self
                .runtime
                .scan_prefix(&prefix_scan_request(
                    branch_id,
                    row_class,
                    prefix,
                    ReadSelector::Latest,
                    limit,
                )?)
                .map_err(map_storage_error)?,
            Err(error) => return Err(map_storage_error(error)),
        };
        Ok(outcome
            .rows()
            .iter()
            .map(PersistenceReadRow::from_storage)
            .collect())
    }

    pub(crate) fn scan_range(
        &mut self,
        branch_id: BranchId,
        row_class: RowClass,
        start: Option<Vec<u8>>,
        end: Option<Vec<u8>>,
        selector: ReadSelector,
        limit: Option<usize>,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        if limit == Some(0) {
            return Ok(Vec::new());
        }
        let limit = read_limit(limit)?;
        let outcome = match self.runtime.scan_range(&scan_range_request(
            branch_id,
            row_class,
            start.clone(),
            end.clone(),
            selector,
            limit,
        )?) {
            Ok(outcome) => outcome,
            Err(error) if is_before_retained_timestamp_history(&error) => return Ok(Vec::new()),
            Err(error) if should_fall_back_to_latest(selector, &error) => self
                .runtime
                .scan_range(&scan_range_request(
                    branch_id,
                    row_class,
                    start,
                    end,
                    ReadSelector::Latest,
                    limit,
                )?)
                .map_err(map_storage_error)?,
            Err(error) => return Err(map_storage_error(error)),
        };
        Ok(outcome
            .rows()
            .iter()
            .map(PersistenceReadRow::from_storage)
            .collect())
    }

    pub(crate) fn close(&mut self) -> EngineResult<StorageCloseSummary> {
        self.runtime.close().map_err(map_storage_error)
    }

    #[must_use]
    pub(crate) const fn durable(&self) -> bool {
        self.durable
    }
}

fn to_storage_mutation(mutation: &RowMutation) -> EngineResult<CommitMutation> {
    match mutation {
        RowMutation::Put { address, value } => Ok(CommitMutation::Put {
            storage_space: storage_space(address)?,
            key: storage_key(address)?,
            value: StorageValue::new(value.clone()),
            ttl: None,
        }),
        RowMutation::Delete { address } => Ok(CommitMutation::Delete {
            storage_space: storage_space(address)?,
            key: storage_key(address)?,
        }),
    }
}

fn storage_space(address: &RowAddress) -> EngineResult<StorageSpaceId> {
    storage_space_for_class(address.row_class())
}

fn storage_space_for_class(row_class: RowClass) -> EngineResult<StorageSpaceId> {
    StorageSpaceId::new(vec![row_class.storage_space_id()]).map_err(map_storage_error)
}

fn storage_key(address: &RowAddress) -> EngineResult<StorageKey> {
    storage_key_from_bytes(address.key().to_vec())
}

fn storage_key_from_bytes(bytes: Vec<u8>) -> EngineResult<StorageKey> {
    StorageKey::new(bytes).map_err(map_storage_error)
}

fn point_read_request(
    address: &RowAddress,
    selector: ReadSelector,
) -> EngineResult<PointReadRequest> {
    Ok(PointReadRequest::new(
        address.branch_id(),
        storage_space(address)?,
        storage_key(address)?,
        storage_read_bound(selector),
    ))
}

fn prefix_scan_request(
    branch_id: BranchId,
    row_class: RowClass,
    prefix: Vec<u8>,
    selector: ReadSelector,
    limit: Option<ReadLimit>,
) -> EngineResult<PrefixScanReadRequest> {
    Ok(PrefixScanReadRequest::new(
        branch_id,
        storage_space_for_class(row_class)?,
        storage_key_from_bytes(prefix)?,
        storage_read_bound(selector),
        limit,
    ))
}

fn scan_range_request(
    branch_id: BranchId,
    row_class: RowClass,
    start: Option<Vec<u8>>,
    end: Option<Vec<u8>>,
    selector: ReadSelector,
    limit: Option<ReadLimit>,
) -> EngineResult<ScanReadRequest> {
    let range = ScanRange::new(
        start.map(storage_key_from_bytes).transpose()?,
        end.map(storage_key_from_bytes).transpose()?,
    )
    .map_err(map_storage_error)?;
    Ok(ScanReadRequest::new(
        branch_id,
        storage_space_for_class(row_class)?,
        range,
        storage_read_bound(selector),
        limit,
    ))
}

fn storage_read_bound(selector: ReadSelector) -> ReadBound {
    match selector {
        ReadSelector::Latest => ReadBound::Latest,
        ReadSelector::AtVersion(version) => ReadBound::AtVersion(version),
        ReadSelector::AtTimestamp(timestamp) => ReadBound::AtTimestamp(timestamp),
    }
}

fn read_limit(limit: Option<usize>) -> EngineResult<Option<ReadLimit>> {
    match limit {
        Some(limit) => ReadLimit::new(limit).map(Some).map_err(map_storage_error),
        None => Ok(None),
    }
}

fn is_before_retained_timestamp_history(error: &StorageApiError) -> bool {
    matches!(
        error,
        StorageApiError::TimestampHistoryUnavailable { reason, .. }
            if *reason == "timestamp is before retained timeline history"
    )
}

fn should_fall_back_to_latest(selector: ReadSelector, error: &StorageApiError) -> bool {
    matches!(selector, ReadSelector::AtTimestamp(_))
        && matches!(
            error,
            StorageApiError::TimestampHistoryUnavailable { reason, .. }
                if *reason == "timestamp is after latest retained timeline history"
        )
}

const fn durable_commit_summary(summary: CommitDurabilitySummary) -> bool {
    matches!(
        summary,
        CommitDurabilitySummary::Standard | CommitDurabilitySummary::Always
    )
}

pub(crate) fn map_storage_error(error: StorageApiError) -> EngineError {
    match &error {
        StorageApiError::BranchGenerationMismatch { .. } => {
            return EngineError::with_source(
                EngineErrorClass::Conflict,
                "conflict.engine.branch_generation",
                false,
                "branch generation changed before the write could commit",
                error,
            );
        }
        StorageApiError::RecoveryDegraded { .. } => {
            return EngineError::with_source(
                EngineErrorClass::Corruption,
                "data_loss.engine.persistence_recovery",
                false,
                "persistence recovery reported degraded state",
                error,
            );
        }
        StorageApiError::LowerLayer { .. } => {
            return EngineError::with_source(
                EngineErrorClass::Unavailable,
                "unavailable.engine.persistence",
                true,
                "persistence lower layer is unavailable",
                error,
            );
        }
        _ => {}
    }
    let (class, code, retryable, message) = match error.class() {
        StorageApiErrorClass::InvalidArgument => (
            EngineErrorClass::InvalidInput,
            "invalid_argument.engine.persistence",
            false,
            "persistence request was invalid",
        ),
        StorageApiErrorClass::NotFound => (
            EngineErrorClass::NotFound,
            "not_found.engine.persistence",
            false,
            "persistence target was not found",
        ),
        StorageApiErrorClass::AlreadyExists | StorageApiErrorClass::Conflict => (
            EngineErrorClass::Conflict,
            "conflict.engine.persistence",
            false,
            "persistence target conflicted with existing state",
        ),
        StorageApiErrorClass::Unsupported => (
            EngineErrorClass::Unavailable,
            "unavailable.engine.persistence_capability",
            false,
            "requested persistence capability is unavailable",
        ),
        StorageApiErrorClass::HistoryUnavailable => (
            EngineErrorClass::NotFound,
            "not_found.engine.persistence_history",
            false,
            "requested persistence history is unavailable",
        ),
        StorageApiErrorClass::AmbiguousCommit => (
            EngineErrorClass::AmbiguousCommit,
            "ambiguous_commit.engine.persistence",
            true,
            "persistence could not prove whether the commit succeeded",
        ),
        StorageApiErrorClass::FailedPrecondition => (
            EngineErrorClass::Unavailable,
            "failed_precondition.engine.persistence",
            true,
            "persistence is temporarily unable to accept the request",
        ),
        StorageApiErrorClass::Internal => (
            EngineErrorClass::Internal,
            "internal.engine.persistence",
            false,
            "persistence returned an internal failure",
        ),
        _ => (
            EngineErrorClass::Internal,
            "internal.engine.persistence",
            false,
            "persistence returned an unknown failure",
        ),
    };
    EngineError::with_source(class, code, retryable, message, error)
}

pub(crate) fn close_summary_is_durable(summary: StorageCloseSummary) -> bool {
    summary.state() == StorageRuntimeState::Closed && summary.durable_synced()
}

#[cfg(test)]
mod tests {
    use strata_core_next::BranchId;
    use strata_storage_next::api::{
        CommitAdmissionPressureReason, CommitAdmissionPressureSeverity, StorageApiError,
        StorageApiLowerLayer,
    };

    use super::map_storage_error;
    use crate::diagnostics::EngineErrorClass;

    #[test]
    fn storage_conflict_maps_to_engine_conflict() {
        let error = map_storage_error(StorageApiError::Conflict {
            branch_id: BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
            storage_space: None,
            key_fingerprint: None,
            user_key_len: None,
            reason: "test conflict",
        });
        assert_eq!(error.class(), EngineErrorClass::Conflict);
        assert_eq!(error.code(), "conflict.engine.persistence");
        assert!(!error.retryable());
        assert!(error.source_arc().is_some());
    }

    #[test]
    fn storage_pressure_maps_to_retryable_unavailable() {
        let error = map_storage_error(StorageApiError::StoragePressure {
            branch_id: BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
            severity: CommitAdmissionPressureSeverity::Blocking,
            pressure_reason: CommitAdmissionPressureReason::MaintenanceQueueBacklog,
            reason: "test pressure",
            retryable: true,
        });
        assert_eq!(error.class(), EngineErrorClass::Unavailable);
        assert_eq!(error.code(), "failed_precondition.engine.persistence");
        assert!(error.retryable());
        assert!(error.source_arc().is_some());
    }

    #[test]
    fn ambiguous_storage_commit_remains_retryable_and_ambiguous() {
        let error = map_storage_error(StorageApiError::durable_uncertain("test uncertainty"));
        assert_eq!(error.class(), EngineErrorClass::AmbiguousCommit);
        assert_eq!(error.code(), "ambiguous_commit.engine.persistence");
        assert!(error.retryable());
        assert!(error.source_arc().is_some());
    }

    #[test]
    fn storage_branch_generation_mismatch_maps_to_conflict() {
        let error = map_storage_error(StorageApiError::BranchGenerationMismatch {
            branch_id: BranchId::from_bytes([0x11; BranchId::BYTE_LEN]),
            expected: 1,
            actual: 2,
        });
        assert_eq!(error.class(), EngineErrorClass::Conflict);
        assert_eq!(error.code(), "conflict.engine.branch_generation");
        assert!(!error.retryable());
        assert!(error.source_arc().is_some());
    }

    #[test]
    fn degraded_recovery_maps_to_corruption() {
        let error = map_storage_error(StorageApiError::RecoveryDegraded {
            reason: "test recovery degradation",
        });
        assert_eq!(error.class(), EngineErrorClass::Corruption);
        assert_eq!(error.code(), "data_loss.engine.persistence_recovery");
        assert!(!error.retryable());
        assert!(error.source_arc().is_some());
    }

    #[test]
    fn lower_layer_storage_error_maps_to_retryable_unavailable() {
        let error = map_storage_error(StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Service,
            "test lower layer",
            std::io::Error::new(std::io::ErrorKind::Other, "test source"),
        ));
        assert_eq!(error.class(), EngineErrorClass::Unavailable);
        assert_eq!(error.code(), "unavailable.engine.persistence");
        assert!(error.retryable());
        assert!(error.source_arc().is_some());
    }
}
