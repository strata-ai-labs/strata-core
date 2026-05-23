# L8H Test Plan: Maintenance Task Executor

Status: implemented test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that L8H provides a deterministic, storage-internal maintenance executor
that later L8 slices can use without background-thread nondeterminism or product
scheduler coupling.

Tests should fail if L8H:

1. accepts ordinary maintenance outside `Open`;
2. runs close-required drain outside `Closing`;
3. uses non-deterministic ordering for equal-priority tasks;
4. loses or reorders tasks during coalescing;
5. silently drops duplicate tasks without reporting coalescing;
6. runs cancelable ordinary tasks during close drain;
7. leaves an active task after success, deferred, failure, or error;
8. allows queue depth to exceed the configured limit;
9. reports maintenance readiness when recovery health makes maintenance unsafe;
10. imports engine/product/background scheduler code;
11. adds sleeps, wall-clock waits, or real threads to unit tests;
12. implements flush/checkpoint/compaction/reclaim behavior inside L8H.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/maintenance.rs` for direct executor
   and runtime-integration unit tests.
2. `crates/storage-next/src/testkit/lifecycle/maintenance.rs` for generated
   maintenance scripts, counters, and model checks.
3. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration tests
   that exercise cache and durable runtime surfaces in memory.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated lifecycle
   properties behind `testkit`.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8H verification and sensitivity-probe record after implementation.

## Test Data Principles

1. Use fake deterministic task runners for L8H. Do not call real L4/L5/L6
   maintenance behavior in this slice.
2. Represent task effects with storage facts: task kind, task id, affected
   object count, reclaimed bytes, retryability, and health debt.
3. Keep canonical smoke scripts separate from generated-input coverage.
4. Generated tests must count input-derived operations separately from fixed
   setup operations.
5. Avoid reserved layout string literals in fixture names.
6. Assert `LifecycleError::code()` and variant shape, not display strings.
7. Tests must stay deterministic without sleeps, timers, or thread races.

## Direct Unit Tests

### 1. Task Construction And Validation

Required tests:

1. `maintenance_task_allocates_monotonic_ids`
2. `maintenance_task_rejects_invalid_scope_for_branch_level`
3. `maintenance_task_accepts_every_task_kind`
4. `maintenance_priority_orders_critical_high_normal_low`
5. `maintenance_coalesce_key_includes_kind_and_scope`
6. `maintenance_policy_distinguishes_drain_and_cancel`
7. `maintenance_task_debug_uses_storage_vocabulary`

Assertions:

1. task ids are deterministic and monotonic;
2. no task constructor accepts malformed branch/level/scope facts;
3. task facts do not include product names or user command wording.

### 2. Queue Admission

Required tests:

1. `ordinary_maintenance_requires_open_state`
2. `ordinary_maintenance_rejects_new_opening_recovering_closing_closed_failed`
3. `close_required_drain_requires_closing_state`
4. `health_status_is_admitted_in_every_state`
5. `queue_full_returns_typed_maintenance_error`
6. `queue_capacity_one_accepts_one_and_rejects_second`
7. `queue_depth_exactly_at_capacity_is_allowed`

Assertions:

1. executor entry points call lifecycle admission;
2. rejected admission does not mutate queue, active task, stats, or ids;
3. queue-full errors use lifecycle maintenance error codes.

### 3. Deterministic Ordering

Required tests:

1. `executor_runs_highest_priority_first`
2. `executor_preserves_fifo_for_equal_priority`
3. `executor_order_is_stable_after_interleaved_coalescing`
4. `executor_order_is_stable_after_canceling_pending_tasks`
5. `executor_does_not_depend_on_map_iteration_order`

Assertions:

1. priority is the primary sort key;
2. enqueue sequence is the equal-priority tiebreaker;
3. coalescing does not update a task's sequence unless explicitly documented.

### 4. Coalescing

Required tests:

1. `duplicate_flush_task_coalesces_by_branch`
2. `duplicate_checkpoint_task_coalesces_by_checkpoint_scope`
3. `duplicate_wal_truncation_task_coalesces_by_wal_scope`
4. `duplicate_compaction_task_coalesces_by_branch_level`
5. `duplicate_materialization_task_coalesces_by_branch_layer_scope`
6. `purge_tasks_do_not_coalesce_by_default`
7. `repair_tasks_do_not_coalesce_by_default`
8. `active_task_is_not_coalesced_away`
9. `coalesced_enqueue_reports_existing_task_id`
10. `coalesced_enqueue_increments_coalesced_metric`

Assertions:

1. queue length changes only for real enqueues;
2. coalescing returns an explicit fact;
3. duplicate handling is deterministic by kind and scope.

### 5. Run Protocol

Required tests:

1. `run_next_marks_task_active_before_runner`
2. `run_next_clears_active_after_completed`
3. `run_next_clears_active_after_deferred`
4. `run_next_clears_active_after_failed_outcome`
5. `run_next_clears_active_after_runner_error`
6. `run_next_updates_completed_metric`
7. `run_next_updates_deferred_metric`
8. `run_next_updates_failed_metric`
9. `run_next_on_empty_queue_returns_deferred_or_no_work_outcome`
10. `runner_error_preserves_source_chain`

Assertions:

1. no normal or error path leaves active work behind;
2. runner errors do not lose source chains;
3. `MaintenanceOutcome` effects are preserved exactly.

### 6. Drain And Cancel

Required tests:

1. `cancel_pending_removes_only_cancelable_tasks`
2. `cancel_pending_does_not_cancel_active_task`
3. `drain_runs_only_drain_required_tasks`
4. `drain_skips_ordinary_cancelable_tasks`
5. `drain_empty_queue_is_idempotent`
6. `cancel_empty_queue_is_idempotent`
7. `drain_reports_drained_count`
8. `cancel_reports_canceled_count`
9. `close_drain_rejects_when_lifecycle_is_open`
10. `ordinary_run_rejects_when_lifecycle_is_closing`

Assertions:

1. close drain cannot accidentally start ordinary maintenance;
2. cancellation affects pending tasks only;
3. repeated close integration calls are deterministic.

### 7. Fault Hooks

Required tests:

1. `fault_before_enqueue_leaves_queue_unchanged`
2. `fault_after_enqueue_keeps_pending_task_observable`
3. `fault_at_task_start_clears_active_and_records_failure`
4. `fault_after_runner_success_converts_to_failed_outcome`
5. `fault_during_drain_records_failed_drain_metric`
6. `fault_hooks_fire_in_deterministic_order`

Assertions:

1. every injected fault has a typed lifecycle error or failed outcome;
2. metrics identify the phase that failed;
3. no fault path uses panic as control flow.

### 8. Maintenance Readiness

Required tests:

1. `cache_open_reports_maintenance_ready_after_executor_attached`
2. `durable_healthy_open_reports_maintenance_ready_after_executor_attached`
3. `durable_telemetry_degraded_open_reports_maintenance_ready`
4. `durable_data_loss_degraded_open_reports_maintenance_not_ready`
5. `durable_policy_downgrade_open_reports_maintenance_not_ready`
6. `failed_recovery_never_reports_maintenance_ready`
7. `readiness_does_not_mean_durable_tasks_are_supported_in_cache_mode`

Assertions:

1. readiness reflects executor availability and recovery safety;
2. cache mode can be scheduler-ready while durable handlers still defer;
3. data-loss and policy-downgrade recovery remain conservative.

### 9. Runtime Integration

Required tests:

1. `cache_runtime_can_enqueue_and_run_health_collection`
2. `cache_runtime_defers_durable_only_task`
3. `durable_runtime_can_enqueue_and_run_health_collection`
4. `runtime_status_reports_pending_and_active_counts`
5. `runtime_stats_reflect_executor_work`
6. `runtime_close_path_can_cancel_pending_tasks_without_running_them`
7. `runtime_close_path_can_drain_required_tasks`

Assertions:

1. runtime methods are crate-private;
2. cache integration does not import durable services;
3. durable integration does not run concrete flush/checkpoint behavior in L8H.

## Integration Tests

Add `crates/storage-next/tests/lifecycle_maintenance.rs`.

Required tests:

1. memory cache open can enqueue, coalesce, run, cancel, and drain tasks;
2. memory durable open can enqueue, coalesce, run, cancel, and drain tasks;
3. queue-full behavior matches `LifecycleConfig::max_maintenance_queue_depth`;
4. source chains survive through integration-level runner failures;
5. no raw object-layout literals are needed in maintenance fixture names.

Localfs integration is not required for L8H because no durable maintenance
operation is implemented yet. Localfs crash/reopen maintenance tests belong to
L8I-L8O.

## Generated Properties

Add a maintenance script model in `src/testkit/lifecycle/maintenance.rs`.

Script operations:

1. enqueue task;
2. enqueue duplicate task;
3. run next task;
4. cancel pending;
5. drain required tasks;
6. inject fault;
7. transition model state to open/closing/closed/failed;
8. query status.

Model facts:

1. pending task list;
2. active task;
3. completed task ids;
4. canceled task ids;
5. failed task ids;
6. coalesced request count;
7. queue-full count;
8. admission rejection count.

Required generated assertions:

1. production pending order equals model pending order;
2. production completed/canceled/failed counts equal model counts;
3. active task is empty after every operation boundary;
4. generated input, not only canonical setup, reaches enqueue, coalesce, run,
   cancel, drain, and fault categories.

## Source Guards

Extend `lifecycle_source_guard.rs`.

Required checks:

1. `src/lifecycle/maintenance.rs` has no imports from engine, product,
   StrataHub, follower, raw filesystem, or environment modules;
2. cache lifecycle files do not import L4 durable services for maintenance;
3. lower layers do not import `crate::lifecycle`;
4. lifecycle maintenance tests contain no sleeps or thread spawns;
5. no lifecycle maintenance symbol is exported as `pub` from the crate root;
6. lifecycle maintenance source does not contain public user-command wording.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| H1 | Allow ordinary maintenance in `Closing`. | Admission test fails. |
| H2 | Run equal-priority tasks in reverse insertion order. | FIFO ordering test/property fails. |
| H3 | Drop duplicate task without reporting coalescing. | Coalescing metric test fails. |
| H4 | Coalesce active task away. | Active-task coalescing test fails. |
| H5 | Ignore queue capacity. | Queue-full test fails. |
| H6 | Leave active set after runner error. | Active cleanup test fails. |
| H7 | Run cancelable ordinary task during close drain. | Drain policy test fails. |
| H8 | Report maintenance ready after data-loss recovery. | Readiness test fails. |
| H9 | Import engine background scheduler. | Source guard fails. |
| H10 | Add sleep-based executor test. | Source guard fails. |

## Deferred Coverage

These are not L8H obligations:

1. flush publication fault windows;
2. checkpoint publication and WAL truncation fault windows;
3. compaction/materialization lower-layer mutation;
4. retention proof and snapshot pruning;
5. quarantine, purge, and repair mutation;
6. full close timeout behavior;
7. localfs crash/reopen maintenance tests;
8. fuzz target inventory.

Each deferred item must be covered by the later slice that implements the
concrete handler.

## Verification Commands

After implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

When `cargo-hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Exit Criteria

L8H tests are complete when:

1. every executor transition has a direct unit test;
2. generated maintenance scripts compare against an independent model;
3. cache and durable runtimes both use the executor shape;
4. queue-full, coalescing, drain, cancel, and fault hooks are covered;
5. readiness semantics are tested across healthy, telemetry-degraded,
   data-loss-degraded, and policy-downgraded recovery;
6. source guards prevent executor scope drift;
7. the L8 porting log records shipped files, verification commands, and
   sensitivity probes.
