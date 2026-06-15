//! Storage-local memory and cache budget accounting.
//!
//! The ledger exposes two complementary admission models:
//!
//! 1. RAII reservations via `StorageBudgetLedger::reserve` and the returned
//!    `StorageBudgetReservation`. Acquired bytes and count stay charged to
//!    a pool until the guard drops, at which point usage is released on
//!    every path — panics, early returns, and failed nested work all
//!    converge through `Drop`. Tests exercise this mode to prove the
//!    accounting contract, and later lazy-block-reader work will use it
//!    to track range reservations.
//! 2. Admission checks via `check_available`, plus the
//!    `require_table_reader_budget`, `require_generated_artifact_budget`,
//!    and `require_manifest_catalog_budget` helpers. These verify that a
//!    single requested allocation fits under the pool limit but do not
//!    hold the budget after the call returns.
//!
//! V1 production paths use admission checks for the `TableReader`,
//! `GeneratedArtifact`, and `ManifestCatalog` pools. Whole-object readers,
//! generated artifacts, and manifest catalog bytes are admitted in a
//! single check against the configured pool limit; once the call returns
//! the ledger usage for those pools stays at zero. This bounds any single
//! allocation but does not track cumulative usage across concurrent
//! flushes, compactions, or recoveries. Block-range RAII reservations are
//! deferred until whole-object reads are replaced by lazy block reads;
//! those code paths will switch from `check_available` to `reserve`, and
//! the existing ledger contract carries over without changes.
//!
//! `ActiveMutable`, `FrozenMutable`, and `MaintenanceQueue` usage are
//! reported by [`snapshot_with_runtime_usage`] from runtime state — the
//! current `BranchLocalState` and `MaintenanceExecutorStatus` — rather
//! than from the ledger. The ledger snapshot still serves limit and
//! pressure facts for those pools, but used bytes and counts derive from
//! the live runtime, which is exact under the current single-threaded
//! admission ordering and would need synchronized reservations to hold
//! under multi-threaded admission.

use super::{LifecycleError, LifecycleLowerLayer, LifecycleResult, MaintenanceExecutorStatus};
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBranchApplyTarget, CommitExpiry, CommitLowerLayer, CommitMutation,
    CommitRuntimeError, CommitRuntimeResult,
};
use crate::row::StorageRow;
use crate::table::{TableBlockCache, TableCacheConfig, TableRow};
use std::sync::{Arc, Mutex, MutexGuard};
use strata_core_next::{CommitVersion, Timestamp};

