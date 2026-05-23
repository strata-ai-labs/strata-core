# M4-L8 Porting Log

Status: active

Parent plans:

- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

Closeout status for the L8A-L8G cleanup pass:

- Resolved in code: clippy exit gate, bootstrap failure -> `Failed`, L8G
  bootstrap file boundary, checkpoint-boundary replay idempotence, lossy
  health classification, strict WAL-tail rejection, quarantine/table validation
  before WAL repair, cache open admission, idempotent close facts, capability
  required/missing facts, structured open outcome facts, stable error codes,
  checkpoint timestamp guard catch-up, timeline mismatch mapping, and positive
  capability-order/source-guard tests.
- Explicitly deferred: exhaustive crash/fault/fuzz closeout, localfs recovery
  integration expansion, full maintenance/retention/quarantine/repair outcomes,
  durable close drain/sync outcomes, and the remaining named L8F/L8G/L8D/L8E
  matrix rows that require later L8H-L8P machinery.

## L8A - Lifecycle Scaffold

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/commit/config.rs`
- `crates/storage-next/src/commit/error.rs`
- `crates/storage-next/src/commit/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/commit_runtime_properties.rs`
- `crates/storage-next/tests/commit_runtime_source_guard.rs`
- `crates/storage-next/tests/common/mod.rs`
- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/result.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Lifecycle state vocabulary: new, opening, recovering, open, closing, closed,
  and failed.
- Storage mode vocabulary: cache, durable local standard, durable local always,
  and object durable candidate.
- Storage open plan and open outcome facts.
- Recovery health shape: healthy, degraded, and failed.
- Recovery fault categories for manifest, snapshot, WAL, table object, inherited
  layer, IO, quarantine inventory, and timeline mismatch facts.
- Maintenance task vocabulary for flush, checkpoint, WAL truncation,
  compaction, materialization, snapshot pruning, retention, quarantine, purge,
  repair, and health collection.
- Retention, quarantine, and close fact shells.
- Lower-layer source-chain preservation through `LifecycleError::source()` and
  stable `LifecycleError::code()` strings.

### Raw Health And Fact Vocabulary

- Lifecycle enums are marked `#[non_exhaustive]` so later L8/L9 slices can add
  fields or variants without changing current call sites by accident.
- `StorageOpenOutcome` now carries backend capabilities, database id, codec id,
  recovered max commit version, checkpoint/WAL/table/quarantine recovery facts,
  L7 bootstrap facts, and raw `LifecycleStats` in addition to the original mode,
  disposition, recovered visible version, health, and maintenance-ready facts.
- `MaintenanceOutcome` and `CloseOutcome` have reserved structured facts for
  later maintenance/close slices: recovery health, affected-object counts,
  reclaimed bytes, retryability, close fact, close effects, and raw stats.
- `LifecycleError::CapabilityMismatch` carries both required and missing
  backend capabilities; timeline replay mismatches and strict WAL-tail repair
  rejections have dedicated lifecycle error codes.

### Intentional Changes

- The scaffold is crate-private. The crate root remains `mod lifecycle;`.
- Config uses explicit enums for close timeout and lossy recovery policy.
- Lossy recovery is disabled by default and must be explicit before an open plan
  can request lossy fallback.
- Cache-mode open plans cannot request durable recovery fallback.
- Cache-mode open outcomes cannot claim a recovered durable visible version.
- The generated lifecycle property route is a scaffold contract only; it does
  not open storage or mutate lower layers.

### Retired From V1 L8

- Product open policy and public open wording.
- Public maintenance command vocabulary.
- Primitive reconstruction callbacks.
- Product recovery advice.
- Follower refresh behavior.
- IPC or multi-process product behavior.
- StrataHub behavior.
- Product value, graph, vector, search, embedding, or inference DTOs.

### Deferred By Owner Slice

- Lifecycle state transition validation: L8B.
- Backend and service capability validation: L8C.
- Cache-mode open and close baseline: L8D.
- Durable local open/create service assembly: L8E.
- Recovery orchestration and L7 replay/bootstrap: L8F-L8G.
- Maintenance executor and task queue behavior: L8H.
- Flush, checkpoint, WAL truncation, compaction, and materialization scheduling:
  L8I-L8K.
- Retention, quarantine, purge, and repair orchestration: L8L-L8M.
- Close ordering, drain, sync, and guard release: L8N.
- Fault, crash, fuzz, sensitivity, and closeout inventory: L8O-L8P.

### Tests Added

