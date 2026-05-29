# L9F Test Plan: Maintenance API

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-implementation-plan.md`

## Goal

Prove that L9 maintenance APIs expose L8 maintenance safely and fail closed
without current proof.

## Test Locations

1. `crates/storage-next/src/api/tests/maintenance.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_faults.rs`
4. `crates/storage-next/tests/api_properties.rs`

## Required Tests

### Checkpoint And Flush

1. `api_checkpoint_returns_watermark_facts`
2. `api_checkpoint_after_close_rejects`
3. `api_checkpoint_cache_mode_returns_unsupported_or_deferred`
4. `api_flush_returns_publication_facts`
5. `api_flush_does_not_claim_wal_truncation_without_proof`
6. `api_flush_failure_preserves_orphan_facts`

### Rewrite Maintenance

1. `api_compaction_returns_rewrite_facts`
2. `api_compaction_reports_checkpoint_debt`
3. `api_materialization_returns_rewrite_facts`
4. `api_materialization_uses_stable_intent`
5. `api_rewrite_unknown_branch_rejects`
6. `api_rewrite_cache_mode_does_not_call_durable_services`

### Retention And Quarantine

1. `api_retention_without_current_proof_rejects`
2. `api_retention_table_objects_deferred_when_unsupported`
3. `api_snapshot_pruning_preserves_required_snapshot`
4. `api_quarantine_via_queue_defers_without_explicit_request`
5. `api_purge_requires_fresh_proof`
6. `api_repair_reports_reconciliation_facts`
7. `api_reclaim_degraded_health_blocks_when_required`

### WAL Growth And Queue

1. `api_wal_growth_policy_status_reports_not_needed`
2. `api_wal_growth_policy_status_reports_checkpoint_due`
3. `api_wal_growth_trigger_runs_supported_path`
4. `api_maintenance_queue_status_reports_pending_only`
5. `api_maintenance_drain_is_deterministic`
6. `api_maintenance_after_close_rejects`

## Fault Tests

Inject:

1. snapshot publish failure;
2. manifest publish failure;
3. table publish failure;
4. compaction lower-layer failure;
5. materialization lower-layer failure;
6. retention proof failure;
7. quarantine inventory mismatch;
8. purge publish uncertainty;
9. repair failure.

Each failure must preserve source-chain fields and retryability.

## Sensitivity Probes

1. Report retention no-op as successful table-object deletion.
2. Collapse checkpoint truncation failure into failed checkpoint.
3. Drop orphan object facts after flush failure.
4. Accept cache durable-only maintenance as pending forever.

## Verification

```bash
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
```