const DEFAULT_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_BLOCK_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_TABLE_READER_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_ACTIVE_MUTABLE_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_FROZEN_MUTABLE_BYTES: u64 = 128 * 1024 * 1024;
const DEFAULT_MAINTENANCE_QUEUE_BYTES: u64 = 1024 * 1024;
const DEFAULT_GENERATED_ARTIFACT_BYTES: u64 = 96 * 1024 * 1024;
const DEFAULT_MANIFEST_CATALOG_BYTES: u64 = 16 * 1024 * 1024;
const DEFAULT_MAX_OPEN_READERS: u32 = 1024;
const DEFAULT_MAX_FROZEN_TABLES: u32 = 1024;
const DEFAULT_MAX_PENDING_MAINTENANCE_TASKS: u32 = 1024;
const MAINTENANCE_TASK_METADATA_BYTES: u64 = 256;
const COMMIT_TIMELINE_ACTIVE_BYTE_RESERVE: u64 = 1024;
const POOL_COUNT: usize = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageRuntimeBudget {
    total_bytes: u64,
    block_cache_bytes: u64,
    table_reader_bytes: u64,
    active_mutable_bytes: u64,
    frozen_mutable_bytes: u64,
    maintenance_queue_bytes: u64,
    generated_artifact_bytes: u64,
    manifest_catalog_bytes: u64,
    max_open_readers: u32,
    max_frozen_tables: u32,
    max_pending_maintenance_tasks: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageRuntimeBudgetParts {
    pub(crate) total_bytes: u64,
    pub(crate) block_cache_bytes: u64,
    pub(crate) table_reader_bytes: u64,
    pub(crate) active_mutable_bytes: u64,
    pub(crate) frozen_mutable_bytes: u64,
    pub(crate) maintenance_queue_bytes: u64,
    pub(crate) generated_artifact_bytes: u64,
    pub(crate) manifest_catalog_bytes: u64,
    pub(crate) max_open_readers: u32,
    pub(crate) max_frozen_tables: u32,
    pub(crate) max_pending_maintenance_tasks: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub(crate) enum StorageBudgetPool {
    BlockCache,
    TableReader,
    ActiveMutable,
    FrozenMutable,
    MaintenanceQueue,
    GeneratedArtifact,
    ManifestCatalog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum StorageBudgetPressureSeverity {
    Normal,
    Evicting,
    DeferOptionalMaintenance,
    RejectOptionalWork,
    RejectMutatingAdmission,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageBudgetUsage {
    pool: StorageBudgetPool,
    used_bytes: u64,
    limit_bytes: u64,
    used_count: u64,
    limit_count: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageBudgetSnapshot {
    budget: StorageRuntimeBudget,
    usages: [StorageBudgetUsage; POOL_COUNT],
}

#[derive(Clone, Debug)]
pub(crate) struct StorageBudgetLedger {
    budget: StorageRuntimeBudget,
    state: Arc<Mutex<StorageBudgetCounters>>,
}

type StorageBudgetCounters = ([u64; POOL_COUNT], [u64; POOL_COUNT]);

#[derive(Debug)]
pub(crate) struct StorageBudgetReservation {
    ledger: StorageBudgetLedger,
    pool: StorageBudgetPool,
    bytes: u64,
    count: u64,
    released: bool,
}

pub(crate) struct BudgetedCommitBranch<'a> {
    branch: &'a mut BranchLocalState,
    ledger: &'a StorageBudgetLedger,
}

pub(crate) fn branch_config_with_storage_budget(
    branch_config: BranchRuntimeConfig,
    budget: StorageRuntimeBudget,
) -> LifecycleResult<BranchRuntimeConfig> {
    branch_config
        .with_active_rotation_bytes(active_rotation_bytes_from_budget(budget))
        .map_err(|source| {
            LifecycleError::lower_layer_with(
                LifecycleLowerLayer::BranchRuntime,
                "branch configuration rejected active mutable budget",
                source,
            )
        })
}

pub(crate) fn table_block_cache_from_storage_budget(
    budget: StorageRuntimeBudget,
) -> LifecycleResult<Option<Arc<TableBlockCache>>> {
    let config = budget.table_cache_config()?;
    if !config.enabled() {
        return Ok(None);
    }
    Ok(Some(Arc::new(TableBlockCache::new(config))))
}

impl StorageRuntimeBudget {
    pub(crate) fn from_parts(parts: StorageRuntimeBudgetParts) -> LifecycleResult<Self> {
        let budget = Self {
            total_bytes: parts.total_bytes,
            block_cache_bytes: parts.block_cache_bytes,
            table_reader_bytes: parts.table_reader_bytes,
            active_mutable_bytes: parts.active_mutable_bytes,
            frozen_mutable_bytes: parts.frozen_mutable_bytes,
            maintenance_queue_bytes: parts.maintenance_queue_bytes,
            generated_artifact_bytes: parts.generated_artifact_bytes,
            manifest_catalog_bytes: parts.manifest_catalog_bytes,
            max_open_readers: parts.max_open_readers,
            max_frozen_tables: parts.max_frozen_tables,
            max_pending_maintenance_tasks: parts.max_pending_maintenance_tasks,
        };
        budget.validate()?;
        Ok(budget)
    }

    /// An effectively-unlimited budget for volatile (cache) storage.
    ///
    /// Cache mode never flushes mutable state to table sources, so frozen and
    /// active mutable memory grow with the working set. Like an in-memory cache
    /// (or a `:memory:` database), the only real bound is host memory, so the
    /// mutable pools and the overall total are set far above any real working
    /// set. The durable-artifact pools keep their defaults because cache never
    /// produces table/manifest artifacts. The mutable pools are set to a
    /// fraction of `u64::MAX` rather than the max itself so the validation sum
    /// stays free of overflow. The huge active-mutable limit also lifts the
    /// active rotation threshold, so a cache branch keeps a single growing
    /// in-memory table instead of fanning out across frozen tables.
    pub(crate) fn unlimited() -> Self {
        let unbounded_pool = u64::MAX / 4;
        Self::from_parts(StorageRuntimeBudgetParts {
            total_bytes: u64::MAX,
            block_cache_bytes: DEFAULT_BLOCK_CACHE_BYTES,
            table_reader_bytes: DEFAULT_TABLE_READER_BYTES,
            active_mutable_bytes: unbounded_pool,
            frozen_mutable_bytes: unbounded_pool,
            maintenance_queue_bytes: DEFAULT_MAINTENANCE_QUEUE_BYTES,
            generated_artifact_bytes: DEFAULT_GENERATED_ARTIFACT_BYTES,
            manifest_catalog_bytes: DEFAULT_MANIFEST_CATALOG_BYTES,
            max_open_readers: DEFAULT_MAX_OPEN_READERS,
            max_frozen_tables: u32::MAX,
            max_pending_maintenance_tasks: DEFAULT_MAX_PENDING_MAINTENANCE_TASKS,
        })
        .expect("unlimited storage budget is valid")
    }

    pub(crate) fn low_memory_test_profile() -> Self {
        Self::from_parts(StorageRuntimeBudgetParts {
            total_bytes: 64 * 1024,
            block_cache_bytes: 0,
            table_reader_bytes: 8 * 1024,
            active_mutable_bytes: 8 * 1024,
            frozen_mutable_bytes: 16 * 1024,
            maintenance_queue_bytes: 1024,
            generated_artifact_bytes: 24 * 1024,
            manifest_catalog_bytes: 7 * 1024,
            max_open_readers: 4,
            max_frozen_tables: 4,
            max_pending_maintenance_tasks: 4,
        })
        .expect("low-memory storage budget profile is valid")
    }

    #[cfg(test)]
    pub(crate) fn scaled_closed_loop_test_profile() -> Self {
        Self::from_parts(StorageRuntimeBudgetParts {
            total_bytes: 4 * 1024 * 1024,
            block_cache_bytes: 0,
            table_reader_bytes: 512 * 1024,
            active_mutable_bytes: 256 * 1024,
            frozen_mutable_bytes: 768 * 1024,
            maintenance_queue_bytes: 64 * 1024,
            generated_artifact_bytes: 2 * 1024 * 1024,
            manifest_catalog_bytes: 256 * 1024,
            max_open_readers: 32,
            max_frozen_tables: 8,
            max_pending_maintenance_tasks: 64,
        })
        .expect("scaled closed-loop storage budget profile is valid")
    }

    pub(crate) fn validate(self) -> LifecycleResult<()> {
        require_nonzero("storage_budget.total_bytes", self.total_bytes)?;
        require_nonzero("storage_budget.table_reader_bytes", self.table_reader_bytes)?;
        require_nonzero(
            "storage_budget.active_mutable_bytes",
            self.active_mutable_bytes,
        )?;
        require_nonzero(
            "storage_budget.frozen_mutable_bytes",
            self.frozen_mutable_bytes,
        )?;
        require_nonzero(
            "storage_budget.maintenance_queue_bytes",
            self.maintenance_queue_bytes,
        )?;
        require_nonzero(
            "storage_budget.generated_artifact_bytes",
            self.generated_artifact_bytes,
        )?;
        require_nonzero(
            "storage_budget.manifest_catalog_bytes",
            self.manifest_catalog_bytes,
        )?;
        if self.max_open_readers == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "storage_budget.max_open_readers",
                reason: "must be nonzero",
            });
        }
        if self.max_frozen_tables == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "storage_budget.max_frozen_tables",
                reason: "must be nonzero",
            });
        }
        if self.max_pending_maintenance_tasks == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "storage_budget.max_pending_maintenance_tasks",
                reason: "must be nonzero",
            });
        }
        let sum = self
            .block_cache_bytes
            .checked_add(self.table_reader_bytes)
            .and_then(|sum| sum.checked_add(self.active_mutable_bytes))
            .and_then(|sum| sum.checked_add(self.frozen_mutable_bytes))
            .and_then(|sum| sum.checked_add(self.maintenance_queue_bytes))
            .and_then(|sum| sum.checked_add(self.generated_artifact_bytes))
            .and_then(|sum| sum.checked_add(self.manifest_catalog_bytes))
            .ok_or(LifecycleError::InvalidConfig {
                field: "storage_budget.total_bytes",
                reason: "pool byte sum overflowed",
            })?;
        if sum > self.total_bytes {
            return Err(LifecycleError::InvalidConfig {
                field: "storage_budget.total_bytes",
                reason: "pool byte sum exceeds total",
            });
        }
        Ok(())
    }

    pub(crate) const fn total_bytes(self) -> u64 {
        self.total_bytes
    }

    pub(crate) const fn pool_limit_bytes(self, pool: StorageBudgetPool) -> u64 {
        match pool {
            StorageBudgetPool::BlockCache => self.block_cache_bytes,
            StorageBudgetPool::TableReader => self.table_reader_bytes,
            StorageBudgetPool::ActiveMutable => self.active_mutable_bytes,
            StorageBudgetPool::FrozenMutable => self.frozen_mutable_bytes,
            StorageBudgetPool::MaintenanceQueue => self.maintenance_queue_bytes,
            StorageBudgetPool::GeneratedArtifact => self.generated_artifact_bytes,
            StorageBudgetPool::ManifestCatalog => self.manifest_catalog_bytes,
        }
    }

    pub(crate) const fn pool_limit_count(self, pool: StorageBudgetPool) -> Option<u64> {
        match pool {
            StorageBudgetPool::TableReader => Some(self.max_open_readers as u64),
            StorageBudgetPool::FrozenMutable => Some(self.max_frozen_tables as u64),
            StorageBudgetPool::MaintenanceQueue => Some(self.max_pending_maintenance_tasks as u64),
            StorageBudgetPool::BlockCache
            | StorageBudgetPool::ActiveMutable
            | StorageBudgetPool::GeneratedArtifact
            | StorageBudgetPool::ManifestCatalog => None,
        }
    }

    pub(crate) const fn max_frozen_tables(self) -> u32 {
        self.max_frozen_tables
    }

    pub(crate) const fn max_pending_maintenance_tasks(self) -> u32 {
        self.max_pending_maintenance_tasks
    }

    pub(crate) fn table_cache_config(self) -> LifecycleResult<TableCacheConfig> {
        let capacity =
            usize::try_from(self.block_cache_bytes).map_err(|_| LifecycleError::InvalidConfig {
                field: "storage_budget.block_cache_bytes",
                reason: "must fit in usize",
            })?;
        TableCacheConfig::new(capacity > 0, capacity).map_err(|source| {
            LifecycleError::lower_layer_with(
                super::LifecycleLowerLayer::TableRuntime,
                "table cache configuration rejected storage budget",
                source,
            )
        })
    }
}

