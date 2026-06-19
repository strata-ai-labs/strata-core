use super::*;
use crate::backend::memory::MemoryBackend;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitRuntimeError, CommitTimestampPolicy,
    CommitValidationFacts,
};
use crate::row::{PhysicalKey, StorageSpaceId};
use crate::service::TableManifestService;
use std::error::Error;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[test]
fn storage_budget_accepts_explicit_low_memory_profile() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();

    budget.validate().expect("low-memory budget");
    assert_eq!(
        budget.pool_limit_bytes(StorageBudgetPool::BlockCache),
        0,
        "low-memory profile preserves explicit zero cache"
    );
    assert!(budget.total_bytes() < 128 * 1024);
    assert!(!budget.table_cache_config().expect("cache config").enabled());
}

#[test]
fn storage_budget_accepts_zero_optional_block_cache() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();

    assert_eq!(budget.pool_limit_bytes(StorageBudgetPool::BlockCache), 0);
    assert!(!budget.table_cache_config().expect("cache config").enabled());
}

#[test]
fn storage_budget_builds_optional_shared_table_block_cache() {
    let disabled_cache =
        table_block_cache_from_storage_budget(StorageRuntimeBudget::low_memory_test_profile())
            .expect("disabled table block cache config");
    assert!(disabled_cache.is_none());

    let budget = StorageRuntimeBudget::default();
    let enabled_cache = table_block_cache_from_storage_budget(budget)
        .expect("enabled table block cache config")
        .expect("default budget enables table block cache");
    assert!(enabled_cache.enabled());
    assert_eq!(
        enabled_cache.stats().capacity_bytes(),
        usize::try_from(budget.pool_limit_bytes(StorageBudgetPool::BlockCache))
            .expect("block cache budget fits usize")
    );
}

#[test]
fn database_total_sums_ledger_reservations_and_runtime_contribution() {
    let ledger = crate::lifecycle::StorageBudgetLedger::new(
        StorageRuntimeBudget::scaled_closed_loop_test_profile(),
    )
    .expect("ledger");
    assert_eq!(ledger.total_used_bytes(), 0);

    // An in-flight operation reservation charges the ledger and shows in the global total.
    let reservation = ledger
        .reserve(StorageBudgetPool::TableReader, 1_000, 1, "reader")
        .expect("reserve reader");
    assert_eq!(ledger.total_used_bytes(), 1_000);

    // The database-wide runtime contribution (memtables + resident readers + cache) adds on top.
    ledger.set_runtime_total_bytes(5_000);
    assert_eq!(ledger.total_used_bytes(), 6_000);

    // Releasing the reservation drops its bytes; the runtime contribution remains.
    drop(reservation);
    assert_eq!(ledger.total_used_bytes(), 5_000);
}

#[test]
fn global_pressure_escalates_as_the_database_total_approaches_budget() {
    use crate::lifecycle::StorageBudgetPressureSeverity;
    let budget = StorageRuntimeBudget::scaled_closed_loop_test_profile();
    let total = budget.total_bytes();
    let ledger = crate::lifecycle::StorageBudgetLedger::new(budget).expect("ledger");

    ledger.set_runtime_total_bytes(total / 2);
    assert_eq!(
        ledger.global_pressure(),
        StorageBudgetPressureSeverity::Normal
    );

    // At or above the 80% high-water mark, optional work defers so maintenance can reclaim.
    ledger.set_runtime_total_bytes(total.saturating_mul(4) / 5);
    assert_eq!(
        ledger.global_pressure(),
        StorageBudgetPressureSeverity::DeferOptionalMaintenance
    );

    // Over budget, mutating admission is rejected.
    ledger.set_runtime_total_bytes(total + 1);
    assert_eq!(
        ledger.global_pressure(),
        StorageBudgetPressureSeverity::RejectMutatingAdmission
    );
}

#[test]
fn would_exceed_total_projects_the_requested_increment() {
    let budget = StorageRuntimeBudget::scaled_closed_loop_test_profile();
    let total = budget.total_bytes();
    let ledger = crate::lifecycle::StorageBudgetLedger::new(budget).expect("ledger");

    ledger.set_runtime_total_bytes(total);
    assert!(
        !ledger.would_exceed_total(0),
        "exactly at budget is allowed"
    );
    assert!(
        ledger.would_exceed_total(1),
        "one byte over budget is refused"
    );
}

#[test]
fn from_total_bytes_at_default_total_matches_default() {
    let default = StorageRuntimeBudget::default();
    let derived = StorageRuntimeBudget::from_total_bytes(default.total_bytes()).expect("derive");
    assert_eq!(
        derived, default,
        "deriving from the default total reproduces the default split"
    );
}

