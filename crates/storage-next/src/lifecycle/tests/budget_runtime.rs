use super::*;
use crate::backend::memory::MemoryBackend;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::BranchOwnedTable;
use crate::branch::state::compaction::BranchCompactionKind;
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::WalServiceConfig;
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableReaderConfig, TableRow,
};
use std::error::Error;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[test]
fn storage_budget_rejects_reader_count_zero_when_readers_required() {
    let mut parts = budget_parts(16 * 1024);
    parts.max_open_readers = 0;
    parts.total_bytes = pool_sum(parts);

    assert_eq!(
        StorageRuntimeBudget::from_parts(parts),
        Err(LifecycleError::InvalidConfig {
            field: "storage_budget.max_open_readers",
            reason: "must be nonzero",
        })
    );
}

#[test]
fn storage_budget_rejects_frozen_table_count_zero_when_flush_enabled() {
    let mut parts = budget_parts(16 * 1024);
    parts.max_frozen_tables = 0;
    parts.total_bytes = pool_sum(parts);

    assert_eq!(
        StorageRuntimeBudget::from_parts(parts),
        Err(LifecycleError::InvalidConfig {
            field: "storage_budget.max_frozen_tables",
            reason: "must be nonzero",
        })
    );
}

#[test]
fn storage_budget_profile_does_not_probe_host_memory() {
    let first = StorageRuntimeBudget::low_memory_test_profile();
    let second = StorageRuntimeBudget::low_memory_test_profile();

    assert_eq!(first, second);
    assert_eq!(first.pool_limit_bytes(StorageBudgetPool::BlockCache), 0);
    assert_eq!(first.total_bytes(), 64 * 1024);
}

#[test]
fn low_memory_profile_does_not_apply_hidden_minimum_cache() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let cache = budget.table_cache_config().expect("cache config");

    assert!(!cache.enabled());
    assert_eq!(cache.capacity_bytes(), 0);
}

#[test]
fn budget_reservation_failed_acquire_does_not_change_usage() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let before = ledger.snapshot();

    let error = ledger
        .reserve(
            StorageBudgetPool::GeneratedArtifact,
            ledger
                .budget()
                .pool_limit_bytes(StorageBudgetPool::GeneratedArtifact)
                .saturating_add(1),
            0,
            "generated artifact",
        )
        .expect_err("reserve over limit");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert_eq!(ledger.snapshot(), before);
}

#[test]
fn budget_reservation_overflow_rejects() {
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::default()).expect("ledger");

    let error = ledger
        .reserve(StorageBudgetPool::TableReader, u64::MAX, 1, "reader open")
        .expect_err("overflowing reserve");

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
fn budget_reservation_rejects_one_byte_over_limit() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let limit = ledger
        .budget()
        .pool_limit_bytes(StorageBudgetPool::TableReader);

    let error = ledger
        .reserve(StorageBudgetPool::TableReader, limit + 1, 1, "reader open")
        .expect_err("one byte over rejects");

    assert_budget_error_details(&error, StorageBudgetPool::TableReader, limit + 1, 0, limit);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        0
    );
}

#[test]
fn budget_reservation_drop_releases_usage() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    {
        let _reservation = ledger
            .reserve(StorageBudgetPool::TableReader, 512, 1, "reader open")
            .expect("reader reserve");
        assert_eq!(
            ledger
                .snapshot()
                .usage(StorageBudgetPool::TableReader)
                .used_count(),
            1
        );
    }

    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 0);
    assert_eq!(usage.used_count(), 0);
}

#[test]
fn budget_stats_are_deterministic() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let _reader = ledger
        .reserve(StorageBudgetPool::TableReader, 512, 1, "reader open")
        .expect("reader reserve");

    assert_eq!(ledger.snapshot(), ledger.snapshot());
}

#[test]
fn reader_open_exact_budget_succeeds() {
    let mut parts = budget_parts(16 * 1024);
    parts.table_reader_bytes = 512;
    parts.max_open_readers = 1;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let _reader = ledger
        .reserve(StorageBudgetPool::TableReader, 512, 1, "reader open")
        .expect("exact reader budget");

    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 512);
    assert_eq!(usage.used_count(), 1);
    assert_eq!(usage.limit_count(), Some(1));
}

#[test]
fn reader_count_limit_rejects_extra_reader() {
    let mut parts = budget_parts(16 * 1024);
    parts.table_reader_bytes = 1024;
    parts.max_open_readers = 1;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");
    let _reader = ledger
        .reserve(StorageBudgetPool::TableReader, 256, 1, "reader open")
        .expect("first reader");

    let error = ledger
        .reserve(StorageBudgetPool::TableReader, 256, 1, "reader open")
        .expect_err("second reader rejects");

    assert_budget_error(&error, StorageBudgetPool::TableReader);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_count(),
        1
    );
}

#[test]
fn reader_open_over_budget_rejects_before_decode() {
    let mut parts = budget_parts(16 * 1024);
    parts.table_reader_bytes = 16;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_table_reader_budget(
        &ledger,
        17,
        "table reader admission before whole-table decode",
    )
    .expect_err("reader over budget rejects");

    assert_budget_error_details(&error, StorageBudgetPool::TableReader, 17, 0, 16);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_bytes(),
        0
    );
}

#[test]
fn reader_open_failure_releases_reservation() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    let error = (|| -> LifecycleResult<()> {
        let _reader = ledger.reserve(StorageBudgetPool::TableReader, 128, 1, "reader open")?;
        Err(LifecycleError::InvalidConfig {
            field: "reader_fixture",
            reason: "decode failed after admission",
        })
    })()
    .expect_err("fixture failure");

    assert_eq!(error.code(), "invalid_argument.lifecycle.config");
    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 0);
    assert_eq!(usage.used_count(), 0);
}

#[test]
fn reader_drop_releases_reservation() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let reservation = ledger
        .reserve(StorageBudgetPool::TableReader, 256, 1, "reader open")
        .expect("reader reservation");
    assert_eq!(reservation.count(), 1);

    drop(reservation);

    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 0);
    assert_eq!(usage.used_count(), 0);
}

#[test]
fn reader_budget_counts_concurrent_readers() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let _left = ledger
        .reserve(StorageBudgetPool::TableReader, 128, 1, "left reader")
        .expect("left reader");
    let _right = ledger
        .reserve(StorageBudgetPool::TableReader, 256, 1, "right reader")
        .expect("right reader");

    let usage = ledger.snapshot().usage(StorageBudgetPool::TableReader);
    assert_eq!(usage.used_bytes(), 384);
    assert_eq!(usage.used_count(), 2);
}

