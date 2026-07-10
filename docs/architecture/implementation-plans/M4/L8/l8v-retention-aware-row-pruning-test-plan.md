# L8V Test Plan: Retention-Aware Row Pruning

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

## Goal

Prove that row pruning reclaims old MVCC history only when storage has proof
that no retained read, timestamp query, active view, branch inheritance edge, or
recovery path can observe the dropped rows.

The suite must fail if L8V:

1. prunes rows without an explicit proof;
2. drops a row needed by `getv`, history, or `as_of`;
3. drops tombstones that still shadow lower or inherited values;
4. drops TTL-expired rows that remain visible to a retained timestamp;
5. ignores active or pinned read views;
6. ignores child branches or inherited layers;
7. narrows history without recording a typed coverage boundary;
8. lets recovery silently misread pruned history;
9. deletes table objects, truncates WAL, or quarantines data;
10. imports raw IO, product policy, primitive DTOs, or milestone labels.

Do not add tests whose only assertion is that planning documents exist or link
to other planning documents.

## Coverage Boundary

Covered:

1. row-pruning proof validation;
2. old-version pruning;
3. tombstone elision;
4. TTL-expired row elision;
5. max-version-per-key pruning with pinned-view protection;
6. inherited-layer and child-local safety;
7. timestamp/as-of coverage boundaries;
8. durable rewrite/manifest coverage after pruning;
9. generated/model assurance;
10. source guards.

Not covered:

1. table-object deletion/quarantine, covered by L8S/L8M;
2. table-manifest publication mechanics, covered by L8R/L8U;
3. WAL truncation, covered by L8T;
4. memory-budgeted pruning scans, covered by L8W;
5. lazy reads during pruning, covered by L8X;
6. public retention API, covered by L9.

## Old-Code Regression Sources

| Old source | Behavior to preserve | Storage-next assertion |
|---|---|---|
| `storage/src/compaction.rs::CompactionIterator` | Keep all rows above floor and one below-floor survivor per key. | Version pruning keeps retained rows and the floor survivor. |
| `CompactionIterator::with_snapshot_floor` | Active snapshots protect rows from max-version pruning. | Pinned read views block pruning below their floor. |
| `CompactionIterator::with_is_bottommost` | Below-floor tombstones survive non-bottommost compaction. | Tombstones survive when lower owned or inherited values can be resurrected. |
| `CompactionIterator::with_drop_expired` | Expired TTL rows drop only in bottommost compaction and below floor. | TTL pruning requires timestamp and inheritance proof. |
| Old tombstone issue tests | Tombstones do not count against version caps when required for shadowing. | Max-version policy cannot evict required tombstones. |
| `ttl.rs::TTLIndex` | TTL cleanup can find expired keys efficiently. | L8V may identify TTL candidates, but tests assert no unbounded global index is required. |

Tests must not port:

1. raw segment path handling;
2. public engine retention configuration;
3. background pruning threads;
4. object deletion side effects;
5. product-specific retention language.

## Test Locations

Use:

1. `crates/storage-next/src/table/tests/compaction.rs` for L5 policy adapter
   tests.
2. `crates/storage-next/src/branch/tests/owned_compaction.rs` for L6 proof
   acceptance and read parity.
3. `crates/storage-next/src/branch/tests/inheritance_materialization/` for
   inherited-layer pruning safety.
4. `crates/storage-next/src/lifecycle/tests/compaction.rs` or
   `crates/storage-next/src/lifecycle/tests/pruning.rs` for L8 proof/outcome
   tests.
5. `crates/storage-next/src/testkit/lifecycle/compaction.rs` for generated
   scripts.
6. `crates/storage-next/tests/lifecycle_maintenance.rs` for integration smoke.
7. `crates/storage-next/tests/lifecycle_source_guard.rs` for source guards.
8. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   verification and sensitivity-probe records after implementation.

Split direct tests into submodules once any file approaches 1,000 lines.

## Test Data Principles

1. Use multiple versions of the same logical key.
2. Include rows above, equal to, and below the retained version floor.
3. Include commit timestamps that are not identical to version order.
4. Include tombstones above and below the retained floor.
5. Include lower-level values shadowed by tombstones.
6. Include inherited parent rows and child-local tombstones.
7. Include TTL rows expired before and after the retained timestamp floor.
8. Include active/pinned read views.
9. Include durable table manifests and recovery after pruning.
10. Assert reads and typed insufficient-history errors, not just row counts.

## Direct Unit Tests

### 1. Proof Validation

Required tests:

1. `row_pruning_request_without_proof_rejects`
2. `row_pruning_proof_branch_mismatch_rejects`
3. `row_pruning_proof_stale_epoch_rejects`
4. `row_pruning_proof_degraded_recovery_rejects`
5. `row_pruning_proof_retained_floor_above_visible_rejects`
6. `row_pruning_proof_timestamp_floor_without_coverage_rejects`
7. `row_pruning_proof_active_view_below_floor_rejects`
8. `row_pruning_proof_pinned_view_below_floor_rejects`
9. `row_pruning_proof_inherited_layer_unknown_rejects`
10. `row_pruning_proof_cache_mode_cannot_claim_durable_coverage`
11. `row_pruning_proof_zero_floor_keeps_all`
12. `row_pruning_proof_is_deterministic_for_shuffled_facts`

