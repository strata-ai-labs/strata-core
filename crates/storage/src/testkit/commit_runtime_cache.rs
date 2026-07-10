//! Generated cache/no-WAL commit contract helpers.

use crate::branch::config::BranchRuntimeConfig;
use crate::branch::read::{BranchReadBound, BranchScanBounds};
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitBranchGuardSet, CommitBranchRegistry, CommitCacheRuntime, CommitConflictKind,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitFactAllocator, CommitManualTimestampSource,
    CommitMutation, CommitObservedVersion, CommitOrigin, CommitOutcomeKind, CommitPhase,
    CommitReadFact, CommitReadOnlyDiagnostics, CommitRetentionHint, CommitRuntimeConfig,
    CommitRuntimeError, CommitRuntimeResult, CommitTimelineRows, CommitTimelineView,
    CommitTimestampGuard, CommitTimestampPolicy, CommitValidationFacts, CommitVersionAllocator,
    VisibleVersionTracker, COMMIT_TIMELINE_SPACE,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use strata_core::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

pub(crate) struct CommitRuntimeCacheContract {
    pub(crate) put_commits: usize,
    pub(crate) delete_commits: usize,
    pub(crate) mixed_commits: usize,
    pub(crate) one_version_per_batch: usize,
    pub(crate) one_timestamp_per_batch: usize,
    pub(crate) timeline_rows_installed: usize,
    pub(crate) visible_publications: usize,
    pub(crate) not_durable_outcomes: usize,
    pub(crate) branch_admission_rejections: usize,
    pub(crate) conflict_rejections: usize,
    pub(crate) non_cache_rejections: usize,
    pub(crate) apply_failure_atomicity: usize,
    pub(crate) version_gap_after_failure: usize,
    pub(crate) applied_above_visible_rejections: usize,
    pub(crate) visible_allocator_mismatch_rejections: usize,
    pub(crate) guard_release_after_failure: usize,
}

pub(crate) fn check_commit_runtime_cache_contract(
    script: &[u8],
) -> Result<CommitRuntimeCacheContract, TestkitError> {
    Ok(CommitRuntimeCacheContract {
        put_commits: check_put_commit(script)?,
        delete_commits: check_delete_commit(script)?,
        mixed_commits: check_mixed_commit(script)?,
        one_version_per_batch: check_one_version_per_batch(script)?,
        one_timestamp_per_batch: check_one_timestamp_per_batch(script)?,
        timeline_rows_installed: check_timeline_rows_installed(script)?,
        visible_publications: check_visible_publication(script)?,
        not_durable_outcomes: check_not_durable_outcome(script)?,
        branch_admission_rejections: check_branch_admission_rejections(script)?,
        conflict_rejections: check_conflict_rejection(script)?,
        non_cache_rejections: check_non_cache_rejections(script)?,
        apply_failure_atomicity: check_apply_failure_atomicity(script)?,
        version_gap_after_failure: check_version_gap_after_post_allocation_failure(script)?,
        applied_above_visible_rejections: check_applied_above_visible_rejection(script)?,
        visible_allocator_mismatch_rejections: check_visible_allocator_mismatch(script)?,
        guard_release_after_failure: check_guard_release_after_failure(script)?,
    })
}

fn check_put_commit(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 0));
    let key = physical_key(branch, 0x20, vec![b'p', script_byte(script, 1)]);
    let value = vec![script_byte(script, 2), 0xaa];
    let timestamp = timestamp(script_byte(script, 3));
    let mut fixture = CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp)?;
    let outcome = fixture
        .execute(put_batch(branch, key.clone(), value.clone()))
        .map_err(testkit_error)?;

    require_visible_cache_outcome(&outcome, CommitVersion::new(1), timestamp)?;
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let visible = view
        .latest(&key)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("cache put was not visible"))?;
    if visible.row().commit_version() != CommitVersion::new(1)
        || visible.row().commit_timestamp() != timestamp
        || visible.row().value() != value.as_slice()
    {
        return Err(TestkitError::new("cache put row facts were incorrect"));
    }
    Ok(1)
}