impl Default for StorageRuntimeBudget {
    fn default() -> Self {
        Self::from_parts(StorageRuntimeBudgetParts::default())
            .expect("default storage runtime budget is valid")
    }
}

impl Default for StorageRuntimeBudgetParts {
    fn default() -> Self {
        Self {
            total_bytes: DEFAULT_TOTAL_BYTES,
            block_cache_bytes: DEFAULT_BLOCK_CACHE_BYTES,
            table_reader_bytes: DEFAULT_TABLE_READER_BYTES,
            active_mutable_bytes: DEFAULT_ACTIVE_MUTABLE_BYTES,
            frozen_mutable_bytes: DEFAULT_FROZEN_MUTABLE_BYTES,
            maintenance_queue_bytes: DEFAULT_MAINTENANCE_QUEUE_BYTES,
            generated_artifact_bytes: DEFAULT_GENERATED_ARTIFACT_BYTES,
            manifest_catalog_bytes: DEFAULT_MANIFEST_CATALOG_BYTES,
            max_open_readers: DEFAULT_MAX_OPEN_READERS,
            max_frozen_tables: DEFAULT_MAX_FROZEN_TABLES,
            max_pending_maintenance_tasks: DEFAULT_MAX_PENDING_MAINTENANCE_TASKS,
        }
    }
}