Assertions:

1. missing/unknown facts fail closed;
2. proof is bound to current branch and health epochs;
3. no output table is built on proof failure.

### 2. Old-Version Pruning

Required tests:

1. `version_pruning_keeps_all_versions_at_or_above_floor`
2. `version_pruning_keeps_newest_below_floor_survivor`
3. `version_pruning_drops_older_below_floor_versions`
4. `version_pruning_floor_zero_keeps_all`
5. `version_pruning_preserves_latest_read`
6. `version_pruning_preserves_getv_within_floor`
7. `version_pruning_history_reports_retained_boundary`
8. `version_pruning_as_of_below_floor_returns_insufficient_history`
9. `version_pruning_non_monotone_timestamps_respects_timestamp_floor`
10. `version_pruning_reports_drop_summary`

Assertions:

1. latest reads remain unchanged;
2. retained history works;
3. pruned history fails with typed boundary, not silent absence.

### 3. Max Versions Per Key

Required tests:

1. `max_versions_keeps_newest_n_versions`
2. `max_versions_zero_means_unbounded`
3. `max_versions_does_not_drop_versions_above_pinned_floor`
4. `max_versions_does_not_drop_versions_needed_by_as_of`
5. `max_versions_counts_values_but_not_required_tombstones`
6. `max_versions_with_floor_keeps_floor_survivor`
7. `max_versions_reports_older_version_drop_reason`

Assertions:

1. max-version pruning is subordinate to safety floors;
2. required tombstones do not displace retained values.

### 4. Tombstone Elision

Required tests:

1. `tombstone_pruning_rejects_without_elision_proof`
2. `bottommost_tombstone_below_floor_can_be_elided`
3. `non_bottommost_tombstone_below_floor_is_kept`
4. `tombstone_above_floor_is_kept`
5. `tombstone_needed_to_shadow_lower_owned_value_is_kept`
6. `tombstone_needed_to_shadow_inherited_value_is_kept`
7. `child_local_tombstone_hiding_parent_value_is_kept`
8. `materialized_replacement_tombstone_safety_is_checked`
9. `tombstone_elision_does_not_resurrect_deleted_key`
10. `tombstone_elision_reports_drop_summary`

Assertions:

1. tombstone elision requires bottommost owned and inherited safety;
2. deleted keys stay deleted after compaction and recovery.

### 5. TTL Elision

Required tests:

1. `ttl_pruning_rejects_without_ttl_proof`
2. `expired_ttl_below_floor_can_be_elided`
3. `expired_ttl_above_version_floor_is_kept`
4. `expired_ttl_needed_by_as_of_timestamp_is_kept`
5. `non_expired_ttl_row_is_kept`
6. `ttl_pruning_uses_supplied_cutoff_not_wall_clock`
7. `ttl_pruning_across_inherited_parent_child_keeps_required_parent_row`
8. `ttl_pruning_preserves_child_newer_override`
9. `ttl_pruning_reports_expired_drop_summary`
10. `ttl_pruning_does_not_create_global_unbounded_index`

Assertions:

1. TTL pruning is deterministic and proof-bound;
2. wall-clock time is not read by pruning policy;
3. inherited TTL semantics match L6 reads.

### 6. Inheritance And Materialization Safety

Required tests:

1. `inherited_parent_row_visible_to_child_blocks_parent_pruning`
2. `child_tombstone_shadowing_parent_blocks_tombstone_pruning`
3. `materialized_layer_replacement_preserves_pruned_history_boundary`
4. `materialization_with_pruning_preserves_child_local_precedence`
5. `forked_child_with_lower_fork_version_blocks_parent_floor`
6. `shared_table_identity_reachability_blocks_pruning`
7. `pruning_rejects_unknown_descendant_branch_facts`
8. `pruning_after_materialization_uses_source_identity_not_layer_index`
9. `pruning_does_not_drop_rows_above_child_fork_gate`
10. `pruning_model_matches_production_for_chained_inheritance`

Assertions:

1. child branches never lose inherited rows they can still read;
2. tombstone pruning never resurrects inherited rows;
3. materialization provenance remains recoverable.

### 7. Durable Rewrite And Recovery

Required tests:

1. `durable_pruned_compaction_publishes_pruned_manifest_facts`
2. `durable_pruned_compaction_recovery_restores_retained_reads`
3. `durable_pruned_compaction_recovery_rejects_pruned_history`
4. `durable_pruned_materialization_recovery_preserves_retained_reads`
5. `manifest_records_retained_version_floor`
6. `manifest_records_retained_timestamp_floor`
7. `manifest_missing_pruning_facts_rejects_recovery`
8. `wal_tail_replay_after_pruned_manifest_preserves_newer_rows`
9. `checkpoint_after_pruning_preserves_coverage_boundary`
10. `cache_pruning_reports_volatile_coverage_only`

Assertions:

1. durable table manifests are the source of retained-history coverage after
   reopen;
2. recovery does not silently widen history after pruning.