fn check_delete_commit(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 4));
    let key = physical_key(branch, 0x20, vec![b'd', script_byte(script, 5)]);
    let timestamp = timestamp(script_byte(script, 6));
    let mut fixture = CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp)?;
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        b"old".to_vec(),
    ))?;
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(1));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));

    let outcome = fixture
        .execute(delete_batch(branch, key.clone()))
        .map_err(testkit_error)?;

    require_visible_cache_outcome(&outcome, CommitVersion::new(2), timestamp)?;
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    if view.latest(&key).map_err(testkit_error)?.is_some() {
        return Err(TestkitError::new("cache delete did not hide latest row"));
    }
    Ok(1)
}

fn check_mixed_commit(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 7));
    let first = physical_key(branch, 0x20, vec![b'm', 0]);
    let deleted = physical_key(branch, 0x21, vec![b'm', 1]);
    let second = physical_key(branch, 0x22, vec![b'm', 2]);
    let timestamp = timestamp(script_byte(script, 8));
    let mut fixture = CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp)?;
    let batch = CommitBatch::mutating(
        branch,
        vec![
            CommitMutation::put(
                first.clone(),
                b"first".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(deleted.clone()),
            CommitMutation::put(
                second.clone(),
                b"second".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
        ],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    );

    let outcome = fixture.execute(batch).map_err(testkit_error)?;
    let version = outcome
        .commit_version()
        .ok_or_else(|| TestkitError::new("mixed cache outcome missed version"))?;
    let commit_timestamp = outcome
        .commit_timestamp()
        .ok_or_else(|| TestkitError::new("mixed cache outcome missed timestamp"))?;

    if outcome.mutation_counts().puts() != 2
        || outcome.mutation_counts().deletes() != 1
        || fixture.state.active_row_count() != 3 + CommitTimelineRows::timeline_row_count()
    {
        return Err(TestkitError::new("mixed cache commit counts were wrong"));
    }
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let first_row = view
        .latest(&first)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("mixed cache commit missed first row"))?;
    let second_row = view
        .latest(&second)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("mixed cache commit missed second row"))?;
    if first_row.row().value() != b"first"
        || first_row.row().commit_version() != version
        || first_row.row().commit_timestamp() != commit_timestamp
        || view.latest(&deleted).map_err(testkit_error)?.is_some()
        || second_row.row().value() != b"second"
        || second_row.row().commit_version() != version
        || second_row.row().commit_timestamp() != commit_timestamp
    {
        return Err(TestkitError::new("mixed cache commit readback failed"));
    }
    let timeline = timeline_view(&view, branch)?;
    if timeline
        .version_at_or_before(commit_timestamp)
        .matched_version()
        != Some(version)
        || timeline.timestamp_for_version(version) != Some(commit_timestamp)
    {
        return Err(TestkitError::new(
            "mixed cache commit timeline facts diverged",
        ));
    }
    Ok(1)
}

fn check_one_version_per_batch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 9));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(9))?;
    let key_a = physical_key(branch, 0x20, b"version-a".to_vec());
    let key_b = physical_key(branch, 0x21, b"version-b".to_vec());
    let outcome = fixture
        .execute(CommitBatch::mutating(
            branch,
            vec![
                CommitMutation::put(
                    key_a.clone(),
                    b"a".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                ),
                CommitMutation::put(
                    key_b.clone(),
                    b"b".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                ),
            ],
            CommitValidationFacts::empty(),
            CommitBatchOptions::default(),
        ))
        .map_err(testkit_error)?;
    let version = outcome
        .commit_version()
        .ok_or_else(|| TestkitError::new("cache outcome missed version"))?;
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    for key in [&key_a, &key_b] {
        if view
            .latest(key)
            .map_err(testkit_error)?
            .is_none_or(|row| row.row().commit_version() != version)
        {
            return Err(TestkitError::new("cache batch used multiple versions"));
        }
    }
    Ok(1)
}

