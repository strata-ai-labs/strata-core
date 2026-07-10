use super::*;

#[test]
fn commit_version_allocator_starts_after_floor_and_never_returns_zero() {
    let mut allocator = CommitVersionAllocator::default();

    assert_eq!(allocator.last_allocated(), CommitVersion::ZERO);
    assert_eq!(
        allocator.allocate_next().expect("first allocation"),
        CommitVersion::new(1)
    );
    assert_eq!(
        allocator.allocate_next().expect("second allocation"),
        CommitVersion::new(2)
    );

    let mut recovered = CommitVersionAllocator::new(CommitVersion::new(40));
    assert_eq!(
        recovered.allocate_next().expect("allocation after floor"),
        CommitVersion::new(41)
    );
}

#[test]
fn commit_version_allocator_catches_up_idempotently() {
    let mut allocator = CommitVersionAllocator::new(CommitVersion::new(10));

    allocator.catch_up_to(CommitVersion::new(7));
    assert_eq!(allocator.last_allocated(), CommitVersion::new(10));
    allocator.catch_up_to(CommitVersion::new(10));
    assert_eq!(allocator.last_allocated(), CommitVersion::new(10));
    allocator.catch_up_to(CommitVersion::new(30));
    assert_eq!(allocator.last_allocated(), CommitVersion::new(30));
    allocator.catch_up_to(CommitVersion::ZERO);
    assert_eq!(allocator.last_allocated(), CommitVersion::new(30));
    assert_eq!(
        allocator
            .allocate_next()
            .expect("allocation after catch-up"),
        CommitVersion::new(31)
    );
}

#[test]
fn commit_version_allocator_overflow_is_typed_and_stable() {
    let mut allocator = CommitVersionAllocator::new(CommitVersion::MAX);
    let expected = CommitRuntimeError::VersionAllocatorOverflow {
        last_allocated: CommitVersion::MAX,
    };

    assert_eq!(allocator.preview_next(), Err(expected.clone()));
    assert_eq!(allocator.allocate_next(), Err(expected.clone()));
    assert_eq!(allocator.last_allocated(), CommitVersion::MAX);
    assert_eq!(allocator.allocate_next(), Err(expected));
}

#[test]
fn commit_version_allocator_allocates_max_then_overflows() {
    let mut allocator = CommitVersionAllocator::new(CommitVersion::new(u64::MAX - 1));

    assert_eq!(
        allocator.preview_next().expect("max version preview"),
        CommitVersion::MAX
    );
    assert_eq!(
        allocator.allocate_next().expect("max version allocation"),
        CommitVersion::MAX
    );
    assert_eq!(
        allocator.allocate_next(),
        Err(CommitRuntimeError::VersionAllocatorOverflow {
            last_allocated: CommitVersion::MAX,
        })
    );
}

#[test]
fn commit_timestamp_guard_accepts_equal_and_clamps_generated_backwards_time() {
    let mut guard = CommitTimestampGuard::default();
    let ten = Timestamp::from_micros(10);
    let twenty = Timestamp::from_micros(20);
    let five = Timestamp::from_micros(5);

    assert_eq!(
        guard.guard_generated(ten),
        (ten, CommitTimestampAllocationSource::RuntimeGenerated)
    );
    assert_eq!(guard.last_allocated(), Some(ten));
    assert_eq!(
        guard.guard_generated(ten),
        (ten, CommitTimestampAllocationSource::RuntimeGenerated)
    );
    assert_eq!(
        guard.guard_generated(twenty),
        (twenty, CommitTimestampAllocationSource::RuntimeGenerated)
    );
    assert_eq!(guard.last_allocated(), Some(twenty));
    assert_eq!(
        guard.guard_generated(five),
        (
            twenty,
            CommitTimestampAllocationSource::RuntimeGeneratedClamped
        )
    );
    assert_eq!(guard.last_allocated(), Some(twenty));
}

#[test]
fn commit_timestamp_guard_rejects_explicit_time_before_floor() {
    let mut guard = CommitTimestampGuard::new(Some(Timestamp::from_micros(100)));

    assert_eq!(
        guard
            .guard_explicit(Timestamp::from_micros(99))
            .expect_err("timestamp below floor"),
        CommitRuntimeError::InvalidTimestampPolicy {
            reason: "explicit commit timestamp is before the monotonic floor",
        }
    );
    assert_eq!(guard.last_allocated(), Some(Timestamp::from_micros(100)));
    assert_eq!(
        guard
            .guard_explicit(Timestamp::from_micros(100))
            .expect("equal explicit timestamp"),
        Timestamp::from_micros(100)
    );
    assert_eq!(
        guard
            .guard_explicit(Timestamp::from_micros(101))
            .expect("new explicit timestamp"),
        Timestamp::from_micros(101)
    );
    assert_eq!(guard.last_allocated(), Some(Timestamp::from_micros(101)));
}

