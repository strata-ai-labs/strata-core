//! Executor error boundary.

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Deserializer, Serialize};
use strata_engine::{
    CommitOutcomeStatus, EngineError, EngineErrorStatus, ErrorClass, ErrorDetail, RetryPolicy,
};

use crate::error_registry::{
    public_error_code_entry, unregistered_code_entry, ERROR_REGISTRY_DOC_PAGE,
};

const DEFAULT_DOCS_BASE_URL: &str = "https://stratadb.org";

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

    fn docs_url_for(&self, docs_slug: &str) -> String {
        // Stable short per-code slug: https://stratadb.org/e/<code> — the code
        // is a path segment, not a fragment, so every code is its own page.
        format!(
            "{}/{ERROR_REGISTRY_DOC_PAGE}/{docs_slug}",
            self.docs_base_url.trim_end_matches('/')
        )
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct ErrorStatus {
    class: ErrorClass,
    code: String,
    retry_policy: RetryPolicy,
    retryable: bool,
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
        normalize_explicit_status(
            class,
            code,
            retry_policy,
            commit_outcome,
            message.into(),
            suggested_fix.into(),
            None,
            reference_id.into(),
            trace_id,
            details,
            hints,
        )
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
        normalize_explicit_status(
            class,
            code.into(),
            retry_policy,
            commit_outcome,
            message.into(),
            suggested_fix.into(),
            Some(docs_url.into()),
            reference_id.into(),
            trace_id,
            details,
            hints,
        )
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

    /// Returns whether the retry policy permits a retry.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        self.retryable
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

#[derive(Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
struct RawErrorStatus {
    class: ErrorClass,
    code: String,
    retry_policy: RetryPolicy,
    #[serde(default, rename = "retryable")]
    _retryable: Option<bool>,
    commit_outcome: CommitOutcomeStatus,
    message: String,
    suggested_fix: String,
    docs_url: String,
    reference_id: String,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default)]
    details: Vec<ErrorDetail>,
    #[serde(default)]
    hints: Vec<String>,
}

impl<'de> Deserialize<'de> for ErrorStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawErrorStatus::deserialize(deserializer)?;
        Ok(Self {
            class: raw.class,
            code: raw.code,
            retry_policy: raw.retry_policy,
            retryable: retry_policy_allows_retry(raw.retry_policy),
            commit_outcome: raw.commit_outcome,
            message: raw.message,
            suggested_fix: raw.suggested_fix,
            docs_url: raw.docs_url,
            reference_id: raw.reference_id,
            trace_id: raw.trace_id,
            details: raw.details,
            hints: raw.hints,
        })
    }
}

/// Executor result alias.
pub type ExecutorResult<T> = Result<T, ExecutorError>;

/// Public executor error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
    pub fn from_status(status: ErrorStatus) -> Self {
        Self {
            status: normalize_status(status),
        }
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
        self.status.retryable()
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
        // The public status is intentionally pure (no source wording), so the
        // engine source chain would be lost at the boundary. Emit one structured
        // log line correlating the reference id shown to users with the code and
        // the full underlying cause (ERR-2). Capturing is the consumer's choice.
        let source_chain = source_chain_display(&value);
        let error = Self::from_status(engine_error_status(value.status()));
        if let Some(chain) = source_chain {
            tracing::error!(
                reference_id = error.reference_id(),
                code = error.code(),
                source = chain.as_str(),
                "engine error crossed the executor boundary"
            );
        }
        error
    }
}

/// Renders an error's full `Error::source()` chain into one line, or `None` when
/// the error has no source.
fn source_chain_display(error: &dyn Error) -> Option<String> {
    let mut source = error.source()?;
    let mut chain = source.to_string();
    while let Some(next) = source.source() {
        chain.push_str("; caused by: ");
        chain.push_str(&next.to_string());
        source = next;
    }
    Some(chain)
}

