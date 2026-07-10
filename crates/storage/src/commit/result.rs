//! Commit-runtime result alias.

use super::error::CommitRuntimeError;

pub(crate) type CommitRuntimeResult<T> = Result<T, CommitRuntimeError>;