impl StorageBudgetPool {
    pub(crate) const ALL: [Self; POOL_COUNT] = [
        Self::BlockCache,
        Self::TableReader,
        Self::ActiveMutable,
        Self::FrozenMutable,
        Self::MaintenanceQueue,
        Self::GeneratedArtifact,
        Self::ManifestCatalog,
    ];

    const fn index(self) -> usize {
        match self {
            Self::BlockCache => 0,
            Self::TableReader => 1,
            Self::ActiveMutable => 2,
            Self::FrozenMutable => 3,
            Self::MaintenanceQueue => 4,
            Self::GeneratedArtifact => 5,
            Self::ManifestCatalog => 6,
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::BlockCache => "block_cache",
            Self::TableReader => "table_reader",
            Self::ActiveMutable => "active_mutable",
            Self::FrozenMutable => "frozen_mutable",
            Self::MaintenanceQueue => "maintenance_queue",
            Self::GeneratedArtifact => "generated_artifact",
            Self::ManifestCatalog => "manifest_catalog",
        }
    }
}

impl StorageBudgetUsage {
    pub(crate) const fn new(
        budget: StorageRuntimeBudget,
        pool: StorageBudgetPool,
        used_bytes: u64,
        used_count: u64,
    ) -> Self {
        Self {
            pool,
            used_bytes,
            limit_bytes: budget.pool_limit_bytes(pool),
            used_count,
            limit_count: budget.pool_limit_count(pool),
        }
    }

    pub(crate) const fn pool(self) -> StorageBudgetPool {
        self.pool
    }

    pub(crate) const fn used_bytes(self) -> u64 {
        self.used_bytes
    }

    pub(crate) const fn limit_bytes(self) -> u64 {
        self.limit_bytes
    }

    pub(crate) const fn used_count(self) -> u64 {
        self.used_count
    }

    pub(crate) const fn limit_count(self) -> Option<u64> {
        self.limit_count
    }
}

impl StorageBudgetSnapshot {
    fn from_state(budget: StorageRuntimeBudget, state: StorageBudgetCounters) -> Self {
        let usages = StorageBudgetPool::ALL.map(|pool| {
            StorageBudgetUsage::new(budget, pool, state.0[pool.index()], state.1[pool.index()])
        });
        Self { budget, usages }
    }

    pub(crate) const fn budget(&self) -> StorageRuntimeBudget {
        self.budget
    }

    pub(crate) fn usage(&self, pool: StorageBudgetPool) -> StorageBudgetUsage {
        self.usages[pool.index()]
    }

    pub(crate) fn usages(&self) -> &[StorageBudgetUsage] {
        &self.usages
    }

    pub(crate) fn pressure(&self, pool: StorageBudgetPool) -> StorageBudgetPressureSeverity {
        pressure_severity(self.usage(pool))
    }