- Module-local scaffold tests in `src/lifecycle/tests/mod.rs`.
- Source-boundary guard in `tests/lifecycle_source_guard.rs`.
- Generated scaffold property harness in `tests/lifecycle_properties.rs`.
- Hidden testkit route `check_lifecycle_scaffold_contract`.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Product/engine import | Import engine, product, StrataHub, or follower modules in lifecycle source | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Raw filesystem/env access | Import `std::fs`, `Path`, `File`, mmap, `OpenOptions`, or `std::env` | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Lower layer imports lifecycle | Import `crate::lifecycle` from backend, format, service, table, branch, or commit source | `lower_layers_do_not_import_lifecycle_upward` |
| Public lifecycle surface | Add unscoped `pub` item in lifecycle source | `lifecycle_stays_crate_private` |
| Config zero limit | Accept zero config limits | `lifecycle_config_rejects_zero_limits` and generated lifecycle properties |
| Error code collapse | Remove stable lifecycle error code mapping | `lifecycle_error_display_and_source_chain_are_typed` |
| Source-chain collapse | Drop lower-layer source from `LifecycleError` | `lifecycle_error_display_and_source_chain_are_typed` |

### Verification

Commands to run for L8A:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8B - Lifecycle State And Open Plan

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/lifecycle/tests/state.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`

### Preserved As Storage Vocabulary

- Side-effect-free lifecycle state transitions for new, opening, recovering,
  open, closing, closed, and failed.
- Transition triggers for open requested, cache ready, durable recovery needed,
  recovery accepted, close requested, close completed, close retried, and phase
  failure.
- Operation admission facts for open, ordinary read, commit, recovery step,
  ordinary maintenance, close-required drain, health query, close, and close
  retry.
- Failure facts that preserve the failed storage phase and reason.
- Close facts that distinguish requested, retry-pending, complete, and
  already-closed idempotence.
- Storage open disposition facts for created vs opened-existing outcomes.

### Intentional Changes

- `StorageOpenOutcome` now stores `StorageOpenDisposition` instead of a raw
  boolean while keeping the derived `opened_existing()` getter.
- Cache-mode open outcomes reject durable recovery degradation as well as
  recovered durable visible versions.
- State transition validation is centralized in `lifecycle/state.rs`; invalid
  transitions return `LifecycleError::InvalidLifecycleState` without mutating
  machine state.
- Closed close retry is the only idempotent state transition in L8B.
- Closed close and closed close retry are explicitly admitted as idempotent
  operations; closing close retry remains retryable but not complete.
- Direct lifecycle tests were split into `src/lifecycle/tests/mod.rs` and
  `src/lifecycle/tests/state.rs` before the file grew past the local
  maintainability threshold.

### Retired From V1 L8B

- Raw public open policy booleans in storage open outcome facts.
- Any product API, engine handle, StrataHub, follower, or public maintenance
  vocabulary in lifecycle state/admission code.
- Any backend, service, WAL, manifest, snapshot, branch, commit, maintenance, or
  close side effects in the L8B state layer.

### Deferred By Owner Slice

- Backend and service capability validation: L8C.
- Cache-mode runtime open and close baseline: L8D.
- Durable service assembly: L8E.
- Recovery orchestration, WAL replay, and L7 replay/bootstrap: L8F-L8G.
- Maintenance executor and task queue execution: L8H.
- Close drain, durable sync, and guard release side effects: L8N.
- Cross-slice fault, crash, fuzz, and closeout inventory: L8O-L8P.

### Tests Added

- `lifecycle_state_machine_initial_state_admits_only_open_and_health`
- `lifecycle_state_machine_accepts_open_and_recovery_transitions`
- `lifecycle_state_machine_accepts_close_and_retry_transitions`
- `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state`
- `lifecycle_operation_admission_matrix_is_state_specific`
- `lifecycle_failure_facts_preserve_phase_and_reject_empty_reasons`
- `lifecycle_close_retry_and_closed_idempotence_are_distinct`
- Open-outcome validation coverage for cache durable recovery degradation.
- Generated lifecycle scaffold counters for valid transitions, invalid
  transitions, admission accepts, admission rejects, close retry,
  closed-idempotence, failed-state stickiness, and input-derived state routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Transition skip | Allow `New + CacheOpenReady -> Open` | `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state` |
| Recovery exposure | Allow ordinary read in `Recovering` | `lifecycle_operation_admission_matrix_is_state_specific` |
| Commit outside open | Allow commit in `Opening` or `Closing` | `lifecycle_operation_admission_matrix_is_state_specific` |
| Close false success | Treat `Closing + CloseRetried` as `Closed` | `lifecycle_close_retry_and_closed_idempotence_are_distinct` |
| Failed-state loosened | Allow open or close retry in `Failed` | `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state` |
| Empty failure reason | Accept `PhaseFailed { reason: "" }` | `lifecycle_failure_facts_preserve_phase_and_reject_empty_reasons` |
| Cache degraded recovery | Accept degraded recovery health in cache mode | `storage_open_outcome_rejects_cache_durable_recovery_claims` |

### Verification

Commands to run for L8B:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8C - Storage Mode Capability Validation

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/config/mode.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/capability.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/capability.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Lifecycle storage modes map to the existing `StorageModeRequest` capability
  checks instead of duplicating backend capability matrices.
- Cache mode accepts browser-like object capabilities without requiring object
  metadata or durable primitives.
- Durable local standard and durable local always share durable backend
  requirements while preserving `DurabilityPolicy::Standard` vs
  `DurabilityPolicy::Always` for later open/runtime wiring.
- Object-durable candidate remains candidate-tagged and accepts either
  `ConditionalPublish` or `ConditionalCreate + ConditionalUpdate` fencing.
- Capability mismatch is a typed lifecycle error carrying the requested storage
  mode, complete required `BackendCapability` list, and exact missing
  capability list.

### Intentional Changes

- Added `validate_storage_mode_capabilities(plan, capabilities)` for pure
  capability-fact validation.
- Added `validate_backend_capabilities_for_open(plan, backend)`, which calls
  only `backend.capabilities()`.
- Added `LifecycleCapabilityOutcome` and `ObjectDurableFenceMode` as
  crate-private lifecycle facts.
- Added display names for lifecycle `StorageMode` so capability errors remain
  bounded and storage-shaped.
- Split the generated lifecycle testkit into `lifecycle/mod.rs`,
  `lifecycle/outcome.rs`, and `lifecycle/capability.rs`.

### Retired From V1 L8C

- Ad hoc lifecycle capability strings.
- Capability validation that constructs services, opens manifests, opens WALs,
  acquires writer locks, or mutates L6/L7 state.
- Product open wording in capability mismatch errors.

### Deferred By Owner Slice

- Cache-mode runtime open and close: L8D.
- Durable service assembly and writer-lock acquisition: L8E.
- Recovery orchestration, WAL replay, and L7 bootstrap: L8F-L8G.
- Maintenance execution, retention, quarantine, repair, and close side effects:
  L8H-L8P.
- Production object-durable mode claims beyond candidate capability validation:
  post-V1 object durability design.

### Tests Added

- `capability_validation_maps_lifecycle_modes_to_storage_mode_requests`
- `cache_capability_validation_accepts_browser_like_backend_without_metadata`
- `durable_local_modes_reject_each_missing_durable_capability`
- `object_candidate_accepts_either_publish_fence_or_create_update_pair`
- `object_candidate_reports_base_and_partial_fence_missing_capabilities`
- `cache_capability_validation_never_requires_durable_storage_capabilities`
- `backend_capability_preflight_reads_only_capabilities`
- `lifecycle_capability_validator_stays_preflight_only`
- Generated lifecycle counters for accepted/rejected capability cases, per-mode
  capability cases, missing-capability categories, object-candidate fence
  variants, backend preflight across every mode, and input-derived capability
  masks.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Cache over-requires metadata | Add `ObjectMetadata` to cache requirements | `cache_capability_validation_accepts_browser_like_backend_without_metadata` |
| Durable under-requires append | Remove `AppendObject` from durable requirements | `durable_local_modes_reject_each_missing_durable_capability` |
| Durable policy collapse | Map durable always to standard policy | `capability_validation_maps_lifecycle_modes_to_storage_mode_requests` |
| Object fence missing | Accept object candidate without any fence | `object_candidate_reports_base_and_partial_fence_missing_capabilities` |
| Fence preference drift | Prefer create/update when conditional publish is also present | `object_candidate_accepts_either_publish_fence_or_create_update_pair` |
| Preflight side effect | Call read/list/write/publish/append/lock during validation | `backend_capability_preflight_reads_only_capabilities` |
| Untyped mismatch | Report only a string reason for capability mismatch | `object_candidate_reports_base_and_partial_fence_missing_capabilities` |

### Verification

Commands run for L8C:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8D - Cache Open And Close

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/commit/cache.rs`
- `crates/storage-next/src/commit/branch_registry.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/engine/src/database/recovery.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/cache.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/cache.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`

