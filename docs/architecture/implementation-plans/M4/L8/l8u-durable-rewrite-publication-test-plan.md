# L8U Test Plan: Durable Rewrite Publication

Status: implemented

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that durable compaction and materialization publish output table objects,
validate them, install through L6, publish updated branch table manifests, and
preserve all partial-progress facts needed for recovery and later retention.

The suite must fail if L8U:

1. installs rewrite outputs before durable table-object publication;
2. skips reopen/fact validation before L6 install;
3. publishes branch table manifest before L6 install;
4. drops old versions, tombstones, timestamps, or TTL-expired rows without L8V
   proof;
5. deletes or quarantines replaced input objects;
6. deletes published-but-not-installed output objects;
7. targets materialization by stale layer index instead of handle/source facts;
8. treats manifest publish failure after install as clean success;
9. advances flush watermark or truncates WAL directly;
10. imports raw IO, product code, primitive DTOs, or deletion APIs.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. durable compaction output publication;
2. durable materialization output publication;
3. publish/reopen/install/manifest fault windows;
4. branch table-manifest update after rewrite install;
5. read parity under keep-all rewrites;
6. stale candidate and materialization handle retries;
7. orphan/replaced object facts for L8S/L8M;
8. cache-mode volatile behavior;
9. generated counters and source guards.

Not covered:

1. table-manifest byte format, covered by L8Q;
2. table-manifest recovery basics, covered by L8R;
3. table-object retention/quarantine/purge, covered by L8S/L8M;
4. WAL truncation and flush watermark persistence, covered by L8T;
5. row pruning, covered by L8V;
6. memory-budget admission, covered by L8W;
7. lazy reads, covered by L8X.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `segmented/compaction.rs::compact_l0_to_l1` | Output is built, installed atomically, and manifest is updated. | Output object publish/reopen precedes L6 install; manifest update follows install. |
| `segmented/compaction.rs::compact_level` | Visible reads are unchanged after compaction. | Durable compaction preserves latest/history/range/tombstone reads. |
| `segmented/mod.rs::materialize_layer` | Materialization preserves child-local precedence and removes inherited layer only after replacement is visible. | Replacement object publish/reopen precedes L6 materialization install; reads match before/after. |
| `publish_failures.rs` | Failure windows preserve either old visible state or explicit forward-progress debt. | Every publish/reopen/install/manifest failure has typed outcome facts. |
| `resurrection.rs` | Stale rewrite cannot resurrect deleted/cleared state. | Candidate stale after publication fails closed and preserves current reads. |
| `gc_orphan_segments` | Orphan outputs are retained for later safe reclaim. | Published-but-not-installed outputs are named and not deleted. |

Tests must not port:

1. direct filesystem mutation;
2. old public compaction commands;
3. logs-only assertions;
4. row pruning expectations;
5. product write-stall behavior;
6. background-thread timing.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/compaction.rs` or
   `crates/storage-next/src/lifecycle/tests/rewrite.rs` for direct durable
   rewrite tests.
2. `crates/storage-next/src/lifecycle/tests/compaction/` if direct tests exceed
   1,000 lines.
3. `crates/storage-next/src/branch/tests/owned_compaction.rs` and
   `crates/storage-next/src/branch/tests/inheritance_materialization/` for
   lower-layer evidence only, not lifecycle substitutes.
4. `crates/storage-next/src/testkit/lifecycle/compaction.rs` for generated
   rewrite scripts.
5. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration smoke.
6. `crates/storage-next/tests/lifecycle_source_guard.rs` for boundary tests.
7. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

## Test Data Principles

1. Build branch state through L6/L7 helpers where possible.
2. Include overlapping L0 tables and L1+ tables.
3. Include tombstones, older versions, TTL-expired rows, and timeline rows.
4. Include at least one inherited layer with child-local shadowing.
5. Include materializing-layer retry fixtures.
6. Include fault seams after output publish and before L6 install.
7. Include manifest publish failure after L6 install.
8. Assert stable error codes/source chains, not display text.

## Direct Unit Tests

### 1. Request And Admission

Required tests:

1. `durable_rewrite_rejects_cache_durable_publication_request`
2. `durable_rewrite_rejects_before_open`
3. `durable_rewrite_rejects_while_closing`
4. `durable_rewrite_rejects_empty_output_seed`
5. `durable_rewrite_rejects_path_like_output_seed`
6. `durable_rewrite_rejects_pruning_policy_without_retention_proof`
7. `durable_rewrite_uses_ordinary_maintenance_admission`
8. `durable_rewrite_releases_admission_after_publish_failure`

Assertions:

1. invalid requests fail before table-object publication;
2. no close/admission leak remains after failure;
3. cache mode cannot claim durable rewrite publication.

### 2. Durable Compaction Publication

Required tests:

1. `durable_compaction_publishes_output_before_install`
2. `durable_compaction_reopens_output_before_install`
3. `durable_compaction_validates_output_facts_before_install`
4. `durable_compaction_installs_only_after_all_outputs_validate`
5. `durable_compaction_publishes_manifest_after_install`
6. `durable_compaction_manifest_includes_outputs`
7. `durable_compaction_manifest_excludes_replaced_inputs`
8. `durable_compaction_catalog_marks_replaced_inputs_retained`
9. `durable_compaction_output_identities_are_retry_stable`
10. `durable_compaction_no_candidate_is_deferred`

Assertions:

1. publication order is table object -> reopen -> L6 install -> manifest;
2. replaced inputs are retained, not deleted;
3. manifest represents post-install L6 reachability.

### 3. Durable Materialization Publication

Required tests:

1. `durable_materialization_binds_handle_before_output_publish`
2. `durable_materialization_publishes_replacement_before_layer_removal`
3. `durable_materialization_reopens_replacement_before_install`
4. `durable_materialization_validates_replacement_facts`
5. `durable_materialization_publishes_manifest_after_layer_removal`
6. `durable_materialization_manifest_removes_inherited_layer`
7. `durable_materialization_manifest_includes_replacements`
8. `durable_materialization_preserves_child_local_precedence`
9. `durable_materialization_retry_after_removed_layer_uses_source_identity`
10. `durable_materialization_rejects_stale_layer_index_task`

Assertions:

1. handle/source identity survives queued-task reindexing;
2. replacement visibility is atomic through L6;
3. manifest update follows L6 materialization outcome.

### 4. Read Parity

Required tests:

1. `durable_compaction_preserves_latest_reads`
2. `durable_compaction_preserves_history_reads`
3. `durable_compaction_preserves_prefix_scans`
4. `durable_compaction_preserves_range_scans`
5. `durable_compaction_preserves_tombstones`
6. `durable_compaction_preserves_ttl_expired_rows_under_keep_all`
7. `durable_compaction_preserves_commit_timestamps`
8. `durable_materialization_preserves_latest_reads`
9. `durable_materialization_preserves_history_reads`
10. `durable_materialization_preserves_fork_version_gate`

Assertions:

1. keep-all rewrite is observationally equivalent;
2. no row pruning occurs;
3. materialization does not lose child-local shadowing.

### 5. Fault Windows

Required tests:

1. `rewrite_output_publish_failure_leaves_reads_unchanged`
2. `rewrite_output_publish_uncertain_reports_health_debt`
3. `rewrite_output_reopen_failure_leaves_reads_unchanged`
4. `rewrite_output_fact_mismatch_leaves_reads_unchanged`
5. `rewrite_install_failure_after_publish_names_orphan_outputs`
6. `rewrite_install_failure_after_publish_does_not_delete_outputs`
7. `rewrite_manifest_publish_failure_after_install_keeps_new_reads_visible`
8. `rewrite_manifest_publish_failure_after_install_reports_manifest_debt`
9. `rewrite_manifest_publish_uncertain_after_install_reports_uncertainty`
10. `rewrite_retry_after_manifest_failure_reuses_catalog_entries`
11. `rewrite_retry_after_output_publish_collision_rejects_conflict`
12. `rewrite_stale_candidate_after_publish_fails_without_resurrection`

Assertions:

1. old state remains visible until L6 install succeeds;
2. new state remains visible after successful install even if manifest publish
   fails;
3. every orphan/partial output object is named for L8S/L8M.

### 6. Recovery Interaction

Required tests:

1. `recovery_after_durable_compaction_uses_manifest_outputs`
2. `recovery_after_durable_materialization_uses_manifest_replacements`
3. `recovery_after_manifest_publish_failure_uses_previous_manifest_or_wal`
4. `recovery_after_output_publish_before_install_ignores_orphan_output`
5. `recovery_after_install_before_manifest_records_health_debt`
6. `recovery_rejects_corrupt_rewrite_output_listed_by_manifest`
7. `recovery_rejects_missing_rewrite_output_listed_by_manifest`
8. `recovery_preserves_reads_after_wal_tail_replay`

Assertions:

1. table manifests make completed rewrites recoverable;
2. orphan outputs are not live;
3. manifest debt is visible after reopen.

### 7. Watermark Boundary

Required tests:

1. `durable_rewrite_completion_does_not_directly_persist_flush_watermark`
2. `durable_rewrite_completion_does_not_directly_truncate_wal`
3. `durable_rewrite_manifest_facts_can_build_flush_coverage_candidate`
4. `durable_rewrite_manifest_failure_cannot_build_flush_coverage_candidate`
5. `durable_rewrite_checkpoint_debt_reduced_only_after_manifest_success`

Assertions:

1. L8U does not implement L8T;
2. coverage facts are available only after manifest success;
3. WAL deletion remains delegated to L8T/L4.

### 8. No Deletion Or Pruning

Required tests:

1. `durable_rewrite_does_not_delete_replaced_inputs`
2. `durable_rewrite_does_not_quarantine_replaced_inputs`
3. `durable_rewrite_does_not_delete_published_orphan_outputs`
4. `durable_rewrite_does_not_prune_old_versions`
5. `durable_rewrite_does_not_prune_tombstones`
6. `durable_rewrite_does_not_prune_ttl_expired_rows`
7. `durable_rewrite_does_not_call_quarantine_service`
8. `durable_rewrite_does_not_call_purge`

Assertions:

1. L8S/L8M own reclaim;
2. L8V owns pruning;
3. rewrite publication is durability, not cleanup.

## Generated And Property Tests

Add durable rewrite operations:

```text
build_branch_tables(branch)
enqueue_compaction(branch)
enqueue_materialization(branch, source)
inject_fault(phase)
run_rewrite()
recover()
assert_model_reads()
assert_object_facts()
```

Required counters:

1. compaction_output_published;
2. materialization_output_published;
3. output_reopened;
4. install_after_publish;
5. manifest_after_install;
6. publish_failed_before_install;
7. install_failed_after_publish;
8. manifest_failed_after_install;
9. orphan_output_recorded;
10. no_pruning_observed;

Properties:

1. accepted durable rewrites preserve model reads;
2. failed pre-install rewrites preserve old reads;
3. failed post-install manifest updates preserve new reads and record debt;
4. orphan outputs never become live without manifest;
5. no generated route deletes replaced inputs or prunes rows.

## Source Guards

Required source guard tests:

1. `durable_rewrite_publication_does_not_import_raw_io`
2. `durable_rewrite_publication_does_not_import_backend_delete`
3. `durable_rewrite_publication_does_not_import_quarantine_mutation`
4. `durable_rewrite_publication_does_not_import_purge`
5. `durable_rewrite_publication_does_not_import_row_pruning_policy`
6. `durable_rewrite_publication_does_not_import_engine_or_product_crates`
7. `durable_rewrite_publication_does_not_import_stratahub`
8. `durable_rewrite_publication_does_not_import_primitive_modules`
9. `cache_rewrite_path_does_not_import_table_object_publication`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `delete_object`
4. `quarantine_object`
5. `purge_quarantine`
6. `prune`
7. `retention_report`
8. `strata_engine`
9. `stratahub`
10. `primitive`
11. `graph`
12. `vector`
13. `json`

Scope scans to production rewrite-publication modules to avoid false positives
from planning docs and lower-layer pruning names.

## Fault And Crash Windows

Required phase tests:

1. crash after first output table object publish before all outputs publish;
2. crash after all output publish before L6 install;
3. crash after L6 install before table-manifest publish;
4. crash after table-manifest publish before database flush watermark update;
5. crash with stale candidate after output publish;
6. crash during materialization after replacement publish before layer removal;
7. crash during materialization after layer removal before manifest publish;
8. crash after manifest publish uncertainty.

Every phase must have a non-ignored unit or integration equivalent even if
process-level crash tests are marked ignored.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| U1 | Install before output publish. | Publication-order test fails. |
| U2 | Skip output reopen validation. | Fact-mismatch test fails. |
| U3 | Publish manifest before L6 install. | Manifest-order test fails. |
| U4 | Delete replaced inputs. | No-deletion test fails. |
| U5 | Prune tombstones during rewrite. | Read parity/no-pruning test fails. |
| U6 | Use layer index after materialization handle bind. | Stale-layer test fails. |
| U7 | Treat manifest publish failure as clean success. | Manifest-debt test fails. |
| U8 | Let orphan output become live after recovery. | Recovery orphan test fails. |
| U9 | Truncate WAL directly from rewrite completion. | Watermark boundary test fails. |
| U10 | Import raw IO in rewrite publication. | Source guard fails. |

## Command Matrix

Mandatory commands before L8U closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8U test coverage is complete when:

1. durable compaction and materialization publish/reopen outputs before install;
2. branch table manifest publication happens after install;
3. every partial-progress window has typed outcome and health facts;
4. recovery after completed rewrite uses table-manifest outputs;
5. orphan outputs and replaced inputs are retained for later proof;
6. no deletion, quarantine, purge, pruning, flush-watermark, or WAL truncation
   occurs in this slice;
7. generated properties cover both compaction and materialization;
8. source guards enforce boundaries;
9. sensitivity probes and command results are recorded in the porting log.
