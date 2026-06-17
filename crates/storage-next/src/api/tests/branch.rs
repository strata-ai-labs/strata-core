use super::*;

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open_ephemeral()
        .expect("open ephemeral runtime")
        .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn branch_with(byte: u8) -> BranchId {
    branch_id(byte)
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid key")
}

fn put_batch_for(branch_id: BranchId, key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch_id,
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid put batch")
}

fn put_at(
    runtime: &mut StorageRuntime<'_>,
    branch_id: BranchId,
    key: &[u8],
    value: &[u8],
    timestamp: u64,
) -> CommitSummary {
    runtime
        .commit_for_test(
            &put_batch_for(branch_id, key, value),
            Timestamp::from_micros(timestamp),
        )
        .expect("commit")
}

fn branch_request(branch_id: BranchId, action: BranchAction) -> BranchRequest {
    BranchRequest::new(branch_id, action, Some(BranchGeneration::new(1)))
}

fn create_request(branch_id: BranchId) -> BranchRequest {
    branch_request(branch_id, BranchAction::Create)
}

fn describe_request(branch_id: BranchId) -> BranchRequest {
    BranchRequest::new(branch_id, BranchAction::Describe, None)
}

fn list_request() -> BranchRequest {
    BranchRequest::new(branch(), BranchAction::List, None)
}

fn read_value(runtime: &StorageRuntime<'_>, branch_id: BranchId, key: &[u8]) -> Option<Vec<u8>> {
    runtime
        .read_point(&PointReadRequest::new(
            branch_id,
            engine_space(),
            api_key(key),
            ReadBound::Latest,
        ))
        .expect("read")
        .row()
        .and_then(|row| row.value().map(|value| value.as_bytes().to_vec()))
}

#[test]
fn branch_create_returns_generation() {
    let mut runtime = open_runtime();
    let new_branch = branch_with(0x20);

    let outcome = runtime
        .branch(&create_request(new_branch))
        .expect("create branch");
    let summary = outcome.branch().expect("created branch");

    assert_eq!(outcome.operation(), BranchOperation::Created);
    assert_eq!(summary.branch_id(), new_branch);
    assert_eq!(summary.status(), BranchStatus::Active);
    assert_eq!(summary.generation(), BranchGeneration::new(1));
    assert_eq!(outcome.generation_after(), Some(BranchGeneration::new(1)));
}

#[test]
fn branch_create_duplicate_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&create_request(branch()))
        .expect_err("duplicate branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::AlreadyExists);
}

#[test]
fn branch_create_invalid_identifier_rejects() {
    let mut runtime = open_runtime();
    let zero = BranchId::from_bytes([0; BranchId::BYTE_LEN]);

    let error = runtime
        .branch(&create_request(zero))
        .expect_err("zero branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn branch_list_is_deterministic() {
    let mut runtime = open_runtime();
    runtime
        .branch(&create_request(branch_with(0x33)))
        .expect("create third");
    runtime
        .branch(&create_request(branch_with(0x22)))
        .expect("create second");

    let outcome = runtime.branch(&list_request()).expect("list branches");
    let ids = outcome
        .branches()
        .iter()
        .map(|branch| branch.branch_id())
        .collect::<Vec<_>>();

    assert_eq!(outcome.operation(), BranchOperation::Listed);
    assert_eq!(ids, vec![branch(), branch_with(0x22), branch_with(0x33)]);
}

#[test]
fn branch_describe_reports_generation() {
    let mut runtime = open_runtime();

    let outcome = runtime
        .branch(&describe_request(branch()))
        .expect("describe branch");
    let summary = outcome.branch().expect("branch");

    assert_eq!(outcome.operation(), BranchOperation::Described);
    assert_eq!(summary.branch_id(), branch());
    assert_eq!(summary.generation(), BranchGeneration::new(1));
    assert_eq!(summary.status(), BranchStatus::Active);
}

#[test]
fn branch_describe_unknown_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&describe_request(branch_with(0x41)))
        .expect_err("unknown branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn branch_fork_current_copies_visible_frontier() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"key", b"old", 10);
    put_at(&mut runtime, branch(), b"key", b"new", 20);
    let child = branch_with(0x42);

    let outcome = runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect("fork current");

    assert_eq!(outcome.operation(), BranchOperation::Forked);
    assert_eq!(outcome.source_branch_id(), Some(branch()));
    assert_eq!(outcome.fork_version(), Some(CommitVersion::new(2)));
    assert_eq!(read_value(&runtime, child, b"key"), Some(b"new".to_vec()));
}

