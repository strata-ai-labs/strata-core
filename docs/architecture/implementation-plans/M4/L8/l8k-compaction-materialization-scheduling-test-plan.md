# L8K Test Plan: Compaction And Materialization Scheduling Hooks

Status: implemented for the conservative V1 table-rewrite scope

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that lifecycle compaction and materialization scheduling delegates
algorithmic behavior to L6/L5, preserves read semantics, handles partial
publication windows explicitly, and reports storage pressure without product
vocabulary.

## V1 Coverage Boundary

The implemented V1 scope is deliberately conservative:

1. cache compaction/materialization run through L6 and return branch-runtime
   outcomes;
2. durable-local compaction/materialization run through the same L6 paths and
   report checkpoint-required outcomes;
3. standalone table-object publication for compaction/materialization is
   deferred until table-manifest recovery can make those objects durable
   reachability facts;
4. tests therefore focus on request validation, maintenance task routing,
   no-candidate/deferred outcomes, read parity, checkpoint-required durable
   debt, and storage pressure facts.

The durable publish/reopen/published-not-installed tests listed below remain
the reference-grade envelope for the later table-manifest slice. They are not
required to close this conservative V1 scheduling slice.

Tests should fail if L8K:

1. chooses compaction input rows without L6 candidate facts;
2. reimplements L5 table merge semantics;
3. rewrites inherited rows directly instead of using L6 materialization;
4. installs branch rewrite outputs before durable publication when durable
   publication is claimed;
5. deletes replaced table objects;
6. drops tombstones or older versions without a retention proof;
7. loses child-local precedence during materialization;
8. treats no-candidate or already-materialized work as hard failure;
9. emits product write-stall wording;
10. hides lower-layer source chains behind generic display strings.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Old-Code Regression Sources

The old codebase supplies regression behavior, not API names.

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/storage/src/segmented/compaction.rs` | Branch compaction is scheduled from level/table facts and preserves read results. | Lifecycle asks L6 for candidates and read parity holds after keep-all compaction. |
| `crates/storage/src/segmented/tests/leveled.rs` | L0, L0-to-L1, and nonzero-level compaction have distinct candidate behavior. | Tests cover each L6 compaction kind through lifecycle routing. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | Publish failure leaves old state visible; install/fsync failure after publish is a partial-progress window. | Lifecycle reports failed maintenance with health debt and no hidden branch-state loss. |
| `crates/storage/src/test_hooks.rs` | Compaction/materialization can pause between I/O and atomic install. | Deterministic fake runners/fault hooks cover the same boundary without global pause hooks. |
| `crates/storage/src/segmented/mod.rs::materialize_layer` | Materialization preserves child-local precedence and removes the inherited layer only after replacement is visible. | Lifecycle uses L6 handle-based materialization and verifies pre/post read parity. |
| `crates/storage/src/segmented/tests/resurrection.rs` | Compaction/materialization must not resurrect cleared/deleted branch state after a stale I/O window. | Lifecycle stale-candidate tests fail closed and preserve current reads. |
| `crates/engine/src/database/transaction.rs::schedule_background_compaction` | Duplicate background compaction chains are coalesced and one unit of work runs before re-evaluation. | Maintenance executor coalesces duplicate compaction/materialization tasks and generated scripts run one task at a time. |
| `crates/engine/src/database/transaction.rs::pick_and_run_one` | Scheduler chooses the highest storage-pressure task and also tries inherited-layer materialization. | Pressure facts suggest deterministic compaction/materialization tasks without spawning background work. |
| `crates/engine/src/database/transaction.rs::check_write_stall` | Product write-stall policy uses L0, memory, and metadata pressure. | Storage-next only emits storage pressure facts; tests reject product write-stall wording. |
| `crates/engine/src/database/open.rs` post-recovery compaction scheduling | Recovery schedules compaction so accumulated L0 state does not linger forever. | L8K exposes explicit hooks and pressure facts; automatic post-open scheduling remains deferred and must not appear accidentally. |
| `crates/engine/src/database/tests/shutdown.rs` | Mutating maintenance APIs reject after shutdown starts. | Lifecycle admission tests reject compaction/materialization while closing. |
| `crates/engine/src/database/compaction.rs` | Engine may request compaction, but storage owns physical maintenance facts. | L8 tests use storage maintenance vocabulary only; no public command or product wording appears. |

Tests must not port:

1. old public command names;
2. raw filesystem paths or direct file operations;
3. old primitive value DTOs;
4. logs-only failure assertions;
5. background-thread timing assumptions;
6. condition-variable write-stall behavior;
7. manifest publication or object deletion from the compaction handler.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/compaction.rs` for direct runtime
   tests.
