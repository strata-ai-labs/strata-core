use super::*;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::BranchOwnedTable;
use crate::branch::state::BranchLocalState;
use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableReaderConfig, TableRow,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(super) fn install_l0_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) {
    install_owned_table(state, branch, BranchLevel::ZERO, identity, rows);
}

/// Build an owned L0 table through the flush-install path (append -> rotate -> replace frozen).
/// Unlike `install_l0_table` (which installs via the hook recompute), this exercises the
/// incremental `per_level_bytes` delta (`apply_flush_install_shape_delta`) — the maintenance path
/// BS1.3's scoring depends on but that only the debug shape oracle otherwise covers.
pub(super) fn flush_install_state(
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    for row in &rows {
        state.append_committed_row(row.clone()).expect("append");
    }
    let _rotation = state.rotate_active();
    let table = owned_table(branch, BranchLevel::ZERO, identity, rows);
    state
        .replace_frozen_with_l0_table(0, table)
        .expect("flush install");
    state
}

pub(super) fn read_shape_state(branch: BranchId) -> BranchLocalState {
    let mut state = BranchLocalState::empty(branch);
    let live = put_row(branch, b"scan-a", 3, 3_000, b"live");
    let expiring = put_expiring_row(branch, b"scan-b", 4, 4_000, 4_500, b"expiring");
    let old = put_row(branch, b"history", 1, 1_000, b"old");
    let delete = tombstone_row(branch, b"history", 2, 2_000);
    install_l0_table(&mut state, branch, "read-shape-left", vec![old, live]);
    install_l0_table(
        &mut state,
        branch,
        "read-shape-right",
        vec![delete, expiring],
    );
    state
}

pub(super) fn compact_read_shape_state(
    branch: BranchId,
    state: &mut BranchLocalState,
) -> LifecycleCompactionOutcome {
    compact_cache_branch(
        state,
        &LifecycleCompactionRequest::new(
            branch,
            BranchCompactionKind::CompactL0,
            "read-shape-rewrite",
        )
        .expect("request"),
    )
    .expect("outcome")
}

pub(super) fn materialization_read_state(parent: BranchId, child: BranchId) -> BranchLocalState {
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "materialization-read-parent",
        vec![
            put_row(parent, b"scan-a", 1, 1_000, b"inherited-live"),
            put_expiring_row(parent, b"scan-b", 2, 2_000, 2_500, b"inherited-expiring"),
            put_row(parent, b"history", 3, 3_000, b"history-old"),
            tombstone_row(parent, b"history", 4, 4_000),
            put_row(parent, b"shared", 5, 5_000, b"parent-shared"),
        ],
    );
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    parent_state
        .append_committed_row(put_row(parent, b"post-fork", 6, 6_000, b"late-parent"))
        .expect("append parent after fork");
    install_l0_table(
        &mut child_state,
        child,
        "materialization-child-owned",
        vec![put_row(child, b"shared", 7, 7_000, b"child-shared")],
    );
    child_state
}

pub(super) fn materialize_read_state(
    child: BranchId,
    child_state: &mut BranchLocalState,
) -> LifecycleMaterializationOutcome {
    materialize_cache_branch(
        child_state,
        &LifecycleMaterializationRequest::new(child, 0, "materialization-read").expect("request"),
    )
    .expect("outcome")
}

pub(super) fn install_owned_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) {
    state
        .install_owned_table_at_level(level, owned_table(branch, level, identity, rows))
        .expect("install table");
}

fn owned_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    let identity = TableIdentity::new(identity).expect("identity");
    let mut table_rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    let artifact = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .expect("builder")
        .build_from_rows(identity.clone(), &table_rows)
        .expect("built table");
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("reader");
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), level).expect("descriptor");
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    BranchOwnedTable::new(branch, descriptor, reader, extras).expect("owned table")
}

pub(super) fn put_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    value: &[u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

pub(super) fn put_expiring_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    expires_at: u64,
    value: &[u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::from_micros(expires_at),
        value.to_vec(),
    )
}

pub(super) fn tombstone_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
) -> StorageRow {
    StorageRow::tombstone(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    )
}

pub(super) fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "rewrite",
        StorageSpaceId::engine(0x51).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

pub(super) fn history_versions(rows: &[crate::branch::read::BranchHistoryRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect()
}

pub(super) fn scan_user_keys(rows: &[crate::branch::read::BranchVisibleRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

pub(super) fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

pub(super) fn empty_maintenance_status() -> MaintenanceExecutorStatus {
    LifecycleMaintenanceExecutor::new(8)
        .expect("executor")
        .status()
}

pub(super) fn cache_runtime(
    branch: BranchId,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    cache_runtime_with_config(branch, LifecycleConfig::default())
}

pub(super) fn cache_runtime_with_config(
    branch: BranchId,
    config: LifecycleConfig,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    let backend = crate::backend::memory::MemoryBackend::new();
    LifecycleCacheRuntime::open(
        cache_open_request(branch, config),
        &backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(4_000)),
    )
    .expect("cache runtime")
}

fn cache_open_request(branch: BranchId, config: LifecycleConfig) -> LifecycleCacheOpenRequest {
    LifecycleCacheOpenRequest::new(
        StorageOpenPlan::new(
            StorageMode::Cache,
            LifecycleCodecId::identity(),
            RecoveryStrictness::Strict,
            config,
        )
        .expect("cache plan"),
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .expect("cache request")
}

pub(super) fn open_state() -> LifecycleStateMachine {
    let mut state = LifecycleStateMachine::new();
    state
        .transition(LifecycleTransitionTrigger::OpenRequested)
        .expect("open requested");
    state
        .transition(LifecycleTransitionTrigger::CacheOpenReady)
        .expect("cache open ready");
    state
}