    pub(crate) fn with_usage(
        mut self,
        pool: StorageBudgetPool,
        used_bytes: u64,
        used_count: u64,
    ) -> Self {
        self.usages[pool.index()] =
            StorageBudgetUsage::new(self.budget, pool, used_bytes, used_count);
        self
    }
}

impl StorageBudgetLedger {
    pub(crate) fn new(budget: StorageRuntimeBudget) -> LifecycleResult<Self> {
        budget.validate()?;
        Ok(Self {
            budget,
            state: Arc::new(Mutex::new(empty_budget_counters())),
        })
    }

    pub(crate) const fn budget(&self) -> StorageRuntimeBudget {
        self.budget
    }

    pub(crate) fn reserve(
        &self,
        pool: StorageBudgetPool,
        bytes: u64,
        count: u64,
        reason: &'static str,
    ) -> LifecycleResult<StorageBudgetReservation> {
        let mut state = self.lock_state()?;
        check_available(self.budget, *state, pool, bytes, count, reason)?;
        let index = pool.index();
        state.0[index] =
            state.0[index]
                .checked_add(bytes)
                .ok_or(LifecycleError::StorageBudgetExceeded {
                    pool,
                    requested_bytes: bytes,
                    used_bytes: state.0[index],
                    limit_bytes: self.budget.pool_limit_bytes(pool),
                    requested_count: count,
                    used_count: state.1[index],
                    limit_count: self.budget.pool_limit_count(pool),
                    reason: "budget byte accounting overflow",
                })?;
        state.1[index] =
            state.1[index]
                .checked_add(count)
                .ok_or(LifecycleError::StorageBudgetExceeded {
                    pool,
                    requested_bytes: bytes,
                    used_bytes: state.0[index],
                    limit_bytes: self.budget.pool_limit_bytes(pool),
                    requested_count: count,
                    used_count: state.1[index],
                    limit_count: self.budget.pool_limit_count(pool),
                    reason: "budget count accounting overflow",
                })?;
        Ok(StorageBudgetReservation {
            ledger: self.clone(),
            pool,
            bytes,
            count,
            released: false,
        })
    }

    pub(crate) fn check_available(
        &self,
        pool: StorageBudgetPool,
        bytes: u64,
        count: u64,
        reason: &'static str,
    ) -> LifecycleResult<()> {
        let state = self.lock_state()?;
        check_available(self.budget, *state, pool, bytes, count, reason)
    }

    pub(crate) fn snapshot(&self) -> StorageBudgetSnapshot {
        let state = self
            .state
            .lock()
            .map_or_else(|poisoned| *poisoned.into_inner(), |guard| *guard);
        StorageBudgetSnapshot::from_state(self.budget, state)
    }

    fn release(&self, pool: StorageBudgetPool, bytes: u64, count: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = pool.index();
        state.0[index] = state.0[index].saturating_sub(bytes);
        state.1[index] = state.1[index].saturating_sub(count);
    }

    fn lock_state(&self) -> LifecycleResult<MutexGuard<'_, StorageBudgetCounters>> {
        self.state
            .lock()
            .map_err(|_| LifecycleError::MaintenanceFailed {
                reason: "storage budget ledger lock is poisoned",
            })
    }
}

impl StorageBudgetReservation {
    pub(crate) const fn pool(&self) -> StorageBudgetPool {
        self.pool
    }

    pub(crate) const fn bytes(&self) -> u64 {
        self.bytes
    }

    pub(crate) const fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if !self.released {
            self.ledger.release(self.pool, self.bytes, self.count);
            self.released = true;
        }
    }
}

impl Drop for StorageBudgetReservation {
    fn drop(&mut self) {
        self.release_inner();
    }
}

impl<'a> BudgetedCommitBranch<'a> {
    pub(crate) fn new(branch: &'a mut BranchLocalState, ledger: &'a StorageBudgetLedger) -> Self {
        Self { branch, ledger }
    }
}

impl CommitBranchApplyTarget for BudgetedCommitBranch<'_> {
    fn branch_id(&self) -> strata_core_next::BranchId {
        self.branch.branch_id()
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        self.branch.max_commit_version()
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<crate::branch::read::BranchReadView> {
        self.branch.capture_read_view().map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "branch read view capture failed",
                source,
            )
        })
    }

    fn validate_committed_rows_before_apply(
        &self,
        rows: &[crate::row::StorageRow],
    ) -> CommitRuntimeResult<()> {
        require_active_rows_budget(self.ledger, self.branch, rows).map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::StorageBudget,
                "storage budget rejected commit rows",
                source,
            )
        })
    }

    fn append_committed_rows_atomically(
        &mut self,
        rows: Vec<crate::row::StorageRow>,
    ) -> CommitRuntimeResult<()> {
        self.branch
            .append_committed_rows_atomically(rows)
            .map(|_| ())
            .map_err(|source| {
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::BranchRuntime,
                    "branch state rejected commit rows",
                    source,
                )
            })
    }
}

