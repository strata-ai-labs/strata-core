//! Generated durable/WAL commit contract helpers.

use crate::branch::{BranchLocalState, BranchReadBound, BranchRuntimeConfig, BranchScanBounds};
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitBranchGuardSet, CommitBranchRegistry, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitDurableRuntime,
    CommitExpiry, CommitFactAllocator, CommitLowerLayer, CommitManualTimestampSource,
    CommitMutation, CommitOrigin, CommitOutcome, CommitOutcomeKind, CommitPhase,
    CommitReadOnlyDiagnostics, CommitRetentionHint, CommitRuntimeConfig, CommitRuntimeError,
    CommitTimelineRows, CommitTimelineView, CommitTimestampGuard, CommitTimestampPolicy,
    CommitValidationFacts, CommitVersionAllocator, CommitWalAppendError, CommitWalAppendFacts,
    CommitWalAppender, VisibleVersionTracker, COMMIT_TIMELINE_SPACE,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::WalRecord;
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

pub(crate) struct CommitRuntimeDurableContract {
    pub(crate) standard_commits: usize,
    pub(crate) always_commits: usize,
    pub(crate) wal_payload_parity: usize,
    pub(crate) clean_wal_failures: usize,
    pub(crate) uncertain_wal_failures: usize,
    pub(crate) cache_mode_rejections: usize,
    pub(crate) policy_mismatches: usize,
    pub(crate) unforced_always_rejections: usize,
    pub(crate) guard_release_after_failure: usize,
    pub(crate) read_only_rejections: usize,
}

pub(crate) fn check_commit_runtime_durable_contract(
    script: &[u8],
) -> Result<CommitRuntimeDurableContract, TestkitError> {
    Ok(CommitRuntimeDurableContract {
        standard_commits: check_standard_success(script)?,
        always_commits: check_always_success(script)?,
        wal_payload_parity: check_wal_payload_parity(script)?,
        clean_wal_failures: check_clean_wal_failure(script)?,
        uncertain_wal_failures: check_uncertain_wal_failure(script)?,
        cache_mode_rejections: check_cache_mode_rejection(script)?,
        policy_mismatches: check_policy_mismatch(script)?,
        unforced_always_rejections: check_unforced_always_rejection(script)?,
        guard_release_after_failure: check_guard_release_after_failure(script)?,
        read_only_rejections: check_read_only_rejection(script)?,
    })
}

fn check_standard_success(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 0));
    let key = physical_key(branch, 0x20, vec![b's', script_byte(script, 1)]);
    let commit_timestamp = timestamp(script_byte(script, 2));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
        commit_timestamp,
    )?;

    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                key.clone(),
                vec![script_byte(script, 3), 0x5a],
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        ))
        .map_err(testkit_error)?;

    require_visible_durable_outcome(
        &outcome,
        CommitDurabilityClass::Standard,
        CommitVersion::new(1),
        commit_timestamp,
    )?;
    require_record_payload(
        single_record(&fixture)?,
        branch,
        CommitVersion::new(1),
        commit_timestamp,
        &[(&key, false)],
    )?;
    require_readback(&fixture, &key, Some(CommitVersion::new(1)))?;
    require_timeline_visible(&fixture, branch, CommitVersion::new(1), commit_timestamp)?;
    Ok(1)
}

fn check_always_success(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 4));
    let key = physical_key(branch, 0x20, vec![b'a', script_byte(script, 5)]);
    let commit_timestamp = timestamp(script_byte(script, 6));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::Succeed {
            forced_durable: true,
        },
        commit_timestamp,
    )?;

    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Always,
            vec![CommitMutation::put(
                key.clone(),
                b"always".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        ))
        .map_err(testkit_error)?;

    require_visible_durable_outcome(
        &outcome,
        CommitDurabilityClass::Always,
        CommitVersion::new(1),
        commit_timestamp,
    )?;
    require_record_payload(
        single_record(&fixture)?,
        branch,
        CommitVersion::new(1),
        commit_timestamp,
        &[(&key, false)],
    )?;
    Ok(1)
}

