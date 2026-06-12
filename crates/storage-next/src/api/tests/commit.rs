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

fn multi_byte_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20, 0x21]).expect("valid opaque storage space")
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
fn commit_rejects_zero_ttl() {
    let error = CommitBatch::new(
        branch(),
        vec![put_mutation_with_ttl(b"zero-ttl", b"value", Duration::ZERO)],
        CommitOptions::default(),
    )
    .expect_err("zero TTL rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
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
#[cfg(feature = "localfs")]
fn commit_rejects_always_request_on_standard_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("commit-always-on-standard"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"always-on-standard", b"value")],
        CommitOptions::default().with_durability(CommitDurability::Always),
    )
    .expect("valid shape");

    let error = runtime
        .commit(&batch)
        .expect_err("always request rejected by standard runtime");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert_eq!(error.code(), "unsupported.storage_api.capability");
}

#[test]
#[cfg(feature = "localfs")]
fn commit_rejects_standard_request_on_always_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("commit-standard-on-always"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"standard-on-always", b"value")],
        CommitOptions::default().with_durability(CommitDurability::Standard),
    )
    .expect("valid shape");

    let error = runtime
        .commit(&batch)
        .expect_err("standard request rejected by always runtime");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert_eq!(error.code(), "unsupported.storage_api.capability");
}

#[test]
#[cfg(feature = "localfs")]
fn commit_rejects_not_durable_request_on_durable_runtime() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("commit-not-durable-on-durable"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"not-durable-on-durable", b"value")],
        CommitOptions::default().with_durability(CommitDurability::NotDurable),
    )
    .expect("valid shape");

    let error = runtime
        .commit(&batch)
        .expect_err("not-durable request rejected by durable runtime");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert_eq!(error.code(), "unsupported.storage_api.capability");
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
    assert_eq!(
        summary.admission().status(),
        CommitAdmissionStatus::AcceptedClean
    );
    assert_eq!(
        summary.admission().pressure_severity(),
        CommitAdmissionPressureSeverity::None
    );
    assert_eq!(
        summary.admission().pressure_reason(),
        CommitAdmissionPressureReason::None
    );
    assert!(!summary.admission().inline_maintenance_driven());
    assert!(!summary.admission().cleared_prior_pressure_rejection());
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
#[cfg(feature = "localfs")]
fn durable_runtime_default_uses_configured_policy() {
    let standard_backend =
        StorageBackend::local_fs(temp_dir_for_api_test("commit-runtime-default-standard"));
    let mut standard_runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &standard_backend,
    )
    .expect("standard durable open")
    .into_runtime();
    let standard_summary = standard_runtime
        .commit(
            &CommitBatch::new(
                branch(),
                vec![put_mutation(b"runtime-default-standard", b"value")],
                CommitOptions::default().with_durability(CommitDurability::RuntimeDefault),
            )
            .expect("valid batch"),
        )
        .expect("standard commit");
    assert_eq!(
        standard_summary.durability(),
        CommitDurabilitySummary::Standard
    );

    let always_backend =
        StorageBackend::local_fs(temp_dir_for_api_test("commit-runtime-default-always"));
    let mut always_runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &always_backend,
    )
    .expect("always durable open")
    .into_runtime();
    let always_summary = always_runtime
        .commit(
            &CommitBatch::new(
                branch(),
                vec![put_mutation(b"runtime-default-always", b"value")],
                CommitOptions::default().with_durability(CommitDurability::RuntimeDefault),
            )
            .expect("valid batch"),
        )
        .expect("always commit");
    assert_eq!(always_summary.durability(), CommitDurabilitySummary::Always);
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
fn commit_rejected_request_does_not_allocate_version() {
    let mut runtime = open_runtime();
    let first = runtime
        .commit(&put_batch(b"before-reject", b"value"))
        .expect("first commit");
    let rejected = CommitBatch::new(
        branch(),
        vec![put_mutation(b"rejected", b"value")],
        CommitOptions::default().with_durability(CommitDurability::Standard),
    )
    .expect("valid shape");

    let error = runtime
        .commit(&rejected)
        .expect_err("unsupported durability rejected");
    let second = runtime
        .commit(&put_batch(b"after-reject", b"value"))
        .expect("second commit");

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert_eq!(
        second.commit_version(),
        first.commit_version().checked_next().expect("next version")
    );
}

