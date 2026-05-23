//! Generated checkpoint, flush-watermark, and retention-proof contract helpers.

use super::recovery::{branch_id, put_row, testkit_error};
use super::{ensure, script_byte};
use crate::branch::BranchLocalState;
use crate::commit::{CommitTimelineEntry, CommitTimelineRows};
use crate::lifecycle::{
    flush_cache_branch, FlushFrozenRequest, FlushTableIdentitySeed, FlushTableObjectId,
    LifecycleCheckpointRequest, LifecycleFlushWatermarkProof, LifecycleFlushWatermarkRequest,
    LifecycleWalTruncationRequest,
};
use crate::row::StorageRow;
use crate::service::{WalRetentionProof, WalRetentionProofSource};
use crate::testkit::TestkitError;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleCheckpointContractOutcome {
    accepted_requests: usize,
    deferred_requests: usize,
    active_rows: usize,
    frozen_rows: usize,
    owned_rows: usize,
    tombstone_rows: usize,
    timeline_rows: usize,
    flush_accepts: usize,
    flush_rejects: usize,
    flush_noops: usize,
    retention_accepts: usize,
    retention_rejects: usize,
    cache_rejections: usize,
}

pub fn check_lifecycle_checkpoint_contract(
    script: &[u8],
) -> Result<LifecycleCheckpointContractOutcome, TestkitError> {
    let mut outcome = LifecycleCheckpointContractOutcome::default();
    check_input_request(script, &mut outcome)?;
    check_input_rows(script, &mut outcome)?;
    check_input_flush_proofs(script, &mut outcome)?;
    check_input_retention_proofs(script, &mut outcome)?;
    check_input_cache_rejection(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleCheckpointContractOutcome {
    pub const fn accepted_request_cases(&self) -> usize {
        self.accepted_requests
    }

    pub const fn deferred_request_cases(&self) -> usize {
        self.deferred_requests
    }

    pub const fn active_row_cases(&self) -> usize {
        self.active_rows
    }

    pub const fn frozen_row_cases(&self) -> usize {
        self.frozen_rows
    }

    pub const fn owned_row_cases(&self) -> usize {
        self.owned_rows
    }

    pub const fn tombstone_row_cases(&self) -> usize {
        self.tombstone_rows
    }

    pub const fn timeline_row_cases(&self) -> usize {
        self.timeline_rows
    }

    pub const fn flush_accept_cases(&self) -> usize {
        self.flush_accepts
    }

    pub const fn flush_reject_cases(&self) -> usize {
        self.flush_rejects
    }

    pub const fn flush_noop_cases(&self) -> usize {
        self.flush_noops
    }

    pub const fn retention_accept_cases(&self) -> usize {
        self.retention_accepts
    }

    pub const fn retention_reject_cases(&self) -> usize {
        self.retention_rejects
    }

    pub const fn cache_rejection_cases(&self) -> usize {
        self.cache_rejections
    }
}

fn check_input_request(
    script: &[u8],
    outcome: &mut LifecycleCheckpointContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 0));
    let snapshot_id = 1 + u64::from(script_byte(script, 1));
    let request = LifecycleCheckpointRequest::new(
        branch,
        snapshot_id,
        Timestamp::from_micros(1 + u64::from(script_byte(script, 2))),
    )
    .map_err(testkit_error)?;
    ensure(
        request.branch_id() == branch && request.snapshot_id() == snapshot_id,
        "checkpoint request did not preserve input-derived facts",
    )?;
    outcome.accepted_requests += 1;

    let empty = BranchLocalState::empty(branch);
    ensure(
        empty
            .checkpoint_rows(CommitVersion::new(1))
            .map_err(testkit_error)?
            .is_empty(),
        "empty branch produced checkpoint rows",
    )?;
    outcome.deferred_requests += 1;
    Ok(())
}

