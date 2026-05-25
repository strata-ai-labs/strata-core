# L8S Test Plan: Table-Object Reachability And Retention

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that table-object retention is proof-backed, deterministic, and safe:
live objects are retained, unreferenced objects become quarantine candidates
only with complete safe proof, and unsupported or incomplete scopes never report
clean success.

The suite must fail if L8S:

1. treats table-object retention as a silent no-op;
2. deletes or purges a table object;
3. trusts prefix-listed objects as live without a table manifest;
4. ignores inherited-layer reachability;
5. ignores cross-branch shared reachability;
6. emits quarantine candidates under unsafe recovery health;
7. accepts stale proof tokens;
8. reports unsupported table-object scope as completed success;
9. imports raw IO, product code, primitive DTOs, or quarantine mutation APIs.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. table-object inventory classification;
2. manifest-backed reachability graph;
3. inherited-layer and shared-object retention;
4. quarantine-candidate generation;
5. proof-incomplete and unsafe-health barriers;
6. proof-token freshness;
7. cache-mode unsupported behavior;
8. generated/property counters;
9. source guards.

Not covered:

1. table-manifest byte format, covered by L8Q;
2. table-manifest publication/recovery, covered by L8R;
3. quarantine object movement, covered by L8M;
4. purge, covered by L8M;
5. table-manifest-backed WAL truncation, covered by L8T;
6. row pruning, covered by L8V.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `quarantine_protocol.rs::retention_snapshot` | Durable manifests decide live/shared/detached/quarantined bytes. | Graph decisions come from trusted table manifests and quarantine inventory. |
| `gc_orphan_segments` | Orphan files are candidates only after manifest proof and healthy recovery. | Orphan table object becomes quarantine candidate only under complete safe proof. |
| `gc_under_degradation.rs` | Corrupt/missing manifest recovery blocks orphan GC. | Unsafe recovery yields retain/deferred decisions. |
| `quarantine_reconciliation.rs` | Quarantine inventory disagreement blocks unsafe purge. | Inventory mismatch delegates to repair and blocks candidate mutation. |
| `SegmentRefRegistry` | Runtime refs are acceleration, not durable truth. | Runtime-only reachability cannot justify delete/quarantine. |
| `retention_report.rs` | Product report is built above raw storage facts. | L8S emits storage-shaped decisions only. |

Tests must not port:

1. direct filesystem deletion;
2. product retention report text;
3. branch-name attribution;
4. logs-only debt;
5. old public GC commands;
6. row pruning assertions.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/retention.rs` for direct proof and
   decision tests.
2. `crates/storage-next/src/lifecycle/tests/retention/` if direct tests approach
   1,000 lines.
3. `crates/storage-next/src/testkit/lifecycle/retention.rs` for generated
   reachability scripts.
4. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration smoke
   if maintenance tests remain grouped there.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for boundary checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

## Test Data Principles

1. Use table-manifest fixtures produced by L8Q/L8R helpers.
2. Include at least two branches.
3. Include one object referenced by two branches.
4. Include one object referenced only by an inherited layer.
5. Include one prefix-listed orphan table object.
6. Include one already-quarantined table object.
7. Include healthy, telemetry, policy-downgrade, data-loss, and failed recovery
   health.
8. Include stale proof tokens with changed manifest epoch, inventory epoch, and
   recovery health epoch.
9. Assert object names and byte counts, not only decision counts.

## Direct Unit Tests

### 1. Inventory And Graph Construction

Required tests:

1. `table_object_inventory_lists_candidates_in_stable_order`
2. `table_object_inventory_rejects_malformed_object_name`
3. `table_object_inventory_failure_records_proof_incomplete`
4. `reachability_graph_retains_manifest_owned_table`
5. `reachability_graph_retains_inherited_layer_table`
6. `reachability_graph_retains_materialization_replacement_table`
7. `reachability_graph_records_shared_object_reasons`
8. `reachability_graph_ignores_non_table_object_families`
9. `reachability_graph_is_deterministic_for_shuffled_inputs`
10. `reachability_graph_reports_manifest_object_names`

Assertions:

1. graph order does not depend on map iteration;
2. only table-object namespace entries participate;
3. every live object has at least one durable reason.

### 2. Decision Classification

Required tests:

1. `manifest_referenced_object_is_retained_live`
2. `shared_object_is_retained_until_all_refs_drop`
3. `inherited_layer_object_is_retained`
4. `prefix_orphan_with_complete_safe_proof_is_quarantine_candidate`
5. `prefix_orphan_with_incomplete_manifest_proof_is_retained`
6. `prefix_orphan_with_inventory_failure_is_retained`
7. `already_quarantined_object_is_delegated`
8. `unsupported_table_object_scope_is_not_completed_success`
9. `no_table_objects_returns_completed_empty_graph`
10. `malformed_table_object_name_returns_repair_candidate`

Assertions:

1. unsupported/no-op scopes are explicit;
2. candidates include object names, bytes, and proof tokens;
3. L8S never returns a direct delete decision.

### 3. Recovery Health Gates

Required tests:

1. `healthy_recovery_allows_quarantine_candidate`
2. `telemetry_health_allows_unrelated_table_object_candidate`
3. `policy_downgrade_blocks_table_object_candidate`
4. `data_loss_blocks_table_object_candidate`
5. `failed_health_blocks_table_object_candidate`
6. `unsafe_health_records_health_debt`
7. `unsafe_health_retains_all_candidates`
8. `health_generation_change_stales_proof_token`

Assertions:

1. unsafe health blocks before mutation;
2. health debt is structured;
3. proof tokens are bound to current health.

### 4. Proof Tokens

Required tests:

1. `proof_token_includes_manifest_epoch`
2. `proof_token_includes_table_inventory_epoch`
3. `proof_token_includes_quarantine_inventory_epoch`
4. `proof_token_includes_recovery_health_epoch`
5. `proof_token_includes_object_fingerprint`
6. `proof_token_rejects_manifest_epoch_change`
7. `proof_token_rejects_table_inventory_epoch_change`
8. `proof_token_rejects_quarantine_inventory_epoch_change`
9. `proof_token_rejects_recovery_health_epoch_change`
10. `proof_token_rejects_object_fingerprint_change`

Assertions:

1. hand-constructed freshness without epochs is impossible;
2. stale candidates cannot reach L8M mutation paths;
3. token validation is deterministic.

### 5. Quarantine Integration Boundary

Required tests:

1. `quarantine_candidate_can_build_l8m_request`
2. `already_quarantined_object_is_not_requarantined`
3. `quarantine_inventory_mismatch_blocks_candidate`
4. `quarantine_inventory_mismatch_records_repair_fact`
5. `l8s_does_not_call_quarantine_mutation`
6. `l8s_does_not_call_purge`
7. `candidate_state_changes_are_visible_in_maintenance_outcome`

Assertions:

1. L8S only hands off facts;
2. L8M owns durable mutation;
3. inventory mismatch is not treated as success.

### 6. Cache Mode

Required tests:

1. `cache_table_object_retention_returns_unsupported`
2. `cache_table_object_retention_does_not_list_objects`
3. `cache_table_object_retention_does_not_construct_table_manifest_service`
4. `cache_table_object_retention_does_not_claim_durable_reachability`
5. `cache_table_object_retention_outcome_names_mode`

Assertions:

1. cache mode has no durable table-object retention;
2. unsupported is explicit and non-successful where required;
3. no durable service is touched.

### 7. No Mutation Guarantees

Required tests:

1. `table_object_retention_does_not_delete_candidate_object`
2. `table_object_retention_does_not_move_candidate_to_quarantine`
3. `table_object_retention_does_not_rewrite_quarantine_inventory`
4. `table_object_retention_does_not_update_database_manifest`
5. `table_object_retention_does_not_truncate_wal`
6. `table_object_retention_does_not_prune_snapshots`

Assertions:

1. L8S is proof/classification only;
2. side-effect counters remain zero for mutation services;
3. all destructive behavior is deferred to later slices.

### 8. Old Regression Shapes

Required tests:

1. `corrupt_manifest_health_blocks_orphan_candidate`
2. `missing_manifest_health_blocks_orphan_candidate`
3. `runtime_only_ref_does_not_make_object_live`
4. `manifest_ref_without_runtime_ref_still_retains_object`
5. `deleted_parent_orphan_storage_is_not_clean_success`
6. `quarantine_only_directory_does_not_recreate_manifest`
7. `shared_object_survives_one_branch_manifest_drop`
8. `shared_object_becomes_candidate_after_all_manifest_refs_drop`

Assertions:

1. durable manifest proof wins over runtime acceleration;
2. degraded recovery gates reclaim;
3. shared reachability behaves like old manifest-retention contract.

## Generated And Property Tests

Add a table-object reachability script family:

```text
add_manifest(branch, objects)
drop_manifest(branch)
add_table_object(object, facts)
mark_quarantined(object)
set_recovery_health(class)
build_reachability()
assert_decisions()
```

Required counters:

1. live_owned;
2. live_inherited;
3. live_shared;
4. orphan_candidate;
5. proof_incomplete;
6. unsafe_health_blocked;
7. already_quarantined;
8. unsupported_scope;
9. stale_token_rejected;
10. no_mutation_observed.

Properties:

1. every manifest-referenced object is retained;
2. no unmanifested object is live solely because it is prefix-listed;
3. unsafe recovery never produces quarantine candidates;
4. graph output is stable under shuffled inputs;
5. mutation counters remain zero.

## Source Guards

Required source guard tests:

1. `table_reachability_does_not_import_raw_io`
2. `table_reachability_does_not_import_backend_delete`
3. `table_reachability_does_not_import_quarantine_mutation`
4. `table_reachability_does_not_import_purge`
5. `table_reachability_does_not_import_engine_or_product_crates`
6. `table_reachability_does_not_import_stratahub`
7. `table_reachability_does_not_import_primitive_modules`
8. `table_reachability_does_not_use_product_retention_report`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `std::env`
4. `delete_object`
5. `purge_quarantine`
6. `quarantine_object`
7. `retention_report`
8. `strata_engine`
9. `stratahub`
10. `primitive`
11. `graph`
12. `vector`
13. `json`

Scope the scan to production reachability/retention modules so test names do
not create false positives.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Treat prefix-listed object as live without manifest. | Runtime-only/ref test fails. |
| S2 | Delete orphan candidate directly. | No-mutation test fails. |
| S3 | Ignore inherited-layer manifest refs. | Inherited live test fails. |
| S4 | Ignore shared refs from second branch. | Shared-object test fails. |
| S5 | Allow candidate under data-loss health. | Unsafe health test fails. |
| S6 | Accept stale proof token after manifest change. | Token stale test fails. |
| S7 | Report unsupported scope as clean completed success. | Unsupported-scope test fails. |
| S8 | Call quarantine mutation from reachability code. | Source guard/no-mutation test fails. |
| S9 | Drop affected object names from candidate outcome. | Decision classification test fails. |
| S10 | Use map iteration order for graph output. | Determinism property fails. |

## Command Matrix

Mandatory commands before L8S closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8S test coverage is complete when:

1. table-object retention has no silent success no-op path;
2. live, inherited, shared, orphan, incomplete, unsafe, quarantined, and
   unsupported decisions are tested;
3. proof tokens reject stale manifest/inventory/health facts;
4. no tests observe direct delete, quarantine mutation, purge, checkpoint, or
   WAL truncation from L8S;
5. generated properties cover ordering and safety categories;
6. source guards enforce boundaries;
7. sensitivity probes and command results are recorded in the porting log.
