# L8Y Test Plan: Branch Lifecycle Completeness

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that storage-internal branch lifecycle operations are correct,
generation-safe, recovery-safe, and ready for the public API layer without
embedding product branch policy.

The suite must fail if L8Y:

1. silently overwrites or duplicates branch descriptors;
2. loses source rows during fork;
3. allows fork-at-history outside retained history;
4. lets clear/delete invalidate pinned read views;
5. physically deletes shared table objects from branch lifecycle code;
6. lets stale flush/compaction/materialization outputs resurrect old rows;
7. ignores branch generation on queued work;
8. lets deleted branch WAL/table-manifest state resurrect after recovery;
9. returns product-policy errors instead of storage facts;
10. adds milestone labels to Rust code, test names, fixture bytes, or user-facing
    error strings.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. branch catalog create/list/descriptor validation;
2. duplicate create and missing branch rejection;
3. current-state fork;
4. fork-at-history;
5. clear;
6. delete;
7. generation reuse;
8. pinned read views;
9. stale queued maintenance and rewrite tasks;
10. recovery of branch lifecycle facts;
11. cache and durable local behavior;
12. generated/fault/source-guard assurance.

Not covered:

1. public API naming and user permissions;
2. product branch workflows;
3. merge, cherry-pick, revert, restore, compare;
4. remote/hub branch sync;
5. object-store provider semantics;
6. direct object deletion policy;
7. query/index API.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `storage/src/segmented/mod.rs` | Branch create/fork/clear/delete mutate segmented branch state atomically. | Lifecycle branch catalog operations produce all-or-nothing raw facts. |
| `storage/src/segmented/ref_registry.rs` | Shared references block unsafe deletion. | Clear/delete emit release facts and pinned-view roots keep objects retained. |
| `storage/src/segmented/compaction.rs` | Stale rewrite candidates cannot resurrect cleared/deleted state. | Stale flush/compaction/materialization outputs reject before publication/install. |
| `storage/src/segmented/recovery.rs` | Recovery honors deleted/missing/corrupt branch state. | Deleted markers and generations outrank older manifests/WAL. |
| `storage/src/segmented/quarantine_protocol.rs` | Ambiguous released objects go through quarantine/reclaim proof. | Branch lifecycle never backend-deletes table objects directly. |
| Old resurrection tests | Clear/delete racing with maintenance does not restore rows. | Generated and direct tests simulate stale branch work after lifecycle transitions. |

Tests must not port:

1. product branch naming;
2. primitive-specific reconstruction;
3. raw filesystem path logic;
4. process-global reference registries;
5. background thread nondeterminism;
6. user-facing branch policy.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/branch_lifecycle.rs` for direct
   lifecycle tests.
2. `crates/storage-next/src/branch/tests/` for L6 pinned-view and inherited
   layer regression tests when the lower-layer invariant is local to L6.
3. `crates/storage-next/src/commit/tests/branch_registry.rs` for generation and
   admission tests.
4. `crates/storage-next/src/lifecycle/tests/recovery.rs` for durable recovery
   and bootstrap tests.
5. `crates/storage-next/src/lifecycle/tests/checkpoint.rs` and
   `crates/storage-next/src/lifecycle/tests/compaction.rs` for lifecycle
   interactions.
6. `crates/storage-next/src/testkit/lifecycle/branch_lifecycle.rs` for generated
   branch operation scripts.
7. `crates/storage-next/tests/lifecycle_branch_lifecycle.rs` for integration
   smoke.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source guards.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split files before they exceed 1,000 lines.

## Test Data Principles

1. Use at least three branch ids in deterministic byte order.
2. Use nonzero branch generations and at least one generation reuse.
3. Include active rows, frozen rows, owned tables, inherited layers, and
   materialized replacement tables.
4. Include shared table identities referenced by more than one branch.
5. Include pinned views captured before clear/delete.
6. Include commit versions before, at, and after fork-at-history boundaries.
7. Include timestamp coverage and insufficient-history cases.
8. Include stale queued maintenance tasks with old generations.
9. Include durable manifests/checkpoints/WAL rows for deleted and recreated
   branches.
10. Avoid timing, sleeping, random ordering, or host-dependent memory behavior.

## Direct Unit Tests

### 1. Branch Catalog Create And List

Required tests:

1. `branch_catalog_create_empty_branch`
2. `branch_catalog_duplicate_create_rejects`
3. `branch_catalog_create_rejects_zero_generation`
4. `branch_catalog_list_active_branches_in_deterministic_order`
5. `branch_catalog_list_includes_deleted_when_requested`
6. `branch_catalog_missing_lookup_rejects`
7. `branch_catalog_descriptor_branch_mismatch_rejects`
8. `branch_catalog_commit_registry_stays_coherent`
9. `branch_catalog_create_does_not_publish_table_objects`
10. `branch_catalog_cache_create_reports_no_durable_claim`

Assertions:

1. duplicate create returns the typed already-exists error;
2. list ordering is deterministic;
3. lifecycle catalog and commit registry have matching descriptors.

### 2. Current-State Fork

Required tests:

1. `fork_current_missing_source_rejects_before_destination_mutation`
2. `fork_current_existing_destination_rejects`
3. `fork_current_nonempty_destination_rejects`
4. `fork_current_source_with_active_rows_is_flushed_or_rejected_explicitly`
5. `fork_current_inherits_owned_tables_without_copying_objects`
6. `fork_current_inherited_rows_are_visible_in_child`
7. `fork_current_child_local_row_shadows_inherited_row`
8. `fork_current_source_later_write_does_not_change_child_view`
9. `fork_current_records_source_branch_and_fork_version`
10. `fork_current_reachability_facts_include_shared_tables`
11. `fork_current_works_from_materialized_replacement_tables`
12. `fork_current_preserves_inherited_chain_order`

Assertions:

1. source rows are never silently dropped;
2. child state uses inherited references or documented materialization, not
   eager object copies;
3. child-local precedence matches L6 read rules.

### 3. Fork At History

Required tests:

1. `fork_at_history_retained_version_succeeds`
2. `fork_at_history_visible_latest_matches_current_fork`
3. `fork_at_history_after_visible_version_rejects`
4. `fork_at_history_below_retained_floor_rejects`
5. `fork_at_history_missing_timestamp_coverage_rejects`
6. `fork_at_history_timestamp_lookup_uses_timeline_tiebreaker`
7. `fork_at_history_child_excludes_rows_after_requested_version`
8. `fork_at_history_child_includes_rows_at_requested_version`
9. `fork_at_history_tombstone_at_boundary_is_preserved`
10. `fork_at_history_recovered_child_keeps_requested_fork_version`
11. `fork_at_history_source_deleted_before_capture_rejects`
12. `fork_at_history_destination_generation_guard_is_enforced`

Assertions:

1. retained-history proof is required;
2. no row newer than the fork version appears in the child;
3. timestamp lookup follows the L7 timeline rule.

### 4. Clear Branch

Required tests:

1. `clear_branch_removes_active_frozen_owned_and_inherited_rows`
2. `clear_branch_keeps_branch_id_and_generation_active`
3. `clear_branch_pinned_view_can_still_read_old_rows`
4. `clear_branch_new_view_sees_empty_branch`
5. `clear_branch_after_clear_accepts_new_commits`
6. `clear_branch_release_facts_name_owned_and_inherited_tables`
7. `clear_branch_does_not_delete_table_objects`
8. `clear_branch_rejects_missing_branch`
9. `clear_branch_rejects_deleted_branch`
10. `clear_branch_stale_flush_output_cannot_resurrect_rows`
11. `clear_branch_stale_compaction_output_cannot_resurrect_rows`
12. `clear_branch_stale_materialization_output_cannot_resurrect_rows`

Assertions:

1. clear is atomic from new readers' perspective;
2. pinned readers remain valid;
3. stale work is generation or lifecycle-state rejected before publication or
   install.

### 5. Delete Branch

Storage-internal clear and delete are atomic synchronous transitions:
the only externally observable branch states are `Active` and
`Deleted`. Observable transients (a `Deleting` state visible to
admission guards) belong at higher layers where async work happens and
are intentionally out of scope here.

Required tests:

1. `delete_branch_pinned_view_can_still_read_old_rows`
2. `delete_branch_new_read_rejects_after_deleted`
3. `delete_branch_commit_rejects_after_deleted`
4. `delete_branch_release_facts_feed_retention`
5. `delete_branch_does_not_backend_delete_shared_table_objects`
6. `delete_branch_missing_branch_rejects`
7. `delete_branch_already_deleted_is_idempotent_or_typed`
8. `delete_branch_with_shared_parent_table_keeps_parent_readable`
9. `delete_branch_durable_tombstone_prevents_recovery_resurrection`

Assertions:

1. delete returns raw release facts;
2. shared tables remain reachable while another branch or pinned view references
   them.

### 6. Generation Reuse

Required tests:

1. `recreate_deleted_branch_requires_greater_generation`
2. `recreate_deleted_branch_rejects_same_generation`
3. `recreate_deleted_branch_rejects_lower_generation`
4. `recreate_deleted_branch_rejects_generation_exhaustion`
5. `stale_commit_generation_rejects_after_recreate`
6. `stale_flush_task_generation_rejects_after_recreate`
7. `stale_compaction_task_generation_rejects_after_recreate`
8. `stale_materialization_task_generation_rejects_after_recreate`
9. `new_generation_does_not_see_old_rows`
10. `recovery_preserves_highest_generation`

Assertions:

1. queued work carries generation facts;
2. stale generation rejects before durable object publication;
3. generation reuse never exposes old rows in the new branch.

### 7. Recovery

Required tests:

1. `recovery_rebuilds_multiple_branch_descriptors`
2. `recovery_rebuilds_active_branch_states`
3. `recovery_rebuilds_inherited_layers`
4. `recovery_rebuilds_fork_at_history_version`
5. `recovery_deleted_marker_outranks_older_table_manifest`
6. `recovery_newer_generation_outranks_older_deleted_marker`
7. `recovery_rejects_wal_row_for_missing_branch`
8. `recovery_rejects_wal_row_for_deleted_generation`
9. `recovery_preserves_branch_release_facts`
10. `recovery_checkpoint_multi_branch_rows_round_trip`
11. `recovery_table_manifest_multi_branch_rows_round_trip`

Assertions:

1. recovery returns typed conflicts instead of product policy decisions;
2. deleted and generation facts are not lost;
3. row visibility after recovery matches pre-restart pinned expectations.

### 8. Maintenance And Rewrite Interactions

Required tests:

1. `maintenance_missing_branch_rejects_before_publication`
2. `maintenance_deleting_branch_rejects_before_publication`
3. `maintenance_deleted_branch_rejects_before_publication`
4. `flush_target_generation_checked_before_table_build`
5. `compaction_target_generation_checked_before_output_publish`
6. `materialization_target_generation_checked_before_handle_bind`
7. `checkpoint_includes_all_active_branch_rows`
8. `checkpoint_excludes_deleted_branch_rows`
9. `retention_receives_branch_release_candidates`
10. `quarantine_receives_ambiguous_branch_release_candidates`

Assertions:

1. branch lifecycle admission happens before expensive or durable side effects;
2. checkpoint/table-manifest facts remain branch-tagged;
3. retention/quarantine handoff is explicit.

### 9. Inter-Branch Isolation

Required tests:

1. `commit_to_branch_a_does_not_change_branch_b`
2. `clear_branch_a_does_not_change_branch_b`
3. `delete_branch_a_does_not_change_branch_b`
4. `fork_branch_a_to_branch_c_does_not_change_branch_b`
5. `materialize_branch_c_does_not_change_branch_a`
6. `row_with_wrong_branch_id_rejects_install`
7. `shared_table_delete_candidate_is_blocked_by_other_branch`
8. `prefix_scan_branch_a_does_not_emit_branch_b_rows`
9. `as_of_branch_a_does_not_use_branch_b_timeline`
10. `history_branch_a_does_not_use_branch_b_rows`

Assertions:

1. branch id remains part of every visibility and install contract;
2. shared physical table references do not collapse logical branch isolation.

### 10. Pinned View Reachability

Required tests:

1. `pinned_view_survives_clear`
2. `pinned_view_survives_delete`
3. `pinned_view_survives_recreate_same_branch_id_new_generation`
4. `pinned_view_survives_source_branch_delete_after_fork`
5. `pinned_view_blocks_object_release_until_dropped`
6. `pinned_view_release_unblocks_retention_candidate`
7. `pinned_view_inherited_layer_rows_remain_readable`
8. `pinned_view_materialized_rows_remain_readable`
9. `pinned_view_records_generation_at_capture`
10. `pinned_view_cannot_observe_partial_clear_or_delete`

Assertions:

1. pinned views are retention roots;
2. old views and new views observe coherent snapshots.

## Cache Mode Tests

Required tests:

1. `cache_branch_create_list_fork_clear_delete_smoke`
2. `cache_branch_reopen_loses_volatile_branches`
3. `cache_branch_delete_reports_no_durable_claim`
4. `cache_branch_clear_does_not_call_durable_services`
5. `cache_branch_fork_does_not_publish_table_manifest`
6. `cache_branch_generation_guards_match_durable_mode`
7. `cache_branch_lifecycle_after_close_rejects`
8. `cache_branch_lifecycle_while_closing_rejects`

Assertions:

1. cache mode remains durable-claim-free;
2. branch lifecycle semantics that do not depend on durability match durable
   local mode.

## Durable Mode Tests

Required tests:

1. `durable_branch_create_publishes_branch_catalog_fact`
2. `durable_branch_fork_publishes_inherited_reachability`
3. `durable_branch_clear_publishes_empty_branch_state`
4. `durable_branch_delete_publishes_deleted_marker`
5. `durable_branch_recreate_publishes_new_generation`
6. `durable_branch_recovery_round_trips_create_fork_clear_delete`
7. `durable_branch_recovery_rejects_corrupt_branch_catalog`
8. `durable_branch_recovery_rejects_manifest_generation_mismatch`
9. `durable_branch_lifecycle_publish_failure_reports_uncertain_fact`
10. `durable_branch_lifecycle_retry_is_idempotent`

Assertions:

1. durable facts are enough to recover without product callbacks;
2. publication uncertainty is typed and retryable only when safe.

## Generated Model Tests

Add a generated branch-lifecycle model with operations:

1. create branch;
2. commit row;
3. rotate/flush branch;
4. fork current;
5. fork at version;
6. clear;
7. delete;
8. recreate deleted branch;
9. capture pinned view;
10. drop pinned view;
11. run stale maintenance candidate;
12. checkpoint/recover.

Reference model requirements:

1. model stores branch id, generation, status, rows, inherited sources, retained
   floors, and pinned-view roots independently of production code;
2. model computes point, prefix, range, history, and as-of reads;
3. model rejects stale generations and deleted branches;
4. model records release candidates and pinned retention roots;
5. generated scripts must use input bytes to choose operations, branch ids,
   generations, versions, and fault points.

Required tests:

1. `branch_lifecycle_generated_model_matches_point_reads`
2. `branch_lifecycle_generated_model_matches_prefix_reads`
3. `branch_lifecycle_generated_model_matches_as_of_reads`
4. `branch_lifecycle_generated_model_tracks_generation_reuse`
5. `branch_lifecycle_generated_model_tracks_pinned_view_roots`
6. `branch_lifecycle_generated_model_rejects_stale_work`
7. `branch_lifecycle_generated_model_recovery_round_trip`
8. `branch_lifecycle_generated_model_no_resurrection_after_clear_delete`

## Fault Windows

Required fault tests:

1. `fault_create_descriptor_published_before_empty_state`
2. `fault_create_state_installed_before_descriptor_publish`
3. `fault_fork_descriptor_published_before_inherited_refs`
4. `fault_fork_inherited_refs_published_before_descriptor`
5. `fault_clear_deleting_commits_blocked_before_state_swap`
6. `fault_clear_state_swapped_before_release_facts_publish`
7. `fault_delete_marker_published_before_release_facts`
8. `fault_delete_release_facts_published_before_deleted_marker`
9. `fault_recreate_new_generation_published_before_old_delete_reconciled`
10. `fault_recovery_wal_row_for_deleted_generation`
11. `fault_stale_compaction_after_branch_delete`
12. `fault_pinned_view_release_after_delete`

Assertions:

1. recovery either converges or returns typed health facts;
2. no fault window creates partial branch visibility;
3. no stale artifact becomes visible after a clear/delete/recreate boundary.

## Source Guards

Required source-guard tests:

1. branch lifecycle code does not import engine, primitive, query, remote, hub,
   raw filesystem, environment, or network APIs;
2. branch lifecycle code does not call backend delete for table objects;
3. branch lifecycle code does not include product branch vocabulary such as
   merge, cherry-pick, revert, restore, workspace policy, permission, or remote
   ref except in comments explicitly listing out-of-scope behavior;
4. L6 branch modules do not import lifecycle;
5. L7 commit modules do not import lifecycle branch catalog code;
6. milestone labels do not appear in Rust code, test function names, fixture
   bytes, fuzz corpora, or user-facing error strings;
7. source guards scan production source and tests, not just one module.

## Sensitivity Probes

Record each probe in the porting log with mutation site and fired test.

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Allow duplicate branch create to replace descriptor. | Duplicate-create and generated model tests fail. |
| S2 | Skip source active/frozen precondition during fork. | Fork source row-loss test fails. |
| S3 | Use latest source version for fork-at-history. | Fork-at-history boundary tests fail. |
| S4 | Ignore retained floor for fork-at-history. | Below-retained-floor test fails. |
| S5 | Clear branch by dropping pinned-view refs. | Pinned-view clear tests fail. |
| S6 | Delete branch and backend-delete table objects directly. | Source guard and shared-table tests fail. |
| S7 | Permit commits while branch is deleting. | Delete admission tests fail. |
| S8 | Skip generation guard on queued flush. | Stale flush generation test fails. |
| S9 | Skip generation guard on queued compaction. | Stale compaction generation test fails. |
| S10 | Skip generation guard on materialization handle bind. | Stale materialization test fails. |
| S11 | Let deleted marker lose to older table manifest. | Recovery deleted-marker test fails. |
| S12 | Recreate branch with same generation. | Generation reuse tests fail. |
| S13 | Let branch B timeline satisfy branch A as-of. | Inter-branch as-of test fails. |
| S14 | Omit release facts on clear. | Retention handoff test fails. |
| S15 | Return product-policy wording from a storage error. | Source guard fails. |
| S16 | Put milestone label in Rust fixture bytes. | Source guard fails. |

## Fuzz Targets

Required targets:

1. `branch_lifecycle_catalog`
2. `branch_lifecycle_fork`
3. `branch_lifecycle_clear_delete`
4. `branch_lifecycle_recovery`

Rules:

1. Each target must decode a distinct operation script.
2. Each target must have at least three seed corpus files with semantic bytes,
   not ASCII labels only.
3. The closeout test must verify that each target calls its own contract.
4. Generated counters must distinguish input-derived coverage from canonical
   smoke coverage.

## Integration Tests

Required integration tests:

1. `lifecycle_branch_lifecycle_cache_smoke`
2. `lifecycle_branch_lifecycle_durable_smoke`
3. `lifecycle_branch_lifecycle_recovery_smoke`
4. `lifecycle_branch_lifecycle_clear_delete_no_resurrection`
5. `lifecycle_branch_lifecycle_pinned_view_retention`
6. `lifecycle_branch_lifecycle_generation_reuse`
7. `lifecycle_branch_lifecycle_fork_at_history`
8. `lifecycle_branch_lifecycle_stale_maintenance_rejection`

These tests should exercise public crate test helpers, not planning-document
presence.

## Verification Commands

Run at slice closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_branch_lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle_properties
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_branch_lifecycle_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

If `cargo hack` is available:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

No-default and wasm smoke:

```bash
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
```

Nightly fuzz smoke, when nightly and cargo-fuzz are available:

```bash
cargo +nightly fuzz run branch_lifecycle_catalog -- -runs=256
cargo +nightly fuzz run branch_lifecycle_fork -- -runs=256
cargo +nightly fuzz run branch_lifecycle_clear_delete -- -runs=256
cargo +nightly fuzz run branch_lifecycle_recovery -- -runs=256
```

## Exit Gate

L8Y test coverage is complete when:

1. all direct branch lifecycle operations have positive and negative tests;
2. pinned read views are tested across clear, delete, recreate, and source
   branch deletion;
3. stale maintenance and rewrite tasks are generation-checked before side
   effects;
4. fork-at-history is tested against retained and unretained versions;
5. recovery round trips active, deleted, cleared, forked, and recreated branches;
6. retention/quarantine handoff is tested without direct object deletion;
7. generated and fuzz tests use input bytes for branch ids, operations,
   versions, generations, and faults;
8. source guards cover production source, test source, fixture bytes, and fuzz
   corpora;
9. sensitivity probes are recorded with the exact test that failed;
10. the command matrix and outcomes are recorded in the porting log.
