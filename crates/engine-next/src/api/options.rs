//! Explicit database open options.

use crate::branch::BranchName;
use crate::diagnostics::EngineResult;

/// Options for explicit cache database open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheOpenOptions {
    default_branch: Option<BranchName>,
}

#[allow(clippy::new_without_default)]
impl CacheOpenOptions {
    /// Creates cache open options.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            default_branch: None,
        }
    }

    /// Selects the default branch for a newly-created database.
    pub fn with_default_branch(mut self, name: impl Into<String>) -> EngineResult<Self> {
        self.default_branch = Some(BranchName::new(name)?);
        Ok(self)
    }

    pub(crate) fn into_default_branch(self) -> Option<BranchName> {
        self.default_branch
    }
}

/// Options for explicit durable-local database open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DurableLocalOpenOptions {
    default_branch: Option<BranchName>,
}

#[allow(clippy::new_without_default)]
impl DurableLocalOpenOptions {
    /// Creates durable-local open options.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            default_branch: None,
        }
    }

    /// Selects the default branch for a newly-created database.
    pub fn with_default_branch(mut self, name: impl Into<String>) -> EngineResult<Self> {
        self.default_branch = Some(BranchName::new(name)?);
        Ok(self)
    }

    pub(crate) fn into_default_branch(self) -> Option<BranchName> {
        self.default_branch
    }
}
