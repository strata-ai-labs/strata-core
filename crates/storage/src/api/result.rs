//! API result alias.

use super::StorageApiError;

pub type StorageApiResult<T> = Result<T, StorageApiError>;
