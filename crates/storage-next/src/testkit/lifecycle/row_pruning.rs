//! Generated lifecycle row-pruning contract helpers.

use super::{ensure, script_byte};
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::error::BranchRuntimeError;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::pruning::{
    BranchCompactionPruningProof, BranchInheritancePruningProof, BranchRecoveryHealthAttestation,
    BranchSharedTableSafety, BranchTombstoneElisionProof, BranchTtlElisionProof,
};
use crate::branch::read::{BranchOwnedTable, BranchTimestampCoverage};
use crate::branch::state::compaction::{
    BranchCompactionKind, BranchCompactionRequest, BranchCompactionRetentionPolicy,
};
use crate::branch::state::BranchLocalState;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableReaderConfig, TableRow,
};
use crate::testkit::TestkitError;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleRowPruningContractOutcome {
    proof_rejected: usize,
    old_version_dropped: usize,
    tombstone_dropped: usize,
    tombstone_kept_for_shadowing: usize,
    expired_row_dropped: usize,
    expired_row_kept_for_as_of: usize,
    pinned_view_blocked: usize,
    inherited_layer_blocked: usize,
    retained_boundary_reported: usize,
    recovery_boundary_enforced: usize,
}

pub fn check_lifecycle_row_pruning_contract(
    script: &[u8],
) -> Result<LifecycleRowPruningContractOutcome, TestkitError> {
    let mut outcome = LifecycleRowPruningContractOutcome::default();
    check_proof_rejected(script, &mut outcome)?;
    check_old_versions(script, &mut outcome)?;
    check_tombstone_drop(script, &mut outcome)?;
    check_tombstone_shadowing(script, &mut outcome)?;
    check_expired_rows(script, &mut outcome)?;
    check_expired_row_needed_by_timestamp(script, &mut outcome)?;
    check_pinned_view_block(script, &mut outcome)?;
    check_inherited_layer_block(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleRowPruningContractOutcome {
    pub const fn proof_rejected_cases(&self) -> usize {
        self.proof_rejected
    }

    pub const fn old_version_dropped_cases(&self) -> usize {
        self.old_version_dropped
    }

    pub const fn tombstone_dropped_cases(&self) -> usize {
        self.tombstone_dropped
    }

    pub const fn tombstone_kept_for_shadowing_cases(&self) -> usize {
        self.tombstone_kept_for_shadowing
    }

    pub const fn expired_row_dropped_cases(&self) -> usize {
        self.expired_row_dropped
    }

    pub const fn expired_row_kept_for_as_of_cases(&self) -> usize {
        self.expired_row_kept_for_as_of
    }

    pub const fn pinned_view_blocked_cases(&self) -> usize {
        self.pinned_view_blocked
    }

    pub const fn inherited_layer_blocked_cases(&self) -> usize {
        self.inherited_layer_blocked
    }

    pub const fn retained_boundary_reported_cases(&self) -> usize {
        self.retained_boundary_reported
    }

    pub const fn recovery_boundary_enforced_cases(&self) -> usize {
        self.recovery_boundary_enforced
    }
}

fn check_proof_rejected(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 0).max(1));
    let mut state = version_state(branch, "generated-proof", &[4, 2])?;
    let request =
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, "generated-proof")
            .map_err(pruning_error)?
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions);

    let error = state
        .compact_branch_owned_tables(&request)
        .expect_err("missing proof rejects");

    ensure(
        matches!(error, BranchRuntimeError::InvalidCompaction { .. }),
        "missing pruning proof was not rejected",
    )?;
    outcome.proof_rejected += 1;
    Ok(())
}

