# L8N Test Plan: Close And Shutdown Ordering

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that storage close is ordered, retryable where required, idempotent after
success, and mode-correct.

Tests should fail if L8N:

1. accepts commits or ordinary maintenance after close begins;
2. starts ordinary maintenance during close;
3. drops drain-required work without running or reporting it;
4. leaves cancelable work runnable after `Closed`;
5. reports clean durable close without WAL/final sync facts;
6. releases the writer guard more than once;
7. collapses timeout or sync failure into success;
8. loses lower-layer source chains;
9. calls durable services from cache close;
10. reintroduces durable close logic into recovery bootstrap;
11. uses display strings instead of stable error codes in tests.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Coverage Boundary

In scope:

1. close request, state admission, outcome, effects, and error validation;
2. cache close alignment;
3. durable close phase ordering;
4. maintenance cancel/drain integration;
5. commit quiesce integration;
6. WAL close/sync and final fact publication;
7. writer guard release;
8. retry and idempotence;
9. generated testkit counters;
10. source guards.

Out of scope:

1. public database close API;
2. engine primitive freeze hooks;
3. IPC/server shutdown;
4. background worker threads;
5. product branch registry release;
6. branch deletion/clear policy;
7. crash/fuzz closeout;
8. StrataHub push/pull behavior.

Those belong to L9, engine-next, L8O/L8P, or later lifecycle work.

## Old-Code Regression Sources

The old codebase supplies behavior requirements, not type names.

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/engine/src/database/lifecycle.rs` | Shutdown gates new work before draining and releasing durable ownership. | Close admission happens before cancel/drain/quiesce/sync/release; commits reject after close starts. |
| `crates/engine/src/background.rs` | Background work has drain/cancel policy and does not rely on sleeps for correctness. | Executor policies determine close behavior; deterministic tests do not sleep. |
| `crates/storage/src/durability/checkpoint_runtime.rs` | Durable state is synced before clean shutdown is reported. | WAL/service sync failure prevents `Complete` close status. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Writer ownership is released during close and can be reacquired on reopen. | Writer guard release is exactly-once; a second durable runtime can acquire after close. |
| `crates/storage/src/durability/compaction/wal_only.rs` | Retention/compaction work does not run after shutdown starts. | Ordinary maintenance is not started during close. |

Tests must not port:

1. raw filesystem close/fsync code;
2. product close callbacks;
3. engine registry or IPC shutdown;
4. product logs;
5. background thread sleeps.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/close.rs` for shared close tests if
   a new file is added.
2. `crates/storage-next/src/lifecycle/tests/cache.rs` for cache close cases.
3. `crates/storage-next/src/lifecycle/tests/durable.rs` for durable shell close
   and writer-guard cases.
4. `crates/storage-next/src/lifecycle/tests/maintenance.rs` for executor
   cancel/drain behavior.
5. `crates/storage-next/src/lifecycle/tests/checkpoint.rs` only for close sync
   interactions that reuse checkpoint/final-health helpers.
6. `crates/storage-next/src/testkit/lifecycle/close.rs` for generated close
   scripts if the testkit splits by lifecycle family.
7. `crates/storage-next/tests/lifecycle_maintenance.rs` or a dedicated
   integration target for close integration smoke tests.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Keep Rust test names behavior-focused. Do not put milestone or slice labels in
Rust test names, comments, fixture bytes, or panic messages.

## Direct Unit Tests

### 1. Close Request And Outcome Validation

Required tests:

1. `close_request_default_is_valid`
2. `close_request_rejects_invalid_timeout_policy`
3. `close_outcome_reports_prior_and_final_state`
4. `close_outcome_reports_canceled_and_drained_task_counts`
5. `close_outcome_reports_quiesce_sync_and_guard_effects`
6. `close_error_preserves_source_error_chain`
7. `close_outcome_complete_requires_closed_final_state`
8. `close_outcome_timeout_is_retryable`
9. `close_outcome_idempotent_requires_prior_final_fact`
10. `close_error_codes_are_stable`