fn require_active_rows_budget(
    ledger: &StorageBudgetLedger,
    branch: &BranchLocalState,
    rows: &[crate::row::StorageRow],
) -> LifecycleResult<()> {
    let requested = estimate_rows_active_bytes(rows)?;
    let current = branch.active().approximate_size_bytes() as u64;
    let projected =
        current
            .checked_add(requested)
            .ok_or(LifecycleError::StorageBudgetExceeded {
                pool: StorageBudgetPool::ActiveMutable,
                requested_bytes: requested,
                used_bytes: current,
                limit_bytes: ledger
                    .budget()
                    .pool_limit_bytes(StorageBudgetPool::ActiveMutable),
                requested_count: 0,
                used_count: 0,
                limit_count: None,
                reason: "active mutable byte accounting overflowed",
            })?;
    if projected >= branch.config().active_rotation_bytes() as u64
        && branch.frozen().len() < branch.config().max_frozen_tables()
    {
        return require_projected_rotation_budget(ledger, branch, projected);
    }
    require_projected_usage(
        ledger.budget(),
        StorageBudgetPool::ActiveMutable,
        current,
        0,
        requested,
        0,
        "commit would exceed active mutable storage budget",
    )
}

fn require_projected_rotation_budget(
    ledger: &StorageBudgetLedger,
    branch: &BranchLocalState,
    projected_active_bytes: u64,
) -> LifecycleResult<()> {
    let frozen_bytes = frozen_bytes(branch)?;
    let frozen_count = u64::try_from(branch.frozen().len()).unwrap_or(u64::MAX);
    require_projected_usage(
        ledger.budget(),
        StorageBudgetPool::FrozenMutable,
        frozen_bytes,
        frozen_count,
        projected_active_bytes,
        1,
        "rotation after commit would exceed frozen mutable storage budget",
    )
}

pub(crate) fn require_table_reader_budget(
    ledger: &StorageBudgetLedger,
    bytes: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    ledger.check_available(StorageBudgetPool::TableReader, bytes, 1, reason)
}

pub(crate) fn require_generated_artifact_budget(
    ledger: &StorageBudgetLedger,
    bytes: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    ledger.check_available(StorageBudgetPool::GeneratedArtifact, bytes, 0, reason)
}

pub(crate) fn require_manifest_catalog_budget(
    ledger: &StorageBudgetLedger,
    bytes: u64,
    count: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    ledger.check_available(StorageBudgetPool::ManifestCatalog, bytes, count, reason)
}

pub(crate) fn require_rotate_budget(
    ledger: &StorageBudgetLedger,
    branch: &BranchLocalState,
) -> LifecycleResult<()> {
    if branch.active().is_empty() {
        return Ok(());
    }
    let active_bytes = branch.active().approximate_size_bytes() as u64;
    let frozen_bytes = frozen_bytes(branch)?;
    let frozen_count = u64::try_from(branch.frozen().len()).unwrap_or(u64::MAX);
    require_projected_usage(
        ledger.budget(),
        StorageBudgetPool::FrozenMutable,
        frozen_bytes,
        frozen_count,
        active_bytes,
        1,
        "rotation would exceed frozen mutable storage budget",
    )
}

pub(crate) fn estimate_commit_batch_active_bytes(batch: &CommitBatch) -> LifecycleResult<u64> {
    if batch.mutations().is_empty() {
        return Ok(0);
    }
    let mut total = COMMIT_TIMELINE_ACTIVE_BYTE_RESERVE;
    for mutation in batch.mutations() {
        let row = storage_row_for_mutation_estimate(mutation);
        total = add_estimated_row_bytes(total, &row)?;
    }
    Ok(total)
}

pub(crate) fn projected_commit_rotation_would_exceed_frozen_budget(
    ledger: &StorageBudgetLedger,
    branch: &BranchLocalState,
    incoming_active_bytes: u64,
) -> LifecycleResult<bool> {
    if incoming_active_bytes == 0 {
        return Ok(false);
    }
    let current_active = branch.active().approximate_size_bytes() as u64;
    let projected_active = current_active.checked_add(incoming_active_bytes).ok_or(
        LifecycleError::StorageBudgetExceeded {
            pool: StorageBudgetPool::ActiveMutable,
            requested_bytes: incoming_active_bytes,
            used_bytes: current_active,
            limit_bytes: ledger
                .budget()
                .pool_limit_bytes(StorageBudgetPool::ActiveMutable),
            requested_count: 0,
            used_count: 0,
            limit_count: None,
            reason: "active mutable byte accounting overflowed",
        },
    )?;
    if projected_active < branch.config().active_rotation_bytes() as u64
        || branch.frozen().len() >= branch.config().max_frozen_tables()
    {
        return Ok(false);
    }
    let frozen_bytes = frozen_bytes(branch)?;
    let limit = ledger
        .budget()
        .pool_limit_bytes(StorageBudgetPool::FrozenMutable);
    let projected_rotation_bytes = frozen_bytes.checked_add(projected_active).ok_or(
        LifecycleError::StorageBudgetExceeded {
            pool: StorageBudgetPool::FrozenMutable,
            requested_bytes: projected_active,
            used_bytes: frozen_bytes,
            limit_bytes: limit,
            requested_count: 1,
            used_count: u64::try_from(branch.frozen().len()).unwrap_or(u64::MAX),
            limit_count: ledger
                .budget()
                .pool_limit_count(StorageBudgetPool::FrozenMutable),
            reason: "rotation after commit would exceed frozen mutable storage budget",
        },
    )?;
    Ok(projected_rotation_bytes > limit)
}