### Preserved As Storage Vocabulary

- Cache mode opens as volatile storage state only.
- Cache open composes existing L6 `BranchLocalState` and L7
  `CommitCacheRuntime` rather than adding a bespoke row store or commit path.
- Cache open runs L8C backend capability preflight before assembling branch or
  commit runtime state.
- Cache open reports `StorageMode::Cache`, `StorageOpenDisposition::Created`,
  healthy recovery, no recovered durable visible version, and maintenance not
  ready.
- Cache close is a lifecycle state transition and does not perform durable
  shutdown work.

### Intentional Changes

- Added `LifecycleCacheOpenRequest` to carry the cache open plan, initial
  branch id, and branch generation as storage facts.
- Added `LifecycleCacheRuntime<S>` for crate-private volatile cache runtime
  state.
- Added cache commit execution by constructing short-lived `CommitCacheRuntime`
  instances over owned L6/L7 state.
- Added cache read-view access through `BranchLocalState::capture_read_view`.
- Added idempotent cache close using the L8B state machine.
- Extended generated lifecycle counters with cache open, close, commit/read,
  durable-absence, and reopen-empty categories.
- Added a source guard that keeps `lifecycle/cache.rs` out of durable service,
  layout, format, object publication, sync, append, and writer-lock APIs.

### Retired From V1 L8D