Assertions:

1. tests assert `code()` and source-chain class, not display strings;
2. invalid outcome combinations fail closed;
3. source errors are preserved through the `LifecycleError` returned by
   `LifecycleResult<CloseOutcome>`.

### 2. Lifecycle State Admission

Required tests:

1. `close_from_open_transitions_to_closing_before_side_effects`
2. `close_from_closed_is_idempotent`
3. `close_from_new_rejects`
4. `close_from_opening_rejects`
5. `close_from_recovering_rejects`
6. `close_from_failed_rejects_unless_retryable_close_failure`
7. `close_retry_pending_allows_retry`
8. `close_requested_blocks_ordinary_maintenance`
9. `close_requested_blocks_mutating_commits`
10. `diagnostic_health_query_allowed_after_close`

Assertions:

1. `LifecycleCloseFact::Requested` is recorded before drain/quiesce/sync;
2. failed admission performs no maintenance or backend calls.

### 3. Cache Close

Required tests:

1. `cache_close_cancels_cancelable_pending_work`
2. `cache_close_rejects_or_drains_drain_required_work_by_policy`
3. `cache_close_cancels_ordinary_pending_work_before_closed`
4. `cache_close_does_not_start_ordinary_maintenance`
5. `cache_close_does_not_call_wal_manifest_snapshot_table_or_quarantine_services`
6. `cache_close_reports_no_durable_sync`
7. `cache_close_releases_volatile_guards`
8. `cache_double_close_returns_idempotent_prior_facts`
9. `cache_commit_after_close_rejects_before_allocation`
10. `cache_read_after_close_rejects_as_lifecycle_state`

Assertions:

1. durable backend operation count stays zero except capability preflight from
   open;
2. pending task count is zero after clean close unless the close returned a
   typed retryable failure;
3. second close does not modify stats except close-attempt accounting if that is
   the documented behavior.

### 4. Maintenance Drain And Cancel

Required tests:

1. `close_cancel_sweep_removes_cancel_before_close_tasks`
2. `close_cancel_sweep_removes_or_defers_ordinary_tasks_by_contract`
3. `close_drain_runs_drain_before_close_tasks`
4. `close_drain_preserves_task_order`
5. `close_drain_failure_returns_typed_close_error`
6. `close_drain_timeout_returns_timeout_and_retry_fact`
7. `close_does_not_cancel_active_task_by_queue_removal`
8. `close_retry_after_drain_failure_does_not_rerun_completed_tasks`
9. `close_outcome_includes_maintenance_stats`
10. `close_outcome_includes_completed_drain_task_stats`

Assertions:

1. drain uses `LifecycleOperationKind::CloseRequiredDrain`;
2. ordinary maintenance is never started during close;
3. task source errors cross the close `LifecycleError` boundary.

### 5. Commit Quiesce

Required tests:

1. `close_acquires_commit_quiesce_after_maintenance_drain`
2. `active_commit_guard_causes_typed_close_timeout`
3. `quiesce_blocks_new_branch_guards_until_close_completes`
4. `quiesce_guard_released_on_retryable_failure_when_contract_allows_retry`
5. `quiesce_guard_not_reacquired_on_idempotent_second_close`
6. `commit_after_close_requested_rejects_before_version_allocation`
7. `cross_branch_commit_after_quiesce_rejects`

Assertions:

1. close uses `CommitBranchGuardSet::try_begin_quiesce`;
2. allocator and branch state do not advance on rejected post-close commits.

### 6. Durable WAL And Final Sync

Required tests:

1. `durable_close_calls_wal_close_in_standard_mode`
2. `durable_close_calls_wal_close_in_always_mode`
3. `durable_close_wal_close_failure_returns_typed_source_chain`
4. `durable_close_wal_sync_uncertain_returns_retry_pending`
5. `durable_close_does_not_report_complete_with_unresolved_durable_gate`
6. `durable_close_force_syncs_manifest_when_health_changed`
   (formerly `durable_close_persists_final_health_fact_when_dirty`; renamed
   to reflect that V1 does not persist health into the manifest payload —
   the close-time publish forces a final fsync only)
