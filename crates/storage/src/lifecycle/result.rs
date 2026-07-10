//! Lifecycle result alias.

use super::LifecycleError;

pub(crate) type LifecycleResult<T> = Result<T, LifecycleError>;