#[test]
fn from_total_bytes_scales_pools_proportionally() {
    let default = StorageRuntimeBudget::default();
    let quarter = default.total_bytes() / 4;
    let derived = StorageRuntimeBudget::from_total_bytes(quarter).expect("derive");

    assert_eq!(derived.total_bytes(), quarter);
    assert_eq!(
        derived.pool_limit_bytes(StorageBudgetPool::BlockCache),
        default.pool_limit_bytes(StorageBudgetPool::BlockCache) / 4,
        "pools scale proportionally with the total"
    );
    // from_parts validated the sum-within-total invariant; confirm every pool stayed nonzero.
    for pool in StorageBudgetPool::ALL {
        assert!(
            derived.pool_limit_bytes(pool) > 0,
            "derived pool {pool:?} must be nonzero"
        );
    }
}

#[test]
fn storage_budget_rejects_zero_mandatory_active_pool() {
    let parts = StorageRuntimeBudgetParts {
        active_mutable_bytes: 0,
        ..StorageRuntimeBudgetParts::default()
    };

    assert_eq!(
        StorageRuntimeBudget::from_parts(parts),
        Err(LifecycleError::InvalidConfig {
            field: "storage_budget.active_mutable_bytes",
            reason: "must be nonzero",
        })
    );
}

#[test]
fn storage_budget_rejects_total_smaller_than_required_pools() {
    let parts = StorageRuntimeBudgetParts {
        total_bytes: 1,
        ..StorageRuntimeBudgetParts::default()
    };

    assert_eq!(
        StorageRuntimeBudget::from_parts(parts),
        Err(LifecycleError::InvalidConfig {
            field: "storage_budget.total_bytes",
            reason: "pool byte sum exceeds total",
        })
    );
}

#[test]
fn storage_budget_rejects_overflowing_pool_sum() {
    let parts = StorageRuntimeBudgetParts {
        total_bytes: u64::MAX,
        block_cache_bytes: u64::MAX,
        ..StorageRuntimeBudgetParts::default()
    };

    assert_eq!(
        StorageRuntimeBudget::from_parts(parts),
        Err(LifecycleError::InvalidConfig {
            field: "storage_budget.total_bytes",
            reason: "pool byte sum overflowed",
        })
    );
}

#[test]
fn storage_budget_reports_all_pool_limits() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();

    for pool in StorageBudgetPool::ALL {
        let usage = StorageBudgetUsage::new(budget, pool, 0, 0);
        assert_eq!(usage.pool(), pool);
        assert_eq!(usage.limit_bytes(), budget.pool_limit_bytes(pool));
        assert_eq!(usage.limit_count(), budget.pool_limit_count(pool));
    }
}

#[test]
fn budget_reservation_acquire_and_release() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    {
        let reservation = ledger
            .reserve(StorageBudgetPool::TableReader, 512, 1, "reader open")
            .expect("reserve");
        assert_eq!(reservation.pool(), StorageBudgetPool::TableReader);
        assert_eq!(reservation.bytes(), 512);
        assert_eq!(
            ledger
                .snapshot()
                .usage(StorageBudgetPool::TableReader)
                .used_bytes(),
            512
        );
    }

    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        0
    );
}

#[test]
fn budget_reservation_exact_fit_succeeds() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let limit = ledger
        .budget()
        .pool_limit_bytes(StorageBudgetPool::GeneratedArtifact);

    let _reservation = ledger
        .reserve(
            StorageBudgetPool::GeneratedArtifact,
            limit,
            0,
            "generated artifact",
        )
        .expect("exact-fit generated artifact reserve");

    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::GeneratedArtifact)
            .used_bytes(),
        limit
    );
}

#[test]
fn budget_reservation_explicit_release_clears_usage() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    let reservation = ledger
        .reserve(StorageBudgetPool::TableReader, 512, 1, "reader open")
        .expect("reserve");
    assert_eq!(reservation.count(), 1);
    reservation.release();

    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 0);
    assert_eq!(usage.used_count(), 0);
}

#[test]
fn budget_reservation_nested_failure_releases_outer() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    {
        let _outer = ledger
            .reserve(
                StorageBudgetPool::ManifestCatalog,
                1024,
                0,
                "manifest decode",
            )
            .expect("outer reserve");
        let error = ledger
            .reserve(
                StorageBudgetPool::ManifestCatalog,
                ledger
                    .budget()
                    .pool_limit_bytes(StorageBudgetPool::ManifestCatalog),
                0,
                "manifest decode",
            )
            .expect_err("nested over-budget reserve");
        assert_budget_error(&error, StorageBudgetPool::ManifestCatalog);
    }

    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::ManifestCatalog)
            .used_bytes(),
        0
    );
}

