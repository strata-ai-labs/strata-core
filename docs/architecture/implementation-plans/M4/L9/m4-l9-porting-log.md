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
- `docs/architecture/storage/target-crate-shape-and-test-harness.md`
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
- `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
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
- Point and scan reads route through L6 read views after L9 resolves timestamp
  selectors to retained L7 timeline frontiers. This prevents timestamp reads from
  silently clamping after-latest requests to current state.
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
- Version lookups and version-bounded reads require an exact retained L7
  timeline entry. The selected commit timestamp is carried back into the API
  mapper so TTL is evaluated at the selected temporal frontier for version and
  timestamp reads.
- Timestamp read selectors use the matched commit's timestamp as the temporal
  frontier. A request between two commits therefore observes the previous commit
  frontier, including TTL evaluation at that commit timestamp. Direct timeline
  lookup remains diagnostic-friendly and returns the latest retained commit with
  an `AfterLatestRetained` miss flag for after-latest timestamps; point and scan
  timestamp reads reject after-latest requests.
- Timeline reconstruction uses the tombstone-preserving branch scan path so
  storage-owned timeline tombstones are validated as corruption instead of being
  hidden as ordinary value absence.
- Timeline reconstruction currently scans timeline rows from the branch read
  view for each non-latest read. That is correct for the boundary but should be
  replaced by a retained timeline index/cache before high-cardinality timestamp
  reads become performance-sensitive.
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
- `read_at_version_rejects_unrecorded_future_version`
- `read_at_timestamp_resolves_to_commit_version`
- `read_at_timestamp_after_latest_rejects`
- `read_at_timestamp_rejects_insufficient_history`
- `read_at_version_applies_ttl_at_selected_frontier`
- `read_at_timestamp_applies_ttl_at_matched_commit_frontier`
- `scan_at_version_applies_ttl_at_selected_frontier`
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
- `timestamp_lookup_after_latest_returns_matched_with_miss_flag`
- `version_lookup_returns_commit_timestamp`
- `version_lookup_unretained_version_rejects`
- `timeline_bounds_report_retained_range`
- `timeline_corruption_maps_to_diagnostic_error`
- `timeline_tombstone_corruption_maps_to_diagnostic_error`
- `generated_read_contract_matches_model_for_mutations_and_reads`
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
  `timeline_corruption_maps_to_diagnostic_error` and
  `timeline_tombstone_corruption_maps_to_diagnostic_error`.
- Bypassing timeline resolution for timestamp reads is caught by
  `read_at_timestamp_after_latest_rejects`.
- Treating version reads as raw commit-version bounds instead of retained
  timeline frontiers is caught by `read_at_version_rejects_unrecorded_future_version`.
- Ignoring TTL at selected temporal frontiers is caught by
  `read_at_version_applies_ttl_at_selected_frontier`,
  `read_at_timestamp_applies_ttl_at_matched_commit_frontier`, and
  `scan_at_version_applies_ttl_at_selected_frontier`.
- Generated put/delete/read/history/scan scripts are covered by
  `generated_read_contract_matches_model_for_mutations_and_reads`. This remains
  an in-tree unit contract until L9D exposes public commit APIs to integration
  tests.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --features testkit --locked --test api_properties` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

## L9D - Commit API

Status: implemented

### Source Evidence To Read

