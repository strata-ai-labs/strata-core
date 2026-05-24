//! Generated lifecycle table rewrite contract helpers.

use super::{ensure, script_byte};
use crate::branch::{
    BranchCompactionKind, BranchLevel, BranchLocalState, BranchOwnedTable, BranchTableDescriptor,
};
use crate::lifecycle::{
    collect_storage_pressure, compact_cache_branch, compact_durable_branch,
    materialize_cache_branch, LifecycleCompactionRequest, LifecycleCompactionStatus,
    LifecycleMaintenanceExecutor, LifecycleMaterializationRequest, LifecycleMaterializationStatus,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity, MaintenanceOutcomeStatus,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableReaderConfig, TableRow,
};
use crate::testkit::TestkitError;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleTableRewriteContractOutcome {
    cache_compactions: usize,
    durable_compactions: usize,
    materializations: usize,
    pressure_cases: usize,
}

pub fn check_lifecycle_table_rewrite_contract(
    script: &[u8],
) -> Result<LifecycleTableRewriteContractOutcome, TestkitError> {
    let mut outcome = LifecycleTableRewriteContractOutcome::default();
    check_cache_compaction(script, &mut outcome)?;
    check_durable_compaction(script, &mut outcome)?;
    check_cache_materialization(script, &mut outcome)?;
    check_storage_pressure(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleTableRewriteContractOutcome {
    pub const fn cache_compaction_cases(&self) -> usize {
        self.cache_compactions
    }

    pub const fn durable_compaction_cases(&self) -> usize {
        self.durable_compactions
    }

    pub const fn materialization_cases(&self) -> usize {
        self.materializations
    }

    pub const fn pressure_cases(&self) -> usize {
        self.pressure_cases
    }
}

fn check_cache_compaction(
    script: &[u8],
    outcome: &mut LifecycleTableRewriteContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 0).max(1));
    let mut state = compactable_state(branch, "rewrite-cache-left", "rewrite-cache-right")?;
    let key = physical_key(branch, b"shared")?;
    let request = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        format!("rewrite-cache-{}", script_byte(script, 1)),
    )
    .map_err(rewrite_error)?;

    let rewrite = compact_cache_branch(&mut state, &request).map_err(rewrite_error)?;

    ensure(
        rewrite.status() == LifecycleCompactionStatus::Completed,
        "cache compaction did not complete",
    )?;
    ensure(
        rewrite.maintenance_outcome().status() == MaintenanceOutcomeStatus::Completed,
        "cache compaction did not produce completed maintenance outcome",
    )?;
    ensure(
        latest_value(&state, &key)?.as_deref() == Some(b"newer".as_slice()),
        "cache compaction changed visible value",
    )?;
    outcome.cache_compactions += 1;
    Ok(())
}

fn check_durable_compaction(
    script: &[u8],
    outcome: &mut LifecycleTableRewriteContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 2).max(2));
    let mut state = compactable_state(branch, "rewrite-durable-left", "rewrite-durable-right")?;
    let request = LifecycleCompactionRequest::new(
        branch,
        BranchCompactionKind::CompactL0,
        format!("rewrite-durable-{}", script_byte(script, 3)),
    )
    .map_err(rewrite_error)?;

    let rewrite = compact_durable_branch(&mut state, &request).map_err(rewrite_error)?;

    ensure(
        rewrite.status() == LifecycleCompactionStatus::CompletedCheckpointRequired,
        "durable compaction did not report checkpoint debt",
    )?;
    ensure(
        rewrite.checkpoint_required(),
        "durable compaction did not set checkpoint-required flag",
    )?;
    ensure(
        rewrite.maintenance_outcome().checkpoint_required(),
        "durable compaction did not preserve checkpoint debt in maintenance outcome",
    )?;
    outcome.durable_compactions += 1;
    Ok(())
}

