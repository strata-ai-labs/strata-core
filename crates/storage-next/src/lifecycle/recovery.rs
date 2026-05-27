//! Durable-local recovery orchestration.

use super::checkpoint::branch_durable_rows_cover_interval;
use super::{
    preflight_table_manifest_with_checkpoint, require_generated_artifact_budget,
    require_table_manifest_covers_checkpoint_rows, LifecycleDurableLocalShell, LifecycleError,
    LifecycleLowerLayer, LifecycleResult, LifecycleTableManifestRecoveryOutcome,
    LifecycleTableManifestRecoveryStage, RecoveryDegradationClass, RecoveryFault,
    RecoveryFaultKind, RecoveryHealth, RecoveryStrictness, StorageBudgetLedger, StorageOpenPlan,
};
use crate::branch::{
    install_snapshot_rows_into_branches, BranchSnapshotInstallOutcome, BranchSnapshotInstallRequest,
};
use crate::format::{
    decode_storage_row, encode_storage_row, FormatError, SnapshotContainer, SnapshotSection,
    WalRecord,
};
use crate::object::ObjectName;
use crate::row::StorageRow;
use crate::service::{
    QuarantineServiceError, SnapshotServiceError, TableObjectFacts, WalRepair, WalServiceError,
    WalTruncation,
};
use crate::table::TableIdentity;
use strata_core_next::{CommitVersion, Timestamp};

pub(crate) const SNAPSHOT_ROW_SECTION_KIND: u8 = 1;

const SNAPSHOT_ROWS_FORMAT: &str = "lifecycle_snapshot_rows";
const SNAPSHOT_ROWS_MAGIC: [u8; 4] = *b"STRR";
const SNAPSHOT_ROWS_VERSION: u32 = 1;
const SNAPSHOT_ROWS_HEADER_SIZE: usize = 12;

