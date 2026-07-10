# Engine Branch Semantics Parity Test Plan

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/engine-branch-semantics-parity-implementation-plan.md`

## Goal

Prove that rebuilt engine branch semantics preserve the load-bearing product
invariants recovered from the old engine while using the current storage
architecture:

1. Product branch names resolve to stable, old-compatible branch identities.
2. Root create, fork-current, historical fork, delete, and recreate have clear
   and distinct behavior.
3. Branch catalog, lifecycle, parent/fork metadata, generation state, and
   default-branch metadata survive durable reopen.
4. KV reads and writes resolve branch state through the engine catalog and are
   fenced by the live branch generation.
5. Half-finished branch operations fail closed rather than being exposed as
   healthy state.
6. Executor branch commands remain a serialized delegating layer over engine
   branch APIs.
7. Deferred workflows remain absent from public APIs until their own plans land.

This plan tests invariants and observable behavior. It must not force the
rebuilt engine to copy old internal branch mechanisms.

## Test Targets

Expected engine targets:

```text
crates/engine-next/tests/branch_identity.rs
crates/engine-next/tests/branch_semantics.rs
crates/engine-next/tests/branch_recovery.rs
crates/engine-next/tests/branch_faults.rs
crates/engine-next/tests/branch_and_kv.rs
crates/engine-next/tests/control_plane.rs
crates/engine-next/tests/persistence_adapter.rs
crates/engine-next/tests/dependency_guards.rs
crates/engine-next/tests/common/mod.rs
```

Expected executor targets after branch commands land:

```text
crates/executor-next/tests/branch_command_contract.rs
crates/executor-next/tests/branch_behavior.rs
crates/executor-next/tests/error_and_guards.rs
```

Unit tests may live next to pure encode/decode or validation code. End-to-end
cache/durable behavior should live in integration tests.

## Test Data Policy

1. Use deterministic names: `default`, `main`, `feature`, `feature/abc`,
   `scratch`, `parent`, `child`, `recreated`, and `victim`.
2. Use byte KV keys and values that make branch visibility obvious:
   `shared=base`, `shared=child`, `after-fork=parent-later`,
   `deleted=old`, `recreated=new`.
3. Use temp directories for durable tests and never write under user home paths.
4. Use cache and durable-local variants for every behavior that can drift by
   storage mode.
5. Use explicit helper assertions; avoid hiding important assertions inside
   setup helpers.
6. Test names, code comments, error codes, metrics, and module names must use
   permanent domain vocabulary, not planning labels.

## Required Harness Helpers

Add or extend helpers under `tests/common`:

1. `open_cache_database()`
2. `open_cache_database_with_default_branch(name)`
3. `open_durable_database(path)`
4. `open_durable_database_with_default_branch(path, name)`
5. `branch(name)`
6. `space(name)`
7. `key(bytes)`
8. `value(bytes)`
9. `put_kv(db, branch, space, key, value)`
10. `get_kv(db, branch, space, key)`
11. `assert_branch_exists(db, name)`
12. `assert_branch_missing(db, name)`
13. `assert_branch_summary(db, expected)`
14. `assert_branch_value(db, branch, space, key, value)`
15. `assert_branch_value_missing(db, branch, space, key)`
16. `assert_error(error, class, code)`
17. `assert_no_storage_type_in_engine_error(error)`
18. `collect_rust_sources(root)`

Fault-injection helpers should live behind a test-support feature or test-only
module and must not leak into public APIs.

## Traceability Matrix

| Invariant | Primary tests |
| --- | --- |
| Old-compatible branch id derivation | `branch_id_default_is_nil_uuid`, `branch_id_common_names_match_locked_anchors` |
| One live lifecycle per name | `branch_create_duplicate_rejects`, `branch_recreate_increments_generation` |
| Root create starts empty | `branch_create_root_starts_empty` |
| Fork-current inherits source frontier | `branch_fork_current_inherits_source_visible_rows` |
| Historical fork uses requested frontier | `branch_fork_at_retained_version_uses_requested_frontier` |
| Delete removes branch visibility | `branch_delete_removes_from_list`, `branch_get_deleted_returns_not_found` |
| Deleted data does not resurrect | `branch_delete_blocks_late_stale_write`, `branch_recreate_does_not_inherit_deleted_data` |
| Generation guards fence KV writes | `kv_stale_generation_commit_rejects` |
| Default branch is protected | `branch_delete_default_rejects` |
| Durable reopen preserves catalog | `durable_reopen_preserves_branch_catalog_and_parent_facts` |
| Pending operation fails closed | `pending_branch_operation_fails_closed_on_reopen` |
| Executor delegates to engine | `executor_branch_commands_delegate_to_engine` |
| Deferred workflows absent | source guards for merge, publish, review, cherry-pick, revert, restore |

## Unit Test Matrix

### Branch Name Validation

Target: `crates/engine-next/tests/branch_identity.rs` or source-local unit tests.

Required tests:

1. `branch_name_accepts_common_product_names`
2. `branch_name_accepts_slashes`
3. `branch_name_accepts_uuid_string`
4. `branch_name_rejects_empty`
5. `branch_name_rejects_whitespace_only`
6. `branch_name_rejects_reserved_prefix`
7. `branch_name_rejects_system_branch`
8. `branch_name_rejects_control_bytes`
9. `branch_name_rejects_over_255_bytes`
10. `branch_name_rejects_default_sentinel_alias`
11. `branch_name_default_is_case_sensitive`

Assertions:

1. Invalid names return engine invalid-input errors.
2. Reserved names are rejected at the product boundary even if internal helpers
   can derive ids for compatibility fixtures.

### Branch Identity

Target: `crates/engine-next/tests/branch_identity.rs`.

Required tests:

1. `branch_id_default_is_nil_uuid`
2. `branch_id_main_matches_locked_anchor`
3. `branch_id_system_anchor_matches_old_internal_derivation`
4. `branch_id_uuid_string_passes_through`
5. `branch_id_uuid_string_case_and_hyphenation_match`
6. `branch_id_feature_name_is_stable`
7. `branch_id_distinct_names_do_not_collide`
8. `branch_catalog_source_does_not_use_sha256_derivation`

Assertions:

1. The branch id byte anchors match the old engine characterization tests.
2. The default branch is stored under the nil branch id.
3. Branch id derivation is deterministic.

### Control-Plane Record Encoding

Target: `crates/engine-next/tests/control_plane.rs`.

Required tests:

1. `branch_record_round_trips_root_branch`
2. `branch_record_round_trips_forked_branch`
3. `branch_record_round_trips_deleted_branch`
4. `branch_record_rejects_unknown_payload_version`
5. `branch_record_rejects_truncated_parent_facts`
6. `branch_record_rejects_invalid_lifecycle_tag`
7. `branch_index_rejects_duplicate_names`
8. `branch_index_remains_sorted`
9. `branch_generation_state_round_trips`
10. `branch_generation_state_rejects_truncated_payload`

Assertions:

1. Payload versioning is explicit.
2. Unsupported future payload versions fail closed.
3. Parent and fork metadata cannot be silently dropped.

### Persistence Adapter Branch Operations

Target: `crates/engine-next/tests/persistence_adapter.rs`.

Required tests:

1. `persistence_branch_create_maps_success`
2. `persistence_branch_describe_maps_summary`
3. `persistence_branch_list_maps_summaries`
4. `persistence_branch_fork_current_maps_fork_facts`
5. `persistence_branch_fork_at_version_maps_fork_facts`
6. `persistence_branch_fork_at_timestamp_maps_fork_facts`
7. `persistence_branch_delete_maps_cleanup_facts`
8. `persistence_branch_generation_mismatch_maps_conflict`
9. `persistence_branch_unknown_maps_not_found`
10. `persistence_branch_history_unavailable_maps_not_found_or_history_error`
11. `persistence_branch_storage_unavailable_maps_retryable_unavailable`
12. `persistence_branch_corruption_maps_non_retryable_corruption`

Assertions:

1. Public engine errors do not expose storage type names.
2. Storage branch request and outcome types do not leave `persistence`.

## Integration Test Matrix

### Cache Bootstrap And Default Branch

Target: `crates/engine-next/tests/branch_semantics.rs`.

Workflow:

1. Open cache database.
2. Assert default branch exists.
3. Assert default branch id is nil UUID.
4. Assert default branch generation is the initial generation.
5. Assert branch list contains only live product branches.
6. Close database.

Required tests:

1. `cache_open_bootstraps_default_branch_with_nil_id`
2. `cache_open_with_custom_default_branch_creates_that_branch`
3. `cache_branch_list_excludes_system_branch`
4. `cache_default_branch_delete_rejects`

### Durable Bootstrap And Reopen

Target: `crates/engine-next/tests/branch_recovery.rs`.

Workflow:

1. Open durable-local database in temp dir.
2. Create root branch `scratch`.
3. Put data on default.
4. Fork `feature` from default.
5. Put child-specific data on `feature`.
6. Close.
7. Reopen.
8. Assert default, scratch, and feature summaries survive.
9. Assert feature parent/fork facts survive.
10. Assert KV visibility survives.

Required tests:

1. `durable_reopen_preserves_branch_catalog_and_parent_facts`
2. `durable_reopen_preserves_default_branch_marker`
3. `durable_reopen_preserves_generation_after_recreate`
4. `durable_reopen_rejects_corrupt_branch_catalog`
5. `durable_reopen_rejects_pending_branch_operation`

### Root Create

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_create_root_starts_empty`
2. `branch_create_root_has_no_parent`
3. `branch_create_root_generation_is_reported`
4. `branch_create_duplicate_rejects`
5. `branch_create_invalid_name_rejects`
6. `branch_create_after_close_rejects`

