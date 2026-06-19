# Engine Core Control Plane Test Plan

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/engine-core-control-plane-implementation-plan.md`

## Goal

Prove that the engine has a narrow, durable, hidden, and fail-closed core
control plane for database identity, registry validation, branch catalog,
branch lineage, default branch state, space catalog, and reserved namespace
protection.

This test plan intentionally does not test recipes, search/query, retrieval,
shadow vectors, derived-state health, local AI, or caches. Those systems need
separate plans.

## Test Targets

Expected integration targets:

```text
crates/engine-next/tests/control_plane.rs
crates/engine-next/tests/control_plane_registry.rs
crates/engine-next/tests/control_plane_space.rs
crates/engine-next/tests/control_plane_recovery.rs
crates/engine-next/tests/control_plane_visibility.rs
crates/engine-next/tests/branch_semantics.rs
crates/engine-next/tests/dependency_guards.rs
```

Expected source-local unit targets:

```text
crates/engine-next/src/control/
crates/engine-next/src/persistence/key.rs
crates/engine-next/src/persistence/space.rs
crates/engine-next/src/branch/
```

## Test Data Policy

1. Use temp directories for durable tests.
2. Do not write under user home paths.
3. Use deterministic branch names: `default`, `main`, `feature`, `scratch`,
   `parent`, and `child`.
4. Use deterministic space names: `default`, `kvspace`, `jsonspace`,
   `eventspace`, `vectorspace`, `graphspace`, and `_system_`.
5. Use small payloads in control-row corruption tests so expected byte shapes
   are readable.
6. Use cache and durable-local variants for visibility and branch-local space
   behavior.
7. Do not use recipe, search, retrieval, intelligence, inference, or derived
   index fixtures in this suite.

## Required Harness Helpers

Add or extend test helpers:

1. `open_cache_database()`
2. `open_durable_database(path)`
3. `reopen_durable_database(path)`
4. `create_branch(db, name)`
5. `fork_branch(db, source, name)`
6. `delete_branch(db, name)`
7. `create_space(db, branch, space)`
8. `list_spaces(db, branch)`
9. `assert_branch_hidden(db, "_system_")`
10. `assert_space_hidden(db, branch, "_system_")`
11. `assert_control_health_ok(db)`
12. `assert_control_health_error(db, code)`
13. `inject_control_row(path_or_db, row_class, key, payload)`
14. `delete_control_row(path_or_db, row_class, key)`
15. `corrupt_control_row(path_or_db, row_class, key, payload)`
16. `collect_rust_sources(root)`

Fault-injection helpers must be test-only and must not become public engine or
executor APIs.

## Traceability Matrix

| Invariant | Primary tests |
| --- | --- |
| System branch is hidden and protected | `system_branch_not_listed`, `system_branch_cannot_be_created_or_deleted` |
| System space is hidden and protected | `system_space_not_listed`, `system_space_cannot_be_created_or_deleted` |
| Global control rows live on system branch | `global_control_rows_commit_to_system_branch` |
| Branch-local rows live in branch-local system space | `space_catalog_rows_are_branch_local` |
| Registry validates on open | `durable_open_rejects_registry_conflict` |
| Identity validates on open | `durable_open_rejects_missing_identity` |
| Branch catalog validates on open | `durable_open_rejects_missing_default_branch_catalog` |
| Pending operations fail closed | `durable_open_rejects_unresolved_pending_branch_operation` |
| Space catalog validates lazily and through diagnostics | `space_catalog_corruption_fails_branch_space_diagnostics` |
| Control rows do not leak into primitive scans | primitive scan visibility tests |
| Upper layers cannot write raw control rows | dependency/source guards |
| Deferred systems stay out of core | source guards for recipes/search/retrieval/shadow/derived/cache row families |

## Unit Test Matrix

### Row Class Registry

Target: `crates/engine-next/src/persistence/space.rs`.

Required tests:

1. `core_control_row_ids_are_locked`
2. `branch_control_uses_0x30`
3. `space_control_uses_0x31`
4. `registry_uses_0x32`
5. `identity_uses_0x34`
6. `core_registry_has_no_duplicate_ids`
7. `core_registry_marks_0x33_unused`
8. `core_registry_marks_derived_ids_out_of_scope`

Assertions:

1. The compiled core registry has stable IDs.
2. `0x33` is not assigned to recipes in this slice.
3. `0x40..=0x45` are not assigned by core control tests.

### Control Key Encoding

Target: `crates/engine-next/src/persistence/key.rs` and `control` unit tests.

Required tests:

1. `database_identity_key_is_stable`
2. `local_instance_identity_key_is_stable`
3. `storage_registry_key_is_stable`
4. `capability_registry_key_is_stable`
5. `migration_registry_key_is_stable`
6. `branch_index_key_is_stable`
7. `branch_default_key_is_stable`
8. `branch_catalog_key_orders_by_branch_name`
9. `branch_generation_key_orders_by_branch_id`
10. `branch_lineage_key_orders_by_branch_generation_and_sequence`
11. `branch_pending_key_orders_by_operation_id`
12. `space_index_key_is_stable`
13. `space_catalog_key_orders_by_space_name`
14. `reserved_space_key_is_stable`

Assertions:

1. Keys round-trip through encode/decode where decode exists.
2. Prefix scans return deterministic ordering.
3. Keys do not use recipe, search, retrieval, shadow vector, derived, or cache
   prefixes.

### Payload Encoding

Target: source-local control unit tests.

Required tests:

1. `database_identity_round_trips`
2. `database_identity_rejects_unknown_version`
3. `database_identity_rejects_truncated_payload`
4. `local_instance_identity_round_trips`
5. `registry_seed_round_trips`
6. `registry_rejects_duplicate_storage_space_ids`
7. `registry_rejects_label_mismatch`
8. `capability_registry_round_trips`
9. `migration_registry_round_trips`
10. `branch_catalog_record_round_trips`
11. `branch_catalog_record_rejects_unknown_lifecycle`
12. `branch_lineage_record_round_trips_fork`
13. `branch_lineage_record_rejects_unknown_edge_kind`
14. `space_catalog_record_round_trips`
15. `reserved_space_record_round_trips_system_space`
16. `reserved_space_record_rejects_user_managed_system_space`

Assertions:

1. Every source row has an explicit payload version.
2. Unknown future payload versions fail closed.
3. Corruption maps to structured engine errors, not generic storage strings.

## Integration Test Matrix

### Create And Open

Target: `crates/engine-next/tests/control_plane.rs`.

Required tests:

1. `cache_create_bootstraps_core_control_rows`
2. `durable_create_bootstraps_core_control_rows`
3. `durable_reopen_validates_core_control_rows`
4. `create_initializes_default_branch_space_catalog`
5. `open_starts_primitive_services_after_core_validation`

Assertions:

1. Database identity exists.
2. Registry rows exist.
3. Branch catalog and default branch rows exist.
4. Default branch has branch-local space catalog rows.
5. Product KV writes still work after bootstrap.

### System Branch Visibility

Target: `crates/engine-next/tests/control_plane_visibility.rs`.

Required tests:

1. `system_branch_not_listed`
2. `system_branch_cannot_be_selected_by_product_api`
3. `system_branch_cannot_be_created`
4. `system_branch_cannot_be_deleted`
5. `system_branch_uuid_alias_cannot_be_selected`
6. `ordinary_underscore_branch_names_are_reserved`

Assertions:

1. User branch APIs never expose `_system_`.
2. Reserved-branch failures use stable invalid-input or not-found errors.
3. Diagnostics can report system branch health without returning it as a user
   branch.

### System Space Visibility

Target: `crates/engine-next/tests/control_plane_space.rs`.

Required tests:

1. `system_space_not_listed_on_default_branch`
2. `system_space_not_listed_on_created_branch`
3. `system_space_not_listed_on_forked_branch`
4. `system_space_exists_as_reserved_fact`
5. `system_space_cannot_be_created`
6. `system_space_cannot_be_deleted`
7. `system_space_cannot_be_used_for_user_kv`
8. `system_space_cannot_be_used_for_user_json`
9. `system_space_cannot_be_used_for_user_event`
10. `system_space_cannot_be_used_for_user_vector`
11. `system_space_cannot_be_used_for_user_graph`

Assertions:

1. `_system_` is implicit for engine control use.
2. `_system_` never appears in user-visible space lists.
3. Primitive services reject `_system_` as a user space before committing.

### Space Catalog

Target: `crates/engine-next/tests/control_plane_space.rs`.

Required tests:

1. `space_catalog_starts_with_default_space`
2. `space_catalog_registers_user_space_once`
3. `space_catalog_register_is_idempotent`
4. `space_catalog_lists_spaces_sorted`
5. `space_catalog_survives_durable_reopen`
6. `space_catalog_is_branch_local`
7. `space_catalog_fork_inherits_expected_space_facts`
8. `space_catalog_delete_removes_user_space`
9. `space_catalog_delete_default_rejects`
10. `space_catalog_delete_system_rejects`

Assertions:

1. Branch-local space metadata does not leak across branches.
2. Fork behavior matches the branch operation contract.
3. User data in a deleted space is handled by the owning primitive cleanup path,
   not by raw control-row deletion alone.

### Branch Catalog And Lineage

Target: `crates/engine-next/tests/branch_semantics.rs`.

Required tests:

1. `branch_create_writes_catalog_and_generation_guard`
2. `branch_fork_writes_catalog_generation_guard_and_lineage`
3. `branch_delete_updates_catalog_generation_guard_and_lifecycle`
4. `branch_recreate_increments_generation`
5. `branch_default_fact_survives_reopen`
6. `branch_lineage_survives_reopen`
7. `branch_catalog_does_not_depend_on_graph_projection`

Assertions:

1. Branch lineage is readable from control rows.
2. No graph product rows are required to answer current branch lineage queries.
3. Default branch metadata is validated before product branch selection.

### Pending Operation Recovery

Target: `crates/engine-next/tests/control_plane_recovery.rs`.

Required tests:

1. `open_rejects_pending_branch_create_without_activation`
2. `open_rejects_pending_branch_fork_without_activation`
3. `open_rejects_pending_branch_delete_without_completion`
4. `open_recovers_completed_branch_operation_with_cleared_pending_row`
5. `pending_operation_index_rejects_unknown_operation_kind`

Assertions:

1. Interrupted operations fail closed.
2. No half-created branch appears as healthy.
3. Error codes distinguish pending-operation failure from storage corruption.

### Corruption And Missing Rows

Target: `crates/engine-next/tests/control_plane_recovery.rs`.

Required tests:

1. `open_rejects_missing_database_identity`
2. `open_rejects_corrupt_database_identity`
3. `open_rejects_missing_storage_registry`
4. `open_rejects_corrupt_storage_registry`
5. `open_rejects_registry_id_conflict`
6. `open_rejects_missing_default_branch_row`
7. `open_rejects_default_branch_missing_catalog_record`
8. `open_rejects_duplicate_branch_names`
9. `open_rejects_generation_guard_behind_catalog_generation`
10. `space_diagnostics_reject_corrupt_space_catalog`
11. `space_diagnostics_reject_missing_reserved_system_space_fact`
12. `space_diagnostics_reject_user_managed_system_space_fact`

Assertions:

1. Existing durable databases fail closed on corrupt source control rows.
2. Create-path initialization is the only path that may create missing source
   rows.
3. Errors carry stable code families.

### Primitive Isolation

Target: existing primitive integration tests plus
`crates/engine-next/tests/control_plane_visibility.rs`.

Required tests:

1. `kv_scan_does_not_return_control_rows`
2. `json_scan_does_not_return_control_rows`
3. `event_read_does_not_return_control_rows`
4. `vector_list_does_not_return_control_rows`
5. `graph_list_does_not_return_control_rows`
6. `control_row_write_does_not_create_user_space`
7. `user_space_write_does_not_create_control_row`

Assertions:

1. Primitive key decoders do not decode core control rows.
2. User space creation is explicit through space catalog service.
3. `_system_` remains inaccessible through primitive commands.

### Diagnostics

Target: `crates/engine-next/tests/control_plane.rs`.

Required tests:

1. `core_control_health_reports_identity_registry_branch_and_space`
2. `core_control_health_omits_recipe_status`
3. `core_control_health_omits_search_status`
4. `core_control_health_omits_retrieval_status`
5. `core_control_health_omits_shadow_vector_status`
6. `core_control_health_omits_derived_index_status`
7. `core_control_health_does_not_expose_raw_keys_by_default`

Assertions:

1. Health output is typed and stable.
2. Raw control paths are absent unless a future explicit admin mode adds them.
3. Deferred systems do not appear in core health.

## Source And Dependency Guards

Target: `crates/engine-next/tests/dependency_guards.rs` or equivalent.

Required tests:

1. `only_control_and_persistence_modules_import_control_key_helpers`
2. `ordinary_primitive_services_do_not_construct_system_space_keys`
3. `executor_does_not_import_control_modules`
4. `inference_does_not_import_control_modules`
5. `intelligence_does_not_import_control_modules`
6. `no_recipe_control_row_family_in_core_control`
7. `no_search_control_row_family_in_core_control`
8. `no_retrieval_control_row_family_in_core_control`
9. `no_shadow_vector_control_row_family_in_core_control`
10. `no_derived_manifest_row_family_in_core_control`
11. `no_query_cache_row_family_in_core_control`
12. `branch_lineage_tests_do_not_depend_on_graph_store`

Assertions:

1. Upper layers request product services instead of raw control writes.
2. Deferred systems stay out of this slice.
3. Branch lineage authority does not require graph product code.

## Cross-Mode Matrix

Run these suites in cache and durable-local mode:

1. System branch visibility.
2. System space visibility.
3. Space catalog registration and branch-local behavior.
4. Branch catalog create/fork/delete behavior.
5. Primitive isolation.

Run these suites in durable-local mode only:

1. Create/reopen validation.
2. Missing-row faults.
3. Corruption faults.
4. Pending operation recovery.
5. Registry conflict tests.

## Acceptance Criteria

1. All core control unit and integration tests pass.
2. Cache and durable-local behavior match where persistence is not the behavior
   under test.
3. Durable reopen proves identity, registry, branch catalog, branch lineage,
   default branch, and space catalog persistence.
4. Corrupt source control rows fail closed.
5. `_system_` branch and `_system_` space remain hidden and protected.
6. Primitive services do not expose or decode core control rows.
7. Source/dependency guards prove executor, inference, intelligence, and
   primitive services cannot bypass control services for raw control writes.
8. No recipe, search, retrieval, shadow vector, derived manifest, or cache row
   family is introduced.

## Non-Goals

Do not add tests for:

1. Recipe resolution.
2. Search/query behavior.
3. Retrieval policies.
4. Autoembedding or shadow vectors.
5. Derived-state rebuilds.
6. Query/prompt caches.
7. CLI or SDK control inspection.
8. Merge/revert/restore behavior.
