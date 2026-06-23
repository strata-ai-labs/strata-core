//! Executor error boundary.

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use strata_engine_next::{
    CommitOutcomeStatus, EngineError, EngineErrorStatus, ErrorClass, ErrorDetail, RetryPolicy,
};

const DEFAULT_DOCS_BASE_URL: &str = "https://strata.dev/docs/errors";

/// Source of user-visible error reference ids.
pub trait ErrorReferenceIdSource: Send + Sync {
    /// Returns the next reference id to attach to a rendered public error.
    fn next_reference_id(&self) -> String;
}

/// Sequential local reference id source used by the embedded default boundary.
#[derive(Debug)]
pub struct SequentialErrorReferenceIdSource {
    prefix: String,
    counter: AtomicU64,
}

impl SequentialErrorReferenceIdSource {
    /// Creates a sequential source using `prefix` and one-based numeric ids.
    #[must_use]
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
            counter: AtomicU64::new(1),
        }
    }
}

impl ErrorReferenceIdSource for SequentialErrorReferenceIdSource {
    fn next_reference_id(&self) -> String {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{}{id:06}", self.prefix)
    }
}

/// Boundary-specific public error rendering configuration.
#[derive(Clone)]
pub struct ErrorRenderConfig {
    docs_base_url: String,
    reference_id_source: Arc<dyn ErrorReferenceIdSource>,
}

impl fmt::Debug for ErrorRenderConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ErrorRenderConfig")
            .field("docs_base_url", &self.docs_base_url)
            .finish_non_exhaustive()
    }
}

impl ErrorRenderConfig {
    /// Creates a renderer with an explicit docs base URL and reference id source.
    #[must_use]
    pub fn new(
        docs_base_url: impl Into<String>,
        reference_id_source: Arc<dyn ErrorReferenceIdSource>,
    ) -> Self {
        Self {
            docs_base_url: docs_base_url.into(),
            reference_id_source,
        }
    }

    fn docs_url_for(&self, code: &str) -> String {
        format!("{}/{code}", self.docs_base_url.trim_end_matches('/'))
    }

    fn next_reference_id(&self) -> String {
        self.reference_id_source.next_reference_id()
    }
}

impl Default for ErrorRenderConfig {
    fn default() -> Self {
        Self::new(DEFAULT_DOCS_BASE_URL, default_reference_id_source())
    }
}

/// Runs `operation` with a boundary-specific error renderer.
pub fn with_error_render_config<T>(config: ErrorRenderConfig, operation: impl FnOnce() -> T) -> T {
    let previous = ERROR_RENDER_CONFIG.with(|current| current.replace(config));
    let _reset = ErrorRenderConfigReset {
        previous: Some(previous),
    };
    operation()
}

struct ErrorRenderConfigReset {
    previous: Option<ErrorRenderConfig>,
}

impl Drop for ErrorRenderConfigReset {
    fn drop(&mut self) {
        let previous = self
            .previous
            .take()
            .expect("render config reset should have previous config");
        ERROR_RENDER_CONFIG.with(|current| {
            current.replace(previous);
        });
    }
}

thread_local! {
    static ERROR_RENDER_CONFIG: std::cell::RefCell<ErrorRenderConfig> =
        std::cell::RefCell::new(ErrorRenderConfig::default());
}

/// Stable executor compatibility error class.
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

/// Public V1 executor error status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorStatus {
    class: ErrorClass,
    code: String,
    retry_policy: RetryPolicy,
    commit_outcome: CommitOutcomeStatus,
    message: String,
    suggested_fix: String,
    docs_url: String,
    reference_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    details: Vec<ErrorDetail>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    hints: Vec<String>,
}

impl ErrorStatus {
    /// Creates a public error status.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        class: ErrorClass,
        code: impl Into<String>,
        retry_policy: RetryPolicy,
        commit_outcome: CommitOutcomeStatus,
        message: impl Into<String>,
        suggested_fix: impl Into<String>,
        reference_id: impl Into<String>,
        trace_id: Option<String>,
        details: Vec<ErrorDetail>,
        hints: Vec<String>,
    ) -> Self {
        let code = code.into();
        Self {
            class,
            docs_url: docs_url_for(&code),
            code,
            retry_policy,
            commit_outcome,
            message: message.into(),
            suggested_fix: suggested_fix.into(),
            reference_id: reference_id.into(),
            trace_id,
            details,
            hints,
        }
    }

    /// Creates a public error status with a boundary-rendered docs URL.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new_with_docs_url(
        class: ErrorClass,
        code: impl Into<String>,
        retry_policy: RetryPolicy,
        commit_outcome: CommitOutcomeStatus,
        message: impl Into<String>,
        suggested_fix: impl Into<String>,
        docs_url: impl Into<String>,
        reference_id: impl Into<String>,
        trace_id: Option<String>,
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
            docs_url: docs_url.into(),
            reference_id: reference_id.into(),
            trace_id,
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

    /// Returns the public message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the suggested fix.
    #[must_use]
    pub fn suggested_fix(&self) -> &str {
        &self.suggested_fix
    }

    /// Returns the docs URL.
    #[must_use]
    pub fn docs_url(&self) -> &str {
        &self.docs_url
    }

    /// Returns the reference id.
    #[must_use]
    pub fn reference_id(&self) -> &str {
        &self.reference_id
    }

    /// Returns the optional trace id.
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
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