#[derive(Debug)]
pub(crate) struct LifecycleRecoveryRuntime<'shell, 'backend, S> {
    shell: &'shell mut LifecycleDurableLocalShell<'backend, S>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveryRequest {
    strictness: RecoveryStrictness,
    max_faults: usize,
    max_snapshot_sections: usize,
    checkpoint_identity_seed: TableIdentity,
    table_objects: Vec<LifecycleRecoveryTableObject>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveryTableObject {
    identity: TableIdentity,
    facts: TableObjectFacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveryOutcome {
    health: RecoveryHealth,
    checkpoint: LifecycleRecoveredCheckpoint,
    wal: LifecycleRecoveredWal,
    quarantine: LifecycleRecoveredQuarantine,
    tables: LifecycleRecoveredTables,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveredCheckpoint {
    snapshot_id: Option<u64>,
    trusted_watermark: Option<CommitVersion>,
    section_count: usize,
    row_count: usize,
    install_outcome: Option<BranchSnapshotInstallOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveredWal {
    replay_start: CommitVersion,
    records: Vec<WalRecord>,
    truncation: Option<WalTruncation>,
    repair: Option<WalRepair>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveredQuarantine {
    object: Option<ObjectName>,
    present: bool,
    byte_count: u64,
    entry_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveredTables {
    validated: Vec<LifecycleRecoveredTable>,
    table_manifest: LifecycleTableManifestRecoveryOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveredTable {
    identity: TableIdentity,
    facts: TableObjectFacts,
}

struct TableRecoveryStage {
    recovered_tables: LifecycleRecoveredTables,
    table_manifest_stage: Option<LifecycleTableManifestRecoveryStage>,
}

impl<'shell, 'backend, S> LifecycleRecoveryRuntime<'shell, 'backend, S> {
    pub(crate) fn new(shell: &'shell mut LifecycleDurableLocalShell<'backend, S>) -> Self {
        Self { shell }
    }

    pub(crate) fn recover(
        &mut self,
        request: &LifecycleRecoveryRequest,
    ) -> LifecycleResult<LifecycleRecoveryOutcome> {
        self.shell.admit_recovery_step()?;
        request.validate_against_plan(self.shell.open_plan())?;

        let mut faults = Vec::new();
        let checkpoint = self.recover_checkpoint(request, &mut faults)?;
        let quarantine = self.recover_quarantine(request, &mut faults)?;
        let tables = self.recover_tables(request, &mut faults)?;
        let trusted_flush_watermark = validate_flush_watermark_is_recoverable(
            self.shell.assembly_facts().manifest_flush_watermark(),
            &checkpoint.checkpoint,
            &tables.recovered_tables,
            tables
                .table_manifest_stage
                .as_ref()
                .map(LifecycleTableManifestRecoveryStage::staged_branch),
            request.strictness(),
            &mut faults,
            request.max_faults(),
        )?;
        let replay_start = trusted_replay_start(
            checkpoint.checkpoint.trusted_watermark(),
            trusted_flush_watermark,
        );
        let wal = self.recover_wal(request, replay_start, &mut faults)?;
        let health = recovery_health_from_faults(request, faults)?;
        let use_table_manifest_as_base = trusted_flush_watermark.is_some_and(|flush| {
            checkpoint
                .checkpoint
                .trusted_watermark()
                .is_none_or(|checkpoint| flush > checkpoint)
        });
        if let Some(recovered_branch) = checkpoint.recovered_branch {
            if let Some(stage) = tables.table_manifest_stage {
                // Recovery Protocol rule 9: when both checkpoint rows and a
                // table manifest are present, preflight the combined state
                // before installing either source. Exact byte duplicates at
                // the same internal key are accepted; divergent bytes fail.
                preflight_table_manifest_with_checkpoint(&recovered_branch, stage.staged_branch())?;
                if use_table_manifest_as_base {
                    require_table_manifest_covers_checkpoint_rows(
                        &recovered_branch,
                        stage.staged_branch(),
                    )?;
                    self.shell.apply_table_manifest_recovery(stage);
                    return Ok(LifecycleRecoveryOutcome {
                        health,
                        checkpoint: checkpoint.checkpoint,
                        wal,
                        quarantine,
                        tables: tables.recovered_tables,
                    });
                }
                // The flush watermark sits at or below the checkpoint, so the
                // checkpoint is authoritative for the in-memory branch state.
                // The staged manifest stays staged: its table objects remain
                // reachable via the catalog and are protected by table-object
                // reachability retention, but the rows are not promoted into
                // the branch state — the checkpoint is a superset for this
                // commit range.
                //
                // The manifest's retained-history extension must still be
                // applied to the checkpoint-recovered branch so that prior
                // row pruning narrows timestamp coverage on reopen.
                let staged_coverage = stage.staged_branch().timestamp_coverage();
                *self.shell.branch_state_mut() = recovered_branch;
                self.shell
                    .branch_state_mut()
                    .set_timestamp_coverage(staged_coverage);
                return Ok(LifecycleRecoveryOutcome {
                    health,
                    checkpoint: checkpoint.checkpoint,
                    wal,
                    quarantine,
                    tables: tables.recovered_tables,
                });
            }
            *self.shell.branch_state_mut() = recovered_branch;
        } else if let Some(stage) = tables.table_manifest_stage {
            self.shell.apply_table_manifest_recovery(stage);
        }

        Ok(LifecycleRecoveryOutcome {
            health,
            checkpoint: checkpoint.checkpoint,
            wal,
            quarantine,
            tables: tables.recovered_tables,
        })
    }

    fn recover_checkpoint(
        &mut self,
        request: &LifecycleRecoveryRequest,
        faults: &mut Vec<RecoveryFault>,
    ) -> LifecycleResult<CheckpointRecovery> {
        let snapshot_id = self.shell.assembly_facts().manifest_snapshot_id();
        let snapshot_watermark = manifest_snapshot_watermark(self.shell.assembly_facts())?;
        match (snapshot_id, snapshot_watermark) {
            (None, None) => Ok(CheckpointRecovery::empty()),
            (Some(id), Some(watermark)) => {
                self.load_and_install_checkpoint(request, faults, id, watermark)
            }
            (Some(_), None) | (None, Some(_)) => Err(LifecycleError::RecoveryFailed {
                reason: "manifest snapshot id and watermark must be present together",
            }),
        }
    }

    fn load_and_install_checkpoint(
        &mut self,
        request: &LifecycleRecoveryRequest,
        faults: &mut Vec<RecoveryFault>,
        snapshot_id: u64,
        watermark: CommitVersion,
    ) -> LifecycleResult<CheckpointRecovery> {
        if watermark == CommitVersion::ZERO {
            return Err(LifecycleError::RecoveryFailed {
                reason: "manifest snapshot watermark must be nonzero",
            });
        }
        if snapshot_id == 0 {
            return Err(LifecycleError::RecoveryFailed {
                reason: "manifest snapshot id must be nonzero",
            });
        }

        let container = match self.shell.services().snapshot().load_required_for_codec(
            snapshot_id,
            *self.shell.assembly_facts().database_id(),
            self.shell.assembly_facts().codec_id(),
        ) {
            Ok(container) => container,
            Err(SnapshotServiceError::Missing { .. })
                if request.strictness == RecoveryStrictness::AllowExplicitLossyFallback =>
            {
                push_fault(
                    faults,
                    request.max_faults,
                    RecoveryFaultKind::MissingSnapshotObject,
                    "manifest-listed snapshot is missing",
                )?;
                return Ok(CheckpointRecovery::missing_lossy(snapshot_id));
            }
            Err(source) => {
                return Err(snapshot_error(source));
            }
        };

        validate_snapshot_watermark(&container, watermark)?;
        if container.sections().len() > request.max_snapshot_sections {
            return Err(LifecycleError::RecoveryFailed {
                reason: "snapshot section count exceeds lifecycle recovery limit",
            });
        }
        require_checkpoint_decode_budget(self.shell.budget(), container.sections())?;
        let rows = decode_checkpoint_rows(container.sections())?;
        validate_checkpoint_rows(watermark, self.shell.branch_state().branch_id(), &rows)?;
        let row_count = rows.len();
        let install_outcome = install_checkpoint_rows(
            self.shell.branch_state().clone(),
            request.checkpoint_identity_seed(),
            rows,
        )?;
        Ok(CheckpointRecovery {
            checkpoint: LifecycleRecoveredCheckpoint {
                snapshot_id: Some(snapshot_id),
                trusted_watermark: Some(watermark),
                section_count: container.sections().len(),
                row_count,
                install_outcome: Some(install_outcome.outcome),
            },
            recovered_branch: install_outcome.recovered_branch,
        })
    }

    fn recover_wal(
        &mut self,
        request: &LifecycleRecoveryRequest,
        replay_start: CommitVersion,
        faults: &mut Vec<RecoveryFault>,
    ) -> LifecycleResult<LifecycleRecoveredWal> {
        let read = self
            .shell
            .services()
            .wal()
            .read_after_commit_version(replay_start)
            .map_err(wal_error)?;
        let truncation = read.truncation().cloned();
        let repair = match truncation.as_ref() {
            Some(truncation) => {
                if request.strictness() == RecoveryStrictness::Strict {
                    return Err(LifecycleError::WalTailRepairRejected {
                        reason: "strict recovery cannot repair partial WAL tail",
                    });
                }
                push_fault(
                    faults,
                    request.max_faults(),
                    RecoveryFaultKind::WalTailRepairFailed,
                    "partial WAL tail repaired with data loss",
                )?;
                Some(
                    self.shell
                        .services_mut()
                        .wal_mut()
                        .repair_latest_tail(truncation)
                        .map_err(wal_repair_error)?,
                )
            }
            None => None,
        };

        Ok(LifecycleRecoveredWal {
            replay_start,
            records: read.records().to_vec(),
            truncation,
            repair,
        })
    }

    fn recover_tables(
        &mut self,
        request: &LifecycleRecoveryRequest,
        faults: &mut Vec<RecoveryFault>,
    ) -> LifecycleResult<TableRecoveryStage> {
        if !request.table_objects().is_empty() {
            return Err(LifecycleError::RecoveryFailed {
                reason: "table object recovery references require a table manifest",
            });
        }
        let stage = match self.shell.stage_table_manifest_recovery() {
            Ok(stage) => stage,
            Err(error)
                if request.strictness() == RecoveryStrictness::AllowExplicitLossyFallback
                    && is_lossy_table_manifest_recovery_error(&error) =>
            {
                push_fault_for_branch(
                    faults,
                    request.max_faults(),
                    table_manifest_recovery_fault_kind(&error),
                    table_manifest_recovery_fault_reason(&error),
                    self.shell.branch_state().branch_id(),
                )?;
                return Ok(TableRecoveryStage {
                    recovered_tables: LifecycleRecoveredTables {
                        validated: Vec::new(),
                        table_manifest: LifecycleTableManifestRecoveryOutcome::absent(),
                    },
                    table_manifest_stage: None,
                });
            }
            Err(error) => return Err(error),
        };
        let outcome = stage.outcome().clone();
        let table_manifest_stage = if outcome.manifest_sequence().is_some() {
            Some(stage)
        } else {
            None
        };
        Ok(TableRecoveryStage {
            recovered_tables: LifecycleRecoveredTables {
                validated: Vec::new(),
                table_manifest: outcome,
            },
            table_manifest_stage,
        })
    }

    fn recover_quarantine(
        &self,
        request: &LifecycleRecoveryRequest,
        faults: &mut Vec<RecoveryFault>,
    ) -> LifecycleResult<LifecycleRecoveredQuarantine> {
        let branch_id = self.shell.branch_state().branch_id();
        match self.shell.services().quarantine().load_inventory(
            branch_id,
            *self.shell.assembly_facts().database_id(),
            self.shell.assembly_facts().codec_id(),
        ) {
            Ok(load) => Ok(LifecycleRecoveredQuarantine {
                object: Some(load.object().clone()),
                present: load.is_present(),
                byte_count: load.byte_count(),
                entry_count: load.entry_count(),
            }),
            Err(source)
                if request.strictness == RecoveryStrictness::AllowExplicitLossyFallback
                    && is_quarantine_inventory_mismatch(&source) =>
            {
                let object = quarantine_error_object(&source);
                // The mismatch is scoped to this branch — attach the branch
                // id so downstream `safe_for_candidate` /
                // `fresh_for_candidate` admission checks can refuse
                // reclaim under Telemetry debt that names this branch.
                push_fault_for_branch(
                    faults,
                    request.max_faults,
                    RecoveryFaultKind::QuarantineInventoryMismatch,
                    "quarantine inventory mismatch",
                    branch_id,
                )?;
                Ok(LifecycleRecoveredQuarantine::unknown(object))
            }
            Err(source) => Err(quarantine_error(source)),
        }
    }
}

fn require_checkpoint_decode_budget(
    budget: &StorageBudgetLedger,
    sections: &[SnapshotSection],
) -> LifecycleResult<()> {
    let bytes = sections.iter().try_fold(0_u64, |total, section| {
        let payload_len = u64::try_from(section.payload().len()).map_err(|_| {
            LifecycleError::StorageBudgetExceeded {
                pool: super::StorageBudgetPool::GeneratedArtifact,
                requested_bytes: u64::MAX,
                used_bytes: total,
                limit_bytes: budget
                    .budget()
                    .pool_limit_bytes(super::StorageBudgetPool::GeneratedArtifact),
                requested_count: 0,
                used_count: 0,
                limit_count: None,
                reason: "checkpoint recovery decode byte count overflowed",
            }
        })?;
        total
            .checked_add(payload_len)
            .ok_or(LifecycleError::StorageBudgetExceeded {
                pool: super::StorageBudgetPool::GeneratedArtifact,
                requested_bytes: payload_len,
                used_bytes: total,
                limit_bytes: budget
                    .budget()
                    .pool_limit_bytes(super::StorageBudgetPool::GeneratedArtifact),
                requested_count: 0,
                used_count: 0,
                limit_count: None,
                reason: "checkpoint recovery decode byte count overflowed",
            })
    })?;
    require_generated_artifact_budget(
        budget,
        bytes,
        "checkpoint recovery decode exceeds storage budget",
    )
}

impl LifecycleRecoveryRequest {
    pub(crate) fn new(
        strictness: RecoveryStrictness,
        max_faults: usize,
        max_snapshot_sections: usize,
        checkpoint_identity_seed: impl Into<String>,
    ) -> LifecycleResult<Self> {
        let request = Self {
            strictness,
            max_faults,
            max_snapshot_sections,
            checkpoint_identity_seed: TableIdentity::new(checkpoint_identity_seed.into())
                .map_err(table_runtime_error)?,
            table_objects: Vec::new(),
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn from_open_plan(plan: &StorageOpenPlan) -> LifecycleResult<Self> {
        Self::new(
            plan.recovery_policy(),
            plan.lifecycle_config().max_recovery_faults(),
            4096,
            "lifecycle-checkpoint",
        )
    }

    pub(crate) const fn strictness(&self) -> RecoveryStrictness {
        self.strictness
    }

    pub(crate) const fn max_faults(&self) -> usize {
        self.max_faults
    }

    pub(crate) const fn max_snapshot_sections(&self) -> usize {
        self.max_snapshot_sections
    }

    pub(crate) const fn checkpoint_identity_seed(&self) -> &TableIdentity {
        &self.checkpoint_identity_seed
    }

    pub(crate) fn table_objects(&self) -> &[LifecycleRecoveryTableObject] {
        &self.table_objects
    }

    #[allow(
        dead_code,
        reason = "table-object recovery references are introduced before table-backed checkpoint sections"
    )]
    pub(crate) fn with_table_objects(
        mut self,
        table_objects: Vec<LifecycleRecoveryTableObject>,
    ) -> Self {
        self.table_objects = table_objects;
        self
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.max_faults == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_faults",
                reason: "must be nonzero",
            });
        }
        if self.max_snapshot_sections == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_snapshot_sections",
                reason: "must be nonzero",
            });
        }
        if !self.table_objects.is_empty() {
            return Err(LifecycleError::RecoveryFailed {
                reason: "table object recovery references require a table manifest",
            });
        }
        Ok(())
    }

    fn validate_against_plan(&self, plan: &StorageOpenPlan) -> LifecycleResult<()> {
        self.validate()?;
        if self.strictness == RecoveryStrictness::AllowExplicitLossyFallback
            && plan.recovery_policy() != RecoveryStrictness::AllowExplicitLossyFallback
        {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "lossy recovery request requires lossy open plan",
            });
        }
        Ok(())
    }
}

