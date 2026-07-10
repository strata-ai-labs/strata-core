# L9G Test Plan: Diagnostics, Health, And Observability

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-implementation-plan.md`

## Goal

Prove that L9 diagnostics expose raw storage facts and do not become product
telemetry or user-facing advice.

## Test Locations

1. `crates/storage-next/src/api/tests/diagnostics.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_source_guard.rs`
4. `crates/storage-next/src/testkit/api/diagnostics.rs`

## Required Tests

### Health And Recovery

1. `diagnostics_reports_healthy_recovery`
2. `diagnostics_reports_degraded_recovery`
3. `diagnostics_reports_failed_recovery`
4. `diagnostics_preserves_recovery_fault_class`
5. `diagnostics_distinguishes_unknown_from_unsupported`
6. `diagnostics_after_close_reports_closed_state`

### Resource Facts

1. `diagnostics_reports_memory_budget_limits`
2. `diagnostics_reports_memory_budget_usage`
3. `diagnostics_reports_cache_budget_facts`
4. `diagnostics_reports_lazy_read_counters`
5. `diagnostics_reports_pressure_facts`
6. `diagnostics_cache_mode_marks_durable_facts_unsupported`

### Storage Object Facts

1. `diagnostics_reports_table_manifest_reachability`
2. `diagnostics_reports_table_object_retention_summary`
3. `diagnostics_reports_quarantine_summary`
4. `diagnostics_reports_wal_growth_policy`
5. `diagnostics_reports_checkpoint_watermark`
6. `diagnostics_reports_branch_count_and_generation_summary`

### Product Neutrality

1. `diagnostics_do_not_contain_product_vocabulary`
2. `diagnostics_do_not_contain_primitive_vocabulary`
3. `diagnostics_do_not_contain_user_advice`
4. `diagnostics_do_not_import_engine_telemetry`

### Review Regression Tests

1. `diagnostics_reports_live_degraded_recovery_from_runtime`
2. `diagnostics_after_close_preserves_recovery_summary`
3. `diagnostics_branch_scope_reports_requested_branch_pressure`
4. `diagnostics_unknown_branch_scope_marks_pressure_unknown`
5. `diagnostics_closed_runtime_without_open_reports_unknown_recovery`
6. `diagnostics_failed_io_recovery_is_not_classified_as_corruption`
7. `diagnostics_manifest_read_failure_marks_checkpoint_unknown`
8. `diagnostics_branch_generation_summary_ignores_deleted_branches`
9. `api_conformance_diagnostics_reports_boundary_facts`
10. `api_property_harness_matches_generated_diagnostics_model`

## Sensitivity Probes

1. Convert degraded recovery to healthy.
2. Drop memory budget limit.
3. Emit product recommendation text.
4. Treat unsupported as zero.
5. Report default-branch pressure for a branch-scoped request.
6. Treat an absent branch as pressure-free instead of unknown.
7. Report unopened closed-runtime recovery as healthy.
8. Classify failed I/O recovery as corruption.
9. Fail the whole diagnostics request when checkpoint manifest facts cannot be
   loaded.
10. Include deleted branches in active branch generation min/max facts.

## Open Follow-Ups

1. Durable quarantine diagnostics remain `Unknown`; inventory-backed counts
   require a separate diagnostics read path.
2. The generated diagnostics property harness exercises cache mode only.
   Durable diagnostics are covered by focused `localfs` unit tests.
3. The generated diagnostics harness `object_reports` counter currently guards
   read-activity unknown semantics rather than table-object facts.

## Verification

```bash
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_source_guard
```
