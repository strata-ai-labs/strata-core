# L8W Implementation Plan: Memory And Cache Budget Enforcement

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md`

## Objective

Make storage-owned memory consumption explicit, bounded, observable, and
testable.

Storage-next must work on small embedded targets as well as developer machines.
The runtime cannot rely on hidden process-global caches, host-memory
auto-detection, or unbounded table/artifact loading. L8W introduces resolved
storage memory budgets and accounting for block cache, table readers,
active/frozen branch state, maintenance queues, generated table artifacts, and
manifest/catalog metadata.

The slice is about storage mechanics, not product policy. The engine/resource
planner may later decide a Raspberry Pi Zero-style profile or a desktop profile
and pass explicit values into storage. L8W consumes those values, enforces
storage limits, and reports raw pressure facts.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
4. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md`
10. `crates/storage-next/src/table/cache.rs`
11. `crates/storage-next/src/table/reader.rs`
12. `crates/storage-next/src/table/builder.rs`
13. `crates/storage-next/src/table/compaction.rs`
14. `crates/storage-next/src/branch/state.rs`
15. `crates/storage-next/src/lifecycle/cache.rs`
16. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
17. `crates/storage-next/src/lifecycle/maintenance.rs`
18. `crates/storage-next/src/lifecycle/outcome.rs`
19. `crates/storage/src/block_cache.rs`
20. `crates/storage/src/compaction.rs`

## Existing-Code Source Map

| Current file | Evidence | L8W action |
|---|---|---|
| `table/cache.rs` | Storage-next already has table cache mechanics and stats, but the budget owner is not yet lifecycle-wide. | Bind table/block cache capacity to a database-local runtime budget and expose pressure facts. |
| `table/reader.rs` | Current readers can open whole byte sources. | Add reader admission by byte count and live reader reservation. L8X later replaces whole-object reads with lazy block reads. |
| `table/builder.rs` | Built table artifacts hold encoded bytes in memory before publication. | Reserve generated-artifact budget before build/publish/compaction output accumulation. |
| `table/compaction.rs` | Reports output bytes and approximates pending output size. | Check output artifact and pending-output budgets while preserving L5 compaction ownership. |
| `branch/state.rs` | Tracks active rows, approximate active bytes, frozen tables, and table counts. | Add branch-state budget facts and admission for active append, rotate, freeze, and install. |
| `lifecycle/maintenance.rs` | Queue and active task state can grow with scheduled work. | Add queue/task budget dimensions and reject or defer work before allocation. |
| `lifecycle/outcome.rs` | Open/maintenance/close outcomes carry raw stats and health. | Surface selected budget, current usage, pressure, and rejection facts. |
| `lifecycle/cache.rs` | Cache mode runs entirely in memory. | Enforce the same active/frozen/maintenance budgets without durable-service claims. |

## Old Codebase Porting Map

The old engine's most relevant memory feature is the block cache: explicit
capacity, zero-capacity disablement, pinned bytes, stats, and bounded eviction
effort. L8W ports those properties into storage-next's database-local budget
model.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `storage/src/block_cache.rs::BlockCache::new` | Cache has an explicit byte capacity and can be zero. | Use explicit per-database cache budget. No hidden process-global default. | Zero-capacity cache stores nothing; nonzero cache respects capacity. |
| `BlockCacheStats` | Reports hits, misses, entries, size, capacity, pinned bytes. | Expose raw table cache and budget stats in lifecycle outcomes/metrics. | Stats reflect usage and pressure after reads/evictions. |
| CLOCK eviction | Eviction has bounded effort and can skip caching rather than spin. | Preserve bounded eviction/admission. Avoid unbounded scans under pressure. | Eviction test proves effort cap and uncached fallback. |
| Pinned priority | Pinned blocks consume separate pinned bytes and resist eviction. | Track pinned-reader/cache reservations separately and prevent pinned overcommit. | Pinned usage blocks over-budget operations instead of evicting pinned data. |
| `set_global_capacity` / `global_cache` | Process-global cache caused concurrent-open and test-isolation ambiguity. | Do not port hidden globals. Runtime cache is database-local or explicitly shared later. | Two runtime instances have isolated budgets unless a shared cache is explicit. |
| `auto_detect_capacity` | Old code inspected host memory and used quarter-of-available with no minimum clamp. | Do not auto-detect in storage-next. Resource planner passes resolved values. Keep no-minimum-clamp behavior in profile tests. | Low-memory profile accepts small/zero cache without clamping upward. |
| `test_issue_1735_no_minimum_cache_clamp` | Raspberry Pi-style available memory must not become a large hidden minimum. | Profile smoke uses explicit small budgets and proves storage honors them. | Small profile does not inflate cache or artifact budgets. |