#[test]
fn budget_ledger_is_database_local() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let left = StorageBudgetLedger::new(budget).expect("left ledger");
    let right = StorageBudgetLedger::new(budget).expect("right ledger");

    let _left_reader = left
        .reserve(StorageBudgetPool::TableReader, 512, 1, "left reader")
        .expect("left reserve");

    assert_eq!(
        left.snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        512
    );
    assert_eq!(
        right
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        0
    );
}

#[test]
fn budget_reservation_rejects_one_byte_over_limit_without_usage_change() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    let error = ledger
        .reserve(
            StorageBudgetPool::TableReader,
            ledger
                .budget()
                .pool_limit_bytes(StorageBudgetPool::TableReader)
                + 1,
            1,
            "reader open",
        )
        .expect_err("over budget");

    assert_budget_error(&error, StorageBudgetPool::TableReader);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        0
    );
}

#[test]
fn budget_pressure_reports_pool_usage_and_limit() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let ledger = StorageBudgetLedger::new(budget).expect("ledger");

    ledger
        .check_available(StorageBudgetPool::MaintenanceQueue, 256, 1, "queue task")
        .expect("exact available check");
    let _reservation = ledger
        .reserve(StorageBudgetPool::MaintenanceQueue, 900, 1, "queue task")
        .expect("reserve high-water queue");
    let snapshot = ledger.snapshot();
    let usage = snapshot.usage(StorageBudgetPool::MaintenanceQueue);

    assert_eq!(snapshot.usages().len(), StorageBudgetPool::ALL.len());
    assert_eq!(usage.pool(), StorageBudgetPool::MaintenanceQueue);
    assert_eq!(usage.used_bytes(), 900);
    assert_eq!(usage.limit_bytes(), 1024);
    assert_eq!(usage.used_count(), 1);
    assert_eq!(usage.limit_count(), Some(4));
    assert_eq!(
        snapshot.pressure(StorageBudgetPool::MaintenanceQueue),
        StorageBudgetPressureSeverity::DeferOptionalMaintenance
    );
    assert_eq!(budget.max_frozen_tables(), 4);
    assert_eq!(budget.max_pending_maintenance_tasks(), 4);
}

#[test]
fn cache_open_reports_selected_storage_budget() {
    let branch = branch_id(0x20);
    let runtime = open_cache_runtime(branch, storage_budget_with_active(4096));
    let budget = runtime
        .open_outcome()
        .budget()
        .expect("open outcome budget");

    assert_eq!(
        budget
            .budget()
            .pool_limit_bytes(StorageBudgetPool::ActiveMutable),
        4096
    );
    assert_eq!(
        budget.usage(StorageBudgetPool::ActiveMutable).used_bytes(),
        0
    );
    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert_eq!(
        runtime.capability_outcome().storage_mode(),
        StorageMode::Cache
    );
}