7. `durable_close_skips_manifest_write_when_no_final_fact_dirty`
8. `durable_close_manifest_publish_failure_returns_typed_source_chain`
9. `durable_close_does_not_truncate_wal_unless_drain_task_did_so`
10. `durable_close_does_not_prune_snapshots_or_purge_quarantine_implicitly`

Assertions:

1. WAL close/sync occurs after commit quiesce;
2. clean close requires durable sync effect;
3. lower-layer WAL/manifest errors remain available through `Error::source`.

### 7. Writer Guard Release

Required tests:

1. `durable_close_releases_writer_guard_after_sync`
2. `durable_close_does_not_release_writer_guard_before_sync_failure`
3. `durable_close_reports_typed_error_when_writer_guard_is_missing_at_release`
4. `durable_double_close_does_not_double_release_writer_guard`
5. `durable_reopen_can_acquire_writer_guard_after_close`
6. `durable_failed_close_keeps_guard_when_retry_requires_it`
7. `durable_retry_after_release_does_not_use_released_guard`

Assertions:

1. release ordering is visible in the fake backend operation log;
2. lock release is represented in close outcome effects;
3. no test relies on `Drop` timing without an observable reacquire assertion.

### 8. Retry And Idempotence

Required tests:

1. `close_timeout_leaves_runtime_retryable`
2. `close_retry_after_timeout_completes_when_blocker_clears`
3. `close_retry_after_wal_failure_retries_sync_phase`
4. `close_retry_after_manifest_failure_retries_final_fact_phase`
5. `close_retry_after_missing_writer_guard_failure_retries_release_phase`
6. `close_retry_does_not_restart_completed_ordinary_work`
7. `double_close_after_success_returns_prior_final_outcome`
8. `double_close_after_success_does_not_touch_backend`
9. `close_after_failed_nonretryable_state_rejects`

Assertions:

1. retry facts are explicit;
2. completed non-idempotent phases are not repeated;
3. idempotent second close differs from retry.

### 9. Failure Windows

Required tests:

1. `close_failure_before_cancel_leaves_tasks_pending`
2. `close_failure_after_cancel_before_drain_reports_canceled_count`
3. `close_failure_during_drain_preserves_completed_drain_facts`
4. `close_failure_during_quiesce_preserves_drain_facts`
5. `close_failure_during_wal_sync_preserves_quiesce_fact`
6. `close_failure_during_manifest_sync_preserves_wal_fact`
7. `close_failure_during_guard_release_preserves_sync_fact`
8. `close_recovery_health_debt_is_not_lost_on_failure`

Assertions:

1. every phase failure reports the phase where it occurred;
2. partial progress is visible in the close outcome;
3. no failure is converted to clean `Closed`.

### 10. Integration Tests

Required tests:

1. `cache_open_commit_close_reopen_is_empty_and_no_durable_calls`
2. `durable_open_commit_close_reopen_recovers_committed_rows`
3. `durable_close_after_checkpoint_does_not_rewrite_checkpoint_without_dirty_fact`
4. `durable_close_after_flush_does_not_advance_flush_watermark_unless_checkpointed`
5. `durable_close_with_pending_retention_drain_runs_required_task`
6. `durable_close_with_pending_quarantine_drain_preserves_reclaim_facts`
7. `durable_close_with_ordinary_compaction_task_does_not_start_compaction`
8. `durable_close_after_failed_maintenance_reports_health_debt`
9. `second_durable_runtime_can_open_after_first_clean_close`

Assertions:

1. integration tests use storage-next durable services, not product wrappers;
2. localfs integration can be feature-gated if the repo keeps that convention;
3. cache-mode integration must not create durable objects.

## Generated Testkit

Add a close contract to the lifecycle testkit.

Required counters:

