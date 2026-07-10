# L8R Test Plan: Table Manifest Publication And Recovery

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that durable table manifests are published after durable table-object
installation, recovered before WAL finalization, and trusted only when every
manifest-listed table object validates.

The suite must fail if L8R:

1. publishes a table manifest before table objects are durable and validated;
2. records volatile table refs in a durable table manifest;
3. loads orphan table objects that are not listed by a trusted manifest;
4. treats corrupt table manifests as healthy;
5. treats missing table objects as corrupt manifests or vice versa;
6. accepts table object fact mismatches;
7. bypasses L6 validation during recovered table install;
8. advances flush watermarks or truncates WAL from table-manifest coverage;
9. lets cache mode publish or recover durable table manifests;
10. imports raw IO, product code, primitive DTOs, or StrataHub code.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. typed table-manifest service load/publish;
2. manifest publication after durable flush;
3. manifest publication uncertainty;
4. manifest-driven recovery;
5. table object validation before install;
6. checkpoint/table-manifest interaction preflight;
7. strict/lossy recovery classifications;
8. cache-mode absence;
9. source guards and generated counters.

Not covered:

1. table-manifest byte-format golden vectors, covered by L8Q;
2. table-object retention/quarantine, deferred to L8S/L8M;
3. table-manifest-backed flush watermark proof, deferred to L8T;
4. durable rewrite output publication, deferred to L8U;
5. branch list/delete/clear/fork-at-history completion, deferred to L8Y;
6. public L9 API behavior.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `crates/storage/src/segmented/tests/leveled.rs::recover_with_manifest_restores_levels` | Manifest recovery restores levels. | Recovered branch preserves L0 precedence and L1+ placement. |
| `crates/storage/src/segmented/tests/leveled.rs::recover_manifest_corrupt_returns_error` | Corrupt manifest is a recovery fault, not fallback to all files. | Corrupt table manifest fails/degrades and installs no orphan objects. |
| `crates/storage/src/segmented/tests/concurrency.rs::test_issue_1680_corrupt_manifest_rejects_orphan_loading` | Corrupt manifest must not load orphan table files. | Valid orphan object is ignored when manifest is corrupt. |
| `crates/storage/src/segmented/tests/lifecycle.rs::recovery_skips_orphan_sst_not_in_manifest` | Object not listed in manifest is not live. | Extra branch table object remains absent from recovered reads. |
| `crates/storage/src/segmented/tests/flush.rs::recover_missing_manifest_listed_produces_fault` | Missing listed table object becomes a recovery fault. | Strict recovery fails; lossy recovery reports data-loss health. |
| `crates/storage/src/segmented/tests/flush.rs::recover_corrupt_manifest_listed_segment_is_not_reported_missing` | Corrupt listed object keeps its corrupt-object classification. | Table reader/source error is preserved. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | Manifest publish failure after table object publish is partial progress. | Outcome records table object facts plus table-manifest publication debt. |

Tests must not port:

1. raw filesystem path mutation;
2. old filename-only manifest entries;
3. direct directory listing as recovery truth;
4. logs-only health assertions;
5. product branch commands;
6. retention deletion.

## Test Locations

Use:

1. `crates/storage-next/src/service/manifest.rs` for focused table-manifest
   service tests.
2. `crates/storage-next/src/lifecycle/tests/recovery.rs` or
   `crates/storage-next/src/lifecycle/tests/table_manifest_recovery.rs` for
   direct recovery tests.
3. `crates/storage-next/src/lifecycle/tests/flush.rs` for flush publication
   integration.
4. `crates/storage-next/src/branch/tests/` for L6 table-manifest install tests
   if a new L6 install surface is added.
5. `crates/storage-next/src/testkit/lifecycle/recovery.rs` for generated
   recovery counters.
6. `crates/storage-next/tests/lifecycle_recovery.rs` for durable local
   integration smoke.
7. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
8. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split direct tests into `tests/table_manifest_recovery/` if a single file
approaches 1,000 lines.

## Test Data Principles

1. Build table objects through L5/L4 helpers, not arbitrary bytes, unless the
   test explicitly corrupts them.