2. `crates/storage-next/src/lifecycle/tests/compaction/` for helpers if the
   direct file approaches 1,000 lines.
3. `crates/storage-next/src/lifecycle/tests/maintenance.rs` only for generic
   executor routing shared with other task kinds.
4. `crates/storage-next/src/branch/tests/owned_compaction.rs` and
   `crates/storage-next/src/branch/tests/inheritance_materialization/` as
   lower-layer regression evidence, not as lifecycle substitutes.
5. `crates/storage-next/src/testkit/lifecycle/compaction.rs` for generated
   scripts and counters.
6. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration smoke
   through the lifecycle maintenance entry point.
7. `crates/storage-next/tests/lifecycle_properties.rs` for generated lifecycle
   properties behind `testkit`.
8. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
9. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

## Test Data Principles

1. Build rows through L7/L6 helpers where possible.
2. Use storage rows with multiple physical keys, multiple commit versions,
   tombstones, expired rows, and at least two branches.
3. Use branch-owned immutable tables created by L6/L5 helpers, not hand-edited
   descriptors.
4. Include overlapping L0 tables and non-overlapping nonzero-level tables.
5. Include inherited layers with fork-version gates and child-local shadowing.
6. Keep fixture bytes free of reserved layout literals unless the test is a
   layout guard.
7. Assert stable error codes and source-chain classes, not display text.
8. Keep canonical smoke scripts separate from input-derived generated coverage
   counters.

## Direct Unit Tests

### 1. Request And Task Validation

Required tests:

1. `compaction_request_rejects_wrong_branch_scope`
2. `compaction_request_rejects_empty_output_seed`
3. `compaction_request_rejects_path_like_output_seed`
4. `compaction_request_rejects_pruning_policy_without_retention_proof`
5. `materialization_request_rejects_wrong_scope`
6. `materialization_request_rejects_empty_output_prefix`
7. `materialization_request_rejects_path_like_output_prefix`
8. `maintenance_request_builds_compaction_scope`
9. `maintenance_request_builds_materialization_scope`
10. `compaction_and_materialization_tasks_coalesce_by_scope`
11. `kind_specific_runner_skips_unrelated_pending_tasks`

Assertions:

1. invalid lifecycle facts fail before lower-layer calls;
2. task scope must match task kind;
3. coalescing keys include branch/level/table or branch/layer facts;
4. concrete runners must not remove or fail unrelated pending tasks.

### 2. Lifecycle Admission

Required tests:

1. `compaction_rejected_before_open`
2. `compaction_rejected_while_closing`
3. `materialization_rejected_before_open`
4. `materialization_rejected_while_closing`
5. `queued_compaction_does_not_run_after_close_begins_unless_drain_required`
6. `queued_materialization_does_not_run_after_close_begins_unless_drain_required`
7. `failed_maintenance_does_not_transition_open_runtime_to_closed`

Assertions:

1. admission uses lifecycle state machine;
2. rejected work does not mutate branch state;
3. close policy remains owned by the maintenance executor.

### 3. Compaction Candidate Routing

Required tests:

1. `compaction_asks_branch_runtime_for_candidate`
2. `compaction_empty_level_returns_deferred`
3. `compaction_single_l0_table_returns_deferred`
4. `compaction_last_level_returns_deferred`
5. `compaction_l0_candidate_reports_input_refs`
6. `compaction_l0_to_level_one_candidate_reports_overlap_refs`
7. `compaction_nonzero_level_candidate_reports_output_level`
8. `compaction_stale_candidate_fails_without_mutation`
9. `compaction_invalid_level_returns_branch_source_error`
10. `compaction_report_is_forwarded_from_lower_layer`

Assertions:

1. lifecycle does not select row ranges itself;
2. no-candidate is a deferred maintenance outcome;
3. stale candidates fail closed and preserve old read results.

### 4. Compaction Read Semantics

Required tests:

1. `compaction_preserves_latest_point_reads`
2. `compaction_preserves_history_reads`
3. `compaction_preserves_prefix_scans`
4. `compaction_preserves_range_scans`
5. `compaction_preserves_tombstones`
6. `compaction_preserves_older_versions_under_keep_all`
7. `compaction_preserves_expired_rows_under_keep_all`
8. `compaction_preserves_branch_ids`
9. `compaction_preserves_commit_timestamps`
10. `compaction_preserves_materialization_provenance_when_all_inputs_share_it`

Assertions:

1. keep-all compaction is observationally equivalent;
2. L8 does not introduce hidden retention rules;
3. provenance facts remain those produced by L6.

### 5. Durable Publication Windows

Required tests:

1. `durable_compaction_publishes_output_before_install_when_publication_is_claimed`
2. `durable_compaction_reopens_output_before_install_when_publication_is_claimed`
3. `durable_compaction_publish_failure_leaves_reads_unchanged`
4. `durable_compaction_reopen_failure_leaves_reads_unchanged`
5. `durable_compaction_install_failure_after_publish_reports_health_debt`
6. `durable_compaction_install_failure_after_publish_lists_published_objects`
7. `durable_compaction_without_table_manifest_reports_checkpoint_required`
8. `durable_compaction_does_not_advance_flush_watermark`
9. `durable_compaction_does_not_truncate_wal`
10. `cache_compaction_does_not_call_table_object_service`

Assertions:

1. durable claims require L4 publication and validation first;
2. published-not-installed is visible in outcome facts;
3. replay shortening remains checkpoint-owned.

### 6. Materialization Intent And Retry

Required tests:

1. `materialization_marks_layer_materializing_before_install`
2. `materialization_uses_handle_to_build_request`
3. `materialization_never_uses_naked_layer_index_after_intent`
4. `materialization_no_layer_returns_deferred`
5. `materialization_already_materialized_returns_idempotent_outcome`
6. `materialization_retry_after_partial_replacement_removes_layer`
7. `materialization_retry_after_compacted_partial_replacement_does_not_collide`
8. `materialization_preserves_materializing_status_on_failure`
9. `materialization_source_facts_are_reported`
10. `materialization_failure_preserves_source_chain`
11. `materialization_retry_after_removed_layer_uses_source_handle`

Assertions:

1. lifecycle binds intent to handle;
2. retry paths are explicit outcomes, not generic success strings;
3. L6 remains the owner of materialization recovery states;
4. absent-layer retry with a source identity reaches the L6 idempotence path.

### 7. Materialization Read Semantics

Required tests:

1. `materialization_preserves_latest_point_reads`
2. `materialization_preserves_history_reads`
3. `materialization_preserves_prefix_scans`
4. `materialization_preserves_range_scans`
5. `materialization_preserves_tombstones`
6. `materialization_preserves_fork_version_gate`
7. `materialization_preserves_child_local_precedence`
8. `materialization_preserves_child_owned_immutable_precedence`
9. `materialization_excludes_post_fork_source_rows`
10. `materialization_rewrites_rows_only_through_branch_runtime`

Assertions:

1. inherited-layer visibility and child-local shadowing do not change;
2. lifecycle never rewrites storage rows directly;
3. fork boundaries remain enforced by L6.

### 8. Materialization Durable Windows

Required tests:

1. `durable_materialization_publish_failure_leaves_inherited_layer_visible`
2. `durable_materialization_reopen_failure_leaves_inherited_layer_visible`
3. `durable_materialization_install_failure_after_publish_reports_health_debt`
4. `durable_materialization_install_failure_lists_published_objects`
5. `durable_materialization_without_table_manifest_reports_checkpoint_required`
6. `durable_materialization_does_not_advance_flush_watermark`
7. `durable_materialization_does_not_truncate_wal`
8. `cache_materialization_does_not_call_table_object_service`

Assertions:

1. inherited layer remains visible until replacement is installed;
2. partial durable output is not silently deleted;
3. recovery facts do not overclaim durability.

### 9. Storage Pressure Facts

Required tests:

1. `pressure_reports_none_for_empty_branch`
2. `pressure_reports_background_for_frozen_backlog`
3. `pressure_reports_background_for_l0_table_count`
4. `pressure_reports_urgent_for_large_l0_table_count`
5. `pressure_reports_inherited_layer_backlog`
6. `pressure_reports_materializing_layer_count`
7. `pressure_reports_pending_queue_depth`
8. `pressure_suggests_flush_before_compaction_when_frozen_backlog_exists`
9. `pressure_suggests_compaction_for_l0_backlog`
10. `pressure_suggests_materialization_for_inherited_layer`
11. `pressure_facts_do_not_contain_product_write_stall_wording`
12. `pressure_facts_are_deterministic_for_same_branch_state`

Assertions:

1. facts are storage vocabulary;
2. suggestions are deterministic;
3. pressure facts do not mutate state.

### 10. Maintenance Outcome Mapping

Required tests:

1. `compaction_completed_maps_to_completed_maintenance`
2. `compaction_no_candidate_maps_to_deferred_maintenance`
3. `compaction_failure_maps_to_failed_maintenance_with_health`
4. `materialization_completed_maps_to_completed_maintenance`
5. `materialization_already_done_maps_to_deferred_or_completed_by_documented_policy`
6. `materialization_failure_maps_to_failed_maintenance_with_health`
7. `published_not_installed_outcome_is_retryable`
8. `affected_object_count_reports_published_outputs`
9. `reclaimed_bytes_is_zero_until_retention_slice`
10. `stats_count_completed_deferred_and_failed_tasks`

Assertions:

1. outcome facts are structured enough for later retention/close slices;
2. replaced objects are not counted as reclaimed bytes.

### 11. Source Chains And Error Codes

Required tests:

1. `compaction_branch_error_preserves_source_chain`
2. `compaction_table_error_preserves_source_chain`
3. `compaction_publish_error_preserves_source_chain`
4. `materialization_branch_error_preserves_source_chain`
5. `materialization_publish_error_preserves_source_chain`
6. `maintenance_errors_assert_stable_code_not_display_text`
7. `invalid_request_errors_have_specific_lifecycle_code`
8. `published_not_installed_health_contains_telemetry_fault`

Assertions:

1. no test should assert on human display text;
2. lower-layer source type remains inspectable;
3. lifecycle error codes stay class-prefixed.

## Generated And Property Tests

Status for V1: deferred. Direct unit tests cover the shipped scheduling surface.
Generated compaction/materialization scripts remain valuable assurance-depth
work once table-manifest recovery and standalone rewrite publication are in
place.

Add a generated contract that decodes input-derived scripts into operations:

1. enqueue compaction task;
2. enqueue materialization task;
3. rotate/flush to create tables;
4. fork/attach inherited layer;
5. run next maintenance task;
6. inject no-candidate branch state;
7. inject stale-candidate branch state;
8. inject publish failure;
9. inject install failure;
10. collect pressure facts.

