use std::error::Error;
use std::fmt;

use serde::ser::Serializer;

/// Errors that can occur during inference operations.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, serde::Deserialize)]
pub enum InferenceError {
    /// Local llama.cpp runtime failure.
    LlamaCpp(String),

    /// Cloud provider failure.
    Provider(String),

    /// Model registry or model download failure.
    Registry(String),

    /// Filesystem or process IO failure.
    Io(String),

    /// Requested provider, model, or operation is unavailable.
    NotSupported(String),

    /// The model spec itself is malformed (empty, missing model name, or an
    /// unknown provider) — caller input error, not a provider outage.
    InvalidSpec(String),
}

impl fmt::Debug for InferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LlamaCpp(message) => formatter
                .debug_tuple("LlamaCpp")
                .field(&redact_secrets(message))
                .finish(),
            Self::Provider(message) => formatter
                .debug_tuple("Provider")
                .field(&redact_secrets(message))
                .finish(),
            Self::Registry(message) => formatter
                .debug_tuple("Registry")
                .field(&redact_secrets(message))
                .finish(),
            Self::Io(message) => formatter
                .debug_tuple("Io")
                .field(&redact_secrets(message))
                .finish(),
            Self::NotSupported(message) => formatter
                .debug_tuple("NotSupported")
                .field(&redact_secrets(message))
                .finish(),
            Self::InvalidSpec(message) => formatter
                .debug_tuple("InvalidSpec")
                .field(&redact_secrets(message))
                .finish(),
        }
    }
}

impl fmt::Display for InferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LlamaCpp(message) => write!(formatter, "llama.cpp: {}", redact_secrets(message)),
            Self::Provider(message) => {
                write!(formatter, "provider error: {}", redact_secrets(message))
            }
            Self::Registry(message) => {
                write!(formatter, "registry error: {}", redact_secrets(message))
            }
            Self::Io(message) => write!(formatter, "IO error: {}", redact_secrets(message)),
            Self::NotSupported(message) => {
                write!(formatter, "not supported: {}", redact_secrets(message))
            }
            Self::InvalidSpec(message) => {
                write!(formatter, "invalid model spec: {}", redact_secrets(message))
            }
        }
    }
}

impl serde::Serialize for InferenceError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::LlamaCpp(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                0,
                "LlamaCpp",
                &redact_secrets(message),
            ),
            Self::Provider(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                1,
                "Provider",
                &redact_secrets(message),
            ),
            Self::Registry(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                2,
                "Registry",
                &redact_secrets(message),
            ),
            Self::Io(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                3,
                "Io",
                &redact_secrets(message),
            ),
            Self::NotSupported(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                4,
                "NotSupported",
                &redact_secrets(message),
            ),
            Self::InvalidSpec(message) => serializer.serialize_newtype_variant(
                "InferenceError",
                5,
                "InvalidSpec",
                &redact_secrets(message),
            ),
        }
    }
}

impl Error for InferenceError {}

/// Stable inference error class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceErrorClass {
    /// Caller supplied invalid input.
    InvalidInput,
    /// Requested model/provider is not available.
    NotFound,
    /// Required provider, model, or runtime is unavailable.
    Unavailable,
    /// Operation can be retried without changing user input.
    Retryable,
    /// Stored or downloaded model data is corrupt.
    Corruption,
    /// Internal runtime failure.
    Internal,
}

impl From<std::io::Error> for InferenceError {
    fn from(e: std::io::Error) -> Self {
        InferenceError::Io(e.to_string())
    }
}

impl InferenceError {
    /// Returns the stable public error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::LlamaCpp(message) => llama_code(message),
            Self::Provider(message) => provider_code(message),
            Self::Registry(message) => registry_code(message),
            Self::Io(_) => "inference.io_failure",
            Self::NotSupported(message) => not_supported_code(message),
            Self::InvalidSpec(_) => "inference.invalid_request",
        }
    }

    /// Returns the stable public error class.
    pub fn class(&self) -> InferenceErrorClass {
        match self.code() {
            "inference.invalid_request"
            | "inference.unsupported_parameter"
            | "inference.download_disabled" => InferenceErrorClass::InvalidInput,
            "inference.missing_model" | "inference.unsupported_provider" => {
                InferenceErrorClass::NotFound
            }
            "inference.provider_rate_limited" | "inference.provider_timeout" => {
                InferenceErrorClass::Retryable
            }
            "inference.download_verification_failed"
            | "inference.registry_corrupt"
            | "inference.provider_malformed_response" => InferenceErrorClass::Corruption,
            "inference.local_runtime_failed" | "inference.io_failure" => {
                InferenceErrorClass::Internal
            }
            _ => InferenceErrorClass::Unavailable,
        }
    }

    /// Returns whether retrying without changing input may succeed.
    pub fn retryable(&self) -> bool {
        matches!(
            self.code(),
            "inference.provider_rate_limited"
                | "inference.provider_timeout"
                | "inference.provider_unavailable"
                | "inference.download_failed"
                | "inference.io_failure"
        )
    }

    /// Returns the public redacted message.
    pub fn public_message(&self) -> String {
        redact_secrets(&self.to_string())
    }
}

