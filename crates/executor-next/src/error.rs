//! Executor error boundary.

use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};
use strata_engine_next::{EngineError, EngineErrorClass};

/// Stable executor error class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorErrorClass {
    /// Caller supplied invalid input.
    InvalidInput,
    /// Requested object was not found.
    NotFound,
    /// Request conflicted with current state.
    Conflict,
    /// Required state is unavailable.
    Unavailable,
    /// Commit result could not be proven.
    AmbiguousCommit,
    /// Stored layout is incompatible.
    IncompatibleLayout,
    /// Stored data is corrupt.
    Corruption,
    /// Executor handle is closed.
    ClosedHandle,
    /// Internal failure.
    Internal,
}

/// Executor result alias.
pub type ExecutorResult<T> = Result<T, ExecutorError>;

/// Stable executor error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutorError {
    class: ExecutorErrorClass,
    code: String,
    retryable: bool,
    message: String,
}

impl ExecutorError {
    /// Creates an executor error.
    pub fn new(
        class: ExecutorErrorClass,
        code: impl Into<String>,
        retryable: bool,
        message: impl Into<String>,
    ) -> Self {
        Self {
            class,
            code: code.into(),
            retryable,
            message: message.into(),
        }
    }

    /// Creates an invalid-input error.
    pub fn invalid_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ExecutorErrorClass::InvalidInput, code, false, message)
    }

    /// Returns the stable class.
    pub const fn class(&self) -> ExecutorErrorClass {
        self.class
    }

    /// Returns the stable code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns whether retrying without input changes may succeed.
    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    /// Returns the public message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<EngineError> for ExecutorError {
    fn from(value: EngineError) -> Self {
        let class = match value.class() {
            EngineErrorClass::InvalidInput => ExecutorErrorClass::InvalidInput,
            EngineErrorClass::NotFound => ExecutorErrorClass::NotFound,
            EngineErrorClass::Conflict => ExecutorErrorClass::Conflict,
            EngineErrorClass::Unavailable => ExecutorErrorClass::Unavailable,
            EngineErrorClass::AmbiguousCommit => ExecutorErrorClass::AmbiguousCommit,
            EngineErrorClass::IncompatibleLayout => ExecutorErrorClass::IncompatibleLayout,
            EngineErrorClass::Corruption => ExecutorErrorClass::Corruption,
            EngineErrorClass::ClosedRuntime => ExecutorErrorClass::ClosedHandle,
            EngineErrorClass::Internal | _ => ExecutorErrorClass::Internal,
        };
        Self::new(
            class,
            executor_code(value.code()),
            value.retryable(),
            value.message().to_owned(),
        )
    }
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ExecutorError {}

fn executor_code(code: &str) -> String {
    code.replace(".engine.", ".executor.")
}