- Cache recovery from backend object inventory.
- Cache-mode manifest, WAL, snapshot, table-object, and quarantine service
  construction.
- Cache-mode writer-lock acquisition or release.
- Product open/close, freeze-hook, follower, IPC, primitive, or StrataHub
  behavior.

### Deferred By Owner Slice

- Durable local service assembly and writer-lock acquisition: L8E.
- Durable recovery orchestration: L8F.
- L7 replay/bootstrap from durable facts: L8G.
- Maintenance scheduling, flush, checkpoint, compaction, retention, quarantine,
  repair, and full durable close: later L8 slices.
- Public storage open/read/commit API wrapping: L9.

### Tests Added

- `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims`
- `cache_open_rejects_non_cache_plan_before_backend_preflight`
- `cache_open_request_validation_rejects_invalid_plan_shapes`
- `cache_open_runs_capability_preflight_without_backend_side_effects`
- `cache_runtime_executes_cache_commit_and_reads_through_l6`
- `cache_runtime_generated_timestamp_proves_zero_allocator_and_empty_timestamp_guard`
- `cache_runtime_rejects_wrong_mode_batch_and_preserves_state`
- `cache_runtime_rejects_read_only_wrong_branch_stale_generation_and_conflict`
- `cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls`
- `cache_close_without_commits_completes_and_preserves_diagnostic_facts`
- `cache_reopen_starts_empty_even_when_prior_runtime_committed_rows`
- `lifecycle_cache_runtime_stays_cache_only`
- Generated lifecycle counters for cache open accepted/rejected, cache baseline,
  durable absence, commit/read smoke, close, close idempotence,
  commit-after-close rejection, reopen-empty, and input-derived cache operation
  routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Skip cache capability preflight | Construct runtime before `validate_backend_capabilities_for_open` | `cache_open_runs_capability_preflight_without_backend_side_effects` |
| Backend side effect during cache open | Call `list_prefix`, `read_object`, or writer-lock APIs | `cache_open_runs_capability_preflight_without_backend_side_effects` |
| Cache durable recovery claim | Report recovered visible version or degraded health | `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims` |
| Durable service import | Import WAL/manifest/snapshot/table/quarantine services in `lifecycle/cache.rs` | `lifecycle_cache_runtime_stays_cache_only` |
| Nonzero cache baseline | Start visible tracker above `CommitVersion::ZERO` | `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims` |
| Persistent cache reopen | Reuse prior volatile branch rows on reopen | `cache_reopen_starts_empty_even_when_prior_runtime_committed_rows` |
| Post-close mutation | Allow commit or ordinary read after close | `cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls` |
| Manual cache stamping | Bypass `CommitCacheRuntime` for user rows or timeline rows | `cache_runtime_executes_cache_commit_and_reads_through_l6` |

### Verification

Commands run for L8D:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8E - Durable Open/Create Service Assembly

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/format/manifest.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/sidecar.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/checkpoint.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/branch_registry.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/recovery.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/durable.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/testkit/lifecycle/durable.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`

### Preserved As Storage Vocabulary

- Durable local standard and durable local always assemble the same durable L4
  service bundle while preserving `DurabilityPolicy::Standard` vs
  `DurabilityPolicy::Always`.
- Durable assembly runs L8C capability preflight before writer-lock acquisition
  or durable object access.
- Durable assembly acquires the backend writer guard through
  `ObjectLayout::writer_lock()` and owns the guard in the returned shell.
- Missing database manifest creates an initial durable manifest.
- Existing database manifest loads without replacement and preserves snapshot,
  snapshot-id, flush, and active-WAL recovery facts.
- WAL opens on the manifest active segment.
- The returned durable shell stays in `LifecycleState::Recovering`; ordinary
  reads, commits, and ordinary maintenance are not admitted before L8F/L8G
  finish recovery.

### Intentional Changes

- Added `LifecycleDurableLocalOpenRequest` for durable assembly inputs.
- Added `LifecycleDurableAssemblyFacts` for manifest, writer-lock,
  active-WAL, and durability policy facts.
- Added `LifecycleDurableLocalServices<'a>` as the crate-private L4 service
  bundle.
- Added `LifecycleDurableLocalShell<'a, S>` as the recovery-stage shell.
- Made `WalServiceConfig::validate` crate-visible so lifecycle can reject
  invalid WAL config before publishing an initial manifest.
- Extended generated lifecycle counters with durable standard/always assembly,
  durable rejection, manifest create/open, manifest create-race, manifest
  publish-fault, WAL-open failure, lock failure, identity mismatch,
  recovering-state admission, no-recovery side effects, and input-derived
  durable-mode routes.
