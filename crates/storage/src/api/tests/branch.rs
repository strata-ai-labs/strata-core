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
    let runtime = open_runtime();
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
    let runtime = open_runtime();

    let error = runtime
        .branch(&create_request(branch()))
        .expect_err("duplicate branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::AlreadyExists);
}

#[test]
fn branch_create_invalid_identifier_rejects() {
    let runtime = open_runtime();
    let zero = BranchId::from_bytes([0; BranchId::BYTE_LEN]);

    let error = runtime
        .branch(&create_request(zero))
        .expect_err("zero branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn branch_list_is_deterministic() {
    let runtime = open_runtime();
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
    let runtime = open_runtime();

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
    let runtime = open_runtime();

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
fn branch_fork_from_empty_source_creates_empty_parented_child() {
    // #2521: forking a history-less source is the legitimate empty-fork case
    // — an empty child at version zero with parent linkage intact. The old
    // rejection forced the engine into a silent `create_branch` fallback
    // that produced an UNPARENTED child (the silent-data-loss half of the
    // fork-of-a-fork regression).
    let runtime = open_runtime();

    let outcome = runtime
        .branch(&branch_request(
            branch_with(0x5a),
            BranchAction::ForkCurrent { source: branch() },
        ))
        .expect("empty source forks at version zero");
    let child = outcome.branches().first().expect("forked child");
    let parent = child.parent().expect("parent linkage survives");
    assert_eq!(parent.source_branch_id(), branch());
    assert_eq!(parent.fork_version(), CommitVersion::ZERO);
}

#[test]
fn branch_fork_invalid_source_identifier_rejects() {
    let runtime = open_runtime();
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
    let runtime = open_runtime();

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
    let runtime = open_runtime();
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
    let runtime = open_runtime();
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
    let runtime = open_runtime();
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
    let runtime = StorageRuntime::open_with_backend(
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

/// Fork-manifest fix: a `ForkCurrent` child of a FLUSHED parent is a COW fork (inherited layers,
/// no row copies), and the fork now publishes the child's table manifest at fork time — so after
/// reopen the child reads the parent's rows through manifest-recovered layers, not through the
/// O(parent dataset) `rebuild_fork_snapshot_rows` fallback. Complements the unflushed variant
/// above (whose eager child still recovers through the gated fallback).
#[cfg(feature = "localfs")]
#[test]
fn durable_flushed_parent_cow_fork_round_trips_after_reopen() {
    let root = temp_dir_for_api_test("branch-durable-cow-fork-roundtrip");
    let child = branch_with(0x5a);
    {
        let backend = StorageBackend::local_fs(root.clone());
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        put_at(&mut runtime, branch(), b"cow-fork", b"parent", 10);
        runtime
            .flush_default_branch_for_test()
            .expect("flush the parent so the fork is COW");
        runtime
            .branch(&branch_request(
                child,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork the flushed parent");
        runtime.close().expect("close durable runtime");
    }

    let backend = StorageBackend::local_fs(root);
    let runtime = StorageRuntime::open_with_backend(
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
        read_value(&runtime, child, b"cow-fork"),
        Some(b"parent".to_vec()),
        "the child must read the parent's row through its manifest-recovered inherited layer",
    );
}

/// Fork-manifest fix enabler: fork-time child manifests interleave manifest sequences across
/// branches (parent seq → child seq → parent seq), while recovery applies manifests in branch-id
/// order — `record_recovered_manifest` must tolerate the reordering (the strict runtime
/// regression check would fail recovery here).
#[cfg(feature = "localfs")]
#[test]
fn durable_interleaved_branch_manifest_sequences_recover() {
    let root = temp_dir_for_api_test("branch-durable-interleaved-manifests");
    let child = branch_with(0x5c);
    {
        let backend = StorageBackend::local_fs(root.clone());
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        // Parent manifest (seq A) → child manifest at fork (seq B > A) → parent manifest again
        // (seq C > B). Recovery loads them in branch-id order, so a strict sequence check would
        // see C then B and refuse.
        put_at(&mut runtime, branch(), b"interleaved-a", b"first", 10);
        runtime
            .flush_default_branch_for_test()
            .expect("first parent flush publishes the parent manifest");
        runtime
            .branch(&branch_request(
                child,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork publishes the child manifest");
        put_at(&mut runtime, branch(), b"interleaved-b", b"second", 20);
        runtime
            .flush_default_branch_for_test()
            .expect("second parent flush republishes the parent manifest");
        runtime.close().expect("close durable runtime");
    }

    let backend = StorageBackend::local_fs(root);
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable reopen with interleaved manifest sequences")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, branch(), b"interleaved-b"),
        Some(b"second".to_vec())
    );
    assert_eq!(
        read_value(&runtime, child, b"interleaved-a"),
        Some(b"first".to_vec()),
        "the child sees pre-fork parent rows, not post-fork ones",
    );
    assert_eq!(
        read_value(&runtime, child, b"interleaved-b"),
        None,
        "post-fork parent writes must not leak into the child",
    );
}

/// Fork-manifest fix crash window: the fork's catalog publish landed but its child-manifest
/// publish did not (simulated by deleting the child's manifest object). Reopen must still succeed
/// and the child must read the parent's rows — via the narrowly-kept `rebuild_fork_snapshot_rows`
/// fallback for layer-less children.
#[cfg(feature = "localfs")]
#[test]
fn durable_fork_child_manifest_crash_window_recovers_via_rebuild() {
    let root = temp_dir_for_api_test("branch-durable-fork-crash-window");
    let child = branch_with(0x5d);
    {
        let backend = StorageBackend::local_fs(root.clone());
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        put_at(&mut runtime, branch(), b"crash-window", b"parent", 10);
        runtime
            .flush_default_branch_for_test()
            .expect("flush the parent so the fork is COW");
        runtime
            .branch(&branch_request(
                child,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork the flushed parent");
        runtime.close().expect("close durable runtime");
    }

    // Simulate the crash window: the child's fork-time table manifest never became durable.
    // (`.object@` is the localfs backend's on-disk object-file suffix.)
    let child_manifest = root
        .join("tables")
        .join(child.to_string())
        .join("manifest.object@");
    assert!(
        child_manifest.is_file(),
        "the fork must have published the child's table manifest at {}",
        child_manifest.display()
    );
    std::fs::remove_file(&child_manifest).expect("delete the child's table manifest");

    let backend = StorageBackend::local_fs(root);
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable reopen without the child manifest")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, child, b"crash-window"),
        Some(b"parent".to_vec()),
        "a layer-less child must recover its fork view through the rebuild fallback",
    );
}

#[cfg(feature = "localfs")]
#[test]
fn durable_branch_delete_allows_reopen_after_process_drop() {
    let root = temp_dir_for_api_test("branch-durable-delete-reopen");
    let backend = StorageBackend::local_fs(root.clone());
    let child = branch_with(0x5a);
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    runtime
        .branch(&create_request(child))
        .expect("create branch");
    put_at(&mut runtime, child, b"deleted-branch-row", b"value", 10);
    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete branch");
    drop(runtime);

    let backend = StorageBackend::local_fs(root);
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable reopen after branch delete")
    .into_runtime();
    let described = runtime
        .branch(&describe_request(child))
        .expect("describe deleted branch")
        .branch()
        .expect("branch summary");

    assert_eq!(described.status(), BranchStatus::Deleted);
}

#[test]
fn branch_delete_refused_while_layerless_fork_children_live() {
    // Durable: the refusal protects RECOVERY, so cache mode (no recovery)
    // deliberately keeps unrestricted deletes — the branch-DAG model pins
    // that. Replay is likewise exempt (a WAL'd delete already happened).
    let root = temp_dir_for_api_test("branch-layerless-parent-delete-refusal");
    let backend = StorageBackend::local_fs(root);
    let parent = branch_with(0x60);
    let child = branch_with(0x61);
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    runtime
        .branch(&create_request(parent))
        .expect("create parent");
    let first = put_at(&mut runtime, parent, b"parent-row", b"one", 10);
    put_at(&mut runtime, parent, b"parent-row", b"two", 20);

    // A historical (eager) fork is layer-less: its materialized rows are not
    // WAL'd, so recovery re-materializes them from the parent's state.
    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkAtVersion {
                source: parent,
                version: first.commit_version(),
            },
        ))
        .expect("historical fork");

    // Deleting the source while such a child lives would arm a permanent
    // recovery failure — it must refuse.
    let error = runtime
        .branch(&branch_request(parent, BranchAction::Delete))
        .expect_err("deleting the layer-less fork's source must refuse");
    assert_eq!(error.code(), "failed_precondition.storage_api.state");

    // Direction control: once the dependent child is gone, the parent
    // delete proceeds.
    runtime
        .branch(&branch_request(child, BranchAction::Delete))
        .expect("delete child");
    runtime
        .branch(&branch_request(parent, BranchAction::Delete))
        .expect("delete parent after its children are gone");
}

#[test]
fn branch_delete_of_empty_fork_source_stays_allowed() {
    // A fork of a rowless parent re-materializes nothing at recovery: the
    // dependency check must consult the actual fork-visible rows (an
    // always-dependent fold would refuse here — the mutation the engine's
    // cache-mode DAG model cannot see).
    let root = temp_dir_for_api_test("branch-empty-fork-parent-delete");
    let backend = StorageBackend::local_fs(root);
    let parent = branch_with(0x66);
    let child = branch_with(0x67);
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    runtime
        .branch(&create_request(parent))
        .expect("create parent");
    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: parent },
        ))
        .expect("fork empty parent");
    runtime
        .branch(&branch_request(parent, BranchAction::Delete))
        .expect("an empty fork keeps its parent deletable");
}

#[test]
fn branch_delete_of_layered_fork_source_stays_allowed() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    // Durable, with the parent's rows flushed to owned tables BEFORE the
    // fork: the hybrid fork child then carries durably published inherited
    // layers, its recovery never dereferences the parent, and the parent
    // stays deletable — the exact boundary of the #2820 refusal. (A fork
    // from an UNFLUSHED parent copies unsealed rows that are not WAL'd:
    // that child is layer-less and correctly blocks the delete.)
    let root = temp_dir_for_api_test("branch-layered-parent-delete");
    let backend = StorageBackend::local_fs(root);
    let parent = branch_with(0x64);
    let child = branch_with(0x65);
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            ),
        &backend,
    )
    .expect("durable open")
    .into_runtime();
    runtime
        .branch(&create_request(parent))
        .expect("create parent");
    put_at(&mut runtime, parent, b"layered-row", b"one", 10);
    runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(parent),
        ))
        .expect("enqueue flush");
    runtime.drain_maintenance().expect("drain flush");

    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: parent },
        ))
        .expect("hybrid fork of a flushed parent");
    runtime
        .branch(&branch_request(parent, BranchAction::Delete))
        .expect("layered child keeps its parent deletable");
    assert_eq!(
        read_value(&runtime, child, b"layered-row"),
        Some(b"one".to_vec()),
        "the child keeps serving inherited rows after the parent delete"
    );
}

