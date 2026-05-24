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
  required/missing facts, structured open outcome facts, class-prefixed stable
  error codes, checkpoint timestamp guard catch-up, timeline mismatch mapping,
  typed recovery-visibility failure reporting, and positive
  capability-order/source-guard tests.
- Explicitly deferred: exhaustive crash/fault/fuzz closeout, localfs recovery
  integration expansion, full maintenance/retention/quarantine/repair outcomes,
  durable close drain/sync outcomes, and the remaining named L8F/L8G/L8D/L8E
  matrix rows that require later L8H-L8P machinery.
- Ordering note: sections below reflect the order closeout entries were appended
  during implementation. The parent implementation plan remains the canonical
  slice ordering guide.

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
  stable V1 class-prefixed `LifecycleError::code()` strings such as
  `invalid_argument.lifecycle.config`,
  `failed_precondition.lifecycle.state`, and
  `corruption.lifecycle.recovery`.

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

## L8N - Close And Shutdown Ordering

### Shipped Files

- `crates/storage-next/src/lifecycle/durable/close.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/cache.rs`
- `crates/storage-next/src/lifecycle/tests/close.rs`
- `crates/storage-next/src/lifecycle/tests/durable.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance.rs`
- `crates/storage-next/src/testkit/lifecycle/close.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md`

### Preserved As Storage Vocabulary

- Close remains a storage lifecycle transition with `Requested`,
  `RetryPending`, `Complete`, and `AlreadyClosed` facts.
- Durable close reports commit quiesce, maintenance drain, durable sync, writer
  guard release, idempotent retry, and close stats through `CloseOutcome`.
- Close timeout is represented by the stable
  `deadline_exceeded.lifecycle.close` code.
- WAL close failures preserve the lower-layer service source chain.
- Writer guard release remains RAII-backed but is now explicit in durable close
  ownership and observable through reacquire behavior.

### Intentional Changes

- Durable close now lives in a dedicated durable close module rather than the
  recovery bootstrap module.
- Durable runtime close cancels cancelable pending maintenance, drains
  drain-required maintenance, acquires commit quiesce, closes/syncs the WAL,
  releases the writer guard, and transitions to `Closed`.
- Close with an active commit guard records retry-pending state and returns a
  typed timeout instead of silently waiting or succeeding.
- WAL sync failure leaves the runtime in retryable closing state and keeps the
  writer guard held until a successful retry.
- Durable services now store the writer guard as optional ownership so close can
  release it exactly once.
- Cache close remains volatile and does not import durable services.

### Retired From V1 L8N

- Product close callbacks, primitive freeze hooks, IPC/server shutdown, and
  public database handle release.
- Background worker thread shutdown.
- Raw filesystem close/fsync code in lifecycle.
- Retention, purge, snapshot pruning, or WAL truncation implicitly started by
  close. Only already-queued drain-required maintenance can run during close.

### Deferred By Owner Slice

- Public close API and product error mapping: L9/engine.
- Crash and fuzz close assurance: L8O/L8P.
- Multi-process lease renewal or handoff beyond the existing writer guard:
  later durable/object-backend work.
- Branch deletion and clear policy during close: later branch lifecycle work.

### Tests Added

- `durable_close_syncs_log_releases_writer_guard_and_is_idempotent`
- `durable_close_calls_wal_close_in_always_mode`
- `durable_close_does_not_report_complete_with_unresolved_durable_gate`
- `durable_close_does_not_truncate_wal_prune_snapshots_or_purge_quarantine_implicitly`
- `durable_reopen_can_acquire_writer_guard_after_close`
- `commit_after_close_requested_rejects_before_version_allocation`
- `durable_close_timeout_while_commit_guard_active_is_retryable`
- `durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable`
- `durable_close_log_sync_failure_preserves_writer_guard_for_retry`
- `cache_close_cancels_cancelable_pending_work`
- `cache_close_cancels_ordinary_pending_work_before_closed`
- `close_drain_preserves_task_order`
- `close_retry_after_drain_failure_does_not_rerun_completed_tasks`
- `maintenance_executor_drain_error_keeps_task_pending_for_retry`
- `lifecycle_close_contract_covers_shutdown_categories`
- `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache`
- The remaining close-shutdown test-plan inventory is represented directly by
  plan-named close, cache, maintenance, and durable tests. The inventory check
  over `l8n-close-shutdown-ordering-test-plan.md` reports `missing=0`.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Commit admission after close | `crates/storage-next/src/lifecycle/state.rs` | Admit commits while state is closing | `durable_close_timeout_while_commit_guard_active_is_retryable` / existing closed-commit tests |
| Active guard ignored | `crates/storage-next/src/lifecycle/durable/close.rs` | Skip `try_begin_quiesce` failure | `durable_close_timeout_while_commit_guard_active_is_retryable` |
| Drain task quiesce error loses retry | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove a failed drain-required task permanently | `durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable` / `maintenance_executor_drain_error_keeps_task_pending_for_retry` |
| WAL sync failure marked clean | `crates/storage-next/src/lifecycle/durable/close.rs` | Ignore `WalService::close` error | `durable_close_log_sync_failure_preserves_writer_guard_for_retry` |
| Writer guard released before sync | `crates/storage-next/src/lifecycle/durable/close.rs` | Release writer guard before WAL close | `durable_close_log_sync_failure_preserves_writer_guard_for_retry` |
| Writer guard not released | `crates/storage-next/src/lifecycle/durable/close.rs` | Skip guard release on successful close | `durable_close_syncs_log_releases_writer_guard_and_is_idempotent` |
| Unresolved durable gate ignored | `crates/storage-next/src/lifecycle/durable/close.rs` | Continue to WAL close despite unresolved durable commit | `durable_close_does_not_report_complete_with_unresolved_durable_gate` |
| Double close repeats durable sync | `crates/storage-next/src/lifecycle/durable/close.rs` | Run close phases again after `Closed` | `durable_close_syncs_log_releases_writer_guard_and_is_idempotent` |
| Ordinary close-canceled work survives close | `crates/storage-next/src/lifecycle/cache.rs` | Leave ordinary/cancelable tasks pending after cache close | `cache_close_cancels_ordinary_pending_work_before_closed` / `cache_close_cancels_cancelable_pending_work` |
| Generated close counters removed | `crates/storage-next/src/testkit/lifecycle/close.rs` | Do not increment retry/drain/quiesce/sync counters | `lifecycle_close_contract_covers_shutdown_categories` / `lifecycle_property_harness_runs_scaffold_contract` |
| Close logic moved into bootstrap | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Call close/drain/sync from bootstrap | `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache` |
| Cache close calls durable services | `crates/storage-next/src/lifecycle/cache.rs` | Call WAL close or release writer guard in cache close | `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache` |

### Verification

