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
- The API source guard scans for async/runtime terms and product vocabulary in
  production API source. L9B narrows the earlier lower-layer-import guard:
  private API implementation may call lower storage layers, but public
  signatures must not expose lower-layer concrete types.
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
- Importing engine/product crates from `src/api/**` is caught by
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

Status: implemented

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

- `crates/storage-next/src/api/backend.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/options.rs`
- `crates/storage-next/src/api/outcome.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/tests/api_conformance.rs`
- `crates/storage-next/tests/api_source_guard.rs`

### Boundary Decisions

- `StorageRuntime` owns one lower runtime variant behind an opaque public
  handle. Lower runtime concrete types stay private.
- Cache open can use the default in-memory backend through `StorageRuntime::open`.
- Durable-local open requires an explicit `StorageBackend` handle because the
  durable lifecycle runtime borrows its backend services. The public backend
  handle remains storage-shaped and does not expose lifecycle/service objects.
- `StorageBackend::local_fs` is the V1 durable-local constructor when the
  `localfs` feature is enabled. Memory backend use with durable-local mode is
  rejected through a storage API unsupported-capability error.
- Open summaries expose mode, disposition, recovery health, recovered visible
  version, maintenance readiness, durable-fact presence, and backend capability
  use. Cache summaries do not report durable recovery facts.
- Close summaries expose idempotence and final close effects without leaking
  lifecycle close outcome types.
- A second close returns an idempotent closed summary. Operations after close
  use `require_open` until read/commit/maintenance APIs land.
- Source guards now permit private `api` implementation imports from lower
  storage modules while continuing to block lower concrete types from public
  signatures and upward imports from lower modules into `api`.
- The public open options preserve budget policy and WAL-growth policy knobs
  and reject zero WAL-growth thresholds before lifecycle construction. Durable
  local storage still takes an explicit opaque backend handle rather than a raw
  path field.
- `StorageOpenSummary` constructors are crate-private so callers cannot
  fabricate cache summaries with durable/recovery facts that the boundary mapper
  would never emit.
- `StorageRuntime::open` validates options before choosing the cache/durable
  path, keeping direct cache opens and backend-backed opens on the same
  user-facing error vocabulary.

### Tests Added

- `open_options_default_is_cache_or_explicitly_invalid`
- `open_options_rejects_zero_limits`
- `open_rejects_zero_limits_before_lifecycle_mapping`
- `open_options_rejects_cache_with_durable_path_requirement`
- `open_options_rejects_durable_without_local_backend`
- `open_options_rejects_durable_without_local_path`
- `open_options_rejects_object_durable_candidate`
- `open_options_rejects_distributed_writer_mode`
- `open_options_reject_cache_lossy_recovery`
- `open_options_preserves_recovery_strictness`
- `open_options_preserves_budget_policy`
- `open_cache_returns_open_runtime`
- `open_cache_reports_cache_mode`
- `open_cache_reports_no_durable_recovery_facts`
- `open_cache_does_not_construct_wal_or_manifest_services`
- `open_cache_returns_open_runtime_and_cache_summary`
- `open_cache_close_is_idempotent`
- `open_cache_operation_after_close_rejects`
- `open_durable_modes_return_open_runtime`
- `open_durable_standard_returns_open_runtime`
- `open_durable_always_returns_open_runtime`
- `create_durable_local_returns_created_disposition`
- `open_existing_durable_local_returns_opened_disposition`
- `durable_open_reports_backend_capabilities_used`
- `durable_open_reports_recovery_health`
- `durable_open_degraded_health_survives_boundary_mapping`
- `durable_open_failure_returns_storage_api_error`
- `durable_open_with_memory_backend_returns_storage_api_error`
- `close_open_cache_returns_final_facts`
- `close_open_durable_returns_final_facts`
- `close_twice_returns_idempotent_outcome`
- `close_failure_preserves_source_chain`
- `close_then_read_rejects_closed_runtime`
- `close_then_commit_rejects_closed_runtime`
- `close_then_maintenance_rejects_closed_runtime`
- `api_conformance_cache_open_close_round_trip`
- `api_conformance_durable_open_close_round_trip`
- `api_conformance_unsupported_modes_fail_before_runtime_construction`
- `api_conformance_closed_runtime_rejects_operations`
- `api_dependency_guard_catches_engine_product_imports`
- `public_signature_guard_catches_multiline_lower_types`
- `api_open_signatures_do_not_expose_lifecycle_types`
- `api_close_signatures_do_not_expose_lifecycle_types`
- `api_open_does_not_expose_backend_services`
- `api_open_unsupported_modes_do_not_claim_production_support`

### Sensitivity Probes

- Removing unsupported object-durable/distributed validation is caught by
  `open_options_reject_unsupported_modes` and
  `api_conformance_unsupported_modes_fail_before_runtime_construction`.
- Allowing cache mode to request lossy durable recovery fallback is caught by
  `open_options_reject_cache_lossy_recovery`.
- Allowing zero WAL-growth thresholds is caught by
  `open_options_rejects_zero_limits`.
- Making durable-local open construct without an explicit backend is caught by
  `open_options_rejects_durable_without_local_backend`.
- Returning errors on idempotent second close is caught by
  `open_cache_close_is_idempotent`.