#[cfg(feature = "inference")]
impl From<strata_inference::InferenceError> for ExecutorError {
    fn from(value: strata_inference::InferenceError) -> Self {
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

fn default_reference_id_source() -> Arc<dyn ErrorReferenceIdSource> {
    static SOURCE: OnceLock<Arc<SequentialErrorReferenceIdSource>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            Arc::new(SequentialErrorReferenceIdSource::new(
                process_reference_prefix(),
            ))
        })
        .clone()
}

/// A per-process reference-id prefix so ids do not collide across process runs
/// (the sequential counter alone restarts at 1 each run, so `err_local_000001`
/// would recur every process). Still begins with `err_local_`.
fn process_reference_prefix() -> String {
    let token = crate::time_compat::SystemTime::now()
        .duration_since(crate::time_compat::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    format!("err_local_{token:08x}_")
}

fn current_error_render_config() -> ErrorRenderConfig {
    ERROR_RENDER_CONFIG.with(|current| current.borrow().clone())
}

#[allow(clippy::too_many_arguments)]
fn normalize_explicit_status(
    class: ErrorClass,
    requested_code: String,
    retry_policy: RetryPolicy,
    commit_outcome: CommitOutcomeStatus,
    message: String,
    suggested_fix: String,
    docs_url: Option<String>,
    reference_id: String,
    trace_id: Option<String>,
    mut details: Vec<ErrorDetail>,
    hints: Vec<String>,
) -> ErrorStatus {
    let registry_entry = public_error_code_entry(&requested_code);
    let entry = registry_entry.unwrap_or_else(unregistered_code_entry);
    let code = if registry_entry.is_some() {
        requested_code
    } else {
        details.push(ErrorDetail::new("unregistered_code", requested_code));
        entry.code.to_owned()
    };
    let suggested_fix = if suggested_fix.trim().is_empty()
        || suggested_fix == default_suggested_fix(class)
        || registry_entry.is_none()
    {
        entry.suggested_fix.to_owned()
    } else {
        suggested_fix
    };
    let retry_policy = override_retry_policy(retry_policy, entry.retry_policy);
    ErrorStatus {
        class: entry.class,
        code,
        retry_policy,
        retryable: retry_policy_allows_retry(retry_policy),
        commit_outcome: override_commit_outcome(commit_outcome, entry.commit_outcome),
        message,
        suggested_fix,
        docs_url: normalize_docs_url(docs_url, entry.docs_slug),
        reference_id,
        trace_id,
        details,
        hints,
    }
}

fn normalize_status(status: ErrorStatus) -> ErrorStatus {
    normalize_explicit_status(
        status.class,
        status.code,
        status.retry_policy,
        status.commit_outcome,
        status.message,
        status.suggested_fix,
        Some(status.docs_url),
        status.reference_id,
        status.trace_id,
        status.details,
        status.hints,
    )
}

fn normalize_docs_url(docs_url: Option<String>, docs_slug: &str) -> String {
    if let Some(url) = docs_url {
        // Ignore any legacy fragment; the canonical form is a path slug.
        let base = url
            .split_once('#')
            .map_or(url.as_str(), |(base, _anchor)| base);
        let base = base.trim_end_matches('/');
        let mut segments = base.rsplit('/');
        let last = segments.next();
        let second_last = segments.next();
        // Already the canonical per-code page: …/e/<code>.
        if last == Some(docs_slug) && second_last == Some(ERROR_REGISTRY_DOC_PAGE) {
            return base.to_owned();
        }
        // A docs base ending at the error segment: …/e — append the code.
        if last == Some(ERROR_REGISTRY_DOC_PAGE) {
            return format!("{base}/{docs_slug}");
        }
    }
    current_error_render_config().docs_url_for(docs_slug)
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
    let requested_code = code.into();
    let message = message.into();
    let supplied_fix = suggested_fix.into();
    let registry_entry = public_error_code_entry(&requested_code);
    let entry = registry_entry.unwrap_or_else(unregistered_code_entry);
    let mut details = details;
    let code = if registry_entry.is_some() {
        requested_code
    } else {
        details.push(ErrorDetail::new("unregistered_code", requested_code));
        entry.code.to_owned()
    };
    let suggested_fix = if supplied_fix.trim().is_empty()
        || supplied_fix == default_suggested_fix(class)
        || registry_entry.is_none()
    {
        entry.suggested_fix.to_owned()
    } else {
        supplied_fix
    };
    ErrorStatus::new_with_docs_url(
        entry.class,
        code.clone(),
        override_retry_policy(retry_policy, entry.retry_policy),
        override_commit_outcome(commit_outcome, entry.commit_outcome),
        message,
        suggested_fix,
        config.docs_url_for(entry.docs_slug),
        config.next_reference_id(),
        trace_id,
        details,
        hints,
    )
}

const fn override_retry_policy(
    supplied: RetryPolicy,
    registry_default: RetryPolicy,
) -> RetryPolicy {
    match supplied {
        RetryPolicy::Never => registry_default,
        _ => supplied,
    }
}

const fn retry_policy_allows_retry(policy: RetryPolicy) -> bool {
    matches!(
        policy,
        RetryPolicy::AfterStateChange | RetryPolicy::SameRequest | RetryPolicy::IdempotentOnly
    )
}

const fn override_commit_outcome(
    supplied: CommitOutcomeStatus,
    registry_default: CommitOutcomeStatus,
) -> CommitOutcomeStatus {
    match supplied {
        CommitOutcomeStatus::NotApplicable | CommitOutcomeStatus::NotStarted => registry_default,
        _ => supplied,
    }
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
        "failed_precondition.engine.runtime_closed" => {
            return ExecutorErrorClass::ClosedHandle;
        }
        "failed_precondition.engine.space_not_empty" => return ExecutorErrorClass::Conflict,
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
fn inference_public_class(code: &str, legacy: strata_inference::InferenceErrorClass) -> ErrorClass {
    // The registry is the single source of truth for a code's public class, so
    // a registered inference code drives it directly rather than restating the
    // mapping here (which risked drifting from the registry). Only codes not yet
    // in the registry fall back to the inference crate's legacy class.
    if let Some(entry) = public_error_code_entry(code) {
        return entry.class;
    }
    match legacy {
        strata_inference::InferenceErrorClass::InvalidInput => ErrorClass::InvalidArgument,
        strata_inference::InferenceErrorClass::NotFound => ErrorClass::NotFound,
        strata_inference::InferenceErrorClass::Unavailable
        | strata_inference::InferenceErrorClass::Retryable => ErrorClass::Unavailable,
        strata_inference::InferenceErrorClass::Corruption => ErrorClass::Corruption,
        strata_inference::InferenceErrorClass::Internal => ErrorClass::Internal,
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

#[cfg(test)]
mod tests {
    use super::{process_reference_prefix, source_chain_display};
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    struct Layer {
        message: &'static str,
        cause: Option<Box<Layer>>,
    }

    impl fmt::Display for Layer {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.message)
        }
    }

    impl Error for Layer {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.cause.as_deref().map(|cause| cause as &dyn Error)
        }
    }

    #[test]
    fn source_chain_display_walks_every_cause() {
        let error = Layer {
            message: "top",
            cause: Some(Box::new(Layer {
                message: "middle",
                cause: Some(Box::new(Layer {
                    message: "root",
                    cause: None,
                })),
            })),
        };
        assert_eq!(
            source_chain_display(&error).as_deref(),
            Some("middle; caused by: root")
        );
    }

    #[test]
    fn source_chain_display_is_none_without_a_source() {
        let leaf = Layer {
            message: "only",
            cause: None,
        };
        assert!(source_chain_display(&leaf).is_none());
    }

    #[test]
    fn process_reference_prefix_is_namespaced_and_process_unique() {
        let prefix = process_reference_prefix();
        assert!(prefix.starts_with("err_local_"));
        // Carries a per-process token beyond the bare namespace, so ids do not
        // collide across runs.
        assert!(prefix.len() > "err_local_".len() + 1);
    }
}