#[test]
fn fork_parent_deletion_cannot_brick_recovery() {
    let root = temp_dir_for_api_test("branch-fork-parent-delete-recovery");
    let backend = StorageBackend::local_fs(root.clone());
    let parent = branch_with(0x62);
    let child = branch_with(0x63);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(parent))
            .expect("create parent");
        let first = put_at(&mut runtime, parent, b"lineage", b"one", 10);
        put_at(&mut runtime, parent, b"lineage", b"two", 20);
        runtime
            .branch(&branch_request(
                child,
                BranchAction::ForkAtVersion {
                    source: parent,
                    version: first.commit_version(),
                },
            ))
            .expect("historical fork");

        // The refusal is what makes the store recoverable: an accepted
        // delete here left the child's recovery re-materialization with no
        // source and permanently bricked the reopen.
        let error = runtime
            .branch(&branch_request(parent, BranchAction::Delete))
            .expect_err("deleting the historical fork's source must refuse");
        assert_eq!(error.code(), "failed_precondition.storage_api.state");
    }

    let backend = StorageBackend::local_fs(root);
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen after refused parent delete")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, child, b"lineage"),
        Some(b"one".to_vec()),
        "the historical fork serves its fork-version state after reopen"
    );
}

#[test]
fn branch_delete_unknown_rejects() {
    let runtime = open_runtime();

    let error = runtime
        .branch(&branch_request(branch_with(0x4d), BranchAction::Delete))
        .expect_err("unknown branch rejected");

    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn branch_delete_already_deleted_rejects() {
    let runtime = open_runtime();
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
    let runtime = open_runtime();
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
    let runtime = open_runtime();

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

/// #2826: after delete + re-fork of the same branch id, recovery must not
/// resurrect the dead predecessor generation's rows into the new fork. The
/// ghost commit is deliberately the LAST global commit before the fork, so
/// the fence's `<=` boundary (record version == visible-at-fork) is exactly
/// what this test exercises; the fresh post-fork commit and the inherited
/// source row pin both legal directions.
#[test]
fn fork_onto_deleted_name_does_not_resurrect_predecessor_rows_after_reopen() {
    let root = temp_dir_for_api_test("branch-fork-generation-fence");
    let backend = StorageBackend::local_fs(root.clone());
    let source = branch_with(0x71);
    let victim = branch_with(0x70);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(source))
            .expect("create source");
        put_at(&mut runtime, source, b"base", b"inherited", 10);
        runtime
            .branch(&create_request(victim))
            .expect("create victim generation 1");
        put_at(&mut runtime, victim, b"ghost", b"dead", 20);
        runtime
            .branch(&branch_request(victim, BranchAction::Delete))
            .expect("delete victim generation 1");
        runtime
            .branch(&BranchRequest::new(
                victim,
                BranchAction::ForkCurrent { source },
                Some(BranchGeneration::new(2)),
            ))
            .expect("re-fork the deleted name as generation 2");
        assert_eq!(
            read_value(&runtime, victim, b"ghost"),
            None,
            "live runtime already fences the dead generation"
        );
        put_at(&mut runtime, victim, b"fresh", b"alive", 30);
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, victim, b"ghost"),
        None,
        "recovery must not resurrect the deleted generation's row"
    );
    assert_eq!(
        read_value(&runtime, victim, b"base"),
        Some(b"inherited".to_vec()),
        "the fork keeps serving its inherited source row after reopen"
    );
    assert_eq!(
        read_value(&runtime, victim, b"fresh"),
        Some(b"alive".to_vec()),
        "the new generation's own commit survives reopen"
    );
}