- Added a source guard that keeps `lifecycle/durable.rs` to assembly work and
  blocks hardcoded writer-lock names, WAL record replay, checkpoint execution,
  and quarantine/recovery calls in this slice.

### Retired From V1 L8E

- Product open policy, registry wiring, primitive reconstruction, IPC, and
  external synchronization behavior.
- Object-durable candidate production open.
- Read-only/follower durable open.
- Background WAL sync thread startup.

### Deferred By Owner Slice

- Snapshot, table, WAL-tail, and quarantine recovery orchestration: L8F.
- L7 replay, allocator catch-up, visible-version restore, timeline validation,
  and final `StorageOpenOutcome`: L8G.
- Maintenance scheduling, flush, checkpoint, WAL truncation, compaction,
  materialization, retention, quarantine mutation, purge, and repair: later L8
  slices.
- Durable close drain, final sync, and explicit writer-guard release: L8N.
- Public open/read/commit wrapping: L9.

### Tests Added

- `durable_assembly_creates_manifest_opens_wal_and_remains_recovering`
- `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts`
- `durable_request_rejects_non_durable_modes_without_backend_calls`
- `durable_request_rejects_codec_mismatch_before_backend_calls`
- `durable_request_rejects_invalid_wal_config_before_backend_calls`
- `durable_capability_rejection_happens_before_writer_lock`
- `durable_writer_lock_failure_happens_before_manifest_access`
- `durable_manifest_identity_mismatch_rejects_before_wal_open`
- `durable_manifest_codec_mismatch_rejects_before_wal_open`
- `durable_manifest_publish_uncertainty_preserves_source_chain`
- `durable_manifest_create_precondition_race_reloads_existing_manifest`
- `durable_manifest_create_precondition_race_reloads_and_revalidates_identity`
- `durable_existing_manifest_decode_failures_reject_before_wal_open`
- `durable_wal_open_failures_are_typed_and_do_not_mark_open`
- `durable_wal_header_database_mismatch_rejects_existing_segment`
- `durable_localfs_writer_lock_excludes_second_shell_until_drop`
- `lifecycle_durable_runtime_stays_assembly_only`
- Generated lifecycle counters for durable standard/always assembly, durable
  rejection, manifest create/open, manifest create-race, manifest
  publish-fault, WAL-open failure, writer-lock failure, manifest identity
  mismatch, recovering-state admission, no-recovery side effects, and
  input-derived durable routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Skip capability preflight | Acquire writer guard before `validate_backend_capabilities_for_open` | `durable_capability_rejection_happens_before_writer_lock` |
| Hardcode writer-lock object | Use `"locks/writer"` literal in lifecycle durable source | `lifecycle_durable_runtime_stays_assembly_only` |
| Manifest before lock | Load manifest before writer guard acquisition | `durable_writer_lock_failure_happens_before_manifest_access` |
| Reject missing manifest | Treat absent manifest as recovery failure | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Replace existing manifest | Publish manifest during existing open | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Ignore database id mismatch | Continue after wrong manifest database id | `durable_manifest_identity_mismatch_rejects_before_wal_open` |
| Ignore codec mismatch | Continue after wrong manifest codec id | `durable_manifest_codec_mismatch_rejects_before_wal_open` |
| Ignore create-race identity | Treat precondition race as opened without reload validation | `durable_manifest_create_precondition_race_reloads_and_revalidates_identity` |
| Accept corrupt manifest | Continue after malformed or future database manifest bytes | `durable_existing_manifest_decode_failures_reject_before_wal_open` |
| Ignore WAL metadata failure | Continue after active WAL segment metadata failure | `durable_wal_open_failures_are_typed_and_do_not_mark_open` |
| Ignore WAL header mismatch | Continue after active WAL segment database id mismatch | `durable_wal_header_database_mismatch_rejects_existing_segment` |
| Open hardcoded WAL segment | Ignore manifest active WAL segment | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Drop writer guard early | Do not retain `BackendWriterGuard` in shell | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Permit second local writer | Allow two durable local shells on the same localfs root | `durable_localfs_writer_lock_excludes_second_shell_until_drop` |
| Mark shell open early | Transition directly to `Open` after service assembly | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Collapse always to standard | Pass standard policy for durable always | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Collapse publish uncertainty | Map durability-unconfirmed publish to generic failure | `durable_manifest_publish_uncertainty_preserves_source_chain` |
| Replay during assembly | Construct WAL records or replay runtime in L8E | `lifecycle_durable_runtime_stays_assembly_only` |

### Verification

Commands run for L8E:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --all-features --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8F - Recovery Orchestration

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/replay.rs`
- `crates/storage-next/src/format/snapshot.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/format/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/recovery.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- L8F starts from the L8E durable shell and requires recovery-step admission.
- Manifest snapshot id/watermark facts are validated before WAL replay-start
  selection.
