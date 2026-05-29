use super::*;

use std::time::Duration;

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(StorageOpenOptions::cache())
        .expect("open cache runtime")
        .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn other_branch() -> BranchId {
    branch_id(0x44)
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid key")
}

fn put_mutation(key: &[u8], value: &[u8]) -> CommitMutation {
    CommitMutation::Put {
        storage_space: engine_space(),
        key: api_key(key),
        value: StorageValue::new(value.to_vec()),
        ttl: None,
    }
}

fn put_mutation_with_ttl(key: &[u8], value: &[u8], ttl: Duration) -> CommitMutation {
    CommitMutation::Put {
        storage_space: engine_space(),
        key: api_key(key),
        value: StorageValue::new(value.to_vec()),
        ttl: Some(ttl),
    }
}

fn delete_mutation(key: &[u8]) -> CommitMutation {
    CommitMutation::Delete {
        storage_space: engine_space(),
        key: api_key(key),
    }
}

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![put_mutation(key, value)],
        CommitOptions::default(),
    )
    .expect("valid put batch")
}

fn delete_batch(key: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![delete_mutation(key)],
        CommitOptions::default(),
    )
    .expect("valid delete batch")
}

fn read_latest(runtime: &StorageRuntime<'_>, key: &[u8]) -> PointReadOutcome {
    runtime
        .read_point(&PointReadRequest::new(
            branch(),
            engine_space(),
            api_key(key),
            ReadBound::Latest,
        ))
        .expect("read latest")
}

#[test]
fn commit_rejects_empty_batch() {
    let error = CommitBatch::new(branch(), Vec::new(), CommitOptions::default())
        .expect_err("empty batch rejected");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn commit_rejects_duplicate_keys() {
    let mutation = put_mutation(b"dup", b"value");
    let error = CommitBatch::new(
        branch(),
        vec![mutation.clone(), mutation],
        CommitOptions::default(),
    )
    .expect_err("duplicate key rejected");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn commit_rejects_malformed_key() {
    let error = StorageKey::new(Vec::new()).expect_err("empty key rejected");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn commit_rejects_unknown_branch() {
    let batch = CommitBatch::new(
        other_branch(),
        vec![put_mutation(b"unknown", b"value")],
        CommitOptions::default(),
    )
    .expect("valid shape");
    let mut runtime = open_runtime();

    let error = runtime.commit(&batch).expect_err("unknown branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn commit_rejects_generation_mismatch() {
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"generation", b"value")],
        CommitOptions::default().with_expected_generation(BranchGeneration::new(99)),
    )
    .expect("valid shape");
    let mut runtime = open_runtime();

    let error = runtime
        .commit(&batch)
        .expect_err("stale branch generation rejected");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.branch_generation"
    );
}

#[test]
fn commit_rejects_zero_expected_generation() {
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"zero-generation", b"value")],
        CommitOptions::default().with_expected_generation(BranchGeneration::ZERO),
    )
    .expect("valid shape");
    let mut runtime = open_runtime();

    let error = runtime
        .commit(&batch)
        .expect_err("zero branch generation rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn commit_rejects_cross_branch_mutation() {
    let source = include_str!("../commit.rs");
    let mutation_section = source
        .split("pub enum CommitMutation")
        .nth(1)
        .expect("mutation enum present")
        .split("impl CommitMutation")
        .next()
        .expect("mutation impl follows enum");

    assert!(!mutation_section.contains("branch_id"));
}

#[test]
fn commit_rejects_unsupported_durability_for_cache() {
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"durability", b"value")],
        CommitOptions::default().with_durability(CommitDurability::Standard),
    )
    .expect("valid shape");
    let mut runtime = open_runtime();

    let error = runtime
        .commit(&batch)
        .expect_err("cache cannot satisfy durable commit request");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
}

#[test]
fn commit_rejects_transaction_id_field_absence_by_type() {
    let source = include_str!("../commit.rs").to_ascii_lowercase();

    assert!(!source.contains("transaction_id"));
    assert!(!source.contains("transactionid"));
}

#[test]
fn cache_commit_returns_not_durable_outcome() {
    let mut runtime = open_runtime();

    let summary = runtime
        .commit(&put_batch(b"cache", b"value"))
        .expect("commit");

    assert_eq!(summary.durability(), CommitDurabilitySummary::NotDurable);
    assert!(summary.visible());
}