### 8. No Object Cleanup Or WAL Truncation

Required tests:

1. `row_pruning_does_not_delete_table_objects`
2. `row_pruning_does_not_quarantine_table_objects`
3. `row_pruning_does_not_purge_objects`
4. `row_pruning_does_not_prune_snapshots`
5. `row_pruning_does_not_truncate_wal`
6. `row_pruning_does_not_persist_flush_watermark`
7. `row_pruning_does_not_publish_database_manifest_directly`

Assertions:

1. L8V prunes rows inside rewrite outputs only;
2. L8S/L8M/L8T own object cleanup and WAL retention.

## Generated And Property Tests

Extend lifecycle generated scripts with:

```text
put(branch, key, version, timestamp, ttl)
tombstone(branch, key, version, timestamp)
fork(parent, child, fork_version)
pin_view(branch, version, timestamp)
compact_with_pruning(branch, proof)
materialize_with_pruning(branch, proof)
recover()
read_latest/read_getv/read_as_of/read_history
```

Required counters:

1. proof_rejected;
2. old_version_dropped;
3. tombstone_dropped;
4. tombstone_kept_for_shadowing;
5. expired_row_dropped;
6. expired_row_kept_for_as_of;
7. pinned_view_blocked;
8. inherited_layer_blocked;
9. retained_boundary_reported;
10. recovery_boundary_enforced.

Properties:

1. latest reads are unchanged by accepted pruning;
2. retained `getv`, history, and `as_of` reads match the model;
3. pruned reads return typed retained-history errors;
4. rejected proofs leave branch state unchanged;
5. tombstone pruning never resurrects a lower or inherited value;
6. TTL pruning never depends on ambient wall clock;
7. recovery preserves the same retained-history boundaries.

## Source Guards

Required source guard tests:

1. `row_pruning_does_not_import_raw_io`
2. `row_pruning_does_not_import_backend_delete`
3. `row_pruning_does_not_import_quarantine_or_purge`
4. `row_pruning_does_not_import_snapshot_pruning`
5. `row_pruning_does_not_import_wal_truncation`
6. `row_pruning_does_not_import_product_policy`
7. `row_pruning_does_not_import_stratahub`
8. `row_pruning_does_not_import_primitive_modules`
9. `row_pruning_does_not_use_wall_clock`
10. `row_pruning_code_and_fixture_names_do_not_use_milestone_labels`

Forbidden production tokens include:

1. `std::fs`
2. `std::path::Path`
3. `delete_object`
4. `quarantine_object`
5. `purge_quarantine`
6. `prune_snapshots`
7. `truncate_wal`
8. `Timestamp::now`
9. `SystemTime`
10. `strata_engine`
11. `stratahub`
12. `primitive`
13. `graph`
14. `vector`

Scope scans to production row-pruning modules to avoid false positives from
old-code references in docs and tests.

## Fault And Crash Windows

Required phase tests:

1. fault before proof validation leaves state unchanged;
2. fault during output build leaves state unchanged;
3. fault after pruned output publish before install leaves old reads unchanged;
4. fault after install before manifest publish keeps new reads visible and
   records manifest debt;
5. recovery after published-but-not-installed pruned output ignores orphan;
6. recovery after manifest-published pruned output enforces retained boundary;
7. recovery after corrupt pruning facts fails closed;
8. retry after manifest failure reuses the same pruned output facts.

Every phase must have a non-ignored unit or integration equivalent even if
process-level crash tests are marked ignored.

## Sensitivity Probes

Record probes in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

| Probe | Mutation | Expected failure |
|---|---|---|
| V1 | Accept pruning without proof. | Proof rejection test fails. |
| V2 | Drop all below-floor rows without keeping survivor. | Version pruning test fails. |
| V3 | Drop tombstone in non-bottommost compaction. | Tombstone shadowing test fails. |
| V4 | Let max-version policy evict required tombstone. | Max-version tombstone test fails. |
| V5 | Drop expired TTL row above retained floor. | TTL above-floor test fails. |
| V6 | Use ambient wall clock for TTL cutoff. | Determinism/source guard fails. |
| V7 | Ignore child inherited-layer reachability. | Inheritance model test fails. |
| V8 | Report pruned read as absent instead of insufficient history. | Boundary test fails. |
| V9 | Omit pruning facts from manifest. | Recovery manifest test fails. |
| V10 | Delete table objects as part of pruning. | No-cleanup/source guard fails. |

## Command Matrix

Mandatory commands before L8V closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table::tests::compaction
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## Exit Gate

L8V test coverage is complete when:

1. pruning without proof rejects;
2. old-version pruning preserves retained history and reports pruned-history
   boundaries;
3. tombstone elision cannot resurrect owned or inherited values;
4. TTL elision respects supplied cutoff, timestamp floor, and inherited rows;
5. active/pinned views block unsafe pruning;
6. durable manifests preserve retained-history coverage after recovery;
7. generated model tests cover version, tombstone, TTL, inheritance, and
   materialization interactions;
8. source guards block raw IO, deletion, product policy, wall-clock reads, and
   milestone labels in Rust code;
9. sensitivity probes and command results are recorded in the porting log.