/// #2826 (recreate direction): a fresh empty re-creation of a deleted name
/// must stay empty across reopen — the predecessor's WAL records belong to
/// a dead generation.
#[test]
fn recreate_after_delete_does_not_resurrect_predecessor_rows_after_reopen() {
    let root = temp_dir_for_api_test("branch-recreate-generation-fence");
    let backend = StorageBackend::local_fs(root.clone());
    let victim = branch_with(0x72);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(victim))
            .expect("create victim generation 1");
        // Two ghost commits: the earlier one sits STRICTLY below the
        // recreate point and the later one exactly AT it, so the fence's
        // `<` and `==` arms are both load-bearing here.
        put_at(&mut runtime, victim, b"ghost", b"dead", 10);
        put_at(&mut runtime, victim, b"ghost-two", b"dead-too", 15);
        runtime
            .branch(&branch_request(victim, BranchAction::Delete))
            .expect("delete victim generation 1");
        runtime
            .branch(&BranchRequest::new(
                victim,
                BranchAction::Create,
                Some(BranchGeneration::new(2)),
            ))
            .expect("recreate the deleted name as generation 2");
        assert_eq!(
            read_value(&runtime, victim, b"ghost"),
            None,
            "live runtime already fences the dead generation"
        );
        put_at(&mut runtime, victim, b"fresh", b"alive", 20);
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, victim, b"ghost"),
        None,
        "recovery must not resurrect the deleted generation's earlier row"
    );
    assert_eq!(
        read_value(&runtime, victim, b"ghost-two"),
        None,
        "recovery must not resurrect the deleted generation's boundary row"
    );
    assert_eq!(
        read_value(&runtime, victim, b"fresh"),
        Some(b"alive".to_vec()),
        "the new generation's own commit survives reopen"
    );
}