- Manifest-listed snapshots load through `SnapshotService` with database id,
  codec id, snapshot id, and watermark validation.
- Row-native checkpoint sections use storage row bytes and install through the
  L6 snapshot-install API.
- WAL records are read through `WalService::read_after_commit_version` using
  the trusted recovered checkpoint watermark. A manifest flush watermark that
  is not covered by recovered checkpoint/table state fails closed.
- Latest WAL tail truncation is repaired through `WalService::repair_latest_tail`
  only when the open plan explicitly allows lossy fallback; strict recovery
  rejects partial WAL tails before repair.
- Quarantine inventory loads through `QuarantineService::load_inventory` before
  WAL-tail repair, so a quarantine read/decode failure cannot leave a repaired
  WAL side effect.
- L8F returns a recovery package for L8G and does not invoke L7 replay,
  allocator catch-up, visible-version publication, or product callbacks.

### Intentional Changes

- Added `LifecycleRecoveryRuntime` over `LifecycleDurableLocalShell`.
- Added `LifecycleRecoveryRequest` and crate-private recovery outcome/fact
  structs for checkpoint, WAL, quarantine, and table validation.
- Added `SNAPSHOT_ROW_SECTION_KIND` and `encode_checkpoint_row_section` for
  row-native checkpoint snapshots.
- Re-exported storage-row encode/decode helpers from `format` for lifecycle
  recovery's storage-owned checkpoint section codec.
- Added `MissingSnapshotObject` and `WalTailRepairFailed` recovery fault kinds.
- Missing snapshot and WAL-tail repair degradations classify as `DataLoss`;
  quarantine inventory mismatches classify as `Telemetry`.
- Added mutable shell/service accessors needed by recovery while keeping the
  durable service bundle crate-private.
- Added a source guard that blocks L8F from calling `CommitReplayRuntime`,
  normal commit execution, visible publication, allocator catch-up, or product
  reconstruction hooks.
- Staged checkpoint branch-state replacement until WAL, table validation,
  quarantine inventory, and health aggregation succeed.
- Validate recovered table-object references before WAL tail repair so a missing
  table cannot leave a durable WAL repair side effect.
- Retain validated table identity and table-object facts in the L8F recovery
  package for the L8G/L8J handoff.
- Added a feature-gated lifecycle recovery testkit contract and integration
  test so `tests/lifecycle_recovery.rs` exercises storage behavior, with
  separate counters for canonical smoke paths and script-derived recovery
  coverage.

### Retired From V1 L8F

- Product primitive reconstruction during recovery.
- Direct branch internals mutation for checkpoint rows.
- Treating manifest active WAL segment id as a commit-version watermark.
- Trusting manifest snapshot watermark without loading the snapshot.
- Trusting a manifest flush watermark without recovering the flushed table
  state that proves it.
- Reporting healthy recovery after explicit lossy fallback.

### Deferred By Owner Slice

- L7 WAL replay, allocator/timestamp catch-up, visible-version publication,
  timeline validation, and unresolved durable gate reconciliation: L8G.
- Full generated recovery script counters and fuzz targets: L8O/L8P closeout
  unless pulled forward during L8G.
- Table-backed checkpoint metadata production: L8J. L8F can validate table
  object facts supplied in the recovery request, but current checkpoint tests
  exercise row-native sections.
- Flushed table-state recovery for manifest flush watermarks: L8I/L8J.
- Multi-branch checkpoint installation into runtime branch maps: L8G/L9. L8F
  currently fails closed on checkpoint rows for unopened branches.
- Quarantine mutation, repair, purge, and inventory rewrite: L8M.
- Public open outcome publication: L8G/L9.

### Tests Added

