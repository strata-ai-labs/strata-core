# L9D Test Plan: Commit API

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9d-commit-api-implementation-plan.md`

## Goal

Prove that L9 commits validate storage batches, route through L7/L8, preserve
conflicts and durable uncertainty, and do not expose public transaction-session
semantics.

## Test Locations

1. `crates/storage-next/src/api/tests/commit.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_faults.rs`
4. `crates/storage-next/tests/api_source_guard.rs`
5. `crates/storage-next/src/testkit/api/commit.rs`

## Required Tests

### Batch Validation

1. `commit_rejects_empty_batch`
2. `commit_rejects_duplicate_keys`
3. `commit_rejects_malformed_key`
4. `commit_rejects_unknown_branch`
5. `commit_rejects_generation_mismatch`
6. `commit_rejects_cross_branch_mutation`
7. `commit_rejects_unsupported_durability_for_cache`
8. `commit_rejects_transaction_id_field_absence_by_type`

### Successful Commits

1. `cache_commit_returns_not_durable_outcome`
2. `standard_commit_returns_standard_outcome`
3. `always_commit_returns_always_outcome`
4. `commit_put_then_read_latest_observes_value`
5. `commit_delete_then_read_latest_observes_tombstone`
6. `commit_ttl_metadata_roundtrips_to_read_facts`
7. `commit_outcome_reports_mutation_counts`
8. `commit_outcome_reports_timestamp_and_version`

### Conflict And CAS

1. `commit_blind_write_succeeds_without_read_set`
2. `commit_expected_version_match_succeeds`
3. `commit_expected_version_mismatch_conflicts`
4. `commit_expected_absent_match_succeeds`
5. `commit_expected_absent_mismatch_conflicts`
6. `commit_conflict_error_has_structured_branch_and_key`

### Failure Mapping

1. `commit_wal_append_failure_maps_to_durable_not_acquired`
2. `commit_durability_uncertain_survives_boundary`
3. `commit_applied_not_visible_survives_boundary`
4. `commit_visibility_publish_failure_preserves_source_chain`
5. `commit_after_close_rejects_closed_runtime`
6. `commit_unresolved_durable_gate_rejects_followup`

### Absence Of Deferred Semantics

1. `commit_api_has_no_public_transaction_session_type`
2. `commit_api_has_no_durable_transaction_id_type`
3. `commit_api_does_not_claim_serializable_isolation`
4. `commit_api_rejects_cross_branch_atomic_request`

## Fault Tests

Fault wrapper should inject:

1. validation failure;
2. conflict;
3. failure before allocation;
4. failure after allocation before mutation;
5. WAL append failure;
6. fsync/forced-durability uncertainty;
7. branch apply failure;
8. visible publication failure.

Each injected phase must have a distinct expected L9 outcome or error.

## Sensitivity Probes

1. Allocate version before validation.
2. Drop conflict error mapping.
3. Convert durable uncertainty to generic lower-layer failure.
4. Allow cross-branch mutations.
5. Add a public transaction-session type.

## Verification

```bash
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults
cargo test -p strata-storage-next --features testkit --locked --test api_properties
```