impl LifecycleRecoveryTableObject {
    #[allow(
        dead_code,
        reason = "table-object recovery references are introduced before table-backed checkpoint sections"
    )]
    pub(crate) const fn new(identity: TableIdentity, facts: TableObjectFacts) -> Self {
        Self { identity, facts }
    }
}

impl LifecycleRecoveryOutcome {
    pub(crate) const fn health(&self) -> &RecoveryHealth {
        &self.health
    }

    pub(crate) const fn checkpoint(&self) -> &LifecycleRecoveredCheckpoint {
        &self.checkpoint
    }

    pub(crate) const fn wal(&self) -> &LifecycleRecoveredWal {
        &self.wal
    }

    pub(crate) const fn quarantine(&self) -> &LifecycleRecoveredQuarantine {
        &self.quarantine
    }

    pub(crate) const fn tables(&self) -> &LifecycleRecoveredTables {
        &self.tables
    }
}

impl LifecycleRecoveredCheckpoint {
    const fn empty() -> Self {
        Self {
            snapshot_id: None,
            trusted_watermark: None,
            section_count: 0,
            row_count: 0,
            install_outcome: None,
        }
    }

    const fn missing_lossy(snapshot_id: u64) -> Self {
        Self {
            snapshot_id: Some(snapshot_id),
            trusted_watermark: None,
            section_count: 0,
            row_count: 0,
            install_outcome: None,
        }
    }

