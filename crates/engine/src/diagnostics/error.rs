//! Stable executor-facing engine errors.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// V1 public error class.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// A requested object does not exist.
    NotFound,
    /// A create operation targeted an object that already exists.
    AlreadyExists,
    /// Caller supplied malformed input or an invalid option.
    InvalidArgument,
    /// Current database state or mode does not allow the operation.
    FailedPrecondition,
    /// Caller or backend lacks permission for the operation.
    AccessDenied,
    /// Request conflicts with current state.
    Conflict,
    /// The system cannot prove whether a write committed.
    AmbiguousCommit,
    /// Requested history is outside the retained window.
    HistoryUnavailable,
    /// Feature, backend, mode, format, provider, or capability is unsupported.
    Unsupported,
    /// A capacity, quota, memory, disk, or configured limit was exceeded.
    ResourceExhausted,
    /// Required service, backend, provider, lock, endpoint, or model is unavailable.
    Unavailable,
    /// Storage or filesystem IO failed without stronger classification.
    Io,
    /// Durable state or provider output violates integrity expectations.
    Corruption,
    /// Encoding, decoding, schema, format, or protocol conversion failed.
    Serialization,
    /// Strata hit an invariant failure.
    Internal,
}

/// V1 retry policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum RetryPolicy {
    /// Retrying the same request without changing input or state should not help.
    Never,
    /// Retry may work after configuration, branch, backend, model, or permission changes.
    AfterStateChange,
    /// Retrying the exact same request is safe and may succeed.
    SameRequest,
    /// Retry is safe only for proven-idempotent operations.
    IdempotentOnly,
    /// Strata cannot safely classify retryability.
    Unknown,
}

impl RetryPolicy {
    const fn retryable(self) -> bool {
        matches!(
            self,
            Self::AfterStateChange | Self::SameRequest | Self::IdempotentOnly
        )
    }
}

/// V1 commit outcome status.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub enum CommitOutcomeStatus {
    /// The operation did not attempt a commit.
    NotApplicable,
    /// Validation failed before commit machinery began.
    NotStarted,
    /// Commit machinery began, but no commit became visible or durable.
    DefinitelyNotCommitted,
    /// Strata cannot prove whether the commit became visible or durable.
    MaybeCommitted,
    /// The commit succeeded, but a post-commit action failed.
    CommittedPostCommitFailed,
}

/// Redacted structured error detail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "wire-schemas", derive(schemars::JsonSchema))]
pub struct ErrorDetail {
    key: String,
    value: String,
}

impl ErrorDetail {
    /// Creates a redacted structured detail.
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Returns the detail key.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the redacted detail value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Engine-owned status facts before executor boundary rendering.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EngineErrorStatus {
    class: ErrorClass,
    code: String,
    retry_policy: RetryPolicy,
    commit_outcome: CommitOutcomeStatus,
    message: String,
    suggested_fix: String,
    details: Vec<ErrorDetail>,
    hints: Vec<String>,
}

impl EngineErrorStatus {
    /// Creates engine status facts.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        class: ErrorClass,
        code: impl Into<String>,
        retry_policy: RetryPolicy,
        commit_outcome: CommitOutcomeStatus,
        message: impl Into<String>,
        suggested_fix: impl Into<String>,
        details: Vec<ErrorDetail>,
        hints: Vec<String>,
    ) -> Self {
        Self {
            class,
            code: code.into(),
            retry_policy,
            commit_outcome,
            message: message.into(),
            suggested_fix: suggested_fix.into(),
            details,
            hints,
        }
    }

    /// Returns the public class.
    #[must_use]
    pub const fn class(&self) -> ErrorClass {
        self.class
    }

    /// Returns the stable code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    /// Returns the commit outcome.
    #[must_use]
    pub const fn commit_outcome(&self) -> CommitOutcomeStatus {
        self.commit_outcome
    }

    /// Returns the message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the suggested fix.
    #[must_use]
    pub fn suggested_fix(&self) -> &str {
        &self.suggested_fix
    }