#[test]
fn commit_rejects_ttl_duration_too_large() {
    let mut runtime = open_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation_with_ttl(
            b"ttl-too-large",
            b"value",
            Duration::MAX,
        )],
        CommitOptions::default(),
    )
    .expect("valid shape");

    let error = runtime.commit(&batch).expect_err("TTL overflow rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn commit_rejects_ttl_expiration_overflow() {
    let mut runtime = open_runtime();
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation_with_ttl(
            b"ttl-expiration-overflow",
            b"value",
            Duration::from_micros(1),
        )],
        CommitOptions::default(),
    )
    .expect("valid shape");

    let error = runtime
        .commit_for_test(&batch, Timestamp::from_micros(u64::MAX))
        .expect_err("expiration overflow rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
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
fn commit_expected_absent_succeeds_after_visible_delete() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"deleted-cas", b"old"))
        .expect("first");
    runtime
        .commit(&delete_batch(b"deleted-cas"))
        .expect("delete");
    assert!(read_latest(&runtime, b"deleted-cas")
        .row()
        .expect("tombstone")
        .is_tombstone());
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"deleted-cas", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_absent(
        engine_space(),
        api_key(b"deleted-cas"),
    )])
    .expect("valid condition");

    runtime
        .commit(&batch)
        .expect("visible tombstone counts as absent for CAS");

    let row = read_latest(&runtime, b"deleted-cas")
        .row()
        .cloned()
        .expect("new row");
    assert!(!row.is_tombstone());
    assert_eq!(row.value().expect("value").as_bytes(), b"new");
}

#[test]
fn commit_expected_present_rejects_after_visible_delete() {
    let mut runtime = open_runtime();
    let first = runtime
        .commit(&put_batch(b"deleted-present-cas", b"old"))
        .expect("first");
    runtime
        .commit(&delete_batch(b"deleted-present-cas"))
        .expect("delete");
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"deleted-present-cas", b"new")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_present(
        engine_space(),
        api_key(b"deleted-present-cas"),
        first.commit_version(),
    )])
    .expect("valid condition");

    let error = runtime
        .commit(&batch)
        .expect_err("visible tombstone is not present for CAS");

    assert_eq!(error.class(), StorageApiErrorClass::Conflict);
    assert!(read_latest(&runtime, b"deleted-present-cas")
        .row()
        .expect("tombstone remains after failed condition")
        .is_tombstone());
}

#[test]
fn commit_conditions_are_explicit_cas_not_captured_read_sets() {
    let mut runtime = open_runtime();
    let guarded = runtime
        .commit(&put_batch(b"guarded", b"v1"))
        .expect("initial guarded row");
    runtime
        .commit(&put_batch(b"unrelated", b"v1"))
        .expect("initial unrelated row");
    assert!(read_latest(&runtime, b"unrelated").row().is_some());
    runtime
        .commit(&put_batch(b"unrelated", b"v2"))
        .expect("unrelated row can change before conditional commit");
    let conditioned = CommitBatch::new(
        branch(),
        vec![put_mutation(b"guarded", b"v2")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_present(
        engine_space(),
        api_key(b"guarded"),
        guarded.commit_version(),
    )])
    .expect("valid condition");

    runtime
        .commit(&conditioned)
        .expect("only the explicit guarded condition is checked");
}