- `crates/storage-next/src/commit/batch.rs`
- `crates/storage-next/src/commit/cache.rs`
- `crates/storage-next/src/commit/durable.rs`
- `crates/storage-next/src/commit/outcome.rs`
- `crates/engine/src/database/transaction.rs`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9d-commit-api-test-plan.md`

### Shipped Files

- `crates/storage-next/src/api/commit.rs`
- `crates/storage-next/src/api/error.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/outcome.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/commit.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/testkit/api/commit.rs`
- `crates/storage-next/src/testkit/api/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/api_conformance.rs`
- `crates/storage-next/tests/api_faults.rs`
- `crates/storage-next/tests/api_properties.rs`

### Boundary Decisions

- Public commits accept storage-space/key/value mutations plus optional
  compare-and-set conditions. They do not expose transaction sessions,
  transaction ids, serializable-isolation claims, or cross-branch atomic
  request shapes.
- The API commit bridge routes through the lifecycle runtime, not directly to
  lower commit runtimes. Cache commits map to non-durable commits; durable
  standard and durable always runtimes keep their configured durability policy.
- `CommitDurability::RuntimeDefault` means "use the runtime's configured
  policy." Explicit durability requests that contradict the runtime mode fail at
  the storage API boundary as unsupported capability errors.
- Branch generation is a boundary precondition. A missing generation means no
  generation guard; a supplied zero generation is rejected before lower commit
  mapping.
- Commit outcomes expose only API-shaped facts: branch, commit version,
  timestamp, durability summary, mutation counts, timeline-row count, and
  visibility. Lower commit outcome and lifecycle types stay private.
- Conflict errors carry structured storage-space/key diagnostics without
  exposing lower read-set types. The key fingerprint is diagnostic only.
- Durable uncertainty and applied-not-visible outcomes remain distinct from
  generic lower-layer failure through the `ambiguous_commit` error class.
- Public commit timestamps are allocated by the lower commit runtime through the
  runtime-owned timestamp source. Equal commit timestamps remain valid because
  timeline ordering uses commit-version tiebreaking.
- Public commits use an explicit internal timestamp policy after the API selects
  the next monotonic timestamp. This keeps commit stamping and TTL expiry facts
  on the same frontier while leaving caller-supplied timestamps out of the V1
  public surface.
- Zero TTL is rejected at batch construction so a row cannot be visible at
  latest while already expired at its own commit-version read frontier.

### Tests Added

- `commit_rejects_empty_batch`
- `commit_rejects_duplicate_keys`
- `commit_rejects_malformed_key`
- `commit_rejects_zero_ttl`
- `commit_rejects_unknown_branch`
- `commit_rejects_generation_mismatch`
- `commit_rejects_zero_expected_generation`
- `commit_rejects_cross_branch_mutation`
- `commit_rejects_unsupported_durability_for_cache`
- `commit_rejects_always_request_on_standard_runtime`
- `commit_rejects_standard_request_on_always_runtime`
- `commit_rejects_not_durable_request_on_durable_runtime`
- `commit_rejected_request_does_not_allocate_version`
- `commit_rejects_transaction_id_field_absence_by_type`
- `cache_commit_returns_not_durable_outcome`
- `standard_commit_returns_standard_outcome`
- `always_commit_returns_always_outcome`
- `durable_runtime_default_uses_configured_policy`
- `commit_put_then_read_latest_observes_value`
- `commit_delete_then_read_latest_observes_tombstone`
- `commit_ttl_metadata_roundtrips_to_read_facts`
- `commit_outcome_reports_mutation_counts`
- `commit_outcome_reports_timestamp_and_version`
- `commit_rejects_ttl_duration_too_large`
- `commit_rejects_ttl_expiration_overflow`
- `commit_blind_write_succeeds_without_read_set`
- `commit_expected_version_match_succeeds`
- `commit_expected_version_mismatch_conflicts`
- `commit_expected_absent_match_succeeds`
- `commit_expected_absent_mismatch_conflicts`
- `commit_conflict_error_has_structured_branch_and_key`
- `commit_rejects_condition_with_multi_byte_storage_space`
- `commit_wal_append_failure_maps_to_durable_not_acquired`
- `commit_durability_uncertain_survives_boundary`
- `commit_applied_not_visible_survives_boundary`
- `commit_visibility_publish_failure_preserves_source_chain`
- `commit_after_close_rejects_closed_runtime`
- `commit_unresolved_durable_gate_rejects_followup`
- `commit_api_has_no_public_transaction_session_type`
- `commit_api_has_no_durable_transaction_id_type`
- `commit_api_does_not_claim_serializable_isolation`
- `commit_api_rejects_cross_branch_atomic_request`
- `api_conformance_commit_then_read_round_trip`
- `api_property_harness_matches_generated_commit_model`
- `api_fault_validation_failure_maps_to_invalid_argument`
- `api_fault_conflict_maps_to_conflict`
- `api_fault_durability_request_maps_to_unsupported_capability`
- `api_fault_closed_runtime_maps_to_invalid_runtime_state`
- `api_fault_uncertain_commit_maps_to_ambiguous_commit`

### Sensitivity Probes

- Moving allocation before API validation is caught by
  `commit_rejected_request_does_not_allocate_version`, which verifies a rejected
  commit request does not consume the next commit version.
- Dropping structured conflict mapping is caught by
  `commit_conflict_error_has_structured_branch_and_key`.
- Collapsing durable uncertainty into a generic lower-layer failure is caught by
  `commit_durability_uncertain_survives_boundary` and the fault wrapper.
- Allowing cross-branch atomic request shapes is caught by
  `commit_rejects_cross_branch_mutation` and
  `commit_api_rejects_cross_branch_atomic_request`.
- Adding public transaction-session or durable transaction-id vocabulary is
  caught by the absence tests over the public commit API source.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --features testkit --locked --test api_properties` passed.