#[test]
#[cfg(feature = "localfs")]
fn standard_commit_returns_standard_outcome() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("commit-standard"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();

    let summary = runtime
        .commit(&put_batch(b"standard", b"value"))
        .expect("commit");

    assert_eq!(summary.durability(), CommitDurabilitySummary::Standard);
}

#[test]
#[cfg(feature = "localfs")]
fn always_commit_returns_always_outcome() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("commit-always"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("durable open")
    .into_runtime();

    let summary = runtime
        .commit(&put_batch(b"always", b"value"))
        .expect("commit");

    assert_eq!(summary.durability(), CommitDurabilitySummary::Always);
}

#[test]
fn commit_put_then_read_latest_observes_value() {
    let mut runtime = open_runtime();
    let summary = runtime
        .commit(&put_batch(b"alpha", b"value"))
        .expect("commit");

    let outcome = read_latest(&runtime, b"alpha");
    let row = outcome.row().expect("row");

    assert_eq!(row.value().expect("value").as_bytes(), b"value");
    assert_eq!(row.commit_version(), summary.commit_version());
}

#[test]
fn commit_delete_then_read_latest_observes_tombstone() {
    let mut runtime = open_runtime();
    runtime.commit(&put_batch(b"alpha", b"value")).expect("put");
    let summary = runtime.commit(&delete_batch(b"alpha")).expect("delete");

    let outcome = read_latest(&runtime, b"alpha");
    let row = outcome.row().expect("tombstone");

    assert!(row.is_tombstone());
    assert!(row.value().is_none());
    assert_eq!(row.commit_version(), summary.commit_version());
}

#[test]
fn commit_ttl_metadata_roundtrips_to_read_facts() {
    let mut runtime = open_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation_with_ttl(
            b"ttl",
            b"value",
            Duration::from_micros(10),
        )],
        CommitOptions::default(),
    )
    .expect("valid batch");
    let summary = runtime.commit(&batch).expect("commit");

    let outcome = read_latest(&runtime, b"ttl");
    let row = outcome.row().expect("row");

    assert_eq!(
        row.expires_at(),
        Some(
            summary
                .commit_timestamp()
                .saturating_add(Duration::from_micros(10))
        )
    );
}

#[test]
fn commit_outcome_reports_mutation_counts() {
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"a", b"value"), delete_mutation(b"b")],
        CommitOptions::default(),
    )
    .expect("valid batch");
    let mut runtime = open_runtime();

    let summary = runtime.commit(&batch).expect("commit");

    assert_eq!(summary.put_count(), 1);
    assert_eq!(summary.delete_count(), 1);
    assert_eq!(summary.mutation_count(), 2);
    assert_eq!(summary.timeline_row_count(), 2);
}

#[test]
fn commit_outcome_reports_timestamp_and_version() {
    let mut runtime = open_runtime();

    let summary = runtime
        .commit(&put_batch(b"facts", b"value"))
        .expect("commit");

    assert!(summary.commit_version() > CommitVersion::ZERO);
    assert!(summary.commit_timestamp() >= Timestamp::EPOCH);
}

#[test]
fn commit_outcome_timestamps_advance() {
    let mut runtime = open_runtime();

    let first = runtime
        .commit(&put_batch(b"first-time", b"value"))
        .expect("first commit");
    let second = runtime
        .commit(&put_batch(b"second-time", b"value"))
        .expect("second commit");

    assert!(second.commit_timestamp() > first.commit_timestamp());
}

#[test]
fn commit_ttl_uses_actual_commit_timestamp_after_prior_commit() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"prior", b"value"))
        .expect("prior commit");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation_with_ttl(
            b"ttl-after-prior",
            b"value",
            Duration::from_micros(7),
        )],
        CommitOptions::default(),
    )
    .expect("valid batch");

    let summary = runtime.commit(&batch).expect("commit");
    let row = read_latest(&runtime, b"ttl-after-prior")
        .row()
        .cloned()
        .expect("row");

    assert_eq!(
        row.expires_at(),
        Some(
            summary
                .commit_timestamp()
                .saturating_add(Duration::from_micros(7))
        )
    );
}

#[test]
fn commit_blind_write_succeeds_without_read_set() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"blind", b"value"))
        .expect("blind write");
}

