use super::*;
use crate::backend::memory::MemoryBackend;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchState, CommitDurabilityMode, CommitExpiry,
    CommitMutation, CommitOrigin, CommitRetentionHint, CommitRuntimeConfig, CommitValidationFacts,
};
use crate::lifecycle::tests::checkpoint::shared::{
    durable_batch, generation_guard, open_runtime as open_durable_runtime,
    physical_key as durable_physical_key, CheckpointTestBackend,
};

#[test]
fn branch_catalog_create_empty_branch() {
    let branch = branch_id(1);
    let generation = generation(1);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");

    let outcome = catalog
        .create_branch(branch, generation, Some(CommitVersion::new(1)))
        .expect("create branch");

    assert_eq!(outcome.branch_count(), 1);
    assert_eq!(outcome.descriptor().branch_id(), branch);
    assert_eq!(outcome.descriptor().generation(), generation);
    assert_eq!(outcome.descriptor().status(), LifecycleBranchStatus::Active);
    assert_eq!(outcome.descriptor().state_revision(), 0);
    assert_eq!(
        outcome.descriptor().created_at(),
        Some(CommitVersion::new(1))
    );
    assert!(catalog.branch_state(branch).expect("state").is_empty());
    assert_eq!(
        catalog
            .registry()
            .lookup(branch)
            .expect("registry descriptor")
            .generation(),
        generation
    );
}

#[test]
fn branch_catalog_duplicate_create_rejects() {
    let branch = branch_id(2);
    let mut catalog = catalog_with_branch(branch, generation(1));

    assert_eq!(
        catalog.create_branch(branch, generation(2), None),
        Err(LifecycleError::BranchAlreadyExists { branch_id: branch })
    );
}

#[test]
fn branch_catalog_create_rejects_zero_generation() {
    assert!(CommitBranchGeneration::new(0).is_err());
}

#[test]
fn branch_catalog_list_active_branches_in_deterministic_order() {
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    let high = branch_id(9);
    let low = branch_id(3);
    catalog
        .create_branch(high, generation(1), None)
        .expect("create high");
    catalog
        .create_branch(low, generation(1), None)
        .expect("create low");

    let listed = catalog
        .list_branches(false)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();

    assert_eq!(listed, vec![low, high]);
}

#[test]
fn branch_catalog_list_includes_deleted_when_requested() {
    let active = branch_id(20);
    let deleted = branch_id(21);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(deleted, generation(1), None)
        .expect("create deleted");
    catalog
        .create_branch(active, generation(1), None)
        .expect("create active");
    catalog
        .delete_branch(
            deleted,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(1)),
        )
        .expect("delete");

    let active_only = catalog
        .list_branches(false)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();
    let all = catalog
        .list_branches(true)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();

    assert_eq!(active_only, vec![active]);
    assert_eq!(all, vec![active, deleted]);
}

#[test]
fn branch_catalog_commit_registry_stays_coherent() {
    let branch = branch_id(77);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch, generation(2), Some(CommitVersion::new(1)))
        .expect("create");

    let active = catalog.registry().lookup(branch).expect("active registry");
    assert_eq!(active.generation(), generation(2));
    assert_eq!(active.state(), CommitBranchState::Active);

    catalog
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(2)),
            Some(CommitVersion::new(3)),
        )
        .expect("delete");
    let deleted = catalog.registry().lookup(branch).expect("deleted registry");
    assert_eq!(deleted.generation(), generation(2));
    assert_eq!(deleted.state(), CommitBranchState::Deleted);

    catalog
        .create_branch(branch, generation(3), Some(CommitVersion::new(4)))
        .expect("recreate");
    let recreated = catalog
        .registry()
        .lookup(branch)
        .expect("recreated registry");
    assert_eq!(recreated.generation(), generation(3));
    assert_eq!(recreated.state(), CommitBranchState::Active);
}

#[test]
fn branch_catalog_missing_lookup_rejects() {
    let missing = branch_id(22);
    let catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");

    assert_eq!(
        catalog.lookup(missing),
        Err(LifecycleError::BranchNotFound { branch_id: missing })
    );
}

#[test]
fn branch_catalog_descriptor_branch_mismatch_rejects() {
    let branch = branch_id(23);
    let other = branch_id(24);
    let mut catalog = catalog_with_branch(branch, generation(1));
    let wrong_state = BranchLocalState::new(other, BranchRuntimeConfig::default()).expect("branch");

    assert_eq!(
        catalog.replace_active_branch_state(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)),
            wrong_state,
        ),
        Err(LifecycleError::BranchStateMismatch {
            expected: branch,
            actual: other,
        })
    );
}

