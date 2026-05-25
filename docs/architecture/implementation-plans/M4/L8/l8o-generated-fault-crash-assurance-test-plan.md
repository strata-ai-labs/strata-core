# L8O Test Plan: Generated, Fault, And Crash Assurance

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove lifecycle behavior across generated operation sequences, direct fault
windows, crash/reopen boundaries, and fuzz target routing.

L8O tests should fail if lifecycle:

1. only works for hand-written happy paths;
2. lets generated coverage pass through unconditional canonical smoke setup;
3. loses recovery health or source-chain facts during fault windows;
4. deletes or purges objects beyond a model-proven safe set;
5. advances visibility, checkpoint, flush, or WAL watermarks out of order;
6. recovers differently after crash/reopen than the model expects;
7. lets fuzz targets share one generic scaffold contract;
8. leaves fuzz corpora empty or unregistered;
9. introduces testkit/fuzz dependencies into production lifecycle code;
10. relies on sleeps, threads, wall-clock timing, or documentation-link checks.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Coverage Boundary

In scope:

1. generated lifecycle script decoder and model;
2. generated property harness assertions;
3. recovery, bootstrap, maintenance, flush, checkpoint, retention, quarantine,
   rewrite, close, and health counters;
4. direct fault-window tests;
5. localfs crash/reopen tests for durable persistence windows;
6. fuzz target registration, routing, and seed corpora;
7. source guards for assurance boundaries;
8. porting-log verification records.

Out of scope:

1. L8P final closeout inventory;
2. public L9 lifecycle API tests;
3. engine primitive freeze or IPC shutdown;
4. product recovery wording;
5. background worker thread scheduling;
6. StrataHub behavior;
7. exhaustive process-kill testing on every CI run;
8. distributed object-store lease races.

## Test Locations

Use:

1. `crates/storage-next/src/testkit/lifecycle/script.rs` for generated script
   decoding and model state.
2. `crates/storage-next/src/testkit/lifecycle/fault.rs` for reusable fault
   scripts and counters.
3. `crates/storage-next/src/testkit/lifecycle/crash.rs` for localfs crash/reopen
   helpers.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated property
   tests.
5. `crates/storage-next/tests/lifecycle_maintenance.rs` for non-crash
   integration smoke over generated categories.
6. `crates/storage-next/tests/lifecycle_recovery.rs` for recovery/reopen
   integration not requiring process crash.
7. `crates/storage-next/tests/lifecycle_faults.rs` for direct phase fault tests.
8. `crates/storage-next/tests/crash_recovery.rs` for localfs crash/reopen
   scenarios.
9. `crates/storage-next/tests/lifecycle_source_guard.rs` for assurance
   boundary guards.
10. `crates/storage-next/fuzz/fuzz_targets/` for lifecycle fuzz entry points.
11. `crates/storage-next/fuzz/corpus/lifecycle_*` for seed corpora.
12. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
    shipped files, verification, and deferred items.

Keep Rust test names behavior-focused. Do not put milestone or slice labels in
Rust test names, comments, fixture bytes, or panic messages.

## Generated Script Contract

Required tests:

1. `lifecycle_generated_script_exercises_input_derived_open_recovery_and_close`
2. `lifecycle_generated_script_exercises_input_derived_maintenance_routes`
3. `lifecycle_generated_script_exercises_input_derived_reclaim_routes`
4. `lifecycle_generated_script_rejects_validation_only_script_without_side_effect_claim`
5. `lifecycle_generated_script_model_matches_healthy_recovered_visibility`
6. `lifecycle_generated_script_deletion_set_is_subset_of_model_proof`
7. `lifecycle_generated_script_watermarks_are_monotonic`
8. `lifecycle_generated_script_close_is_idempotent_after_success`
9. `lifecycle_generated_script_cache_mode_never_claims_durable_recovery`
10. `lifecycle_generated_script_lossy_recovery_records_degraded_health`

Assertions:

1. every production route has a matching model route or an explicit typed
   validation error;