2. Include at least one branch with L0 and L1+ tables.
3. Include an inherited layer with fork-version-gated tables.
4. Include a materializing inherited layer.
5. Include an extra valid table object not listed in the manifest.
6. Include table object fact mismatch mutations for byte count, row count,
   commit min/max, data block count, identity, bounds, and object name.
7. Include checkpoint rows and table-manifest rows in the same recovery fixture
   when testing duplicate/conflict behavior.
8. Keep cache-mode tests free of durable object creation.

## Direct Unit Tests

### 1. Table Manifest Service

Required tests:

1. `table_manifest_service_load_absent_returns_none`
2. `table_manifest_service_load_decodes_present_manifest`
3. `table_manifest_service_load_rejects_corrupt_manifest`
4. `table_manifest_service_load_rejects_branch_object_payload_mismatch`
5. `table_manifest_service_publish_create_writes_canonical_bytes`
6. `table_manifest_service_publish_replace_writes_canonical_bytes`
7. `table_manifest_service_publish_validates_publish_outcome`
8. `table_manifest_service_publish_rejects_invalid_manifest`
9. `table_manifest_service_publish_preserves_source_error`
10. `table_manifest_service_does_not_accept_database_manifest_bytes`

Assertions:

1. service returns typed role-specific errors;
2. publish validates durable outcome metadata;
3. branch id in object name and payload must agree.

### 2. Durable Table Catalog

Required tests:

1. `durable_table_catalog_accepts_exact_duplicate`
2. `durable_table_catalog_rejects_identity_with_different_object`
3. `durable_table_catalog_rejects_identity_with_different_facts`
4. `durable_table_catalog_rejects_object_with_different_identity`
5. `durable_table_catalog_builds_manifest_from_reachable_refs`
6. `durable_table_catalog_omits_unreachable_refs`
7. `durable_table_catalog_rejects_volatile_refs`
8. `durable_table_catalog_rebuilds_from_recovered_manifest`

Assertions:

1. catalog is a construction aid, not durable truth;
2. ambiguity blocks manifest publication;
3. volatile table refs do not leak into durable manifests.

### 3. Flush Publication Integration

Required tests:

1. `durable_flush_publishes_table_manifest_after_table_install`
2. `durable_flush_manifest_includes_new_table_object`
3. `durable_flush_manifest_preserves_existing_reachable_tables`
4. `durable_flush_manifest_preserves_inherited_layers`
5. `durable_flush_manifest_publish_failure_keeps_rows_visible`
6. `durable_flush_manifest_publish_failure_reports_health_debt`
7. `durable_flush_manifest_publish_uncertain_reports_uncertainty`
8. `durable_flush_does_not_advance_flush_watermark_from_table_manifest`
9. `durable_flush_does_not_truncate_wal_from_table_manifest`
10. `cache_flush_does_not_publish_table_manifest`

Assertions:

1. manifest publication comes after table object validation and L6 install;
2. manifest failure is partial progress, not clean success;
3. table manifests are not yet WAL retention proof.

### 4. Recovery From Table Manifests

Required tests:

1. `recovery_loads_table_manifest_for_branch`
2. `recovery_installs_manifest_owned_l0_tables`
3. `recovery_installs_manifest_owned_l1_tables`
4. `recovery_preserves_l0_precedence_after_reopen`
5. `recovery_preserves_l1_plus_order_after_reopen`
6. `recovery_installs_inherited_layers_from_manifest`
7. `recovery_preserves_materializing_layer_status`
8. `recovery_rebuilds_durable_table_catalog_from_manifest`
9. `recovery_reports_table_manifest_object_name`
10. `recovery_reports_table_manifest_table_count`

Assertions:

1. L6 install/rebuild surface validates recovered state;
2. read results match the explicit manifest fixture;
3. recovery facts name manifest and table objects.

### 5. Missing And Corrupt Manifest Cases

Required tests:

1. `strict_recovery_rejects_corrupt_table_manifest`
2. `lossy_recovery_reports_corrupt_table_manifest_data_loss`
3. `strict_recovery_rejects_future_table_manifest`
4. `lossy_recovery_reports_future_table_manifest_policy_downgrade`
5. `strict_recovery_rejects_expected_missing_table_manifest`
6. `lossy_recovery_reports_expected_missing_table_manifest`
7. `missing_table_manifest_for_empty_branch_is_healthy`
8. `missing_table_manifest_does_not_load_table_objects_by_prefix`
9. `corrupt_table_manifest_does_not_load_orphan_object`
10. `table_manifest_decode_error_preserves_format_source`

Assertions:

1. missing and corrupt are distinct;
2. lossy recovery is explicit and degraded;
3. no fallback loads every table object as L0.

### 6. Table Object Validation Cases

Required tests:

1. `strict_recovery_rejects_missing_manifest_listed_table_object`
2. `lossy_recovery_reports_missing_manifest_listed_table_object`
3. `strict_recovery_rejects_corrupt_manifest_listed_table_object`
4. `corrupt_manifest_listed_table_object_is_not_reported_missing`
5. `recovery_rejects_table_object_byte_count_mismatch`
6. `recovery_rejects_table_object_row_count_mismatch`
7. `recovery_rejects_table_object_data_block_count_mismatch`
8. `recovery_rejects_table_object_commit_min_mismatch`
9. `recovery_rejects_table_object_commit_max_mismatch`
10. `recovery_rejects_table_identity_mismatch`
11. `recovery_rejects_table_bounds_mismatch`
12. `recovery_rejects_table_object_from_wrong_branch_namespace`

Assertions:

1. table objects are validated before L6 install;
2. object fact mismatches produce typed recovery mismatch errors;
3. lower-layer source chains survive.

### 7. Orphan And Ambiguous Objects

Required tests:

1. `recovery_ignores_valid_orphan_table_object_not_in_manifest`
2. `recovery_ignores_corrupt_orphan_table_object_not_in_manifest`
3. `recovery_reports_orphan_count_for_future_retention`
4. `recovery_rejects_manifest_duplicate_table_identity`
5. `recovery_rejects_manifest_duplicate_object_name`
6. `recovery_rejects_catalog_identity_collision`
7. `recovery_rejects_catalog_object_collision`
8. `recovery_does_not_quarantine_or_delete_orphans`

Assertions:

1. only manifest-listed tables are live;
2. orphan state is reported for later slices, not mutated here;
3. ambiguity blocks recovery or manifest publication.

### 8. Checkpoint And WAL Interaction

Required tests:

1. `recovery_preflights_checkpoint_and_table_manifest_together`
2. `recovery_rejects_checkpoint_table_manifest_duplicate_internal_key_conflict`
3. `recovery_accepts_exact_duplicate_checkpoint_table_manifest_rows_only_via_l6_idempotence`
4. `table_manifest_recovery_does_not_change_wal_replay_start`
5. `table_manifest_recovery_does_not_truncate_wal`
6. `table_manifest_recovery_does_not_persist_flush_watermark`
7. `wal_replay_after_table_manifest_recovery_remains_idempotent`
8. `table_manifest_recovery_then_wal_tail_preserves_latest_reads`

Assertions:

1. L8R does not implement L8T;
2. duplicate/conflict handling is explicit;
3. WAL replay remains conservative.

### 9. Cache Mode

Required tests:

1. `cache_mode_table_manifest_service_absent`
2. `cache_open_does_not_load_table_manifest`
3. `cache_flush_does_not_publish_table_manifest`
4. `cache_recovery_request_rejects_table_manifest_inputs`
5. `cache_mode_reports_table_manifest_unsupported_without_durable_claim`

Assertions:

1. cache mode creates no durable table-manifest objects;
2. unsupported durable table-manifest work is deferred/rejected explicitly;
3. no cache outcome claims crash-durable table reachability.

### 10. Publication Fault Windows

Required tests:

1. `table_manifest_publish_failure_before_write_preserves_old_manifest`
2. `table_manifest_publish_failure_after_table_install_reports_partial_progress`
3. `table_manifest_publish_uncertain_after_replace_reports_uncertainty`
4. `table_manifest_publish_retry_reuses_same_canonical_bytes`
5. `table_manifest_publish_retry_after_old_manifest_keeps_rows_recoverable_from_wal`
6. `table_manifest_publish_wrong_branch_payload_rejected_before_publish`
7. `table_manifest_publish_ambiguous_catalog_rejected_before_publish`
8. `table_manifest_publish_failure_preserves_source_chain`