- The close-failure source-chain path is exercised through a durable runtime
  close after intentionally releasing its writer guard in test-only code, and
  is caught by `close_failure_preserves_source_chain`.
- Reporting durable facts from cache open is caught by
  `open_cache_returns_open_runtime_and_cache_summary`.
- Exposing lifecycle/service concrete types in public API signatures is caught
  by `api_public_signatures_do_not_expose_lower_layer_concrete_types`,
  including multiline signatures via
  `public_signature_guard_catches_multiline_lower_types`.
- The open-outcome constructor is crate-private; callers receive outcomes only
  through storage open entry points.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.
- `cargo test -p strata-storage-next --no-default-features --locked --lib api --no-run` passed with one pre-existing testkit dead-code warning.
- `cargo test -p strata-storage-next --no-default-features --locked --test api_conformance --no-run` passed.

## L9C - Reads And Timeline Resolution

Status: implemented

### Source Evidence To Read

- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/timeline.rs`
- `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9c-reads-timeline-resolution-test-plan.md`

### Shipped Files

- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/read.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/src/api/tests/read.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/tests/api_properties.rs`

### Boundary Decisions

- Read APIs are public byte-oriented storage APIs. They expose key, value,
  commit version, commit timestamp, optional expiry, and tombstone facts without
  exposing `StorageRow`, branch row sources, table identities, or timeline row
  internals.
- Point and scan reads route through L6 read views. L9 adds only mapping,
  storage-space validation, public outcomes, and retained-history/timeline
  error translation.
- API storage spaces map to one engine-owned storage-space byte for this slice.
  Multi-byte product namespaces stay above storage or in later boundary work.
- The API physical-key space is a storage-boundary implementation detail and is
  not exposed to callers.
- Public point reads surface visible tombstone facts. L6's ordinary value-read
  helpers still filter tombstones, so this slice adds a tombstone-preserving
  scan selector for API boundary use.
- Timestamp lookups rebuild the timeline view from L7 timeline rows and use the
  L7 rule: newest commit at or before the requested timestamp, with greatest
  commit version as the equal-timestamp tie-breaker.
- Version lookups and version-bounded reads reject requests below the retained
  timeline floor when a retained floor is known.
- Cache and durable runtimes expose branch-specific read-view accessors. Unknown
  branches map to the public branch-not-found error instead of a lower-layer
  internal failure.
- Test-only API helpers seed cache/durable runtimes through the real commit,
  rotation, flush, fork, and branch mutation paths. They remain crate-private
  and are not public API behavior.

### Tests Added

- `read_latest_returns_newest_visible_value`
- `read_latest_returns_none_for_absent_key`
- `read_latest_returns_tombstone_fact_for_visible_delete`
- `read_at_version_returns_exact_retained_value`
- `read_at_version_uses_latest_at_or_before_version`
- `read_at_version_rejects_unretained_history`
- `read_at_timestamp_resolves_to_commit_version`
- `read_at_timestamp_rejects_insufficient_history`
- `read_after_close_rejects_closed_runtime`
- `read_unknown_branch_rejects`
- `history_returns_newest_first`
- `history_limit_is_enforced`
- `history_before_version_excludes_newer_versions`
- `history_preserves_tombstone_entries`
- `history_pruned_versions_return_retention_error`
- `history_empty_key_returns_empty_history`
- `prefix_scan_returns_sorted_keys`
- `prefix_scan_applies_version_bound`
- `prefix_scan_applies_timestamp_bound`
- `prefix_scan_limit_is_stable`
- `range_scan_respects_start_and_end`
- `range_scan_empty_range_returns_empty`
- `range_scan_tombstone_visibility_matches_point_read`
- `scan_inherited_rows_match_point_reads`
- `timestamp_lookup_returns_newest_commit_at_or_before_timestamp`
- `timestamp_lookup_equal_timestamps_uses_greatest_version`
- `timestamp_lookup_before_retained_range_rejects`
- `version_lookup_returns_commit_timestamp`
- `version_lookup_unretained_version_rejects`
- `timeline_bounds_report_retained_range`
- `timeline_corruption_maps_to_diagnostic_error`
- `api_property_harness_checks_empty_runtime_reads_are_deterministic`
- `api_property_harness_rejects_closed_runtime_reads`

### Sensitivity Probes

- Converting timestamp-history misses to not-found is caught by
  `read_at_timestamp_rejects_insufficient_history` and
  `timestamp_lookup_before_retained_range_rejects`.
- Reversing scan ordering is caught by `prefix_scan_returns_sorted_keys`,
  `prefix_scan_limit_is_stable`, and `range_scan_respects_start_and_end`.
- Dropping tombstone facts from point or scan results is caught by
  `read_latest_returns_tombstone_fact_for_visible_delete`,
  `history_preserves_tombstone_entries`, and
  `range_scan_tombstone_visibility_matches_point_read`.
- Using the smallest version for duplicate timestamps is caught by
  `timestamp_lookup_equal_timestamps_uses_greatest_version`.
- Dropping inherited rows from scans is caught by
  `scan_inherited_rows_match_point_reads`.
- Collapsing timeline corruption into not-found/history miss is caught by
  `timeline_corruption_maps_to_diagnostic_error`.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --features testkit --locked --test api_properties` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

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