#[test]
fn commit_timestamp_guard_catches_up_without_regressing_floor() {
    let mut guard = CommitTimestampGuard::new(Some(Timestamp::from_micros(20)));

    guard.catch_up_to(Timestamp::from_micros(10));
    assert_eq!(guard.last_allocated(), Some(Timestamp::from_micros(20)));
    guard.catch_up_to(Timestamp::from_micros(20));
    assert_eq!(guard.last_allocated(), Some(Timestamp::from_micros(20)));
    guard.catch_up_to(Timestamp::from_micros(30));
    assert_eq!(guard.last_allocated(), Some(Timestamp::from_micros(30)));
}

#[test]
fn manual_timestamp_source_is_deterministic_and_mutable() {
    let mut source = CommitManualTimestampSource::new(Timestamp::from_micros(77));

    assert_eq!(
        source.next_configured_timestamp(),
        Timestamp::from_micros(77)
    );
    assert_eq!(
        source.next_timestamp().expect("manual timestamp"),
        Timestamp::from_micros(77)
    );
    source.set_next_timestamp(Timestamp::EPOCH);
    assert_eq!(
        source.next_timestamp().expect("updated manual timestamp"),
        Timestamp::EPOCH
    );
}

#[test]
fn commit_fact_allocator_allocates_one_stamp_for_mutating_batch() {
    let branch = branch_id(40);
    let batch = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    );
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(50)),
    );

    let allocation = allocator
        .allocate_for_batch(&batch)
        .expect("mutating allocation");

    assert_eq!(allocation.branch_id(), branch);
    assert_eq!(
        allocation.timestamp_source(),
        Some(CommitTimestampAllocationSource::RuntimeGenerated)
    );
    let stamp = allocation.stamp().expect("mutating stamp");
    assert_eq!(stamp.branch_id(), branch);
    assert_eq!(stamp.commit_version(), CommitVersion::new(1));
    assert_eq!(stamp.commit_timestamp(), Timestamp::from_micros(50));
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(1)
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(50))
    );
}

#[test]
fn commit_fact_allocator_clamps_generated_timestamps_but_keeps_versions_increasing() {
    let branch = branch_id(41);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(9)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(100))),
        CommitManualTimestampSource::new(Timestamp::from_micros(80)),
    );

    let allocation = allocator
        .allocate_for_batch(&batch)
        .expect("clamped allocation");
    let stamp = allocation.stamp().expect("mutating stamp");

    assert_eq!(stamp.commit_version(), CommitVersion::new(10));
    assert_eq!(stamp.commit_timestamp(), Timestamp::from_micros(100));
    assert_eq!(
        allocation.timestamp_source(),
        Some(CommitTimestampAllocationSource::RuntimeGeneratedClamped)
    );
}

#[test]
fn commit_fact_allocator_allows_equal_generated_timestamps_across_commits() {
    let branch = branch_id(49);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(70)),
    );

    let first = allocator
        .allocate_for_batch(&batch)
        .expect("first allocation")
        .stamp()
        .expect("first stamp");
    let second = allocator
        .allocate_for_batch(&batch)
        .expect("second allocation")
        .stamp()
        .expect("second stamp");

    assert_eq!(first.commit_version(), CommitVersion::new(1));
    assert_eq!(second.commit_version(), CommitVersion::new(2));
    assert_eq!(first.commit_timestamp(), Timestamp::from_micros(70));
    assert_eq!(second.commit_timestamp(), Timestamp::from_micros(70));
}

#[test]
fn commit_fact_allocator_reads_generated_timestamps_in_source_order() {
    let branch = branch_id(51);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        SequenceTimestampSource::new(vec![Timestamp::from_micros(70), Timestamp::from_micros(90)]),
    );

    let first = allocator
        .allocate_for_batch(&batch)
        .expect("first allocation")
        .stamp()
        .expect("first stamp");
    let second = allocator
        .allocate_for_batch(&batch)
        .expect("second allocation")
        .stamp()
        .expect("second stamp");

    assert_eq!(first.commit_timestamp(), Timestamp::from_micros(70));
    assert_eq!(second.commit_timestamp(), Timestamp::from_micros(90));
    assert_eq!(first.commit_version(), CommitVersion::new(1));
    assert_eq!(second.commit_version(), CommitVersion::new(2));
    assert_eq!(allocator.source().reads(), 2);
}