2. input-derived counters are asserted separately from canonical smoke counters;
3. generated scripts are bounded and deterministic;
4. invalid generated operations return typed lifecycle errors, not panics.

## Generated Property Tests

Add or extend tests in `tests/lifecycle_properties.rs`.

Required tests:

1. `lifecycle_property_harness_runs_generated_script_contract`
2. `lifecycle_property_harness_requires_input_derived_recovery_routes`
3. `lifecycle_property_harness_requires_input_derived_maintenance_routes`
4. `lifecycle_property_harness_requires_input_derived_retention_routes`
5. `lifecycle_property_harness_requires_input_derived_quarantine_routes`
6. `lifecycle_property_harness_requires_input_derived_close_routes`
7. `lifecycle_property_harness_replays_minimized_failure_case`
8. `lifecycle_property_harness_records_regression_file`

Assertions:

1. proptest cases use bounded byte scripts;
2. every required category counter is non-zero across the case set;
3. each property checks at least one input-derived counter;
4. failure persistence path is stable and under `proptest-regressions/`;
5. wasm-disabled tests are correctly cfg-gated when testkit/localfs is needed.

## Direct Fault-Window Tests

Add `tests/lifecycle_faults.rs` or expand it if present.

Required tests:

1. `fault_capability_mismatch_happens_before_durable_side_effects`
2. `fault_writer_guard_acquired_then_manifest_create_fails_releases_or_reports_guard`
3. `fault_manifest_create_visible_but_publish_uncertain_records_health_debt`
4. `fault_snapshot_published_manifest_update_fails_records_orphan_snapshot`
5. `fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success`
6. `fault_partial_wal_tail_strict_fails_before_repair`
7. `fault_partial_wal_tail_lossy_repairs_and_degrades_health`
8. `fault_corrupt_wal_returns_typed_recovery_error`
9. `fault_replay_failure_transitions_bootstrap_to_failed`
10. `fault_replay_visible_publication_failure_records_durable_not_visible`
11. `fault_flush_table_published_l6_install_fails_reports_orphan_table`
12. `fault_table_rewrite_branch_swap_failure_preserves_reads`
13. `fault_incomplete_retention_proof_blocks_delete_before_backend_access`
14. `fault_quarantine_inventory_publish_failure_blocks_purge`
15. `fault_purge_delete_success_inventory_update_failure_preserves_debt`
16. `fault_close_quiesce_timeout_is_retryable`
17. `fault_close_wal_sync_failure_preserves_source_chain`
18. `fault_close_manifest_sync_failure_preserves_final_fact_debt`
19. `fault_writer_guard_missing_at_release_is_typed`

Assertions:

1. errors assert `code()`, not display strings;
2. lower-layer source chains are preserved when applicable;
3. lifecycle state after failure is asserted;
4. retryability is asserted;
5. health/debt facts name affected object families where known;
6. no unsafe object deletion occurs.

## Crash/Reopen Tests

Add or expand localfs tests in `tests/crash_recovery.rs`.

Required tests:

1. `crash_after_wal_append_before_visibility_replays_record`
2. `crash_after_wal_append_with_unresolved_gate_reconciles_on_reopen`
3. `crash_after_snapshot_publish_before_manifest_update_ignores_orphan_snapshot`
4. `crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail`
5. `crash_after_table_publish_before_branch_install_reports_orphan_table`
6. `crash_after_quarantine_inventory_publish_before_object_move_reports_debt`
7. `crash_after_object_quarantine_before_purge_preserves_quarantine_entry`
8. `crash_after_close_wal_sync_before_guard_release_reopens_consistently`
9. `crash_harness_ignored_cases_have_nonignored_phase_equivalents`
10. `crash_harness_respects_case_limit_and_keep_root_environment`

Rules:

1. localfs durable tests are cfg-gated with `feature = "localfs"` and
   `not(target_arch = "wasm32")`;
2. slow process-level tests may be `#[ignore]`;
3. each ignored crash test has a non-ignored unit/integration test covering the
   same classification;