Do not port:

1. host RAM probing;
2. `/proc/meminfo` parsing;
3. process-global cache singleton;
4. local path hashes as table-cache identity;
5. product resource-profile selection;
6. public write-stall policy or user-facing error wording.

## Scope

L8W implements:

1. resolved storage runtime budget types;
2. database-local budget ledger and scoped reservations;
3. table/block cache capacity and eviction accounting;
4. active mutable table byte accounting and admission;
5. frozen table byte/count accounting and rotation admission;
6. table reader admission and live reader reservations;
7. generated artifact budget for flush, compaction, materialization, checkpoint,
   and recovery decode outputs;
8. maintenance queue/task budget;
9. manifest/catalog metadata budget;
10. raw pressure facts and typed budget rejection errors;
11. low-memory profile test fixture;
12. source guards preventing host-memory probing, process globals, product
    policy imports, and milestone labels in Rust code/test names/fixture bytes.

L8W does not implement:

1. lazy object-backed range reads;
2. block-level table reader format changes;
3. public resource-profile selection;
4. automatic device classification;
5. object-store production durability;
6. row pruning policy;
7. branch lifecycle completion;
8. product write-stall UX.

## Budget Model

Suggested shape:

```rust
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

pub(crate) enum StorageBudgetPool {
    BlockCache,
    TableReader,
    ActiveMutable,
    FrozenMutable,
    MaintenanceQueue,
    GeneratedArtifact,
    ManifestCatalog,
}

pub(crate) struct StorageBudgetReservation {
    pool: StorageBudgetPool,
    bytes: u64,
}
```

Exact names can change. Required properties:

1. every budget is explicit;
2. zero means disabled for optional pools, not "use default";
3. mandatory pools can reject zero with a typed config error;
4. reservations are RAII-style and release on every error path;
5. usage and limits are queryable without parsing display strings;
6. pressure facts are raw storage facts, not product policy.

## Budget Dimensions

### Block Cache

Rules:

1. Cache capacity is database-local by default.
2. Zero cache capacity disables storage in the cache while still allowing reads.
3. A block larger than cache capacity is served uncached.
4. Shrinking capacity triggers bounded eviction or pressure; it must not scan
   unboundedly.
5. Pinned entries count against pinned/cache usage and cannot be silently
   evicted.
6. Cache keys use table identity plus block/range address, not filesystem path.

### Table Readers

Rules:

1. Whole-object readers reserve byte count before opening.
2. Opening a reader above `table_reader_bytes` fails with a typed budget error.
3. Reader reservations release on drop and on open failure.
4. L8X can later replace whole-object reservations with block/range
   reservations without changing lifecycle budget facts.

### Active And Frozen Branch State

Rules:

1. L7 append/admission checks active mutable bytes before mutating L6.
2. Rotation checks frozen count and frozen byte budget.
3. Flush releases frozen bytes only after L6 install succeeds.
4. Failed flush/compaction/materialization cannot leak reservations.
5. Cache mode and durable mode use the same in-memory accounting.

### Generated Artifacts

Rules:

1. Table build/compaction/materialization/checkpoint encoding reserves expected
   artifact bytes before allocation when the estimate is available.
2. If exact size is known only after encoding, the runtime must reconcile and
   reject over-budget output before publication/install.
3. Partial output generation releases budget on every failure path.
4. Large outputs can be deferred with pressure facts instead of forcing OOM.

### Maintenance

Rules:

1. Queue entries consume task count and approximate metadata bytes.
2. Active tasks consume active task budget until completion/cancel/failure.
3. Coalescing happens before allocating a duplicate task reservation.
4. Close cancellation/drain releases reservations.
5. Rejected tasks report pool, requested bytes/count, and current usage.

### Manifest And Catalog Metadata

Rules:

1. Recovered manifests, table catalogs, quarantine inventories, and retention
   graphs consume metadata budget.
2. Decode must reject unbounded counts before allocating large vectors.
3. Corrupt/future manifest data must fail closed before allocation.
4. Metadata pressure blocks optional maintenance before it blocks recovery.