#[test]
fn commit_fact_allocator_accepts_equal_explicit_timestamp() {
    let branch = branch_id(42);
    let batch = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(100)),
            CommitOrigin::StorageRuntime,
        ),
    );
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(2)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(100))),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );

    let allocation = allocator
        .allocate_for_batch(&batch)
        .expect("explicit allocation");
    let stamp = allocation.stamp().expect("mutating stamp");

    assert_eq!(stamp.commit_version(), CommitVersion::new(3));
    assert_eq!(stamp.commit_timestamp(), Timestamp::from_micros(100));
    assert_eq!(
        allocation.timestamp_source(),
        Some(CommitTimestampAllocationSource::Explicit)
    );
}

/// BS5.0 regression: an internally generated timestamp base read BEFORE the runtime lock must
/// CLAMP to the monotonic floor, never reject — with concurrent writers, another commit routinely
/// advances the floor between the pre-lock read and allocation (the multi-writer stress caught the
/// old Explicit routing failing spuriously with "before the monotonic floor").
#[test]
fn commit_fact_allocator_clamps_pre_lock_generated_base_below_floor() {
    let branch = branch_id(48);
    let batch = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGeneratedBase(Timestamp::from_micros(99)),
            CommitOrigin::StorageRuntime,
        ),
    );
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(10)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(100))),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );

    let allocation = allocator
        .allocate_for_batch(&batch)
        .expect("stale generated base clamps instead of rejecting");
    let stamp = allocation.stamp().expect("mutating stamp");

    assert_eq!(stamp.commit_version(), CommitVersion::new(11));
    assert_eq!(stamp.commit_timestamp(), Timestamp::from_micros(100));
    assert_eq!(
        allocation.timestamp_source(),
        Some(CommitTimestampAllocationSource::RuntimeGeneratedClamped)
    );

    // A base at-or-above the floor passes through unclamped.
    let fresh = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGeneratedBase(Timestamp::from_micros(120)),
            CommitOrigin::StorageRuntime,
        ),
    );
    let allocation = allocator
        .allocate_for_batch(&fresh)
        .expect("fresh generated base allocates");
    assert_eq!(
        allocation
            .stamp()
            .expect("mutating stamp")
            .commit_timestamp(),
        Timestamp::from_micros(120)
    );
    assert_eq!(
        allocation.timestamp_source(),
        Some(CommitTimestampAllocationSource::RuntimeGenerated)
    );
}

#[test]
fn commit_fact_allocator_rejects_invalid_explicit_timestamp_before_version_allocation() {
    let branch = branch_id(43);
    let batch = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(99)),
            CommitOrigin::StorageRuntime,
        ),
    );
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(10)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(100))),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );

    assert_eq!(
        allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::InvalidTimestampPolicy {
            reason: "explicit commit timestamp is before the monotonic floor",
        })
    );
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(10)
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(100))
    );
}

#[test]
fn commit_fact_allocator_source_failure_does_not_consume_version() {
    let branch = branch_id(44);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(4)),
        CommitTimestampGuard::default(),
        FailingTimestampSource,
    );

    let err = allocator
        .allocate_for_batch(&batch)
        .expect_err("source failure");

    assert_eq!(
        err,
        CommitRuntimeError::timestamp_unavailable("manual failure")
    );
    assert_eq!(err.source().map(ToString::to_string), None);
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(4)
    );
    assert_eq!(allocator.timestamp_guard().last_allocated(), None);
}

#[test]
fn commit_fact_allocator_source_failure_preserves_source_chain() {
    let branch = branch_id(47);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(4)),
        CommitTimestampGuard::default(),
        ChainedFailingTimestampSource,
    );

    let err = allocator
        .allocate_for_batch(&batch)
        .expect_err("source failure");

    assert_eq!(
        err,
        CommitRuntimeError::timestamp_unavailable_with("manual failure", TimestampSourceCause)
    );
    assert_eq!(
        err.source().map(ToString::to_string),
        Some(String::from("timestamp source cause"))
    );
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(4)
    );
}

#[test]
fn commit_fact_allocator_version_overflow_does_not_advance_timestamp_floor() {
    let branch = branch_id(48);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::MAX),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(10))),
        CommitManualTimestampSource::new(Timestamp::from_micros(20)),
    );

    assert_eq!(
        allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::VersionAllocatorOverflow {
            last_allocated: CommitVersion::MAX,
        })
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(10))
    );
}

#[test]
fn commit_fact_allocator_version_overflow_does_not_read_timestamp_source() {
    let branch = branch_id(50);
    let batch = mutating_batch(branch, CommitBatchOptions::default());
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::MAX),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(10))),
        CountingTimestampSource::new(Timestamp::from_micros(20)),
    );

    assert_eq!(
        allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::VersionAllocatorOverflow {
            last_allocated: CommitVersion::MAX,
        })
    );
    assert_eq!(allocator.source().calls(), 0);
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(10))
    );
}