- `recovery_empty_database_returns_healthy_package_without_replay`
- `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail`
- `recovery_does_not_install_checkpoint_when_later_wal_read_fails`
- `recovery_repairs_latest_partial_log_tail_only_when_explicitly_lossy`
- `recovery_rejects_latest_partial_log_tail_in_strict_mode`
- `lossy_missing_snapshot_allows_uncertain_flush_watermark_as_degraded_data_loss`
- `recovery_rejects_checkpoint_row_newer_than_snapshot_watermark`
- `recovery_rejects_checkpoint_rows_for_unopened_branch`
- `recovery_rejects_flush_watermark_without_recovered_table_state`
- `recovery_rejects_missing_referenced_table_object`
- `recovery_records_validated_table_identity_and_facts`
- `recovery_validates_tables_before_wal_tail_repair`
- `recovery_validates_quarantine_before_wal_tail_repair`
- `recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy`
- `recovery_rejects_missing_snapshot_in_strict_mode`
- `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark`
- `recovery_request_rejects_lossy_when_open_plan_is_strict`
- `recovery_request_validates_limits_and_checkpoint_identity`
- `database_manifest_rejects_zero_snapshot_id_before_recovery`
- `recovery_rejects_snapshot_section_count_above_request_limit`
- `checkpoint_row_section_round_trips_and_rejects_trailing_bytes`
- `checkpoint_row_section_rejects_declared_rows_without_length_prefixes`
- `lifecycle_recovery_contract_exercises_storage_recovery_paths`
- `lifecycle_property_harness_runs_recovery_contract`
- `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks`

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Trust missing snapshot watermark | Use manifest snapshot watermark after missing snapshot in lossy mode | `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark` |
| Read WAL from zero | Ignore checkpoint watermark when selecting replay start | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Include watermark-equal records | Use `>= replay_start` instead of `> replay_start` | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Skip L6 snapshot install | Decode checkpoint rows but do not call snapshot install | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Partially mutate shell | Install checkpoint rows before a later WAL failure | `recovery_does_not_install_checkpoint_when_later_wal_read_fails` |
| Skip latest-tail repair | Return truncation without calling repair | `recovery_repairs_latest_partial_log_tail_only_when_explicitly_lossy` |
| Repair latest-tail in strict mode | Run WAL tail repair despite strict recovery | `recovery_rejects_latest_partial_log_tail_in_strict_mode` |
| Repair before table validation | Repair a partial WAL tail before validating a referenced table object | `recovery_validates_tables_before_wal_tail_repair` |
| Repair before quarantine validation | Repair a partial WAL tail before quarantine recovery succeeds | `recovery_validates_quarantine_before_wal_tail_repair` |
| Trust uncovered flush watermark | Use manifest flush watermark without recovered table state | `recovery_rejects_flush_watermark_without_recovered_table_state` |
| Accept too many snapshot sections | Ignore the recovery request section-count cap | `recovery_rejects_snapshot_section_count_above_request_limit` |
| Accept too-new checkpoint row | Install checkpoint row with commit version above snapshot watermark | `recovery_rejects_checkpoint_row_newer_than_snapshot_watermark` |
| Accept unopened branch row | Install checkpoint row for a branch not owned by the shell | `recovery_rejects_checkpoint_rows_for_unopened_branch` |
| Allocate from bogus row count | Reserve row-count capacity before checking payload length | `checkpoint_row_section_rejects_declared_rows_without_length_prefixes` |
| Ignore missing referenced table | Treat missing table object validation as healthy | `recovery_rejects_missing_referenced_table_object` |
| Treat quarantine mismatch as healthy | Ignore corrupt quarantine inventory under lossy policy | `recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy` |
| Healthy lossy fallback | Return `Healthy` after missing snapshot downgrade | `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark` |
| Collapse source chain | Drop lower snapshot decode/source error | `recovery_rejects_missing_snapshot_in_strict_mode` |
| Accept malformed row section | Ignore trailing checkpoint row bytes | `checkpoint_row_section_round_trips_and_rejects_trailing_bytes` |
| Call L7 replay in L8F | Import or invoke `CommitReplayRuntime` | `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks` |
| Advance visible in L8F | Call visible publication from recovery | `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks` |

### Verification

Commands run for L8F:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8G - Commit Bootstrap And Recovery Health

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/commit/replay.rs`
- `crates/storage-next/src/commit/durable.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

### Preserved As Storage Vocabulary

- L8G consumes only the L8F `LifecycleRecoveryOutcome`; it does not read
  manifests, snapshots, WAL segments, table objects, or quarantine inventory.
- WAL replay is delegated to L7 `CommitReplayRuntime`, preserving row-native
  WAL facts, timeline validation, duplicate/idempotent replay behavior,
  allocator catch-up, visible publication, and unresolved durable-gate
  reconciliation.
- Checkpoint-only recovery uses `VisibleVersionTracker` and
  `CommitFactAllocator` version/timestamp catch-up helpers instead of direct
  field mutation.
- Final durable open facts are reported through `StorageOpenOutcome` after the
  lifecycle state machine accepts `RecoveryAccepted`. The outcome now carries
  backend capabilities, database id, codec id, checkpoint/WAL/table/quarantine
  recovery facts, L7 bootstrap report, and raw stats for the L9 envelope.
- The opened durable runtime remains crate-private and composes normal durable
  commits through `CommitDurableRuntime`.

### Intentional Changes

- Added `LifecycleDurableLocalRuntime` in `lifecycle/durable/bootstrap.rs` as
  the opened durable-local runtime wrapper returned after successful recovery
  bootstrap.
- Added `LifecycleRecoveryBootstrapReport` for storage-shaped replay and
  checkpoint catch-up counters.
- Added `LifecycleDurableLocalShell::complete_recovery`, which consumes a
  recovering shell plus L8F package and returns an open durable runtime.