Counters must distinguish canonical setup from input-derived operations:

1. input-derived compaction requested;
2. input-derived no-candidate;
3. input-derived compaction installed;
4. input-derived materialization requested;
5. input-derived materialization installed;
6. input-derived retry/idempotent materialization;
7. input-derived publish failure;
8. input-derived install failure;
9. input-derived pressure fact;
10. input-derived cache-vs-durable route.

Generated assertions:

1. read parity holds before/after keep-all compaction;
2. read parity holds before/after materialization;
3. no source row above fork version becomes visible through materialization;
4. no generated operation deletes table objects;
5. no generated operation advances flush watermark or truncates WAL through this
   slice.

## Source Guards

Extend `lifecycle_source_guard.rs` to assert:

1. `lifecycle/compaction.rs` does not import product/engine modules;
2. no `std::fs`, `std::path::Path`, `File`, `OpenOptions`, or `std::env`;
3. no direct object layout literals such as `tables/`, `wal/`, or `snapshots/`;
4. no old primitive vocabulary such as `kv`, `json`, `vector`, `embedding`, or
   `graph` in lifecycle compaction code/tests;
5. no direct WAL truncation, checkpoint, retention, quarantine, purge, or repair
   calls from compaction/materialization handlers;
6. no code comments or test names contain architecture slice labels;
7. table merge behavior is reached through L6/L5 public crate-private surfaces,
   not copied into lifecycle.

## Sensitivity Probes

Record probe results in the porting log after implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| S1 | Bypass L6 candidate planning and choose all L0 tables in lifecycle. | Candidate routing/source guard test fails. |
| S2 | Drop tombstones during compaction. | Keep-all tombstone parity test fails. |
| S3 | Drop older versions during compaction. | Keep-all history parity test fails. |
| S4 | Install durable compaction output before publish validation. | Publication ordering test fails. |
| S5 | Ignore publish failure and mutate branch state. | Publish failure unchanged-read test fails. |
| S6 | Materialize from layer index after intent instead of handle. | Handle-binding test fails. |
| S7 | Remove inherited layer before replacement visibility. | Materialization failure visibility test fails. |
| S8 | Allow child-owned rows to lose precedence. | Child-local precedence test fails. |
| S9 | Advance flush watermark after compaction. | No-watermark test fails. |
| S10 | Delete replaced table refs in this slice. | Retention deferral/source guard test fails. |
| S11 | Emit product write-stall wording. | Pressure vocabulary test fails. |
| S12 | Collapse lower-layer source to display text. | Source-chain test fails. |

## Integration Smoke Tests

Add or extend `lifecycle_maintenance.rs`:

1. cache runtime can enqueue and run compaction maintenance;
2. cache runtime can enqueue and run materialization maintenance;
3. durable runtime can enqueue compaction and returns documented durable
   outcome;
4. durable runtime can enqueue materialization and returns documented durable
   outcome;
5. checkpoint after volatile rewrite can still recover rows;
6. queued compaction/materialization coalesces through the executor;
7. queued compaction/materialization skips unrelated pending tasks.

These should exercise implementation paths, not planning-document inventory.

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional if `cargo-hack` is installed:

```bash
cargo hack check -p strata-storage-next --feature-powerset --depth 2
```

## Close Criteria

L8K test coverage is complete when:

1. direct tests cover request validation, admission, compaction, materialization,
   pressure facts, and maintenance outcome mapping;
2. keep-all compaction read parity is proven for point, history, prefix, and
   range reads;
3. materialization read parity is proven with child-local shadowing and
   fork-version gates;
4. durable publication failure windows are explicitly tested or documented as
   deferred with a conservative outcome;
5. no test relies on doc-link inventory as proof of storage behavior;
6. generated counters prove input-derived compaction/materialization routes;
7. source guards prevent algorithm and vocabulary drift;
8. sensitivity probes are recorded in the porting log;
9. mandatory verification commands pass.
