//! Stable executor-facing engine errors.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Stable engine error class.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineErrorClass {
    /// Caller supplied invalid input.
    InvalidInput,
    /// Requested engine object was not found.
    NotFound,
    /// Request conflicted with current state.
    Conflict,
    /// Required persistence capability or state is unavailable.
    Unavailable,
    /// Persistence could not prove whether a commit succeeded.
    AmbiguousCommit,
    /// Stored engine layout is incompatible with this binary.
    IncompatibleLayout,
    /// Stored engine control data is corrupt.
    Corruption,
    /// Database handle is closed.
    ClosedRuntime,
    /// Internal engine failure.
    Internal,
}

/// Engine result alias.
pub type EngineResult<T> = Result<T, EngineError>;

/// Stable executor-facing engine error.
#[derive(Clone, Debug)]
pub struct EngineError {
    class: EngineErrorClass,
    code: &'static str,
    retryable: bool,
    message: String,
    source: Option<Arc<dyn Error + Send + Sync + 'static>>,
}

impl EngineError {
    pub(crate) fn new(
        class: EngineErrorClass,
        code: &'static str,
        retryable: bool,
        message: impl Into<String>,
    ) -> Self {
        debug_assert_registered(class, code);
        Self {
            class,
            code,
            retryable,
            message: message.into(),
            source: None,
        }
    }

    pub(crate) fn with_source(
        class: EngineErrorClass,
        code: &'static str,
        retryable: bool,
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        debug_assert_registered(class, code);
        Self {
            class,
            code,
            retryable,
            message: message.into(),
            source: Some(Arc::new(source)),
        }
    }

    #[must_use]
    /// Creates an invalid-input error.
    pub fn invalid_input(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::InvalidInput, code, false, message)
    }

    #[must_use]
    /// Creates a not-found error.
    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::NotFound, code, false, message)
    }

    #[must_use]
    /// Creates a conflict error.
    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::Conflict, code, false, message)
    }

    #[must_use]
    /// Creates a corruption error.
    pub fn corruption(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::Corruption, code, false, message)
    }

    #[must_use]
    /// Creates an incompatible-layout error.
    pub fn incompatible_layout(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::IncompatibleLayout, code, false, message)
    }

    #[must_use]
    /// Creates a control-plane-unavailable error.
    pub(crate) fn control_plane_unavailable(message: impl Into<String>) -> Self {
        Self::new(
            EngineErrorClass::Unavailable,
            "unavailable.engine.control_plane",
            false,
            message,
        )
    }

    #[must_use]
    /// Creates a closed-runtime error.
    pub fn closed_runtime(message: impl Into<String>) -> Self {
        Self::new(
            EngineErrorClass::ClosedRuntime,
            "failed_precondition.engine.runtime_closed",
            false,
            message,
        )
    }

    #[must_use]
    /// Returns the stable error class.
    pub const fn class(&self) -> EngineErrorClass {
        self.class
    }

    #[must_use]
    /// Returns the stable error code.
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    /// Returns whether retrying without input changes may succeed.
    pub const fn retryable(&self) -> bool {
        self.retryable
    }

    #[must_use]
    /// Returns the executor-facing message.
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    /// Returns the retained source error.
    pub fn source_arc(&self) -> Option<&Arc<dyn Error + Send + Sync + 'static>> {
        self.source.as_ref()
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

/// Validates in debug builds that `code` is registered and maps to `class`.
fn debug_assert_registered(class: EngineErrorClass, code: &'static str) {
    debug_assert!(
        super::registry::class_for_code(code) == Some(class),
        "engine error code `{code}` is unregistered or mapped to a class other than {class:?}"
    );
}
