//! Shared identity and ordering atoms for Strata.
//!
//! This crate intentionally keeps a narrow public surface. It owns only the
//! identity and ordering atoms that must be shared below engine policy.

#![deny(unsafe_code)]

mod branch;
mod time;
mod version;

pub use branch::{BranchId, BranchIdError};
pub use time::{ParseTimestampError, Timestamp};
pub use version::{CommitVersion, ParseCommitVersionError};