- `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test api_faults` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

## L9E - Branch Lifecycle API

Status: implemented

### Source Evidence To Read

- `crates/storage-next/src/branch/`
- `crates/storage-next/src/lifecycle/`
- `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

### Shipped Files

- `crates/storage-next/src/api/branch.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/src/api/tests/branch.rs`
- `crates/storage-next/src/testkit/api/branch.rs`
- `crates/storage-next/src/testkit/api/mod.rs`
- `crates/storage-next/tests/api_properties.rs`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9e-branch-lifecycle-api-test-plan.md`

### Boundary Decisions

- Branch lifecycle operations route through the existing lifecycle branch
  catalog surfaces. The public API does not expose branch-local state,
  inherited layers, table refs, materialization handles, or lifecycle runtime
  concrete types.
- `BranchAction::List` keeps using the scaffold `BranchRequest` shape from
  L9A. The request branch id is ignored for list operations; L9G can split
  list into a dedicated request if the final public shape needs that polish.
- The public `ForkCurrent` operation resolves the source branch's current
  retained commit version through the timeline and then uses the retained
  version fork path. This preserves storage-visible active/frozen rows at the
  API boundary without exposing lower unflushed-row restrictions from the
  lifecycle-only `fork_current` helper.
- Fork-at-version accepts retained watermark versions between timeline entries
  when the version is inside the retained `[floor, visible]` interval. This
  matches the lower lifecycle fork contract and avoids rejecting valid
  snapshot watermarks.
- Fork-at-timestamp succeeds only when the timestamp resolves inside retained
  history. Timestamps before retained history, after latest retained history,
  or on an empty branch map to `history_unavailable` API errors.
- All-zero branch ids are rejected at the API boundary for destination and
  source branch identifiers in branch lifecycle requests. Lower durable formats
  may still treat branch ids as opaque atoms.
- Recreating a deleted branch reports the deleted generation as
  `generation_before` and the recreated active generation as `generation_after`.
- Clear/delete under pinned reachability do not reject the branch mutation.
  They surface protected table release counts in `BranchCleanupSummary`, which
  is the lower storage contract for pinned table safety.
- Delete checks branch existence before enforcing the last-active-branch guard
  so unknown branch ids map to `not_found`, not a policy failure.
- Product branch workflows remain absent: merge, cherry-pick, revert, restore,
  publish, and review do not appear in the public branch API source.

### Tests Added

- `branch_create_returns_generation`
- `branch_create_duplicate_rejects`
- `branch_create_invalid_identifier_rejects`
- `branch_list_is_deterministic`
- `branch_describe_reports_generation`
- `branch_describe_unknown_rejects`
- `branch_fork_current_copies_visible_frontier`
- `branch_fork_current_preserves_inherited_visibility`
- `branch_fork_at_retained_version_succeeds`
- `branch_fork_at_retained_watermark_between_commits_succeeds`
- `branch_fork_at_unretained_version_rejects`
- `branch_fork_invalid_source_identifier_rejects`
- `branch_fork_at_timestamp_resolves_timeline`
- `branch_fork_at_unretained_timestamp_rejects`
- `branch_fork_generation_mismatch_rejects`
- `branch_fork_after_close_rejects`
- `branch_recreate_deleted_reports_generation_transition`
- `durable_branch_catalog_round_trips_after_reopen`
- `branch_clear_removes_visible_rows`
- `branch_clear_preserves_branch_identity`
- `branch_clear_generation_mismatch_rejects`
- `branch_clear_with_pinned_view_reports_protected_release`
- `branch_delete_removes_from_list`
- `branch_delete_generation_mismatch_rejects`
- `branch_delete_with_pinned_view_reports_protected_release`
- `branch_delete_unknown_rejects`
- `branch_delete_reports_cleanup_facts`
- `branch_delete_last_required_branch_rejects`
- `branch_api_has_no_merge_method`
- `branch_api_has_no_cherry_pick_method`
- `branch_api_has_no_revert_method`
- `branch_api_has_no_restore_method`
- `branch_api_has_no_publish_review_method`
- `api_property_harness_matches_generated_branch_model`