fn check_one_timestamp_per_batch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 10));
    let expected_timestamp = timestamp(script_byte(script, 11));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), expected_timestamp)?;
    let key = physical_key(branch, 0x20, b"timestamp".to_vec());
    let outcome = fixture
        .execute(put_batch(branch, key.clone(), b"value".to_vec()))
        .map_err(testkit_error)?;
    if outcome.commit_timestamp() != Some(expected_timestamp) {
        return Err(TestkitError::new("cache outcome timestamp was wrong"));
    }
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let row = view
        .latest(&key)
        .map_err(testkit_error)?
        .ok_or_else(|| TestkitError::new("timestamp cache commit missed user row"))?;
    if row.row().commit_timestamp() != expected_timestamp {
        return Err(TestkitError::new("user row timestamp diverged"));
    }
    let timeline = timeline_view(&view, branch)?;
    for entry in timeline.entries() {
        if entry.commit_timestamp() != expected_timestamp {
            return Err(TestkitError::new("timeline row timestamp diverged"));
        }
    }
    Ok(1)
}

fn check_timeline_rows_installed(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 12));
    let expected_timestamp = timestamp(script_byte(script, 13));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), expected_timestamp)?;
    fixture
        .execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"timeline".to_vec()),
            b"value".to_vec(),
        ))
        .map_err(testkit_error)?;
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let timeline = timeline_view(&view, branch)?;
    if timeline
        .version_at_or_before(expected_timestamp)
        .matched_version()
        != Some(CommitVersion::new(1))
        || timeline.timestamp_for_version(CommitVersion::new(1)) != Some(expected_timestamp)
    {
        return Err(TestkitError::new(
            "cache commit did not install timeline rows",
        ));
    }
    Ok(1)
}

fn check_visible_publication(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 14));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(14))?;
    let outcome = fixture
        .execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"visible".to_vec()),
            b"value".to_vec(),
        ))
        .map_err(testkit_error)?;
    if fixture.visible.visible_version() != CommitVersion::new(1)
        || outcome.visibility_facts().visible_version() != Some(CommitVersion::new(1))
        || outcome.visibility_facts().durable_version().is_some()
    {
        return Err(TestkitError::new(
            "cache commit did not publish visible facts",
        ));
    }
    Ok(1)
}

fn check_not_durable_outcome(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 15));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(15))?;
    let outcome = fixture
        .execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"durability".to_vec()),
            b"value".to_vec(),
        ))
        .map_err(testkit_error)?;
    if outcome.kind() != CommitOutcomeKind::Visible
        || outcome.phase() != CommitPhase::Visible
        || outcome.durability() != CommitDurabilityClass::NotDurable
    {
        return Err(TestkitError::new("cache commit claimed wrong durability"));
    }
    Ok(1)
}

fn check_branch_admission_rejections(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 16));
    let batch = put_batch(
        branch,
        physical_key(branch, 0x20, b"admission".to_vec()),
        b"value".to_vec(),
    );

    let mut missing =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(16))?;
    missing.registry = CommitBranchRegistry::new();
    expect_error(
        missing.execute(batch.clone()),
        |error| matches!(error, CommitRuntimeError::BranchNotFound { .. }),
        "missing branch was not rejected",
    )?;
    require_unallocated_unmutated(&missing)?;

    let mut deleting =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(17))?;
    deleting
        .registry
        .mark_deleting(branch)
        .map_err(testkit_error)?;
    expect_error(
        deleting.execute(batch.clone()),
        |error| matches!(error, CommitRuntimeError::BranchNotWritable { .. }),
        "deleting branch was not rejected",
    )?;
    require_unallocated_unmutated(&deleting)?;

    let mut stale =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(18))?;
    expect_error(
        stale.execute_with_generation(
            batch,
            CommitBranchGeneration::new(2).map_err(testkit_error)?,
        ),
        |error| matches!(error, CommitRuntimeError::BranchGenerationMismatch { .. }),
        "stale generation was not rejected",
    )?;
    require_unallocated_unmutated(&stale)?;
    Ok(3)
}