/// Executor result alias.
pub type ExecutorResult<T> = Result<T, ExecutorError>;

/// Public executor error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExecutorError {
    status: ErrorStatus,
}

impl ExecutorError {
    /// Creates an executor error.
    pub fn new(
        class: ExecutorErrorClass,
        code: impl Into<String>,
        retryable: bool,
        message: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let public_class = public_class_for_executor(class, &code);
        let retry_policy = if retryable {
            RetryPolicy::SameRequest
        } else {
            default_retry_policy(public_class)
        };
        Self::from_status(render_status(
            public_class,
            code,
            retry_policy,
            default_commit_outcome(public_class),
            message,
            default_suggested_fix(public_class),
            None,
            Vec::new(),
            Vec::new(),
        ))
    }

    /// Creates an invalid-input error.
    pub fn invalid_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ExecutorErrorClass::InvalidInput, code, false, message)
    }

    /// Creates a not-found error.
    pub fn not_found(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(ExecutorErrorClass::NotFound, code, false, message)
    }

    /// Creates an executor error from an existing public status.
    #[must_use]
    pub const fn from_status(status: ErrorStatus) -> Self {
        Self { status }
    }

    /// Returns the public status.
    #[must_use]
    pub const fn status(&self) -> &ErrorStatus {
        &self.status
    }

    /// Consumes this error and returns the public status.
    pub(crate) fn into_status(self) -> ErrorStatus {
        self.status
    }

    /// Returns the compatibility class.
    #[must_use]
    pub fn class(&self) -> ExecutorErrorClass {
        executor_class_for_status(&self.status)
    }

    /// Returns the public class.
    #[must_use]
    pub const fn public_class(&self) -> ErrorClass {
        self.status.class()
    }

    /// Returns the stable code.
    #[must_use]
    pub fn code(&self) -> &str {
        self.status.code()
    }

    /// Returns whether this error has a retry-permitting policy.
    ///
    /// Prefer [`Self::retry_policy`] when deciding whether the caller can retry
    /// the same request or must first change state, configuration, or input.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self.status.retry_policy(),
            RetryPolicy::AfterStateChange | RetryPolicy::SameRequest | RetryPolicy::IdempotentOnly
        )
    }

    /// Returns the retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> RetryPolicy {
        self.status.retry_policy()
    }

    /// Returns the commit outcome.
    #[must_use]
    pub const fn commit_outcome(&self) -> CommitOutcomeStatus {
        self.status.commit_outcome()
    }

    /// Returns the public message.
    #[must_use]
    pub fn message(&self) -> &str {
        self.status.message()
    }

    /// Returns the suggested fix.
    #[must_use]
    pub fn suggested_fix(&self) -> &str {
        self.status.suggested_fix()
    }

    /// Returns the docs URL.
    #[must_use]
    pub fn docs_url(&self) -> &str {
        self.status.docs_url()
    }

    /// Returns the reference id.
    #[must_use]
    pub fn reference_id(&self) -> &str {
        self.status.reference_id()
    }
}

impl From<EngineError> for ExecutorError {
    fn from(value: EngineError) -> Self {
        Self::from_status(engine_error_status(value.status()))
    }
}

#[cfg(feature = "inference")]
impl From<strata_inference_next::InferenceError> for ExecutorError {
    fn from(value: strata_inference_next::InferenceError) -> Self {
        let code = value.code();
        let class = inference_public_class(code, value.class());
        let status = render_status(
            class,
            code,
            inference_retry_policy(code, value.retryable()),
            CommitOutcomeStatus::NotApplicable,
            value.public_message(),
            inference_suggested_fix(code),
            None,
            Vec::new(),
            Vec::new(),
        );
        Self::from_status(status)
    }
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.message())
    }
}

impl Error for ExecutorError {}

fn docs_url_for(code: &str) -> String {
    format!("{DEFAULT_DOCS_BASE_URL}/{code}")
}

fn default_reference_id_source() -> Arc<dyn ErrorReferenceIdSource> {
    static SOURCE: OnceLock<Arc<SequentialErrorReferenceIdSource>> = OnceLock::new();
    SOURCE
        .get_or_init(|| Arc::new(SequentialErrorReferenceIdSource::new("err_local_")))
        .clone()
}