pub(crate) fn require_maintenance_enqueue_budget(
    ledger: &StorageBudgetLedger,
    maintenance_status: MaintenanceExecutorStatus,
) -> LifecycleResult<()> {
    let used_count = u64::try_from(maintenance_task_count(maintenance_status)).unwrap_or(u64::MAX);
    let used_bytes = used_count.saturating_mul(MAINTENANCE_TASK_METADATA_BYTES);
    require_projected_usage(
        ledger.budget(),
        StorageBudgetPool::MaintenanceQueue,
        used_bytes,
        used_count,
        MAINTENANCE_TASK_METADATA_BYTES,
        1,
        "maintenance queue would exceed storage budget",
    )
}

pub(crate) fn snapshot_with_runtime_usage(
    ledger: &StorageBudgetLedger,
    branch: &BranchLocalState,
    maintenance_status: MaintenanceExecutorStatus,
) -> StorageBudgetSnapshot {
    let active_bytes = branch.active().approximate_size_bytes() as u64;
    let frozen_bytes = frozen_bytes(branch).unwrap_or(u64::MAX);
    let frozen_count = u64::try_from(branch.frozen().len()).unwrap_or(u64::MAX);
    let pending_count =
        u64::try_from(maintenance_task_count(maintenance_status)).unwrap_or(u64::MAX);
    let pending_bytes = pending_count.saturating_mul(MAINTENANCE_TASK_METADATA_BYTES);
    ledger
        .snapshot()
        .with_usage(StorageBudgetPool::ActiveMutable, active_bytes, 0)
        .with_usage(StorageBudgetPool::FrozenMutable, frozen_bytes, frozen_count)
        .with_usage(
            StorageBudgetPool::MaintenanceQueue,
            pending_bytes,
            pending_count,
        )
}

fn maintenance_task_count(status: MaintenanceExecutorStatus) -> usize {
    status.pending_tasks().saturating_add(status.active_tasks())
}

fn estimate_rows_active_bytes(rows: &[crate::row::StorageRow]) -> LifecycleResult<u64> {
    let mut total = 0_u64;
    for row in rows {
        total = add_estimated_row_bytes(total, row)?;
    }
    Ok(total)
}

fn storage_row_for_mutation_estimate(mutation: &CommitMutation) -> StorageRow {
    let estimate_version = CommitVersion::new(1);
    let estimate_timestamp = Timestamp::from_micros(1);
    match mutation {
        CommitMutation::Put {
            key,
            value,
            expires_at,
            ..
        } => StorageRow::put(
            key.clone(),
            estimate_version,
            estimate_timestamp,
            expiry_timestamp_for_estimate(*expires_at),
            value.clone(),
        ),
        CommitMutation::Delete { key } => {
            StorageRow::tombstone(key.clone(), estimate_version, estimate_timestamp)
        }
    }
}

const fn expiry_timestamp_for_estimate(expiry: CommitExpiry) -> Timestamp {
    match expiry {
        CommitExpiry::None => Timestamp::EPOCH,
        CommitExpiry::At(timestamp) => timestamp,
    }
}

fn add_estimated_row_bytes(total: u64, row: &crate::row::StorageRow) -> LifecycleResult<u64> {
    let row_bytes = TableRow::new(row.clone()).approximate_size_bytes() as u64;
    total
        .checked_add(row_bytes)
        .ok_or(LifecycleError::StorageBudgetExceeded {
            pool: StorageBudgetPool::ActiveMutable,
            requested_bytes: row_bytes,
            used_bytes: total,
            limit_bytes: u64::MAX,
            requested_count: 0,
            used_count: 0,
            limit_count: None,
            reason: "commit byte estimate overflowed",
        })
}

fn frozen_bytes(branch: &BranchLocalState) -> LifecycleResult<u64> {
    branch.frozen().iter().try_fold(0_u64, |total, table| {
        total
            .checked_add(table.approximate_size_bytes() as u64)
            .ok_or(LifecycleError::StorageBudgetExceeded {
                pool: StorageBudgetPool::FrozenMutable,
                requested_bytes: table.approximate_size_bytes() as u64,
                used_bytes: total,
                limit_bytes: u64::MAX,
                requested_count: 0,
                used_count: 0,
                limit_count: None,
                reason: "frozen byte accounting overflowed",
            })
    })
}

