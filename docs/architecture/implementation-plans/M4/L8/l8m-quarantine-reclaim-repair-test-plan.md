# L8M Test Plan: Quarantine, Reclaim, Purge, And Repair Facts

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that lifecycle reclaim cannot delete reachable or unproven storage
objects, that quarantine inventory is durable before destructive mutation, and
that purge and repair expose enough raw storage facts for L9/engine policy
without importing product behavior.

Tests should fail if L8M:

1. quarantines or purges under unsafe recovery health;
2. treats incomplete proof as safe;
3. deletes a source object before quarantine inventory and copy are durable;
4. purges an object not listed in quarantine inventory;
5. treats stale purge proof as fresh;
6. hides inventory/object mismatch;
7. drops lower-layer source chains;
8. reports durable reclaim success in cache mode;
9. deletes directly through the backend from lifecycle code;
10. imports engine/product/primitive vocabulary into lifecycle reclaim.

Do not add tests whose only assertion is that plan documents exist or link to
other plan documents.

## Coverage Boundary

In scope:

1. lifecycle quarantine request/proof/outcome validation;
2. L8L retention candidate to quarantine request conversion;
3. quarantine mutation over `QuarantineService`;
4. purge over `QuarantineService`;
5. repair/reconciliation over `QuarantineService`;
6. maintenance routing for quarantine, purge, and repair;
7. cache-mode unsupported/deferred behavior;
8. error codes and source chains;
9. generated testkit counters;
10. source guards.

Out of scope:

1. public repair/reclaim commands;
2. branch deletion and branch clear policy;
3. close-time final drain and sync;
4. WAL truncation;
5. snapshot pruning;
6. row-version retention policy;
7. object-store distributed lease extensions;
8. crash/fuzz closeout.

Those belong to L8N, L8O, L8P, L9, engine-next, or later retention work.

## Old-Code Regression Sources

The old codebase supplies safety behavior, not API names.

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/storage/src/segmented/quarantine_protocol.rs::check_reclaim_allowed` | Unsafe degraded recovery blocks reclaim before filesystem mutation. | Quarantine and purge under data-loss/policy-downgrade health perform no backend writes/deletes. |
| `quarantine_segment_if_unreferenced_inner` | Runtime refcount is only candidate input; durable manifest/inherited-layer truth decides safety. | Runtime-only proof is incomplete; referenced candidate is deferred. |
| `quarantine_segment_if_unreferenced_inner` | Inventory publish precedes object movement; source delete happens after durable quarantine copy. | Fault-injected reports prove operation order. |
| `purge_all_quarantines` | Purge drains inventory-listed objects, treats not-found as gone, and rewrites retained failures. | Purge deletes only listed objects; failures stay retained. |
| `reconcile_quarantine_on_recovery` | Inventory/object mismatch degrades health and blocks unsafe reclaim. | Repair reports mismatch facts and does not clear them silently. |
| `crates/storage/src/quarantine.rs` | Inventory is per-branch, relocation-safe, and validates format identity. | Database id, branch id, codec id, source object, and object id validation fail closed. |
| `crates/storage/src/segmented/ref_registry.rs` | Deletion barrier prevents race but does not replace durable proof. | L8M tests do not accept registry-only safety. |

Tests must not port:

1. raw filesystem path behavior;
2. product branch retention reports;
3. engine background scheduler behavior;
4. logs-only assertions;
5. follower-mode refresh behavior.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/quarantine.rs` for direct tests.
2. `crates/storage-next/src/lifecycle/tests/quarantine/` for shared fixtures if
   the direct test file approaches 1,000 lines.
3. `crates/storage-next/src/lifecycle/tests/retention.rs` only for candidate
   handoff cases that need existing L8L helpers.
4. `crates/storage-next/src/lifecycle/tests/maintenance.rs` only for generic
   executor behavior.
5. `crates/storage-next/src/testkit/lifecycle/quarantine.rs` for generated
   proof/mutation/purge/repair scripts.
6. `crates/storage-next/tests/lifecycle_reclaim_close.rs` if this slice adds a
   dedicated reclaim integration target.
7. `crates/storage-next/tests/lifecycle_maintenance.rs` if lifecycle
   integration tests remain grouped there.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Do not put behavior assertions in a documentation closeout test.

## Test Data Principles

1. Build object names through `ObjectLayout` or service helpers.
2. Use database id and codec id mismatches intentionally in negative tests.
3. Use non-epoch timestamps unless testing timestamp validation.
4. Include source objects from table, snapshot, and WAL families only where the
   service allows them; reject quarantine-as-source.
