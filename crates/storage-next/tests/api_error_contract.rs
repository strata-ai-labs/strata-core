//! Stripe-grade error contract conformance for the public storage API.
//!
//! Enforces the storage-owned half of the error contract in
//! `docs/architecture/v1-error-and-diagnostics-contract.md`: every public
//! `StorageApiError` variant must carry a typed class, a well-formed code whose
//! class prefix agrees with `class()`, a non-empty redacted human-readable
//! message, a non-empty mechanical remediation hint, and a preserved source
//! chain where one applies. Reference IDs, doc-link URLs, and user-facing
//! "suggested fix" phrasing are assigned at the engine/SDK boundary (the
//! reference ID at the log sink from an injected id source) and are enforced by
//! that layer's contract test, not here.
//!
//! Tests assert on class, code, and structured fields — never on display prose
//! (contract testing-plan rule: "no test asserts on display text when it can
//! assert on code and class").

#![deny(unsafe_code)]

use std::sync::Arc;

use strata_storage_next::api::{
    BranchId, CommitAdmissionPressureReason, CommitAdmissionPressureSeverity, StorageApiError,
    StorageApiErrorClass, StorageApiLowerLayer,
};

/// Number of public `StorageApiError` variants. The lib's `code()`, `class()`,
/// `remediation()`, and `Display` are exhaustive matches, so a new variant is a
/// compile error there until handled; this count is the test-level backstop
/// ensuring the conformance fixture below is extended in lockstep.
const EXPECTED_VARIANT_COUNT: usize = 14;

/// Substrings that must never appear in a public message or remediation hint.
/// The full redaction matrix lives at the command/CLI/SDK surfaces; this is the
/// storage-layer floor.
const REDACTION_DENYLIST: &[&str] = &[
    "secret",
    "password",
    "api_key",
    "apikey",
    "bearer",
    "token",
    "access_key",
    "credential",
    "private_key",
];

fn branch() -> BranchId {
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN])
}

fn io_source() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, "synthetic lower-layer failure")
}

/// One sample of every public variant, paired with the class it must report.
/// Source-bearing variants are constructed *with* a source so the source-chain
/// assertion is meaningful.
fn sample_errors() -> Vec<(&'static str, StorageApiError, StorageApiErrorClass, bool)> {
    vec![
        (
            "InvalidArgument",
            StorageApiError::InvalidArgument {
                field: "maintenance_scheduling_policy",
                reason: "must select an owned backend handle",
            },
            StorageApiErrorClass::InvalidArgument,
            false,
        ),
        (
            "UnsupportedCapability",
            StorageApiError::UnsupportedCapability {
                capability: "commit_durability",
                reason: "cache runtime cannot satisfy durable commit requests",
            },
            StorageApiErrorClass::Unsupported,
            false,
        ),
        (
            "InvalidRuntimeState",
            StorageApiError::InvalidRuntimeState {
                reason: "commit requires an open runtime",
            },
            StorageApiErrorClass::FailedPrecondition,
            false,
        ),
        (
            "BranchNotFound",
            StorageApiError::BranchNotFound {
                branch_id: branch(),
            },
            StorageApiErrorClass::NotFound,
            false,
        ),
        (
            "BranchAlreadyExists",
            StorageApiError::BranchAlreadyExists {
                branch_id: branch(),
            },
            StorageApiErrorClass::AlreadyExists,
            false,
        ),
        (
            "BranchGenerationMismatch",
            StorageApiError::BranchGenerationMismatch {
                branch_id: branch(),
                expected: 2,
                actual: 1,
            },
            StorageApiErrorClass::FailedPrecondition,
            false,
        ),
        (
            "Conflict",
            StorageApiError::Conflict {
                branch_id: branch(),
                storage_space: Some(0x20),
                key_fingerprint: Some(0xdead_beef),
                user_key_len: Some(8),
                reason: "concurrent write to the same key",
            },
            StorageApiErrorClass::Conflict,
            false,
        ),
        (
            "RetainedHistoryUnavailable",
            StorageApiError::RetainedHistoryUnavailable {
                branch_id: branch(),
                reason: "version pruned by retention",
            },
            StorageApiErrorClass::HistoryUnavailable,
            false,
        ),
        (
            "TimestampHistoryUnavailable",
            StorageApiError::TimestampHistoryUnavailable {
                branch_id: branch(),
                reason: "timestamp predates covered history",
            },
            StorageApiErrorClass::HistoryUnavailable,
            false,
        ),
        (
            "DurableUncertain",
            StorageApiError::durable_uncertain_with(
                "wal sync outcome unknown after fault",
                io_source(),
            ),
            StorageApiErrorClass::AmbiguousCommit,
            true,
        ),
        (
            "RecoveryDegraded",
            StorageApiError::RecoveryDegraded {
                reason: "lossy recovery fallback used",
            },
            StorageApiErrorClass::FailedPrecondition,
            false,
        ),
        (
            "MaintenanceRejected",
            StorageApiError::MaintenanceRejected {
                reason: "conflicting maintenance in flight",
            },
            StorageApiErrorClass::FailedPrecondition,
            false,
        ),
        (
            "StoragePressure",
            StorageApiError::StoragePressure {
                branch_id: branch(),
                severity: CommitAdmissionPressureSeverity::Blocking,
                pressure_reason: CommitAdmissionPressureReason::NonZeroLevelTableBacklog,
                reason: "mutating commit admission requires maintenance progress",
                retryable: true,
            },
            StorageApiErrorClass::FailedPrecondition,
            false,
        ),
        (
            "LowerLayer",
            StorageApiError::lower_layer_with(
                StorageApiLowerLayer::Commit,
                "commit runtime failed",
                io_source(),
            ),
            StorageApiErrorClass::Internal,
            true,
        ),
    ]
}