    pub(crate) const fn snapshot_id(&self) -> Option<u64> {
        self.snapshot_id
    }

    pub(crate) const fn trusted_watermark(&self) -> Option<CommitVersion> {
        self.trusted_watermark
    }

    pub(crate) const fn section_count(&self) -> usize {
        self.section_count
    }

    pub(crate) const fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) const fn install_outcome(&self) -> Option<&BranchSnapshotInstallOutcome> {
        self.install_outcome.as_ref()
    }

    pub(crate) fn timestamp_max(&self) -> Option<Timestamp> {
        self.install_outcome().and_then(|outcome| {
            outcome
                .branch_outcomes()
                .iter()
                .filter_map(crate::branch::BranchSnapshotInstallBranchOutcome::timestamp_max)
                .max()
        })
    }
}

impl LifecycleRecoveredWal {
    pub(crate) const fn replay_start(&self) -> CommitVersion {
        self.replay_start
    }

    pub(crate) fn records(&self) -> &[WalRecord] {
        &self.records
    }

    pub(crate) const fn truncation(&self) -> Option<&WalTruncation> {
        self.truncation.as_ref()
    }

    pub(crate) const fn repair(&self) -> Option<&WalRepair> {
        self.repair.as_ref()
    }
}

impl LifecycleRecoveredQuarantine {
    const fn unknown(object: Option<ObjectName>) -> Self {
        Self {
            object,
            present: false,
            byte_count: 0,
            entry_count: 0,
        }
    }