fn check_wal_payload_parity(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 7));
    let first = physical_key(branch, 0x20, vec![b'p', 0]);
    let deleted = physical_key(branch, 0x21, vec![b'p', 1]);
    let second = physical_key(branch, 0x22, vec![b'p', 2]);
    let commit_timestamp = timestamp(script_byte(script, 8));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
        commit_timestamp,
    )?;

    let outcome = fixture
        .execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
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
        ))
        .map_err(testkit_error)?;

    require_visible_durable_outcome(
        &outcome,
        CommitDurabilityClass::Standard,
        CommitVersion::new(1),
        commit_timestamp,
    )?;
    require_record_payload(
        single_record(&fixture)?,
        branch,
        CommitVersion::new(1),
        commit_timestamp,
        &[(&first, false), (&deleted, true), (&second, false)],
    )?;
    require_readback(&fixture, &first, Some(CommitVersion::new(1)))?;
    require_readback(&fixture, &deleted, None)?;
    require_readback(&fixture, &second, Some(CommitVersion::new(1)))?;
    Ok(1)
}

fn check_clean_wal_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 9));
    let key = physical_key(branch, 0x20, b"clean-failure".to_vec());
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::CleanFailure,
        timestamp(script_byte(script, 10)),
    )?;

    expect_error(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Standard,
            vec![CommitMutation::put(
                key.clone(),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        |error| {
            matches!(
                error,
                CommitRuntimeError::LowerLayer {
                    layer: CommitLowerLayer::WalService,
                    reason: "injected clean WAL failure",
                    ..
                }
            )
        },
        "clean WAL failure was not classified as lower-layer clean failure",
    )?;
    require_allocated_not_applied(&fixture)?;
    require_readback(&fixture, &key, None)?;
    Ok(1)
}

fn check_uncertain_wal_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 11));
    let key = physical_key(branch, 0x20, b"uncertain-failure".to_vec());
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::UncertainFailure,
        timestamp(script_byte(script, 12)),
    )?;

    expect_error(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Always,
            vec![CommitMutation::put(
                key.clone(),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        |error| {
            matches!(
                error,
                CommitRuntimeError::DurabilityUncertain {
                    branch_id: failed_branch,
                    commit_version,
                    reason: "WAL append durability is uncertain",
                    ..
                } if *failed_branch == branch && *commit_version == CommitVersion::new(1)
            )
        },
        "uncertain WAL failure was not classified as durability-uncertain",
    )?;
    require_allocated_not_applied(&fixture)?;
    require_readback(&fixture, &key, None)?;
    Ok(1)
}

fn check_cache_mode_rejection(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 13));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
        timestamp(script_byte(script, 14)),
    )?;

    expect_error(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Cache,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"cache-reject".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        |error| {
            error
                == &CommitRuntimeError::DurabilityUnavailable {
                    reason: "durable commit executor requires durable mode",
                }
        },
        "cache-mode batch was not rejected by durable executor",
    )?;
    require_unallocated_unattempted(&fixture)?;
    Ok(1)
}

fn check_policy_mismatch(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 15));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: true,
        },
        timestamp(script_byte(script, 16)),
    )?;

    expect_error(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Always,
            vec![CommitMutation::put(
                physical_key(branch, 0x20, b"policy-mismatch".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        |error| {
            error
                == &CommitRuntimeError::DurabilityUnavailable {
                    reason: "durable commit executor requires matching WAL durability policy",
                }
        },
        "durability policy mismatch was not rejected before WAL append",
    )?;
    require_unallocated_unattempted(&fixture)?;
    Ok(1)
}

fn check_unforced_always_rejection(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 17));
    let key = physical_key(branch, 0x20, b"unforced-always".to_vec());
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
        timestamp(script_byte(script, 18)),
    )?;

    expect_error(
        fixture.execute(durable_batch(
            branch,
            CommitDurabilityMode::Always,
            vec![CommitMutation::put(
                key.clone(),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            )],
        )),
        |error| {
            matches!(
                error,
                CommitRuntimeError::DurabilityUncertain {
                    branch_id: failed_branch,
                    commit_version,
                    reason: "always durability requires a forced WAL append",
                    ..
                } if *failed_branch == branch && *commit_version == CommitVersion::new(1)
            )
        },
        "unforced always append was not rejected before L6 apply",
    )?;
    if fixture.wal.records.len() != 1 || fixture.state.active_row_count() != 0 {
        return Err(TestkitError::new(
            "unforced always append mutated branch state incorrectly",
        ));
    }
    require_readback(&fixture, &key, None)?;
    Ok(1)
}