1. close requested;
2. cache close completed;
3. durable close completed;
4. idempotent close;
5. retryable timeout;
6. drain-required task completed;
7. cancelable task canceled;
8. ordinary task not started after close;
9. commit quiesce acquired;
10. commit quiesce blocked;
11. WAL sync failure;
12. manifest sync failure;
13. guard release observed;
14. source chain preserved.

Generated scripts should include:

1. close before work;
2. close after cache commit;
3. close after durable commit;
4. close with pending ordinary task;
5. close with pending drain-required task;
6. close with active commit guard;
7. close with WAL failure;
8. close retry after blocker clears;
9. double close after success.

Coverage assertions:

1. every generated script reaches a close attempt;
2. default cases exercise cache and durable mode;
3. timeout and retry counters are reached by at least one canonical seed;
4. arbitrary input must not panic.

## Source Guards

Required guards:

1. `lifecycle/cache.rs` does not import `crate::service`, `crate::layout`, or
   durable format modules for close.
2. durable close logic lives in a close-focused durable module, not
   `lifecycle/durable/bootstrap.rs`.
3. lifecycle close code does not import `std::fs`, `std::path::Path`,
   `std::env`, `OpenOptions`, `File`, or mmap APIs.
4. lifecycle close code does not import engine, product, graph, vector, search,
   embedding, inference, or StrataHub modules.
5. `branch`, `table`, `commit`, `format`, and `service` production modules do
   not import lifecycle close.
6. Rust code, test names, comments, fixture bytes, and panic messages do not
   contain milestone or slice labels.
7. tests assert stable error codes rather than display strings.

## Sensitivity Probes

Record probe results in the porting log after implementation.

| Probe | Mutation | Expected failing test family |
|---|---|---|
| S1 | Allow commit admission after close requested. | State admission, commit quiesce, generated close. |
| S2 | Skip cancel-before-close sweep. | Cache close, maintenance drain/cancel. |
| S3 | Drop drain-before-close tasks instead of running them. | Maintenance drain, integration drain. |
| S4 | Start ordinary maintenance while closing. | Maintenance drain, source/generated close. |
| S5 | Ignore active commit guard during close. | Commit quiesce timeout. |
| S6 | Report WAL sync failure as complete. | Durable sync failure. |
| S7 | Release writer guard before durable sync. | Guard release ordering. |
| S8 | Release writer guard twice on double close. | Idempotence and fake backend release counter. |
| S9 | Call durable services from cache close. | Cache behavior and source guard. |
| S10 | Put durable close code in bootstrap. | Source guard. |
| S11 | Drop lower-layer source chain from close failure. | Error/source-chain tests. |
| S12 | Treat retry as idempotent close. | Retry/idempotence tests. |
| S13 | Clear pending ordinary tasks without reporting cancel/defer facts. | Close outcome and maintenance tests. |
| S14 | Persist final manifest before commit quiesce. | Durable sync ordering. |
| S15 | Convert timeout into `Closed`. | State/outcome tests. |

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::close
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib commit::tests::guard
cargo test -p strata-storage-next --locked --lib service::wal
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo fmt --package strata-storage-next --check
git diff --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```

If a dedicated close integration target is added, include it in the command
matrix and porting log.

## Exit Criteria

L8N is test-complete when:

1. close admission, drain/cancel, quiesce, sync, release, retry, and idempotence
   have direct tests;
2. cache close proves absence of durable service calls;
3. durable close proves WAL/final sync before clean close;
4. writer guard release is exactly-once and reacquire is tested;
5. failure windows preserve phase and source chain;
6. generated testkit counters cover close success and retry paths;
7. source guards cover cache/durable/bootstrap boundaries;
8. the verification command matrix passes;
9. the porting log records sensitivity probes and deferred items.

## Deferred

1. Public close API and product error mapping.
2. Engine primitive freeze and IPC shutdown.
3. Real background worker thread shutdown.
4. Crash/reopen fuzz around close failure windows.
5. Distributed lease renewal or handoff beyond the current writer guard.
6. Branch delete/clear close policy.