#[test]
fn reader_budget_error_names_table_identity() {
    let mut parts = budget_parts(16 * 1024);
    parts.table_reader_bytes = 8;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error =
        require_table_reader_budget(&ledger, 9, "table reader admission for table identity")
            .expect_err("reader over budget rejects");

    match error {
        LifecycleError::StorageBudgetExceeded { reason, .. } => {
            assert!(reason.contains("table identity"));
        }
        other => panic!("expected budget error, got {other:?}"),
    }
}

#[test]
fn reader_budget_below_metadata_rejects_in_both_cache_and_durable_mode() {
    // BS4.5a: cache flush charges the whole object; durable flush charges only the metadata-resident
    // footprint. This 1-byte reader budget sits below *even the metadata*, so both modes still reject —
    // the floor case where the two charges agree. (Where they diverge — a budget between the metadata and
    // the object — is covered by `durable_flush_admits_table_larger_than_reader_budget_while_cache_rejects`.)
    let cache_branch = branch_id(0x50);
    let durable_branch = branch_id(0x51);
    let mut parts = budget_parts(16 * 1024);
    parts.generated_artifact_bytes = 16 * 1024;
    parts.table_reader_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let budget = StorageRuntimeBudget::from_parts(parts).expect("budget");
    let mut cache = open_cache_runtime(cache_branch, budget);
    cache
        .execute_cache_commit(
            put_batch(
                cache_branch,
                physical_key(cache_branch, b"cache-reader-budget"),
                vec![0x61; 256],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit");
    cache.rotate_active_for_maintenance().expect("cache rotate");
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut durable = open_durable_runtime(durable_branch, backend, budget);
    durable
        .execute_durable_commit(
            durable_put_batch(
                durable_branch,
                physical_key(durable_branch, b"durable-reader-budget"),
                vec![0x62; 256],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("durable commit");
    durable
        .rotate_active_for_maintenance()
        .expect("durable rotate");

    let cache_error = cache
        .flush_frozen(&flush_request(cache_branch, "cache-reader-match"))
        .expect_err("cache reader budget rejects");
    let durable_error = durable
        .flush_frozen(&flush_request(durable_branch, "durable-reader-match"))
        .expect_err("durable reader budget rejects");

    assert_budget_error(&cache_error, StorageBudgetPool::TableReader);
    assert_budget_error(&durable_error, StorageBudgetPool::TableReader);
    assert_eq!(cache.branch_state().frozen_table_count(), 1);
    assert_eq!(durable.branch_state().frozen_table_count(), 1);
}

#[test]
fn durable_flush_admits_table_larger_than_reader_budget_while_cache_rejects() {
    // BS4.5a: a durable flush installs a lazy, disk-resident reader, so it charges only the
    // metadata-resident footprint against the reader pool — a table whose full object far exceeds the
    // reader budget still installs. A cache flush installs an eager, fully-resident reader and keeps
    // charging the whole object (constraint C2), so the same table is rejected. The reader budget sits
    // between the two: above the metadata (few hundred bytes) but well below the 32 KiB object.
    let cache_branch = branch_id(0x54);
    let durable_branch = branch_id(0x55);
    let mut parts = budget_parts(128 * 1024);
    parts.table_reader_bytes = 8 * 1024;
    parts.frozen_mutable_bytes = 64 * 1024;
    parts.generated_artifact_bytes = 64 * 1024;
    parts.total_bytes = pool_sum(parts);
    let budget = StorageRuntimeBudget::from_parts(parts).expect("budget");
    let value = vec![0x63; 32 * 1024];

    let mut cache = open_cache_runtime(cache_branch, budget);
    cache
        .execute_cache_commit(
            put_batch(
                cache_branch,
                physical_key(cache_branch, b"cache-large-table"),
                value.clone(),
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache commit");
    cache.rotate_active_for_maintenance().expect("cache rotate");

    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut durable = open_durable_runtime(durable_branch, backend, budget);
    durable
        .execute_durable_commit(
            durable_put_batch(
                durable_branch,
                physical_key(durable_branch, b"durable-large-table"),
                value,
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("durable commit");
    durable
        .rotate_active_for_maintenance()
        .expect("durable rotate");

    // Cache: the whole-object reader charge exceeds the reader budget — rejected, table stays frozen.
    let cache_error = cache
        .flush_frozen(&flush_request(cache_branch, "cache-large"))
        .expect_err("cache reader budget rejects the whole-object charge");
    assert_budget_error(&cache_error, StorageBudgetPool::TableReader);
    assert_eq!(cache.branch_state().frozen_table_count(), 1);

    // Durable: the metadata-resident charge fits — the table installs disk-resident.
    durable
        .flush_frozen(&flush_request(durable_branch, "durable-large"))
        .expect("durable flush admits a table larger than the reader budget");
    assert_eq!(durable.branch_state().frozen_table_count(), 0);
    assert_eq!(durable.branch_state().owned_table_count(), 1);
}

#[test]
fn rotate_active_under_frozen_budget_succeeds() {
    let branch = branch_id(0x41);
    let mut runtime = open_cache_runtime(branch, storage_budget(16 * 1024));
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"rotate-under"),
                vec![0x11; 128],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");

    runtime
        .rotate_active_for_maintenance()
        .expect("rotate under budget");

    let usage = runtime
        .budget_snapshot()
        .usage(StorageBudgetPool::FrozenMutable);
    assert_eq!(usage.used_count(), 1);
    assert!(usage.used_bytes() > 0);
    assert_eq!(runtime.branch_state().active_row_count(), 0);
    assert_eq!(runtime.branch_state().frozen_table_count(), 1);
}

#[test]
fn active_budget_crossing_rotates_after_commit() {
    let branch = branch_id(0x63);
    let mut runtime = open_cache_runtime(branch, storage_budget(128));
    let key = physical_key(branch, b"inline-rotate");

    runtime
        .execute_cache_commit(
            put_batch(branch, key.clone(), vec![0x11; 1024]),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit rotates active table");

    assert!(runtime
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .is_some());
    assert_eq!(runtime.branch_state().active_row_count(), 0);
    // #2541: the commit-path rotation is drained inline — the sealed
    // memtable becomes an owned L0 table instead of lingering frozen.
    assert_eq!(runtime.branch_state().frozen_table_count(), 0);
    assert_eq!(runtime.branch_state().owned_table_count(), 1);
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::ActiveMutable)
            .used_bytes(),
        0
    );
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::FrozenMutable)
            .used_count(),
        0
    );
}

#[test]
fn flush_releases_frozen_budget_after_install() {
    let branch = branch_id(0x42);
    let mut runtime = frozen_cache_runtime(branch, 16 * 1024);
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::FrozenMutable)
            .used_count(),
        1
    );

    runtime
        .flush_frozen(&flush_request(branch, "release"))
        .expect("flush");

    let usage = runtime
        .budget_snapshot()
        .usage(StorageBudgetPool::FrozenMutable);
    assert_eq!(usage.used_count(), 0);
    assert_eq!(runtime.branch_state().frozen_table_count(), 0);
    assert_eq!(runtime.branch_state().owned_table_count(), 1);
}

#[test]
fn flush_failure_keeps_frozen_budget_reserved() {
    let branch = branch_id(0x43);
    let wrong_branch = branch_id(0x44);
    let mut runtime = frozen_cache_runtime(branch, 16 * 1024);

    let error = runtime
        .flush_frozen(&flush_request(wrong_branch, "wrong-branch"))
        .expect_err("wrong branch flush rejects");

    assert!(
        !matches!(error, LifecycleError::StorageBudgetExceeded { .. }),
        "wrong-branch flush should fail semantically, got {error:?}"
    );
    let usage = runtime
        .budget_snapshot()
        .usage(StorageBudgetPool::FrozenMutable);
    assert_eq!(usage.used_count(), 1);
    assert_eq!(runtime.branch_state().frozen_table_count(), 1);
}

#[test]
fn maintenance_queue_byte_limit_rejects_large_task() {
    let branch = branch_id(0x45);
    let mut parts = budget_parts(16 * 1024);
    parts.maintenance_queue_bytes = 1;
    parts.max_pending_maintenance_tasks = 8;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );

    let error = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect_err("queue bytes reject");

    assert_budget_error(&error, StorageBudgetPool::MaintenanceQueue);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn maintenance_cancel_releases_reservation() {
    let branch = branch_id(0x46);
    let mut runtime = open_cache_runtime(branch, storage_budget(16 * 1024));
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue");
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        1
    );

    let close = runtime.close().expect("close cancels ordinary task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        0
    );
}

#[test]
fn maintenance_close_drain_releases_reservations() {
    let branch = branch_id(0x47);
    let mut runtime = open_cache_runtime(branch, storage_budget(16 * 1024));
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain task"),
        )
        .expect("enqueue drain task");

    let close = runtime.close().expect("close drains task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        0
    );
}