fn check_conflict_rejection(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 19));
    let key = physical_key(branch, 0x20, b"conflict".to_vec());
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(19))?;
    fixture.seed_visible_row(StorageRow::put(
        key.clone(),
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        b"existing".to_vec(),
    ))?;
    fixture.catch_up_to(CommitVersion::new(1), Timestamp::from_micros(1));
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(1));
    let batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key.clone(),
            b"new".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::new(
            vec![CommitReadFact::new(key, CommitObservedVersion::Missing)],
            Vec::new(),
        ),
        CommitBatchOptions::default(),
    );
    let before = fixture.state.active_row_count();
    expect_error(
        fixture.execute(batch),
        |error| {
            matches!(
                error,
                CommitRuntimeError::CommitConflict { conflict }
                    if conflict.kind() == CommitConflictKind::ReadSet
            )
        },
        "conflict was not rejected",
    )?;
    if fixture.state.active_row_count() != before
        || fixture.allocator.version_allocator().last_allocated() != CommitVersion::new(1)
    {
        return Err(TestkitError::new("conflict rejection mutated state"));
    }
    Ok(1)
}

fn check_non_cache_rejections(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 20));
    let mut count = 0;
    for durability in [CommitDurabilityMode::Standard, CommitDurabilityMode::Always] {
        let mut fixture =
            CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(20))?;
        let options = CommitBatchOptions::new(
            durability,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        );
        let batch = CommitBatch::mutating(
            branch,
            vec![CommitMutation::delete(physical_key(
                branch,
                0x20,
                vec![
                    b'n',
                    u8::try_from(count).expect("non-cache case count is bounded"),
                ],
            ))],
            CommitValidationFacts::empty(),
            options,
        );
        expect_error(
            fixture.execute(batch),
            |error| matches!(error, CommitRuntimeError::DurabilityUnavailable { .. }),
            "non-cache durability was not rejected",
        )?;
        require_unallocated_unmutated(&fixture)?;
        count += 1;
    }
    Ok(count)
}

fn check_apply_failure_atomicity(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 21));
    let key = physical_key(branch, 0x20, b"atomic".to_vec());
    let row = StorageRow::put(
        key,
        CommitVersion::new(1),
        Timestamp::from_micros(1),
        Timestamp::EPOCH,
        b"value".to_vec(),
    );
    let mut state =
        BranchLocalState::new(branch, BranchRuntimeConfig::default()).map_err(testkit_error)?;
    if state
        .append_committed_rows_atomically(vec![row.clone(), row])
        .is_ok()
    {
        return Err(TestkitError::new(
            "duplicate atomic append was not rejected",
        ));
    }
    if state.active_row_count() != 0 || state.max_commit_version().is_some() {
        return Err(TestkitError::new(
            "failed atomic append mutated branch state",
        ));
    }
    Ok(1)
}

fn check_version_gap_after_post_allocation_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 22));
    let tight_config = CommitRuntimeConfig::new(1, 1, 2, CommitReadOnlyDiagnostics::Enabled)
        .map_err(testkit_error)?;
    let mut fixture = CacheContractFixture::new(branch, tight_config, timestamp(22))?;
    let batch = put_batch(
        branch,
        physical_key(branch, 0x20, b"gap".to_vec()),
        b"value".to_vec(),
    );
    expect_error(
        fixture.execute(batch),
        |error| matches!(error, CommitRuntimeError::InvalidBatch { .. }),
        "post-allocation row-limit failure did not reject",
    )?;
    if fixture.allocator.version_allocator().last_allocated() != CommitVersion::new(1)
        || fixture.state.active_row_count() != 0
    {
        return Err(TestkitError::new(
            "row-limit failure did not leave a clean gap",
        ));
    }

    fixture.config = CommitRuntimeConfig::default();
    let retry = fixture
        .execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"gap-retry".to_vec()),
            b"value".to_vec(),
        ))
        .map_err(testkit_error)?;
    if retry.commit_version() != Some(CommitVersion::new(2))
        || fixture.visible.visible_version() != CommitVersion::new(2)
    {
        return Err(TestkitError::new("retry did not advance past version gap"));
    }
    Ok(1)
}