#[test]
fn branch_fork_current_preserves_inherited_visibility() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"shared", b"parent", 10);
    let child = branch_with(0x43);

    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect("fork");
    put_at(&mut runtime, child, b"child-only", b"child", 20);

    assert_eq!(
        read_value(&runtime, child, b"shared"),
        Some(b"parent".to_vec())
    );
    assert_eq!(
        read_value(&runtime, child, b"child-only"),
        Some(b"child".to_vec())
    );
    assert_eq!(read_value(&runtime, branch(), b"child-only"), None);
}

#[test]
fn branch_fork_at_retained_version_succeeds() {
    let mut runtime = open_runtime();
    let first = put_at(&mut runtime, branch(), b"history", b"one", 10);
    put_at(&mut runtime, branch(), b"history", b"two", 20);
    let child = branch_with(0x44);

    let outcome = runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkAtVersion {
                source: branch(),
                version: first.commit_version(),
            },
        ))
        .expect("fork at retained version");

    assert_eq!(outcome.fork_version(), Some(first.commit_version()));
    assert_eq!(
        read_value(&runtime, child, b"history"),
        Some(b"one".to_vec())
    );
}

#[test]
fn branch_fork_at_retained_watermark_between_commits_succeeds() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"history-gap", b"one", 10);
    let other = branch_with(0x55);
    runtime
        .branch(&create_request(other))
        .expect("create other");
    put_at(&mut runtime, other, b"other", b"two", 20);
    put_at(&mut runtime, branch(), b"history-gap", b"three", 30);
    let child = branch_with(0x56);

    let outcome = runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkAtVersion {
                source: branch(),
                version: CommitVersion::new(2),
            },
        ))
        .expect("fork at retained watermark");

    assert_eq!(outcome.fork_version(), Some(CommitVersion::new(2)));
    assert_eq!(
        read_value(&runtime, child, b"history-gap"),
        Some(b"one".to_vec())
    );
}

#[test]
fn branch_fork_at_unretained_version_rejects() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"history", b"one", 10);

    let error = runtime
        .branch(&branch_request(
            branch_with(0x45),
            BranchAction::ForkAtVersion {
                source: branch(),
                version: CommitVersion::new(99),
            },
        ))
        .expect_err("unretained version rejected");

    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn branch_fork_from_empty_source_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&branch_request(
            branch_with(0x5a),
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect_err("empty source history rejected");

    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn branch_fork_invalid_source_identifier_rejects() {
    let mut runtime = open_runtime();
    let zero = BranchId::from_bytes([0; BranchId::BYTE_LEN]);

    let error = runtime
        .branch(&branch_request(
            branch_with(0x57),
            BranchAction::ForkCurrent { source: zero },
        ))
        .expect_err("zero source branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn branch_fork_at_timestamp_resolves_timeline() {
    let mut runtime = open_runtime();
    let first = put_at(&mut runtime, branch(), b"timed", b"one", 10);
    put_at(&mut runtime, branch(), b"timed", b"two", 30);
    let child = branch_with(0x46);

    let outcome = runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkAtTimestamp {
                source: branch(),
                timestamp: Timestamp::from_micros(20),
            },
        ))
        .expect("fork at timestamp");

    assert_eq!(outcome.fork_version(), Some(first.commit_version()));
    assert_eq!(outcome.fork_timestamp(), Some(Timestamp::from_micros(20)));
    assert_eq!(read_value(&runtime, child, b"timed"), Some(b"one".to_vec()));
}

