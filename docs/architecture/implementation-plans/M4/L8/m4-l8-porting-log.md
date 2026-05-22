# M4-L8 Porting Log

Status: active

Parent plans:

- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

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
- Lower-layer source-chain preservation through `LifecycleError::source()`.

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

### Sensitivity Probes Planned

- Product/engine import into production lifecycle source must trip
  `lifecycle_source_guard`.
- Raw filesystem or environment access in production lifecycle source must trip
  `lifecycle_source_guard`.
- Lower layers importing `crate::lifecycle` must trip
  `lifecycle_source_guard`.
- Bare public lifecycle items must trip `lifecycle_source_guard`.
- Config zero limits must fail module-local tests and generated scaffold tests.
- Lower-layer source-chain collapse must fail module-local tests and generated
  scaffold tests.

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
  mode and exact missing `BackendCapability` list.

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
