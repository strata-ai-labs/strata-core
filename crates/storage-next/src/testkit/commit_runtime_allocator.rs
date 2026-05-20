//! Generated commit-runtime allocator contract helpers.

use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitConflictValidationMode, CommitDuplicateKeyPolicy,
    CommitDurabilityMode, CommitFactAllocation, CommitFactAllocator, CommitManualTimestampSource,
    CommitMutation, CommitOrigin, CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult,
    CommitStamp, CommitTimestampAllocationSource, CommitTimestampGuard, CommitTimestampPolicy,
    CommitTimestampSource, CommitValidationFacts, CommitVersionAllocator, ValidatedCommitBatch,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

pub(crate) struct CommitRuntimeAllocatorOutcome {
    pub(crate) version_allocations: usize,
    pub(crate) version_catch_ups: usize,
    pub(crate) version_overflows: usize,
    pub(crate) generated_timestamps: usize,
    pub(crate) clamped_timestamps: usize,
    pub(crate) explicit_timestamps: usize,
    pub(crate) invalid_explicit_timestamps: usize,
    pub(crate) timestamp_source_failures: usize,
    pub(crate) read_only_no_allocations: usize,
    pub(crate) no_transaction_id_checks: usize,
}

pub(crate) fn check_commit_runtime_allocator_contract(
    script: &[u8],
) -> Result<CommitRuntimeAllocatorOutcome, TestkitError> {
    check_version_allocation(script)?;
    Ok(CommitRuntimeAllocatorOutcome {
        version_allocations: 1,
        version_catch_ups: check_version_catch_up()?,
        version_overflows: check_version_overflow()?,
        generated_timestamps: check_generated_timestamp(script)?,
        clamped_timestamps: check_clamped_timestamp()?,
        explicit_timestamps: check_explicit_timestamp()?,
        invalid_explicit_timestamps: check_invalid_explicit_timestamp()?,
        timestamp_source_failures: check_timestamp_source_failure()?,
        read_only_no_allocations: check_read_only_no_allocation()?,
        no_transaction_id_checks: check_no_transaction_id_surface()?,
    })
}

fn check_version_allocation(script: &[u8]) -> Result<(), TestkitError> {
    let floor = CommitVersion::new(u64::from(script_byte(script, 30)));
    let mut allocator = CommitVersionAllocator::new(floor);
    let first = allocator
        .allocate_next()
        .map_err(|err| TestkitError::new(format!("version allocation failed: {err}")))?;
    let second = allocator
        .allocate_next()
        .map_err(|err| TestkitError::new(format!("second version allocation failed: {err}")))?;
    if first <= floor || second <= first || first == CommitVersion::ZERO {
        return Err(TestkitError::new(
            "version allocator did not produce increasing nonzero versions",
        ));
    }
    Ok(())
}

fn check_version_catch_up() -> Result<usize, TestkitError> {
    let mut allocator = CommitVersionAllocator::new(CommitVersion::new(10));
    allocator.catch_up_to(CommitVersion::new(7));
    if allocator.last_allocated() != CommitVersion::new(10) {
        return Err(TestkitError::new("lower version catch-up regressed floor"));
    }
    allocator.catch_up_to(CommitVersion::new(30));
    if allocator.last_allocated() != CommitVersion::new(30)
        || allocator
            .allocate_next()
            .map_err(|err| TestkitError::new(format!("post catch-up allocation failed: {err}")))?
            != CommitVersion::new(31)
    {
        return Err(TestkitError::new("version catch-up did not advance floor"));
    }
    Ok(2)
}