#[test]
fn branch_fork_at_unretained_timestamp_rejects() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"timed", b"one", 10);

    let error = runtime
        .branch(&branch_request(
            branch_with(0x47),
            BranchAction::ForkAtTimestamp {
                source: branch(),
                timestamp: Timestamp::from_micros(5),
            },
        ))
        .expect_err("unretained timestamp rejected");

    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn branch_fork_generation_mismatch_rejects() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"history", b"one", 10);
    let child = branch_with(0x48);
    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect("initial fork");
    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete destination");

    let error = runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::ForkCurrent { source: branch() },
            Some(BranchGeneration::new(1)),
        ))
        .expect_err("destination generation mismatch rejected");

    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.branch_generation"
    );
}

#[test]
fn branch_fork_after_close_rejects() {
    let mut runtime = open_runtime();
    runtime.close().expect("close");

    let error = runtime
        .branch(&branch_request(
            branch_with(0x49),
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect_err("closed runtime rejected");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[test]
fn branch_clear_removes_visible_rows() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"clear", b"value", 10);

    let outcome = runtime
        .branch(&branch_request(branch(), BranchAction::Clear))
        .expect("clear");

    assert_eq!(outcome.operation(), BranchOperation::Cleared);
    assert_eq!(read_value(&runtime, branch(), b"clear"), None);
}

#[test]
fn branch_clear_preserves_branch_identity() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"clear", b"value", 10);

    let outcome = runtime
        .branch(&branch_request(branch(), BranchAction::Clear))
        .expect("clear");
    let summary = outcome.branch().expect("cleared branch");

    assert_eq!(summary.branch_id(), branch());
    assert_eq!(summary.status(), BranchStatus::Active);
    assert_eq!(summary.generation(), BranchGeneration::new(1));
}

#[test]
fn branch_clear_generation_mismatch_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&BranchRequest::new(
            branch(),
            BranchAction::Clear,
            Some(BranchGeneration::new(2)),
        ))
        .expect_err("generation mismatch rejected");

    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.branch_generation"
    );
}

#[test]
fn branch_clear_with_pinned_view_reports_protected_release() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"pinned", b"value", 10);
    runtime
        .flush_default_branch_for_test()
        .expect("flush table");
    runtime
        .pin_branch_reachability_for_test(branch())
        .expect("pin reachability");

    let outcome = runtime
        .branch(&branch_request(branch(), BranchAction::Clear))
        .expect("clear with pinned reachability");
    let cleanup = outcome.cleanup().expect("cleanup");

    assert_eq!(outcome.operation(), BranchOperation::Cleared);
    assert_eq!(read_value(&runtime, branch(), b"pinned"), None);
    assert_eq!(cleanup.releasable_tables(), 0);
    assert!(
        cleanup.protected_tables() > 0,
        "pinned reachability must block table release"
    );
}

#[test]
fn branch_delete_removes_from_list() {
    let mut runtime = open_runtime();
    let child = branch_with(0x4a);
    runtime.branch(&create_request(child)).expect("create");

    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete");
    let branches = runtime.branch(&list_request()).expect("list");

    assert!(!branches
        .branches()
        .iter()
        .any(|summary| summary.branch_id() == child));
}

#[test]
fn branch_delete_generation_mismatch_rejects() {
    let mut runtime = open_runtime();
    let child = branch_with(0x4b);
    runtime.branch(&create_request(child)).expect("create");

    let error = runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::Delete,
            Some(BranchGeneration::new(2)),
        ))
        .expect_err("generation mismatch rejected");

    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.branch_generation"
    );
}

#[test]
fn branch_delete_with_pinned_view_reports_protected_release() {
    let mut runtime = open_runtime();
    put_at(&mut runtime, branch(), b"pinned-delete", b"value", 10);
    runtime
        .flush_default_branch_for_test()
        .expect("flush table");
    runtime
        .pin_branch_reachability_for_test(branch())
        .expect("pin reachability");
    runtime
        .branch(&create_request(branch_with(0x4c)))
        .expect("create remaining active branch");

    let outcome = runtime
        .branch(&branch_request(branch(), BranchAction::Delete))
        .expect("delete with pinned reachability");
    let cleanup = outcome.cleanup().expect("cleanup");

    assert_eq!(outcome.operation(), BranchOperation::Deleted);
    assert_eq!(cleanup.releasable_tables(), 0);
    assert!(
        cleanup.protected_tables() > 0,
        "pinned reachability must block table release"
    );
}

