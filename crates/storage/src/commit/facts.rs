//! Commit-runtime fact shells.

use super::{CommitRuntimeError, CommitRuntimeResult};
use strata_core::CommitVersion;

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
pub(crate) struct CommitAdmissionPressureThresholds {
    under_pressure_mutations: Option<usize>,
    under_pressure_bytes: Option<usize>,
    maintenance_mutations: Option<usize>,
    maintenance_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitAdmissionPressureFacts {
    mutations: usize,
    puts: usize,
    deletes: usize,
    approximate_commit_bytes: usize,
    under_pressure: bool,
    would_require_maintenance: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitVisibilityFacts {
    allocated: Option<CommitVersion>,
    durable: Option<CommitVersion>,
    applied: Option<CommitVersion>,
    visible: Option<CommitVersion>,
    timeline: Option<CommitVersion>,
}

impl CommitAdmissionPressureThresholds {
    pub(crate) const fn disabled() -> Self {
        Self {
            under_pressure_mutations: None,
            under_pressure_bytes: None,
            maintenance_mutations: None,
            maintenance_bytes: None,
        }
    }

    pub(crate) fn new(
        under_pressure_mutations: Option<usize>,
        under_pressure_bytes: Option<usize>,
        maintenance_mutations: Option<usize>,
        maintenance_bytes: Option<usize>,
    ) -> CommitRuntimeResult<Self> {
        let thresholds = Self {
            under_pressure_mutations,
            under_pressure_bytes,
            maintenance_mutations,
            maintenance_bytes,
        };
        thresholds.validate()?;
        Ok(thresholds)
    }

    pub(crate) const fn under_pressure_mutations(self) -> Option<usize> {
        self.under_pressure_mutations
    }

    pub(crate) const fn under_pressure_bytes(self) -> Option<usize> {
        self.under_pressure_bytes
    }

    pub(crate) const fn maintenance_mutations(self) -> Option<usize> {
        self.maintenance_mutations
    }

    pub(crate) const fn maintenance_bytes(self) -> Option<usize> {
        self.maintenance_bytes
    }

    pub(crate) fn under_pressure(self, mutations: usize, approximate_commit_bytes: usize) -> bool {
        threshold_reached(self.under_pressure_mutations, mutations)
            || threshold_reached(self.under_pressure_bytes, approximate_commit_bytes)
    }

    pub(crate) fn would_require_maintenance(
        self,
        mutations: usize,
        approximate_commit_bytes: usize,
    ) -> bool {
        threshold_reached(self.maintenance_mutations, mutations)
            || threshold_reached(self.maintenance_bytes, approximate_commit_bytes)
    }

    pub(crate) fn validate(self) -> CommitRuntimeResult<()> {
        require_nonzero_threshold(self.under_pressure_mutations, "under_pressure_mutations")?;
        require_nonzero_threshold(self.under_pressure_bytes, "under_pressure_bytes")?;
        require_nonzero_threshold(self.maintenance_mutations, "maintenance_mutations")?;
        require_nonzero_threshold(self.maintenance_bytes, "maintenance_bytes")?;
        Ok(())
    }
}

impl CommitAdmissionPressureFacts {
    pub(crate) fn new(
        mutations: usize,
        puts: usize,
        deletes: usize,
        approximate_commit_bytes: usize,
        thresholds: CommitAdmissionPressureThresholds,
    ) -> CommitRuntimeResult<Self> {
        let expected_mutations =
            puts.checked_add(deletes)
                .ok_or(CommitRuntimeError::InvalidCommitState {
                    reason: "commit admission mutation count overflow",
                })?;
        if mutations != expected_mutations {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "commit admission mutation counts do not add up",
            });
        }
        Ok(Self {
            mutations,
            puts,
            deletes,
            approximate_commit_bytes,
            under_pressure: thresholds.under_pressure(mutations, approximate_commit_bytes),
            would_require_maintenance: thresholds
                .would_require_maintenance(mutations, approximate_commit_bytes),
        })
    }

    pub(crate) const fn mutations(self) -> usize {
        self.mutations
    }

    pub(crate) const fn puts(self) -> usize {
        self.puts
    }

    pub(crate) const fn deletes(self) -> usize {
        self.deletes
    }

    pub(crate) const fn approximate_commit_bytes(self) -> usize {
        self.approximate_commit_bytes
    }

    pub(crate) const fn under_pressure(self) -> bool {
        self.under_pressure
    }

    pub(crate) const fn would_require_maintenance(self) -> bool {
        self.would_require_maintenance
    }
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

    #[cfg(test)]
    pub(crate) const fn from_parts_unchecked(
        allocated_version: Option<CommitVersion>,
        durable_version: Option<CommitVersion>,
        applied_version: Option<CommitVersion>,
        visible_version: Option<CommitVersion>,
        timeline_version: Option<CommitVersion>,
    ) -> Self {
        Self {
            allocated: allocated_version,
            durable: durable_version,
            applied: applied_version,
            visible: visible_version,
            timeline: timeline_version,
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

fn threshold_reached(threshold: Option<usize>, value: usize) -> bool {
    threshold.is_some_and(|threshold| value >= threshold)
}

fn require_nonzero_threshold(
    threshold: Option<usize>,
    field: &'static str,
) -> CommitRuntimeResult<()> {
    if threshold == Some(0) {
        return Err(CommitRuntimeError::InvalidConfig {
            field,
            reason: "must be nonzero when configured",
        });
    }
    Ok(())
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