Commands run for L8N:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --lib lifecycle::tests::close
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib commit::tests::guard
cargo test -p strata-storage-next --locked --lib service::wal
plan='docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md'; missing=0; for name in $(perl -nE 'while(/`([a-z][a-z0-9_]+)`/g){say $1}' "$plan" | sort -u); do if ! rg -q "fn $name\b|$name" crates/storage-next/src/lifecycle crates/storage-next/src/testkit crates/storage-next/tests docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md; then echo "$name"; missing=$((missing+1)); fi; done; echo missing=$missing
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8O - Generated, Fault, And Crash Assurance

Status: implemented

### Shipped Files

- `crates/storage-next/src/testkit/lifecycle/script.rs`
- `crates/storage-next/src/testkit/lifecycle/fault.rs`
- `crates/storage-next/src/testkit/lifecycle/crash.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_faults.rs`
- `crates/storage-next/tests/lifecycle_fuzz_inventory.rs`
- `crates/storage-next/tests/crash_recovery.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `crates/storage-next/fuzz/Cargo.toml`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_maintenance.rs`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_retention.rs`
- `crates/storage-next/fuzz/corpus/lifecycle_recovery/*`
- `crates/storage-next/fuzz/corpus/lifecycle_maintenance/*`
- `crates/storage-next/fuzz/corpus/lifecycle_retention/*`
- `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-test-plan.md`

### Preserved As Storage Vocabulary

- Generated assurance remains storage-shaped: lifecycle state, storage mode,
  recovery health, checkpoint/flush/WAL watermarks, table-object reachability,
  quarantine/purge facts, maintenance queue facts, and close retryability.
- Fault coverage reports lifecycle error codes, lower-layer source chains,
  retryability, health debt, and affected storage families.
- Crash assurance reports durable phase-family coverage without introducing
  product crash handlers or public recovery wording.
- Fuzz contracts route arbitrary bytes into lifecycle recovery, maintenance,
  and retention contracts without panicking on corrupt inputs.

### Intentional Changes

- Added a composed generated lifecycle script contract that aggregates the
  existing family contracts and asserts input-derived route counters separately
  from canonical smoke routes.
- Added lightweight generated model facts for visible/checkpoint/flush
  watermarks, durable log records, snapshot/table/quarantine/reclaim counts,
  validation rejections, degraded health, and close state.
- Added fault-window and crash-window testkit contracts so integration tests can
  assert phase-family coverage without duplicating all unit fixtures.
- Registered lifecycle fuzz targets for recovery, maintenance, and retention,
  each calling a distinct contract function.
- Added named non-empty seed corpora for each lifecycle fuzz target.
- Extended source guards to ensure assurance code remains in testkit/tests/fuzz,
  production lifecycle code does not import testkit or fuzz helpers, crash tests
  are feature-gated, and generated properties assert input-derived counters.

### Retired From V1 L8O

- Product crash supervisors, process managers, IPC/server shutdown, primitive
  replay callbacks, and user-facing recovery reports.
- Exhaustive process-kill matrix in normal CI. The localfs crash harness remains
  bounded and feature-gated.
- Shared lifecycle fuzz scaffold targets. Each lifecycle fuzz target now names
  and calls its own contract.

### Deferred By Owner Slice

- Final closeout inventory, sensitivity-ledger consolidation, and command-matrix
  enforcement: L8P.
- Nightly/libfuzzer execution in CI remains optional; normal tests verify target
  registration, distinct routing, and seed corpora.
- Distributed object-store lease/crash fault simulation remains later durable
  backend work.

### Tests Added

- `lifecycle_property_harness_runs_generated_script_contract`
- `lifecycle_property_harness_requires_input_derived_recovery_routes`
- `lifecycle_property_harness_requires_input_derived_maintenance_routes`
- `lifecycle_property_harness_requires_input_derived_retention_routes`
- `lifecycle_property_harness_requires_input_derived_quarantine_routes`
- `lifecycle_property_harness_requires_input_derived_close_routes`
- `lifecycle_property_harness_replays_minimized_failure_case`
- `lifecycle_property_harness_records_regression_file`
- `lifecycle_generated_script_exercises_input_derived_open_recovery_and_close`
- `lifecycle_generated_script_exercises_input_derived_maintenance_routes`
- `lifecycle_generated_script_exercises_input_derived_reclaim_routes`
- `lifecycle_generated_script_rejects_validation_only_script_without_side_effect_claim`
- `lifecycle_generated_script_model_matches_healthy_recovered_visibility`
- `lifecycle_generated_script_deletion_set_is_subset_of_model_proof`
- `lifecycle_generated_script_watermarks_are_monotonic`
- `lifecycle_generated_script_close_is_idempotent_after_success`
- `lifecycle_generated_script_cache_mode_never_claims_durable_recovery`
- `lifecycle_generated_script_lossy_recovery_records_degraded_health`
- `lifecycle_generated_integration_runs_default_mode_script`
- `lifecycle_generated_integration_runs_durable_mode_script`
- `lifecycle_generated_integration_runs_reclaim_close_script`
- `lifecycle_fault_integration_covers_all_phase_families`
- `lifecycle_crash_integration_reports_case_counts`
- `generated_recovery_empty_checkpoint_tail_and_lossy_routes_are_input_driven`
- `generated_recovery_corrupt_manifest_snapshot_wal_and_table_are_typed`
- `generated_bootstrap_catches_allocator_timestamp_and_visible_facts`
- `generated_bootstrap_rejects_timeline_mismatch`
- `generated_bootstrap_reconciles_unresolved_durable_gate`
- `generated_recovery_health_matches_fault_family_model`
- `fault_capability_mismatch_happens_before_durable_side_effects`
- `fault_writer_guard_acquired_then_manifest_create_fails_releases_or_reports_guard`
- `fault_manifest_create_visible_but_publish_uncertain_records_health_debt`
- `fault_snapshot_published_manifest_update_fails_records_orphan_snapshot`
- `fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success`
- `fault_partial_wal_tail_strict_fails_before_repair`
- `fault_partial_wal_tail_lossy_repairs_and_degrades_health`
- `fault_corrupt_wal_returns_typed_recovery_error`
- `fault_replay_failure_transitions_bootstrap_to_failed`
- `fault_replay_visible_publication_failure_records_durable_not_visible`
- `fault_flush_table_published_branch_install_fails_reports_orphan_table`
- `fault_table_rewrite_branch_swap_failure_preserves_reads`
- `fault_incomplete_retention_proof_blocks_delete_before_backend_access`
- `fault_quarantine_inventory_publish_failure_blocks_purge`
- `fault_purge_delete_success_inventory_update_failure_preserves_debt`
- `fault_close_quiesce_timeout_is_retryable`
- `fault_close_wal_sync_failure_preserves_source_chain`
- `fault_close_manifest_sync_failure_preserves_final_fact_debt`
- `fault_writer_guard_release_failure_is_typed_when_backend_reports_it`
- `crash_after_wal_append_before_visibility_replays_record`
- `crash_after_wal_append_with_unresolved_gate_reconciles_on_reopen`
- `crash_after_snapshot_publish_before_manifest_update_ignores_orphan_snapshot`
- `crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail`
- `crash_after_table_publish_before_branch_install_reports_orphan_table`
- `crash_after_quarantine_inventory_publish_before_object_move_reports_debt`
- `crash_after_object_quarantine_before_purge_preserves_quarantine_entry`
- `crash_after_close_wal_sync_before_guard_release_reopens_consistently`
- `crash_harness_ignored_cases_have_nonignored_phase_equivalents`
- `crash_harness_respects_case_limit_and_keep_root_environment`
- `lifecycle_fuzz_targets_are_registered`
- `lifecycle_fuzz_targets_call_distinct_contracts`
- `lifecycle_fuzz_corpora_have_non_empty_seed_files`
- `lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes`
- `lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes`
- `lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes`
- `lifecycle_generated_assurance_stays_in_testkit_tests_or_fuzz`
- `lifecycle_production_does_not_import_testkit_or_fuzz`
- `lifecycle_fuzz_targets_use_distinct_contracts`
- `lifecycle_fuzz_corpora_are_seeded`
- `lifecycle_crash_tests_are_feature_gated`
- `ignored_crash_tests_have_nonignored_phase_equivalents`
- `lifecycle_generated_properties_assert_input_derived_counters`
- `lifecycle_assurance_tests_avoid_sleeps_and_thread_spawns`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Generated prelude masks input | `crates/storage-next/src/testkit/lifecycle/script.rs` | Remove input-derived route checks | `lifecycle_property_harness_requires_input_derived_recovery_routes` / generated property harness |
| Recovery health collapse | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Do not require strict/lossy recovery fault routes | `fault_corrupt_wal_returns_typed_recovery_error` |
| Unsafe retention | `crates/storage-next/src/testkit/lifecycle/script.rs` | Skip deletion subset check | `generated_retention_never_deletes_reachable_tables_or_live_snapshots` |
| Stale purge proof | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Do not require stale purge route | `fault_quarantine_inventory_publish_failure_blocks_purge` |
| Checkpoint truncation too aggressive | `crates/storage-next/src/testkit/lifecycle/script.rs` | Drop watermark monotonic check | `generated_checkpoint_truncation_never_removes_uncovered_wal_records` |
| Close starts ordinary work | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Skip close quiesce/timeout route | `generated_close_blocks_new_commits_and_ordinary_maintenance` |
| Fuzz target shares scaffold | `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs` | Call scaffold contract instead of recovery fuzz contract | `lifecycle_fuzz_targets_call_distinct_contracts` |
| Empty corpora | `crates/storage-next/fuzz/corpus/lifecycle_recovery/valid_seed` | Empty or remove seed file | `lifecycle_fuzz_corpora_have_non_empty_seed_files` |
| Crash test not gated | `crates/storage-next/tests/crash_recovery.rs` | Remove localfs/testkit/wasm cfg from crash routes | `lifecycle_crash_tests_are_feature_gated` |
| Production imports testkit | `crates/storage-next/src/lifecycle/*.rs` | Import testkit or fuzz helpers from production lifecycle source | `lifecycle_production_does_not_import_testkit_or_fuzz` |

### Verification

Commands run for L8O:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --bins
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8M - Quarantine, Reclaim, Purge, And Repair

### Shipped Files

- `crates/storage-next/src/lifecycle/quarantine.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/tests/quarantine.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/quarantine.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-test-plan.md`

### Preserved As Storage Vocabulary

- Quarantine proofs distinguish safe, referenced, incomplete, and
  recovery-health-blocked states.
- Quarantine operation outcomes record source object, quarantine object,
  inventory object, byte count, entry count, recovery health, retryability, and
  lower-layer source errors.
- Purge proofs distinguish fresh, stale, incomplete, and
  recovery-health-blocked states before any backend mutation can happen.
- Purge outcomes report deleted, already-missing, failed, and retained
  quarantine entries plus reclaimed byte counts from inventory facts.
- Repair outcomes report branch or family reconciliation facts for listed,
  missing, unlisted, malformed, and inventory-present states.

### Raw Health And Fact Vocabulary

- Quarantine publication uncertainty and publication failure have stable
  lifecycle error codes.
- Inventory mismatch and repair inconclusive states preserve lower-layer service
  errors when available.
- Quarantine and purge deferred outcomes carry telemetry health debt rather than
  claiming durable reclaim success.
- Maintenance outcomes preserve affected object names, state-change counts,
  reclaimed bytes when known, source errors, and recovery-health debt.

### Intentional Changes

- Lifecycle quarantine delegates all durable copy, inventory publication,
  source deletion, purge, and repair operations to `QuarantineService`.
- Cache mode has no durable quarantine mutation surface.
- Retention remains proof-only; it delegates quarantine object families rather
  than mutating inventory or deleting objects directly.
- Durable maintenance now has concrete quarantine purge and repair runners for
  branch and family scopes.
- Non-publication service failures are classified separately from quarantine
  object publication failures so missing source objects and malformed requests
  are not advertised as retryable publish windows.

### Retired From V1 L8M

- Direct backend deletion from lifecycle quarantine code.
- Lifecycle-owned quarantine inventory encoding/decoding.
- Product repair reports.
- Runtime-only reachability proofs as sufficient evidence for destructive
  reclaim.

### Deferred By Owner Slice

- Close-time final quarantine drain: L8N.
- Crash/fuzz assurance expansion: L8O/L8P.
- Public repair and purge commands: L9.
- Automatic table-manifest-backed quarantine proof assembly: later durable
  table-manifest work.

### Tests Added

- `quarantine_proof_complete_from_candidate_and_blocks_unsafe_health`
- `quarantine_incomplete_proof_defers_without_backend_access`
- `quarantine_stages_inventory_copy_and_source_delete_in_order`
- `quarantine_source_delete_failure_reports_retryable_health_debt`
- `quarantine_missing_source_is_service_failure_not_publish_failure`
- `quarantine_proof_allows_unrelated_telemetry_debt`
- `purge_request_rejects_missing_database_id_before_backend_access`
- `purge_requires_fresh_proof_before_backend_access`
- `purge_deletes_inventory_listed_quarantine_objects`
- `repair_request_rejects_missing_database_id_before_backend_access`
- `repair_reports_unlisted_quarantine_object_as_health_debt`
- `durable_quarantine_runs_through_runtime_maintenance_surface`
- `durable_purge_runs_through_runtime_maintenance_surface`
- `durable_repair_runs_through_runtime_maintenance_surface`
- `purge_and_repair_maintenance_requests_preserve_branch_scope`
- `quarantine_errors_have_stable_codes`
- `lifecycle_quarantine_integration`
- `lifecycle_purge_integration`
- `lifecycle_repair_reconciliation_integration`
- `lifecycle_reclaim_blocks_unsafe_recovery_integration`
- `lifecycle_cache_reclaim_unsupported_integration`
- `lifecycle_quarantine_then_purge_round_trip`
- `lifecycle_quarantine_publish_failure_surfaces_health_debt`
- `lifecycle_quarantine_generated_bytes_influence_routes`
- `lifecycle_quarantine_source_uses_quarantine_service_boundary`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Unsafe health quarantines | `crates/storage-next/src/lifecycle/quarantine.rs` | Treat blocked recovery health as complete proof | `quarantine_proof_complete_from_candidate_and_blocks_unsafe_health` |
| Incomplete proof mutates backend | `crates/storage-next/src/lifecycle/quarantine.rs` | Call quarantine service for incomplete proof | `quarantine_incomplete_proof_defers_without_backend_access` |
| Source delete before durable copy | `crates/storage-next/src/service/quarantine/mutation.rs` | Reorder source delete before inventory/copy | `quarantine_stages_inventory_copy_and_source_delete_in_order` |
| Delete failure hidden | `crates/storage-next/src/lifecycle/quarantine.rs` | Collapse source delete error to completed outcome | `quarantine_source_delete_failure_reports_retryable_health_debt` |
| Missing source misclassified | `crates/storage-next/src/lifecycle/quarantine.rs` | Report source metadata/read failures as publish failures | `quarantine_missing_source_is_service_failure_not_publish_failure` |
| Stale purge proof deletes | `crates/storage-next/src/lifecycle/quarantine.rs` | Treat stale purge proof as fresh | `purge_requires_fresh_proof_before_backend_access` |
| Purge deletes unlisted object | `crates/storage-next/src/service/quarantine/mutation.rs` | Delete by prefix instead of inventory entries | `purge_deletes_inventory_listed_quarantine_objects` |
| Purge drops byte facts | `crates/storage-next/src/service/quarantine/mutation.rs` | Do not accumulate reclaimed bytes from deleted inventory entries | `purge_deletes_inventory_listed_quarantine_objects` |
| Repair hides unlisted object | `crates/storage-next/src/lifecycle/quarantine.rs` | Ignore unlisted reconciliation facts | `repair_reports_unlisted_quarantine_object_as_health_debt` |
| Runtime runner bypass | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Remove purge or repair runtime maintenance runner wiring | `durable_purge_runs_through_runtime_maintenance_surface` / `durable_repair_runs_through_runtime_maintenance_surface` |
| Branch purge scope rejected | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove branch scope support for purge tasks | `purge_and_repair_maintenance_requests_preserve_branch_scope` |
| Error code collapses | `crates/storage-next/src/lifecycle/error.rs` | Route quarantine failures through generic maintenance code | `quarantine_errors_have_stable_codes` |
| Lifecycle bypasses service boundary | `crates/storage-next/src/lifecycle/quarantine.rs` | Call backend delete or encode inventory directly | `lifecycle_quarantine_source_uses_quarantine_service_boundary` |

### Verification

Commands run for L8M:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib service::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8K: Compaction And Materialization Scheduling Hooks

### Shipped Files

- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/compaction.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-test-plan.md`

### Preserved As Storage Vocabulary

- Compaction is requested with a branch id, branch compaction kind, output
  identity seed, and table-rewrite durability mode.
- Materialization is requested with a child branch id, inherited-layer index,
  output identity prefix, and table-rewrite durability mode.
- Outcomes report completed, completed-checkpoint-required, no-candidate,
  no-layer, and already-materialized states.
- Storage pressure facts report frozen backlog, level-zero table backlog,
  inherited-layer backlog, maintenance queue backlog, and suggested storage
  maintenance tasks.

### Raw Health And Fact Vocabulary

- V1 durable-local compaction/materialization reports checkpoint debt instead
  of standalone table-object reachability.
- Lower-layer branch runtime errors preserve source chains through
  `LifecycleLowerLayer::BranchRuntime`.
- Maintenance outcomes retain task kind, status, task id, affected-object
  count, stats, retryability, and optional recovery health.

### Intentional Changes

- Lifecycle delegates all candidate selection, table replacement validation,
  inherited-row rewriting, and read semantics to L6. It does not inspect rows
  to choose merge inputs.
- Cache and durable runtimes share the same L6 rewrite paths. Durable mode
  upgrades successful rewrites to checkpoint-required outcomes.
- The source guard now permits storage level names such as `CompactL0...` while
  continuing to reject architecture slice labels like `L8K` in implementation
  source and tests.

### Deferred By Owner Slice

- Standalone table-object publication for compaction/materialization waits for
  table-manifest recovery, so published-not-installed fault windows are not
  claimed here.
- Generated lifecycle property scripts for table rewrites remain later
  assurance-depth work.
- Retention pruning, replaced-object quarantine/purge, background thread
  scheduling, and memory-budget admission remain later slices.

### Tests Added

- `table_rewrite_requests_validate_opaque_identity_components`
- `maintenance_tasks_map_to_table_rewrite_requests`
- `cache_compaction_defers_without_a_candidate`
- `cache_compaction_installs_replacement_and_preserves_reads`
- `durable_compaction_reports_checkpoint_debt`
- `materialization_defers_when_no_inherited_layer_exists`
- `cache_materialization_removes_layer_and_preserves_child_precedence`
- `durable_materialization_reports_checkpoint_debt`
- `storage_pressure_suggests_the_next_table_rewrite_or_flush`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Empty output seed accepted | `crates/storage-next/src/lifecycle/compaction.rs` | Remove lower-layer request validation | `table_rewrite_requests_validate_opaque_identity_components` |
| Wrong maintenance kind routed as compaction | `crates/storage-next/src/lifecycle/compaction.rs` | Skip task-kind check before request conversion | `maintenance_tasks_map_to_table_rewrite_requests` |
| No-candidate treated as success | `crates/storage-next/src/lifecycle/compaction.rs` | Collapse no-candidate to completed outcome | `cache_compaction_defers_without_a_candidate` |
| Durable rewrite overclaims recovery safety | `crates/storage-next/src/lifecycle/compaction.rs` | Return completed instead of checkpoint-required status | `durable_compaction_reports_checkpoint_debt` |
| Materialization ignores child-local precedence | `crates/storage-next/src/branch/state.rs` | Install replacements ahead of child-owned rows | `cache_materialization_removes_layer_and_preserves_child_precedence` |
| Missing inherited layer becomes hard failure | `crates/storage-next/src/lifecycle/compaction.rs` | Return branch error instead of deferred no-layer outcome | `materialization_defers_when_no_inherited_layer_exists` |
| Frozen backlog deprioritized behind compaction | `crates/storage-next/src/lifecycle/compaction.rs` | Suggest compaction before flush when frozen tables exist | `storage_pressure_suggests_the_next_table_rewrite_or_flush` |
| Architecture label allowed in lifecycle source | `crates/storage-next/tests/lifecycle_source_guard.rs` | Permit `L8K`-style labels in implementation text | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8K:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8I - Flush Frozen State And Table Publication

Status: implemented.

### Shipped Files

- `crates/storage-next/src/lifecycle/flush.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/flush.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/flush.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-test-plan.md`

### What Landed

- Added a concrete flush request/outcome surface for one frozen branch table at
  a time.
- Added deterministic frozen-table selection: explicit index when supplied,
  otherwise the oldest frozen table.
- Added cache-mode flush orchestration that builds an immutable table from
  frozen rows and installs it back into branch state without durable object
  services.
- Added durable-local flush orchestration that publishes a table object,
  reopens the published object through the table reader service, constructs a
  branch-owned immutable table, and only then replaces frozen state.
- Added retry handling for the already-created-object window by reopening the
  matching deterministic table object instead of republishing it.
- Uses full SHA-256 hex in deterministic table/object identities rather than a
  short fingerprint, so retry identity is stable without accepting avoidable
  alias risk.
- Durable outcomes count affected objects only when a table object exists;
  cache-mode flushes report zero durable object effects.
- Added maintenance task request construction for branch-scoped flush tasks.
- Added concrete cache and durable runtime dispatch for queued branch-scoped
  flush tasks through the maintenance executor.
- Added generated flush contract coverage under the lifecycle testkit for
  cache success, durable success, no-op, publish failure, reopen failure, retry,
  and read parity.
- Added a branch-layer storage-vocabulary alias for frozen replacement so
  lifecycle implementation code does not contain numbered layer labels.

### Preserved As Storage Vocabulary

- Request facts: branch id, optional frozen index, table identity seed, table
  object id, and target branch level.
- Outcome facts: status, branch id, replaced frozen index, row count, table
  identity, table facts, table object, object facts, install outcome, and
  failure source when a durable object was created but branch install did not
  complete.
- Maintenance mapping: completed, deferred, and retryable failed outcomes map
  onto the generic maintenance outcome vocabulary.

### Intentional Non-Goals

- No database manifest flush watermark update.
- No checkpoint publication.
- No WAL retention or truncation.
- No compaction, materialization scheduling, retention, quarantine, purge, or
  repair.
- No public maintenance command surface.

### Tests Added

- `flush_request_validates_components_and_target_level`
- `flush_without_frozen_state_is_deferred`
- `cache_flush_replaces_oldest_frozen_table_and_preserves_reads`
- `cache_flush_replaces_named_table_and_keeps_other_frozen_order`
- `cache_flush_preserves_tombstones_and_commit_timestamps`
- `repeated_default_flush_after_success_is_deferred`
- `cache_runtime_flushes_explicitly_rotated_state_only`
- `queued_cache_flush_task_runs_through_executor`
- `durable_flush_publishes_reopens_and_installs_table`
- `queued_durable_flush_task_publishes_object_through_executor`
- `durable_publish_failure_leaves_frozen_state_unchanged`
- `durable_reopen_failure_reports_published_not_installed`
- `durable_invalid_publish_metadata_preserves_service_source`
- `durable_reopen_wrong_branch_table_reports_partial_publication`
- `durable_install_failure_reports_orphaned_object_fact`
- `existing_conflicting_object_fails_closed_without_removing_frozen_rows`
- `durable_flush_retries_existing_matching_object`
- `flush_named_frozen_index_must_exist`
- `flush_identity_is_deterministic_and_changes_with_storage_facts`
- `lifecycle_maintenance_contract_covers_flush_categories`
- `lifecycle_property_harness_runs_flush_contract`
- `lifecycle_flush_source_does_not_manage_watermarks_or_log_retention`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Select newest frozen table by default | `crates/storage-next/src/lifecycle/flush.rs` | Return index 0 instead of the highest frozen index | `cache_flush_replaces_oldest_frozen_table_and_preserves_reads` |
| Remove frozen state before durable publication | `crates/storage-next/src/lifecycle/flush.rs` | Call branch replacement before table-object publish | `durable_publish_failure_leaves_frozen_state_unchanged` |
| Skip object reopen after durable publish | `crates/storage-next/src/lifecycle/flush.rs` | Build branch-owned table directly from in-memory bytes after publish | `durable_flush_publishes_reopens_and_installs_table` |
| Treat active rows as an implicit flush candidate | `crates/storage-next/src/lifecycle/cache.rs` | Rotate active rows inside flush | `cache_runtime_flushes_explicitly_rotated_state_only` |
| Lose retry support for existing deterministic object | `crates/storage-next/src/lifecycle/flush.rs` | Return publish precondition failure directly | `durable_flush_retries_existing_matching_object` |
| Collapse cache and durable object effects | `crates/storage-next/src/lifecycle/flush.rs` | Count table identity instead of table object in maintenance effects | `cache_flush_replaces_oldest_frozen_table_and_preserves_reads`, `durable_flush_publishes_reopens_and_installs_table` |
| Shorten deterministic digest | `crates/storage-next/src/lifecycle/flush.rs` | Truncate SHA-256 to a short prefix | `flush_identity_is_deterministic_and_changes_with_storage_facts` |
| Treat published-not-installed as success | `crates/storage-next/src/lifecycle/flush.rs` | Return completed after object publish but before reopen/install succeeds | `durable_reopen_failure_reports_published_not_installed`, `durable_install_failure_reports_orphaned_object_fact` |
| Drop tombstones during table build | `crates/storage-next/src/lifecycle/flush.rs` | Filter tombstone rows from the built table | `cache_flush_preserves_tombstones_and_commit_timestamps` |
| Accept conflicting existing object | `crates/storage-next/src/lifecycle/flush.rs` | Treat any pre-existing deterministic object as matching | `existing_conflicting_object_fails_closed_without_removing_frozen_rows` |
| Call watermark or log-retention services from flush | `crates/storage-next/src/lifecycle/flush.rs` | Add manifest flush-watermark or log truncation calls | `lifecycle_flush_source_does_not_manage_watermarks_or_log_retention` |
| Put architecture labels in lifecycle implementation | `crates/storage-next/src/lifecycle/flush.rs` | Call numbered lower-layer method names directly | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8I:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
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
- `crates/storage-next/src/testkit/lifecycle/bootstrap.rs`
- `crates/storage-next/src/testkit/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
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
- `lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths`
- `lifecycle_property_harness_runs_bootstrap_contract`
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
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8H - Maintenance Task Executor

Status: implemented

### Source Evidence Read

- `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/testkit/lifecycle/`

### Shipped Files

- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/maintenance.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

### Preserved As Storage Vocabulary

- Maintenance task kinds remain the storage-owned vocabulary introduced by the
  lifecycle scaffold: flush, checkpoint, WAL truncation, compaction,
  materialization, snapshot pruning, retention, quarantine, purge, repair, and
  health collection.
- The executor is deterministic and single-threaded. Ordering is explicit:
  priority first, then enqueue sequence for equal priority.
- Queue capacity is driven by `LifecycleConfig::max_maintenance_queue_depth`.
- Coalescing is explicit through task kind plus storage scope. Duplicate
  requests return a coalesced enqueue outcome instead of pretending a second
  task was queued.
- Close integration is prepared through drain-required and cancel-before-close
  policies, but full durable close sequencing remains owned by L8N.
- Maintenance outcome facts carry task id, status, health debt, affected object
  count, reclaimed bytes, retryability, and stats.
- Runtime maintenance hooks remain crate-private. There is still no public user
  maintenance command surface.

### Raw Health And Fact Vocabulary

- `MaintenanceTaskId` and `MaintenanceTaskSequence` are deterministic counters.
- `MaintenanceTaskPriority` records critical/high/normal/low ordering.
- `MaintenanceTaskScope` records global, branch, WAL, checkpoint, quarantine,
  retention, table-level, and inherited-layer scopes without product DTOs.
- `LifecycleMaintenanceStats` records enqueued, coalesced, started, completed,
  deferred, failed, canceled, drained, and queue-full counters.
- `MaintenanceFaultPoint` records before-enqueue, after-enqueue, at-task-start,
  after-task-run, and during-drain boundaries for deterministic fault tests.
- Maintenance readiness now means an executor is attached and recovery health is
  safe enough for ordinary maintenance. Healthy and telemetry-degraded recovery
  can be ready; data-loss, policy-downgrade, and failed recovery are not ready.

### Intentional Changes

- Cache-mode open now reports maintenance readiness once the executor is
  attached. Durable-only task handlers still defer until later slices; cache
  mode does not import durable services.
- Durable-local open reports maintenance readiness after successful bootstrap
  only when recovery health allows ordinary maintenance.
- Cache close cancels pending cancel-before-close maintenance tasks and reports
  the count through close stats. Durable close drain/sync remains deferred.
- A source guard now rejects architecture-layer labels in lifecycle
  implementation, lifecycle testkit, lifecycle unit tests, and lifecycle
  integration tests, keeping milestone vocabulary in plans instead of code.

### Retired From V1 L8H

- Engine background scheduler imports.
- Product or public manual maintenance command wording.
- Wall-clock sleeps or thread races in executor tests.
- Concrete flush, checkpoint, compaction, retention, quarantine, purge, repair,
  or durable-close implementations inside the executor slice.

### Deferred By Owner Slice

- Flush frozen state and table publication: L8I.
- Checkpoint, flush watermark, and WAL truncation: L8J.
- Compaction and materialization scheduling hooks: L8K.
- Retention proof and snapshot pruning: L8L.
- Quarantine, reclaim, purge, and repair facts: L8M.
- Durable close drain/sync/guard release: L8N.
- Crash/fuzz/fault closeout expansion: L8O-L8P.

### Tests Added

- `maintenance_task_request_validates_kind_scope_pairs`
- `maintenance_task_requests_accept_every_supported_kind_and_scope`
- `maintenance_task_ids_and_sequences_are_monotonic`
- `maintenance_policy_and_coalesce_key_preserve_storage_scope`
- `maintenance_debug_output_uses_storage_vocabulary`
- `maintenance_enqueue_requires_open_and_enforces_capacity`
- `maintenance_admission_rejects_ordinary_work_outside_open`
- `maintenance_close_drain_requires_closing_and_ordinary_run_requires_open`
- `lifecycle_health_query_is_admitted_in_every_state`
- `maintenance_queue_depth_allows_exact_capacity`
- `maintenance_executor_orders_by_priority_then_fifo`
- `maintenance_executor_preserves_fifo_for_equal_priority`
- `maintenance_executor_order_survives_coalescing_and_canceling`
- `maintenance_executor_coalesces_pending_tasks_by_key`
- `maintenance_executor_coalesces_each_coalescing_scope_independently`
- `maintenance_executor_does_not_coalesce_non_coalescing_requests`
- `maintenance_executor_does_not_coalesce_active_task`
- `maintenance_executor_clears_active_after_runner_error`
- `maintenance_executor_run_empty_queue_returns_no_work_without_stats`
- `maintenance_executor_records_deferred_and_preserves_effects`
- `maintenance_executor_converts_after_run_fault_to_failed_outcome`
- `maintenance_executor_attaches_health_debt_to_failed_outcome`
- `maintenance_executor_counts_canceled_outcomes_as_canceled`
- `maintenance_executor_cancel_and_drain_respect_close_policy`
- `maintenance_executor_empty_drain_and_cancel_are_idempotent`
- `maintenance_executor_cancel_removes_only_pending_cancelable_tasks`
- `maintenance_executor_records_drain_fault_without_removing_pending_task`
- `maintenance_fault_before_enqueue_leaves_queue_unchanged`
- `maintenance_fault_after_enqueue_keeps_pending_task_observable`
- `maintenance_fault_hooks_fire_in_deterministic_order`
- `maintenance_ready_policy_tracks_recovery_health_class`
- `cache_runtime_can_enqueue_and_run_health_collection_maintenance`
- `cache_close_rejects_pending_drain_required_maintenance_before_transitioning`
- `bootstrap_runtime_can_enqueue_and_run_health_collection_maintenance`
- `lifecycle_maintenance_contract_covers_executor_categories`
- `lifecycle_property_harness_runs_maintenance_contract`
- `lifecycle_maintenance_tests_avoid_sleeps_and_thread_spawns`
- `lifecycle_implementation_avoids_architecture_labels`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Allow ordinary maintenance outside open | `crates/storage-next/src/lifecycle/maintenance.rs` | Skip lifecycle admission in enqueue/run | `maintenance_enqueue_requires_open_and_enforces_capacity` |
| Ignore queue capacity | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove queue-depth check | `maintenance_enqueue_requires_open_and_enforces_capacity` |
| Reverse priority ordering | `crates/storage-next/src/lifecycle/maintenance.rs` | Select lowest priority first | `maintenance_executor_orders_by_priority_then_fifo` |
| Break FIFO tiebreak | `crates/storage-next/src/lifecycle/maintenance.rs` | Sort equal-priority tasks by newest sequence | `maintenance_executor_orders_by_priority_then_fifo` |
| Drop coalescing fact | `crates/storage-next/src/lifecycle/maintenance.rs` | Return enqueued for duplicate pending task | `maintenance_executor_coalesces_pending_tasks_by_key` |
| Coalesce active task away | `crates/storage-next/src/lifecycle/maintenance.rs` | Match active task in duplicate lookup | `maintenance_executor_does_not_coalesce_active_task` |
| Leave active after error | `crates/storage-next/src/lifecycle/maintenance.rs` | Do not clear active on runner error | `maintenance_executor_clears_active_after_runner_error` |
| Run cancelable task during close drain | `crates/storage-next/src/lifecycle/maintenance.rs` | Drain every pending task regardless of close policy | `maintenance_executor_cancel_and_drain_respect_close_policy` |
| Report ready after data loss | `crates/storage-next/src/lifecycle/maintenance.rs` | Treat every degraded health as ready | `maintenance_ready_policy_tracks_recovery_health_class` |
| Add architecture labels to code | `crates/storage-next/src/lifecycle/*.rs`, `crates/storage-next/src/testkit/lifecycle/*.rs`, `crates/storage-next/tests/lifecycle_*.rs` | Put milestone labels in implementation comments, strings, or test names | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8H:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8J - Checkpoint, Flush Watermark, And WAL Truncation

### Size Note

- This slice exceeded the preferred review-size budget because checkpoint
  publication, flush-watermark persistence, WAL truncation, recovery
  round-trips, and service fault windows landed together. The implementation is
  isolated in `checkpoint.rs` plus checkpoint-specific test modules; future
  checkpoint retention/pruning work should split into smaller owner slices.

### Shipped Files

- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/checkpoint.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-test-plan.md`

### Preserved As Storage Vocabulary

- Checkpoint requests carry branch id, snapshot id, creation timestamp, optional
  snapshot sections, and explicit follow-up toggles for flush watermark and WAL
  truncation.
- Checkpoint outcomes report checkpoint status, row count, section count,
  snapshot object, active WAL segment, optional flush-watermark facts, optional
  WAL-truncation facts, and recovery health debt.
- Flush-watermark requests use explicit proof vocabulary:
  checkpoint-covered, already-persisted, and table-objects-only.
- WAL truncation accepts only `WalRetentionProof`, preserving source
  vocabulary from snapshot watermark or flush watermark.
- Maintenance tasks enqueue and run checkpoint and WAL-truncation work through
  the common executor and retain task id, task kind, status, retryability,
  effects, stats, and health debt.

### Raw Health And Fact Vocabulary

- `LifecycleCheckpointStatus` records completed, deferred, partial snapshot
  publication, uncertain snapshot visibility, flush-watermark failure, and
  WAL-truncation failure.
- `LifecycleFlushWatermarkStatus` records persisted and already-persisted
  outcomes.
- `LifecycleWalTruncationStatus` records completed and
  completed-with-health-debt outcomes.
- `WalRetentionProofSource` remains the only truncation proof source vocabulary
  that lifecycle can pass downward.
- Checkpoint follow-up failures are represented as maintenance health debt,
  not clean checkpoint success.

### Intentional Changes

- Lifecycle does not scan WAL records or parse segment object names. Coverage
  and active-segment protection remain owned by L4 WAL service logic.
- Checkpoint row capture uses L6 branch row ordering and L7 commit quiesce. The
  checkpoint watermark is the visible version, not the allocator frontier.
- Snapshot publication is delegated to the checkpoint service. Tests pin the
  service order: active-WAL facts, snapshot create, then live snapshot facts.
- Cache mode exposes no checkpoint/flush-watermark/WAL-truncation durable
  claim surface; source guards keep cache lifecycle code away from durable
  services.
- Generated checkpoint coverage tracks input-derived counters separately from
  direct unit tests.

### Retired From V1 L8J

- Old primitive checkpoint section DTOs.
- Product command naming or public maintenance command behavior.
- Direct filesystem/path/object-name parsing for WAL retention.
- Table-object-only flush facts as a replay-shortening proof.
- Logs-only fault handling for partial checkpoint or WAL delete failures.

### Deferred By Owner Slice

- Snapshot pruning after successful checkpoint: L8L.
- Local filesystem checkpoint/recovery integration harness: L8O/L8P.
- Multi-branch public lifecycle wrapper behavior: L9.

### Tests Added

- `checkpoint_task_rejects_wrong_maintenance_scope`
- `checkpoint_rows_include_tombstones_and_timeline_rows`
- `checkpoint_watermark_uses_visible_version_not_allocated_version`
- `checkpoint_snapshot_publish_failure_releases_quiesce_and_keeps_recovery_facts`
- `checkpoint_publishes_snapshot_between_database_record_updates`
- `checkpoint_manifest_publish_failure_reports_partial_snapshot`
- `checkpoint_manifest_uncertainty_reports_uncertain_status`
- `checkpoint_existing_snapshot_id_collision_fails_closed`
- `checkpoint_with_truncation_skips_delete_when_deferred`
- `checkpoint_recovery_restores_rows_without_covered_log_records`
- `checkpoint_recovery_restores_tombstone_and_timeline_rows`
- `flush_watermark_rejects_bounds_and_preserves_branch_state`
- `flush_watermark_persist_failure_preserves_source_chain`
- `wal_truncation_from_checkpoint_and_flush_proofs_are_typed`
- `duplicate_checkpoint_tasks_coalesce_by_checkpoint_scope`
- `queued_checkpoint_task_failure_adds_health_debt`
- `duplicate_wal_truncation_tasks_coalesce_by_retention_scope`
- `lifecycle_property_harness_runs_checkpoint_contract`
- `lifecycle_checkpoint_runtime_avoids_segment_parsing_and_direct_delete`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Quiesce omitted | `crates/storage-next/src/lifecycle/checkpoint.rs` | Remove commit quiesce before checkpoint row capture | `checkpoint_reads_visible_version_after_commit_quiesce` |
| Allocator watermark used | `crates/storage-next/src/lifecycle/checkpoint.rs` | Use allocator frontier instead of visible version | `checkpoint_watermark_uses_visible_version_not_allocated_version` |
| Hidden rows captured | `crates/storage-next/src/branch/state.rs` | Include rows above checkpoint watermark | `checkpoint_rows_include_owned_frozen_active_and_exclude_newer_rows` |
| Tombstones dropped | `crates/storage-next/src/branch/state.rs` | Filter tombstone rows from checkpoint rows | `checkpoint_rows_include_tombstones_and_timeline_rows` |
| Timeline rows dropped | `crates/storage-next/src/branch/state.rs` | Filter timeline rows from checkpoint rows | `checkpoint_rows_include_tombstones_and_timeline_rows` |
| Snapshot/manifest order inverted | `crates/storage-next/src/service/checkpoint.rs` | Persist snapshot facts before snapshot create | `checkpoint_publishes_snapshot_between_database_record_updates` |
| Partial snapshot marked success | `crates/storage-next/src/lifecycle/checkpoint.rs` | Collapse orphan snapshot status to completed | `checkpoint_manifest_publish_failure_reports_partial_snapshot` |
| Table-only flush proof accepted | `crates/storage-next/src/lifecycle/checkpoint.rs` | Allow table-only proof in flush watermark persistence | `flush_watermark_proofs_are_conservative_and_monotonic` |
| Branch absence advances watermark | `crates/storage-next/src/lifecycle/checkpoint.rs` | Treat no rows as flush proof | `checkpoint_defers_when_branch_has_no_rows_under_visible_watermark` |
| Primitive truncation watermark | `crates/storage-next/src/lifecycle/checkpoint.rs` | Replace typed retention proof with raw commit version | `wal_truncation_from_checkpoint_and_flush_proofs_are_typed` |
| Active segment deleted | `crates/storage-next/src/service/wal.rs` | Remove active-segment protection in covered delete | `checkpoint_recovery_restores_rows_without_covered_log_records` |
| Delete failure ignored | `crates/storage-next/src/lifecycle/checkpoint.rs` | Return clean success after WAL delete error | `checkpoint_reports_wal_truncation_failure_without_losing_snapshot_facts` |
| Cache mode creates durable facts | `crates/storage-next/src/lifecycle/cache.rs` | Import durable services into cache lifecycle | `lifecycle_cache_runtime_stays_cache_only` |
| Old primitive DTO imported | `crates/storage-next/src/lifecycle/*.rs` | Reintroduce old checkpoint DTO vocabulary | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Architecture label added to code | `crates/storage-next/src/lifecycle/*.rs`, `crates/storage-next/src/testkit/lifecycle/*.rs`, `crates/storage-next/tests/lifecycle_*.rs` | Put milestone labels in implementation comments, strings, or test names | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8J:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8L - Retention Proof And Snapshot Pruning

### Shipped Files

- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/tests/retention.rs`
- `crates/storage-next/src/testkit/lifecycle/retention.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-test-plan.md`

### Preserved As Storage Vocabulary

- Retention proof distinguishes complete, incomplete, and
  recovery-health-blocked states.
- Snapshot pruning keeps the manifest-live snapshot, keeps the newest retained
  snapshot window, and clamps zero retain count to one.
- Snapshot pruning requires manifest live-snapshot facts even when the current
  snapshot listing is empty; an empty listing is not treated as a durable safety
  proof.
- Snapshot delete failures become health debt while preserving successfully
  deleted and protected snapshot facts.
- Table objects are classified as retained or quarantine candidates. Lifecycle
  retention does not delete table objects directly. Automatic table-reachability
  proof assembly remains deferred until durable table-manifest/quarantine work.
- WAL and quarantine object families are delegated with explicit skipped
  decisions rather than partially implemented in retention.

### Raw Health And Fact Vocabulary

- `LifecycleRetentionProofStatus` records complete, incomplete, and
  blocked-by-recovery-health proof states.
- `LifecycleRetentionDecisionRecord` records object family, decision, optional
  object name, and storage-shaped reason.
- `LifecycleSnapshotPruningOutcome` records deleted, protected, and failed
  snapshot objects and converts failed deletes into telemetry health debt.
- Maintenance outcomes preserve affected object names, state-change counts,
  source chains for service errors, and retention-block stats.

### Intentional Changes

- Snapshot deletion is delegated exclusively to `SnapshotService::prune_snapshots`.
- Retention code never parses WAL segments, truncates WAL objects, mutates
  quarantine inventory, or deletes table objects.
- Retention task coalescing includes the snapshot-retain policy so explicit
  pruning windows are not lost.
- Global retention maintenance runs snapshot pruning and still reports delegated
  WAL/quarantine families rather than returning only delegated decisions.
- Cache mode rejects durable retention and snapshot-pruning tasks before any
  durable-object access.

### Retired From V1 L8L

- Product retention reports and branch-attribution DTOs.
- Direct filesystem/path deletion.
- Logs-only snapshot pruning diagnostics.
- Table-object purge and quarantine mutation.
- Row-version pruning policy.
- WAL segment parsing or deletion from retention code.

### Deferred By Owner Slice

- Quarantine inventory publication, movement, purge, and repair: L8M.
- Close-time retention drain: L8N.
- Crash/fuzz assurance expansion: L8O/L8P.
- Public retention commands and product reports: L9.
- Automatic table-reachability proof assembly and table-manifest-backed direct
  table-object deletion, if ever allowed: later durable table-manifest work.

### Tests Added

- `retention_request_accepts_zero_snapshot_retain_as_clamped_policy`
- `retention_proof_incomplete_without_manifest_snapshot_when_snapshots_exist`
- `retention_proof_incomplete_without_manifest_snapshot_even_when_listing_empty`
- `retention_proof_incomplete_without_branch_reachability_for_tables`
- `incomplete_snapshot_pruning_proof_defers_before_backend_access`
- `retention_proof_blocks_data_loss_before_backend_access`
- `retention_scope_snapshot_decisions_respect_live_and_newest_windows`
- `global_retention_scope_includes_snapshot_and_delegated_decisions`
- `snapshot_pruning_retains_live_snapshot_outside_newest_window`
- `snapshot_pruning_clamps_zero_retain_count_to_one`
- `snapshot_pruning_delete_failure_records_health_debt_and_continues`
- `snapshot_pruning_list_failure_preserves_service_source_chain`
- `table_object_retention_classifies_quarantine_candidate_without_backend_delete`
- `retention_delegates_wal_and_quarantine_families`
- `snapshot_pruning_tasks_coalesce_by_retain_policy`
- `global_retention_task_prunes_snapshots_through_durable_maintenance`
- `prove_retention_respects_snapshot_scope_without_deleting`
- `cache_runtime_rejects_durable_retention_tasks_before_backend_access`
- `lifecycle_retention_proof_integration`
- `lifecycle_snapshot_pruning_integration`
- `lifecycle_table_retention_delegation_integration`
- `lifecycle_retention_source_delegates_durable_mutation`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Retain count zero deletes all | `crates/storage-next/src/lifecycle/retention.rs` | Pass zero directly as "retain none" | `snapshot_pruning_clamps_zero_retain_count_to_one` |
| Live snapshot not protected | `crates/storage-next/src/lifecycle/retention.rs` | Drop live snapshot id before pruning | `snapshot_pruning_retains_live_snapshot_outside_newest_window` |
| Empty listing trusted without manifest | `crates/storage-next/src/lifecycle/retention.rs` | Treat empty snapshot listing as complete proof without manifest facts | `retention_proof_incomplete_without_manifest_snapshot_even_when_listing_empty` |
| Global retention skips snapshots | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Route global retention only to delegated WAL/quarantine decisions | `global_retention_task_prunes_snapshots_through_durable_maintenance` |
| Scope ignored by proof hook | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Return delegated WAL/quarantine decisions for snapshot-only proof requests | `prove_retention_respects_snapshot_scope_without_deleting` |
| Incomplete proof deletes | `crates/storage-next/src/lifecycle/retention.rs` | Call snapshot service when proof is incomplete | `incomplete_snapshot_pruning_proof_defers_before_backend_access` |
| Data-loss recovery prunes | `crates/storage-next/src/lifecycle/retention.rs` | Treat data-loss health as safe | `retention_proof_blocks_data_loss_before_backend_access` |
| Delete failure hidden | `crates/storage-next/src/lifecycle/retention.rs` | Collapse failed deletes into completed outcome | `snapshot_pruning_delete_failure_records_health_debt_and_continues` |
| Service source chain dropped | `crates/storage-next/src/lifecycle/retention.rs` | Convert list failure into string-only error | `snapshot_pruning_list_failure_preserves_service_source_chain` |
| Table object deleted directly | `crates/storage-next/src/lifecycle/retention.rs` | Call backend delete for table candidates | `lifecycle_retention_source_delegates_durable_mutation` |
| WAL truncation in retention | `crates/storage-next/src/lifecycle/retention.rs` | Call WAL segment deletion from retention | `lifecycle_retention_source_delegates_durable_mutation` |
| Quarantine mutation in retention | `crates/storage-next/src/lifecycle/retention.rs` | Call quarantine mutation/purge APIs | `lifecycle_retention_source_delegates_durable_mutation` |
| Retain policy coalesced away | `crates/storage-next/src/lifecycle/maintenance.rs` | Ignore retain options in coalesce key | `snapshot_pruning_tasks_coalesce_by_retain_policy` |

### Verification

Commands run for L8L:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
