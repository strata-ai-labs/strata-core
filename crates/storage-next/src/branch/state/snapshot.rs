//! Snapshot row extraction and branch snapshot installation.

use super::{branch_reachable_table_identity_exists, BranchLocalState};
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::{BranchLevel, BranchTableDescriptor, InheritedLayerStatus};
use crate::branch::identity::{require_row_branch, rewrite_row_branch};
use crate::branch::read::{try_for_each_reader_row, BranchOwnedTable};
use crate::row::StorageRow;
use crate::table::{
    ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig, TableIdentity,
    TableInternalKeyBytes, TableReaderConfig, TableRuntimeError,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE: usize = 4_096;
const _: () = assert!(SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE > 0);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchSnapshotMissingBranchPolicy {
    Reject,
    Create { config: BranchRuntimeConfig },
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallGroup {
    branch_id: BranchId,
    rows: Vec<StorageRow>,
}

impl BranchSnapshotInstallGroup {
    pub(crate) fn new(branch_id: BranchId, rows: Vec<StorageRow>) -> Self {
        Self { branch_id, rows }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn rows(&self) -> &[StorageRow] {
        &self.rows
    }
}

impl fmt::Debug for BranchSnapshotInstallGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BranchSnapshotInstallGroup")
            .field("branch_id", &self.branch_id)
            .field("row_count", &self.rows.len())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallRequest {
    output_identity_seed: TableIdentity,
    missing_branch_policy: BranchSnapshotMissingBranchPolicy,
    table_builder_config: TableBuilderConfig,
    max_rows_per_table: usize,
    groups: Vec<BranchSnapshotInstallGroup>,
}

impl BranchSnapshotInstallRequest {
    pub(crate) fn new(
        output_identity_seed: impl Into<String>,
        groups: Vec<BranchSnapshotInstallGroup>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_seed = TableIdentity::new(output_identity_seed.into())
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        Ok(Self {
            output_identity_seed,
            missing_branch_policy: BranchSnapshotMissingBranchPolicy::Reject,
            table_builder_config: TableBuilderConfig::default(),
            max_rows_per_table: SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE,
            groups,
        })
    }

    pub(crate) fn from_rows(
        output_identity_seed: impl Into<String>,
        rows: Vec<StorageRow>,
    ) -> BranchRuntimeResult<Self> {
        let mut groups = Vec::<BranchSnapshotInstallGroup>::new();
        for row in rows {
            let branch_id = row.physical_key().branch_id();
            if let Some(group) = groups.iter_mut().find(|group| group.branch_id == branch_id) {
                group.rows.push(row);
            } else {
                groups.push(BranchSnapshotInstallGroup::new(branch_id, vec![row]));
            }
        }
        groups.sort_by_key(|group| *group.branch_id.as_bytes());
        for group in &mut groups {
            group.rows.sort_by_key(TableInternalKeyBytes::from_row);
        }
        Self::new(output_identity_seed, groups)
    }

    pub(crate) const fn output_identity_seed(&self) -> &TableIdentity {
        &self.output_identity_seed
    }

    pub(crate) const fn missing_branch_policy(&self) -> BranchSnapshotMissingBranchPolicy {
        self.missing_branch_policy
    }

    pub(crate) const fn table_builder_config(&self) -> TableBuilderConfig {
        self.table_builder_config
    }

    pub(crate) const fn max_rows_per_table(&self) -> usize {
        self.max_rows_per_table
    }

    pub(crate) fn groups(&self) -> &[BranchSnapshotInstallGroup] {
        &self.groups
    }

    pub(crate) fn with_missing_branch_policy(
        mut self,
        missing_branch_policy: BranchSnapshotMissingBranchPolicy,
    ) -> Self {
        self.missing_branch_policy = missing_branch_policy;
        self
    }

    pub(crate) fn with_table_builder_config(
        mut self,
        table_builder_config: TableBuilderConfig,
    ) -> Self {
        self.table_builder_config = table_builder_config;
        self
    }

    pub(crate) fn with_max_rows_per_table(
        mut self,
        max_rows_per_table: usize,
    ) -> BranchRuntimeResult<Self> {
        if max_rows_per_table == 0 {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "max rows per output table must be nonzero",
            });
        }
        self.max_rows_per_table = max_rows_per_table;
        Ok(self)
    }
}

impl fmt::Debug for BranchSnapshotInstallRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BranchSnapshotInstallRequest")
            .field("output_identity_seed", &self.output_identity_seed)
            .field("missing_branch_policy", &self.missing_branch_policy)
            .field("table_builder_config", &self.table_builder_config)
            .field("max_rows_per_table", &self.max_rows_per_table)
            .field("groups", &self.groups)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallOutcome {
    rows_installed: u64,
    tables_created: usize,
    branches_created: usize,
    branches_replaced: usize,
    timestamp_max: Option<Timestamp>,
    table_identities: Vec<TableIdentity>,
}

impl BranchSnapshotInstallOutcome {
    fn empty() -> Self {
        Self {
            rows_installed: 0,
            tables_created: 0,
            branches_created: 0,
            branches_replaced: 0,
            timestamp_max: None,
            table_identities: Vec::new(),
        }
    }

    fn installed(staged: &[StagedSnapshotBranch]) -> Self {
        let rows_installed = staged.iter().map(|branch| branch.rows_installed).sum();
        let tables_created = staged.iter().map(|branch| branch.tables_created).sum();
        let branches_created = staged.iter().filter(|branch| branch.branch_created).count();
        let branches_replaced = staged.len().saturating_sub(branches_created);
        let timestamp_max = staged
            .iter()
            .filter_map(|branch| branch.timestamp_max)
            .max();
        let table_identities = staged
            .iter()
            .flat_map(|branch| branch.table_identities.iter().cloned())
            .collect();
        Self {
            rows_installed,
            tables_created,
            branches_created,
            branches_replaced,
            timestamp_max,
            table_identities,
        }
    }

    pub(crate) const fn is_empty_plan_noop(&self) -> bool {
        self.rows_installed == 0
            && self.tables_created == 0
            && self.branches_created == 0
            && self.branches_replaced == 0
    }

    pub(crate) const fn rows_installed(&self) -> u64 {
        self.rows_installed
    }

    pub(crate) const fn tables_created(&self) -> usize {
        self.tables_created
    }

    pub(crate) const fn branches_created(&self) -> usize {
        self.branches_created
    }

    pub(crate) const fn branches_replaced(&self) -> usize {
        self.branches_replaced
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }

    pub(crate) fn table_identities(&self) -> &[TableIdentity] {
        &self.table_identities
    }
}

#[derive(Clone)]
struct StagedSnapshotBranch {
    branch_id: BranchId,
    state: BranchLocalState,
    branch_created: bool,
    rows_installed: u64,
    tables_created: usize,
    timestamp_max: Option<Timestamp>,
    table_identities: Vec<TableIdentity>,
}

pub(crate) fn install_snapshot_rows_into_branches(
    branches: &mut Vec<BranchLocalState>,
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<BranchSnapshotInstallOutcome> {
    validate_snapshot_request(request)?;
    validate_branch_state_set(branches)?;
    if request.groups().is_empty() {
        return Ok(BranchSnapshotInstallOutcome::empty());
    }

    let staged = stage_snapshot_install_branches(branches, request)?;
    let outcome = BranchSnapshotInstallOutcome::installed(&staged);

    let mut replacement = branches.clone();
    for staged_branch in staged {
        if let Some(index) = replacement
            .iter()
            .position(|state| state.branch_id() == staged_branch.branch_id)
        {
            replacement[index] = staged_branch.state;
        } else {
            replacement.push(staged_branch.state);
        }
    }
    *branches = replacement;

    Ok(outcome)
}

impl BranchLocalState {
    /// The checkpoint snapshot's rows: a bounded **delta** of the not-yet-durable
    /// state — the active memtable and frozen tables only. Owned-level rows are
    /// already durable in their table objects (recorded in the table manifest) and
    /// are reconstructed from the manifest at recovery, which combines manifest
    /// owned levels + this delta + WAL replay; materializing them here again would
    /// make the snapshot O(database size) and can exceed the snapshot decode
    /// ceiling. Every delta row has `commit_version > flush_watermark`, so at
    /// recovery a delta row is strictly newer than any manifest-resident owned row
    /// at the same physical key.
    pub(crate) fn checkpoint_rows(
        &self,
        watermark: CommitVersion,
    ) -> BranchRuntimeResult<Vec<StorageRow>> {
        if watermark == CommitVersion::ZERO {
            return Ok(Vec::new());
        }
        if !self.inherited_layers.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "checkpoint requires inherited layers to be materialized first",
            });
        }

        let mut rows = Vec::new();
        for row in self.active.iter() {
            push_checkpoint_row(self.branch_id, watermark, row.row(), &mut rows)?;
        }
        for table in &self.frozen {
            for row in table.iter() {
                push_checkpoint_row(self.branch_id, watermark, row.row(), &mut rows)?;
            }
        }

        rows.sort_by_key(TableInternalKeyBytes::from_row);
        validate_checkpoint_rows(&rows)?;
        Ok(rows)
    }

    pub(crate) fn fork_snapshot_rows(
        &self,
        watermark: CommitVersion,
        target_branch_id: BranchId,
    ) -> BranchRuntimeResult<Vec<StorageRow>> {
        if watermark == CommitVersion::ZERO {
            return Ok(Vec::new());
        }

        let mut rows_by_key = BTreeMap::<TableInternalKeyBytes, StorageRow>::new();
        for row in self.active.iter() {
            insert_own_fork_snapshot_row(
                &mut rows_by_key,
                self.branch_id,
                target_branch_id,
                watermark,
                row.row(),
            )?;
        }
        for table in &self.frozen {
            for row in table.iter() {
                insert_own_fork_snapshot_row(
                    &mut rows_by_key,
                    self.branch_id,
                    target_branch_id,
                    watermark,
                    row.row(),
                )?;
            }
        }
        for table in self.owned_levels().iter().flatten() {
            try_for_each_reader_row(table.reader(), |row| {
                insert_own_fork_snapshot_row(
                    &mut rows_by_key,
                    self.branch_id,
                    target_branch_id,
                    watermark,
                    row.row(),
                )
            })?;
        }
        for layer in &self.inherited_layers {
            match layer.status() {
                InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
                InheritedLayerStatus::Materialized => continue,
                InheritedLayerStatus::Unavailable => {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "unavailable inherited layers cannot be snapshotted for fork",
                    });
                }
            }
            let inherited_watermark = watermark.min(layer.fork_version());
            for table in layer.owned_levels().iter().flatten() {
                try_for_each_reader_row(table.reader(), |row| {
                    insert_lower_precedence_fork_snapshot_row(
                        &mut rows_by_key,
                        layer.source_branch_id(),
                        target_branch_id,
                        inherited_watermark,
                        row.row(),
                    )
                })?;
            }
        }

        let rows = rows_by_key.into_values().collect::<Vec<_>>();
        validate_checkpoint_rows(&rows)?;
        Ok(rows)
    }
}