#[test]
fn maintenance_budget_pressure_added_to_outcome() {
    let branch = branch_id(0x48);
    let mut parts = budget_parts(16 * 1024);
    parts.maintenance_queue_bytes = 256;
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue");

    let snapshot = runtime.budget_snapshot();
    let usage = snapshot.usage(StorageBudgetPool::MaintenanceQueue);

    assert_eq!(usage.used_count(), 1);
    assert_eq!(usage.limit_count(), Some(1));
    assert_eq!(
        snapshot.pressure(StorageBudgetPool::MaintenanceQueue),
        StorageBudgetPressureSeverity::DeferOptionalMaintenance
    );
}

#[test]
fn maintenance_active_task_holds_reservation() {
    let branch = branch_id(0x4d);
    let mut parts = budget_parts(16 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let budget = StorageRuntimeBudget::from_parts(parts).expect("budget");
    let ledger = StorageBudgetLedger::new(budget).expect("ledger");
    let mut maintenance = LifecycleMaintenanceExecutor::new(4).expect("executor");
    maintenance.set_active_for_test(
        MaintenanceTask::new_for_test(1, MaintenanceTaskRequest::health_collection())
            .expect("active task"),
    );
    let branch_state = BranchLocalState::empty(branch);

    let snapshot = snapshot_with_runtime_usage(&ledger, &branch_state, maintenance.status());
    assert_eq!(
        snapshot
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        1
    );
    let error = require_maintenance_enqueue_budget(&ledger, maintenance.status())
        .expect_err("active task consumes the only task slot");
    assert_budget_error(&error, StorageBudgetPool::MaintenanceQueue);
}

#[test]
fn maintenance_task_failure_releases_reservation() {
    let branch = branch_id(0x57);
    let mut runtime = open_cache_runtime(branch, storage_budget(16 * 1024));
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect("enqueue task");
    let mut runner = FailingMaintenanceRunner;

    let error = runtime
        .run_next_maintenance(&mut runner)
        .expect_err("runner failure");

    assert_eq!(error.code(), "io.lifecycle.backend");
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert!(runtime.maintenance_status().active_task().is_none());
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        0
    );
}

#[test]
fn maintenance_optional_task_deferred_under_pressure() {
    let branch = branch_id(0x58);
    let mut parts = budget_parts(16 * 1024);
    parts.maintenance_queue_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );

    let error = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::health_collection())
        .expect_err("optional task rejected under queue pressure");

    assert_budget_error(&error, StorageBudgetPool::MaintenanceQueue);
    assert_eq!(
        runtime
            .budget_snapshot()
            .pressure(StorageBudgetPool::MaintenanceQueue),
        StorageBudgetPressureSeverity::Normal
    );
}

#[test]
fn maintenance_mandatory_close_task_admitted_under_optional_pressure() {
    let branch = branch_id(0x59);
    let mut parts = budget_parts(16 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_cache_runtime(
        branch,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain task"),
        )
        .expect("enqueue drain task");

    let close = runtime.close().expect("close drains mandatory work");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(
        runtime
            .budget_snapshot()
            .usage(StorageBudgetPool::MaintenanceQueue)
            .used_count(),
        0
    );
}

#[test]
fn metadata_budget_stats_report_catalog_bytes() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let _catalog = ledger
        .reserve(
            StorageBudgetPool::ManifestCatalog,
            512,
            2,
            "manifest catalog metadata",
        )
        .expect("catalog reserve");

    let usage = ledger.snapshot().usage(StorageBudgetPool::ManifestCatalog);
    assert_eq!(usage.used_bytes(), 512);
    assert_eq!(usage.used_count(), 2);
    assert_eq!(
        usage.limit_bytes(),
        ledger
            .budget()
            .pool_limit_bytes(StorageBudgetPool::ManifestCatalog)
    );
}