5. Keep branch ids explicit and distinct.
6. Assert stable error codes and source-chain classes, not display strings.
7. Assert operation logs for "no backend access" cases.
8. Include recovery health variants: healthy, telemetry degraded, policy
   downgrade, data loss, and failed.
9. Include service fault windows: inventory publish, quarantine publish, source
   delete, purge delete, inventory rewrite, reconciliation list/read.
10. Keep architecture labels out of Rust code, test names, fixture bytes, and
    panic messages.

## Direct Unit Tests

### 1. Request And Proof Validation

Required tests:

1. `quarantine_request_rejects_empty_object_id`
2. `quarantine_request_rejects_quarantine_inventory_object_id`
3. `quarantine_request_rejects_quarantine_source_object`
4. `quarantine_request_rejects_epoch_timestamp_without_override`
5. `quarantine_request_rejects_missing_database_id_or_codec`
6. `quarantine_proof_complete_from_quarantine_candidate`
7. `quarantine_proof_defers_referenced_candidate`
8. `quarantine_proof_defers_incomplete_reachability`
9. `quarantine_proof_blocks_data_loss_recovery`
10. `quarantine_proof_blocks_policy_downgrade_recovery`
11. `quarantine_proof_allows_unrelated_telemetry_debt`
12. `purge_request_requires_fresh_safe_proof`
13. `purge_request_rejects_stale_proof`
14. `repair_request_rejects_empty_codec_id`

Assertions:

1. invalid requests fail before service calls;
2. deferred proof produces no backend writes/deletes;
3. blocked proof carries recovery health debt.

### 2. Quarantine Mutation

Required tests:

1. `quarantine_publishes_inventory_before_quarantine_object`
2. `quarantine_copies_source_before_source_delete`
3. `quarantine_source_delete_happens_only_after_durable_copy`
4. `quarantine_inventory_publish_failure_does_not_copy_or_delete_source`
5. `quarantine_inventory_publish_uncertain_reports_retryable_health_debt`
6. `quarantine_publish_failure_keeps_inventory_evidence`
7. `quarantine_publish_uncertain_preserves_publish_failure_kind`
8. `quarantine_source_delete_failure_reports_retained_source`
9. `quarantine_source_already_missing_after_publish_is_retry_safe`
10. `quarantine_existing_matching_inventory_is_idempotent`
11. `quarantine_existing_matching_inventory_retries_source_delete`
12. `quarantine_existing_conflicting_source_fails_closed`
13. `quarantine_existing_unlisted_quarantine_object_fails_closed`
14. `quarantine_database_mismatch_fails_closed`
15. `quarantine_codec_mismatch_fails_closed`
16. `quarantine_branch_mismatch_fails_closed`
17. `quarantine_backend_metadata_mismatch_fails_closed`

Assertions:

1. operation order is observable through the backend/service log;
2. at least one recoverable copy exists across every publish/delete fault
   window;
3. outcomes list source object, quarantine object, inventory object, byte
   count, retryability, source error, and health debt.

### 3. Purge

Required tests:

1. `purge_requires_complete_safe_proof`
2. `purge_rejects_data_loss_recovery_health_before_backend_access`
3. `purge_rejects_policy_downgrade_recovery_health_before_backend_access`
4. `purge_with_missing_inventory_is_completed_noop_or_inconclusive_by_policy`
5. `purge_deletes_only_inventory_listed_quarantine_objects`
6. `purge_does_not_delete_original_source_object`
7. `purge_treats_missing_listed_object_as_already_gone`
8. `purge_delete_failure_retains_inventory_entry`
9. `purge_inventory_rewrite_failure_reports_source_chain`
10. `purge_idempotent_after_success`
11. `purge_stale_proof_rejects_even_with_inventory`
12. `purge_reports_reclaimed_bytes_when_inventory_has_byte_counts`
13. `purge_preserves_failed_object_names`

Assertions:

1. purge never exceeds the inventory list;
2. failed deletes stay retryable through retained inventory entries;
3. source chains survive maintenance outcome conversion.

### 4. Repair And Reconciliation

Required tests:

1. `repair_clean_empty_inventory_reports_completed_noop`
2. `repair_clean_inventory_reports_listed_objects`
3. `repair_corrupt_inventory_reports_policy_debt`
4. `repair_database_mismatch_reports_corrupt_inventory`
5. `repair_codec_mismatch_reports_corrupt_inventory`
6. `repair_missing_listed_quarantine_object_reports_mismatch`
7. `repair_unlisted_quarantine_object_reports_mismatch`
8. `repair_malformed_quarantine_object_reports_mismatch`
9. `repair_backend_list_failure_reports_unavailable`
10. `repair_backend_read_failure_reports_unavailable`
11. `repair_family_scope_is_deterministic_across_branches`
12. `repair_does_not_delete_without_explicit_proof`
13. `repair_does_not_synthesize_reachability`
14. `repair_reports_inconclusive_facts_without_mutation`

Assertions:

1. repair facts distinguish clean, corrupt, missing, unlisted, malformed, and
   unavailable;
2. default repair is read-only;
3. family reports are sorted deterministically.

### 5. Maintenance Routing

Required tests:

1. `quarantine_task_builds_quarantine_scope`
2. `purge_task_builds_quarantine_scope`
3. `repair_task_accepts_quarantine_and_global_scope`
4. `quarantine_task_rejected_before_open`
5. `purge_task_rejected_while_closing`
6. `repair_task_rejected_after_close_requested`
7. `quarantine_task_coalesces_by_branch_and_source_object`
8. `purge_task_coalesces_by_branch`
9. `repair_task_coalesces_by_scope`
10. `quarantine_task_failure_adds_health_debt`
11. `purge_task_failure_adds_health_debt`
12. `repair_task_failure_adds_health_debt`
13. `quarantine_task_skips_unrelated_pending_tasks`
14. `purge_task_skips_unrelated_pending_tasks`
15. `repair_task_skips_unrelated_pending_tasks`
16. `durable_quarantine_runs_through_runtime_maintenance_surface`
17. `durable_purge_runs_through_runtime_maintenance_surface`
18. `durable_repair_runs_through_runtime_maintenance_surface`

Assertions:

1. task admission uses the lifecycle state machine;
2. rejected work does not mutate branch or backend state;
3. durable runners preserve executor task id, stats, affected names, and reason
   class.

### 6. Cache Mode

Required tests:

1. `cache_quarantine_task_returns_unsupported_before_backend_access`
2. `cache_purge_task_returns_unsupported_before_backend_access`
3. `cache_repair_task_returns_unsupported_before_backend_access`
4. `cache_reclaim_task_does_not_construct_quarantine_service`
5. `cache_reclaim_outcome_does_not_report_durable_success`
6. `cache_reclaim_tasks_do_not_remain_stranded`

Assertions:

1. cache mode rejects or completes unsupported tasks in a way that does not
   leave durable-only work pending forever;
2. cache mode does not import or call manifest, WAL, snapshot, table-object, or
   quarantine services.

### 7. Error And Source Chains

Required tests:

1. `quarantine_incomplete_proof_error_has_stable_code`
2. `quarantine_inventory_mismatch_error_has_stable_code`
3. `quarantine_publication_error_preserves_service_source`
4. `quarantine_uncertain_publication_error_preserves_publish_kind`
5. `purge_stale_proof_error_has_stable_code`
6. `purge_delete_failure_preserves_backend_error`
7. `repair_inconclusive_error_has_stable_code`
8. `repair_backend_unavailable_preserves_backend_error`
9. `maintenance_outcome_preserves_source_error_for_quarantine_failure`
10. `error_display_does_not_include_object_payload_bytes`

Assertions:

1. tests assert `code()`, not `Display`;
2. lower-layer source chains survive outcome conversion;
3. object names may be present, object bytes must not be present.

## Integration Tests

Add or extend `lifecycle_reclaim_close.rs` or the existing lifecycle
maintenance integration target with:

1. `lifecycle_quarantine_integration`
2. `lifecycle_purge_integration`
3. `lifecycle_repair_reconciliation_integration`
4. `lifecycle_reclaim_blocks_unsafe_recovery_integration`
5. `lifecycle_cache_reclaim_unsupported_integration`
6. `lifecycle_quarantine_then_purge_round_trip`
7. `lifecycle_quarantine_publish_failure_surfaces_health_debt`

These should run through lifecycle maintenance entry points, not only direct
helper functions.

## Generated Testkit Contract

Add `check_lifecycle_quarantine_contract`.

Counters:

1. `complete_safe_proof_cases`
2. `incomplete_proof_cases`
3. `blocked_recovery_cases`
4. `referenced_candidate_cases`
5. `staged_object_cases`
6. `already_quarantined_cases`
7. `inventory_publish_failure_cases`
8. `quarantine_publish_failure_cases`
9. `source_delete_failure_cases`
10. `purged_object_cases`
11. `purge_delete_failure_cases`
12. `stale_purge_proof_cases`
13. `corrupt_inventory_repair_cases`
14. `unlisted_object_repair_cases`
15. `cache_deferred_cases`

The generated contract should decode input bytes into:

1. storage mode;
2. recovery health;
3. branch id;
4. database/codec identity variant;
5. source object family;
6. candidate proof status;
7. quarantine fault point;
8. purge proof freshness;
9. inventory shape;
10. reconciliation shape.

Keep canonical smoke coverage separate from input-derived counters. Property
tests should prove generated bytes influence at least one quarantine, purge, or
repair route.

## Source Guards

Extend `lifecycle_source_guard.rs`.

Required checks:

1. quarantine lifecycle code does not import engine modules;
2. quarantine lifecycle code does not import product retention/repair modules;
3. quarantine lifecycle code does not import primitive modules;
4. quarantine lifecycle code does not use raw `std::fs`, `Path`, `File`,
   `OpenOptions`, mmap, or `std::env`;
5. quarantine lifecycle code does not call backend `delete_object` directly;
6. quarantine lifecycle code does not encode/decode quarantine inventory bytes
   directly;
7. quarantine lifecycle code does not call WAL truncation or snapshot pruning;
8. quarantine lifecycle code does not parse object-family paths by hand when
   L2/L4 helpers exist;
9. lifecycle recovery bootstrap does not contain durable maintenance
   quarantine/purge/repair runners;
10. cache lifecycle code does not import quarantine, manifest, WAL, snapshot, or
    table-object services;
11. lower layers do not import lifecycle;
12. Rust code/tests do not include architecture slice labels.

Add fixture self-tests that prove each guard can fail.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| L8M-S1 | Treat incomplete proof as safe. | Incomplete-proof quarantine test fails. |
| L8M-S2 | Allow quarantine under data-loss recovery. | Recovery-health gate test fails. |
| L8M-S3 | Delete source before quarantine publish. | Operation-order test fails. |
| L8M-S4 | Skip inventory publish before quarantine copy. | Inventory-before-copy test fails. |
| L8M-S5 | Treat conflicting inventory as idempotent. | Inventory-mismatch test fails. |
| L8M-S6 | Purge without fresh proof. | Stale-proof purge test fails. |
| L8M-S7 | Delete unlisted quarantine object. | Purge listed-only test fails. |
| L8M-S8 | Drop failed purge entry from inventory. | Retained-entry test fails. |
| L8M-S9 | Repair corrupt inventory as clean. | Repair corruption test fails. |
| L8M-S10 | Delete during default repair. | Repair no-delete test/source guard fails. |
| L8M-S11 | Hide source chain on service failure. | Source-chain test fails. |
| L8M-S12 | Report cache reclaim as durable success. | Cache unsupported test fails. |
| L8M-S13 | Call backend delete directly. | Source guard fails. |
| L8M-S14 | Import product retention report. | Source guard fails. |

## Verification Commands

Mandatory commands after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib service::quarantine
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo fmt --package strata-storage-next --check
git diff --check
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
```

If a dedicated reclaim integration target is added, also run:

```bash
cargo test -p strata-storage-next --locked --test lifecycle_reclaim_close
```

## Exit Gate

L8M is complete when:

1. complete, incomplete, referenced, and recovery-blocked proof cases are
   tested;
2. unsafe recovery health blocks quarantine and purge before backend access;
3. inventory publish precedes quarantine copy/source delete;
4. publish/delete fault windows preserve source chains and affected object
   names;
5. existing matching quarantine state is idempotent;
6. conflicting inventory/object state fails closed;
7. purge requires fresh safe proof;
8. purge deletes only inventory-listed quarantine objects;
9. purge failures retain entries and report health debt;
10. repair/reconciliation distinguishes all required mismatch classes;
11. repair is read-only by default and never deletes without proof;
12. cache mode cannot claim durable reclaim;
13. maintenance routing covers quarantine, purge, and repair tasks;
14. generated testkit counters cover proof, staging, purge, repair, and cache
    cases;
15. source guards cover product, raw IO, direct delete, WAL/snapshot,
    bootstrap, cache-mode, and architecture-label drift;
16. sensitivity probes are recorded;
17. the verification commands pass.