fn validate_snapshot_request(request: &BranchSnapshotInstallRequest) -> BranchRuntimeResult<()> {
    request
        .table_builder_config()
        .validate()
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
    if request.max_rows_per_table() == 0 {
        return Err(BranchRuntimeError::InvalidSnapshotInstall {
            reason: "max rows per output table must be nonzero",
        });
    }
    let mut group_branches = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
    let mut internal_keys = BTreeSet::<TableInternalKeyBytes>::new();
    let mut previous_branch = None::<[u8; BranchId::BYTE_LEN]>;
    for group in request.groups() {
        let branch_bytes = *group.branch_id().as_bytes();
        if previous_branch.is_some_and(|previous| branch_bytes < previous) {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must be sorted by branch id",
            });
        }
        previous_branch = Some(branch_bytes);
        if !group_branches.insert(*group.branch_id().as_bytes()) {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must be unique",
            });
        }
        if group.rows().is_empty() {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must not be empty",
            });
        }
        let mut previous_key = None::<TableInternalKeyBytes>;
        for row in group.rows() {
            require_row_branch(group.branch_id(), row)?;
            let key = TableInternalKeyBytes::from_row(row);
            if let Some(previous) = &previous_key {
                if key == *previous {
                    return Err(BranchRuntimeError::TableRuntime {
                        source: TableRuntimeError::DuplicateInternalKey {
                            key: key.as_slice().to_vec(),
                        },
                    });
                }
                if key < *previous {
                    return Err(BranchRuntimeError::InvalidSnapshotInstall {
                        reason: "snapshot install rows must be strictly sorted by internal key",
                    });
                }
            }
            if !internal_keys.insert(key.clone()) {
                return Err(BranchRuntimeError::TableRuntime {
                    source: TableRuntimeError::DuplicateInternalKey {
                        key: key.as_slice().to_vec(),
                    },
                });
            }
            previous_key = Some(key);
        }
    }
    Ok(())
}