#[test]
fn branch_recreate_deleted_reports_generation_transition() {
    let mut runtime = open_runtime();
    let child = branch_with(0x58);
    runtime.branch(&create_request(child)).expect("create");
    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete");

    let outcome = runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::Create,
            Some(BranchGeneration::new(2)),
        ))
        .expect("recreate deleted branch");
    let summary = outcome.branch().expect("branch summary");

    assert_eq!(outcome.operation(), BranchOperation::Created);
    assert_eq!(outcome.generation_before(), Some(BranchGeneration::new(1)));
    assert_eq!(outcome.generation_after(), Some(BranchGeneration::new(2)));
    assert_eq!(summary.status(), BranchStatus::Active);
    assert_eq!(summary.generation(), BranchGeneration::new(2));
}

#[cfg(feature = "localfs")]
#[test]
fn durable_branch_catalog_round_trips_after_reopen() {
    let root = temp_dir_for_api_test("branch-durable-roundtrip");
    let backend = StorageBackend::local_fs(root.clone());
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    put_at(&mut runtime, branch(), b"durable-branch", b"parent", 10);
    let child = branch_with(0x59);
    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect("fork durable branch");
    runtime.close().expect("close durable runtime");
    drop(runtime);

    let backend = StorageBackend::local_fs(root);
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable reopen")
    .into_runtime();
    let described = runtime
        .branch(&describe_request(child))
        .expect("describe recovered branch")
        .branch()
        .expect("branch summary");

    assert_eq!(described.status(), BranchStatus::Active);
    assert_eq!(
        described
            .parent()
            .map(BranchParentSummary::source_branch_id),
        Some(branch())
    );
    assert_eq!(
        read_value(&runtime, child, b"durable-branch"),
        Some(b"parent".to_vec())
    );
}

#[test]
fn branch_delete_unknown_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&branch_request(branch_with(0x4d), BranchAction::Delete))
        .expect_err("unknown branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn branch_delete_already_deleted_rejects() {
    let mut runtime = open_runtime();
    let child = branch_with(0x5b);
    runtime.branch(&create_request(child)).expect("create");
    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete");

    let error = runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect_err("deleted branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn branch_delete_reports_cleanup_facts() {
    let mut runtime = open_runtime();
    let child = branch_with(0x4e);
    runtime.branch(&create_request(child)).expect("create");

    let outcome = runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete");
    let cleanup = outcome.cleanup().expect("cleanup facts");

    assert_eq!(outcome.operation(), BranchOperation::Deleted);
    assert_eq!(cleanup.removed_refs(), 0);
    assert_eq!(cleanup.releasable_tables(), 0);
    assert_eq!(cleanup.protected_tables(), 0);
}

#[test]
fn branch_delete_last_required_branch_rejects() {
    let mut runtime = open_runtime();

    let error = runtime
        .branch(&branch_request(branch(), BranchAction::Delete))
        .expect_err("last branch delete rejected");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

fn branch_api_source() -> String {
    [include_str!("../branch.rs"), super::RUNTIME_SOURCE]
        .join("\n")
        .to_ascii_lowercase()
}

#[test]
fn branch_api_has_no_merge_method() {
    let source = branch_api_source();

    assert!(!source.contains("merge"));
}

#[test]
fn branch_api_has_no_cherry_pick_method() {
    let source = branch_api_source();

    assert!(!source.contains("cherry"));
}

#[test]
fn branch_api_has_no_revert_method() {
    let source = branch_api_source();

    assert!(!source.contains("revert"));
}

#[test]
fn branch_api_has_no_restore_method() {
    let source = branch_api_source();

    assert!(!source.contains("restore"));
}

#[test]
fn branch_api_has_no_publish_review_method() {
    let source = branch_api_source();

    assert!(!source.contains("pub fn publish"));
    assert!(!source.contains("pub fn review"));
}