    pub(crate) const fn object(&self) -> Option<&ObjectName> {
        self.object.as_ref()
    }

    pub(crate) const fn is_present(&self) -> bool {
        self.present
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub(crate) const fn entry_count(&self) -> usize {
        self.entry_count
    }
}

impl LifecycleRecoveredTables {
    #[allow(
        dead_code,
        reason = "table-manifest recovery tests inspect validated legacy counters"
    )]
    pub(crate) fn validated(&self) -> &[LifecycleRecoveredTable] {
        &self.validated
    }

    pub(crate) const fn validated_count(&self) -> usize {
        self.validated.len()
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery facts are consumed by lifecycle tests"
    )]
    pub(crate) const fn table_manifest(&self) -> &LifecycleTableManifestRecoveryOutcome {
        &self.table_manifest
    }
}

impl LifecycleRecoveredTable {
    #[allow(
        dead_code,
        reason = "table-manifest recovery tests inspect validated legacy counters"
    )]
    pub(crate) const fn identity(&self) -> &TableIdentity {
        &self.identity
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery tests inspect validated legacy counters"
    )]
    pub(crate) const fn facts(&self) -> &TableObjectFacts {
        &self.facts
    }
}

pub(crate) fn encode_checkpoint_row_section(
    rows: &[StorageRow],
) -> Result<SnapshotSection, FormatError> {
    let row_count = u32::try_from(rows.len()).map_err(|_| FormatError::InvalidLength {
        field: "snapshot_row_count",
    })?;
    let mut payload = Vec::new();
    payload.extend_from_slice(&SNAPSHOT_ROWS_MAGIC);
    payload.extend_from_slice(&SNAPSHOT_ROWS_VERSION.to_le_bytes());
    payload.extend_from_slice(&row_count.to_le_bytes());
    for row in rows {
        let row_bytes = encode_storage_row(row)?;
        let row_len = u32::try_from(row_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "snapshot_row_len",
        })?;
        payload.extend_from_slice(&row_len.to_le_bytes());
        payload.extend_from_slice(&row_bytes);
    }
    SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, payload)
}

fn decode_checkpoint_rows(sections: &[SnapshotSection]) -> LifecycleResult<Vec<StorageRow>> {
    let mut rows = Vec::new();
    for section in sections {
        if section.section_kind() != SNAPSHOT_ROW_SECTION_KIND {
            continue;
        }
        rows.extend(decode_checkpoint_row_payload(section.payload())?);
    }
    Ok(rows)
}