#[test]
fn active_append_under_budget_succeeds() {
    let branch = branch_id(0x26);
    let mut runtime = open_cache_runtime(branch, storage_budget_with_active(16 * 1024));
    let key = physical_key(branch, b"active-under-budget");

    let outcome = runtime
        .execute_cache_commit(
            put_batch(branch, key.clone(), b"value".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit under budget");

    assert_eq!(outcome.commit_version(), Some(CommitVersion::new(1)));
    assert!(runtime
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("latest")
        .is_some());
}

#[test]
fn commit_semantic_validation_precedes_active_budget_rejection() {
    let branch = branch_id(0x31);
    let other_branch = branch_id(0x32);
    let mut runtime = open_cache_runtime(branch, storage_budget_with_active(128));
    let batch = put_batch(
        other_branch,
        physical_key(other_branch, b"wrong-branch-large"),
        vec![0x99; 1024],
    );

    let error = runtime
        .execute_cache_commit(batch, CommitBranchGenerationGuard::not_supplied())
        .expect_err("branch mismatch must win before budget");

    // Either BranchMismatch (from commit runtime admission) or BranchNotFound
    // (from catalog registry lookup) is a valid pre-budget rejection. The
    // commit path consults the catalog before charging budget, so a wrong-
    // branch batch can surface either typed variant. Both prove the same
    // invariant: semantic validation precedes budget enforcement.
    assert_commit_runtime_error(&error, |commit_error| {
        matches!(
            commit_error,
            CommitRuntimeError::BranchMismatch { .. } | CommitRuntimeError::BranchNotFound { .. }
        )
    });
    assert_eq!(runtime.visible_version(), CommitVersion::ZERO);
    assert!(runtime.branch_state().is_empty());
}

#[test]
fn active_budget_reports_approximate_bytes_after_commit() {
    let branch = branch_id(0x22);
    let mut runtime = open_cache_runtime(branch, storage_budget_with_active(16 * 1024));
    let key = physical_key(branch, b"small-active");
    runtime
        .execute_cache_commit(
            put_batch(branch, key, b"value".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");

    let usage = runtime
        .budget_snapshot()
        .usage(StorageBudgetPool::ActiveMutable);
    assert!(usage.used_bytes() > 0);
    assert_eq!(usage.limit_bytes(), 16 * 1024);
}

#[test]
fn rotate_active_over_frozen_count_budget_rejects_before_state_change() {
    let branch = branch_id(0x27);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.max_frozen_tables = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"first-freeze"),
                b"v1".to_vec(),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("first commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("first rotate");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"second-freeze"),
                b"v2".to_vec(),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("second commit");

    let error = runtime
        .rotate_active_for_maintenance()
        .expect_err("frozen table count rejects");

    assert_budget_error(&error, StorageBudgetPool::FrozenMutable);
    assert_eq!(runtime.branch_state().frozen_table_count(), 1);
    assert!(runtime.branch_state().active_row_count() > 0);
}

#[test]
fn rotate_active_over_frozen_byte_budget_rejects_before_state_change() {
    let branch = branch_id(0x23);
    let mut budget = budget_parts_with_active(16 * 1024);
    budget.frozen_mutable_bytes = 64;
    budget.total_bytes = pool_sum(budget);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(budget).expect("budget"),
    );
    runtime
        .execute_cache_commit(
            put_batch(branch, physical_key(branch, b"freeze-me"), vec![0x66; 512]),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");

    let error = runtime
        .rotate_active_for_maintenance()
        .expect_err("frozen budget rejects");

    assert_budget_error(&error, StorageBudgetPool::FrozenMutable);
    assert_eq!(runtime.branch_state().frozen_table_count(), 0);
    assert!(runtime.branch_state().active_row_count() > 0);
}

#[test]
fn maintenance_queue_count_limit_rejects_extra_task() {
    let branch = branch_id(0x24);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("first enqueue");
    let error = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect_err("second task rejects");

    assert_budget_error(&error, StorageBudgetPool::MaintenanceQueue);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn maintenance_coalescing_happens_before_budget_reservation() {
    let branch = branch_id(0x25);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("first enqueue");
    let outcome = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("coalesced enqueue");

    assert!(outcome.was_coalesced());
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn maintenance_state_admission_precedes_queue_budget_rejection() {
    let branch = branch_id(0x33);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("fill queue");
    runtime
        .force_close_requested_for_test()
        .expect("move to closing");

    let error = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::flush(branch))
        .expect_err("lifecycle state rejects before budget");

    assert!(
        matches!(error, LifecycleError::InvalidLifecycleState { .. }),
        "expected lifecycle state rejection, got {error:?}"
    );
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn cache_flush_generated_artifact_budget_rejects_before_install() {
    let branch = branch_id(0x34);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.generated_artifact_bytes = 1;
    parts.table_reader_bytes = 16 * 1024;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"flush-generated"),
                vec![0x44; 512],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("freeze active rows");

    let error = runtime
        .flush_frozen(&flush_request(branch, "generated"))
        .expect_err("generated artifact budget rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert_eq!(runtime.branch_state().frozen_table_count(), 1);
    assert_eq!(runtime.branch_state().owned_table_count(), 0);
}

#[test]
fn cache_flush_table_reader_budget_rejects_before_install() {
    let branch = branch_id(0x35);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.generated_artifact_bytes = 16 * 1024;
    parts.table_reader_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"flush-reader"),
                vec![0x55; 512],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("freeze active rows");

    let error = runtime
        .flush_frozen(&flush_request(branch, "reader"))
        .expect_err("reader budget rejects");

    assert_budget_error(&error, StorageBudgetPool::TableReader);
    assert_eq!(runtime.branch_state().frozen_table_count(), 1);
    assert_eq!(runtime.branch_state().owned_table_count(), 0);
}

#[test]
fn table_manifest_publication_checks_manifest_catalog_budget_before_publish() {
    let branch = branch_id(0x36);
    let backend = MemoryBackend::new();
    let table_manifest = TableManifestService::new(&backend);
    let mut catalog = LifecycleDurableTableCatalog::new();
    let state = BranchLocalState::empty(branch);
    let mut parts = budget_parts_with_active(16 * 1024);
    parts.manifest_catalog_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = publish_table_manifest_for_branch_with_budget(
        &state,
        &table_manifest,
        &mut catalog,
        Some(&ledger),
    )
    .expect_err("manifest catalog budget rejects");

    assert_budget_error(&error, StorageBudgetPool::ManifestCatalog);
    assert!(
        table_manifest
            .load_current(branch)
            .expect("load manifest")
            .is_none(),
        "budget rejection must happen before manifest publication"
    );
}

fn open_cache_runtime(
    branch: BranchId,
    budget: StorageRuntimeBudget,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    let backend = MemoryBackend::new();
    let config = LifecycleConfig::default()
        .with_storage_budget(budget)
        .expect("storage budget config");
    let plan = StorageOpenPlan::new(
        StorageMode::Cache,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        config,
    )
    .expect("open plan");
    let request = LifecycleCacheOpenRequest::new(
        plan,
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
    )
    .expect("open request");
    LifecycleCacheRuntime::open(
        request,
        &backend,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        CommitManualTimestampSource::new(Timestamp::from_micros(1_000)),
    )
    .expect("cache runtime")
}

fn put_batch(branch: BranchId, key: PhysicalKey, value: Vec<u8>) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            key,
            value,
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Cache,
            crate::commit::CommitConflictValidationMode::Skip,
            crate::commit::CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn flush_request(branch: BranchId, suffix: &str) -> FlushFrozenRequest {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(format!("budget-{suffix}")).expect("seed"),
        FlushTableObjectId::new(format!("budget-{suffix}")).expect("object id"),
    )
    .expect("flush request")
}

fn storage_budget_with_active(active_bytes: u64) -> StorageRuntimeBudget {
    storage_budget_with_active_and_frozen(active_bytes, 8 * 1024)
}

fn storage_budget_with_active_and_frozen(
    active_bytes: u64,
    frozen_bytes: u64,
) -> StorageRuntimeBudget {
    let mut parts = budget_parts_with_active(active_bytes);
    parts.frozen_mutable_bytes = frozen_bytes;
    parts.total_bytes = pool_sum(parts);
    StorageRuntimeBudget::from_parts(parts).expect("budget")
}

fn budget_parts_with_active(active_bytes: u64) -> StorageRuntimeBudgetParts {
    StorageRuntimeBudgetParts {
        block_cache_bytes: 0,
        table_reader_bytes: 8 * 1024,
        active_mutable_bytes: active_bytes,
        frozen_mutable_bytes: 8 * 1024,
        maintenance_queue_bytes: 1024,
        generated_artifact_bytes: 8 * 1024,
        manifest_catalog_bytes: 1024,
        max_open_readers: 4,
        max_frozen_tables: 4,
        max_pending_maintenance_tasks: 4,
        ..StorageRuntimeBudgetParts::default()
    }
}

fn pool_sum(parts: StorageRuntimeBudgetParts) -> u64 {
    parts.block_cache_bytes
        + parts.table_reader_bytes
        + parts.active_mutable_bytes
        + parts.frozen_mutable_bytes
        + parts.maintenance_queue_bytes
        + parts.generated_artifact_bytes
        + parts.manifest_catalog_bytes
}

fn assert_budget_error(error: &LifecycleError, expected_pool: StorageBudgetPool) {
    let mut current: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(candidate) = current {
        if let Some(LifecycleError::StorageBudgetExceeded { pool, .. }) =
            candidate.downcast_ref::<LifecycleError>()
        {
            assert_eq!(*pool, expected_pool);
            return;
        }
        current = candidate.source();
    }
    panic!("expected storage budget error for {expected_pool:?}, got {error:?}");
}

fn assert_commit_runtime_error(
    error: &LifecycleError,
    predicate: impl FnOnce(&CommitRuntimeError) -> bool,
) {
    match error {
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::CommitRuntime,
            ..
        } => {
            let source = error
                .source()
                .and_then(|source| source.downcast_ref::<CommitRuntimeError>())
                .expect("commit runtime source");
            assert!(
                predicate(source),
                "unexpected commit runtime error {source:?}"
            );
        }
        other => panic!("expected commit runtime lower-layer error, got {other:?}"),
    }
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "budget",
        StorageSpaceId::engine(0x20).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}