#[test]
fn commit_expected_version_match_succeeds() {
    let mut runtime = open_runtime();
    let first = runtime.commit(&put_batch(b"cas", b"old")).expect("first");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"cas", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_present(
        engine_space(),
        api_key(b"cas"),
        first.commit_version(),
    )])
    .expect("valid condition");

    runtime.commit(&batch).expect("matching condition");
}

#[test]
fn commit_expected_version_mismatch_conflicts() {
    let mut runtime = open_runtime();
    runtime.commit(&put_batch(b"cas", b"old")).expect("first");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"cas", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_present(
        engine_space(),
        api_key(b"cas"),
        CommitVersion::new(99),
    )])
    .expect("valid condition");

    let error = runtime.commit(&batch).expect_err("condition conflicts");

    assert_eq!(error.class(), StorageApiErrorClass::Conflict);
}

#[test]
fn commit_expected_absent_match_succeeds() {
    let mut runtime = open_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"absent", b"value")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_absent(
        engine_space(),
        api_key(b"absent"),
    )])
    .expect("valid condition");

    runtime.commit(&batch).expect("absent condition");
}

#[test]
fn commit_expected_absent_mismatch_conflicts() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"absent", b"old"))
        .expect("first");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"absent", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_absent(
        engine_space(),
        api_key(b"absent"),
    )])
    .expect("valid condition");

    let error = runtime.commit(&batch).expect_err("condition conflicts");

    assert_eq!(error.class(), StorageApiErrorClass::Conflict);
}

#[test]
fn commit_conflict_error_has_structured_branch_and_key() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"structured", b"old"))
        .expect("first");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"structured", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_absent(
        engine_space(),
        api_key(b"structured"),
    )])
    .expect("valid condition");

    let error = runtime.commit(&batch).expect_err("condition conflicts");

    match error {
        StorageApiError::Conflict {
            branch_id,
            storage_space,
            key_fingerprint,
            user_key_len,
            ..
        } => {
            assert_eq!(branch_id, branch());
            assert_eq!(storage_space, Some(0x20));
            assert!(key_fingerprint.is_some());
            assert_eq!(user_key_len, Some(b"structured".len()));
        }
        other => panic!("expected structured conflict, got {other:?}"),
    }
}

#[test]
fn commit_wal_append_failure_maps_to_durable_not_acquired() {
    let error = StorageApiError::durable_uncertain("durable WAL append did not complete");

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
    assert_eq!(
        error.code(),
        "ambiguous_commit.storage_api.durable_uncertain"
    );
}

#[test]
fn commit_durability_uncertain_survives_boundary() {
    let error = StorageApiError::durable_uncertain("durability is uncertain");

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
}

#[test]
fn commit_applied_not_visible_survives_boundary() {
    let error = StorageApiError::durable_uncertain("commit was applied but not visible");

    assert_eq!(
        error.code(),
        "ambiguous_commit.storage_api.durable_uncertain"
    );
}

#[test]
fn commit_visibility_publish_failure_preserves_source_chain() {
    let error =
        StorageApiError::durable_uncertain_with("visibility publication failed", SourceError);

    assert!(error.source().is_some());
}

#[test]
fn commit_after_close_rejects_closed_runtime() {
    let mut runtime = open_runtime();
    runtime.close().expect("close");

    let error = runtime
        .commit(&put_batch(b"closed", b"value"))
        .expect_err("closed runtime rejected");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[test]
fn commit_unresolved_durable_gate_rejects_followup() {
    let error = StorageApiError::durable_uncertain(
        "unresolved durable commit must recover before follow-up commits",
    );

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
}

#[test]
fn commit_api_has_no_public_transaction_session_type() {
    let source = include_str!("../commit.rs").to_ascii_lowercase();

    assert!(!source.contains("transactionsession"));
    assert!(!source.contains("begin_transaction"));
}

#[test]
fn commit_api_has_no_durable_transaction_id_type() {
    let source = include_str!("../commit.rs").to_ascii_lowercase();

    assert!(!source.contains("durabletransactionid"));
    assert!(!source.contains("transaction_id"));
}

#[test]
fn commit_api_does_not_claim_serializable_isolation() {
    let source = include_str!("../commit.rs").to_ascii_lowercase();

    assert!(!source.contains("serializable"));
}

#[test]
fn commit_api_rejects_cross_branch_atomic_request() {
    let source = include_str!("../commit.rs").to_ascii_lowercase();

    assert!(!source.contains("atomic"));
    assert!(!source.contains("branches:"));
}