fn check_input_rows(
    script: &[u8],
    outcome: &mut LifecycleCheckpointContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 3));
    let mut state = BranchLocalState::empty(branch);
    let owned = put_row(branch, CommitVersion::new(1), b"generated-owned", b"value");
    let frozen = put_row(branch, CommitVersion::new(2), b"generated-frozen", b"value");
    let active = put_row(branch, CommitVersion::new(3), b"generated-active", b"value");
    let tombstone = StorageRow::tombstone(
        physical_key(branch, b"generated-delete"),
        CommitVersion::new(4),
        Timestamp::from_micros(400),
    );
    let timeline = CommitTimelineRows::from_entry(
        CommitTimelineEntry::new(branch, CommitVersion::new(5), Timestamp::from_micros(500))
            .map_err(testkit_error)?,
    )
    .map_err(testkit_error)?
    .into_rows();

    state
        .append_committed_row(owned.clone())
        .map_err(testkit_error)?;
    state.rotate_active();
    flush_cache_branch(&mut state, &flush_request(branch)?).map_err(testkit_error)?;
    state
        .append_committed_row(frozen.clone())
        .map_err(testkit_error)?;
    state.rotate_active();
    for row in [
        active.clone(),
        tombstone.clone(),
        timeline[0].clone(),
        timeline[1].clone(),
    ] {
        state.append_committed_row(row).map_err(testkit_error)?;
    }
    let hidden = put_row(branch, CommitVersion::new(6), b"generated-hidden", b"value");
    state.append_committed_row(hidden).map_err(testkit_error)?;

    let watermark = CommitVersion::new(5);
    let rows = state.checkpoint_rows(watermark).map_err(testkit_error)?;
    ensure(rows.contains(&owned), "owned checkpoint row missing")?;
    ensure(rows.contains(&frozen), "frozen checkpoint row missing")?;
    ensure(rows.contains(&active), "active checkpoint row missing")?;
    ensure(
        rows.contains(&tombstone),
        "tombstone checkpoint row missing",
    )?;
    ensure(
        rows.contains(&timeline[0]),
        "timeline checkpoint row missing",
    )?;
    ensure(
        rows.contains(&timeline[1]),
        "reverse timeline checkpoint row missing",
    )?;
    ensure(
        rows.iter()
            .all(|row| row.commit_version() <= CommitVersion::new(5)),
        "checkpoint rows included data above the visible watermark",
    )?;

    outcome.owned_rows += 1;
    outcome.frozen_rows += 1;
    outcome.active_rows += 1;
    outcome.tombstone_rows += 1;
    outcome.timeline_rows += 1;
    Ok(())
}

fn check_input_flush_proofs(
    script: &[u8],
    outcome: &mut LifecycleCheckpointContractOutcome,
) -> Result<(), TestkitError> {
    let candidate = CommitVersion::new(1 + u64::from(script_byte(script, 5) % 8));
    let snapshot_watermark =
        CommitVersion::new(candidate.as_u64() + 1 + u64::from(script_byte(script, 6) % 4));
    let accepted = LifecycleFlushWatermarkRequest::new(
        candidate,
        LifecycleFlushWatermarkProof::CheckpointCovered { snapshot_watermark },
    )
    .map_err(testkit_error)?;
    ensure(
        accepted.candidate() == candidate,
        "flush request did not preserve candidate",
    )?;
    outcome.flush_accepts += 1;

    let table_only = LifecycleFlushWatermarkRequest::new(
        candidate,
        LifecycleFlushWatermarkProof::TableObjectsOnly {
            flushed_through: candidate,
        },
    )
    .map_err(testkit_error)?;
    ensure(
        matches!(
            table_only.proof(),
            LifecycleFlushWatermarkProof::TableObjectsOnly { .. }
        ),
        "table-only flush proof lost its source",
    )?;
    outcome.flush_rejects += 1;

    let already = LifecycleFlushWatermarkRequest::new(
        candidate,
        LifecycleFlushWatermarkProof::AlreadyPersisted,
    )
    .map_err(testkit_error)?;
    ensure(
        matches!(
            already.proof(),
            LifecycleFlushWatermarkProof::AlreadyPersisted
        ),
        "already-persisted flush proof lost its source",
    )?;
    outcome.flush_noops += 1;
    Ok(())
}

fn check_input_retention_proofs(
    script: &[u8],
    outcome: &mut LifecycleCheckpointContractOutcome,
) -> Result<(), TestkitError> {
    let covered = CommitVersion::new(1 + u64::from(script_byte(script, 7) % 8));
    let snapshot =
        LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(covered))
            .map_err(testkit_error)?;
    ensure(
        snapshot.proof().source() == WalRetentionProofSource::SnapshotWatermark,
        "snapshot retention proof source changed",
    )?;
    let flush = LifecycleWalTruncationRequest::new(WalRetentionProof::flush_watermark(covered))
        .map_err(testkit_error)?;
    ensure(
        flush.proof().source() == WalRetentionProofSource::FlushWatermark,
        "flush retention proof source changed",
    )?;
    outcome.retention_accepts += 1;

    let zero = LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(
        CommitVersion::ZERO,
    ));
    ensure(zero.is_err(), "zero retention proof was accepted")?;
    outcome.retention_rejects += 1;
    Ok(())
}

fn check_input_cache_rejection(
    script: &[u8],
    outcome: &mut LifecycleCheckpointContractOutcome,
) -> Result<(), TestkitError> {
    let zero_snapshot = LifecycleCheckpointRequest::new(
        branch_id(script_byte(script, 8)),
        0,
        Timestamp::from_micros(1),
    );
    ensure(
        zero_snapshot.is_err(),
        "invalid cache checkpoint fact accepted",
    )?;
    outcome.cache_rejections += 1;
    Ok(())
}

fn flush_request(branch: BranchId) -> Result<FlushFrozenRequest, TestkitError> {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(format!("generated-checkpoint-flush-{branch}"))
            .map_err(testkit_error)?,
        FlushTableObjectId::new(format!("generated-checkpoint-object-{branch}"))
            .map_err(testkit_error)?,
    )
    .map_err(testkit_error)
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> crate::row::PhysicalKey {
    crate::row::PhysicalKey::new(
        branch,
        "checkpoint",
        crate::row::StorageSpaceId::engine(0x41).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}