#[test]
fn branch_catalog_create_does_not_publish_table_objects() {
    let branch = branch_id(25);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch, generation(1), None)
        .expect("create");

    let snapshot = catalog
        .branch_state(branch)
        .expect("branch")
        .reachability_snapshot()
        .expect("reachability");
    assert!(snapshot.table_refs().is_empty());
}

#[test]
fn branch_catalog_cache_create_reports_no_durable_claim() {
    let branch = branch_id(53);
    let backend: &'static MemoryBackend = Box::leak(Box::new(MemoryBackend::new()));
    let runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("plan"),
            branch,
            generation(1),
        )
        .expect("request"),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        crate::commit::CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("open");

    assert_eq!(
        runtime
            .branch_catalog()
            .lookup(branch)
            .expect("descriptor")
            .status(),
        LifecycleBranchStatus::Active
    );
    assert!(runtime.open_outcome().checkpoint().is_none());
}

#[test]
fn branch_catalog_runtime_syncs_after_cache_commit() {
    let branch = branch_id(54);
    let backend: &'static MemoryBackend = Box::leak(Box::new(MemoryBackend::new()));
    let key = physical_key(branch, b"runtime-sync");
    let mut runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("plan"),
            branch,
            generation(1),
        )
        .expect("request"),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        crate::commit::CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("open");

    runtime
        .execute_cache_commit(
            cache_put_batch(branch, key.clone()),
            CommitBranchGenerationGuard::exact(generation(1)),
        )
        .expect("commit");

    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch)
            .expect("catalog state")
            .capture_read_view()
            .expect("view")
            .latest(&key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"runtime"
    );
}

#[test]
fn branch_catalog_runtime_syncs_after_durable_commit() {
    let branch = branch_id(78);
    let backend: &'static CheckpointTestBackend = Box::leak(Box::new(CheckpointTestBackend::new()));
    let key = durable_physical_key(branch, b"durable-runtime-sync");
    let mut runtime = open_durable_runtime(branch, backend);

    runtime
        .execute_durable_commit(
            durable_batch(branch, b"durable-runtime-sync", b"durable"),
            generation_guard(),
        )
        .expect("commit");

    assert_eq!(
        runtime
            .branch_catalog()
            .branch_state(branch)
            .expect("catalog state")
            .capture_read_view()
            .expect("view")
            .latest(&key)
            .expect("read")
            .expect("row")
            .row()
            .value(),
        b"durable"
    );
}

