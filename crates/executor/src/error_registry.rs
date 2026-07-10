//! Public error-code registry for the executor boundary.

use strata_engine::{
    error_code_registry_entries as engine_error_code_registry_entries,
    error_code_registry_entry as engine_error_code_registry_entry, CommitOutcomeStatus, ErrorClass,
    ErrorCodeRegistryEntry, RetryPolicy,
};

/// Path segment for stable per-code public error docs
/// (`https://stratadb.org/e/<code>`).
pub const ERROR_REGISTRY_DOC_PAGE: &str = "e";

const COMMON_SCHEMA: &str = "strata.error.details.common.v1";
const EXECUTOR_SCHEMA: &str = "strata.error.details.executor.v1";
const ARROW_SCHEMA: &str = "strata.error.details.arrow.v1";
const INFERENCE_SCHEMA: &str = "strata.error.details.inference.v1";

const EXECUTOR_ERROR_CODES: &[ErrorCodeRegistryEntry] = &[
    entry(
        "failed_precondition.executor.runtime_closed",
        ErrorClass::FailedPrecondition,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The executor handle is closed.",
        "Open a new executor handle before issuing more commands.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "failed_precondition.executor.space_not_empty",
        ErrorClass::FailedPrecondition,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The requested space is not empty.",
        "Delete or move data out of the space before deleting it.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "failed_precondition.executor.hub_url",
        ErrorClass::FailedPrecondition,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "No hub URL is configured for this invocation.",
        "Pass --hub <url>, set STRATA_HUB_URL, or configure hub.url in a project or global strata config.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "failed_precondition.executor.hub_clone",
        ErrorClass::FailedPrecondition,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The clone cannot proceed against this bundle or destination.",
        "Check the destination is empty and the bundle's engine requirement matches this Strata version.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "unavailable.executor.hub_transport",
        ErrorClass::Unavailable,
        RetryPolicy::SameRequest,
        CommitOutcomeStatus::NotStarted,
        "The hub could not be reached or returned an error.",
        "Check connectivity and the hub URL, then retry.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.hub_dataset",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The dataset name is not valid.",
        "Use the dataset's slug as shown on the hub.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.hub_branch",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The branch name is not valid.",
        "Use a branch name listed by the dataset's refs.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_base64",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow base64 input is invalid.",
        "Provide valid base64-encoded Arrow bytes or use a file path import.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_collection",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow collection target is invalid.",
        "Provide exactly one Arrow collection target compatible with the selected primitive.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_empty_export",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow export source is empty.",
        "Export at least one record or choose a source that contains data.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_embedding_type",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow embedding column type is invalid.",
        "Use a fixed-size list, list, or JSON array of numeric values for vector embeddings.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_feature_disabled",
        ErrorClass::InvalidArgument,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotStarted,
        "Arrow support is not enabled in this build.",
        "Run a build with the Arrow feature enabled before using Arrow import or export.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_format",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow file format is invalid.",
        "Use a supported Arrow file format such as IPC or Parquet.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_graph",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow graph request is invalid.",
        "Provide a graph name and graph-compatible Arrow columns for graph export.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_input_missing",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "Arrow input is missing.",
        "Provide Arrow input bytes or an Arrow input path.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_json_key",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow JSON key column is invalid.",
        "Include a non-null string or binary document key column for JSON import.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_key_column",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow key column is invalid.",
        "Include a non-null string or binary key column in the Arrow input.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_value_column",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow value column is invalid.",
        "Include a value column with a type supported by the selected Arrow import target.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_vector_dimension",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow vector dimension is invalid.",
        "Use embeddings whose length matches the target vector collection dimension.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.arrow_vector_key",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The Arrow vector key column is invalid.",
        "Include a non-null string or binary vector key column for vector import.",
        ARROW_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.batch_item",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "A batch item is invalid.",
        "Correct the batch item input and retry.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.kv_batch_duplicate_key",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The KV batch contains duplicate keys.",
        "Remove duplicate keys so each KV batch item targets a unique key.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.limit",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The requested limit is invalid.",
        "Use a positive limit within the documented maximum.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.vector_dimension",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The vector dimension is invalid.",
        "Use embeddings whose length matches the vector collection dimension.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "invalid_argument.executor.vector_limit",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "The vector query limit is invalid.",
        "Use a positive vector query limit within the documented maximum.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "not_found.executor.vector_collection",
        ErrorClass::NotFound,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The requested vector collection was not found.",
        "Create the vector collection or retry with an existing collection name.",
        EXECUTOR_SCHEMA,
    ),
    entry(
        "unavailable.executor.arrow_io",
        ErrorClass::Unavailable,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The Arrow input or output path is unavailable.",
        "Retry after the Arrow input or output path is readable and writable.",
        ARROW_SCHEMA,
    ),
    entry(
        "internal.executor.arrow",
        ErrorClass::Internal,
        RetryPolicy::Unknown,
        CommitOutcomeStatus::NotApplicable,
        "An internal Arrow boundary error occurred.",
        "Capture the reference id and report the Arrow boundary failure.",
        ARROW_SCHEMA,
    ),
    entry(
        "internal.executor.unregistered_code",
        ErrorClass::Internal,
        RetryPolicy::Unknown,
        CommitOutcomeStatus::NotApplicable,
        "An unregistered executor error code was rendered.",
        "Capture the reference id and report the unregistered executor error code.",
        EXECUTOR_SCHEMA,
    ),
];