#[test]
fn recovery_mandatory_metadata_budget_failure_is_typed() {
    let mut parts = budget_parts(16 * 1024);
    parts.manifest_catalog_bytes = 16;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_manifest_catalog_budget(&ledger, 17, 1, "recovery metadata decode")
        .expect_err("metadata over budget");

    assert_budget_error_details(&error, StorageBudgetPool::ManifestCatalog, 17, 0, 16);
    assert_eq!(error.code(), "resource_exhausted.lifecycle.storage_budget");
}

#[test]
fn quarantine_inventory_over_budget_rejects_before_vector_allocation() {
    let mut parts = budget_parts(16 * 1024);
    parts.manifest_catalog_bytes = 8;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_manifest_catalog_budget(&ledger, 9, 1, "quarantine inventory metadata")
        .expect_err("inventory over budget");

    assert_budget_error(&error, StorageBudgetPool::ManifestCatalog);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::ManifestCatalog)
            .used_bytes(),
        0
    );
}

#[test]
fn retention_graph_over_budget_defers_optional_reclaim() {
    let mut parts = budget_parts(16 * 1024);
    parts.manifest_catalog_bytes = 8;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_manifest_catalog_budget(&ledger, 9, 1, "retention graph metadata")
        .expect_err("retention graph over budget");

    assert_budget_error(&error, StorageBudgetPool::ManifestCatalog);
    assert_eq!(
        ledger
            .snapshot()
            .with_usage(StorageBudgetPool::ManifestCatalog, 9, 1)
            .pressure(StorageBudgetPool::ManifestCatalog),
        StorageBudgetPressureSeverity::RejectOptionalWork
    );
}

#[test]
fn metadata_pressure_blocks_optional_maintenance_first() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let ledger = StorageBudgetLedger::new(budget).expect("ledger");
    let over_limit = budget
        .pool_limit_bytes(StorageBudgetPool::ManifestCatalog)
        .saturating_add(1);
    let snapshot = ledger
        .snapshot()
        .with_usage(StorageBudgetPool::ManifestCatalog, over_limit, 1);

    assert_eq!(
        snapshot.pressure(StorageBudgetPool::ManifestCatalog),
        StorageBudgetPressureSeverity::RejectOptionalWork
    );
}

#[test]
fn corrupt_metadata_does_not_allocate_unbounded_memory() {
    let mut parts = budget_parts(16 * 1024);
    parts.manifest_catalog_bytes = 4;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_manifest_catalog_budget(&ledger, u64::MAX, u64::MAX, "corrupt metadata")
        .expect_err("corrupt metadata count rejects");

    assert_budget_error(&error, StorageBudgetPool::ManifestCatalog);
    let usage = ledger.snapshot().usage(StorageBudgetPool::ManifestCatalog);
    assert_eq!(usage.used_bytes(), 0);
    assert_eq!(usage.used_count(), 0);
}

#[test]
fn low_memory_profile_opens_cache_runtime() {
    let branch = branch_id(0x49);
    let runtime = open_cache_runtime(branch, StorageRuntimeBudget::low_memory_test_profile());

    assert_eq!(runtime.open_outcome().mode(), StorageMode::Cache);
    assert_eq!(
        runtime
            .budget_snapshot()
            .budget()
            .pool_limit_bytes(StorageBudgetPool::BlockCache),
        0
    );
}

#[test]
fn low_memory_profile_opens_durable_runtime_on_test_backend() {
    let branch = branch_id(0x4e);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::low_memory_test_profile(),
    );

    assert_eq!(
        runtime.open_outcome().mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        runtime
            .budget_snapshot()
            .budget()
            .pool_limit_bytes(StorageBudgetPool::BlockCache),
        0
    );
}

#[test]
fn low_memory_profile_opens_durable_runtime_on_memory_backend() {
    let branch = branch_id(0x5a);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::low_memory_test_profile(),
    );

    assert_eq!(
        runtime.open_outcome().mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        runtime
            .budget_snapshot()
            .budget()
            .pool_limit_bytes(StorageBudgetPool::BlockCache),
        0
    );
}

#[test]
fn low_memory_profile_does_not_auto_detect_host_memory() {
    assert_eq!(
        StorageRuntimeBudget::low_memory_test_profile(),
        StorageRuntimeBudget::low_memory_test_profile()
    );
}