fn check_cache_materialization(
    script: &[u8],
    outcome: &mut LifecycleTableRewriteContractOutcome,
) -> Result<(), TestkitError> {
    let parent = branch_id(script_byte(script, 4).max(3));
    let child = branch_id(script_byte(script, 5).max(4));
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "rewrite-material-parent",
        vec![put_row(parent, b"inherited", 1, 1_000, b"parent")?],
    )?;
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .map_err(rewrite_error)?;
    let key = physical_key(child, b"inherited")?;
    let request = LifecycleMaterializationRequest::new(
        child,
        0,
        format!("rewrite-material-{}", script_byte(script, 6)),
    )
    .map_err(rewrite_error)?;

    let materialized =
        materialize_cache_branch(&mut child_state, &request).map_err(rewrite_error)?;

    ensure(
        materialized.status() == LifecycleMaterializationStatus::Completed,
        "cache materialization did not complete",
    )?;
    ensure(
        child_state.inherited_layer_count() == 0,
        "cache materialization left inherited layer visible",
    )?;
    ensure(
        latest_value(&child_state, &key)?.as_deref() == Some(b"parent".as_slice()),
        "cache materialization changed visible value",
    )?;
    outcome.materializations += 1;
    Ok(())
}

fn check_storage_pressure(
    script: &[u8],
    outcome: &mut LifecycleTableRewriteContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 7).max(5));
    let mut state = BranchLocalState::empty(branch);
    for index in 0..4 {
        state
            .append_committed_row(put_row(
                branch,
                &[b'f', index],
                u64::from(index) + 1,
                (u64::from(index) + 1) * 100,
                b"frozen",
            )?)
            .map_err(rewrite_error)?;
        state.rotate_active();
    }
    let maintenance = LifecycleMaintenanceExecutor::new(8)
        .map_err(rewrite_error)?
        .status();

    let pressure = collect_storage_pressure(&state, maintenance);

    ensure(
        pressure.severity() == LifecycleStoragePressureSeverity::BlockMutatingAdmission,
        "storage pressure did not block mutating admission for frozen backlog",
    )?;
    ensure(
        pressure.reason() == LifecycleStoragePressureReason::FrozenBacklog,
        "storage pressure did not report frozen backlog",
    )?;
    ensure(
        pressure.suggested_task().is_some(),
        "storage pressure did not suggest maintenance",
    )?;
    outcome.pressure_cases += 1;
    Ok(())
}

fn compactable_state(
    branch: BranchId,
    left_identity: &str,
    right_identity: &str,
) -> Result<BranchLocalState, TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    install_l0_table(
        &mut state,
        branch,
        left_identity,
        vec![put_row(branch, b"shared", 1, 1_000, b"older")?],
    )?;
    install_l0_table(
        &mut state,
        branch,
        right_identity,
        vec![put_row(branch, b"shared", 2, 2_000, b"newer")?],
    )?;
    Ok(state)
}

fn install_l0_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<(), TestkitError> {
    state
        .install_l0_table(owned_table(branch, identity, rows)?)
        .map_err(rewrite_error)?;
    Ok(())
}

fn owned_table(
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<BranchOwnedTable, TestkitError> {
    let identity = TableIdentity::new(identity).map_err(rewrite_error)?;
    let mut table_rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    let artifact = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .map_err(rewrite_error)?
        .build_from_rows(identity.clone(), &table_rows)
        .map_err(rewrite_error)?;
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .map_err(rewrite_error)?;
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)
            .map_err(rewrite_error)?;
    BranchOwnedTable::new(branch, descriptor, reader).map_err(rewrite_error)
}

fn latest_value(
    state: &BranchLocalState,
    key: &PhysicalKey,
) -> Result<Option<Vec<u8>>, TestkitError> {
    Ok(state
        .capture_read_view()
        .map_err(rewrite_error)?
        .latest(key)
        .map_err(rewrite_error)?
        .map(|row| row.row().value().to_vec()))
}

fn put_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    value: &[u8],
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::put(
        physical_key(branch, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::EPOCH,
        value.to_vec(),
    ))
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> Result<PhysicalKey, TestkitError> {
    PhysicalKey::new(
        branch,
        "rewrite",
        StorageSpaceId::engine(0x56).map_err(rewrite_error)?,
        user_key.to_vec(),
    )
    .map_err(rewrite_error)
}

fn branch_id(seed: u8) -> BranchId {
    BranchId::from_bytes([seed; BranchId::BYTE_LEN])
}

fn rewrite_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(error.to_string())
}
