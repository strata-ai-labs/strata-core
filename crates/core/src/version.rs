//! Commit timeline version atoms.

use serde::{Deserialize, Serialize};
use std::{fmt, num::ParseIntError, str::FromStr};

/// Monotonic commit position in a branch-visible timeline.
///
/// Storage assigns commit versions and engine exposes them for version reads,
/// history, branch operations, and diagnostics. The value is intentionally
/// opaque outside this crate except where lower layers need durable `u64`
/// encoding.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
)]
#[repr(transparent)]
#[serde(transparent)]
pub struct CommitVersion(u64);

impl CommitVersion {
    /// Smallest representable commit version.
    pub const ZERO: Self = Self(0);

    /// Largest representable commit version.
    pub const MAX: Self = Self(u64::MAX);

    /// Creates a commit version from its raw numeric representation.
    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the raw numeric representation.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Returns the next representable commit version.
    ///
    /// `None` means this value is already [`CommitVersion::MAX`].
    #[must_use]
    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

impl fmt::Display for CommitVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CommitVersion {
    type Err = ParseCommitVersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.starts_with('+') {
            return Err(ParseCommitVersionError { source: None });
        }

        let raw = text
            .parse::<u64>()
            .map_err(|source| ParseCommitVersionError {
                source: Some(source),
            })?;
        Ok(Self(raw))
    }
}

/// Error returned when text cannot form a [`CommitVersion`].
#[derive(Debug)]
#[non_exhaustive]
pub struct ParseCommitVersionError {
    source: Option<ParseIntError>,
}

impl fmt::Display for ParseCommitVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid commit version text")
    }
}

impl std::error::Error for ParseCommitVersionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::CommitVersion;
    use std::{error::Error, str::FromStr};

    #[test]
    fn explicit_constructor_preserves_raw_value() {
        let version = CommitVersion::new(42);

        assert_eq!(version.as_u64(), 42);
    }

    #[test]
    fn constants_preserve_numeric_bounds() {
        assert_eq!(CommitVersion::ZERO.as_u64(), 0);
        assert_eq!(CommitVersion::MAX.as_u64(), u64::MAX);
    }

    #[test]
    fn default_is_zero() {
        assert_eq!(CommitVersion::default(), CommitVersion::ZERO);
    }

    #[test]
    fn ordering_follows_raw_value_order() {
        assert!(CommitVersion::new(7) > CommitVersion::new(3));
        assert!(CommitVersion::ZERO < CommitVersion::MAX);
    }

    #[test]
    fn checked_next_advances_without_skipping() {
        assert_eq!(
            CommitVersion::new(41).checked_next(),
            Some(CommitVersion::new(42))
        );
    }

    #[test]
    fn checked_next_refuses_to_wrap() {
        assert_eq!(CommitVersion::MAX.checked_next(), None);
    }

    #[test]
    fn display_uses_raw_decimal_value() {
        assert_eq!(CommitVersion::new(42).to_string(), "42");
    }

    #[test]
    fn from_str_accepts_raw_decimal_value() {
        let version = CommitVersion::from_str("42").expect("valid commit version");

        assert_eq!(version, CommitVersion::new(42));
    }

    #[test]
    fn from_str_rejects_invalid_text() {
        let err = CommitVersion::from_str("v42").expect_err("decorated text should fail");

        assert_eq!(err.to_string(), "invalid commit version text");
        assert!(err.source().is_some());
    }

    #[test]
    fn from_str_rejects_leading_plus() {
        let err = CommitVersion::from_str("+42").expect_err("signed text should fail");

        assert_eq!(err.to_string(), "invalid commit version text");
        assert!(err.source().is_none());
    }

    #[test]
    fn serde_uses_transparent_numeric_value() {
        let json = serde_json::to_string(&CommitVersion::new(42)).expect("serialize version");

        assert_eq!(json, "42");
        assert_eq!(
            serde_json::from_str::<CommitVersion>(&json).expect("deserialize version"),
            CommitVersion::new(42)
        );
    }

    #[test]
    fn serde_binary_round_trips_numeric_value() {
        let bytes = bincode::serialize(&CommitVersion::new(42)).expect("serialize version");

        assert_eq!(
            bincode::deserialize::<CommitVersion>(&bytes).expect("deserialize version"),
            CommitVersion::new(42)
        );
    }

    #[test]
    fn representation_matches_u64_layout() {
        assert_eq!(
            std::mem::size_of::<CommitVersion>(),
            std::mem::size_of::<u64>()
        );
        assert_eq!(
            std::mem::align_of::<CommitVersion>(),
            std::mem::align_of::<u64>()
        );
    }
}