fn validate_branch_state_set(branches: &[BranchLocalState]) -> BranchRuntimeResult<()> {
    let mut seen = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
    for branch in branches {
        if !seen.insert(*branch.branch_id().as_bytes()) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch state set must not contain duplicate branches",
            });
        }
    }
    Ok(())
}

fn push_checkpoint_row(
    branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
    rows: &mut Vec<StorageRow>,
) -> BranchRuntimeResult<()> {
    require_row_branch(branch_id, row)?;
    if row.commit_version() <= watermark {
        rows.push(row.clone());
    }
    Ok(())
}

fn insert_own_fork_snapshot_row(
    rows_by_key: &mut BTreeMap<TableInternalKeyBytes, StorageRow>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
) -> BranchRuntimeResult<()> {
    if row.commit_version() > watermark {
        return Ok(());
    }
    let rewritten = rewrite_row_branch(row, source_branch_id, target_branch_id)?;
    let key = TableInternalKeyBytes::from_row(&rewritten);
    if rows_by_key.insert(key, rewritten).is_some() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "fork snapshot own rows must not contain duplicate internal keys",
        });
    }
    Ok(())
}

fn insert_lower_precedence_fork_snapshot_row(
    rows_by_key: &mut BTreeMap<TableInternalKeyBytes, StorageRow>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
) -> BranchRuntimeResult<()> {
    if row.commit_version() > watermark {
        return Ok(());
    }
    let rewritten = rewrite_row_branch(row, source_branch_id, target_branch_id)?;
    rows_by_key
        .entry(TableInternalKeyBytes::from_row(&rewritten))
        .or_insert(rewritten);
    Ok(())
}