fn check_guard_release_after_failure(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 19));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::default(),
        DurabilityPolicy::Always,
        FakeWalMode::UncertainFailure,
        timestamp(script_byte(script, 20)),
    )?;
    let _ = fixture.execute(durable_batch(
        branch,
        CommitDurabilityMode::Always,
        vec![CommitMutation::put(
            physical_key(branch, 0x20, b"guard-release".to_vec()),
            b"value".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
    ));
    let _guard = fixture
        .guard_set
        .try_acquire_branch_guard(branch)
        .map_err(|error| TestkitError::new(format!("durable guard was not released: {error}")))?;
    Ok(1)
}

fn check_read_only_rejection(script: &[u8]) -> Result<usize, TestkitError> {
    let branch = branch_id(script_byte(script, 21));
    let mut fixture = DurableContractFixture::new(
        branch,
        CommitRuntimeConfig::new(1, 1, 1, CommitReadOnlyDiagnostics::Enabled)
            .map_err(testkit_error)?,
        DurabilityPolicy::Standard,
        FakeWalMode::Succeed {
            forced_durable: false,
        },
        timestamp(script_byte(script, 22)),
    )?;

    expect_error(
        fixture.execute(CommitBatch::read_only_diagnostic(
            branch,
            CommitValidationFacts::empty(),
            CommitBatchOptions::new(
                CommitDurabilityMode::Standard,
                CommitConflictValidationMode::Validate,
                CommitDuplicateKeyPolicy::Reject,
                CommitTimestampPolicy::RuntimeGenerated,
                CommitOrigin::StorageRuntime,
            ),
        )),
        |error| {
            error
                == &CommitRuntimeError::InvalidBatch {
                    reason: "durable commit executor requires mutating batch",
                }
        },
        "read-only durable batch was not rejected",
    )?;
    require_unallocated_unattempted(&fixture)?;
    Ok(1)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FakeWalMode {
    Succeed { forced_durable: bool },
    CleanFailure,
    UncertainFailure,
}

#[derive(Debug)]
struct RecordingWalAppender {
    policy: DurabilityPolicy,
    mode: FakeWalMode,
    records: Vec<WalRecord>,
    append_attempts: usize,
}

impl RecordingWalAppender {
    const fn new(policy: DurabilityPolicy, mode: FakeWalMode) -> Self {
        Self {
            policy,
            mode,
            records: Vec::new(),
            append_attempts: 0,
        }
    }
}

impl CommitWalAppender for RecordingWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy {
        self.policy
    }

    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError> {
        self.append_attempts = self.append_attempts.saturating_add(1);
        match self.mode {
            FakeWalMode::Succeed { forced_durable } => {
                self.records.push(record.clone());
                Ok(CommitWalAppendFacts::new(0, 0, 128, 128, forced_durable))
            }
            FakeWalMode::CleanFailure => Err(CommitWalAppendError::clean(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "injected clean WAL failure",
                ),
            )),
            FakeWalMode::UncertainFailure => Err(CommitWalAppendError::uncertain(
                CommitRuntimeError::lower_layer(
                    CommitLowerLayer::WalService,
                    "injected uncertain WAL failure",
                ),
            )),
        }
    }
}

struct DurableContractFixture {
    config: CommitRuntimeConfig,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<CommitManualTimestampSource>,
    state: BranchLocalState,
    visible: VisibleVersionTracker,
    wal: RecordingWalAppender,
}

impl DurableContractFixture {
    fn new(
        branch: BranchId,
        config: CommitRuntimeConfig,
        policy: DurabilityPolicy,
        wal_mode: FakeWalMode,
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
            wal: RecordingWalAppender::new(policy, wal_mode),
        })
    }

    fn execute(&mut self, batch: CommitBatch) -> Result<CommitOutcome, CommitRuntimeError> {
        CommitDurableRuntime::new(
            &self.config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.state,
            &mut self.visible,
            &mut self.wal,
        )
        .execute(
            batch,
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
    }
}

fn durable_batch(
    branch: BranchId,
    durability: CommitDurabilityMode,
    mutations: Vec<CommitMutation>,
) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        mutations,
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            durability,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn require_visible_durable_outcome(
    outcome: &CommitOutcome,
    durability: CommitDurabilityClass,
    version: CommitVersion,
    timestamp: Timestamp,
) -> Result<(), TestkitError> {
    if outcome.kind() != CommitOutcomeKind::Visible
        || outcome.phase() != CommitPhase::Visible
        || outcome.durability() != durability
        || outcome.commit_version() != Some(version)
        || outcome.commit_timestamp() != Some(timestamp)
    {
        return Err(TestkitError::new(
            "durable outcome did not report visible durable success",
        ));
    }
    let facts = outcome.visibility_facts();
    if facts.allocated_version() != Some(version)
        || facts.durable_version() != Some(version)
        || facts.applied_version() != Some(version)
        || facts.visible_version() != Some(version)
        || facts.timeline_version() != Some(version)
    {
        return Err(TestkitError::new(
            "durable outcome visibility facts were not fully visible",
        ));
    }
    Ok(())
}