## Runtime Profiles

L8W may add named test fixtures such as:

1. `minimal_test_profile`;
2. `low_memory_embedded_profile`;
3. `default_test_profile`;
4. `large_test_profile`.

These are storage test fixtures, not product defaults. Production storage must
consume resolved numeric budgets. It must not inspect host RAM, CPU count,
device model, OS, or environment variables to choose a profile.

Low-memory profile goals:

1. block cache can be zero or very small;
2. generated artifact budget is small enough to force deferred compaction;
3. maintenance queue is bounded;
4. table reader admission rejects large whole-object reads until L8X lazy reads
   are available;
5. normal small commits, flush, checkpoint, and close still work.

## Pressure And Admission

Pressure is a storage fact. Product policy lives above storage.

Suggested shape:

```rust
pub(crate) enum StoragePressureSeverity {
    Normal,
    Evicting,
    DeferOptionalMaintenance,
    RejectOptionalWork,
    RejectMutatingAdmission,
}

pub(crate) struct StorageBudgetPressure {
    pool: StorageBudgetPool,
    used_bytes: u64,
    limit_bytes: u64,
    severity: StoragePressureSeverity,
}
```

Rules:

1. Optional work can be deferred before mandatory work is rejected.
2. Mutating admission may be rejected only by a typed storage budget error.
3. Pressure facts must include pool and limit/usage values.
4. No test should assert on product phrases such as "write stall" or "low
   memory mode".

## Error And Health Vocabulary

Add typed errors for:

1. invalid storage budget config;
2. budget pool over limit;
3. block cache disabled;
4. reader budget exceeded;
5. active mutable budget exceeded;
6. frozen mutable budget exceeded;
7. generated artifact budget exceeded;
8. maintenance queue budget exceeded;
9. manifest/catalog budget exceeded;
10. reservation release mismatch;
11. budget accounting overflow.

Every error must expose a stable code and preserve source chains.

## Source Boundaries

L8W may import:

1. L5 cache/reader/builder facts;
2. L6 branch memory facts;
3. L8 lifecycle state, maintenance, and outcome types;
4. L4 object byte counts and manifest section counts.

L8W must not import:

1. `std::fs` or host-memory probes;
2. `/proc/meminfo` parsing;
3. `sysinfo` or platform resource inspection crates;
4. product/engine resource profile modules;
5. backend delete/quarantine/purge APIs;
6. StrataHub code;
7. primitive/query/vector/graph modules.

Rust code, test names, fixture bytes, and user-facing error strings must not
include milestone labels.

## Implementation Steps

1. Define `StorageRuntimeBudget`, budget pools, pressure facts, and typed
   errors.
2. Add budget config validation and profile test fixtures.
3. Add database-local budget ledger with RAII reservations.
4. Wire block/table cache capacity to the budget.
5. Add table reader admission and release accounting.
6. Add branch active/frozen memory accounting and admission hooks.
7. Add generated artifact reservations around flush, compaction,
   materialization, checkpoint, and recovery decode.
8. Add maintenance queue/task budget reservations.
9. Add manifest/catalog metadata budget checks before allocation.
10. Thread budget facts into open, maintenance, pressure, and close outcomes.
11. Add direct, generated, source-guard, and porting-log coverage.

## Deferred Behavior

Deferred to L8X:

1. lazy object-backed table reads;
2. block-range reader reservations;
3. block-cache integration for large object reads.

Deferred to L9:

1. public resource-profile configuration;
2. product write-stall policy;
3. CLI/diagnostic rendering of budget facts.

Deferred to future object-store work:

1. provider-local shared cache budgets;
2. remote read-ahead and multipart buffering.

## Exit Gate

L8W is complete when:

1. every storage-owned memory pool has an explicit limit and usage fact;
2. zero/small cache budgets do not allocate hidden defaults;
3. table readers and generated artifacts are admitted or rejected by budget;
4. active/frozen branch state respects byte/count limits;
5. maintenance queues and active tasks are bounded;
6. low-memory profile smoke proves ordinary storage operations work without
   unbounded allocation;
7. pressure facts are raw storage facts and contain pool/usage/limit;
8. source guards block host-memory probing, hidden globals, product imports,
   raw deletion, and milestone labels in Rust code;
9. generated tests cover budget admission, release-on-failure, and pressure.
