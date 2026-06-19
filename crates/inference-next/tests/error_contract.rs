//! Stable inference error contract tests.

use serde::de::DeserializeOwned;
use serde::Serialize;
use strata_inference_next::{InferenceError, InferenceErrorClass};

fn round_trip<T>(value: &T) -> T
where
    T: Clone + PartialEq + std::fmt::Debug + Serialize + DeserializeOwned,
{
    let encoded = serde_json::to_string(value).expect("serializes");
    serde_json::from_str(&encoded).expect("deserializes")
}

#[test]
fn every_stable_error_code_has_class_and_retry_policy() {
    let cases = [
        (
            InferenceError::Provider("HTTP 400 bad request".to_owned()),
            "inference.invalid_request",
            InferenceErrorClass::InvalidInput,
            false,
        ),
        (
            InferenceError::Registry("unknown model miniLM".to_owned()),
            "inference.missing_model",
            InferenceErrorClass::NotFound,
            false,
        ),
        (
            InferenceError::LlamaCpp("model load failed".to_owned()),
            "inference.model_load_failed",
            InferenceErrorClass::Unavailable,
            false,
        ),
        (
            InferenceError::NotSupported("openai provider not enabled".to_owned()),
            "inference.unsupported_provider",
            InferenceErrorClass::NotFound,
            false,
        ),
        (
            InferenceError::NotSupported("ranking requires local feature".to_owned()),
            "inference.unsupported_operation",
            InferenceErrorClass::Unavailable,
            false,
        ),
        (
            InferenceError::NotSupported("unsupported parameter top_k".to_owned()),
            "inference.unsupported_parameter",
            InferenceErrorClass::InvalidInput,
            false,
        ),
        (
            InferenceError::Provider("OPENAI_API_KEY not set".to_owned()),
            "inference.missing_api_key",
            InferenceErrorClass::Unavailable,
            false,
        ),
        (
            InferenceError::Provider("invalid API key".to_owned()),
            "inference.provider_auth_failed",
            InferenceErrorClass::Unavailable,
            false,
        ),
        (
            InferenceError::Provider("HTTP 429 rate limit".to_owned()),
            "inference.provider_rate_limited",
            InferenceErrorClass::Retryable,
            true,
        ),
        (
            InferenceError::Provider("request timed out".to_owned()),
            "inference.provider_timeout",
            InferenceErrorClass::Retryable,
            true,
        ),
        (
            InferenceError::Provider("HTTP 503 unavailable".to_owned()),
            "inference.provider_unavailable",
            InferenceErrorClass::Unavailable,
            true,
        ),
        (
            InferenceError::Provider("invalid JSON response".to_owned()),
            "inference.provider_malformed_response",
            InferenceErrorClass::Corruption,
            false,
        ),
        (
            InferenceError::Registry("network access disabled".to_owned()),
            "inference.download_disabled",
            InferenceErrorClass::InvalidInput,
            false,
        ),
        (
            InferenceError::Registry("download failed".to_owned()),
            "inference.download_failed",
            InferenceErrorClass::Unavailable,
            true,
        ),
        (
            InferenceError::Registry("sha-256 mismatch".to_owned()),
            "inference.download_verification_failed",
            InferenceErrorClass::Corruption,
            false,
        ),
        (
            InferenceError::Registry("download sha-256 hash mismatch".to_owned()),
            "inference.download_verification_failed",
            InferenceErrorClass::Corruption,
            false,
        ),
        (
            InferenceError::LlamaCpp("decode failed".to_owned()),
            "inference.local_runtime_failed",
            InferenceErrorClass::Internal,
            false,
        ),
        (
            InferenceError::Registry("corrupt registry".to_owned()),
            "inference.registry_corrupt",
            InferenceErrorClass::Corruption,
            false,
        ),
        (
            InferenceError::Io("disk failed".to_owned()),
            "inference.io_failure",
            InferenceErrorClass::Internal,
            true,
        ),
    ];

    for (error, code, class, retryable) in cases {
        assert_eq!(error.code(), code, "{error:?}");
        assert_eq!(error.class(), class, "{error:?}");
        assert_eq!(error.retryable(), retryable, "{error:?}");
        assert_eq!(round_trip(&error), error, "{error:?}");
    }
}

#[test]
fn public_error_surfaces_redact_provider_secrets() {
    let error = InferenceError::Provider(
        "failed with key=sk-test-secret and url=/v1?key=AIzaabc123 and sk-ant-provider-token"
            .to_owned(),
    );

    let display = error.to_string();
    let debug = format!("{error:?}");
    let public_message = error.public_message();
    let serialized = serde_json::to_string(&error).expect("serializes");

    for rendered in [display, debug, public_message, serialized] {
        assert!(
            !rendered.contains("sk-test-secret"),
            "secret leaked through {rendered}"
        );
        assert!(
            !rendered.contains("AIzaabc123"),
            "secret leaked through {rendered}"
        );
        assert!(
            !rendered.contains("sk-ant-provider-token"),
            "secret leaked through {rendered}"
        );
        assert!(
            rendered.contains("[REDACTED]"),
            "redaction marker missing from {rendered}"
        );
    }
}