### Sensitivity Probes

- Ignoring branch generation mismatch is caught by
  `branch_clear_generation_mismatch_rejects`,
  `branch_delete_generation_mismatch_rejects`, and
  `branch_fork_generation_mismatch_rejects`.
- Dropping pinned-reachability cleanup protection is caught by
  `branch_clear_with_pinned_view_reports_protected_release` and
  `branch_delete_with_pinned_view_reports_protected_release`.
- Letting fork-at-history use latest instead of the requested retained version
  is caught by `branch_fork_at_retained_version_succeeds` and
  `branch_fork_at_timestamp_resolves_timeline`.
- Requiring an exact timeline entry for fork-at-version is caught by
  `branch_fork_at_retained_watermark_between_commits_succeeds`.
- Skipping source branch identifier validation is caught by
  `branch_fork_invalid_source_identifier_rejects`.
- Dropping recreate generation facts is caught by
  `branch_recreate_deleted_reports_generation_transition`.
- Losing durable branch catalog state across reopen is caught by
  `durable_branch_catalog_round_trips_after_reopen`.
- The generated branch model contract exercises create, describe, list,
  fork-current, fork-at-version, fork-at-timestamp, clear, delete, recreate,
  invalid-source rejection, and read-after-branch-operation routes.
- Leaking product branch workflow vocabulary is caught by the merge,
  cherry-pick, revert, restore, publish, and review absence tests, now scanning
  the branch vocabulary and runtime implementation surfaces.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api::tests::branch` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --features testkit --locked --test api_properties` passed.
- `cargo clippy -p strata-storage-next --lib --all-features --locked -- -D warnings` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

## L9F - Maintenance API

Status: implemented

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

- `crates/storage-next/src/api/maintenance.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/maintenance.rs`
- `crates/storage-next/src/api/tests/mod.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`

### Boundary Decisions

- Public maintenance results use `MaintenanceSummary`,
  `MaintenanceQueueSummary`, `MaintenanceDrainSummary`, and
  `MaintenanceWalGrowthSummary`. L8 service handles, branch internals, table
  refs, and lifecycle outcomes remain private.
- Cache mode returns deferred/unsupported summaries for durable-only
  maintenance requests instead of admitting tasks that cannot run.
- Direct flush rotates the requested branch and returns row/publication facts,
  but does not claim WAL truncation.
- Direct checkpoint uses durable snapshot-id allocation and exposes watermark,
  snapshot, row, and truncation facts. WAL truncation remains opt-in through
  lower maintenance paths.
- Retention and snapshot pruning run only on durable runtimes. Table-object
  reclaim maps to branch-scoped table-object retention when durable reachability
  proof is available.
- Quarantine and purge remain fail-closed/deferred at this boundary because the
  public request shape does not carry source-object names or current proof
  tokens.
- WAL growth is exposed as policy status plus trigger/enqueue facts; cache mode
  reports `NoDurableAction`.

### Tests Added

- `maintenance_request_snapshot_pruning_is_constructible`
- `api_maintenance_status_reports_empty_queue`
- `api_checkpoint_cache_mode_returns_deferred`
- `api_flush_returns_publication_facts`
- `api_wal_growth_policy_status_reports_no_durable_action_for_cache`
- `api_maintenance_enqueue_and_drain_are_deterministic`
- `api_maintenance_after_close_rejects`
- `api_rewrite_unknown_branch_rejects`