Key assertions:

1. Root-created branch does not inherit default branch data.
2. Root-created branch can receive independent KV writes.
3. Duplicate creation returns a stable conflict error.

### Fork Current

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_fork_current_inherits_source_visible_rows`
2. `branch_fork_current_records_parent_name_id_generation`
3. `branch_fork_current_records_fork_version`
4. `branch_fork_current_isolates_child_writes`
5. `branch_fork_current_isolates_source_writes_after_fork`
6. `branch_fork_current_missing_source_rejects`
7. `branch_fork_current_duplicate_destination_rejects`
8. `branch_fork_current_deleted_source_rejects`
9. `branch_fork_current_after_close_rejects`

Key assertions:

1. Child sees source rows present at fork time.
2. Child does not see later source writes unless storage semantics explicitly
   define otherwise.
3. Source does not see child writes.

### Historical Fork

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_fork_at_retained_version_uses_requested_frontier`
2. `branch_fork_at_version_between_commits_uses_retained_watermark`
3. `branch_fork_at_unretained_version_rejects`
4. `branch_fork_at_timestamp_resolves_timeline`
5. `branch_fork_at_unretained_timestamp_rejects`
6. `branch_fork_at_history_records_source_and_fork_metadata`
7. `branch_fork_at_history_missing_source_rejects`
8. `branch_fork_at_history_duplicate_destination_rejects`

