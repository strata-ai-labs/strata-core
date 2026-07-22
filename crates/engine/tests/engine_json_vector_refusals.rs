//! JSON and vector validation-refusal code coverage (TCP3.5c).
//!
//! Pins the reachable JSON/vector refusal codes by their literal
//! `<class>.engine.<detail>` string (the existing suites assert by class).
//! Only the genuinely user-reachable refusals are here: investigation found
//! most of the deep-dive's "15 vector/json codes" are unreachable defensive
//! `serde_json::to_vec` encode arms, short-circuited empty-batch invariants,
//! or reopen-time layout/IO faults (#2651, TCP3.15) — recorded in the
//! workspace error-code guard's allowlist with per-code reasons.

mod common;

use serde_json::json;
use strata_engine::{JsonDocumentId, JsonIndexName, VectorMetadata};

use common::{branch, open_cache_database, space};

/// The JSON index-name validator rejects an empty name with a stable code.
#[test]
fn json_index_name_empty_is_rejected() {
    let error = JsonIndexName::new("").expect_err("empty index name must reject");
    assert_eq!(error.code(), "invalid_argument.engine.json_index_name");
}

/// Vector metadata exceeding the 16 MiB encoded ceiling is rejected.
#[test]
fn vector_metadata_too_large_is_rejected() {
    let oversized = json!({ "blob": "x".repeat(17 * 1024 * 1024) });
    let error = VectorMetadata::new(oversized).expect_err("oversized metadata must reject");
    assert_eq!(
        error.code(),
        "invalid_argument.engine.vector_metadata_too_large"
    );
}

/// Vector metadata that is not a JSON object (list, scalar, bool, null) is
/// rejected. Filters match on object fields, so a non-object row would be
/// stored verbatim and then silently unfilterable.
#[test]
fn vector_metadata_must_be_a_json_object() {
    for value in [
        json!([1, 2, 3]),
        json!("scalar"),
        json!(7),
        json!(true),
        json!(null),
    ] {
        let error =
            VectorMetadata::new(value.clone()).expect_err("non-object metadata must reject");
        assert_eq!(
            error.code(),
            "invalid_argument.engine.vector_metadata",
            "value {value} must reject as non-object"
        );
    }

    // An object — including an empty one — is accepted.
    VectorMetadata::new(json!({})).expect("empty object metadata is valid");
    VectorMetadata::new(json!({ "kind": "doc" })).expect("object metadata is valid");
}

/// A JSON batch-delete carrying the same document id twice is rejected — the
/// duplicate-id refusal reached through the public `batch_delete` API.
#[test]
fn json_batch_delete_rejects_duplicate_document_ids() {
    let mut database = open_cache_database().expect("cache open");
    let mut json = database
        .json(branch("default"), space("default"))
        .expect("json service");

    let id = JsonDocumentId::new("dup").expect("doc id");
    let error = json
        .batch_delete([id.clone(), id])
        .expect_err("duplicate ids in one batch must reject");
    assert_eq!(
        error.code(),
        "invalid_argument.engine.json_batch_duplicate_document"
    );
}