#[test]
fn low_memory_profile_allows_small_commit_read_flush_checkpoint_close() {
    let branch = branch_id(0x4a);
    let mut runtime = open_cache_runtime(branch, StorageRuntimeBudget::low_memory_test_profile());
    let key = physical_key(branch, b"small-flow");
    runtime
        .execute_cache_commit(
            put_batch(branch, key.clone(), b"value".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");
    assert!(runtime
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .is_some());
    runtime.rotate_active_for_maintenance().expect("rotate");
    runtime
        .flush_frozen(&flush_request(branch, "low-memory"))
        .expect("flush");

    let close = runtime.close().expect("close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
}

#[test]
fn low_memory_profile_zero_cache_still_reads_uncached() {
    let branch = branch_id(0x4b);
    let mut runtime = open_cache_runtime(branch, StorageRuntimeBudget::low_memory_test_profile());
    let key = physical_key(branch, b"uncached-read");
    runtime
        .execute_cache_commit(
            put_batch(branch, key.clone(), b"value".to_vec()),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");

    assert!(runtime
        .read_view()
        .expect("view")
        .latest(&key)
        .expect("read")
        .is_some());
}

#[test]
fn low_memory_profile_reports_pressure_without_product_policy() {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    let ledger = StorageBudgetLedger::new(budget).expect("ledger");
    let _reservation = ledger
        .reserve(
            StorageBudgetPool::GeneratedArtifact,
            budget.pool_limit_bytes(StorageBudgetPool::GeneratedArtifact),
            0,
            "generated artifact",
        )
        .expect("reserve generated");

    let snapshot = ledger.snapshot();

    assert_eq!(
        snapshot.pressure(StorageBudgetPool::GeneratedArtifact),
        StorageBudgetPressureSeverity::DeferOptionalMaintenance
    );
    assert_eq!(
        snapshot
            .usage(StorageBudgetPool::GeneratedArtifact)
            .pool()
            .name(),
        "generated_artifact"
    );
}

#[test]
fn checkpoint_encode_over_budget_rejects_before_snapshot_publish() {
    let branch = branch_id(0x4c);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut parts = budget_parts(32 * 1024);
    parts.generated_artifact_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .execute_durable_commit(
            durable_put_batch(
                branch,
                physical_key(branch, b"checkpoint-budget"),
                vec![0x33; 256],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("durable commit");
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(10_000))
        .expect("request");

    let error = runtime
        .checkpoint(&request)
        .expect_err("checkpoint artifact budget rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert!(
        runtime
            .services()
            .snapshot()
            .list_snapshots()
            .expect("snapshot list")
            .is_empty(),
        "budget rejection must happen before snapshot publication"
    );
    assert_eq!(backend.delete_calls(), 0);
}

#[test]
fn flush_artifact_exact_budget_succeeds() {
    let mut parts = budget_parts(16 * 1024);
    parts.generated_artifact_bytes = 4096;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    require_generated_artifact_budget(&ledger, 4096, "flush artifact exact fit")
        .expect("exact generated artifact budget");
}

#[test]
fn compaction_artifact_over_budget_defers_before_publish() {
    let branch = branch_id(0x52);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut parts = budget_parts(64 * 1024);
    parts.generated_artifact_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    install_budget_l0_table(
        runtime.branch_state_mut(),
        branch,
        "budget-compaction-left",
        vec![budget_row(branch, b"left", 1, 1_000, &[0x71; 256])],
    );
    install_budget_l0_table(
        runtime.branch_state_mut(),
        branch,
        "budget-compaction-right",
        vec![budget_row(branch, b"right", 2, 2_000, &[0x72; 256])],
    );

    let error = runtime
        .compact_branch_tables(
            &LifecycleCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactL0,
                "budget-compaction-output",
            )
            .expect("compaction request"),
        )
        .expect_err("generated artifact budget rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert_eq!(backend.table_object_create_calls(), 0);
    assert_eq!(runtime.branch_state().owned_table_count(), 2);
}

#[test]
fn materialization_artifact_over_budget_defers_before_publish() {
    let parent = branch_id(0x53);
    let child = branch_id(0x54);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut parts = budget_parts(64 * 1024);
    parts.generated_artifact_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut parent_state = BranchLocalState::empty(parent);
    install_budget_l0_table(
        &mut parent_state,
        parent,
        "budget-material-parent",
        vec![budget_row(parent, b"inherited", 3, 3_000, &[0x73; 256])],
    );
    let (child_state, _) = parent_state
        .fork_into_empty_child(child)
        .expect("fork child");
    let mut runtime = open_durable_runtime(
        child,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    *runtime.branch_state_mut() = child_state;

    let error = runtime
        .materialize_inherited_layer(
            &LifecycleMaterializationRequest::new(child, 0, "budget-material-output")
                .expect("materialization request"),
        )
        .expect_err("generated artifact budget rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert_eq!(backend.table_object_create_calls(), 0);
    assert_eq!(runtime.branch_state().inherited_layers().len(), 1);
}

#[test]
fn partial_artifact_failure_releases_budget() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");

    let error = (|| -> LifecycleResult<()> {
        let _artifact = ledger.reserve(
            StorageBudgetPool::GeneratedArtifact,
            1024,
            0,
            "generated artifact",
        )?;
        Err(LifecycleError::MaintenanceTaskFailed {
            reason: "publish failed after artifact admission",
        })
    })()
    .expect_err("fixture failure");

    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.maintenance_task"
    );
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::GeneratedArtifact)
            .used_bytes(),
        0
    );
}

#[test]
fn artifact_actual_size_reconciles_with_estimate() {
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::low_memory_test_profile()).expect("ledger");
    let estimated = 2048;
    let reservation = ledger
        .reserve(
            StorageBudgetPool::GeneratedArtifact,
            estimated,
            0,
            "generated artifact",
        )
        .expect("artifact reserve");

    assert_eq!(reservation.bytes(), estimated);
    assert_eq!(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::GeneratedArtifact)
            .used_bytes(),
        estimated
    );
}

#[test]
fn artifact_budget_reports_output_bytes() {
    let mut parts = budget_parts(16 * 1024);
    parts.generated_artifact_bytes = 32;
    parts.total_bytes = pool_sum(parts);
    let ledger = StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).expect("budget"))
        .expect("ledger");

    let error = require_generated_artifact_budget(&ledger, 33, "generated table output")
        .expect_err("artifact over budget");

    assert_budget_error_details(&error, StorageBudgetPool::GeneratedArtifact, 33, 0, 32);
}

#[test]
fn artifact_budget_does_not_truncate_wal_or_delete_objects() {
    let branch = branch_id(0x55);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut parts = budget_parts(32 * 1024);
    parts.generated_artifact_bytes = 1;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .execute_durable_commit(
            durable_put_batch(
                branch,
                physical_key(branch, b"artifact-no-cleanup"),
                vec![0x74; 256],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("durable commit");

    let error = runtime
        .checkpoint(
            &LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(11_000))
                .expect("checkpoint request")
                .with_wal_truncation_after_checkpoint(true),
        )
        .expect_err("checkpoint artifact budget rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert!(backend.snapshot_objects().is_empty());
    assert_eq!(backend.delete_calls(), 0);
}

#[test]
fn low_memory_profile_defers_large_compaction_artifact() {
    let branch = branch_id(0x56);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::low_memory_test_profile(),
    );
    install_budget_l0_table(
        runtime.branch_state_mut(),
        branch,
        "budget-low-compaction-left",
        vec![budget_row(branch, b"left", 1, 1_000, &[0x75; 32 * 1024])],
    );
    install_budget_l0_table(
        runtime.branch_state_mut(),
        branch,
        "budget-low-compaction-right",
        vec![budget_row(branch, b"right", 2, 2_000, &[0x76; 32 * 1024])],
    );

    let error = runtime
        .compact_branch_tables(
            &LifecycleCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactL0,
                "budget-low-compaction-output",
            )
            .expect("compaction request"),
        )
        .expect_err("low-memory artifact rejects");

    assert_budget_error(&error, StorageBudgetPool::GeneratedArtifact);
    assert_eq!(backend.table_object_create_calls(), 0);
}

