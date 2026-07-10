# L9B Test Plan: Open, Runtime Handle, And Close

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-implementation-plan.md`

## Goal

Prove that L9 opens and closes cache and durable local storage through L8,
rejects unsupported modes, and hides lifecycle internals behind a stable storage
runtime handle.

## Test Locations

1. `crates/storage-next/src/api/tests/mod.rs`
2. `crates/storage-next/tests/api_conformance.rs`
3. `crates/storage-next/tests/api_source_guard.rs`
4. `crates/storage-next/src/testkit/api/open.rs` if shared fixtures are needed.

## Required Tests

### Open Options

1. `open_options_default_is_cache_or_explicitly_invalid`
2. `open_options_rejects_zero_limits`
3. `open_options_rejects_cache_with_durable_path_requirement`
4. `open_options_rejects_durable_without_local_path`
5. `open_options_rejects_object_durable_candidate`
6. `open_options_rejects_distributed_writer_mode`
7. `open_options_preserves_recovery_strictness`
8. `open_options_preserves_budget_policy`

Notes:

- The V1 API boundary does not expose a durable path field directly. Durable
  local open requires an explicit opaque `StorageBackend`; therefore
  `open_options_rejects_durable_without_local_path` is implemented as the
  explicit-backend-required check.
- Budget policy coverage is currently option-preservation and lifecycle-plan
  wiring coverage. Pool-level budget behavior remains owned by lifecycle budget
  tests.

### Cache Open

1. `open_cache_returns_open_runtime`
2. `open_cache_reports_cache_mode`
3. `open_cache_reports_no_durable_recovery_facts`
4. `open_cache_does_not_construct_wal_or_manifest_services`
5. `open_cache_close_is_idempotent`
6. `open_cache_operation_after_close_rejects`

### Durable Local Open

1. `open_durable_standard_returns_open_runtime`
2. `open_durable_always_returns_open_runtime`
3. `create_durable_local_returns_created_disposition`
4. `open_existing_durable_local_returns_opened_disposition`
5. `durable_open_reports_backend_capabilities_used`
6. `durable_open_reports_recovery_health`
7. `durable_open_degraded_health_survives_boundary_mapping`
8. `durable_open_failure_returns_storage_api_error`

### Close

1. `close_open_cache_returns_final_facts`
2. `close_open_durable_returns_final_facts`
3. `close_twice_returns_idempotent_outcome`
4. `close_failure_preserves_source_chain`
5. `close_then_read_rejects_closed_runtime`
6. `close_then_commit_rejects_closed_runtime`
7. `close_then_maintenance_rejects_closed_runtime`

## Source Guard Tests

1. `api_open_signatures_do_not_expose_lifecycle_types`
2. `api_close_signatures_do_not_expose_lifecycle_types`
3. `api_open_does_not_expose_backend_services`
4. `api_open_unsupported_modes_do_not_claim_production_support`

## Sensitivity Probes

1. Remove unsupported object-durable rejection.
2. Expose `LifecycleOpenOutcome` directly from L9.
3. Make second close return an error.
4. Report durable facts from cache open.

## Verification

```bash
cargo fmt --package strata-storage-next --check
git diff --check
cargo test -p strata-storage-next --locked --lib api
cargo test -p strata-storage-next --locked --test api_conformance
cargo test -p strata-storage-next --locked --test api_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --no-default-features --locked --lib api --no-run
cargo test -p strata-storage-next --no-default-features --locked --test api_conformance --no-run
```
