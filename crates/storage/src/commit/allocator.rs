//! Commit version and timestamp allocation.

use super::{
    CommitBatchKind, CommitRuntimeError, CommitRuntimeResult, CommitStamp, CommitTimestampPolicy,
    ValidatedCommitBatch,
};
use strata_core::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitVersionAllocator {
    last_allocated: CommitVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimestampGuard {
    last_allocated: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitManualTimestampSource {
    next_timestamp: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitFactAllocator<S> {
    versions: CommitVersionAllocator,
    timestamps: CommitTimestampGuard,
    source: S,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitFactAllocation {
    Mutating {
        stamp: CommitStamp,
        timestamp_source: CommitTimestampAllocationSource,
    },
    ReadOnly {
        branch_id: BranchId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTimestampAllocationSource {
    RuntimeGenerated,
    RuntimeGeneratedClamped,
    Explicit,
}

pub(crate) trait CommitTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp>;
}

impl CommitVersionAllocator {
    pub(crate) const fn new(last_allocated: CommitVersion) -> Self {
        Self { last_allocated }
    }

    pub(crate) const fn last_allocated(self) -> CommitVersion {
        self.last_allocated
    }

    pub(crate) fn allocate_next(&mut self) -> CommitRuntimeResult<CommitVersion> {
        let next = self.preview_next()?;
        self.last_allocated = next;
        Ok(next)
    }

    pub(crate) fn preview_next(self) -> CommitRuntimeResult<CommitVersion> {
        self.last_allocated
            .checked_next()
            .ok_or(CommitRuntimeError::VersionAllocatorOverflow {
                last_allocated: self.last_allocated,
            })
    }

    pub(crate) fn catch_up_to(&mut self, recovered: CommitVersion) {
        if recovered > self.last_allocated {
            self.last_allocated = recovered;
        }
    }
}

impl Default for CommitVersionAllocator {
    fn default() -> Self {
        Self::new(CommitVersion::ZERO)
    }
}

impl CommitTimestampGuard {
    pub(crate) const fn new(last_allocated: Option<Timestamp>) -> Self {
        Self { last_allocated }
    }

    pub(crate) const fn last_allocated(&self) -> Option<Timestamp> {
        self.last_allocated
    }

    pub(crate) fn guard_generated(
        &mut self,
        candidate: Timestamp,
    ) -> (Timestamp, CommitTimestampAllocationSource) {
        let guarded = self.preview_generated(candidate);
        self.record_allocated(guarded.0);
        guarded
    }

    pub(crate) fn guard_explicit(
        &mut self,
        timestamp: Timestamp,
    ) -> CommitRuntimeResult<Timestamp> {
        let timestamp = self.preview_explicit(timestamp)?;
        self.record_allocated(timestamp);
        Ok(timestamp)
    }

    fn preview_generated(
        &self,
        candidate: Timestamp,
    ) -> (Timestamp, CommitTimestampAllocationSource) {
        match self.last_allocated {
            Some(last) if candidate < last => (
                last,
                CommitTimestampAllocationSource::RuntimeGeneratedClamped,
            ),
            Some(_) | None => (candidate, CommitTimestampAllocationSource::RuntimeGenerated),
        }
    }

    fn preview_explicit(&self, timestamp: Timestamp) -> CommitRuntimeResult<Timestamp> {
        if self
            .last_allocated
            .is_some_and(|last_allocated| timestamp < last_allocated)
        {
            return Err(CommitRuntimeError::InvalidTimestampPolicy {
                reason: "explicit commit timestamp is before the monotonic floor",
            });
        }
        Ok(timestamp)
    }

    fn record_allocated(&mut self, timestamp: Timestamp) {
        self.last_allocated = Some(timestamp);
    }

    pub(crate) fn catch_up_to(&mut self, recovered: Timestamp) {
        if self
            .last_allocated
            .is_none_or(|last_allocated| recovered > last_allocated)
        {
            self.last_allocated = Some(recovered);
        }
    }
}

impl Default for CommitTimestampGuard {
    fn default() -> Self {
        Self::new(None)
    }
}

impl CommitManualTimestampSource {
    pub(crate) const fn new(next_timestamp: Timestamp) -> Self {
        Self { next_timestamp }
    }

    pub(crate) const fn next_configured_timestamp(self) -> Timestamp {
        self.next_timestamp
    }

    pub(crate) fn set_next_timestamp(&mut self, next_timestamp: Timestamp) {
        self.next_timestamp = next_timestamp;
    }
}

impl CommitTimestampSource for CommitManualTimestampSource {
    fn next_timestamp(&mut self) -> CommitRuntimeResult<Timestamp> {
        Ok(self.next_timestamp)
    }
}

impl<S> CommitFactAllocator<S> {
    pub(crate) const fn new(
        versions: CommitVersionAllocator,
        timestamps: CommitTimestampGuard,
        source: S,
    ) -> Self {
        Self {
            versions,
            timestamps,
            source,
        }
    }

    pub(crate) const fn version_allocator(&self) -> &CommitVersionAllocator {
        &self.versions
    }

    pub(crate) const fn timestamp_guard(&self) -> &CommitTimestampGuard {
        &self.timestamps
    }

    pub(crate) const fn source(&self) -> &S {
        &self.source
    }

    pub(crate) fn source_mut(&mut self) -> &mut S {
        &mut self.source
    }

    pub(crate) fn catch_up_to_recovered_version(&mut self, recovered: CommitVersion) {
        self.versions.catch_up_to(recovered);
    }

    pub(crate) fn catch_up_to_recovered_timestamp(&mut self, recovered: Timestamp) {
        self.timestamps.catch_up_to(recovered);
    }
}

impl<S: CommitTimestampSource> CommitFactAllocator<S> {
    pub(crate) fn allocate_for_batch(
        &mut self,
        batch: &ValidatedCommitBatch,
    ) -> CommitRuntimeResult<CommitFactAllocation> {
        match batch.batch().kind() {
            CommitBatchKind::ReadOnlyDiagnostic => Ok(CommitFactAllocation::ReadOnly {
                branch_id: batch.batch().branch_id(),
            }),
            CommitBatchKind::Mutating => {
                self.versions.preview_next()?;
                let (timestamp, timestamp_source) =
                    self.resolve_timestamp(batch.batch().options().timestamp_policy())?;
                let version = self.versions.allocate_next()?;
                let stamp = CommitStamp::new(batch.batch().branch_id(), version, timestamp)?;
                self.timestamps.record_allocated(timestamp);
                Ok(CommitFactAllocation::Mutating {
                    stamp,
                    timestamp_source,
                })
            }
        }
    }

    fn resolve_timestamp(
        &mut self,
        policy: CommitTimestampPolicy,
    ) -> CommitRuntimeResult<(Timestamp, CommitTimestampAllocationSource)> {
        match policy {
            CommitTimestampPolicy::RuntimeGenerated => {
                let candidate = self.source.next_timestamp()?;
                Ok(self.timestamps.preview_generated(candidate))
            }
            // A pre-lock runtime-generated candidate: clamp to the monotonic floor exactly like
            // a fresh generated read — a concurrent commit advancing the floor in between is
            // ordinary interleaving, never a rejection.
            CommitTimestampPolicy::RuntimeGeneratedBase(candidate) => {
                Ok(self.timestamps.preview_generated(candidate))
            }
            CommitTimestampPolicy::Explicit(timestamp) => {
                let timestamp = self.timestamps.preview_explicit(timestamp)?;
                Ok((timestamp, CommitTimestampAllocationSource::Explicit))
            }
        }
    }
}

impl CommitFactAllocation {
    pub(crate) const fn stamp(self) -> Option<CommitStamp> {
        match self {
            Self::Mutating { stamp, .. } => Some(stamp),
            Self::ReadOnly { .. } => None,
        }
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        match self {
            Self::Mutating { stamp, .. } => stamp.branch_id(),
            Self::ReadOnly { branch_id } => branch_id,
        }
    }

    pub(crate) const fn timestamp_source(self) -> Option<CommitTimestampAllocationSource> {
        match self {
            Self::Mutating {
                timestamp_source, ..
            } => Some(timestamp_source),
            Self::ReadOnly { .. } => None,
        }
    }
}