- Added WAL package validation for branch ownership and strict in-package
  ordering while preserving L7's idempotent replay semantics for checkpoint
  boundary records.
- Added typed `TimelineRecoveryMismatch` mapping for L7 timeline replay errors
  so L8 health/telemetry can distinguish timeline recovery failures from
  generic commit-runtime lower-layer failures.
- Updated lifecycle source guards so durable assembly stays in
  `lifecycle/durable.rs`, L8G replay/catch-up stays in
  `lifecycle/durable/bootstrap.rs`, and L8F remains blocked from replay,
  allocator catch-up, visible publication, and product hooks.

### Retired From V1 L8G

- Reimplementing replay/timeline checks in lifecycle code.
- Opening durable runtime before recovered WAL rows are replayed.
- Publishing checkpoint visibility by mutating visible-version fields directly.
- Accepting timeline-only WAL payloads.
- Treating durable recovery as public API; L9 still owns public wrapping.

### Deferred By Owner Slice

- Multi-branch durable runtime maps and mixed-branch WAL replay: L9 or later L8
  extension.
- Flushed table-state recovery beyond row-native checkpoint install: L8I/L8J.
- Maintenance readiness beyond conservative `false`: L8H+.
- Process-kill crash harnesses across every L8G phase: L8O.
- Durable close drain and sync-on-close: L8N.

### Tests Added

- `bootstrap_empty_recovery_opens_durable_runtime_with_zero_visibility`
- `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator`
- `bootstrap_replays_wal_tail_through_commit_runtime`
- `bootstrap_rejects_timeline_only_wal_payload_before_open`
- `bootstrap_rejects_log_record_without_timeline_rows_before_open`
- `bootstrap_rejects_recovered_log_record_for_unopened_branch`
- `bootstrap_rejects_recovered_log_records_not_strictly_ordered`
- `bootstrap_preserves_degraded_recovery_health_while_replaying_tail`
- `bootstrap_replay_is_idempotent_for_exactly_installed_rows`
- `bootstrap_replay_clears_matching_unresolved_durable_gate`
- `bootstrap_replay_uses_always_durability_for_always_mode`
- `bootstrap_replay_rejects_mismatched_unresolved_durable_gate`
- `lifecycle_durable_runtime_stays_bootstrap_only`
- `lifecycle_bootstrap_runtime_does_not_perform_durable_assembly`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Skip L7 replay | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Install recovered WAL rows directly into L6 | `bootstrap_replays_wal_tail_through_commit_runtime` |
| Ignore checkpoint visible catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not call visible catch-up for checkpoint-only package | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Ignore checkpoint allocator catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not catch allocator above checkpoint watermark | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Ignore checkpoint timestamp catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not catch timestamp guard up to checkpoint row timestamp max | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Drop durable open facts | `crates/storage-next/src/lifecycle/outcome.rs` | Omit checkpoint/WAL/table/quarantine/bootstrap facts from open outcome | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Replay with wrong durability | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Map `DurableLocalAlways` recovery to `CommitDurabilityClass::Standard` | `bootstrap_replay_uses_always_durability_for_always_mode` |
| Accept timeline-only WAL | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Remove L7 replay validation or bypass replay request validation | `bootstrap_rejects_timeline_only_wal_payload_before_open` |
| Accept missing timeline rows | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Bypass L7 replay request validation for user rows without timeline facts | `bootstrap_rejects_log_record_without_timeline_rows_before_open` |
| Replay foreign branch | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip recovered WAL branch-ownership validation | `bootstrap_rejects_recovered_log_record_for_unopened_branch` |
| Replay non-increasing package | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip strict recovered WAL in-package order validation | `bootstrap_rejects_recovered_log_records_not_strictly_ordered` |
| Drop degraded health | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Convert degraded L8F health to healthy during open outcome construction | `bootstrap_preserves_degraded_recovery_health_while_replaying_tail` |
| Reapply exact replay | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Treat exact duplicate rows as newly applied during bootstrap replay | `bootstrap_replay_is_idempotent_for_exactly_installed_rows` |
| Ignore matching durable gate | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not clear a matching unresolved durable gate after replay | `bootstrap_replay_clears_matching_unresolved_durable_gate` |
| Clear mismatched durable gate | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Clear or ignore an unresolved gate for a different durable fact | `bootstrap_replay_rejects_mismatched_unresolved_durable_gate` |
| Open before recovery accepted | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip `RecoveryAccepted` transition | `bootstrap_empty_recovery_opens_durable_runtime_with_zero_visibility` |
| Durable/bootstrap boundary drift | `crates/storage-next/src/lifecycle/durable.rs`, `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Move replay/catch-up into assembly or durable assembly into bootstrap | `lifecycle_durable_runtime_stays_bootstrap_only`, `lifecycle_bootstrap_runtime_does_not_perform_durable_assembly` |

### Verification

Commands run for L8G:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery -- --nocapture
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
