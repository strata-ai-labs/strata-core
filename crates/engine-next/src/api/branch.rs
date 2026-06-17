//! Branch API DTOs.

use crate::branch::BranchName;

/// Product branch summary exposed to executor layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchSummary {
    name: BranchName,
    generation: u64,
}

impl BranchSummary {
    pub(crate) const fn new(name: BranchName, generation: u64) -> Self {
        Self { name, generation }
    }

    #[must_use]
    /// Returns the product branch name.
    pub fn name(&self) -> &BranchName {
        &self.name
    }

    #[must_use]
    /// Returns the branch generation tracked by the engine catalog.
    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Outcome returned after creating a product branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchCreateOutcome {
    branch: BranchSummary,
}

impl BranchCreateOutcome {
    pub(crate) const fn new(branch: BranchSummary) -> Self {
        Self { branch }
    }

    #[must_use]
    /// Returns the created branch summary.
    pub const fn branch(&self) -> &BranchSummary {
        &self.branch
    }
}
