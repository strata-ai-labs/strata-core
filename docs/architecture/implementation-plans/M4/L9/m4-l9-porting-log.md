# M4-L9 Porting Log

Status: draft

Parent plans:

- `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l9-storage-api-boundary-test-plan.md`

## Closeout Status

Not started.

The canonical slice order is:

1. L9A - API Vocabulary And Visibility Boundary
2. L9B - Open, Runtime Handle, And Close
3. L9C - Reads And Timeline Resolution
4. L9D - Commit API
5. L9E - Branch Lifecycle API
6. L9F - Maintenance API
7. L9G - Diagnostics, Health, And Observability
8. L9H - Engine Testkit And Closeout

Slice labels are planning labels only. They should not appear in production Rust
identifiers, fixture bytes, object names, or user-facing strings.

## L9A - API Vocabulary And Visibility Boundary

Status: implemented

### Source Evidence To Read

- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/lib.rs`
- `crates/storage/src/traits.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
- `docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9a-api-vocabulary-visibility-boundary-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lib.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/atoms.rs`
- `crates/storage-next/src/api/branch.rs`
- `crates/storage-next/src/api/commit.rs`
- `crates/storage-next/src/api/diagnostics.rs`
- `crates/storage-next/src/api/error.rs`
- `crates/storage-next/src/api/maintenance.rs`
- `crates/storage-next/src/api/options.rs`
- `crates/storage-next/src/api/outcome.rs`
- `crates/storage-next/src/api/read.rs`
- `crates/storage-next/src/api/result.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/tests/api_source_guard.rs`

### Boundary Decisions

- `storage-next::api` is the only public production storage-next module.
- Lower modules remain private production modules.
- L9A exposes storage-shaped request/outcome/error vocabulary only; no runtime
  open/read/commit/maintenance behavior is wired in this slice.
- The scaffold uses opaque storage atoms and byte values. Product DTOs and
  primitive-aware semantics stay above storage.
- The API source guard scans for lower-layer imports, async/runtime terms, and
  product vocabulary in production API source.
- API request and outcome shells expose accessors for every stored field so
  later slices can map behavior without depending on private struct layout.
- Error codes use the V1 class prefixes where L9A has enough context:
  `unsupported`, `conflict`, `history_unavailable`, and `ambiguous_commit`.
  Lower-layer failures remain `internal.storage_api.lower_layer` until the
  behavior slices have enough context to split IO, corruption, and unavailable
  lower-layer categories without guessing.
- `StorageApiError::RecoveryDegraded` intentionally uses a
  `failed_precondition` code in this scaffold so the boundary does not
  overclaim corruption before later slices carry detailed degradation classes.
- `BranchAction::List` is retained as a scaffold variant even though the
  current `BranchRequest` also stores a branch id. L9E owns the final branch
  request shape and will either split list requests or make the branch id
  optional.

### Tests Added

- `storage_api_error_codes_are_stable`
- `storage_api_error_source_chain_is_preserved`
- `storage_api_error_invalid_argument_has_structured_field`
- `storage_api_error_unsupported_capability_has_structured_field`
- `storage_api_error_history_unavailable_is_distinct_from_not_found`
- `storage_api_error_durable_uncertain_is_distinct_from_lower_layer_failure`
- `storage_api_error_display_does_not_include_payload_bytes`
- `storage_api_error_classes_do_not_overclaim_corruption`
- `storage_key_rejects_empty_when_required`
- `storage_value_accepts_opaque_bytes`
- `read_limit_rejects_zero_when_zero_is_invalid`
- `scan_bound_order_is_validated`
- `branch_generation_zero_policy_is_explicit`
- `maintenance_request_kind_is_constructible`
- `diagnostics_request_kind_is_constructible`
- `open_options_reject_unsupported_modes`
- `commit_batch_rejects_empty_and_duplicate_mutations`
- `request_shells_are_constructible`
- `outcome_summaries_expose_stored_fields`
- `api_is_the_only_public_storage_next_production_module`
- `lower_modules_are_not_public_api`
- `api_public_signatures_do_not_expose_lower_layer_concrete_types`
- `api_source_avoids_engine_product_and_runtime_dependencies`
- `lower_layers_do_not_import_api_upward`
- `api_implementation_avoids_architecture_labels`
- `api_dependency_guard_catches_grouped_lower_layer_imports`
- `upward_api_guard_catches_grouped_api_imports`
- `api_runtime_guard_catches_future_after_lowercasing`
- `api_product_guard_catches_required_product_terms`

### Sensitivity Probes

- Exposing a lower module publicly from `src/lib.rs` is caught by
  `api_is_the_only_public_storage_next_production_module`.
- Importing lower-layer modules from `src/api/**`, including grouped
  `crate::{...}` imports, is caught by
  `api_source_avoids_engine_product_and_runtime_dependencies` and the direct
  helper regression.
- Importing `crate::api` upward from lower layers, including grouped
  `crate::{api::...}` and `super::{api::...}` imports, is caught by
  `lower_layers_do_not_import_api_upward` and
  `upward_api_guard_catches_grouped_api_imports`.
- Introducing async/future runtime vocabulary into production API source is
  caught by `api_source_avoids_engine_product_and_runtime_dependencies`.
- Introducing product vocabulary such as vector or graph terms into production
  API source is caught by
  `api_source_avoids_engine_product_and_runtime_dependencies`.
- Exposing common lower-layer concrete types in public API signatures is caught
  by `api_public_signatures_do_not_expose_lower_layer_concrete_types`.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

## L9B - Open, Runtime Handle, And Close

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/config/mode.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9b-open-runtime-handle-close-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9C - Reads And Timeline Resolution

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/timeline.rs`
- `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9D - Commit API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/commit/batch.rs`
- `crates/storage-next/src/commit/cache.rs`
- `crates/storage-next/src/commit/durable.rs`
- `crates/storage-next/src/commit/outcome.rs`
- `crates/engine/src/database/transaction.rs`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9E - Branch Lifecycle API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/branch/`
- `crates/storage-next/src/lifecycle/`
- `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9F - Maintenance API

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/flush.rs`
- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/src/lifecycle/quarantine.rs`
- `crates/storage-next/src/lifecycle/wal_growth.rs`
- `docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9f-maintenance-api-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9G - Diagnostics, Health, And Observability

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/observability/`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage/src/memory_stats.rs`
- `crates/storage/src/pressure.rs`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.

## L9H - Engine Testkit And Closeout

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/testkit/`
- `crates/storage-next/tests/`
- `docs/architecture/engine-next/testing-and-conformance-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9h-engine-testkit-closeout-test-plan.md`

### Shipped Files

TBD.

### Boundary Decisions

TBD.

### Tests Added

TBD.

### Sensitivity Probes

TBD.

### Verification

TBD.