fn check_applied_above_visible_rejection(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 23));
    let key = physical_key(branch, 0x20, b"hidden".to_vec());
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(23))?;
    fixture.seed_visible_row(StorageRow::put(
        key,
        CommitVersion::new(2),
        Timestamp::from_micros(2),
        Timestamp::EPOCH,
        b"hidden".to_vec(),
    ))?;
    let before_rows = fixture.state.active_row_count();
    expect_error(
        fixture.execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"reject".to_vec()),
            b"value".to_vec(),
        )),
        |error| matches!(error, CommitRuntimeError::InvalidCommitState { .. }),
        "branch-ahead-of-visible was not rejected",
    )?;
    require_unallocated(&fixture)?;
    if fixture.state.active_row_count() != before_rows
        || fixture.visible.visible_version() != CommitVersion::ZERO
    {
        return Err(TestkitError::new(
            "branch-ahead-of-visible rejection mutated state",
        ));
    }
    Ok(1)
}

fn check_visible_allocator_mismatch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 24));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(24))?;
    fixture.visible = VisibleVersionTracker::new(CommitVersion::new(9));
    expect_error(
        fixture.execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"visible-mismatch".to_vec()),
            b"value".to_vec(),
        )),
        |error| matches!(error, CommitRuntimeError::InvalidCommitState { .. }),
        "allocator/visible mismatch was not rejected",
    )?;
    if fixture.allocator.version_allocator().last_allocated() != CommitVersion::new(1)
        || fixture.state.active_row_count() != 0
        || fixture.visible.visible_version() != CommitVersion::new(9)
    {
        return Err(TestkitError::new(
            "allocator/visible mismatch changed the wrong state",
        ));
    }
    Ok(1)
}

fn check_guard_release_after_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 25));
    let mut fixture =
        CacheContractFixture::new(branch, CommitRuntimeConfig::default(), timestamp(25))?;
    let held = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .map_err(testkit_error)?;
    expect_error(
        fixture.execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"guarded".to_vec()),
            b"value".to_vec(),
        )),
        |error| matches!(error, CommitRuntimeError::BranchGuardUnavailable { .. }),
        "held guard did not reject cache commit",
    )?;
    drop(held);
    fixture
        .execute(put_batch(
            branch,
            physical_key(branch, 0x20, b"after-guard".to_vec()),
            b"value".to_vec(),
        ))
        .map_err(testkit_error)?;
    Ok(1)
}

struct CacheContractFixture {
    config: CommitRuntimeConfig,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    state: BranchLocalState,
    visible: VisibleVersionTracker,
    durable_gate: crate::commit::CommitUnresolvedDurableGate,
}

impl CacheContractFixture {
    fn new(
        branch: BranchId,
        config: CommitRuntimeConfig,
        timestamp: Timestamp,
    ) -> Result<Self, TestkitError> {
        let mut registry = CommitBranchRegistry::new();
        registry
            .register_active(
                branch,
                CommitBranchGeneration::new(1).map_err(testkit_error)?,
            )
            .map_err(testkit_error)?;
        Ok(Self {
            config,
            registry,
            guard_set: CommitBranchGuardSet::new(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                CommitManualTimestampSource::new(timestamp),
            ),
            state: BranchLocalState::new(branch, BranchRuntimeConfig::default())
                .map_err(testkit_error)?,
            visible: VisibleVersionTracker::default(),
            durable_gate: crate::commit::CommitUnresolvedDurableGate::new(),
        })
    }