/// #2830 (checkpoint door): a checkpoint captures generation 1's rows; the
/// name is deleted and re-created; recovery must not install the dead
/// generation's checkpoint rows into the fresh branch. Two ghost commits
/// put the fence's strictly-below and at-boundary arms on the line.
#[test]
fn recreate_after_checkpoint_does_not_resurrect_predecessor_rows() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-recreate-checkpoint-fence");
    let backend = StorageBackend::local_fs(root.clone());
    let victim = branch_with(0x73);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(victim))
            .expect("create victim generation 1");
        put_at(&mut runtime, victim, b"ghost", b"dead", 10);
        put_at(&mut runtime, victim, b"ghost-two", b"dead-too", 15);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
        runtime
            .branch(&branch_request(victim, BranchAction::Delete))
            .expect("delete victim generation 1");
        runtime
            .branch(&BranchRequest::new(
                victim,
                BranchAction::Create,
                Some(BranchGeneration::new(2)),
            ))
            .expect("recreate the deleted name as generation 2");
        put_at(&mut runtime, victim, b"fresh", b"alive", 20);
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, victim, b"ghost"),
        None,
        "checkpointed dead-generation row must not resurrect"
    );
    assert_eq!(
        read_value(&runtime, victim, b"ghost-two"),
        None,
        "checkpointed dead-generation boundary row must not resurrect"
    );
    assert_eq!(
        read_value(&runtime, victim, b"fresh"),
        Some(b"alive".to_vec()),
        "the new generation's own commit survives reopen"
    );
}