### Sensitivity Probes

- Cache checkpoint/retention/reclaim paths must not enqueue stranded durable
  work.
- Flush summaries must continue to report publication facts without implying
  WAL truncation.
- Queue drain summaries must report user-initiated drained tasks, not only
  close-time drain counters.
- Unknown branch rewrite maintenance must fail before touching lower rewrite
  services.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api::tests::maintenance -- --nocapture` passed.
- `cargo test -p strata-storage-next --locked --lib api -- --nocapture` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard -- --nocapture` passed.
- `cargo test -p strata-storage-next --locked --test api_conformance -- --nocapture` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.

## L9G - Diagnostics, Health, And Observability

Status: implemented

### Source Evidence To Read

- `crates/storage-next/src/observability/`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage/src/memory_stats.rs`
- `crates/storage/src/pressure.rs`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L9/l9g-diagnostics-health-observability-test-plan.md`

### Shipped Files

- `crates/storage-next/src/api/diagnostics.rs`
- `crates/storage-next/src/api/mod.rs`
- `crates/storage-next/src/api/runtime.rs`
- `crates/storage-next/src/api/tests/diagnostics.rs`
- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/testkit/api/diagnostics.rs`
- `crates/storage-next/src/testkit/api/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/api_conformance.rs`
- `crates/storage-next/tests/api_properties.rs`
- `crates/storage-next/tests/api_source_guard.rs`

### Boundary Decisions

- Diagnostics are a synchronous snapshot on `StorageRuntime::diagnostics`.
- Public diagnostics expose only API-owned storage facts: runtime state, mode,
  recovery health, maintenance queue, budget usage, storage pressure, read
  activity availability, table reachability, retention, quarantine, checkpoint
  watermarks, WAL-growth policy, branch catalog counts, and timeline bounds.
- Every optional fact family has an explicit `Known`, `Unknown`, or
  `Unsupported` state. Cache mode marks durable-only fact families unsupported;
  closed runtimes preserve the last opened mode and last recovery summary but
  report live facts as unknown. A runtime constructed directly in the closed
  state reports recovery as unknown rather than healthy.
- Durable recovery diagnostics use live `current_recovery_health`, not the
  bootstrap snapshot. Checkpoint diagnostics load the current database manifest
  and surface snapshot and flush watermarks without mutating runtime state. If
  manifest facts cannot be loaded, only the checkpoint family becomes unknown;
  the diagnostics request remains partial and successful. This intentionally
  does not run re-recovery or add a new recovery fault.
- Diagnostics mode falls back to the lifecycle open plan only if the API open
  summary is absent. The normal open paths always populate the summary; the
  fallback is defensive against future constructor paths.
- Branch-scoped storage pressure is collected from the requested branch state.
  If the requested branch is absent, pressure facts are `Unknown` rather than
  falling back to another branch's pressure.
- Retention diagnostics expose only counters backed by runtime facts. Pending
  release count is known in durable mode; protected and reclaimed object counts
  remain `None` until lower layers track them.
- Failed recovery classification is derived from the failed recovery fault:
  failed I/O and WAL-tail repair faults are reported as `Io`, not corruption.
  Degraded recovery still trusts the lifecycle-supplied degradation class.
- Branch generation min/max facts aggregate active branches only; deleted branch
  tombstones are counted separately and do not skew active generation bounds.
- Read activity counters are intentionally `Unknown` until the lazy read path
  has durable block-hit/miss counters. The API does not synthesize values.
- Durable quarantine diagnostics remain `Unknown` until a dedicated
  inventory-backed diagnostics read path lands. The generated diagnostics
  property harness is cache-mode only; durable diagnostics are covered by
  focused `localfs` unit tests.
- The public type name for durable table state is
  `DiagnosticsTableReachabilityReport`, avoiding leakage of lower-layer table
  manifest concrete type names through the API boundary.
- Diagnostics remain product-neutral. Source guards reject product vocabulary,
  primitive wording, user-advice wording, and engine telemetry imports in
  production API diagnostics source.

### Tests Added

- `diagnostics_reports_healthy_recovery`
- `diagnostics_reports_degraded_recovery`
- `diagnostics_reports_live_degraded_recovery_from_runtime`
- `diagnostics_reports_failed_recovery`
- `diagnostics_preserves_recovery_fault_class`
- `diagnostics_closed_runtime_without_open_reports_unknown_recovery`
- `diagnostics_failed_io_recovery_is_not_classified_as_corruption`
- `diagnostics_distinguishes_unknown_from_unsupported`
- `diagnostics_after_close_reports_closed_state`
- `diagnostics_after_close_preserves_recovery_summary`
- `diagnostics_reports_memory_budget_limits`
- `diagnostics_reports_memory_budget_usage`
- `diagnostics_reports_cache_budget_facts`
- `diagnostics_reports_lazy_read_counters`
- `diagnostics_reports_pressure_facts`
- `diagnostics_branch_scope_reports_requested_branch_pressure`
- `diagnostics_unknown_branch_scope_marks_pressure_unknown`
- `diagnostics_cache_mode_marks_durable_facts_unsupported`
- `diagnostics_reports_table_manifest_reachability`
- `diagnostics_reports_table_object_retention_summary`
- `diagnostics_reports_quarantine_summary`
- `diagnostics_reports_wal_growth_policy`
- `diagnostics_reports_checkpoint_watermark`
- `diagnostics_manifest_read_failure_marks_checkpoint_unknown`
- `diagnostics_reports_branch_count_and_generation_summary`
- `diagnostics_branch_generation_summary_ignores_deleted_branches`
- `diagnostics_do_not_contain_product_vocabulary`
- `diagnostics_do_not_contain_primitive_vocabulary`
- `diagnostics_do_not_contain_user_advice`
- `diagnostics_do_not_import_engine_telemetry`
- `api_conformance_diagnostics_reports_boundary_facts`
- `api_property_harness_matches_generated_diagnostics_model`

### Sensitivity Probes

- Changing cache durable-only facts from `Unsupported` to `Known` is caught by
  `diagnostics_cache_mode_marks_durable_facts_unsupported`.
- Collapsing unknown read activity counters into zero values is caught by
  `diagnostics_reports_lazy_read_counters` and the generated diagnostics
  property harness.
- Dropping live maintenance queue usage from budget diagnostics is caught by
  `diagnostics_reports_memory_budget_usage`.
- Reporting default-branch pressure for a branch-scoped request is caught by
  `diagnostics_branch_scope_reports_requested_branch_pressure`.
- Treating an absent branch as pressure-free instead of unknown is caught by
  `diagnostics_unknown_branch_scope_marks_pressure_unknown`.
- Reporting unopened closed-runtime recovery as healthy is caught by
  `diagnostics_closed_runtime_without_open_reports_unknown_recovery`.
- Classifying failed I/O recovery as corruption is caught by
  `diagnostics_failed_io_recovery_is_not_classified_as_corruption`.
- Failing the whole diagnostics call when checkpoint manifest facts are
  unavailable is caught by
  `diagnostics_manifest_read_failure_marks_checkpoint_unknown`.
- Including deleted branch tombstones in active generation bounds is caught by
  `diagnostics_branch_generation_summary_ignores_deleted_branches`.
- Leaking lower-layer concrete table manifest type names through public API
  signatures is caught by `api_public_signatures_do_not_expose_lower_layer_concrete_types`.
- Adding product, primitive, user-advice, or engine telemetry vocabulary to
  diagnostics source is caught by the dedicated diagnostics source guards.

### Verification

- `cargo fmt --package strata-storage-next --check` passed.
- `cargo test -p strata-storage-next --locked --lib api` passed.
- `cargo test -p strata-storage-next --locked --lib api --features localfs`
  passed.
- `cargo test -p strata-storage-next --locked --test api_conformance` passed.
- `cargo test -p strata-storage-next --locked --test api_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test api_properties --features testkit`
  passed.
- `cargo test -p strata-storage-next --locked --test api_properties --features testkit,localfs`
  passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
  passed.

## L9H - Engine Testkit And Closeout

Status: planned

### Source Evidence To Read

- `crates/storage-next/src/testkit/`
- `crates/storage-next/tests/`
- `docs/architecture/engine/testing-and-conformance-plan.md`
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