4. tests use `tests/common` temp-root helpers;
5. tests do not sleep or spawn unbounded background work.

## Fuzz Targets

Required normal tests:

1. `lifecycle_fuzz_targets_are_registered`
2. `lifecycle_fuzz_targets_call_distinct_contracts`
3. `lifecycle_fuzz_corpora_have_non_empty_seed_files`
4. `lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes`
5. `lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes`
6. `lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes`

Required targets:

1. `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs`
2. `crates/storage-next/fuzz/fuzz_targets/lifecycle_maintenance.rs`
3. `crates/storage-next/fuzz/fuzz_targets/lifecycle_retention.rs`

Required contract functions:

1. `check_lifecycle_recovery_fuzz_contract`
2. `check_lifecycle_maintenance_fuzz_contract`
3. `check_lifecycle_retention_fuzz_contract`

Required corpora:

1. `crates/storage-next/fuzz/corpus/lifecycle_recovery/valid_seed`
2. `crates/storage-next/fuzz/corpus/lifecycle_recovery/corrupt_seed`
3. `crates/storage-next/fuzz/corpus/lifecycle_recovery/mixed_seed`
4. `crates/storage-next/fuzz/corpus/lifecycle_maintenance/valid_seed`
5. `crates/storage-next/fuzz/corpus/lifecycle_maintenance/fault_seed`
6. `crates/storage-next/fuzz/corpus/lifecycle_maintenance/close_seed`
7. `crates/storage-next/fuzz/corpus/lifecycle_retention/valid_seed`
8. `crates/storage-next/fuzz/corpus/lifecycle_retention/blocked_seed`
9. `crates/storage-next/fuzz/corpus/lifecycle_retention/purge_seed`

Assertions:

1. each seed file is non-empty;
2. each target calls exactly its own lifecycle fuzz contract;
3. no lifecycle fuzz target calls a generic scaffold-only function;
4. normal inventory tests pass without nightly/libfuzzer.

## Recovery And Bootstrap Generated Coverage

Required tests:

1. `generated_recovery_empty_checkpoint_tail_and_lossy_routes_are_input_driven`
2. `generated_recovery_corrupt_manifest_snapshot_wal_and_table_are_typed`
3. `generated_bootstrap_catches_allocator_timestamp_and_visible_facts`
4. `generated_bootstrap_rejects_timeline_mismatch`
5. `generated_bootstrap_reconciles_unresolved_durable_gate`
6. `generated_recovery_health_matches_fault_family_model`

Assertions:

1. healthy recovery has no degradation faults;
2. degraded recovery has at least one named fault;
3. failed recovery cannot be reported as maintenance-ready;
4. bootstrap failure transitions to failed state;
5. recovered visible version is model-consistent.

## Maintenance, Flush, Checkpoint, And Rewrite Generated Coverage

Required tests:

1. `generated_maintenance_model_matches_enqueue_coalesce_run_cancel_drain`
2. `generated_maintenance_queue_full_and_admission_rejections_are_typed`
3. `generated_flush_preserves_read_parity_and_candidate_watermark`
4. `generated_flush_publication_failure_keeps_branch_state_safe`
5. `generated_checkpoint_preserves_row_visibility_and_tail_replay`
6. `generated_checkpoint_truncation_never_removes_uncovered_wal_records`
7. `generated_table_rewrite_preserves_reads_after_compaction`
8. `generated_materialization_preserves_child_precedence`
9. `generated_storage_pressure_suggestions_are_model_consistent`

Assertions:

1. task ordering is deterministic;
2. ordinary tasks do not run while closing;
3. flush does not advance flush watermark by branch absence;
4. checkpoint ignores opaque sections during recovery;
5. rewrite output identities do not collide with reachable identities.

## Retention, Quarantine, Purge, Repair, And Close Generated Coverage

Required tests:

1. `generated_retention_proof_blocks_unsafe_recovery_health`
2. `generated_retention_never_deletes_reachable_tables_or_live_snapshots`
3. `generated_snapshot_pruning_retains_live_and_newest_snapshots`
4. `generated_quarantine_happens_before_purge`
5. `generated_purge_requires_fresh_inventory_proof`
6. `generated_repair_reports_inconclusive_without_mutating_state`
7. `generated_close_blocks_new_commits_and_ordinary_maintenance`
8. `generated_close_retry_and_double_close_match_model`
9. `generated_close_faults_preserve_health_debt`

Assertions:

1. deletion and purge sets are model subsets;
2. stale proofs never reach backend delete;
3. repair facts do not invent missing rows or objects;
4. close after success is idempotent;
5. close timeout remains retryable.

## Source Guards

Extend `tests/lifecycle_source_guard.rs`.

Required tests:

1. `lifecycle_generated_assurance_stays_in_testkit_tests_or_fuzz`
2. `lifecycle_production_does_not_import_testkit_or_fuzz`
3. `lifecycle_fuzz_targets_use_distinct_contracts`
4. `lifecycle_fuzz_corpora_are_seeded`
5. `lifecycle_crash_tests_are_feature_gated`
6. `ignored_crash_tests_have_nonignored_phase_equivalents`
7. `lifecycle_generated_properties_assert_input_derived_counters`
8. `lifecycle_assurance_tests_avoid_sleeps_and_thread_spawns`

Assertions:

1. source guards remain implementation-assurance checks, not plan-link checks;
2. guards do not forbid legitimate storage terms such as checkpoint, recovery,
   retention, quarantine, or close;
3. guards continue to ban architecture labels in implementation/test source.

## Integration Tests

Required tests:

1. `lifecycle_generated_integration_runs_default_mode_script`
2. `lifecycle_generated_integration_runs_durable_mode_script`
3. `lifecycle_generated_integration_runs_reclaim_close_script`
4. `lifecycle_fault_integration_covers_all_phase_families`
5. `lifecycle_crash_integration_reports_case_counts`

Assertions:

1. integration tests call production/testkit contracts rather than checking
   file presence;
2. category counters are asserted directly;
3. failure output names the missing category.

## Sensitivity Probes To Record

L8O should record probe evidence in the porting log for assurance-specific
mutations. L8P will consolidate the final ledger.

Required L8O probe rows:

| Probe | Mutation | Expected failing test |
|---|---|---|
| Generated prelude masks input | Remove all input-derived operation counting | `lifecycle_property_harness_requires_input_derived_*` |
| Recovery health collapse | Report corrupt WAL as healthy | `fault_corrupt_wal_returns_typed_recovery_error` / generated recovery health test |
| Unsafe retention | Delete a reachable table object | `generated_retention_never_deletes_reachable_tables_or_live_snapshots` |
| Stale purge proof | Treat stale proof as fresh | `generated_purge_requires_fresh_inventory_proof` |
| Checkpoint truncation too aggressive | Truncate WAL above proven watermark | `generated_checkpoint_truncation_never_removes_uncovered_wal_records` |
| Close starts ordinary work | Run ordinary task after close requested | `generated_close_blocks_new_commits_and_ordinary_maintenance` |
| Fuzz target shares scaffold | Route all lifecycle fuzz targets to one contract | `lifecycle_fuzz_targets_call_distinct_contracts` |
| Empty corpora | Remove lifecycle fuzz seeds | `lifecycle_fuzz_corpora_have_non_empty_seed_files` |
| Crash test not gated | Remove localfs/wasm cfg from crash test | `lifecycle_crash_tests_are_feature_gated` |
| Production imports testkit | Import testkit from lifecycle production source | `lifecycle_production_does_not_import_testkit_or_fuzz` |

## Verification

Run:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional when nightly/libfuzzer is available:

```bash
cargo +nightly fuzz run lifecycle_recovery -- -max_total_time=60
cargo +nightly fuzz run lifecycle_maintenance -- -max_total_time=60
cargo +nightly fuzz run lifecycle_retention -- -max_total_time=60
```

If nightly fuzzing is unavailable, the normal fuzz inventory tests must still
prove target registration, distinct contract routing, and non-empty seed
corpora.
