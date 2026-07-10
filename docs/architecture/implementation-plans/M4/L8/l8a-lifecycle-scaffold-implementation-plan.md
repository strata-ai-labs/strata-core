# L8A Implementation Plan: Lifecycle Scaffold

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`

## Objective

Create the storage-next lifecycle scaffold without implementing open, recovery,
maintenance, checkpoint, retention, quarantine, repair, close, or crash
recovery behavior.

L8A establishes:

1. the `crates/storage-next/src/lifecycle/` module shape;
2. lifecycle configuration, error, fact, health, outcome, and result shells;
3. crate-private exports for later L8 slices;
4. source-boundary guards for L8 ownership;
5. a generated-property harness stub for later lifecycle slices;
6. the initial M4-L8 porting-log file and source map.

The repository currently has only a one-line `lifecycle/mod.rs` stub. L8A
turns that placeholder into a reviewable storage-owned module boundary and
leaves behavior to L8B-L8P.

The slice should make L8B-L8P implementation easier without importing engine
open policy, public maintenance commands, follower mode, product DTOs,
primitive recovery semantics, StrataHub behavior, raw filesystem calls, or
lower-layer test helpers into production lifecycle code.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/storage/l7-commit-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
8. `crates/storage-next/src/lifecycle/mod.rs`
9. `crates/storage-next/src/backend/`
10. `crates/storage-next/src/layout/`
11. `crates/storage-next/src/service/`
12. `crates/storage-next/src/branch/`
13. `crates/storage-next/src/commit/`
14. `crates/engine/src/database/open.rs`
15. `crates/engine/src/database/recovery.rs`
16. `crates/engine/src/database/lifecycle.rs`
17. `crates/storage/src/durability/recovery_bootstrap.rs`
18. `crates/storage/src/durability/recovery.rs`
19. `crates/storage/src/durability/checkpoint_runtime.rs`
20. `crates/storage/src/segmented/quarantine_protocol.rs`

## Existing-Code Source Map

| Current file | L8A evidence | L8A action |
|---|---|---|
| `crates/engine/src/database/open.rs` | Open sequencing, support directory setup, WAL writer construction, runtime config application, flush thread start. | Record vocabulary and deferrals only. Open/create behavior lands in L8D/L8E. |
| `crates/engine/src/database/recovery.rs` | Bridge between storage recovery and engine primitive callbacks. | Record that primitive reconstruction and product recovery policy are retired from L8. Recovery orchestration lands in L8F/L8G. |
| `crates/engine/src/database/lifecycle.rs` | Shutdown gates, background drain, writer health, close ordering, lock release. | Reserve lifecycle state/error vocabulary. Close behavior lands in L8N. |
| `crates/engine/src/background.rs` | Background task queue, coalescing, drain, cancellation. | Record scheduler evidence. Deterministic maintenance executor lands in L8H. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Manifest/codec preparation, storage recovery config, lossy fallback evidence. | Reserve recovery health and strictness vocabulary. No recovery implementation in L8A. |
| `crates/storage/src/durability/recovery.rs` | Manifest/snapshot/WAL replay coordination and corruption classification. | Record storage-shaped recovery evidence. Replay orchestration lands in L8F/L8G. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Snapshot publication, manifest watermark update, pruning triggers. | Reserve checkpoint outcome vocabulary. Checkpoint behavior lands in L8J. |
| `crates/storage/src/durability/compaction/wal_only.rs` | WAL truncation proof and watermark evidence. | Reserve retention-proof vocabulary. WAL truncation behavior lands in L8J/L8L. |
| `crates/storage/src/segmented/mod.rs` | Flush, branch-table install, compaction hooks, recovery helpers, version tracking. | Record L6/L8 split. Flush and scheduling land in L8I/L8K. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Quarantine, purge, retention snapshot, repair evidence. | Reserve quarantine/reclaim vocabulary. Behavior lands in L8M. |
| `crates/engine/src/database/refresh.rs` | Follower refresh and blocked transaction watermarks. | Record retired behavior. Follower mode must not enter L8. |

## Scope

L8A implements scaffolding only:

1. lifecycle module submodule declarations;
2. lifecycle config shell;
3. lifecycle error/result type;
4. lifecycle phase and state shells;
5. storage mode vocabulary shell;
6. open plan and open outcome shells;
7. recovery health and recovery fault shells;
8. maintenance task, outcome, and stats shells;
9. retention, quarantine, and close fact shells;
10. crate-private re-exports from `lifecycle/mod.rs`;
11. source guard tests for production `lifecycle/` boundaries;
12. small unit tests for construction, display, source chains, and validation;
13. testkit/property harness placeholders with nonzero scaffold counters;
14. initial `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`
    entry.

L8A does not implement:

1. lifecycle state transitions;
2. backend capability validation;
3. cache-mode open or close;
4. durable local service assembly;
5. writer guard acquisition;
6. manifest load/create/publish;
7. snapshot recovery or publication;
8. WAL replay or tail repair;
9. L7 replay or clock bootstrap;
10. maintenance scheduling;
11. flush, checkpoint, WAL truncation, compaction, or materialization;
12. retention proof, quarantine, purge, or repair;
13. close ordering, drain, quiesce, or sync;
14. public storage API exposure.

## Module Layout

Target initial layout:

```text
crates/storage-next/src/lifecycle/
  mod.rs
  config.rs
  error.rs
  facts.rs
  health.rs
  outcome.rs
  result.rs
  tests.rs