Assertions:

1. publication errors are phase-specific;
2. retries are deterministic;
3. no wrong-branch manifest is published.

## Generated And Property Tests

Add table-manifest recovery operations to lifecycle generated scripts:

```text
publish_table_object(branch, table)
publish_table_manifest(branch, graph)
corrupt_table_manifest(branch)
delete_table_object(object)
add_orphan_table_object(branch)
recover(strictness)
assert_model_reads()
```

Required counters:

1. table_manifest_published;
2. table_manifest_recovered;
3. table_manifest_corrupt;
4. table_manifest_missing;
5. table_object_missing;
6. table_object_corrupt;
7. table_object_mismatch;
8. orphan_ignored;
9. checkpoint_manifest_conflict;
10. cache_manifest_unsupported.

Properties:

1. recovered reads equal model reads for manifest-listed valid tables;
2. orphan objects never become visible;
3. corrupt manifest prevents trusting table objects;
4. table-manifest recovery does not shorten WAL replay;
5. cache mode never creates durable table-manifest objects.

## Source Guards

Required source guard tests:

1. `table_manifest_recovery_does_not_import_raw_io`
2. `table_manifest_recovery_does_not_import_engine_or_product_crates`
3. `table_manifest_recovery_does_not_import_stratahub`
4. `table_manifest_recovery_does_not_import_primitive_modules`
5. `table_manifest_recovery_does_not_list_table_prefix_for_reachability`
6. `table_manifest_publication_does_not_touch_wal_truncation`
7. `cache_mode_does_not_import_table_manifest_service`
8. `l4_manifest_service_does_not_import_lifecycle`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `read_dir`
4. `list_prefix`
5. `crate::lifecycle` inside L4 service/format modules
6. `strata_engine`
7. `stratahub`
8. `primitive`
9. `merge`
10. `cherry`
11. `revert`

Use focused file scopes so tests do not flag this planning document.

## Fault And Crash Windows

Required phase tests:

1. crash after table object publish before table manifest publish;
2. crash after table manifest publish before database manifest update;
3. crash after table manifest replace with old checkpoint watermark;
4. crash after manifest publication uncertainty;
5. crash with orphan table object and no manifest reference;
6. crash with corrupt manifest and valid orphan object;
7. crash with missing listed object under strict mode;
8. crash with missing listed object under lossy mode.

Process-level crash tests may be slower, but every phase must have a
non-ignored unit or integration equivalent.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| R1 | Publish table manifest before table object validation. | Flush publication ordering test fails. |
| R2 | Include volatile table refs in durable manifest. | Catalog validation test fails. |
| R3 | On corrupt manifest, list table prefix and install objects as L0. | Corrupt-manifest orphan test fails. |
| R4 | Treat missing listed object as corrupt manifest. | Missing/corrupt classification test fails. |
| R5 | Ignore table object row-count mismatch. | Fact mismatch test fails. |
| R6 | Advance flush watermark after manifest recovery. | WAL interaction test fails. |
| R7 | Let cache mode publish a table manifest. | Cache absence test fails. |
| R8 | Bypass L6 recovered-table validation. | Duplicate/conflict preflight test fails. |
| R9 | Drop lower-layer source chain on table reader error. | Source-chain test fails. |
| R10 | Import raw IO into recovery manifest path. | Source guard fails. |

## Command Matrix

Mandatory commands before L8R closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table_manifest
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8R test coverage is complete when:

1. table-manifest service tests cover load, create, replace, corrupt, mismatch,
   and publish uncertainty;
2. durable flush publishes table manifests after table object install;
3. recovery restores manifest-listed owned and inherited tables;
4. corrupt manifests and orphan objects cannot become live state;
5. missing/corrupt/mismatched listed table objects classify distinctly;
6. table-manifest recovery does not advance flush watermarks or truncate WAL;
7. cache mode has explicit durable-table-manifest absence;
8. generated counters cover publication and recovery categories;
9. source guards enforce boundaries;
10. sensitivity probes and command results are recorded in the porting log.