#[test]
fn commit_condition_rejects_zero_expected_present_version() {
    let error = CommitBatch::new(
        branch(),
        vec![put_mutation(b"zero-condition", b"value")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_present(
        engine_space(),
        api_key(b"zero-condition"),
        CommitVersion::ZERO,
    )])
    .expect_err("zero expected-present version is malformed");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
    match error {
        StorageApiError::InvalidArgument { field, reason } => {
            assert_eq!(field, "conditions");
            assert_eq!(reason, "expected present version must be nonzero");
        }
        other => panic!("expected invalid condition argument, got {other:?}"),
    }
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
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durability_uncertain_with(
            branch(),
            CommitVersion::new(2),
            "durable WAL append did not complete",
            SourceError,
        ),
    );

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
    assert_eq!(
        error.code(),
        "ambiguous_commit.storage_api.durable_uncertain"
    );
    assert!(error.source().is_some());
}

#[test]
fn commit_durability_uncertain_survives_boundary() {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::DurabilityUncertain {
            branch_id: branch(),
            commit_version: CommitVersion::new(3),
            reason: "durability is uncertain",
            source: None,
        },
    );

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
}

#[test]
fn commit_applied_not_visible_survives_boundary() {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::AppliedButNotVisible {
            branch_id: branch(),
            commit_version: CommitVersion::new(4),
            reason: "commit was applied but not visible",
        },
    );

    assert_eq!(
        error.code(),
        "ambiguous_commit.storage_api.durable_uncertain"
    );
}

#[test]
fn commit_disabled_read_only_diagnostics_maps_to_api_capability_error() {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        },
    );

    assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    assert_eq!(error.code(), "unsupported.storage_api.capability");
    assert!(error.source().is_none());
    match error {
        StorageApiError::UnsupportedCapability { capability, reason } => {
            assert_eq!(capability, "read_only_diagnostics");
            assert_eq!(reason, "read-only diagnostics are disabled");
        }
        other => panic!("expected unsupported diagnostics capability, got {other:?}"),
    }
}

#[test]
fn commit_visibility_publish_failure_preserves_source_chain() {
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::durable_but_not_visible_with(
            branch(),
            CommitVersion::new(5),
            "visibility publication failed",
            SourceError,
        ),
    );

    assert!(error.source().is_some());
}

#[test]
fn commit_rejects_condition_with_multi_byte_storage_space() {
    let batch = CommitBatch::new(
        branch(),
        vec![put_mutation(b"condition-space", b"value")],
        CommitOptions::default(),
    )
    .expect("valid batch")
    .with_conditions(vec![CommitCondition::expected_absent(
        multi_byte_space(),
        api_key(b"condition-space"),
    )])
    .expect("condition shape accepted");
    let mut runtime = open_runtime();

    let error = runtime
        .commit(&batch)
        .expect_err("condition storage space rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
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
    let error = crate::api::map_commit_error_for_test(
        crate::commit::CommitRuntimeError::UnresolvedDurableCommit {
            branch_id: branch(),
            commit_version: CommitVersion::new(6),
            reason: "unresolved durable commit must recover before follow-up commits",
        },
    );

    assert_eq!(error.class(), StorageApiErrorClass::AmbiguousCommit);
}

#[test]
fn commit_storage_pressure_rejection_maps_to_retryable_api_error() {
    let error = crate::api::map_lifecycle_error_for_test(
        crate::lifecycle::LifecycleError::StoragePressureRejected {
            branch_id: branch(),
            severity: crate::lifecycle::LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            pressure_reason:
                crate::lifecycle::LifecycleStoragePressureReason::LevelZeroTableBacklog,
            retryable: true,
            reason: "mutating commit admission requires maintenance progress",
        },
    );

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.storage_pressure"
    );
    assert!(matches!(
        error,
        StorageApiError::StoragePressure {
            branch_id,
            severity: crate::api::CommitAdmissionPressureSeverity::Blocking,
            pressure_reason:
                crate::api::CommitAdmissionPressureReason::LevelZeroTableBacklog,
            retryable: true,
            ..
        } if branch_id == branch()
    ));
}

#[test]
fn public_open_uses_background_maintenance_policy() {
    let runtime = open_runtime();

    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
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
