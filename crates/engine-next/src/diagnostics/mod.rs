//! Engine diagnostics and error vocabulary.

mod error;
mod registry;

pub use error::{
    CommitOutcomeStatus, EngineError, EngineErrorClass, EngineErrorStatus, EngineResult,
    ErrorClass, ErrorDetail, RetryPolicy,
};