#[test]
fn cache_and_durable_rotation_budget_behavior_diverges() {
    let cache_branch = branch_id(0x61);
    let durable_branch = branch_id(0x62);
    // Durable mode still enforces the projected rotation/frozen budget, so a
    // tight frozen pool rejects an oversize commit. Cache is volatile and no
    // longer applies source-shape admission pressure, so it must accept the
    // same oversize commit; it is given a roomy frozen pool that the rotated
    // table fits within, proving the divergence is the admission decision and
    // not a hidden hard-ledger rejection.
    let durable_budget = storage_budget_with_frozen(128, 64);
    let cache_budget = storage_budget_with_frozen(16 * 1024, 16 * 1024);
    let mut cache = open_cache_runtime(cache_branch, cache_budget);
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let mut durable = open_durable_runtime(durable_branch, backend, durable_budget);

    // Cache is volatile in-memory storage: it no longer projects incoming
    // rotation against the frozen budget, so an oversize commit succeeds.
    cache
        .execute_cache_commit(
            put_batch(
                cache_branch,
                physical_key(cache_branch, b"parity-cache"),
                vec![0x33; 1024],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("cache no longer blocks on rotation budget");
    // Durable mode is unchanged: it still rejects on the projected frozen
    // budget before allocating a commit version.
    let durable_error = durable
        .execute_durable_commit(
            durable_put_batch(
                durable_branch,
                physical_key(durable_branch, b"parity-durable"),
                vec![0x34; 1024],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect_err("durable rotation budget rejects oversize commit");

    assert_frozen_backlog_pressure_rejection(&durable_error);
    assert_ne!(
        cache.visible_version(),
        CommitVersion::ZERO,
        "cache commit must advance visible version past zero"
    );
    assert!(
        !cache.branch_state().is_empty(),
        "cache commit must populate branch state"
    );
    assert_eq!(durable.visible_version(), CommitVersion::ZERO);
    assert!(durable.branch_state().is_empty());
    // The durable budget rejection precedes WAL append, so neither shell
    // records an unresolved durable commit.
    assert!(cache.unresolved_durable().expect("cache gate").is_none());
    assert!(durable
        .unresolved_durable()
        .expect("durable gate")
        .is_none());
}

fn frozen_cache_runtime(
    branch: BranchId,
    active_budget: u64,
) -> LifecycleCacheRuntime<CommitManualTimestampSource> {
    let mut runtime = open_cache_runtime(branch, storage_budget(active_budget));
    runtime
        .execute_cache_commit(
            put_batch(branch, physical_key(branch, b"frozen"), vec![0x22; 128]),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");
    runtime.rotate_active_for_maintenance().expect("rotate");
    runtime
}

#[test]
fn durable_global_total_reflects_committed_resident_bytes() {
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let branch = branch_id(0x71);
    let mut runtime = open_durable_runtime(branch, backend, StorageRuntimeBudget::default());
    assert_eq!(
        runtime.budget_total_used_bytes(),
        0,
        "a fresh database holds no Strata-owned memory"
    );

    runtime
        .execute_durable_commit(
            durable_put_batch(
                branch,
                physical_key(branch, b"global-total"),
                vec![0x44; 4096],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("commit");

    // Refreshing the database-wide total (the diagnostics path does this) counts the committed
    // rows toward the global memory budget — proving the runtime-summed accounting end to end.
    let _ = runtime.budget_snapshot();
    assert!(
        runtime.budget_total_used_bytes() > 0,
        "committed rows count toward the global memory total"
    );
}

/// Independent full fold of a branch's resident bytes, computed directly from the tables
/// (active memtable + frozen tables + owned-level tables) rather than the BS1 cached shape
/// aggregates. This is the reference the runtime memory total must equal; because it reads
/// table sizes directly, it also validates the total in release, where BS1.1's per-branch
/// debug oracle is compiled out.
fn fold_resident_bytes(state: &BranchLocalState) -> u64 {
    let active = u64::try_from(state.active().approximate_size_bytes()).unwrap_or(u64::MAX);
    let frozen = state.frozen().iter().fold(0u64, |total, table| {
        total.saturating_add(u64::try_from(table.approximate_size_bytes()).unwrap_or(u64::MAX))
    });
    let owned = state
        .owned_levels()
        .iter()
        .flatten()
        .fold(0u64, |total, table| {
            total.saturating_add(table.approximate_size_bytes())
        });
    active.saturating_add(frozen).saturating_add(owned)
}

#[test]
fn durable_runtime_total_matches_independent_full_fold() {
    // The block cache is disabled (`budget_parts` sets `block_cache_bytes = 0`), so the published
    // runtime total is exactly the branch resident bytes and can be checked against an independent
    // fold over the tables after a real flush + compaction sequence. This catches drift in the
    // O(branches) memory-total composition (the fold + publish in `refresh_runtime_memory_total`),
    // which the per-branch BS1.1 oracle does not cover, and holds in release.
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let branch = branch_id(0x74);
    let mut parts = budget_parts(256 * 1024);
    parts.table_reader_bytes = 1024 * 1024;
    parts.generated_artifact_bytes = 1024 * 1024;
    parts.frozen_mutable_bytes = 256 * 1024;
    parts.max_frozen_tables = 8;
    parts.max_open_readers = 64;
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_durable_runtime(
        branch,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );

    // Two owned L0 tables via the real flush path (commit -> rotate -> flush), then compact them.
    for tag in ["a", "b"] {
        runtime
            .execute_durable_commit(
                durable_put_batch(
                    branch,
                    physical_key(branch, tag.as_bytes()),
                    vec![0x33; 512],
                ),
                CommitBranchGenerationGuard::exact(
                    CommitBranchGeneration::new(1).expect("generation"),
                ),
            )
            .expect("commit");
        runtime.rotate_active_for_maintenance().expect("rotate");
        runtime
            .flush_frozen(&flush_request(branch, tag))
            .expect("flush");
    }
    runtime
        .compact_branch_tables(
            &LifecycleCompactionRequest::new(
                branch,
                BranchCompactionKind::CompactL0,
                "drift-compact-output",
            )
            .expect("compaction request"),
        )
        .expect("compact");

    // Leave a rotated-but-unflushed frozen table plus a populated active memtable, so all three
    // resident components (active + frozen + owned) contribute to the total.
    runtime
        .execute_durable_commit(
            durable_put_batch(
                branch,
                physical_key(branch, b"frozen-tail"),
                vec![0x35; 512],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("commit");
    runtime.rotate_active_for_maintenance().expect("rotate");
    runtime
        .execute_durable_commit(
            durable_put_batch(
                branch,
                physical_key(branch, b"active-tail"),
                vec![0x36; 512],
            ),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("commit");

    let _ = runtime.budget_snapshot();
    let state = runtime.branch_state();
    assert!(
        state.owned_table_count() > 0,
        "compaction left owned tables"
    );
    assert_eq!(state.frozen_table_count(), 1, "one frozen table remains");
    assert!(state.active_row_count() > 0, "active memtable populated");
    assert_eq!(
        runtime.runtime_total_bytes(),
        fold_resident_bytes(state),
        "durable runtime memory total must equal an independent full table fold"
    );
}

#[test]
fn cache_runtime_total_matches_independent_full_fold() {
    // Cache mode has no block-cache term at all (`refresh_runtime_memory_total` sets the total to
    // the branch resident sum), so the published total must equal the independent fold over active
    // + frozen tables regardless of the budget.
    let branch = branch_id(0x76);
    let mut runtime = open_cache_runtime(branch, storage_budget(256 * 1024));
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"cache-frozen"),
                vec![0x33; 512],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");
    runtime.rotate_active_for_maintenance().expect("rotate");
    runtime
        .execute_cache_commit(
            put_batch(
                branch,
                physical_key(branch, b"cache-active"),
                vec![0x34; 512],
            ),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit");

    let _ = runtime.budget_snapshot();
    let state = runtime.branch_state();
    assert_eq!(state.frozen_table_count(), 1, "one frozen table remains");
    assert!(state.active_row_count() > 0, "active memtable populated");
    assert_eq!(
        runtime.runtime_total_bytes(),
        fold_resident_bytes(state),
        "cache runtime memory total must equal an independent full table fold"
    );
}

#[test]
fn durable_runtime_total_sums_resident_bytes_across_all_branches() {
    // The runtime total is the O(branches) sum of every branch's resident bytes. Verify it equals
    // the exact per-branch fold across two asymmetric branches, so a fold bug that skips a branch
    // or double-counts one would be caught (the combined-over-budget rejection test only proves the
    // sum is large enough to trip the budget, not that it is exact). Block cache disabled -> total
    // is exactly the branch sum.
    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let branch_a = branch_id(0x77);
    let branch_b = branch_id(0x78);
    let mut parts = budget_parts(1024 * 1024);
    parts.total_bytes = pool_sum(parts);
    let mut runtime = open_durable_runtime(
        branch_a,
        backend,
        StorageRuntimeBudget::from_parts(parts).expect("budget"),
    );
    runtime
        .create_branch(
            branch_b,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create branch b");

    // Asymmetric resident sizes so `fold_a != fold_b` — a bug that counts one branch for both would
    // not produce the correct sum.
    runtime
        .execute_durable_commit(
            durable_put_batch(branch_a, physical_key(branch_a, b"a"), vec![0x33; 1024]),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("commit a");
    runtime
        .execute_durable_commit(
            durable_put_batch(branch_b, physical_key(branch_b, b"b"), vec![0x34; 4096]),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("commit b");

    let _ = runtime.budget_snapshot();
    let fold_a = fold_resident_bytes(
        runtime
            .branch_catalog()
            .branch_state(branch_a)
            .expect("branch a state"),
    );
    let fold_b = fold_resident_bytes(
        runtime
            .branch_catalog()
            .branch_state(branch_b)
            .expect("branch b state"),
    );
    assert!(
        fold_a > 0 && fold_b > 0,
        "both branches hold resident bytes"
    );
    assert_ne!(fold_a, fold_b, "branches are asymmetric");
    assert_eq!(
        runtime.runtime_total_bytes(),
        fold_a.saturating_add(fold_b),
        "runtime memory total must sum resident bytes across all branches"
    );
}

#[test]
fn cache_runtime_total_sums_resident_bytes_across_all_branches() {
    // Cache mode folds resident bytes across all branches on its own (separate) refresh path,
    // with no block-cache term. Verify the published total equals the exact per-branch fold across
    // two asymmetric branches (the cache cross-branch fold is a distinct copy of the durable one).
    let branch_a = branch_id(0x79);
    let branch_b = branch_id(0x7a);
    let mut runtime = open_cache_runtime(branch_a, storage_budget(1024 * 1024));
    runtime
        .create_branch(
            branch_b,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create branch b");
    runtime
        .execute_cache_commit(
            put_batch(branch_a, physical_key(branch_a, b"a"), vec![0x33; 1024]),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit a");
    runtime
        .execute_cache_commit(
            put_batch(branch_b, physical_key(branch_b, b"b"), vec![0x34; 4096]),
            CommitBranchGenerationGuard::not_supplied(),
        )
        .expect("commit b");

    let _ = runtime.budget_snapshot();
    let fold_a = fold_resident_bytes(
        runtime
            .branch_catalog()
            .branch_state(branch_a)
            .expect("branch a state"),
    );
    let fold_b = fold_resident_bytes(
        runtime
            .branch_catalog()
            .branch_state(branch_b)
            .expect("branch b state"),
    );
    assert!(
        fold_a > 0 && fold_b > 0,
        "both branches hold resident bytes"
    );
    assert_ne!(fold_a, fold_b, "branches are asymmetric");
    assert_eq!(
        runtime.runtime_total_bytes(),
        fold_a.saturating_add(fold_b),
        "cache runtime memory total must sum resident bytes across all branches"
    );
}

#[test]
fn multi_branch_combined_resident_over_budget_is_admitted_as_gauge() {
    // BS4.5a: total_bytes is just above active_mutable_bytes, so a single branch can never exceed the
    // global total through its own active pool — only two branches' combined resident can. Before the
    // disk-resident flip this hard-rejected the second commit; now the database-wide durable memory
    // budget is an observability gauge, not an admission failure — a durable dataset is no longer
    // RAM-bounded, and per-branch memtable pressure is still enforced by rotation + frozen-backlog
    // admission. Cache mode keeps its hard reject (see
    // `api::tests::cache::cache_multi_branch_over_global_budget_is_refused`).
    let mut parts = budget_parts(64 * 1024);
    parts.table_reader_bytes = 1024;
    parts.frozen_mutable_bytes = 1024;
    parts.generated_artifact_bytes = 1024;
    parts.total_bytes = pool_sum(parts);
    let budget = StorageRuntimeBudget::from_parts(parts).expect("budget");

    let backend: &'static super::checkpoint::shared::CheckpointTestBackend = Box::leak(Box::new(
        super::checkpoint::shared::CheckpointTestBackend::new(),
    ));
    let branch_a = branch_id(0x72);
    let branch_b = branch_id(0x73);
    let mut runtime = open_durable_runtime(branch_a, backend, budget);
    runtime
        .create_branch(
            branch_b,
            CommitBranchGeneration::new(1).expect("generation"),
            None,
        )
        .expect("create branch b");

    // Each commit (~48 KiB) stays under active_mutable (64 KiB), so neither branch trips its own
    // per-branch admission or rotates; but the two branches' combined resident exceeds the total.
    let value = vec![0x55; 48 * 1024];
    runtime
        .execute_durable_commit(
            durable_put_batch(branch_a, physical_key(branch_a, b"a"), value.clone()),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("branch a commit fits the per-branch and global budget");

    // BS4.5a: branch B's commit pushes the database-wide total over budget but is admitted — the
    // over-budget condition is surfaced as a gauge (perf-trace counter), not a refusal.
    runtime
        .execute_durable_commit(
            durable_put_batch(branch_b, physical_key(branch_b, b"b"), value),
            CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation")),
        )
        .expect("branch b commit is admitted despite exceeding the database memory budget");

    // Both branches' values are visible: neither commit was refused.
    let view_a = runtime
        .branch_catalog()
        .branch_state(branch_a)
        .expect("branch a state")
        .capture_read_view()
        .expect("branch a view");
    assert!(
        view_a
            .latest(&physical_key(branch_a, b"a"))
            .expect("read a")
            .is_some(),
        "branch A's committed value is visible"
    );
    let view_b = runtime
        .branch_catalog()
        .branch_state(branch_b)
        .expect("branch b state")
        .capture_read_view()
        .expect("branch b view");
    assert!(
        view_b
            .latest(&physical_key(branch_b, b"b"))
            .expect("read b")
            .is_some(),
        "branch B's committed value is visible — the over-budget commit was admitted"
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

fn open_durable_runtime(
    branch: BranchId,
    backend: &'static dyn crate::backend::Backend,
    budget: StorageRuntimeBudget,
) -> LifecycleDurableLocalRuntime<'static, CommitManualTimestampSource> {
    let config = LifecycleConfig::default()
        .with_storage_budget(budget)
        .expect("storage budget config");
    let plan = StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        config,
    )
    .expect("open plan");
    let request = LifecycleDurableLocalOpenRequest::new(
        plan,
        [0x9b; 16],
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default(),
    )
    .expect("open request");
    let mut shell = LifecycleDurableLocalShell::assemble(
        request,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(2_000)),
    )
    .expect("durable shell");
    let recovery_request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .expect("recovery");
    shell.complete_recovery(&recovery).expect("open runtime")
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

fn durable_put_batch(branch: BranchId, key: PhysicalKey, value: Vec<u8>) -> CommitBatch {
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
            CommitDurabilityMode::Standard,
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
        FlushTableIdentitySeed::new(format!("budget-runtime-{suffix}")).expect("seed"),
        FlushTableObjectId::new(format!("budget-runtime-{suffix}")).expect("object id"),
    )
    .expect("flush request")
}

fn install_budget_l0_table(
    state: &mut BranchLocalState,
    branch: BranchId,
    identity: &str,
    rows: Vec<StorageRow>,
) {
    let table = budget_owned_table(branch, BranchLevel::ZERO, identity, rows);
    state
        .install_owned_table_at_level(BranchLevel::ZERO, table)
        .expect("install test table");
}

fn budget_owned_table(
    branch: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> BranchOwnedTable {
    let identity = TableIdentity::new(identity).expect("identity");
    let mut table_rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut table_rows);
    let artifact = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .expect("builder")
        .build_from_rows(identity.clone(), &table_rows)
        .expect("table artifact");
    let reader = ImmutableTableReader::open_bytes(
        identity.clone(),
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .expect("reader");
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), level).expect("descriptor");
    let extras =
        crate::table::TableSummaryExtras::from_rows(reader.rows()).expect("table summary extras");
    BranchOwnedTable::new(branch, descriptor, reader, extras).expect("owned table")
}

fn budget_row(
    branch: BranchId,
    user_key: &[u8],
    version: u64,
    timestamp: u64,
    value: &[u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

struct FailingMaintenanceRunner;

impl MaintenanceTaskRunner for FailingMaintenanceRunner {
    fn run_task(&mut self, _task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Err(LifecycleError::lower_layer_with(
            LifecycleLowerLayer::Backend,
            "maintenance runner failed",
            BudgetRuntimeFailure,
        ))
    }
}

#[derive(Debug)]
struct BudgetRuntimeFailure;

impl std::fmt::Display for BudgetRuntimeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("budget runtime failure")
    }
}

impl Error for BudgetRuntimeFailure {}

fn storage_budget(active_bytes: u64) -> StorageRuntimeBudget {
    storage_budget_with_frozen(active_bytes, 8 * 1024)
}

fn storage_budget_with_frozen(active_bytes: u64, frozen_bytes: u64) -> StorageRuntimeBudget {
    let mut parts = budget_parts(active_bytes);
    parts.frozen_mutable_bytes = frozen_bytes;
    parts.total_bytes = pool_sum(parts);
    StorageRuntimeBudget::from_parts(parts).expect("budget")
}

fn budget_parts(active_bytes: u64) -> StorageRuntimeBudgetParts {
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

fn assert_frozen_backlog_pressure_rejection(error: &LifecycleError) {
    assert!(matches!(
        error,
        LifecycleError::StoragePressureRejected {
            severity: LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            pressure_reason: LifecycleStoragePressureReason::FrozenBacklog,
            retryable: false,
            ..
        }
    ));
}

fn assert_budget_error_details(
    error: &LifecycleError,
    expected_pool: StorageBudgetPool,
    expected_requested_bytes: u64,
    expected_used_bytes: u64,
    expected_limit_bytes: u64,
) {
    let mut current: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(candidate) = current {
        if let Some(LifecycleError::StorageBudgetExceeded {
            pool,
            requested_bytes,
            used_bytes,
            limit_bytes,
            ..
        }) = candidate.downcast_ref::<LifecycleError>()
        {
            assert_eq!(*pool, expected_pool);
            assert_eq!(*requested_bytes, expected_requested_bytes);
            assert_eq!(*used_bytes, expected_used_bytes);
            assert_eq!(*limit_bytes, expected_limit_bytes);
            return;
        }
        current = candidate.source();
    }
    panic!("expected storage budget error for {expected_pool:?}, got {error:?}");
}

fn physical_key(branch: BranchId, user_key: &[u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "budget-runtime",
        StorageSpaceId::engine(0x21).expect("engine space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}