/// #2830 (table-manifest door): generation 1's rows are FLUSHED to owned
/// tables (durable table manifest) before the delete; the recreate leaves
/// the stale manifest on disk; recovery must not attach the dead
/// generation's tables to the fresh branch.
#[test]
fn recreate_after_flush_does_not_resurrect_predecessor_tables() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-recreate-manifest-fence");
    let backend = StorageBackend::local_fs(root.clone());
    let victim = branch_with(0x74);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(victim))
            .expect("create victim generation 1");
        put_at(&mut runtime, victim, b"ghost", b"dead", 10);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(victim),
            ))
            .expect("enqueue flush");
        runtime.drain_maintenance().expect("drain flush");
        runtime
            .branch(&branch_request(victim, BranchAction::Delete))
            .expect("delete victim generation 1");
        runtime
            .branch(&BranchRequest::new(
                victim,
                BranchAction::Create,
                Some(BranchGeneration::new(2)),
            ))
            .expect("recreate the deleted name as generation 2");
        put_at(&mut runtime, victim, b"fresh", b"alive", 20);
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, victim, b"ghost"),
        None,
        "flushed dead-generation table row must not resurrect"
    );
    assert_eq!(
        read_value(&runtime, victim, b"fresh"),
        Some(b"alive".to_vec()),
        "the new generation's own commit survives reopen"
    );
}

/// #2830 direction control: a LEGITIMATE parentless branch (`created_at`
/// stamped, no delete/recreate anywhere) keeps its flushed tables across
/// reopen — the stale-manifest skip must not fire on a live generation.
/// Kills the `manifest_max_commit_version -> None / Some(default)` stub
/// mutants: either stub misclassifies this manifest as stale and the row
/// vanishes. The initial commit on another branch makes the victim's
/// `created_at` Some (visible > 0), which is what routes recovery through
/// the fence at all.
#[test]
fn legitimate_flushed_branch_survives_reopen_with_created_at_stamped() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-legit-manifest-not-fenced");
    let backend = StorageBackend::local_fs(root.clone());
    let keeper = branch_with(0x75);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        put_at(&mut runtime, branch(), b"warm-up", b"one", 5);
        runtime
            .branch(&create_request(keeper))
            .expect("create keeper with created_at stamped");
        put_at(&mut runtime, keeper, b"kept", b"safe", 10);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(keeper),
            ))
            .expect("enqueue flush");
        runtime.drain_maintenance().expect("drain flush");
        // Checkpoint AFTER the flush: replay starts above the row, so the
        // flushed table (via the manifest) is the row's sole recovery
        // source — a wrongly skipped manifest is a lost row, not a
        // WAL-replay-healed one.
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, keeper, b"kept"),
        Some(b"safe".to_vec()),
        "a live generation's flushed table must survive reopen"
    );
}