```

The exact split may change during implementation, but L8A should avoid a large
`mod.rs`. The scaffold should leave clear ownership for later slices:

1. `config.rs`: lifecycle limits and mode-independent configuration shells.
2. `error.rs`: typed lifecycle errors and source-chain wrappers.
3. `facts.rs`: lifecycle state, phase, storage mode, capability, maintenance,
   retention, quarantine, and close fact shells.
4. `health.rs`: storage-shaped recovery and lifecycle health shells.
5. `outcome.rs`: open, maintenance, and close outcome shells.
6. `result.rs`: `LifecycleResult<T>` alias and small result helpers.
7. `tests.rs`: module-local scaffold tests.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production types stay
`pub(crate)`.

### `LifecycleConfig`

Suggested fields:

```text
LifecycleConfig {
    max_maintenance_queue_depth: usize,
    max_recovery_faults: usize,
    close_timeout_policy: LifecycleCloseTimeoutPolicy,
    lossy_recovery: LifecycleLossyRecoveryPolicy,
}
```

Rules:

1. defaults must be valid;
2. zero limits that make required lifecycle accounting impossible are rejected;
3. lossy recovery must be explicit and disabled by default;
4. close timeout policy is a storage fact, not product error wording;
5. config must not include filesystem paths, engine options, IPC settings,
   primitive extension lists, StrataHub endpoints, or public maintenance command
   flags.

Use explicit enums instead of boolean control fields.

### `LifecycleError`

Initial variants should cover scaffold and later-slice routes without encoding
behavior prematurely:

```text
LifecycleError::InvalidConfig { field }
LifecycleError::InvalidLifecycleState { reason }
LifecycleError::InvalidOpenPlan { reason }
LifecycleError::CapabilityMismatch { storage_mode, required, missing }
LifecycleError::RecoveryFailed { reason }
LifecycleError::MaintenanceFailed { reason }
LifecycleError::RetentionBlocked { reason }
LifecycleError::CloseFailed { reason }
LifecycleError::TimelineRecoveryMismatch { reason }
LifecycleError::WalTailRepairRejected { reason }
LifecycleError::LowerLayer { layer, source }
```

Rules:

1. displays are bounded;
2. displays use storage lifecycle terms, not product open or user-maintenance
   wording;
3. displays do not include row value bytes, object payload bytes, or product DTO
   names;
4. every variant exposes a stable `code()` string for tests and telemetry;
5. wrapped lower-layer source errors must be preserved through `Error::source()`;
6. follower, IPC, and public command errors are not included in V1.

### `LifecycleState`

Suggested shape:

```text
LifecycleState::New
LifecycleState::Opening
LifecycleState::Recovering
LifecycleState::Open
LifecycleState::Closing
LifecycleState::Closed
LifecycleState::Failed
```

L8A only defines vocabulary. L8B validates transitions.

### `StorageMode`

Suggested shape:

```text
StorageMode::Cache
StorageMode::DurableLocalStandard
StorageMode::DurableLocalAlways
StorageMode::ObjectDurableCandidate
```

Rules:

1. cache mode has no durable recovery claim;
2. durable local standard and always remain distinguishable;
3. object durable candidate is not a production durability claim;
4. follower mode is absent.

### `StorageOpenPlan`

Suggested fields:

```text
StorageOpenPlan {
    storage_mode: StorageMode,
    codec_id: CodecId,
    recovery_policy: RecoveryStrictness,
    lifecycle_config: LifecycleConfig,
}
```

L8A may use placeholder facts rather than concrete lower-layer handles. L8C-L8E
add capability and service assembly details.

Rules:

1. plan validation must reject impossible storage-mode/config combinations;
2. plan must not contain product access mode, IPC settings, follower settings,
   primitive registries, engine subsystem handles, or StrataHub configuration;
3. durable capability validation is not implemented until L8C.

### `StorageOpenOutcome`

Suggested fields:

```text
StorageOpenOutcome {
    mode: StorageMode,
    disposition: StorageOpenDisposition,
    recovered_visible_version: Option<CommitVersion>,
    recovery_health: RecoveryHealth,
    maintenance_ready: bool,
    backend_capabilities: Option<BackendCapabilities>,
    database_id: Option<[u8; 16]>,
    codec_id: Option<String>,
    recovered_max_commit_version: Option<CommitVersion>,
    checkpoint: Option<LifecycleRecoveredCheckpoint>,
    wal: Option<LifecycleRecoveredWal>,
    tables: Option<LifecycleRecoveredTables>,
    quarantine: Option<LifecycleRecoveredQuarantine>,
    bootstrap: Option<LifecycleRecoveryBootstrapReport>,
    stats: LifecycleStats,
}
```

Rules:

1. outcome is raw storage fact reporting;
2. outcome must not say whether product open should succeed for a user;
3. durable facts remain optional until durable open/recovery slices populate
   them;
4. cache outcomes must leave durable recovery fact fields empty.

### `RecoveryHealth`

Suggested shape:

```text
RecoveryHealth::Healthy
RecoveryHealth::Degraded {
    class: RecoveryDegradationClass,
    faults: Vec<RecoveryFault>,
}
RecoveryHealth::Failed {
    fault: RecoveryFault,
}
```

Rules:

1. health facts are storage-shaped;
2. data loss and policy downgrade are distinguishable;
3. telemetry-only degradation is distinguishable from data loss;
4. health displays do not contain product recovery advice.

### `MaintenanceTaskKind`

Suggested shape:

```text
MaintenanceTaskKind::Flush
MaintenanceTaskKind::Checkpoint
MaintenanceTaskKind::WalTruncation
MaintenanceTaskKind::Compaction
MaintenanceTaskKind::Materialization
MaintenanceTaskKind::SnapshotPruning
MaintenanceTaskKind::Retention
MaintenanceTaskKind::Quarantine
MaintenanceTaskKind::Purge
MaintenanceTaskKind::Repair
MaintenanceTaskKind::HealthCollection
```

L8A only defines vocabulary. L8H-L8M implement behavior.

### `LifecycleStats`

Suggested fields:

```text
LifecycleStats {
    open_attempts: usize,
    recovery_faults: usize,
    maintenance_tasks: usize,
    retention_blocks: usize,
    close_attempts: usize,
}
```

L8A can expose default/empty stats only. Counters move in later slices.

## Source Boundary Policy

Production `lifecycle/` code may import:

1. `crate::backend` capability traits and backend error types;
2. `crate::layout` object layout types;
3. `crate::format` durable format error types;
4. `crate::service` durable service types;
5. `crate::table` crate-private table runtime APIs;
6. `crate::branch` crate-private branch runtime APIs;
7. `crate::commit` crate-private commit runtime APIs;
8. `crate::row` storage row types;
9. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
10. standard library error/sync/type utilities.

Production `lifecycle/` code must not import:

1. engine crates;
2. product DTOs or old `strata_core` product payload vocabulary;
3. JSON, graph, vector, search, event, embedding, inference, or intelligence
   modules;
4. StrataHub client/server modules;
5. follower refresh modules;
6. public transaction-session APIs;
7. raw `std::fs`, `std::path`, `File`, mmap, or environment APIs;
8. lower-layer test helpers outside tests or testkit.

Lower layers must not import `crate::lifecycle`.

Production `lifecycle/` code must default to `pub(crate)`. L8A should not add
any crate-root `pub mod lifecycle` export.

## Testkit Scaffold

L8A should add a hidden lifecycle testkit scaffold only if needed for property
tests. The scaffold should remain behind test/testkit surfaces and should not
become an alternate public lifecycle API.

Suggested testkit route:

```text
check_lifecycle_scaffold_contract(script: &[u8]) -> LifecycleScaffoldReport
```

The route should exercise:

1. valid config construction;
2. invalid config rejection;
3. storage mode construction;
4. lifecycle state construction;
5. open plan construction;
6. recovery health construction;
7. maintenance task vocabulary construction;
8. error display checks;
9. error source-chain checks;
10. stats default checks.

Later slices should replace or extend scaffold counters with real lifecycle
operation counters.

## Porting Log

Create `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`
before behavior lands.

The `M4-L8A` entry must record:

1. current files read;
2. behavior preserved as vocabulary/source evidence;
3. behavior intentionally changed;
4. behavior retired from V1, especially follower mode and public maintenance
   commands;
5. behavior deferred by owner slice;
6. tests and guards added;
7. sensitivity probes planned or run.

The entry should not claim open, recovery, checkpoint, maintenance, retention,
quarantine, repair, or close behavior is implemented.

## Implementation Steps

1. Create the `M4/L8` slice docs and porting-log file.
2. Expand `crates/storage-next/src/lifecycle/mod.rs` into the target module
   layout.
3. Add `LifecycleConfig` and explicit policy enums.
4. Add `LifecycleError`, `LifecycleLowerLayer`, and `LifecycleResult<T>`.
5. Add fact shells for state, mode, open plans, health, maintenance, retention,
   quarantine, close, and stats.
6. Add validation only where it is construction-level and behavior-free.
7. Add module-local scaffold tests.
8. Add lifecycle source guard tests with fixture self-tests.
9. Add generated scaffold contract and property test if testkit is already the
   repo pattern for the layer.
10. Add the L8A porting-log entry.
11. Run the L8A command matrix.

## Deferred Behavior Map

Owned by later L8 slices:

1. lifecycle transition validation: L8B;
2. backend capability validation: L8C;
3. cache open/close: L8D;
4. durable service assembly: L8E;
5. recovery orchestration: L8F;
6. L7 replay/bootstrap and recovery health finalization: L8G;
7. maintenance executor: L8H;
8. flush/table publication: L8I;
9. checkpoint/WAL truncation: L8J;
10. compaction/materialization scheduling: L8K;
11. retention/snapshot pruning: L8L;
12. quarantine/reclaim/purge/repair: L8M;
13. close/shutdown ordering: L8N;
14. generated/fault/crash assurance: L8O;
15. closeout: L8P.

Owned above L8 or post-V1:

1. public open policy;
2. IPC;
3. follower mode;
4. public manual maintenance commands;
5. primitive snapshot materialization;
6. product recovery wording;
7. StrataHub fleet behavior;
8. production object-store durability.

## Exit Gate

L8A is complete when:

1. lifecycle module scaffold compiles under default, no-default, all-features,
   and wasm/no-default where applicable;
2. config, error, fact, health, outcome, result, and stats shells have direct
   tests;
3. source guards reject engine, product, StrataHub, follower, public command,
   raw filesystem, and lower-layer upward imports;
4. generated scaffold route exposes nonzero counters for every scaffold category
   if added;
5. porting log records the L8A source map, deferrals, and probes;
6. no open, recovery, maintenance, checkpoint, retention, quarantine, repair, or
   close behavior was implemented ahead of its owning slice.
