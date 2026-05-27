//! Generated lifecycle storage-budget contract helpers.

use super::{ensure, script_byte};
use crate::lifecycle::{
    require_generated_artifact_budget, require_maintenance_enqueue_budget,
    require_table_reader_budget, LifecycleMaintenanceExecutor, StorageBudgetLedger,
    StorageBudgetPool, StorageRuntimeBudget, StorageRuntimeBudgetParts,
};
use crate::table::{
    CacheInsert, TableBlockAddress, TableBlockCache, TableBlockCacheKey, TableBlockCacheKind,
    TableCacheConfig, TableCacheTableId,
};
use crate::testkit::TestkitError;
use std::sync::Arc;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleBudgetContractOutcome {
    budget_accept: usize,
    budget_reject: usize,
    reservation_release_on_success: usize,
    reservation_release_on_failure: usize,
    cache_eviction: usize,
    reader_reject: usize,
    active_reject: usize,
    artifact_defer: usize,
    maintenance_queue_reject: usize,
    low_memory_smoke: usize,
    isolation: usize,
}

pub fn check_lifecycle_budget_contract(
    script: &[u8],
) -> Result<LifecycleBudgetContractOutcome, TestkitError> {
    let mut outcome = LifecycleBudgetContractOutcome::default();
    check_budget_accept(script, &mut outcome)?;
    check_budget_reject(&mut outcome)?;
    check_release_on_success(script, &mut outcome)?;
    check_release_on_failure(script, &mut outcome)?;
    check_cache_eviction(script, &mut outcome)?;
    check_reader_reject(script, &mut outcome)?;
    check_active_reject(script, &mut outcome)?;
    check_artifact_defer(script, &mut outcome)?;
    check_maintenance_queue_reject(script, &mut outcome)?;
    check_low_memory_smoke(&mut outcome)?;
    check_database_local_isolation(script, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleBudgetContractOutcome {
    pub const fn budget_accept_cases(&self) -> usize {
        self.budget_accept
    }

    pub const fn budget_reject_cases(&self) -> usize {
        self.budget_reject
    }

    pub const fn reservation_release_on_success_cases(&self) -> usize {
        self.reservation_release_on_success
    }

    pub const fn reservation_release_on_failure_cases(&self) -> usize {
        self.reservation_release_on_failure
    }

    pub const fn cache_eviction_cases(&self) -> usize {
        self.cache_eviction
    }

    pub const fn reader_reject_cases(&self) -> usize {
        self.reader_reject
    }

    pub const fn active_reject_cases(&self) -> usize {
        self.active_reject
    }

    pub const fn artifact_defer_cases(&self) -> usize {
        self.artifact_defer
    }

    pub const fn maintenance_queue_reject_cases(&self) -> usize {
        self.maintenance_queue_reject
    }

    pub const fn low_memory_smoke_cases(&self) -> usize {
        self.low_memory_smoke
    }

    pub const fn isolation_cases(&self) -> usize {
        self.isolation
    }
}

fn check_budget_accept(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = scripted_budget(script)?;
    budget.validate().map_err(testkit_error)?;
    outcome.budget_accept += 1;
    Ok(())
}

fn check_budget_reject(outcome: &mut LifecycleBudgetContractOutcome) -> Result<(), TestkitError> {
    let error = StorageRuntimeBudget::from_parts(StorageRuntimeBudgetParts {
        active_mutable_bytes: 0,
        ..StorageRuntimeBudgetParts::default()
    })
    .expect_err("zero active pool rejects");
    ensure(
        error.code() == "invalid_argument.lifecycle.config",
        "invalid budget did not return stable config code",
    )?;
    outcome.budget_reject += 1;
    Ok(())
}

fn check_release_on_success(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let ledger = StorageBudgetLedger::new(scripted_budget(script)?).map_err(testkit_error)?;
    {
        let _reservation = ledger
            .reserve(StorageBudgetPool::TableReader, 16, 1, "reader")
            .map_err(testkit_error)?;
        ensure(
            ledger
                .snapshot()
                .usage(StorageBudgetPool::TableReader)
                .used_count()
                == 1,
            "reader reservation was not counted",
        )?;
    }
    ensure(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_count()
            == 0,
        "reader reservation did not release",
    )?;
    outcome.reservation_release_on_success += 1;
    Ok(())
}

fn check_release_on_failure(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let ledger = StorageBudgetLedger::new(scripted_budget(script)?).map_err(testkit_error)?;
    {
        let _reservation = ledger
            .reserve(StorageBudgetPool::GeneratedArtifact, 32, 0, "artifact")
            .map_err(testkit_error)?;
        let error = ledger
            .reserve(
                StorageBudgetPool::GeneratedArtifact,
                ledger
                    .budget()
                    .pool_limit_bytes(StorageBudgetPool::GeneratedArtifact),
                0,
                "artifact",
            )
            .expect_err("nested generated artifact reserve rejects");
        ensure(
            error.code() == "resource_exhausted.lifecycle.storage_budget",
            "nested generated artifact rejection did not use budget error",
        )?;
    }
    ensure(
        ledger
            .snapshot()
            .usage(StorageBudgetPool::GeneratedArtifact)
            .used_bytes()
            == 0,
        "failed generated artifact path leaked reservation",
    )?;
    outcome.reservation_release_on_failure += 1;
    Ok(())
}

fn check_cache_eviction(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let capacity_len = u32::from(script_byte(script, 0).max(16));
    let capacity = usize::try_from(capacity_len).expect("u32 fits usize on supported targets");
    let cache = TableBlockCache::new(
        TableCacheConfig::new(true, capacity)
            .map_err(|error| TestkitError::new(error.to_string()))?,
    );
    cache
        .insert(cache_key("left", 0, 8), Arc::from(vec![1_u8; 8]))
        .map_err(|error| TestkitError::new(error.to_string()))?;
    let inserted = cache
        .insert(
            cache_key("right", 8, capacity_len),
            Arc::from(vec![2_u8; capacity]),
        )
        .map_err(|error| TestkitError::new(error.to_string()))?;
    ensure(
        matches!(inserted, CacheInsert::Inserted(_)),
        "cache did not admit bounded entry",
    )?;
    ensure(
        cache.stats().bytes() <= capacity,
        "cache exceeded configured capacity",
    )?;
    outcome.cache_eviction += 1;
    Ok(())
}

fn check_reader_reject(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = scripted_budget(script)?;
    let ledger = StorageBudgetLedger::new(budget).map_err(testkit_error)?;
    let limit = budget.pool_limit_bytes(StorageBudgetPool::TableReader);
    let error = require_table_reader_budget(&ledger, limit + 1, "reader")
        .expect_err("reader over limit rejects");
    ensure(
        error.code() == "resource_exhausted.lifecycle.storage_budget",
        "reader rejection did not use budget error",
    )?;
    outcome.reader_reject += 1;
    Ok(())
}

fn check_active_reject(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = scripted_budget(script)?;
    let ledger = StorageBudgetLedger::new(budget).map_err(testkit_error)?;
    let limit = budget.pool_limit_bytes(StorageBudgetPool::ActiveMutable);
    let error = ledger
        .check_available(
            StorageBudgetPool::ActiveMutable,
            limit + 1,
            0,
            "active rows",
        )
        .expect_err("active rows over limit reject");
    ensure(
        error.code() == "resource_exhausted.lifecycle.storage_budget",
        "active rejection did not use budget error",
    )?;
    outcome.active_reject += 1;
    Ok(())
}

fn check_artifact_defer(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = scripted_budget(script)?;
    let ledger = StorageBudgetLedger::new(budget).map_err(testkit_error)?;
    let limit = budget.pool_limit_bytes(StorageBudgetPool::GeneratedArtifact);
    let error = require_generated_artifact_budget(&ledger, limit + 1, "artifact")
        .expect_err("artifact over limit rejects");
    ensure(
        error.code() == "resource_exhausted.lifecycle.storage_budget",
        "artifact rejection did not use budget error",
    )?;
    outcome.artifact_defer += 1;
    Ok(())
}

fn check_maintenance_queue_reject(
    _script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let mut parts = budget_parts(64 * 1024);
    parts.max_pending_maintenance_tasks = 1;
    parts.total_bytes = pool_sum(parts);
    let ledger =
        StorageBudgetLedger::new(StorageRuntimeBudget::from_parts(parts).map_err(testkit_error)?)
            .map_err(testkit_error)?;
    let mut executor = LifecycleMaintenanceExecutor::new(4).map_err(testkit_error)?;
    executor
        .enqueue(
            open_state()?,
            crate::lifecycle::MaintenanceTaskRequest::health_collection(),
        )
        .map_err(testkit_error)?;
    let error = require_maintenance_enqueue_budget(&ledger, executor.status())
        .expect_err("queue limit rejects");
    ensure(
        error.code() == "resource_exhausted.lifecycle.storage_budget",
        "maintenance rejection did not use budget error",
    )?;
    outcome.maintenance_queue_reject += 1;
    Ok(())
}

fn check_low_memory_smoke(
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = StorageRuntimeBudget::low_memory_test_profile();
    ensure(
        budget.pool_limit_bytes(StorageBudgetPool::BlockCache) == 0,
        "low-memory profile inflated cache",
    )?;
    ensure(
        !budget
            .table_cache_config()
            .map_err(testkit_error)?
            .enabled(),
        "zero cache profile enabled cache",
    )?;
    outcome.low_memory_smoke += 1;
    Ok(())
}

fn check_database_local_isolation(
    script: &[u8],
    outcome: &mut LifecycleBudgetContractOutcome,
) -> Result<(), TestkitError> {
    let budget = scripted_budget(script)?;
    let left = StorageBudgetLedger::new(budget).map_err(testkit_error)?;
    let right = StorageBudgetLedger::new(budget).map_err(testkit_error)?;
    let _left_reader = left
        .reserve(StorageBudgetPool::TableReader, 64, 1, "left reader")
        .map_err(testkit_error)?;
    ensure(
        right
            .snapshot()
            .usage(StorageBudgetPool::TableReader)
            .used_count()
            == 0,
        "budget ledger leaked usage across instances",
    )?;
    outcome.isolation += 1;
    Ok(())
}

fn scripted_budget(script: &[u8]) -> Result<StorageRuntimeBudget, TestkitError> {
    let active = 8 * 1024 + u64::from(script_byte(script, 1)) * 16;
    let mut parts = budget_parts(active);
    parts.total_bytes = pool_sum(parts);
    StorageRuntimeBudget::from_parts(parts).map_err(testkit_error)
}

fn budget_parts(active_bytes: u64) -> StorageRuntimeBudgetParts {
    StorageRuntimeBudgetParts {
        block_cache_bytes: 1024,
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

fn cache_key(table: &str, offset: u64, length: u32) -> TableBlockCacheKey {
    TableBlockCacheKey::new(
        TableCacheTableId::new(table.as_bytes()).expect("table id"),
        TableBlockAddress::new(TableBlockCacheKind::Data, offset, length, None)
            .expect("cache address"),
    )
}

fn open_state() -> Result<crate::lifecycle::LifecycleStateMachine, TestkitError> {
    let mut state = crate::lifecycle::LifecycleStateMachine::new();
    state
        .transition(crate::lifecycle::LifecycleTransitionTrigger::OpenRequested)
        .map_err(testkit_error)?;
    state
        .transition(crate::lifecycle::LifecycleTransitionTrigger::CacheOpenReady)
        .map_err(testkit_error)?;
    Ok(state)
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}