/// #2830: after delete + recreate + checkpoint + reopen, a timestamp from
/// the DEAD generation's era must stay unresolvable — stale timeline
/// entries seeded into the fresh branch would let fork-at-timestamp
/// resolve a version the generation never had. The fresh-era fork is the
/// in-test direction control. Kills the fence-inversion (`!` deletion)
/// mutant in the timeline seeding.
#[test]
fn recreated_branch_refuses_dead_generation_timestamps_after_reopen() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-recreate-timeline-fence");
    let backend = StorageBackend::local_fs(root.clone());
    let victim = branch_with(0x76);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(victim))
            .expect("create victim generation 1");
        put_at(&mut runtime, victim, b"ghost", b"dead", 10);
        runtime
            .branch(&branch_request(victim, BranchAction::Delete))
            .expect("delete victim generation 1");
        runtime
            .branch(&BranchRequest::new(
                victim,
                BranchAction::Create,
                Some(BranchGeneration::new(2)),
            ))
            .expect("recreate as generation 2");
        put_at(&mut runtime, victim, b"fresh", b"alive", 30);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    let error = runtime
        .branch(&branch_request(
            branch_with(0x77),
            BranchAction::ForkAtTimestamp {
                source: victim,
                timestamp: Timestamp::from_micros(10),
            },
        ))
        .expect_err("dead-generation-era timestamp must stay unresolvable");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
    let outcome = runtime
        .branch(&branch_request(
            branch_with(0x78),
            BranchAction::ForkAtTimestamp {
                source: victim,
                timestamp: Timestamp::from_micros(30),
            },
        ))
        .expect("fresh-era timestamp resolves");
    assert_eq!(
        read_value(&runtime, branch_with(0x78), b"fresh"),
        Some(b"alive".to_vec()),
        "the fresh-era fork serves the new generation's row"
    );
    let _ = outcome;
}

/// #2830 direction control: a FORK child's checkpoint-covered timeline
/// keeps its inherited-era entries across reopen — the fence must never
/// filter fork children (their inherited history sits at or below
/// `created_at` by design). Kills the `&&`->`||` mutant in the timeline
/// seeding's parentless qualification.
#[test]
fn fork_child_keeps_inherited_era_timestamps_after_reopen() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-fork-timeline-not-fenced");
    let backend = StorageBackend::local_fs(root.clone());
    let source = branch_with(0x79);
    let child = branch_with(0x7a);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        runtime
            .branch(&create_request(source))
            .expect("create source");
        put_at(&mut runtime, source, b"lineage", b"one", 10);
        put_at(&mut runtime, source, b"lineage", b"two", 30);
        runtime
            .branch(&branch_request(child, BranchAction::ForkCurrent { source }))
            .expect("fork child");
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    let outcome = runtime
        .branch(&branch_request(
            branch_with(0x7b),
            BranchAction::ForkAtTimestamp {
                source: child,
                timestamp: Timestamp::from_micros(20),
            },
        ))
        .expect("inherited-era timestamp resolves on the fork child after reopen");
    assert_eq!(
        read_value(&runtime, branch_with(0x7b), b"lineage"),
        Some(b"one".to_vec()),
        "the inherited-era fork serves the pre-fork row"
    );
    let _ = outcome;
}

/// #2847: a checkpoint records a non-seeded branch's memtable rows while the
/// branch holds no durable base (the structural guard correctly passes);
/// a LATER flush publishes that branch's table manifest, durably violating
/// the "snapshot never coexists with a non-seeded durable base" invariant
/// from the flush side, where no guard exists. Reopen must COMBINE the
/// manifest base with the checkpoint's (byte-identical) rows — pre-fix it
/// refused with `InvalidSnapshotInstall` and the store was permanently
/// unopenable.
#[test]
fn checkpoint_then_flush_of_non_seeded_branch_survives_reopen() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-ckpt-then-flush-combine");
    let backend = StorageBackend::local_fs(root.clone());
    let victim = branch_with(0x7c);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        put_at(&mut runtime, branch(), b"seeded-row", b"base", 5);
        runtime
            .branch(&create_request(victim))
            .expect("create victim");
        put_at(&mut runtime, victim, b"covered", b"both", 10);
        put_at(&mut runtime, victim, b"covered-two", b"both-too", 15);
        // Checkpoint FIRST: victim has no owned tables, so the structural
        // guard passes and the snapshot records victim's memtable rows.
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
        // Flush AFTER: victim gains a durable table-manifest base covering
        // the same rows the live snapshot already carries.
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(victim),
            ))
            .expect("enqueue flush");
        runtime.drain_maintenance().expect("drain flush");
        // A WAL-tail row above the flush: replay must still land it.
        put_at(&mut runtime, victim, b"tail", b"walled", 20);
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("reopen must combine the checkpoint with the flushed base, not refuse")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, victim, b"covered"),
        Some(b"both".to_vec()),
        "manifest-and-checkpoint row survives the combine"
    );
    assert_eq!(
        read_value(&runtime, victim, b"covered-two"),
        Some(b"both-too".to_vec()),
        "second overlapping row survives the combine"
    );
    assert_eq!(
        read_value(&runtime, victim, b"tail"),
        Some(b"walled".to_vec()),
        "the WAL-tail row above the flush replays"
    );
    assert_eq!(
        read_value(&runtime, branch(), b"seeded-row"),
        Some(b"base".to_vec()),
        "the seeded branch is untouched"
    );
    // The combined state must be fork-visible: a fresh fork of the victim
    // sees every row the combine reassembled.
    let child = branch_with(0x7e);
    runtime
        .branch(&branch_request(
            child,
            BranchAction::ForkCurrent { source: victim },
        ))
        .expect("fork the combined branch");
    assert_eq!(
        read_value(&runtime, child, b"covered"),
        Some(b"both".to_vec()),
        "fork of the combined branch inherits the manifest-covered row"
    );
    assert_eq!(
        read_value(&runtime, child, b"tail"),
        Some(b"walled".to_vec()),
        "fork of the combined branch inherits the WAL-tail row"
    );
}