fn decode_checkpoint_row_payload(payload: &[u8]) -> LifecycleResult<Vec<StorageRow>> {
    if payload.len() < SNAPSHOT_ROWS_HEADER_SIZE {
        return Err(format_error(FormatError::InsufficientBytes {
            format: SNAPSHOT_ROWS_FORMAT,
            needed: SNAPSHOT_ROWS_HEADER_SIZE,
            actual: payload.len(),
        }));
    }
    if payload[..4] != SNAPSHOT_ROWS_MAGIC {
        return Err(format_error(FormatError::InvalidMagic {
            format: SNAPSHOT_ROWS_FORMAT,
        }));
    }
    let version = u32::from_le_bytes(
        payload[4..8]
            .try_into()
            .map_err(|_| format_error(FormatError::InvalidLength { field: "version" }))?,
    );
    if version != SNAPSHOT_ROWS_VERSION {
        return Err(format_error(FormatError::FutureFormat {
            format: SNAPSHOT_ROWS_FORMAT,
            version,
            max_supported: SNAPSHOT_ROWS_VERSION,
        }));
    }
    let row_count =
        usize::try_from(u32::from_le_bytes(payload[8..12].try_into().map_err(
            |_| format_error(FormatError::InvalidLength { field: "row_count" }),
        )?))
        .map_err(|_| format_error(FormatError::InvalidLength { field: "row_count" }))?;
    let max_possible_rows = (payload.len() - SNAPSHOT_ROWS_HEADER_SIZE) / 4;
    if row_count > max_possible_rows {
        return Err(format_error(FormatError::InsufficientBytes {
            format: SNAPSHOT_ROWS_FORMAT,
            needed: SNAPSHOT_ROWS_HEADER_SIZE.saturating_add(row_count.saturating_mul(4)),
            actual: payload.len(),
        }));
    }
    let mut rows = Vec::new();
    let mut cursor = SNAPSHOT_ROWS_HEADER_SIZE;
    for _ in 0..row_count {
        let len_end = cursor
            .checked_add(4)
            .ok_or_else(|| format_error(FormatError::InvalidLength { field: "row_len" }))?;
        if payload.len() < len_end {
            return Err(format_error(FormatError::InsufficientBytes {
                format: SNAPSHOT_ROWS_FORMAT,
                needed: len_end,
                actual: payload.len(),
            }));
        }
        let row_len = usize::try_from(u32::from_le_bytes(
            payload[cursor..len_end]
                .try_into()
                .map_err(|_| format_error(FormatError::InvalidLength { field: "row_len" }))?,
        ))
        .map_err(|_| format_error(FormatError::InvalidLength { field: "row_len" }))?;
        cursor = len_end;
        let row_end = cursor
            .checked_add(row_len)
            .ok_or_else(|| format_error(FormatError::InvalidLength { field: "row_len" }))?;
        if payload.len() < row_end {
            return Err(format_error(FormatError::InsufficientBytes {
                format: SNAPSHOT_ROWS_FORMAT,
                needed: row_end,
                actual: payload.len(),
            }));
        }
        rows.push(decode_storage_row(&payload[cursor..row_end]).map_err(format_error)?);
        cursor = row_end;
    }
    if cursor != payload.len() {
        return Err(format_error(FormatError::TrailingData {
            format: SNAPSHOT_ROWS_FORMAT,
            remaining: payload.len() - cursor,
        }));
    }
    Ok(rows)
}

struct CheckpointInstall {
    recovered_branch: Option<crate::branch::BranchLocalState>,
    outcome: BranchSnapshotInstallOutcome,
}

struct CheckpointRecovery {
    checkpoint: LifecycleRecoveredCheckpoint,
    recovered_branch: Option<crate::branch::BranchLocalState>,
}

impl CheckpointRecovery {
    const fn empty() -> Self {
        Self {
            checkpoint: LifecycleRecoveredCheckpoint::empty(),
            recovered_branch: None,
        }
    }

    const fn missing_lossy(snapshot_id: u64) -> Self {
        Self {
            checkpoint: LifecycleRecoveredCheckpoint::missing_lossy(snapshot_id),
            recovered_branch: None,
        }
    }
}

fn install_checkpoint_rows(
    current_branch: crate::branch::BranchLocalState,
    identity_seed: &TableIdentity,
    rows: Vec<StorageRow>,
) -> LifecycleResult<CheckpointInstall> {
    let branch_id = current_branch.branch_id();
    let mut branches = vec![current_branch];
    let request = BranchSnapshotInstallRequest::from_rows(identity_seed.as_str(), rows)
        .map_err(branch_error)?;
    let outcome =
        install_snapshot_rows_into_branches(&mut branches, &request).map_err(branch_error)?;
    let recovered_branch = branches
        .into_iter()
        .find(|branch| branch.branch_id() == branch_id);
    Ok(CheckpointInstall {
        recovered_branch,
        outcome,
    })
}