#[test]
fn cache_branch_lifecycle_after_close_rejects() {
    let branch = branch_id(79);
    let backend: &'static MemoryBackend = Box::leak(Box::new(MemoryBackend::new()));
    let key = physical_key(branch, b"closed-cache");
    let mut runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("plan"),
            branch,
            generation(1),
        )
        .expect("request"),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        crate::commit::CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("open");

    runtime.close().expect("close");

    assert!(matches!(
        runtime.execute_cache_commit(
            cache_put_batch(branch, key),
            CommitBranchGenerationGuard::exact(generation(1)),
        ),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

#[test]
fn cache_branch_lifecycle_while_closing_rejects() {
    let branch = branch_id(80);
    let backend: &'static MemoryBackend = Box::leak(Box::new(MemoryBackend::new()));
    let key = physical_key(branch, b"closing-cache");
    let mut runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("plan"),
            branch,
            generation(1),
        )
        .expect("request"),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        crate::commit::CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("open");

    runtime
        .force_close_requested_for_test()
        .expect("closing state");

    assert!(matches!(
        runtime.execute_cache_commit(
            cache_put_batch(branch, key),
            CommitBranchGenerationGuard::exact(generation(1)),
        ),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

fn open_cache_runtime(initial: BranchId) -> (&'static MemoryBackend, LifecycleCacheRuntime) {
    let backend: &'static MemoryBackend = Box::leak(Box::new(MemoryBackend::new()));
    let runtime = LifecycleCacheRuntime::open(
        LifecycleCacheOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::Cache,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("plan"),
            initial,
            generation(1),
        )
        .expect("request"),
        backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        crate::commit::CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("open");
    (backend, runtime)
}

#[test]
fn cache_runtime_create_branch_appears_in_list_branches() {
    let initial = branch_id(130);
    let new = branch_id(131);
    let (_backend, mut runtime) = open_cache_runtime(initial);

    runtime
        .create_branch(new, generation(1), Some(CommitVersion::new(2)))
        .expect("create new branch");

    let listed = runtime
        .list_branches(false)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();
    assert!(listed.contains(&initial));
    assert!(listed.contains(&new));
    assert_eq!(listed.len(), 2);
}

#[test]
fn cache_runtime_fork_current_with_unflushed_source_rejects() {
    // The runtime's fork_current delegates to the catalog. Verify that a
    // commit to the source (which lands in the active table) causes the
    // runtime fork to surface the typed `SourceHasUnflushedRows` error,
    // proving the delegation path is wired and the typed error surfaces.
    let source = branch_id(132);
    let child = branch_id(133);
    let (_backend, mut runtime) = open_cache_runtime(source);

    let key = physical_key(source, b"runtime-fork");
    runtime
        .execute_cache_commit(
            CommitBatch::mutating(
                source,
                vec![CommitMutation::put(
                    key,
                    b"seed".to_vec(),
                    CommitExpiry::None,
                    CommitRetentionHint::Append,
                )],
                CommitValidationFacts::empty(),
                CommitBatchOptions::new(
                    CommitDurabilityMode::Cache,
                    crate::commit::CommitConflictValidationMode::Skip,
                    crate::commit::CommitDuplicateKeyPolicy::Reject,
                    crate::commit::CommitTimestampPolicy::Explicit(Timestamp::from_micros(2_000)),
                    CommitOrigin::StorageRuntime,
                ),
            ),
            CommitBranchGenerationGuard::exact(generation(1)),
        )
        .expect("commit");

    assert!(matches!(
        runtime.fork_current(source, child, generation(1)),
        Err(LifecycleError::SourceHasUnflushedRows { .. })
    ));
}

#[test]
fn cache_runtime_create_branch_after_close_rejects() {
    let initial = branch_id(134);
    let new = branch_id(135);
    let (_backend, mut runtime) = open_cache_runtime(initial);
    runtime.close().expect("close");

    assert!(matches!(
        runtime.create_branch(new, generation(1), None),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

#[test]
fn cache_runtime_create_branch_while_closing_rejects() {
    let initial = branch_id(136);
    let new = branch_id(137);
    let (_backend, mut runtime) = open_cache_runtime(initial);
    runtime
        .force_close_requested_for_test()
        .expect("closing state");

    assert!(matches!(
        runtime.create_branch(new, generation(1), None),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

#[test]
fn durable_runtime_create_branch_appears_in_list_branches() {
    let initial = branch_id(138);
    let new = branch_id(139);
    let backend: &'static CheckpointTestBackend = Box::leak(Box::new(CheckpointTestBackend::new()));
    let mut runtime = open_durable_runtime(initial, backend);

    runtime
        .create_branch(new, generation(1), Some(CommitVersion::new(3)))
        .expect("create new branch");

    let listed = runtime
        .list_branches(false)
        .into_iter()
        .map(LifecycleBranchDescriptor::branch_id)
        .collect::<Vec<_>>();
    assert!(listed.contains(&initial));
    assert!(listed.contains(&new));
}

#[test]
fn durable_runtime_fork_at_retained_timestamp_resolves_via_coverage() {
    let source = branch_id(140);
    let child = branch_id(141);
    let backend: &'static CheckpointTestBackend = Box::leak(Box::new(CheckpointTestBackend::new()));
    let mut runtime = open_durable_runtime(source, backend);
    // The seeded source has BranchTimestampCoverage::Unknown by default;
    // a timestamp fork must reject with the typed error.
    let result = runtime.fork_at_retained_timestamp(
        source,
        child,
        generation(1),
        Timestamp::from_micros(500),
        CommitVersion::new(1),
    );
    assert!(matches!(
        result,
        Err(LifecycleError::InsufficientTimestampHistory { .. })
    ));
}

#[test]
fn cache_runtime_clear_branch_appears_in_list_branches() {
    let branch = branch_id(160);
    let (_backend, mut runtime) = open_cache_runtime(branch);
    // Clear is non-destructive of the descriptor — branch stays Active in
    // the catalog after clear.
    let outcome = runtime
        .clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("clear branch via runtime");

    assert_eq!(outcome.descriptor().branch_id(), branch);
    assert!(runtime
        .list_branches(false)
        .iter()
        .any(|d| d.branch_id() == branch));
}

#[test]
fn cache_runtime_clear_branch_admission_rejects_after_close() {
    let branch = branch_id(161);
    let (_backend, mut runtime) = open_cache_runtime(branch);
    runtime.close().expect("close");

    assert!(matches!(
        runtime.clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1))),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

#[test]
fn cache_runtime_delete_branch_after_clear_succeeds() {
    let branch = branch_id(162);
    let (_backend, mut runtime) = open_cache_runtime(branch);
    runtime
        .clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("clear branch");

    let outcome = runtime
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(2)),
        )
        .expect("delete branch via runtime");

    assert_eq!(outcome.descriptor().branch_id(), branch);
    assert_eq!(
        outcome.descriptor().status(),
        LifecycleBranchStatus::Deleted
    );
}

#[test]
fn cache_runtime_delete_branch_admission_rejects_after_close() {
    let branch = branch_id(163);
    let (_backend, mut runtime) = open_cache_runtime(branch);
    runtime.close().expect("close");

    assert!(matches!(
        runtime.delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)),
            None
        ),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
}

#[test]
fn durable_runtime_clear_branch_buffers_release_plan() {
    let branch = branch_id(164);
    let backend: &'static CheckpointTestBackend = Box::leak(Box::new(CheckpointTestBackend::new()));
    let mut runtime = open_durable_runtime(branch, backend);
    assert!(runtime.pending_releases().is_empty());

    runtime
        .clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("clear");

    assert_eq!(runtime.pending_releases().len(), 1);
    assert_eq!(runtime.pending_releases()[0].released_branch_id(), branch);
}

#[test]
fn durable_runtime_delete_branch_buffers_release_plan() {
    let branch = branch_id(165);
    let backend: &'static CheckpointTestBackend = Box::leak(Box::new(CheckpointTestBackend::new()));
    let mut runtime = open_durable_runtime(branch, backend);
    assert!(runtime.pending_releases().is_empty());

    runtime
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)),
            Some(CommitVersion::new(2)),
        )
        .expect("delete");

    assert_eq!(runtime.pending_releases().len(), 1);
    assert_eq!(runtime.pending_releases()[0].released_branch_id(), branch);
}

fn cache_put_batch(branch: BranchId, key: PhysicalKey) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            b"runtime".to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Skip,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            crate::commit::CommitTimestampPolicy::Explicit(Timestamp::from_micros(1_000)),
            CommitOrigin::StorageRuntime,
        ),
    )
}

#[test]
fn branch_state_checkout_round_trips_and_fails_closed_while_out() {
    // BS5.4: the write group's parallel apply checks branch states OUT of the
    // catalog by ownership transfer. While a state is out, every accessor
    // fails closed (the leader holds the runtime mutex for the whole window,
    // so this is a same-thread-bug guard, not a concurrency surface).
    let branch = branch_id(0xa0);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch, generation(1), None)
        .expect("create branch");

    let state = catalog
        .take_branch_state(branch, CommitBranchGenerationGuard::exact(generation(1)))
        .expect("checkout");
    assert_eq!(state.branch_id(), branch);

    // Every access while checked out fails closed.
    assert!(matches!(
        catalog.branch_state(branch),
        Err(LifecycleError::BranchNotWritable { .. })
    ));
    assert!(matches!(
        catalog.branch_state_mut(branch, CommitBranchGenerationGuard::exact(generation(1))),
        Err(LifecycleError::BranchNotWritable { .. })
    ));
    // Double checkout fails closed.
    assert!(matches!(
        catalog.take_branch_state(branch, CommitBranchGenerationGuard::exact(generation(1))),
        Err(LifecycleError::BranchNotWritable { .. })
    ));

    catalog.check_in_branch_state(state).expect("check in");
    assert!(catalog.branch_state(branch).is_ok());
}

#[test]
fn branch_state_check_in_rejects_states_that_were_not_checked_out() {
    let branch = branch_id(0xa1);
    let mut catalog = LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).expect("catalog");
    catalog
        .create_branch(branch, generation(1), None)
        .expect("create branch");

    // A state that was never checked out (a stray clone) must be refused —
    // restoring it would silently discard the catalog's live slot semantics.
    let stray = catalog.branch_state(branch).expect("read state").clone();
    assert!(matches!(
        catalog.check_in_branch_state(stray),
        Err(LifecycleError::InvalidLifecycleState { .. })
    ));
    // The live slot is untouched.
    assert!(catalog.branch_state(branch).is_ok());
}