/// #2847, the DST seed-52 shape: the non-seeded branch is an eager
/// (fork-at-version) child whose materialized rows live in its memtable.
/// Checkpoint captures them, a later flush makes them durable, and reopen
/// must combine rather than refuse.
#[test]
fn eager_fork_checkpoint_then_flush_survives_reopen() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("branch-fork-ckpt-then-flush-combine");
    let backend = StorageBackend::local_fs(root.clone());
    let child = branch_with(0x7d);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("durable open")
        .into_runtime();
        let first = put_at(&mut runtime, branch(), b"lineage", b"one", 10);
        put_at(&mut runtime, branch(), b"lineage", b"two", 20);
        runtime
            .branch(&branch_request(
                child,
                BranchAction::ForkAtVersion {
                    source: branch(),
                    version: first.commit_version(),
                },
            ))
            .expect("eager fork at version");
        put_at(&mut runtime, child, b"own", b"mine", 30);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Branch(child),
            ))
            .expect("enqueue checkpoint");
        runtime.drain_maintenance().expect("drain checkpoint");
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(child),
            ))
            .expect("enqueue flush");
        runtime.drain_maintenance().expect("drain flush");
    }

    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("reopen must combine the checkpoint with the flushed fork base")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, child, b"lineage"),
        Some(b"one".to_vec()),
        "the fork-materialized pre-fork row survives"
    );
    assert_eq!(
        read_value(&runtime, child, b"own"),
        Some(b"mine".to_vec()),
        "the child's own post-fork row survives"
    );
    assert_eq!(
        read_value(&runtime, branch(), b"lineage"),
        Some(b"two".to_vec()),
        "the source branch stays at its own head"
    );
}