fn llama_code(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("load") || lower.contains("not found") || lower.contains("no such file") {
        "inference.model_load_failed"
    } else {
        "inference.local_runtime_failed"
    }
}

fn provider_code(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    let mentions_api_key =
        lower.contains("api key") || lower.contains("api_key") || lower.contains("_api_key");
    if lower.contains("invalid request") || lower.contains("bad request") || lower.contains("400") {
        "inference.invalid_request"
    } else if mentions_api_key && (lower.contains("not set") || lower.contains("empty")) {
        "inference.missing_api_key"
    } else if lower.contains("invalid api key")
        || lower.contains("auth")
        || lower.contains("401")
        || lower.contains("403")
    {
        "inference.provider_auth_failed"
    } else if lower.contains("rate limit") || lower.contains("429") {
        "inference.provider_rate_limited"
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "inference.provider_timeout"
    } else if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("unavailable")
    {
        "inference.provider_unavailable"
    } else if lower.contains("invalid json")
        || lower.contains("missing")
        || lower.contains("empty choices")
        || lower.contains("empty candidates")
        || lower.contains("malformed")
    {
        "inference.provider_malformed_response"
    } else if lower.contains("unsupported") {
        "inference.unsupported_parameter"
    } else {
        "inference.provider_unavailable"
    }
}

fn registry_code(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unknown model") || lower.contains("not found locally") {
        "inference.missing_model"
    } else if lower.contains("download disabled") || lower.contains("network access") {
        "inference.download_disabled"
    } else if lower.contains("sha-256") || lower.contains("hash mismatch") {
        "inference.download_verification_failed"
    } else if lower.contains("download") {
        "inference.download_failed"
    } else if lower.contains("corrupt") {
        "inference.registry_corrupt"
    } else {
        "inference.registry_corrupt"
    }
}

fn not_supported_code(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("provider") {
        "inference.unsupported_provider"
    } else if lower.contains("parameter") || lower.contains("knob") {
        "inference.unsupported_parameter"
    } else if lower.contains("download") {
        "inference.download_disabled"
    } else {
        "inference.unsupported_operation"
    }
}

fn redact_secrets(message: &str) -> String {
    let mut redacted = String::with_capacity(message.len());
    let mut index = 0;
    while index < message.len() {
        let rest = &message[index..];
        let lower = rest.to_ascii_lowercase();
        let secret_prefix = ["sk-ant", "sk_ant", "sk-", "aiza"]
            .iter()
            .find(|prefix| lower.starts_with(**prefix));
        if secret_prefix.is_some() {
            redacted.push_str("[REDACTED]");
            index += secret_len(rest);
            continue;
        }

        let ch = rest
            .chars()
            .next()
            .expect("index is within string boundary");
        redacted.push(ch);
        index += ch.len_utf8();
    }
    redacted
}