fn current_error_render_config() -> ErrorRenderConfig {
    ERROR_RENDER_CONFIG.with(|current| current.borrow().clone())
}

#[allow(clippy::too_many_arguments)]
fn render_status(
    class: ErrorClass,
    code: impl Into<String>,
    retry_policy: RetryPolicy,
    commit_outcome: CommitOutcomeStatus,
    message: impl Into<String>,
    suggested_fix: impl Into<String>,
    trace_id: Option<String>,
    details: Vec<ErrorDetail>,
    hints: Vec<String>,
) -> ErrorStatus {
    let config = current_error_render_config();
    let code = code.into();
    ErrorStatus::new_with_docs_url(
        class,
        code.clone(),
        retry_policy,
        commit_outcome,
        message,
        suggested_fix,
        config.docs_url_for(&code),
        config.next_reference_id(),
        trace_id,
        details,
        hints,
    )
}

pub(crate) fn batch_item_error_status(message: impl Into<String>) -> ErrorStatus {
    render_status(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.batch_item",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        message,
        "Correct the batch item input and retry.",
        None,
        Vec::new(),
        Vec::new(),
    )
}

pub(crate) fn engine_error_status(status: &EngineErrorStatus) -> ErrorStatus {
    render_status(
        status.class(),
        status.code().to_owned(),
        status.retry_policy(),
        status.commit_outcome(),
        status.message().to_owned(),
        status.suggested_fix().to_owned(),
        None,
        status.details().to_vec(),
        status.hints().to_vec(),
    )
}

fn public_class_for_executor(class: ExecutorErrorClass, code: &str) -> ErrorClass {
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
            ExecutorErrorClass::InvalidInput => ErrorClass::InvalidArgument,
            ExecutorErrorClass::NotFound => ErrorClass::NotFound,
            ExecutorErrorClass::Conflict => ErrorClass::Conflict,
            ExecutorErrorClass::Unavailable => ErrorClass::Unavailable,
            ExecutorErrorClass::AmbiguousCommit => ErrorClass::AmbiguousCommit,
            ExecutorErrorClass::IncompatibleLayout | ExecutorErrorClass::ClosedHandle => {
                ErrorClass::FailedPrecondition
            }
            ExecutorErrorClass::Corruption => ErrorClass::Corruption,
            ExecutorErrorClass::Internal => ErrorClass::Internal,
        },
    }
}

fn executor_class_for_status(status: &ErrorStatus) -> ExecutorErrorClass {
    match status.code() {
        "failed_precondition.engine.runtime_closed"
        | "failed_precondition.executor.runtime_closed" => {
            return ExecutorErrorClass::ClosedHandle;
        }
        "failed_precondition.engine.space_not_empty"
        | "failed_precondition.executor.space_not_empty" => return ExecutorErrorClass::Conflict,
        _ => {}
    }
    match status.class() {
        ErrorClass::InvalidArgument | ErrorClass::Serialization => ExecutorErrorClass::InvalidInput,
        ErrorClass::NotFound | ErrorClass::HistoryUnavailable => ExecutorErrorClass::NotFound,
        ErrorClass::AlreadyExists | ErrorClass::Conflict => ExecutorErrorClass::Conflict,
        ErrorClass::Unavailable
        | ErrorClass::Unsupported
        | ErrorClass::ResourceExhausted
        | ErrorClass::Io
        | ErrorClass::AccessDenied
        | ErrorClass::FailedPrecondition => ExecutorErrorClass::Unavailable,
        ErrorClass::AmbiguousCommit => ExecutorErrorClass::AmbiguousCommit,
        ErrorClass::Corruption => ExecutorErrorClass::Corruption,
        _ => ExecutorErrorClass::Internal,
    }
}

const fn default_retry_policy(class: ErrorClass) -> RetryPolicy {
    match class {
        ErrorClass::Unavailable | ErrorClass::ResourceExhausted => RetryPolicy::AfterStateChange,
        ErrorClass::AmbiguousCommit | ErrorClass::Internal | ErrorClass::Io => RetryPolicy::Unknown,
        _ => RetryPolicy::Never,
    }
}

const fn default_commit_outcome(class: ErrorClass) -> CommitOutcomeStatus {
    match class {
        ErrorClass::InvalidArgument
        | ErrorClass::AlreadyExists
        | ErrorClass::Conflict
        | ErrorClass::FailedPrecondition => CommitOutcomeStatus::NotStarted,
        ErrorClass::AmbiguousCommit => CommitOutcomeStatus::MaybeCommitted,
        _ => CommitOutcomeStatus::NotApplicable,
    }
}