fn trusted_replay_start(
    checkpoint_watermark: Option<CommitVersion>,
    flush_watermark: Option<CommitVersion>,
) -> CommitVersion {
    checkpoint_watermark
        .into_iter()
        .chain(flush_watermark)
        .max()
        .unwrap_or(CommitVersion::ZERO)
}

fn validate_flush_watermark_is_recoverable(
    flush_watermark: Option<CommitVersion>,
    checkpoint: &LifecycleRecoveredCheckpoint,
    tables: &LifecycleRecoveredTables,
    staged_table_manifest_branch: Option<&crate::branch::BranchLocalState>,
    strictness: RecoveryStrictness,
    faults: &mut Vec<RecoveryFault>,
    max_faults: usize,
) -> LifecycleResult<Option<CommitVersion>> {
    if let Some(flush_watermark) = flush_watermark {
        if checkpoint
            .trusted_watermark()
            .is_some_and(|watermark| flush_watermark <= watermark)
        {
            return Ok(Some(flush_watermark));
        }
        if table_manifest_covers_flush_watermark(
            flush_watermark,
            checkpoint.trusted_watermark(),
            tables,
            staged_table_manifest_branch,
        ) {
            return Ok(Some(flush_watermark));
        }
        if checkpoint.snapshot_id().is_some()
            && checkpoint.trusted_watermark().is_none()
            && strictness == RecoveryStrictness::AllowExplicitLossyFallback
        {
            push_fault(
                faults,
                max_faults,
                RecoveryFaultKind::MissingSnapshotObject,
                "manifest flush watermark lost with missing snapshot",
            )?;
            return Ok(None);
        }
        if checkpoint
            .trusted_watermark()
            .is_none_or(|watermark| flush_watermark > watermark)
        {
            return Err(LifecycleError::RecoveryFailed {
                reason: "manifest flush watermark requires recovered flushed table state",
            });
        }
    }
    Ok(None)
}

fn table_manifest_covers_flush_watermark(
    flush_watermark: CommitVersion,
    checkpoint_watermark: Option<CommitVersion>,
    tables: &LifecycleRecoveredTables,
    staged_branch: Option<&crate::branch::BranchLocalState>,
) -> bool {
    let Some(checkpoint_watermark) = checkpoint_watermark else {
        return false;
    };
    if tables
        .table_manifest()
        .install_outcome()
        .and_then(crate::branch::BranchTableManifestRecoveryOutcome::max_commit_version)
        .is_none_or(|max_commit_version| max_commit_version < flush_watermark)
    {
        return false;
    }
    staged_branch.is_some_and(|branch| {
        branch_durable_rows_cover_interval(branch, checkpoint_watermark, flush_watermark)
    })
}

fn manifest_snapshot_watermark(
    facts: &super::LifecycleDurableAssemblyFacts,
) -> LifecycleResult<Option<CommitVersion>> {
    facts
        .manifest_snapshot_watermark()
        .map(|watermark| {
            let version = CommitVersion::new(watermark);
            if version == CommitVersion::ZERO {
                return Err(LifecycleError::RecoveryFailed {
                    reason: "manifest snapshot watermark must be nonzero",
                });
            }
            Ok(version)
        })
        .transpose()
}

fn validate_snapshot_watermark(
    container: &SnapshotContainer,
    expected: CommitVersion,
) -> LifecycleResult<()> {
    if container.header().watermark_commit_version() != expected {
        return Err(LifecycleError::RecoveryFailed {
            reason: "snapshot watermark does not match database manifest",
        });
    }
    Ok(())
}

fn validate_checkpoint_rows(
    watermark: CommitVersion,
    open_branch_id: strata_core_next::BranchId,
    rows: &[StorageRow],
) -> LifecycleResult<()> {
    for row in rows {
        if row.commit_version() > watermark {
            return Err(LifecycleError::RecoveryFailed {
                reason: "checkpoint row commit version exceeds snapshot watermark",
            });
        }
        if row.physical_key().branch_id() != open_branch_id {
            return Err(LifecycleError::RecoveryFailed {
                reason: "checkpoint contains rows for unopened branch",
            });
        }
    }
    Ok(())
}

fn recovery_health_from_faults(
    request: &LifecycleRecoveryRequest,
    faults: Vec<RecoveryFault>,
) -> LifecycleResult<RecoveryHealth> {
    if faults.is_empty() {
        return Ok(RecoveryHealth::Healthy);
    }
    if request.strictness() == RecoveryStrictness::Strict {
        return Err(LifecycleError::RecoveryFailed {
            reason: "strict recovery cannot return degraded health",
        });
    }
    let class = degradation_class_for_faults(&faults);
    RecoveryHealth::degraded(class, faults)
}