const INFERENCE_ERROR_CODES: &[ErrorCodeRegistryEntry] = &[
    entry(
        "inference.download_disabled",
        ErrorClass::FailedPrecondition,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "Model downloads are disabled.",
        "Enable model downloads or install the model manually.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.download_failed",
        ErrorClass::Unavailable,
        RetryPolicy::SameRequest,
        CommitOutcomeStatus::NotApplicable,
        "The model download failed.",
        "Retry the model download when network access is available.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.download_verification_failed",
        ErrorClass::Corruption,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The downloaded model failed verification.",
        "Delete the corrupted model artifact and download it again.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.invalid_request",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The inference request is invalid.",
        "Correct the inference request options for the selected provider or model.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.io_failure",
        ErrorClass::Io,
        RetryPolicy::Unknown,
        CommitOutcomeStatus::NotApplicable,
        "The local inference runtime hit an I/O failure.",
        "Inspect local model-cache filesystem permissions and retry.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.local_runtime_failed",
        ErrorClass::Unavailable,
        RetryPolicy::Unknown,
        CommitOutcomeStatus::NotApplicable,
        "The local inference runtime failed.",
        "Check the local inference runtime and model compatibility before retrying.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.missing_api_key",
        ErrorClass::FailedPrecondition,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The provider API key is missing.",
        "Set the provider API key and retry.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.missing_model",
        ErrorClass::FailedPrecondition,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The requested model is missing.",
        "Install, pull, or configure the requested model before retrying.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.model_load_failed",
        ErrorClass::FailedPrecondition,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The model could not be loaded.",
        "Verify the local model path and format, then load a compatible model.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.provider_auth_failed",
        ErrorClass::AccessDenied,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The provider rejected authentication.",
        "Check provider credentials, permissions, and account access.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.provider_malformed_response",
        ErrorClass::Serialization,
        RetryPolicy::Unknown,
        CommitOutcomeStatus::NotApplicable,
        "The provider returned a malformed response.",
        "Retry or switch providers if the provider response remains invalid.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.provider_rate_limited",
        ErrorClass::Unavailable,
        RetryPolicy::AfterStateChange,
        CommitOutcomeStatus::NotApplicable,
        "The provider rate limit was reached.",
        "Wait for provider rate limits to reset or use a different provider key.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.provider_timeout",
        ErrorClass::Unavailable,
        RetryPolicy::SameRequest,
        CommitOutcomeStatus::NotApplicable,
        "The provider request timed out.",
        "Retry the request when the provider endpoint is responsive.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.provider_unavailable",
        ErrorClass::Unavailable,
        RetryPolicy::SameRequest,
        CommitOutcomeStatus::NotApplicable,
        "The provider is unavailable.",
        "Retry when the provider service is available.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.registry_corrupt",
        ErrorClass::Corruption,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The local inference registry is corrupt.",
        "Repair or recreate the local model registry before retrying.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.unsupported_operation",
        ErrorClass::Unsupported,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The inference operation is unsupported.",
        "Use an inference operation supported by the selected provider and model.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.unsupported_parameter",
        ErrorClass::InvalidArgument,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The inference parameter is unsupported.",
        "Remove or change parameters unsupported by the selected provider.",
        INFERENCE_SCHEMA,
    ),
    entry(
        "inference.unsupported_provider",
        ErrorClass::Unsupported,
        RetryPolicy::Never,
        CommitOutcomeStatus::NotApplicable,
        "The inference provider is unsupported.",
        "Use a provider configured in this build and runtime.",
        INFERENCE_SCHEMA,
    ),
];

const fn entry(
    code: &'static str,
    class: ErrorClass,
    retry_policy: RetryPolicy,
    commit_outcome: CommitOutcomeStatus,
    message_template: &'static str,
    suggested_fix: &'static str,
    details_schema: &'static str,
) -> ErrorCodeRegistryEntry {
    ErrorCodeRegistryEntry {
        code,
        class,
        retry_policy,
        commit_outcome,
        message_template,
        suggested_fix,
        docs_slug: code,
        details_schema,
    }
}

/// Returns the public registry entry for any code emitted by engine or
/// executor.
#[must_use]
pub fn public_error_code_entry(code: &str) -> Option<ErrorCodeRegistryEntry> {
    engine_error_code_registry_entry(code)
        .or_else(|| {
            EXECUTOR_ERROR_CODES
                .iter()
                .copied()
                .find(|entry| entry.code == code)
        })
        .or_else(|| {
            INFERENCE_ERROR_CODES
                .iter()
                .copied()
                .find(|entry| entry.code == code)
        })
}

/// Returns every public error-code registry entry visible through executor.
pub fn public_error_code_entries() -> impl Iterator<Item = ErrorCodeRegistryEntry> {
    engine_error_code_registry_entries()
        .chain(EXECUTOR_ERROR_CODES.iter().copied())
        .chain(INFERENCE_ERROR_CODES.iter().copied())
}

/// Returns the fallback entry used when public executor rendering receives an
/// unregistered code.
#[must_use]
pub fn unregistered_code_entry() -> ErrorCodeRegistryEntry {
    public_error_code_entry("internal.executor.unregistered_code")
        .expect("unregistered-code fallback is registered")
}

/// Fallback details schema for explicitly constructed public statuses.
#[must_use]
pub const fn common_details_schema() -> &'static str {
    COMMON_SCHEMA
}
