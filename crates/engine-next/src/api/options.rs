//! Explicit database open options.

use crate::branch::BranchName;
use crate::diagnostics::EngineResult;

/// Options for explicit cache database open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheOpenOptions {
    default_branch: Option<BranchName>,
    memory_budget_bytes: Option<u64>,
}

#[allow(clippy::new_without_default)]
impl CacheOpenOptions {
    /// Creates cache open options.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            default_branch: None,
            memory_budget_bytes: None,
        }
    }

    /// Selects the default branch for a newly-created database.
    pub fn with_default_branch(mut self, name: impl Into<String>) -> EngineResult<Self> {
        self.default_branch = Some(BranchName::new(name)?);
        Ok(self)
    }

    /// Sets the total storage memory budget, in bytes, for the opened database.
    ///
    /// The value is validated by the storage layer at open time; values below
    /// the minimum supported budget are rejected with a storage error.
    #[must_use]
    pub const fn with_memory_budget(mut self, total_bytes: u64) -> Self {
        self.memory_budget_bytes = Some(total_bytes);
        self
    }

    pub(crate) fn into_default_branch(self) -> Option<BranchName> {
        self.default_branch
    }

    pub(crate) const fn memory_budget_bytes(&self) -> Option<u64> {
        self.memory_budget_bytes
    }
}

/// Options for explicit durable-local database open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableLocalOpenOptions {
    default_branch: Option<BranchName>,
    memory_budget_bytes: Option<u64>,
}

#[allow(clippy::new_without_default)]
impl DurableLocalOpenOptions {
    /// Creates durable-local open options.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            default_branch: None,
            memory_budget_bytes: None,
        }
    }

    /// Selects the default branch for a newly-created database.
    pub fn with_default_branch(mut self, name: impl Into<String>) -> EngineResult<Self> {
        self.default_branch = Some(BranchName::new(name)?);
        Ok(self)
    }

    /// Sets the total storage memory budget, in bytes, for the opened database.
    ///
    /// The value is validated by the storage layer at open time; values below
    /// the minimum supported budget are rejected with a storage error.
    #[must_use]
    pub const fn with_memory_budget(mut self, total_bytes: u64) -> Self {
        self.memory_budget_bytes = Some(total_bytes);
        self
    }

    pub(crate) fn into_default_branch(self) -> Option<BranchName> {
        self.default_branch
    }

    pub(crate) const fn memory_budget_bytes(&self) -> Option<u64> {
        self.memory_budget_bytes
    }
}