fn degradation_class_for_faults(faults: &[RecoveryFault]) -> RecoveryDegradationClass {
    if faults.iter().any(|fault| {
        matches!(
            fault.kind(),
            RecoveryFaultKind::CorruptManifest
                | RecoveryFaultKind::MissingSnapshotObject
                | RecoveryFaultKind::MissingTableObject
                | RecoveryFaultKind::InheritedLayerLoss
                | RecoveryFaultKind::NoManifestFallback
                | RecoveryFaultKind::WalTailRepairFailed
        )
    }) {
        RecoveryDegradationClass::DataLoss
    } else if faults
        .iter()
        .any(|fault| matches!(fault.kind(), RecoveryFaultKind::QuarantineInventoryMismatch))
    {
        RecoveryDegradationClass::Telemetry
    } else {
        RecoveryDegradationClass::PolicyDowngrade
    }
}

fn is_lossy_table_manifest_recovery_error(error: &LifecycleError) -> bool {
    matches!(
        error,
        LifecycleError::TableManifestRecoveryMismatch { .. }
            | LifecycleError::TableManifestBranchInstallFailed { .. }
    )
}

fn table_manifest_recovery_fault_kind(error: &LifecycleError) -> RecoveryFaultKind {
    match error {
        LifecycleError::TableManifestRecoveryMismatch { reason, .. }
            if *reason == "table manifest listed table object is missing"
                || *reason == "table manifest facts do not match table object"
                || *reason == "table manifest listed table object failed validation" =>
        {
            RecoveryFaultKind::MissingTableObject
        }
        LifecycleError::TableManifestBranchInstallFailed { .. } => {
            RecoveryFaultKind::InheritedLayerLoss
        }
        _ => RecoveryFaultKind::CorruptManifest,
    }
}

fn table_manifest_recovery_fault_reason(error: &LifecycleError) -> &'static str {
    match error {
        LifecycleError::TableManifestRecoveryMismatch { reason, .. }
        | LifecycleError::TableManifestBranchInstallFailed { reason, .. } => reason,
        _ => "table manifest recovery failed",
    }
}

fn push_fault(
    faults: &mut Vec<RecoveryFault>,
    max_faults: usize,
    kind: RecoveryFaultKind,
    reason: &'static str,
) -> LifecycleResult<()> {
    if faults.len() == max_faults {
        return Err(LifecycleError::RecoveryFailed {
            reason: "recovery fault limit exceeded",
        });
    }
    faults.push(RecoveryFault::new(kind, reason)?);
    Ok(())
}

fn push_fault_for_branch(
    faults: &mut Vec<RecoveryFault>,
    max_faults: usize,
    kind: RecoveryFaultKind,
    reason: &'static str,
    branch_id: strata_core_next::BranchId,
) -> LifecycleResult<()> {
    if faults.len() == max_faults {
        return Err(LifecycleError::RecoveryFailed {
            reason: "recovery fault limit exceeded",
        });
    }
    faults.push(RecoveryFault::new(kind, reason)?.with_affected_branch(branch_id));
    Ok(())
}

fn is_quarantine_inventory_mismatch(source: &QuarantineServiceError) -> bool {
    matches!(
        source,
        QuarantineServiceError::Decode { .. }
            | QuarantineServiceError::DatabaseMismatch { .. }
            | QuarantineServiceError::BranchMismatch { .. }
            | QuarantineServiceError::CodecMismatch { .. }
    )
}

fn quarantine_error_object(source: &QuarantineServiceError) -> Option<ObjectName> {
    match source {
        QuarantineServiceError::Decode { object, .. }
        | QuarantineServiceError::DatabaseMismatch { object, .. }
        | QuarantineServiceError::BranchMismatch { object, .. }
        | QuarantineServiceError::CodecMismatch { object, .. } => Some(object.clone()),
        _ => None,
    }
}

fn format_error(source: FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Format,
        "snapshot section decode failed",
        source,
    )
}

fn snapshot_error(source: SnapshotServiceError) -> LifecycleError {
    let reason = match source {
        SnapshotServiceError::Missing { .. } => "manifest-listed snapshot is missing",
        SnapshotServiceError::Decode { .. } | SnapshotServiceError::Visit { .. } => {
            "snapshot decode failed"
        }
        SnapshotServiceError::CodecMismatch { .. } => "snapshot codec mismatch",
        SnapshotServiceError::DatabaseMismatch { .. } => "snapshot database mismatch",
        SnapshotServiceError::SnapshotIdMismatch { .. } => "snapshot id mismatch",
        _ => "snapshot service failed",
    };
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, reason, source)
}

fn wal_error(source: WalServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "WAL recovery read failed",
        source,
    )
}

fn wal_repair_error(source: WalServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "WAL latest-tail repair failed",
        source,
    )
}

fn quarantine_error(source: QuarantineServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "quarantine inventory recovery failed",
        source,
    )
}

fn branch_error(source: crate::branch::BranchRuntimeError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "checkpoint install failed",
        source,
    )
}

fn table_runtime_error(source: crate::table::TableRuntimeError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::TableRuntime,
        "invalid recovery table identity",
        source,
    )
}