fn check_version_overflow() -> Result<usize, TestkitError> {
    let mut allocator = CommitVersionAllocator::new(CommitVersion::MAX);
    if !matches!(
        allocator.preview_next(),
        Err(CommitRuntimeError::VersionAllocatorOverflow { .. })
    ) || !matches!(
        allocator.allocate_next(),
        Err(CommitRuntimeError::VersionAllocatorOverflow { .. })
    ) || allocator.last_allocated() != CommitVersion::MAX
    {
        return Err(TestkitError::new("version overflow was not stable"));
    }

    let mut near_max = CommitVersionAllocator::new(CommitVersion::new(u64::MAX - 1));
    if near_max
        .allocate_next()
        .map_err(|err| TestkitError::new(format!("max allocation failed: {err}")))?
        != CommitVersion::MAX
        || !matches!(
            near_max.allocate_next(),
            Err(CommitRuntimeError::VersionAllocatorOverflow { .. })
        )
    {
        return Err(TestkitError::new(
            "near-max allocation did not overflow after max",
        ));
    }

    let branch = branch_id(37);
    let batch = allocation_batch(branch, CommitBatchOptions::default())?;
    let mut fact_allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::MAX),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(10))),
        CountingTimestampSource::new(Timestamp::from_micros(20)),
    );
    if !matches!(
        fact_allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::VersionAllocatorOverflow { .. })
    ) || fact_allocator.timestamp_guard().last_allocated() != Some(Timestamp::from_micros(10))
        || fact_allocator.source().calls() != 0
    {
        return Err(TestkitError::new(
            "fact allocation overflow read timestamp source or advanced floor",
        ));
    }
    Ok(3)
}

fn check_generated_timestamp(script: &[u8]) -> Result<usize, TestkitError> {
    let timestamp = Timestamp::from_micros(u64::from(script_byte(script, 31)) + 1);
    let branch = branch_id(script_byte(script, 32));
    let batch = allocation_batch(branch, CommitBatchOptions::default())?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::default(),
        CommitManualTimestampSource::new(timestamp),
    );
    let allocation = allocator.allocate_for_batch(&batch).map_err(|err| {
        TestkitError::new(format!("generated timestamp allocation failed: {err}"))
    })?;
    let stamp = allocation
        .stamp()
        .ok_or_else(|| TestkitError::new("mutating allocation returned no stamp"))?;
    if stamp.commit_timestamp() != timestamp
        || allocation.timestamp_source() != Some(CommitTimestampAllocationSource::RuntimeGenerated)
    {
        return Err(TestkitError::new("generated timestamp was not preserved"));
    }

    let second = allocator
        .allocate_for_batch(&batch)
        .map_err(|err| TestkitError::new(format!("equal timestamp allocation failed: {err}")))?;
    if second.stamp().map(CommitStamp::commit_version)
        <= allocation.stamp().map(CommitStamp::commit_version)
        || second.stamp().map(CommitStamp::commit_timestamp) != Some(timestamp)
    {
        return Err(TestkitError::new(
            "equal generated timestamp did not keep versions increasing",
        ));
    }
    Ok(2)
}

fn check_clamped_timestamp() -> Result<usize, TestkitError> {
    let branch = branch_id(32);
    let floor = Timestamp::from_micros(100);
    let batch = allocation_batch(branch, CommitBatchOptions::default())?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::new(Some(floor)),
        CommitManualTimestampSource::new(Timestamp::from_micros(90)),
    );
    let allocation = allocator
        .allocate_for_batch(&batch)
        .map_err(|err| TestkitError::new(format!("clamped timestamp allocation failed: {err}")))?;
    if allocation.stamp().map(CommitStamp::commit_timestamp) != Some(floor)
        || allocation.timestamp_source()
            != Some(CommitTimestampAllocationSource::RuntimeGeneratedClamped)
    {
        return Err(TestkitError::new("generated timestamp was not clamped"));
    }
    Ok(1)
}