#[test]
fn commit_fact_allocator_read_only_path_does_not_touch_source_or_clocks() {
    let branch = branch_id(45);
    let read_only = CommitBatch::read_only_diagnostic(
        branch,
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Always,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(99)),
            CommitOrigin::Diagnostic,
        ),
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("read-only batch");
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(7)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(8))),
        FailingTimestampSource,
    );

    let allocation = allocator
        .allocate_for_batch(&read_only)
        .expect("read-only allocation");

    assert_eq!(
        allocation,
        CommitFactAllocation::ReadOnly { branch_id: branch }
    );
    assert_eq!(allocation.stamp(), None);
    assert_eq!(allocation.timestamp_source(), None);
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(7)
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(8))
    );
}

#[test]
fn commit_fact_allocator_recovery_catch_up_helpers_update_local_floors() {
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(10)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(10))),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );

    allocator.catch_up_to_recovered_version(CommitVersion::new(12));
    allocator.catch_up_to_recovered_timestamp(Timestamp::from_micros(12));
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(12)
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(12))
    );

    allocator.catch_up_to_recovered_version(CommitVersion::new(11));
    allocator.catch_up_to_recovered_timestamp(Timestamp::from_micros(11));
    assert_eq!(
        allocator.version_allocator().last_allocated(),
        CommitVersion::new(12)
    );
    assert_eq!(
        allocator.timestamp_guard().last_allocated(),
        Some(Timestamp::from_micros(12))
    );

    *allocator.source_mut() = CommitManualTimestampSource::new(Timestamp::from_micros(20));
    assert_eq!(
        allocator.source().next_configured_timestamp(),
        Timestamp::from_micros(20)
    );
}

#[test]
fn commit_fact_allocator_allows_epoch_as_commit_timestamp() {
    let branch = branch_id(46);
    let batch = mutating_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::EPOCH),
            CommitOrigin::StorageRuntime,
        ),
    );
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(10)),
    );

    let stamp = allocator
        .allocate_for_batch(&batch)
        .expect("epoch commit timestamp")
        .stamp()
        .expect("mutating stamp");

    assert_eq!(stamp.commit_timestamp(), Timestamp::EPOCH);
}

fn mutating_batch(branch: BranchId, options: CommitBatchOptions) -> ValidatedCommitBatch {
    CommitBatch::mutating(
        branch,
        vec![
            CommitMutation::put(
                physical_key(branch, 0x20, b"put".to_vec()),
                b"value".to_vec(),
                CommitExpiry::None,
                CommitRetentionHint::Append,
            ),
            CommitMutation::delete(physical_key(branch, 0x21, b"delete".to_vec())),
        ],
        CommitValidationFacts::empty(),
        options,
    )
    .validate(&CommitRuntimeConfig::default())
    .expect("valid mutating batch")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailingTimestampSource;

impl CommitTimestampSource for FailingTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        Err(CommitRuntimeError::timestamp_unavailable("manual failure"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChainedFailingTimestampSource;

impl CommitTimestampSource for ChainedFailingTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        Err(CommitRuntimeError::timestamp_unavailable_with(
            "manual failure",
            TimestampSourceCause,
        ))
    }
}

#[derive(Debug)]
struct TimestampSourceCause;

impl fmt::Display for TimestampSourceCause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("timestamp source cause")
    }
}

impl Error for TimestampSourceCause {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CountingTimestampSource {
    timestamp: Timestamp,
    calls: usize,
}

impl CountingTimestampSource {
    const fn new(timestamp: Timestamp) -> Self {
        Self {
            timestamp,
            calls: 0,
        }
    }

    const fn calls(self) -> usize {
        self.calls
    }
}

impl CommitTimestampSource for CountingTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        self.calls += 1;
        Ok(self.timestamp)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SequenceTimestampSource {
    timestamps: Vec<Timestamp>,
    next_index: usize,
}

impl SequenceTimestampSource {
    fn new(timestamps: Vec<Timestamp>) -> Self {
        Self {
            timestamps,
            next_index: 0,
        }
    }

    const fn reads(&self) -> usize {
        self.next_index
    }
}

impl CommitTimestampSource for SequenceTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        let Some(timestamp) = self.timestamps.get(self.next_index).copied() else {
            return Err(CommitRuntimeError::timestamp_unavailable(
                "sequence source exhausted",
            ));
        };
        self.next_index += 1;
        Ok(timestamp)
    }
}