fn check_old_versions(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 1).max(2));
    let mut state = version_state(branch, "generated-version", &[9, 6, 4, 2])?;
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)?;

    let compaction = state
        .compact_branch_owned_tables(&drop_older_request(branch, "generated-version-out", proof)?)
        .map_err(pruning_error)?;

    ensure(
        compaction
            .table_report()
            .is_some_and(|report| report.dropped_rows() == 1),
        "old-version pruning did not drop exactly one older row",
    )?;
    ensure(
        history_for(&state, branch, b"key")? == vec![9, 6, 4],
        "old-version pruning did not retain expected history",
    )?;
    ensure(
        state.timestamp_coverage()
            == BranchTimestampCoverage::complete_since(Timestamp::from_micros(50)),
        "row pruning did not report retained timestamp boundary",
    )?;
    ensure(
        matches!(
            state
                .capture_read_view()
                .map_err(pruning_error)?
                .read_point(
                    &physical_key(branch, b"key")?,
                    crate::branch::read::BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
                ),
            Err(BranchRuntimeError::InsufficientTimestampHistory { .. })
        ),
        "pruned timestamp read did not return insufficient-history boundary",
    )?;
    outcome.old_version_dropped += 1;
    outcome.retained_boundary_reported += 1;
    outcome.recovery_boundary_enforced += 1;
    Ok(())
}

fn check_tombstone_drop(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 2).max(3));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(1, 8, 8).map_err(pruning_error)?,
    )
    .map_err(pruning_error)?;
    install_l0_table(
        &mut state,
        branch,
        "generated-tombstone-left",
        vec![tombstone_row(branch, b"deleted", 6, 60)?],
    )?;
    install_l0_table(
        &mut state,
        branch,
        "generated-tombstone-right",
        vec![put_row(branch, b"other", 2, 20, b"other")?],
    )?;
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)?
        .with_tombstone_elision(BranchTombstoneElisionProof::BottommostOwnedAndInheritedSafe)
        .map_err(pruning_error)?;

    let compaction = state
        .compact_branch_owned_tables(&tombstone_request(
            branch,
            "generated-tombstone-out",
            proof,
        )?)
        .map_err(pruning_error)?;

    ensure(
        compaction
            .table_report()
            .is_some_and(|report| report.dropped_rows() == 1),
        "tombstone pruning did not drop the proven bottommost tombstone",
    )?;
    outcome.tombstone_dropped += 1;
    Ok(())
}

fn check_tombstone_shadowing(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 3).max(4));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(1, 8, 8).map_err(pruning_error)?,
    )
    .map_err(pruning_error)?;
    install_l0_table(
        &mut state,
        branch,
        "generated-shadow-left",
        vec![tombstone_row(branch, b"deleted", 6, 60)?],
    )?;
    install_l0_table(
        &mut state,
        branch,
        "generated-shadow-right",
        vec![put_row(branch, b"deleted", 2, 20, b"old")?],
    )?;
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 7, 70)?
        .with_tombstone_elision(BranchTombstoneElisionProof::BottommostOwnedAndInheritedSafe)
        .map_err(pruning_error)?;

    let error = state
        .compact_branch_owned_tables(&tombstone_request(branch, "generated-shadow-out", proof)?)
        .expect_err("shadowing tombstone rejects");

    ensure(
        matches!(error, BranchRuntimeError::InvalidCompaction { .. }),
        "unsafe tombstone elision did not reject",
    )?;
    outcome.tombstone_kept_for_shadowing += 1;
    Ok(())
}

fn check_expired_rows(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 4).max(5));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(1, 8, 8).map_err(pruning_error)?,
    )
    .map_err(pruning_error)?;
    install_l0_table(
        &mut state,
        branch,
        "generated-ttl-left",
        vec![put_expiring_row(branch, b"ttl", 6, 45, 45, b"expired-new")?],
    )?;
    install_l0_table(
        &mut state,
        branch,
        "generated-ttl-right",
        vec![put_expiring_row(branch, b"ttl", 4, 40, 45, b"expired-old")?],
    )?;
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)?
        .with_ttl_elision(BranchTtlElisionProof::ExpiredAtOrBefore {
            timestamp: Timestamp::from_micros(50),
        })
        .map_err(pruning_error)?;

    let compaction = state
        .compact_branch_owned_tables(&ttl_request(branch, "generated-ttl-out", proof)?)
        .map_err(pruning_error)?;

    ensure(
        compaction
            .table_report()
            .is_some_and(|report| report.dropped_rows() == 1),
        "expired row pruning did not drop exactly one expired row",
    )?;
    outcome.expired_row_dropped += 1;
    Ok(())
}