fn require_record_payload(
    record: &WalRecord,
    branch: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
    expected_user_rows: &[(&PhysicalKey, bool)],
) -> Result<(), TestkitError> {
    if record.branch_id() != branch
        || record.commit_version() != version
        || record.commit_timestamp() != timestamp
    {
        return Err(TestkitError::new("WAL record outer facts diverged"));
    }
    let rows = record.commit_payload().rows();
    let expected_rows = expected_user_rows
        .len()
        .saturating_add(CommitTimelineRows::timeline_row_count());
    if rows.len() != expected_rows {
        return Err(TestkitError::new("WAL payload row count diverged"));
    }
    for row in rows {
        if row.physical_key().branch_id() != branch
            || row.commit_version() != version
            || row.commit_timestamp() != timestamp
        {
            return Err(TestkitError::new(
                "WAL payload row facts diverged from record facts",
            ));
        }
    }
    for (row, (expected_key, expected_tombstone)) in rows.iter().zip(expected_user_rows.iter()) {
        if row.physical_key() != *expected_key || row.is_tombstone() != *expected_tombstone {
            return Err(TestkitError::new("WAL user row ordering diverged"));
        }
    }
    for row in &rows[expected_user_rows.len()..] {
        if row.physical_key().storage_space_id() != StorageSpaceId::COMMIT_TIMELINE {
            return Err(TestkitError::new(
                "WAL payload did not append timeline rows after user rows",
            ));
        }
    }
    Ok(())
}

fn require_readback(
    fixture: &DurableContractFixture,
    key: &PhysicalKey,
    expected_version: Option<CommitVersion>,
) -> Result<(), TestkitError> {
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let row = view.latest(key).map_err(testkit_error)?;
    match (expected_version, row) {
        (Some(version), Some(row)) if row.row().commit_version() == version => Ok(()),
        (None, None) => Ok(()),
        _ => Err(TestkitError::new(
            "durable readback did not match expected row",
        )),
    }
}

fn require_timeline_visible(
    fixture: &DurableContractFixture,
    branch: BranchId,
    version: CommitVersion,
    timestamp: Timestamp,
) -> Result<(), TestkitError> {
    let view = fixture.state.capture_read_view().map_err(testkit_error)?;
    let timeline = timeline_view(&view, branch)?;
    if timeline.version_at_or_before(timestamp).matched_version() != Some(version)
        || timeline.timestamp_for_version(version) != Some(timestamp)
    {
        return Err(TestkitError::new("durable timeline rows were not visible"));
    }
    Ok(())
}

fn require_allocated_not_applied(fixture: &DurableContractFixture) -> Result<(), TestkitError> {
    if fixture.allocator.version_allocator().last_allocated() != CommitVersion::new(1)
        || fixture.state.active_row_count() != 0
        || fixture.visible.visible_version() != CommitVersion::ZERO
        || fixture.wal.append_attempts != 1
    {
        return Err(TestkitError::new(
            "WAL failure did not leave allocated-not-applied state",
        ));
    }
    Ok(())
}

fn require_unallocated_unattempted(fixture: &DurableContractFixture) -> Result<(), TestkitError> {
    if fixture.allocator.version_allocator().last_allocated() != CommitVersion::ZERO
        || fixture.state.active_row_count() != 0
        || fixture.visible.visible_version() != CommitVersion::ZERO
        || fixture.wal.append_attempts != 0
    {
        return Err(TestkitError::new(
            "pre-allocation durable rejection mutated state or attempted WAL",
        ));
    }
    Ok(())
}

fn single_record(fixture: &DurableContractFixture) -> Result<&WalRecord, TestkitError> {
    if fixture.wal.records.len() != 1 || fixture.wal.append_attempts != 1 {
        return Err(TestkitError::new(
            "durable contract expected one WAL append",
        ));
    }
    Ok(&fixture.wal.records[0])
}

fn timeline_view(
    view: &crate::branch::BranchReadView,
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
        rows.iter().map(crate::branch::BranchVisibleRow::row),
    )
    .map_err(testkit_error)
}

fn expect_error<T: std::fmt::Debug>(
    result: Result<T, CommitRuntimeError>,
    predicate: impl FnOnce(&CommitRuntimeError) -> bool,
    message: &'static str,
) -> Result<(), TestkitError> {
    match result {
        Err(error) if predicate(&error) => Ok(()),
        other => Err(TestkitError::new(format!("{message}: {other:?}"))),
    }
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