Key assertions:

1. Latest source writes after the requested frontier are not visible in the
   child.
2. The branch summary reports the actual fork version used.
3. Unretained history produces a stable engine error.

### Delete

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_delete_removes_from_list`
2. `branch_get_deleted_returns_not_found`
3. `branch_delete_unknown_rejects`
4. `branch_delete_default_rejects`
5. `branch_delete_last_live_branch_rejects`
6. `branch_delete_generation_mismatch_rejects`
7. `branch_delete_reports_cleanup_facts`
8. `branch_delete_with_pinned_read_reports_protected_cleanup`
9. `branch_delete_after_close_rejects`
10. `branch_delete_is_idempotence_decision_locked`

The final delete idempotence behavior must be explicit: either a second delete
returns not-found or a stable deleted/no-op outcome. The test name can be
renamed once the decision is encoded.

### Recreate

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_recreate_increments_generation`
2. `branch_recreate_does_not_inherit_deleted_kv_data`
3. `branch_recreate_can_be_forked_again`
4. `branch_recreate_summary_reports_new_generation`
5. `branch_recreate_durable_reopen_preserves_generation`
6. `branch_recreate_stale_generation_commit_rejects`

Key assertions:

1. Same name after recreate has the same branch id but a higher generation, if
   the product identity model keeps id deterministic by name.