fn check_expired_row_needed_by_timestamp(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 5).max(6));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(1, 8, 8).map_err(pruning_error)?,
    )
    .map_err(pruning_error)?;
    install_l0_table(
        &mut state,
        branch,
        "generated-ttl-kept-left",
        vec![put_expiring_row(branch, b"ttl", 4, 60, 45, b"kept")?],
    )?;
    install_l0_table(
        &mut state,
        branch,
        "generated-ttl-kept-right",
        vec![put_row(branch, b"ttl", 1, 10, b"survivor")?],
    )?;
    state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&state, 5, 50)?
        .with_ttl_elision(BranchTtlElisionProof::ExpiredAtOrBefore {
            timestamp: Timestamp::from_micros(50),
        })
        .map_err(pruning_error)?;

    let compaction = state
        .compact_branch_owned_tables(&ttl_request(branch, "generated-ttl-kept-out", proof)?)
        .map_err(pruning_error)?;

    ensure(
        compaction
            .table_report()
            .is_some_and(|report| report.dropped_rows() == 0),
        "timestamp-retained expired row was dropped",
    )?;
    outcome.expired_row_kept_for_as_of += 1;
    Ok(())
}

fn check_pinned_view_block(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 6).max(7));
    let state = version_state(branch, "generated-pinned", &[4, 2])?;

    let error = proof_for(&state, 3, 30)?
        .with_pinned_view_floor(CommitVersion::new(2))
        .expect_err("pinned view blocks pruning");

    ensure(
        matches!(error, BranchRuntimeError::InvalidCompaction { .. }),
        "pinned-view pruning proof was not rejected",
    )?;
    outcome.pinned_view_blocked += 1;
    Ok(())
}

fn check_inherited_layer_block(
    script: &[u8],
    outcome: &mut LifecycleRowPruningContractOutcome,
) -> Result<(), TestkitError> {
    let parent = branch_id(script_byte(script, 7).max(8));
    let child = distinct_branch_id(parent, script_byte(script, 8).max(9));
    let mut parent_state = BranchLocalState::empty(parent);
    install_l0_table(
        &mut parent_state,
        parent,
        "generated-inherited-parent",
        vec![put_row(parent, b"inherited", 4, 40, b"parent")?],
    )?;
    let (mut child_state, _) = parent_state
        .fork_into_empty_child(child)
        .map_err(pruning_error)?;
    install_l0_table(
        &mut child_state,
        child,
        "generated-inherited-child",
        vec![put_row(child, b"local", 5, 50, b"local")?],
    )?;
    install_l0_table(
        &mut child_state,
        child,
        "generated-inherited-child-right",
        vec![put_row(child, b"other", 3, 30, b"other")?],
    )?;
    child_state.set_timestamp_coverage(BranchTimestampCoverage::complete());
    let proof = proof_for(&child_state, 4, 40)?;

    let error = child_state
        .compact_branch_owned_tables(&drop_older_request(
            child,
            "generated-inherited-out",
            proof,
        )?)
        .expect_err("inherited layer blocks pruning");

    ensure(
        matches!(error, BranchRuntimeError::InvalidCompaction { .. }),
        "inherited-layer pruning proof was not rejected",
    )?;
    outcome.inherited_layer_blocked += 1;
    Ok(())
}

fn proof_for(
    state: &BranchLocalState,
    retained_floor: u64,
    retained_timestamp_floor: u64,
) -> Result<BranchCompactionPruningProof, TestkitError> {
    BranchCompactionPruningProof::from_branch_state(state, CommitVersion::new(retained_floor))
        .map_err(pruning_error)?
        .with_retained_timestamp_floor(Timestamp::from_micros(retained_timestamp_floor))
        .map_err(pruning_error)?
        .with_inherited_safety(BranchInheritancePruningProof::NoReadableInheritedLayers)
        .map_err(pruning_error)?
        .with_shared_table_safety(BranchSharedTableSafety::NotShared)
        .map_err(pruning_error)?
        .with_recovery_health(BranchRecoveryHealthAttestation::Healthy)
        .map_err(pruning_error)
}