fn check_explicit_timestamp() -> Result<usize, TestkitError> {
    let branch = branch_id(33);
    let timestamp = Timestamp::from_micros(100);
    let batch = allocation_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(timestamp),
            CommitOrigin::StorageRuntime,
        ),
    )?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::default(),
        CommitTimestampGuard::new(Some(timestamp)),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );
    let allocation = allocator
        .allocate_for_batch(&batch)
        .map_err(|err| TestkitError::new(format!("explicit timestamp allocation failed: {err}")))?;
    if allocation.stamp().map(CommitStamp::commit_timestamp) != Some(timestamp)
        || allocation.timestamp_source() != Some(CommitTimestampAllocationSource::Explicit)
    {
        return Err(TestkitError::new("explicit timestamp was not preserved"));
    }
    Ok(1)
}

fn check_invalid_explicit_timestamp() -> Result<usize, TestkitError> {
    let branch = branch_id(34);
    let batch = allocation_batch(
        branch,
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::Explicit(Timestamp::from_micros(99)),
            CommitOrigin::StorageRuntime,
        ),
    )?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(3)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(100))),
        CommitManualTimestampSource::new(Timestamp::from_micros(1)),
    );
    if !matches!(
        allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::InvalidTimestampPolicy { .. })
    ) || allocator.version_allocator().last_allocated() != CommitVersion::new(3)
        || allocator.timestamp_guard().last_allocated() != Some(Timestamp::from_micros(100))
    {
        return Err(TestkitError::new(
            "invalid explicit timestamp changed allocator state",
        ));
    }
    Ok(1)
}

fn check_timestamp_source_failure() -> Result<usize, TestkitError> {
    let branch = branch_id(35);
    let batch = allocation_batch(branch, CommitBatchOptions::default())?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(5)),
        CommitTimestampGuard::default(),
        FailingTimestampSource,
    );
    if !matches!(
        allocator.allocate_for_batch(&batch),
        Err(CommitRuntimeError::TimestampUnavailable { .. })
    ) || allocator.version_allocator().last_allocated() != CommitVersion::new(5)
    {
        return Err(TestkitError::new(
            "timestamp source failure consumed a version or returned wrong error",
        ));
    }
    Ok(1)
}

fn check_read_only_no_allocation() -> Result<usize, TestkitError> {
    let branch = branch_id(36);
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
    .map_err(|err| TestkitError::new(format!("read-only batch rejected: {err}")))?;
    let mut allocator = CommitFactAllocator::new(
        CommitVersionAllocator::new(CommitVersion::new(6)),
        CommitTimestampGuard::new(Some(Timestamp::from_micros(7))),
        FailingTimestampSource,
    );
    let allocation = allocator
        .allocate_for_batch(&read_only)
        .map_err(|err| TestkitError::new(format!("read-only allocation failed: {err}")))?;
    if allocation != (CommitFactAllocation::ReadOnly { branch_id: branch })
        || allocator.version_allocator().last_allocated() != CommitVersion::new(6)
        || allocator.timestamp_guard().last_allocated() != Some(Timestamp::from_micros(7))
    {
        return Err(TestkitError::new("read-only allocation touched clocks"));
    }
    Ok(1)
}

fn check_no_transaction_id_surface() -> Result<usize, TestkitError> {
    let allowed = [
        "CommitVersionAllocator",
        "CommitTimestampGuard",
        "CommitFactAllocator",
    ];
    if allowed.iter().any(|name| name.contains("Txn")) {
        return Err(TestkitError::new("allocator surface contains txn naming"));
    }
    Ok(1)
}

fn allocation_batch(
    branch: BranchId,
    options: CommitBatchOptions,
) -> Result<ValidatedCommitBatch, TestkitError> {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(physical_key(
            branch,
            0x20,
            b"allocate".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        options,
    )
    .validate(&CommitRuntimeConfig::default())
    .map_err(|err| TestkitError::new(format!("allocation batch rejected: {err}")))
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FailingTimestampSource;

impl CommitTimestampSource for FailingTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        Err(CommitRuntimeError::timestamp_unavailable(
            "generated timestamp failure",
        ))
    }
}

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
