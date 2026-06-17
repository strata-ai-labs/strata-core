//! Product branch-name validation.

use std::fmt;

use crate::diagnostics::{EngineError, EngineResult};

pub(crate) const DEFAULT_BRANCH: &str = "default";
pub(crate) const SYSTEM_BRANCH: &str = "_system_";
const MAX_BRANCH_NAME_BYTES: usize = u16::MAX as usize;

/// Validated product branch name.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BranchName(String);

impl BranchName {
    /// Creates a branch name after rejecting reserved internal spellings.
    pub fn new(name: impl Into<String>) -> EngineResult<Self> {
        let name = name.into();
        validate_branch_name(&name)?;
        Ok(Self(name))
    }

    pub(crate) fn default_branch() -> Self {
        Self(DEFAULT_BRANCH.to_owned())
    }

    /// Returns the branch name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for BranchName {
    type Error = EngineError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn validate_branch_name(name: &str) -> EngineResult<()> {
    if name.is_empty() {
        return Err(EngineError::invalid_input(
            "invalid_argument.engine.branch_name",
            "branch name must not be empty",
        ));
    }
    if name == SYSTEM_BRANCH || name.starts_with('_') {
        return Err(EngineError::invalid_input(
            "invalid_argument.engine.branch_name_reserved",
            "branch name is reserved for engine control data",
        ));
    }
    if name.len() > MAX_BRANCH_NAME_BYTES {
        return Err(EngineError::invalid_input(
            "invalid_argument.engine.branch_name",
            "branch name is too long",
        ));
    }
    if name.bytes().any(|byte| byte == 0 || byte == b'\n') {
        return Err(EngineError::invalid_input(
            "invalid_argument.engine.branch_name",
            "branch name contains an unsupported control byte",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::BranchName;
    use crate::diagnostics::EngineErrorClass;

    #[test]
    fn branch_name_rejects_reserved_internal_name() {
        let error = BranchName::new("_system_").expect_err("reserved branch must fail");
        assert_eq!(error.class(), EngineErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.engine.branch_name_reserved");
    }

    #[test]
    fn branch_name_rejects_values_that_cannot_be_length_encoded() {
        let error = BranchName::new("a".repeat(usize::from(u16::MAX) + 1))
            .expect_err("oversized branch name must fail");
        assert_eq!(error.class(), EngineErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.engine.branch_name");
    }
}