    fn execute(&mut self, batch: CommitBatch) -> CommitRuntimeResult<crate::commit::CommitOutcome> {
        self.execute_with_generation(batch, CommitBranchGeneration::new(1).expect("generation"))
    }

    fn execute_with_generation(
        &mut self,
        batch: CommitBatch,
        generation: CommitBranchGeneration,
    ) -> CommitRuntimeResult<crate::commit::CommitOutcome> {
        CommitCacheRuntime::new(
            &self.config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.state,
            &mut self.visible,
            &self.durable_gate,
        )
        .execute(batch, CommitBranchGenerationGuard::exact(generation))
    }

    fn seed_visible_row(&mut self, row: StorageRow) -> Result<(), TestkitError> {
        self.state
            .append_committed_row(row)
            .map(|_| ())
            .map_err(testkit_error)
    }

    fn catch_up_to(&mut self, version: CommitVersion, timestamp: Timestamp) {
        self.allocator.catch_up_to_recovered_version(version);
        self.allocator.catch_up_to_recovered_timestamp(timestamp);
    }
}

fn require_visible_cache_outcome(
    outcome: &crate::commit::CommitOutcome,
    version: CommitVersion,
    timestamp: Timestamp,
) -> Result<(), TestkitError> {
    if outcome.kind() != CommitOutcomeKind::Visible
        || outcome.phase() != CommitPhase::Visible
        || outcome.durability() != CommitDurabilityClass::NotDurable
        || outcome.commit_version() != Some(version)
        || outcome.commit_timestamp() != Some(timestamp)
    {
        return Err(TestkitError::new(
            "cache outcome did not report visible success",
        ));
    }
    Ok(())
}

fn require_unallocated_unmutated(fixture: &CacheContractFixture) -> Result<(), TestkitError> {
    require_unallocated(fixture)?;
    if fixture.state.active_row_count() != 0
        || fixture.visible.visible_version() != CommitVersion::ZERO
    {
        return Err(TestkitError::new(
            "rejected cache commit mutated branch or visibility",
        ));
    }
    Ok(())
}

fn require_unallocated(fixture: &CacheContractFixture) -> Result<(), TestkitError> {
    if fixture.allocator.version_allocator().last_allocated() != CommitVersion::ZERO {
        return Err(TestkitError::new(
            "rejected cache commit allocated a version",
        ));
    }
    Ok(())
}

fn expect_error<T: std::fmt::Debug>(
    result: CommitRuntimeResult<T>,
    predicate: impl FnOnce(&CommitRuntimeError) -> bool,
    message: &'static str,
) -> Result<(), TestkitError> {
    match result {
        Err(error) if predicate(&error) => Ok(()),
        other => Err(TestkitError::new(format!("{message}: {other:?}"))),
    }
}

fn put_batch(branch: BranchId, key: PhysicalKey, value: Vec<u8>) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
}

fn delete_batch(branch: BranchId, key: PhysicalKey) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(key)],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
}

fn timeline_view(
    view: &crate::branch::read::BranchReadView,
    branch: BranchId,
) -> Result<CommitTimelineView, TestkitError> {
    let bounds = BranchScanBounds::unbounded(
        branch,
        COMMIT_TIMELINE_SPACE,
        StorageSpaceId::COMMIT_TIMELINE,
    )
    .map_err(testkit_error)?;
    let rows = view
        .scan_range(&bounds, BranchReadBound::latest())
        .map_err(testkit_error)?;
    CommitTimelineView::from_rows(
        branch,
        rows.iter().map(crate::branch::read::BranchVisibleRow::row),
    )
    .map_err(testkit_error)
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_id: BranchId, storage_space_id: u8, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "default",
        StorageSpaceId::engine(storage_space_id).expect("engine-owned space"),
        user_key,
    )
    .expect("physical key")
}

fn timestamp(byte: u8) -> Timestamp {
    Timestamp::from_micros(u64::from(byte).saturating_add(1_000))
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}