/// #2833: re-fork a deleted name from a source that ITSELF has inherited
/// layers (the EAGER fork path). The dead generation's flushed manifest must
/// not survive as the new generation's recovery provenance — pre-fix, the
/// eager path published no child manifest, the stale gen-1 artifact remained
/// on disk, and the recovered branch silently served gen 1's lineage
/// (dropping `root-new`, inherited via the REAL parent). Both flushes are
/// load-bearing: the source flush gives gen 1's layer real tables (else the
/// recovery rebuild heals the staleness), the child flush durably publishes
/// gen 1's manifest.
#[test]
fn eager_refork_over_deleted_name_recovers_current_lineage() {
    let root = temp_dir_for_api_test("triage-eager-refork");
    let backend = StorageBackend::local_fs(root.clone());
    let mid = branch_with(0x87);
    let leaf = branch_with(0x88);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("open 1")
        .into_runtime();
        put_at(&mut runtime, branch(), b"root-old", b"old-v", 10);
        // Flush DEFAULT first: gen 1's inherited layer must reference real
        // flushed source tables (a table-less layer counts as layer-less and
        // the #2820 recovery rebuild would heal the staleness).
        runtime
            .enqueue_maintenance(&crate::api::MaintenanceRequest::new(
                crate::api::MaintenanceTask::Flush,
                crate::api::MaintenanceScope::Branch(branch()),
            ))
            .expect("enqueue default flush");
        runtime.drain_maintenance().expect("drain default flush");
        // leaf gen 1: COW fork of default (publishes a child manifest with
        // gen 1's inherited provenance).
        runtime
            .branch(&branch_request(
                leaf,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork leaf gen 1");
        put_at(&mut runtime, branch(), b"root-new", b"new-v", 20);
        // mid: fork of default AFTER root-new (mid has an inherited layer).
        runtime
            .branch(&branch_request(
                mid,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork mid");
        // Flush the gen-1 leaf: the flush publishes its table manifest WITH
        // the gen-1 inherited-layer record — the artifact that must not
        // outlive the generation.
        put_at(&mut runtime, leaf, b"leaf-ghost", b"dead", 25);
        runtime
            .enqueue_maintenance(&crate::api::MaintenanceRequest::new(
                crate::api::MaintenanceTask::Flush,
                crate::api::MaintenanceScope::Branch(leaf),
            ))
            .expect("enqueue flush");
        runtime.drain_maintenance().expect("drain flush");
        // delete leaf gen 1, re-fork the name from MID (source has inherited
        // layers -> EAGER path -> no manifest publish).
        runtime
            .branch(&branch_request(leaf, BranchAction::Delete))
            .expect("delete leaf gen 1");
        runtime
            .branch(&BranchRequest::new(
                leaf,
                BranchAction::ForkCurrent { source: mid },
                Some(BranchGeneration::new(2)),
            ))
            .expect("re-fork leaf gen 2 from mid");
        // The layer-less re-fork must have REMOVED gen 1's stale manifest
        // (and the layered mid fork must have PUBLISHED one) — both sides of
        // the boundary observed on disk.
        assert!(
            !table_manifest_path(&root, leaf).exists(),
            "the eager re-fork removes the dead generation's manifest"
        );
        assert!(
            table_manifest_path(&root, mid).exists(),
            "the layered fork keeps publishing its manifest"
        );
        put_at(&mut runtime, leaf, b"leaf-own", b"own-v", 30);
        assert_eq!(
            read_value(&runtime, leaf, b"root-new"),
            Some(b"new-v".to_vec()),
            "live: gen 2 inherits root-new via mid"
        );
    }
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, leaf, b"root-new"),
        Some(b"new-v".to_vec()),
        "recovered gen 2 must inherit root-new via mid, not gen 1's stale lineage"
    );
    assert_eq!(
        read_value(&runtime, leaf, b"leaf-own"),
        Some(b"own-v".to_vec()),
        "recovered gen 2 keeps its own row"
    );
}

/// On-disk path of a branch's table-manifest object (`tables/<branch>/manifest`
/// + the `.object@` suffix).
fn table_manifest_path(root: &std::path::Path, branch_id: BranchId) -> std::path::PathBuf {
    root.join(format!("tables/{branch_id}/manifest.object@"))
}

/// #2833 direction control: an eager fork onto a FRESH name (no predecessor,
/// nothing to remove) still recovers through the fork rebuild — the removal
/// arm is a `NotFound` no-op there.
#[test]
fn eager_fork_on_fresh_name_recovers_current_lineage() {
    use crate::api::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};

    let root = temp_dir_for_api_test("eager-fork-fresh-name");
    let backend = StorageBackend::local_fs(root.clone());
    let mid = branch_with(0x89);
    let leaf = branch_with(0x8a);
    {
        let mut runtime = StorageRuntime::open_with_backend(
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
                .with_maintenance_scheduling_policy(
                    crate::api::StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
            &backend,
        )
        .expect("open")
        .into_runtime();
        put_at(&mut runtime, branch(), b"root-row", b"root-v", 10);
        runtime
            .enqueue_maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Flush,
                MaintenanceScope::Branch(branch()),
            ))
            .expect("enqueue default flush");
        runtime.drain_maintenance().expect("drain default flush");
        runtime
            .branch(&branch_request(
                mid,
                BranchAction::ForkCurrent { source: branch() },
            ))
            .expect("fork mid (layered)");
        runtime
            .branch(&branch_request(
                leaf,
                BranchAction::ForkCurrent { source: mid },
            ))
            .expect("eager fork leaf from mid on a fresh name");
        put_at(&mut runtime, leaf, b"leaf-own", b"own-v", 20);
    }
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("reopen")
    .into_runtime();
    assert_eq!(
        read_value(&runtime, leaf, b"root-row"),
        Some(b"root-v".to_vec()),
        "fresh-name eager fork inherits through the chain after reopen"
    );
    assert_eq!(
        read_value(&runtime, leaf, b"leaf-own"),
        Some(b"own-v".to_vec()),
        "fresh-name eager fork keeps its own row"
    );
}
