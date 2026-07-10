//! Engine diagnostics and error vocabulary.

mod error;
mod registry;

pub use error::{
    CommitOutcomeStatus, EngineError, EngineErrorClass, EngineErrorStatus, EngineResult,
    ErrorClass, ErrorDetail, RetryPolicy,
};
pub use registry::{
    error_code_registry_entries, error_code_registry_entry, ErrorCodeRegistryEntry,
};