fn validate_checkpoint_rows(rows: &[StorageRow]) -> BranchRuntimeResult<()> {
    let mut previous = None::<TableInternalKeyBytes>;
    for row in rows {
        let key = TableInternalKeyBytes::from_row(row);
        if previous.as_ref().is_some_and(|previous| previous >= &key) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "checkpoint rows must be strictly sorted by internal key",
            });
        }
        previous = Some(key);
    }
    Ok(())
}

fn stage_snapshot_install_branches(
    branches: &[BranchLocalState],
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<Vec<StagedSnapshotBranch>> {
    let mut output_identities = BTreeSet::<String>::new();
    let mut staged = Vec::with_capacity(request.groups().len());
    for group in request.groups() {
        let (mut state, branch_created) =
            snapshot_target_state(branches, group.branch_id(), request)?;
        let tables = build_snapshot_l0_tables(
            group.branch_id(),
            request.output_identity_seed(),
            request.table_builder_config(),
            request.max_rows_per_table(),
            group.rows(),
        )?;
        let table_identities = tables
            .iter()
            .map(|table| table.descriptor().identity().clone())
            .collect::<Vec<_>>();
        for identity in &table_identities {
            if !output_identities.insert(identity.as_str().to_owned()) {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason: "snapshot output identities must be unique",
                });
            }
            if branches.iter().any(|branch| {
                branch_reachable_table_identity_exists(
                    identity,
                    branch.owned_levels(),
                    &branch.inherited_layers,
                )
            }) {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason:
                        "snapshot output identity must not collide with existing reachable table",
                });
            }
        }
        for table in tables.into_iter().rev() {
            state.install_l0_table(table)?;
        }
        state.reachability_snapshot()?;
        let facts = state.facts()?;
        let rows_installed = u64::try_from(group.rows().len()).map_err(|_| {
            BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install row count must fit in u64",
            }
        })?;
        staged.push(StagedSnapshotBranch {
            branch_id: group.branch_id(),
            state,
            branch_created,
            rows_installed,
            tables_created: table_identities.len(),
            timestamp_max: facts.timestamp_max(),
            table_identities,
        });
    }
    Ok(staged)
}