fn require_projected_usage(
    budget: StorageRuntimeBudget,
    pool: StorageBudgetPool,
    used_bytes: u64,
    used_count: u64,
    requested_bytes: u64,
    requested_count: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    let state = state_with_usage(pool, used_bytes, used_count);
    check_available(
        budget,
        state,
        pool,
        requested_bytes,
        requested_count,
        reason,
    )
}

fn state_with_usage(
    pool: StorageBudgetPool,
    used_bytes: u64,
    used_count: u64,
) -> StorageBudgetCounters {
    let mut state = empty_budget_counters();
    state.0[pool.index()] = used_bytes;
    state.1[pool.index()] = used_count;
    state
}

fn check_available(
    budget: StorageRuntimeBudget,
    state: StorageBudgetCounters,
    pool: StorageBudgetPool,
    requested_bytes: u64,
    requested_count: u64,
    reason: &'static str,
) -> LifecycleResult<()> {
    let index = pool.index();
    let used_bytes = state.0[index];
    let used_count = state.1[index];
    let limit_bytes = budget.pool_limit_bytes(pool);
    let limit_count = budget.pool_limit_count(pool);
    let projected_bytes =
        used_bytes
            .checked_add(requested_bytes)
            .ok_or(LifecycleError::StorageBudgetExceeded {
                pool,
                requested_bytes,
                used_bytes,
                limit_bytes,
                requested_count,
                used_count,
                limit_count,
                reason: "budget byte accounting overflow",
            })?;
    let projected_count =
        used_count
            .checked_add(requested_count)
            .ok_or(LifecycleError::StorageBudgetExceeded {
                pool,
                requested_bytes,
                used_bytes,
                limit_bytes,
                requested_count,
                used_count,
                limit_count,
                reason: "budget count accounting overflow",
            })?;
    if projected_bytes > limit_bytes
        || limit_count.is_some_and(|limit_count| projected_count > limit_count)
    {
        return Err(LifecycleError::StorageBudgetExceeded {
            pool,
            requested_bytes,
            used_bytes,
            limit_bytes,
            requested_count,
            used_count,
            limit_count,
            reason,
        });
    }
    Ok(())
}

fn active_rotation_bytes_from_budget(budget: StorageRuntimeBudget) -> usize {
    usize::try_from(budget.pool_limit_bytes(StorageBudgetPool::ActiveMutable)).unwrap_or(usize::MAX)
}

const fn empty_budget_counters() -> StorageBudgetCounters {
    ([0; POOL_COUNT], [0; POOL_COUNT])
}

fn pressure_severity(usage: StorageBudgetUsage) -> StorageBudgetPressureSeverity {
    if usage.used_bytes == 0 && usage.used_count == 0 {
        return StorageBudgetPressureSeverity::Normal;
    }
    if usage.used_bytes > usage.limit_bytes
        || usage
            .limit_count
            .is_some_and(|limit_count| usage.used_count > limit_count)
    {
        return match usage.pool {
            StorageBudgetPool::ActiveMutable => {
                StorageBudgetPressureSeverity::RejectMutatingAdmission
            }
            StorageBudgetPool::MaintenanceQueue
            | StorageBudgetPool::GeneratedArtifact
            | StorageBudgetPool::ManifestCatalog => {
                StorageBudgetPressureSeverity::RejectOptionalWork
            }
            StorageBudgetPool::BlockCache => StorageBudgetPressureSeverity::Evicting,
            StorageBudgetPool::TableReader | StorageBudgetPool::FrozenMutable => {
                StorageBudgetPressureSeverity::DeferOptionalMaintenance
            }
        };
    }
    if usage.limit_bytes == 0 {
        return StorageBudgetPressureSeverity::Normal;
    }
    let high_water = usage.limit_bytes.saturating_mul(4) / 5;
    if usage.used_bytes >= high_water {
        match usage.pool {
            StorageBudgetPool::BlockCache => StorageBudgetPressureSeverity::Evicting,
            StorageBudgetPool::ActiveMutable => {
                StorageBudgetPressureSeverity::RejectMutatingAdmission
            }
            StorageBudgetPool::MaintenanceQueue
            | StorageBudgetPool::GeneratedArtifact
            | StorageBudgetPool::ManifestCatalog
            | StorageBudgetPool::TableReader
            | StorageBudgetPool::FrozenMutable => {
                StorageBudgetPressureSeverity::DeferOptionalMaintenance
            }
        }
    } else {
        StorageBudgetPressureSeverity::Normal
    }
}

const fn require_nonzero(field: &'static str, value: u64) -> LifecycleResult<()> {
    if value == 0 {
        return Err(LifecycleError::InvalidConfig {
            field,
            reason: "must be nonzero",
        });
    }
    Ok(())
}