fn secret_len(value: &str) -> usize {
    value
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .last()
        .map_or(value.len(), |(index, ch)| index + ch.len_utf8())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Display tests: verify exact format, not just substring ---

    #[test]
    fn display_llamacpp_error() {
        let e = InferenceError::LlamaCpp("model not found".into());
        assert_eq!(e.to_string(), "llama.cpp: model not found");
    }

    #[test]
    fn display_provider_error() {
        let e = InferenceError::Provider("rate limited".into());
        assert_eq!(e.to_string(), "provider error: rate limited");
    }

    #[test]
    fn display_registry_error() {
        let e = InferenceError::Registry("unknown model".into());
        assert_eq!(e.to_string(), "registry error: unknown model");
    }

    #[test]
    fn display_io_error() {
        let e = InferenceError::Io("file not found".into());
        assert_eq!(e.to_string(), "IO error: file not found");
    }

    #[test]
    fn display_not_supported() {
        let e = InferenceError::NotSupported("tokenize on cloud".into());
        assert_eq!(e.to_string(), "not supported: tokenize on cloud");
    }

    // --- Each variant has a distinct prefix (no ambiguity) ---

    #[test]
    fn all_variants_have_distinct_prefixes() {
        let errors = [
            InferenceError::LlamaCpp("x".into()),
            InferenceError::Provider("x".into()),
            InferenceError::Registry("x".into()),
            InferenceError::Io("x".into()),
            InferenceError::NotSupported("x".into()),
        ];
        let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        // Every pair should be different despite same inner "x"
        for i in 0..messages.len() {
            for j in (i + 1)..messages.len() {
                assert_ne!(messages[i], messages[j], "variants {i} and {j} collide");
            }
        }
    }

    // --- io::Error conversion ---

    #[test]
    fn io_error_conversion_preserves_full_message() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: InferenceError = io_err.into();
        assert!(matches!(e, InferenceError::Io(_)));
        // Verify the full formatted string, not just a substring
        assert_eq!(e.to_string(), "IO error: gone");
    }

    #[test]
    fn io_error_conversion_preserves_kind_description() {
        // When io::Error has no custom message, to_string() includes the kind
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "cannot read /etc/shadow",
        );
        let e: InferenceError = io_err.into();
        assert!(
            e.to_string().contains("cannot read /etc/shadow"),
            "got: {}",
            e
        );
    }

    #[test]
    fn io_error_from_real_fs_operation() {
        // Exercise the From impl with a real filesystem error
        let result: Result<Vec<u8>, InferenceError> =
            std::fs::read("/nonexistent/path/that/does/not/exist").map_err(Into::into);
        let e = result.unwrap_err();
        assert!(matches!(e, InferenceError::Io(_)));
        assert!(!e.to_string().is_empty());
    }

    // --- Clone across all variants ---

    #[test]
    fn clone_all_variants() {
        let cases: Vec<InferenceError> = vec![
            InferenceError::LlamaCpp("llama msg".into()),
            InferenceError::Provider("provider msg".into()),
            InferenceError::Registry("registry msg".into()),
            InferenceError::Io("io msg".into()),
            InferenceError::NotSupported("not supported msg".into()),
        ];
        for original in &cases {
            let cloned = original.clone();
            assert_eq!(original.to_string(), cloned.to_string());
            // Verify the clone is structurally the same variant
            assert_eq!(
                std::mem::discriminant(original),
                std::mem::discriminant(&cloned)
            );
        }
    }

    // --- Error messages contain original context ---

    #[test]
    fn every_variant_preserves_context_string() {
        let context = "specific context string with special chars: !@#$%";
        let cases: Vec<InferenceError> = vec![
            InferenceError::LlamaCpp(context.into()),
            InferenceError::Provider(context.into()),
            InferenceError::Registry(context.into()),
            InferenceError::Io(context.into()),
            InferenceError::NotSupported(context.into()),
        ];
        for e in &cases {
            assert!(
                e.to_string().contains(context),
                "variant {:?} lost context: {}",
                std::mem::discriminant(e),
                e
            );
        }
    }

    // --- Empty string edge case ---

    #[test]
    fn empty_string_variants_still_have_prefix() {
        // Empty inner string shouldn't produce empty Display output
        let cases: Vec<(InferenceError, &str)> = vec![
            (InferenceError::LlamaCpp(String::new()), "llama.cpp: "),
            (InferenceError::Provider(String::new()), "provider error: "),
            (InferenceError::Registry(String::new()), "registry error: "),
            (InferenceError::Io(String::new()), "IO error: "),
            (
                InferenceError::NotSupported(String::new()),
                "not supported: ",
            ),
        ];
        for (e, expected) in &cases {
            assert_eq!(&e.to_string(), expected);
        }
    }

    // --- std::error::Error trait ---

    #[test]
    fn implements_std_error_trait() {
        let e = InferenceError::Provider("test".into());
        // Verify it can be used as &dyn std::error::Error
        let dyn_err: &dyn std::error::Error = &e;
        assert!(!dyn_err.to_string().is_empty());
        // source() should be None for our string-wrapping variants
        assert!(dyn_err.source().is_none());
    }

    #[test]
    fn usable_with_question_mark_operator() {
        fn fallible() -> Result<(), InferenceError> {
            let _data = std::fs::read("/nonexistent/file/for/test")?;
            Ok(())
        }
        let result = fallible();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), InferenceError::Io(_)));
    }
}