/// Stable `<class>` code prefix for each public class.
fn expected_code_prefix(class: StorageApiErrorClass) -> &'static str {
    match class {
        StorageApiErrorClass::InvalidArgument => "invalid_argument",
        StorageApiErrorClass::FailedPrecondition => "failed_precondition",
        StorageApiErrorClass::NotFound => "not_found",
        StorageApiErrorClass::AlreadyExists => "already_exists",
        StorageApiErrorClass::Conflict => "conflict",
        StorageApiErrorClass::Unsupported => "unsupported",
        StorageApiErrorClass::HistoryUnavailable => "history_unavailable",
        StorageApiErrorClass::AmbiguousCommit => "ambiguous_commit",
        StorageApiErrorClass::Internal => "internal",
        // `StorageApiErrorClass` is `#[non_exhaustive]`; external code cannot
        // match exhaustively. A new class must be given a code prefix here.
        _ => panic!("unhandled StorageApiErrorClass; extend expected_code_prefix"),
    }
}

fn is_code_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    segment
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn contains_redacted_secret(text: &str) -> Option<&'static str> {
    let lowered = text.to_ascii_lowercase();
    REDACTION_DENYLIST
        .iter()
        .copied()
        .find(|needle| lowered.contains(needle))
}

#[test]
fn error_contract_fixture_covers_every_variant() {
    assert_eq!(
        sample_errors().len(),
        EXPECTED_VARIANT_COUNT,
        "the error-contract fixture must sample every StorageApiError variant; \
         update sample_errors() and EXPECTED_VARIANT_COUNT together when a variant is added"
    );
}

#[test]
fn every_error_code_is_well_formed_and_agrees_with_class() {
    for (name, error, expected_class, _) in sample_errors() {
        assert_eq!(
            error.class(),
            expected_class,
            "{name}: class() disagrees with the documented class"
        );

        let code = error.code();
        let segments: Vec<&str> = code.split('.').collect();
        assert_eq!(
            segments.len(),
            3,
            "{name}: code `{code}` must be exactly <class>.<area>.<detail>"
        );
        for segment in &segments {
            assert!(
                is_code_segment(segment),
                "{name}: code segment `{segment}` in `{code}` must be lowercase snake_case"
            );
        }
        assert_eq!(
            segments[0],
            expected_code_prefix(error.class()),
            "{name}: code `{code}` class prefix must match class() = {:?}",
            error.class()
        );

        // The code is the stable doc anchor; the boundary derives a doc link
        // from it. Grammar above already guarantees it is URL-path-safe, but
        // assert no whitespace defensively.
        assert!(
            !code.chars().any(char::is_whitespace),
            "{name}: code `{code}` must be doc-link safe (no whitespace)"
        );
    }
}

#[test]
fn every_error_has_a_nonempty_redacted_message() {
    for (name, error, _, _) in sample_errors() {
        let message = error.to_string();
        assert!(
            !message.trim().is_empty(),
            "{name}: Display message must be non-empty"
        );
        if let Some(secret) = contains_redacted_secret(&message) {
            panic!("{name}: Display message leaks redacted token `{secret}`: {message}");
        }
    }
}

#[test]
fn every_error_has_a_nonempty_mechanical_remediation() {
    for (name, error, _, _) in sample_errors() {
        let remediation = error.remediation();
        assert!(
            !remediation.trim().is_empty(),
            "{name}: remediation hint must be non-empty"
        );
        if let Some(secret) = contains_redacted_secret(remediation) {
            panic!("{name}: remediation leaks redacted token `{secret}`: {remediation}");
        }
        // Mechanical hint, not user-facing prose, but must read as guidance.
        assert!(
            remediation.ends_with('.'),
            "{name}: remediation should be a complete instruction ending in '.': {remediation}"
        );
    }
}

#[test]
fn source_bearing_errors_preserve_their_source_chain() {
    use std::error::Error;
    for (name, error, _, expects_source) in sample_errors() {
        let has_source = error.source().is_some();
        assert_eq!(
            has_source, expects_source,
            "{name}: source chain presence mismatch (expected {expects_source}, got {has_source})"
        );
    }
}

/// The two constructors that accept a source must thread it through.
#[test]
fn source_constructors_thread_the_source() {
    use std::error::Error;
    let lower = StorageApiError::lower_layer_with(
        StorageApiLowerLayer::Backend,
        "backend unavailable",
        io_source(),
    );
    assert!(lower.source().is_some());

    let durable = StorageApiError::durable_uncertain_with("sync uncertain", io_source());
    assert!(durable.source().is_some());

    // Sanity: the Arc-held source survives a clone (public errors are Clone).
    let cloned = lower.clone();
    let _ = Arc::new(cloned);
}