fn snapshot_target_state(
    branches: &[BranchLocalState],
    branch_id: BranchId,
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<(BranchLocalState, bool)> {
    match branches.iter().find(|state| state.branch_id() == branch_id) {
        Some(existing) => {
            if !existing.is_empty() {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason: "snapshot install target branch must be empty",
                });
            }
            Ok((BranchLocalState::new(branch_id, existing.config())?, false))
        }
        None => match request.missing_branch_policy() {
            BranchSnapshotMissingBranchPolicy::Reject => {
                Err(BranchRuntimeError::BranchNotFound { branch_id })
            }
            BranchSnapshotMissingBranchPolicy::Create { config } => {
                Ok((BranchLocalState::new(branch_id, config)?, true))
            }
        },
    }
}

fn build_snapshot_l0_tables(
    branch_id: BranchId,
    output_identity_seed: &TableIdentity,
    table_builder_config: TableBuilderConfig,
    max_rows_per_table: usize,
    rows: &[StorageRow],
) -> BranchRuntimeResult<Vec<BranchOwnedTable>> {
    let builder = ImmutableTableBuilder::new(table_builder_config)
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
    let mut tables = Vec::new();
    for (output_index, chunk) in rows.chunks(max_rows_per_table).enumerate() {
        let identity =
            snapshot_table_identity(output_identity_seed, branch_id, output_index, chunk)?;
        let artifact = builder
            .build_from_storage_rows(identity.clone(), chunk)
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let reader = ImmutableTableReader::open_bytes(
            identity.clone(),
            artifact.into_bytes(),
            TableReaderConfig::default(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let descriptor =
            BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)?;
        tables.push(BranchOwnedTable::new(branch_id, descriptor, reader)?);
    }
    Ok(tables)
}

fn snapshot_table_identity(
    output_identity_seed: &TableIdentity,
    branch_id: BranchId,
    output_index: usize,
    rows: &[StorageRow],
) -> BranchRuntimeResult<TableIdentity> {
    let fingerprint = snapshot_rows_fingerprint(rows);
    TableIdentity::new(format!(
        "{}-snapshot-{}-{}-{fingerprint}",
        output_identity_seed.as_str(),
        branch_id,
        output_index,
    ))
    .map_err(|source| BranchRuntimeError::TableRuntime { source })
}

fn snapshot_rows_fingerprint(rows: &[StorageRow]) -> String {
    let mut hash = Sha256::new();
    for row in rows {
        hash.update(TableInternalKeyBytes::from_row(row).as_slice());
        hash.update(row.commit_timestamp().as_micros().to_le_bytes());
        hash.update(row.expires_at().as_micros().to_le_bytes());
        hash.update([u8::from(row.is_tombstone())]);
        hash.update(
            u64::try_from(row.value().len())
                .expect("row value length fits in u64")
                .to_le_bytes(),
        );
        hash.update(row.value());
    }
    hex_lower(&hash.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}