    /// Returns structured details.
    #[must_use]
    pub fn details(&self) -> &[ErrorDetail] {
        &self.details
    }

    /// Returns user-facing hints.
    #[must_use]
    pub fn hints(&self) -> &[String] {
        &self.hints
    }
}

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
    status: EngineErrorStatus,
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
        let retry_policy = if retryable {
            RetryPolicy::SameRequest
        } else {
            default_retry_policy(class)
        };
        let message = message.into();
        Self {
            class,
            status: EngineErrorStatus::new(
                public_class_for_legacy(class, code),
                code,
                retry_policy,
                default_commit_outcome(class),
                message,
                default_suggested_fix(class),
                Vec::new(),
                Vec::new(),
            ),
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
        let retry_policy = if retryable {
            RetryPolicy::SameRequest
        } else {
            default_retry_policy(class)
        };
        let message = message.into();
        Self {
            class,
            status: EngineErrorStatus::new(
                public_class_for_legacy(class, code),
                code,
                retry_policy,
                default_commit_outcome(class),
                message,
                default_suggested_fix(class),
                Vec::new(),
                Vec::new(),
            ),
            source: Some(Arc::new(source)),
        }
    }

    /// Reconstructs an engine error from a preserved V1 status.
    ///
    /// Used when a captured per-item status (e.g. a failed batch entry) must be
    /// re-raised as a top-level error without substituting a coarser code. The
    /// legacy class is recovered from the registered code so `class()` stays
    /// consistent with the status.
    pub(crate) fn from_status(status: EngineErrorStatus) -> Self {
        let legacy_class = super::registry::class_for_code(status.code())
            .unwrap_or(EngineErrorClass::InvalidInput);
        Self {
            class: legacy_class,
            status,
            source: None,
        }
    }

    /// Creates an engine error from explicit V1 status facts.
    pub(crate) fn with_status(
        legacy_class: EngineErrorClass,
        status: EngineErrorStatus,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        debug_assert_registered(legacy_class, status.code());
        Self {
            class: legacy_class,
            status,
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
    /// Creates an unsupported-operation error.
    ///
    /// The legacy class is `Unavailable` (the coarse pre-V1 vocabulary has no
    /// dedicated unsupported variant); the public V1 class is derived from the
    /// `unsupported.` code prefix and resolves to `ErrorClass::Unsupported`.
    pub(crate) fn unsupported(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(EngineErrorClass::Unavailable, code, false, message)
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
    pub fn code(&self) -> &str {
        self.status.code()
    }

    #[must_use]
    /// Returns whether this error has a retry-permitting policy.
    ///
    /// Prefer [`Self::retry_policy`] when deciding whether the caller can retry
    /// the same request or must first change state, configuration, or input.
    pub const fn retryable(&self) -> bool {
        self.status.retry_policy().retryable()
    }

    #[must_use]
    /// Returns the executor-facing message.
    pub fn message(&self) -> &str {
        self.status.message()
    }

    /// Returns the V1 public class.
    #[must_use]
    pub const fn public_class(&self) -> ErrorClass {
        self.status.class()
    }

    /// Returns the V1 retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> RetryPolicy {
        self.status.retry_policy()
    }

    /// Returns the V1 commit outcome.
    #[must_use]
    pub const fn commit_outcome(&self) -> CommitOutcomeStatus {
        self.status.commit_outcome()
    }

    /// Returns the suggested fix.
    #[must_use]
    pub fn suggested_fix(&self) -> &str {
        self.status.suggested_fix()
    }

    /// Returns structured details.
    #[must_use]
    pub fn details(&self) -> &[ErrorDetail] {
        self.status.details()
    }

    /// Returns user-facing hints.
    #[must_use]
    pub fn hints(&self) -> &[String] {
        self.status.hints()
    }

    /// Returns engine status facts.
    #[must_use]
    pub const fn status(&self) -> &EngineErrorStatus {
        &self.status
    }

    #[must_use]
    /// Returns the retained source error.
    pub fn source_arc(&self) -> Option<&Arc<dyn Error + Send + Sync + 'static>> {
        self.source.as_ref()
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.message())
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
fn debug_assert_registered(class: EngineErrorClass, code: &str) {
    debug_assert!(
        super::registry::class_for_code(code) == Some(class),
        "engine error code `{code}` is unregistered or mapped to a class other than {class:?}"
    );
}

fn public_class_for_legacy(class: EngineErrorClass, code: &str) -> ErrorClass {
    match code.split('.').next() {
        Some("not_found") => ErrorClass::NotFound,
        Some("already_exists") => ErrorClass::AlreadyExists,
        Some("invalid_argument") => ErrorClass::InvalidArgument,
        Some("failed_precondition") => ErrorClass::FailedPrecondition,
        Some("access_denied") => ErrorClass::AccessDenied,
        Some("conflict") => ErrorClass::Conflict,
        Some("ambiguous_commit") => ErrorClass::AmbiguousCommit,
        Some("history_unavailable") => ErrorClass::HistoryUnavailable,
        Some("unsupported") => ErrorClass::Unsupported,
        Some("resource_exhausted") => ErrorClass::ResourceExhausted,
        Some("unavailable") => ErrorClass::Unavailable,
        Some("io") => ErrorClass::Io,
        Some("corruption" | "data_loss") => ErrorClass::Corruption,
        Some("serialization") => ErrorClass::Serialization,
        Some("internal") => ErrorClass::Internal,
        _ => match class {
            EngineErrorClass::InvalidInput => ErrorClass::InvalidArgument,
            EngineErrorClass::NotFound => ErrorClass::NotFound,
            EngineErrorClass::Conflict => ErrorClass::Conflict,
            EngineErrorClass::Unavailable => ErrorClass::Unavailable,
            EngineErrorClass::AmbiguousCommit => ErrorClass::AmbiguousCommit,
            EngineErrorClass::IncompatibleLayout | EngineErrorClass::ClosedRuntime => {
                ErrorClass::FailedPrecondition
            }
            EngineErrorClass::Corruption => ErrorClass::Corruption,
            EngineErrorClass::Internal => ErrorClass::Internal,
        },
    }
}

const fn default_retry_policy(class: EngineErrorClass) -> RetryPolicy {
    match class {
        EngineErrorClass::Unavailable => RetryPolicy::AfterStateChange,
        EngineErrorClass::AmbiguousCommit | EngineErrorClass::Internal => RetryPolicy::Unknown,
        _ => RetryPolicy::Never,
    }
}

const fn default_commit_outcome(class: EngineErrorClass) -> CommitOutcomeStatus {
    match class {
        EngineErrorClass::InvalidInput
        | EngineErrorClass::Conflict
        | EngineErrorClass::ClosedRuntime => CommitOutcomeStatus::NotStarted,
        EngineErrorClass::AmbiguousCommit => CommitOutcomeStatus::MaybeCommitted,
        _ => CommitOutcomeStatus::NotApplicable,
    }
}

const fn default_suggested_fix(class: EngineErrorClass) -> &'static str {
    match class {
        EngineErrorClass::InvalidInput => {
            "Correct the request input and retry the operation."
        }
        EngineErrorClass::NotFound => {
            "Check that the requested branch, space, collection, graph, document, key, or model exists."
        }
        EngineErrorClass::Conflict => {
            "Reload the current state and retry the operation against the latest version."
        }
        EngineErrorClass::Unavailable => {
            "Wait for the required database state or backend capability to become available, then retry."
        }
        EngineErrorClass::AmbiguousCommit => {
            "Re-open or inspect the database state before assuming whether the write committed."
        }
        EngineErrorClass::IncompatibleLayout => {
            "Open the database with a compatible Strata version or run the required migration."
        }
        EngineErrorClass::Corruption => {
            "Stop writing to the database and inspect recovery diagnostics before continuing."
        }
        EngineErrorClass::ClosedRuntime => {
            "Open a new database handle before issuing more commands."
        }
        EngineErrorClass::Internal => {
            "Capture the reference id and report this as a Strata bug."
        }
    }
}
