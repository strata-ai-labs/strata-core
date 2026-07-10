# L9H Test Plan: Engine Testkit And Closeout

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-implementation-plan.md`

## Goal

Prove that L9's fake, faulting, generated, source-guard, and closeout coverage
are strong enough for engine-next to depend on the storage boundary.

## Test Locations

1. `crates/storage-next/src/testkit/api/`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_properties.rs`
4. `crates/storage-next/tests/api_faults.rs`
5. `crates/storage-next/tests/api_source_guard.rs`
6. `crates/storage-next/tests/api_closeout.rs`
7. `crates/storage-next/fuzz/fuzz_targets/api_commit_script.rs`
8. `crates/storage-next/fuzz/fuzz_targets/api_fault_script.rs`
9. engine-next boundary tests once the crate exists.

## Required Tests

### Fake Persistence

1. `fake_persistence_passes_basic_conformance`
2. `fake_persistence_supports_deterministic_versions`
3. `fake_persistence_supports_deterministic_timestamps`
4. `fake_persistence_models_retained_history_miss`
5. `fake_persistence_models_conflict`
6. `fake_persistence_models_branch_delete`
7. `fake_persistence_models_recovery_health`
8. `fake_persistence_does_not_expose_private_helpers`

### Faulting Wrapper

1. `faulting_wrapper_injects_open_failure`
2. `faulting_wrapper_injects_read_failure`
3. `faulting_wrapper_injects_validation_failure`
4. `faulting_wrapper_injects_conflict`
5. `faulting_wrapper_injects_durable_uncertainty`
6. `faulting_wrapper_injects_applied_not_visible`
7. `faulting_wrapper_injects_recovery_degradation`
8. `faulting_wrapper_injects_maintenance_failure`
9. `faulting_wrapper_injects_close_failure`
10. `faulting_wrapper_errors_match_production_code_shape`

### Generated Conformance

1. `generated_api_scripts_cover_cache_open`
2. `generated_api_scripts_cover_durable_open`
3. `generated_api_scripts_cover_reads`
4. `generated_api_scripts_cover_commits`
5. `generated_api_scripts_cover_branch_lifecycle`
6. `generated_api_scripts_cover_maintenance`
7. `generated_api_scripts_cover_close`
8. `generated_api_scripts_are_input_derived`
9. `generated_api_scripts_do_not_rely_on_canonical_smoke_only`

### Closeout

1. `api_closeout_public_surface_snapshot_exists`
2. `api_closeout_source_guards_cover_required_boundaries`
3. `api_closeout_conformance_tests_cover_all_operation_families`
4. `api_closeout_fault_tests_cover_boundary_failure_families`
5. `api_closeout_generated_tests_have_input_derived_counters`
6. `api_closeout_porting_log_records_command_results`
7. `api_closeout_sensitivity_ledger_has_required_probes`
8. `api_closeout_deferred_work_matches_parent_plan`

## Engine Boundary Tests

When engine-next exists, add tests proving:

1. engine imports only L9 production API;
2. engine semantic tests can run on fake L9 persistence;
3. engine boundary/failure tests can run on faulting L9 wrapper;
4. engine code does not import storage internals.

## Sensitivity Probes

1. Route fake persistence through a different method than production.
2. Make generated counters nonzero from canonical setup only.
3. Remove a fault family.
4. Import private storage module from engine test.
5. Delete public API snapshot update.

## Verification

```bash
cargo test -p strata-storage-next --features testkit --locked --test api_properties
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo test -p strata-storage-next --locked --test api_closeout
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```