fn drop_older_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> Result<BranchCompactionRequest, TestkitError> {
    Ok(
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
            .map_err(pruning_error)?
            .with_retention_policy(BranchCompactionRetentionPolicy::DropOlderVersions)
            .with_pruning_proof(proof),
    )
}

fn tombstone_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> Result<BranchCompactionRequest, TestkitError> {
    Ok(
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
            .map_err(pruning_error)?
            .with_retention_policy(BranchCompactionRetentionPolicy::DropTombstones)
            .with_pruning_proof(proof),
    )
}

fn ttl_request(
    branch: BranchId,
    seed: &str,
    proof: BranchCompactionPruningProof,
) -> Result<BranchCompactionRequest, TestkitError> {
    Ok(
        BranchCompactionRequest::new(branch, BranchCompactionKind::CompactL0, seed)
            .map_err(pruning_error)?
            .with_retention_policy(BranchCompactionRetentionPolicy::DropExpired)
            .with_pruning_proof(proof),
    )
}

fn version_state(
    branch: BranchId,
    seed: &str,
    versions: &[u64],
) -> Result<BranchLocalState, TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let midpoint = versions.len().saturating_add(1) / 2;
    let left = versions[..midpoint]
        .iter()
        .map(|version| {
            put_row(
                branch,
                b"key",
                *version,
                version.saturating_mul(10),
                format!("v{version}").as_bytes(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let right = versions[midpoint..]
        .iter()
        .map(|version| {
            put_row(
                branch,
                b"key",
                *version,
                version.saturating_mul(10),
                format!("v{version}").as_bytes(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    install_l0_table(&mut state, branch, &format!("{seed}-left"), left)?;
    install_l0_table(&mut state, branch, &format!("{seed}-right"), right)?;
    Ok(state)
}

fn install_l0_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<(), TestkitError> {
    let identity = TableIdentity::new(identity).map_err(pruning_error)?;
    let mut table_rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    let artifact = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .map_err(pruning_error)?
        .build_from_rows(identity.clone(), &table_rows)
        .map_err(pruning_error)?;
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .map_err(pruning_error)?;
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)
            .map_err(pruning_error)?;
    let table = BranchOwnedTable::new(branch, descriptor, reader).map_err(pruning_error)?;
    state.install_l0_table(table).map_err(pruning_error)?;
    Ok(())
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

fn put_expiring_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    expires_at: u64,
    value: &[u8],
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::put(
        physical_key(branch, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::from_micros(expires_at),
        value.to_vec(),
    ))
}

fn tombstone_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::tombstone(
        physical_key(branch, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    ))
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> Result<PhysicalKey, TestkitError> {
    PhysicalKey::new(
        branch,
        "rewrite",
        StorageSpaceId::engine(0x61).map_err(pruning_error)?,
        user_key.to_vec(),
    )
    .map_err(pruning_error)
}

fn history_for(
    state: &BranchLocalState,
    branch: BranchId,
    key: &[u8],
) -> Result<Vec<u64>, TestkitError> {
    let view = state.capture_read_view().map_err(pruning_error)?;
    Ok(view
        .history(
            &physical_key(branch, key)?,
            crate::branch::read::BranchHistoryOptions::all(),
        )
        .map_err(pruning_error)?
        .iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect())
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn distinct_branch_id(other: BranchId, byte: u8) -> BranchId {
    let candidate = branch_id(byte);
    if candidate == other {
        branch_id(byte.wrapping_add(1))
    } else {
        candidate
    }
}

fn pruning_error(error: impl std::error::Error) -> TestkitError {
    TestkitError::new(format!("row pruning contract failed: {error}"))
}