const fn default_suggested_fix(class: ErrorClass) -> &'static str {
    match class {
        ErrorClass::InvalidArgument => "Correct the command input and retry.",
        ErrorClass::NotFound => "Check that the requested object exists before retrying.",
        ErrorClass::AlreadyExists => "Use the existing object or choose a new name.",
        ErrorClass::FailedPrecondition => {
            "Change the database state or command options before retrying."
        }
        ErrorClass::AccessDenied => "Update credentials or permissions before retrying.",
        ErrorClass::Conflict => "Reload current state and retry against the latest version.",
        ErrorClass::AmbiguousCommit => {
            "Re-open or inspect the database state before assuming whether the write committed."
        }
        ErrorClass::HistoryUnavailable => "Request history inside the retained window.",
        ErrorClass::Unsupported => "Use a supported mode, backend, command, or option.",
        ErrorClass::ResourceExhausted => "Reduce resource pressure or raise the configured limit.",
        ErrorClass::Unavailable => "Retry after the required service or backend is available.",
        ErrorClass::Io => "Inspect local IO state and retry when the backend is healthy.",
        ErrorClass::Corruption => "Stop writing and inspect diagnostics before continuing.",
        ErrorClass::Serialization => "Correct the serialized payload or use a compatible format.",
        _ => "Capture the reference id and report this as a Strata bug.",
    }
}

#[cfg(feature = "inference")]
fn inference_public_class(
    code: &str,
    legacy: strata_inference_next::InferenceErrorClass,
) -> ErrorClass {
    match code {
        "inference.invalid_request" | "inference.unsupported_parameter" => {
            ErrorClass::InvalidArgument
        }
        "inference.missing_model"
        | "inference.model_load_failed"
        | "inference.missing_api_key"
        | "inference.download_disabled" => ErrorClass::FailedPrecondition,
        "inference.unsupported_provider" | "inference.unsupported_operation" => {
            ErrorClass::Unsupported
        }
        "inference.provider_auth_failed" => ErrorClass::AccessDenied,
        "inference.provider_malformed_response" => ErrorClass::Serialization,
        "inference.download_verification_failed" | "inference.registry_corrupt" => {
            ErrorClass::Corruption
        }
        "inference.io_failure" => ErrorClass::Io,
        "inference.provider_rate_limited"
        | "inference.provider_timeout"
        | "inference.provider_unavailable"
        | "inference.download_failed"
        | "inference.local_runtime_failed" => ErrorClass::Unavailable,
        _ => match legacy {
            strata_inference_next::InferenceErrorClass::InvalidInput => ErrorClass::InvalidArgument,
            strata_inference_next::InferenceErrorClass::NotFound => ErrorClass::NotFound,
            strata_inference_next::InferenceErrorClass::Unavailable
            | strata_inference_next::InferenceErrorClass::Retryable => ErrorClass::Unavailable,
            strata_inference_next::InferenceErrorClass::Corruption => ErrorClass::Corruption,
            strata_inference_next::InferenceErrorClass::Internal => ErrorClass::Internal,
        },
    }
}

#[cfg(feature = "inference")]
const fn inference_retry_policy(code: &str, retryable: bool) -> RetryPolicy {
    match code.as_bytes() {
        b"inference.provider_timeout"
        | b"inference.provider_unavailable"
        | b"inference.download_failed" => RetryPolicy::SameRequest,
        b"inference.provider_rate_limited"
        | b"inference.missing_model"
        | b"inference.model_load_failed"
        | b"inference.missing_api_key"
        | b"inference.provider_auth_failed"
        | b"inference.download_disabled"
        | b"inference.download_verification_failed" => RetryPolicy::AfterStateChange,
        b"inference.provider_malformed_response"
        | b"inference.io_failure"
        | b"inference.local_runtime_failed" => RetryPolicy::Unknown,
        _ if retryable => RetryPolicy::SameRequest,
        _ => RetryPolicy::Never,
    }
}

#[cfg(feature = "inference")]
const fn inference_suggested_fix(code: &str) -> &'static str {
    match code.as_bytes() {
        b"inference.missing_api_key" => "Set the provider API key and retry.",
        b"inference.provider_auth_failed" => "Check provider credentials and permissions.",
        b"inference.provider_rate_limited" => "Wait for provider rate limits to reset.",
        b"inference.provider_timeout" | b"inference.provider_unavailable" => {
            "Retry when the provider is available."
        }
        b"inference.missing_model" | b"inference.model_load_failed" => {
            "Install or configure the requested model before retrying."
        }
        b"inference.download_disabled" => "Enable model downloads or install the model manually.",
        b"inference.download_verification_failed" => {
            "Delete the corrupted download and download the model again."
        }
        b"inference.provider_malformed_response" => {
            "Retry or switch providers if the response remains invalid."
        }
        b"inference.io_failure" => "Inspect local filesystem access and retry.",
        _ => "Inspect inference configuration and retry with supported settings.",
    }
}