2. Deleted data remains invisible.
3. KV writes using a stale generation fail.

### KV Branch Integration

Target: `crates/engine-next/tests/branch_and_kv.rs`.

Required tests:

1. `kv_resolves_branch_through_catalog_before_point_read`
2. `kv_resolves_branch_through_catalog_before_scan`
3. `kv_resolves_branch_through_catalog_before_history`
4. `kv_resolves_branch_through_catalog_before_write`
5. `kv_put_on_deleted_branch_rejects`
6. `kv_get_on_deleted_branch_rejects`
7. `kv_scan_on_deleted_branch_rejects`
8. `kv_history_on_deleted_branch_rejects`
9. `kv_stale_generation_commit_rejects`
10. `kv_fork_current_read_equivalence_point_scan_history`
11. `kv_fork_at_version_read_equivalence_point_scan_history`
12. `kv_branch_delete_and_recreate_keeps_histories_separate`

Key assertions:

1. Missing/deleted branches fail before persistence row reads.
2. Every mutating KV commit carries expected generation.
3. Point reads, scans, and history agree on branch visibility.

## Fault And Race Tests

Target: `crates/engine-next/tests/branch_faults.rs`.

### Pending Operation Fail-Closed

Required tests:

1. `pending_root_create_fails_closed_on_reopen`
2. `pending_fork_current_fails_closed_on_reopen`
3. `pending_fork_at_version_fails_closed_on_reopen`
4. `pending_fork_at_timestamp_fails_closed_on_reopen`
5. `pending_delete_fails_closed_on_reopen`

Assertions:

1. Open fails with a stable corruption error.
2. The half-created or half-deleted branch is not listed as healthy.
3. The error message identifies branch operation state, not lower storage
   details.

### Catalog Activation Faults

Required tests:

1. `storage_create_success_catalog_activation_failure_does_not_expose_branch`
2. `storage_fork_success_catalog_activation_failure_does_not_expose_branch`
3. `catalog_activation_failure_clears_pending_marker_or_fails_closed`
4. `delete_catalog_failure_does_not_report_success`

Assertions:

1. No successful API result is returned when catalog activation fails.
2. Durable reopen does not expose uncertain branch state as healthy.

### Same-Name Races

Required tests:

1. `concurrent_create_same_name_has_one_winner`
2. `concurrent_fork_same_destination_has_one_winner`
3. `concurrent_delete_and_recreate_preserves_generation_order`
4. `concurrent_delete_and_write_rejects_late_write`
5. `concurrent_delete_and_fork_from_source_either_orders_or_rejects`
6. `concurrent_recreate_and_stale_write_rejects_stale_write`

Assertions:

1. At most one live lifecycle instance exists for a name.
2. Losers return conflict, not corruption.
3. No stale write becomes visible after delete/recreate.

## Executor Test Matrix

Target: `crates/executor-next/tests/branch_command_contract.rs` and
`crates/executor-next/tests/branch_behavior.rs`.

### Command Contract

Required tests:

1. `branch_list_command_round_trips_json`
2. `branch_describe_command_round_trips_json`
3. `branch_create_command_round_trips_json`
4. `branch_fork_current_command_round_trips_json`
5. `branch_fork_at_version_command_round_trips_json`
6. `branch_fork_at_timestamp_command_round_trips_json`
7. `branch_delete_command_round_trips_json`
8. `branch_outputs_round_trip_json`
9. `branch_command_names_are_exhaustive`

### Delegation Behavior

Required tests:

1. `executor_branch_list_delegates_to_engine`
2. `executor_branch_describe_delegates_to_engine`
3. `executor_branch_create_delegates_to_engine`
4. `executor_branch_fork_current_delegates_to_engine`
5. `executor_branch_fork_at_version_delegates_to_engine`
6. `executor_branch_fork_at_timestamp_delegates_to_engine`
7. `executor_branch_delete_delegates_to_engine`
8. `executor_branch_errors_are_executor_shaped`
9. `executor_branch_commands_do_not_duplicate_kv_visibility_logic`

Assertions:

1. Executor outputs contain engine branch facts and no storage vocabulary.
2. Executor convenience methods call `execute(Command::...)`.

## Source And Dependency Guards

Target: `crates/engine-next/tests/dependency_guards.rs` and
`crates/executor-next/tests/error_and_guards.rs`.

Required guards:

1. `engine_public_branch_api_has_no_storage_types`
2. `engine_branch_service_does_not_import_storage_api`
3. `engine_branch_catalog_does_not_use_sha256_derivation`
4. `engine_branch_api_has_no_merge_method`
5. `engine_branch_api_has_no_publish_review_method`
6. `engine_branch_api_has_no_cherry_pick_revert_restore_methods`
7. `engine_branch_tests_use_permanent_domain_vocabulary`
8. `executor_branch_handlers_do_not_import_storage`
9. `executor_branch_helpers_delegate_through_execute_command`
10. `executor_branch_api_has_no_deferred_workflow_commands`

Guards should scan only relevant source directories and avoid brittle
substring bans that would fail on explanatory docs or unrelated data files.

## Property And Model Tests

Add a small deterministic model once create/fork/delete/recreate are all
implemented.

Model state:

1. map of branch name to live generation
2. tombstone generation history
3. parent/fork facts
4. per-branch latest KV values
5. selected read frontiers for historical fork cases

Generated operations:

1. create root
2. fork current
3. fork at retained version
4. put KV
5. delete branch
6. recreate branch
7. read point
8. list branches
9. close/reopen at selected boundaries for durable model runs

Required property:

1. `branch_lifecycle_model_matches_engine_behavior`

Sensitivity probes:

1. ignore generation mismatch
2. let fork-at-history use latest
3. resurrect deleted branch data
4. lose parent facts on reopen
5. allow two live generations for one name
6. allow stale write after delete
7. leak deleted branches into normal list

## Verification Commands

During implementation:

```bash
cargo fmt --all
cargo test -p strata-engine-next --all-features --test branch_identity
cargo test -p strata-engine-next --all-features --test branch_semantics
cargo test -p strata-engine-next --all-features --test branch_recovery
cargo test -p strata-engine-next --all-features --test branch_faults
cargo test -p strata-engine-next --all-features --test branch_and_kv
cargo test -p strata-engine-next --all-features --test control_plane
cargo test -p strata-engine-next --all-features --test persistence_adapter
cargo test -p strata-engine-next --all-features --test dependency_guards
cargo clippy -p strata-engine-next --all-features --all-targets -- -D warnings
```

After executor branch commands land:

```bash
cargo test -p strata-executor-next --all-features --test branch_command_contract
cargo test -p strata-executor-next --all-features --test branch_behavior
cargo test -p strata-executor-next --all-features --test error_and_guards
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```

Closeout:

```bash
cargo fmt --all --check
cargo test -p strata-engine-next --all-features
cargo test -p strata-executor-next --all-features
cargo clippy -p strata-engine-next --all-features --all-targets -- -D warnings
cargo clippy -p strata-executor-next --all-features --all-targets -- -D warnings
```

## Exit Gates

1. Branch id fixture tests match the old stable anchors.
2. Cache and durable-local pass the same branch lifecycle and KV visibility
   matrix.
3. Durable reopen preserves catalog, generation, parent/fork facts, default
   branch metadata, and tombstone generation state.
4. Pending branch operations fail closed.
5. Root create, fork-current, historical fork, delete, and recreate are all
   covered by positive and negative tests.
6. Stale writes after delete/recreate cannot become visible.
7. Executor branch commands round-trip and delegate to engine APIs.
8. Source guards prove deferred branch workflows are absent.
9. Engine and executor public errors do not leak storage implementation types.

## Stop Conditions

Stop implementation and revise the plan if any test reveals:

1. The current storage branch API cannot model cache fork visibility.
2. The nil default branch id conflicts with engine control-plane layout.
3. Generation-fenced commits do not prevent stale writes.
4. Durable reopen cannot distinguish healthy branch state from pending operation
   state.
5. Historical fork outcomes do not expose enough metadata for engine summaries.
6. Source guards need broad brittle substring bans to pass.
