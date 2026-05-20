//! Commit-runtime fact shells.

use super::{CommitRuntimeError, CommitRuntimeResult};
use strata_core_next::CommitVersion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitPhase {
    RejectedBeforeAllocation,
    AllocatedNotDurable,
    DurableNotApplied,
    AppliedNotVisible,
    Visible,
    Replay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitDurabilityClass {
    NotDurable,
    Standard,
    Always,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitVisibilityFacts {
    allocated: Option<CommitVersion>,
    durable: Option<CommitVersion>,
    applied: Option<CommitVersion>,
    visible: Option<CommitVersion>,
    timeline: Option<CommitVersion>,
}

impl CommitVisibilityFacts {
    pub(crate) fn new(
        allocated_version: Option<CommitVersion>,
        durable_version: Option<CommitVersion>,
        applied_version: Option<CommitVersion>,
        visible_version: Option<CommitVersion>,
        timeline_version: Option<CommitVersion>,
    ) -> CommitRuntimeResult<Self> {
        let facts = Self {
            allocated: allocated_version,
            durable: durable_version,
            applied: applied_version,
            visible: visible_version,
            timeline: timeline_version,
        };
        facts.validate()?;
        Ok(facts)
    }

    pub(crate) const fn empty() -> Self {
        Self {
            allocated: None,
            durable: None,
            applied: None,
            visible: None,
            timeline: None,
        }
    }

    pub(crate) const fn allocated_version(self) -> Option<CommitVersion> {
        self.allocated
    }

    pub(crate) const fn durable_version(self) -> Option<CommitVersion> {
        self.durable
    }

    pub(crate) const fn applied_version(self) -> Option<CommitVersion> {
        self.applied
    }

    pub(crate) const fn visible_version(self) -> Option<CommitVersion> {
        self.visible
    }

    pub(crate) const fn timeline_version(self) -> Option<CommitVersion> {
        self.timeline
    }

    pub(crate) fn validate(self) -> CommitRuntimeResult<()> {
        require_not_after(
            self.durable,
            self.allocated,
            "durable version must not exceed allocated version",
        )?;
        require_not_after(
            self.applied,
            self.allocated,
            "applied version must not exceed allocated version",
        )?;
        require_not_after(
            self.visible,
            self.applied,
            "visible version must not exceed applied version",
        )?;
        require_not_after(
            self.timeline,
            self.applied,
            "timeline version must not exceed applied version",
        )?;
        require_not_after(
            self.visible,
            self.timeline,
            "visible version must not exceed timeline version",
        )?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitRuntimeStats {
    committed_batches: u64,
    read_only_batches: u64,
    rejected_batches: u64,
    replayed_batches: u64,
    durable_but_not_visible: u64,
}

impl CommitRuntimeStats {
    pub(crate) const fn new(
        committed_batches: u64,
        read_only_batches: u64,
        rejected_batches: u64,
        replayed_batches: u64,
        durable_but_not_visible: u64,
    ) -> Self {
        Self {
            committed_batches,
            read_only_batches,
            rejected_batches,
            replayed_batches,
            durable_but_not_visible,
        }
    }

    pub(crate) const fn committed_batches(self) -> u64 {
        self.committed_batches
    }

    pub(crate) const fn read_only_batches(self) -> u64 {
        self.read_only_batches
    }

    pub(crate) const fn rejected_batches(self) -> u64 {
        self.rejected_batches
    }

    pub(crate) const fn replayed_batches(self) -> u64 {
        self.replayed_batches
    }

    pub(crate) const fn durable_but_not_visible(self) -> u64 {
        self.durable_but_not_visible
    }
}

fn require_not_after(
    lower: Option<CommitVersion>,
    upper: Option<CommitVersion>,
    reason: &'static str,
) -> CommitRuntimeResult<()> {
    match (lower, upper) {
        (Some(lower), Some(upper)) if lower > upper => {
            Err(CommitRuntimeError::InvalidVisibilityFacts { reason })
        }
        (Some(_), None) => Err(CommitRuntimeError::InvalidVisibilityFacts { reason }),
        _ => Ok(()),
    }
}
