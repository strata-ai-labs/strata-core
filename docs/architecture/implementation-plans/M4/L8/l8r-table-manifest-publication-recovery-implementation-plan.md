# L8R Implementation Plan: Table Manifest Publication And Recovery

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`

## Objective

Publish and recover durable branch table manifests.

L8Q defines stable table-manifest bytes. L8R wires those bytes into lifecycle
services and recovery:

1. publish branch-scoped table manifests through L4 manifest service;
2. build table-manifest payloads from L6 reachability plus L8 table-object
   publication facts;
3. recover L6 branch table state from table manifests and validated table
   objects;
4. classify missing, corrupt, mismatched, and ambiguous table-manifest/table
   object state;
5. preserve table-manifest uncertainty as recovery health debt instead of
   treating it as clean success.

L8R does not make table manifests a WAL-truncation proof. L8T owns
table-manifest-backed flush watermarks and replay shortening. Until L8T,
checkpoint/WAL recovery rules remain conservative even if table manifests are
present.

## Inputs

1. `docs/architecture/storage/l2-object-layout.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
8. `crates/storage-next/src/format/table_manifest.rs`
9. `crates/storage-next/src/service/manifest.rs`
10. `crates/storage-next/src/service/table.rs`
11. `crates/storage-next/src/lifecycle/recovery.rs`
12. `crates/storage-next/src/lifecycle/flush.rs`
13. `crates/storage-next/src/lifecycle/compaction.rs`
14. `crates/storage-next/src/lifecycle/durable/maintenance.rs`
15. `crates/storage-next/src/branch/state.rs`
16. `crates/storage-next/src/branch/read.rs`
17. `crates/storage-next/src/table/reader.rs`
18. `crates/storage/src/manifest.rs`
19. `crates/storage/src/segmented/recovery.rs`
20. `crates/storage/src/segmented/tests/concurrency.rs`
21. `crates/storage/src/segmented/tests/gc_under_degradation.rs`
22. `crates/storage/src/segmented/tests/publish_failures.rs`

## Existing-Code Source Map

| Current file | Evidence | L8R action |
|---|---|---|
| `format/table_manifest.rs` | L8Q provides canonical table-manifest bytes and validation. | Use encode/decode only through the L8Q format surface. Do not duplicate format validation in lifecycle. |
| `service/manifest.rs` | `TableManifestService` already loads and publishes raw branch table-manifest bytes under `tables/<branch-id>/manifest`. | Change it from raw-byte service to typed table-manifest service, with load/decode/publish/replace helpers. |
| `service/table.rs` | `TableObjectReaderService::open_reader` validates backend object metadata, table bytes, table identity, and object facts. | Every manifest-listed table object must pass this service before L6 sees it. |
| `lifecycle/recovery.rs` | Recovery currently accepts caller-supplied `LifecycleRecoveryTableObject` facts and validates readers, but does not load a durable table graph. | Replace ad hoc table-object recovery inputs with table-manifest-driven recovery facts. |
| `lifecycle/flush.rs` | Flush publishes table object, validates it, installs into L6, and reports partial publication facts. | After a successful durable install, publish a branch table manifest that includes the new durable table graph. |
| `lifecycle/compaction.rs` | Durable compaction/materialization currently reports checkpoint-required debt because rewrite outputs are not manifest-backed. | Provide reusable manifest publication hooks. Do not record volatile rewrite outputs as durable table-manifest facts until L8U publishes those table objects. |
| `branch/state.rs` | Branch state owns owned-table levels, inherited layers, replacement provenance, and install validation. | Add or use a crate-private L6 recovery/install surface for validated manifest table refs. L8 must not mutate branch internals directly. |
| `lifecycle/retention.rs` | Table-object retention is currently conservative because table-manifest reachability is absent. | Expose recovered/published table-manifest facts for L8S; do not delete or quarantine table objects here. |

## Old Codebase Porting Map

The old storage engine persisted branch table reachability through
`segments.manifest`. L8R ports the recovery and publication semantics, not the
old path-based format.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/manifest.rs::write_manifest` | Manifest publish is atomic at the branch-manifest boundary and protected by checksum. | Publish typed table-manifest bytes through L4 `TableManifestService`; no raw filesystem writes. | Publish-create/replace outcome validation and checksum-backed reload. |
| `crates/storage/src/manifest.rs::read_manifest` | Missing manifest is distinct from corrupt manifest. | Missing, corrupt, and future-version table manifests receive distinct lifecycle facts. Recovery policy decides strict/lossy handling. | Strict/lossy missing and corrupt cases are separate. |
| `crates/storage/src/segmented/recovery.rs::recover_segments` | Valid manifests restore levels; corrupt manifests fail closed; orphan files are not loaded as L0. | Recover only manifest-listed table objects that validate against manifest facts. Never list table objects and infer reachability by prefix. | Orphan table object ignored; corrupt manifest does not load orphan objects. |
| `crates/storage/src/segmented/tests/concurrency.rs::test_issue_1680_corrupt_manifest_rejects_orphan_loading` | Corrupt manifest must not cause fallback to orphan table loading. | Preserve as table-manifest recovery test. | Corrupt manifest plus valid orphan object fails/degrades without installing the orphan. |
| `crates/storage/src/segmented/tests/lifecycle.rs::recovery_skips_orphan_sst_not_in_manifest` | Objects not named by manifest are skipped during recovery. | Preserve by only opening manifest-listed table objects. | Extra valid table object under branch prefix is ignored. |
| `crates/storage/src/segmented/tests/leveled.rs::recover_with_manifest_restores_levels` | Manifest recovery restores L0/L1+ level placement. | Install recovered table refs into L6 using manifest level/order facts. | L0 precedence and L1+ order survive reopen. |
| `crates/storage/src/segmented/tests/flush.rs::recover_missing_manifest_listed_produces_fault` | Missing manifest-listed table object becomes a recovery fault. | Strict mode fails; lossy mode records typed data-loss health and does not install the missing table. | Missing table object classification. |
| `crates/storage/src/segmented/tests/flush.rs::recover_corrupt_manifest_listed_segment_is_not_reported_missing` | Corrupt listed table object is not misclassified as missing. | Table reader errors preserve source class and object name. | Corrupt object vs missing object classification. |
| `crates/storage/src/segmented/quarantine_protocol.rs` | Recovery-trusted manifests are durable reachability proof for later reclaim. | L8R emits table-manifest recovery facts. L8S consumes them for retention. | Recovered facts include object names and manifest object name. |
| `crates/storage/src/segmented/tests/publish_failures.rs` | Manifest publish failure after table publication is a partial-progress window, not clean success. | Preserve through typed table-manifest publication outcomes and recovery health debt. | Table object visible but manifest missing/old records health debt and remains replay/checkpoint recoverable. |

Do not port:

1. raw branch directories or segment filenames;
2. direct `std::fs` manifest writes;
3. manifest-missing fallback that loads every table object as L0;
4. product branch names, tags, notes, merge, revert, or cherry-pick concepts;
5. direct table-object deletion, purge, or quarantine mutation;
6. old global pause hooks;
7. row pruning rules;
8. public maintenance commands.

## Scope

L8R implements:

1. typed `TableManifestService` load/publish-create/publish-replace helpers;
2. lifecycle table-manifest publication requests and outcomes;
3. a lifecycle-owned durable table catalog keyed by table identity/object name
   for manifest construction;
4. branch table-manifest building from L6 reachability plus L8 table-object
   facts;
5. table-manifest publication after durable flush installs table objects;
6. table-manifest recovery during durable open;
7. validation that manifest branch id matches the branch manifest object name;
8. validation that every manifest-listed object opens through
   `TableObjectReaderService`;
9. L6 install/rebuild surface for validated manifest table refs;
10. typed health/fault vocabulary for missing, corrupt, mismatched, ambiguous,
    and uncertain table-manifest state;
11. source guards preventing lifecycle recovery from listing branch table
    objects and inferring reachability without a manifest;
12. testkit counters and porting-log entries.

L8R does not implement:

1. the table-manifest byte format itself;
2. table-manifest-backed flush watermark proof;
3. WAL truncation from table-manifest coverage;
4. durable compaction/materialization output publication;
5. table-object retention/quarantine/purge;
6. row-version, tombstone, or TTL pruning;
7. branch list/delete/clear/fork-at-history completion;
8. object-store/OpenDAL durability;
9. L9 public API exposure.

## Publication Protocol

Target durable flush sequence:

```text
require durable local mode
publish table object through L4 table object service
reopen/validate table object facts
install table into L6
update lifecycle durable table catalog
build branch table manifest from current L6 reachability + catalog facts
publish table manifest through L4 table manifest service
return completed outcome, or completed-with-health-debt if manifest publish is uncertain
```

Rules:

1. A table object must be durable and validated before it can appear in a table
   manifest.
2. A table manifest must not list volatile/in-memory-only table refs.
3. Manifest publication failure after L6 install does not roll back visible
   state. It records health debt and leaves checkpoint/WAL recovery as the
   conservative safety path.
4. Manifest publication success does not by itself advance flush watermark or
   permit WAL truncation. L8T owns that proof.
5. The database manifest is not changed by L8R except through existing snapshot,
   WAL, and flush-watermark paths.
6. Cache mode does not publish table manifests and must not claim durable table
   reachability.

## Recovery Protocol

Target durable recovery sequence:

```text
load database manifest
load row-native checkpoint if present
load branch table manifest for the branch being recovered
decode and validate table manifest bytes
for each manifest-listed table object:
  open reader through TableObjectReaderService
  validate object facts against manifest facts
build L6 branch table recovery request
install/rebuild branch-owned and inherited table refs atomically
replay WAL through L7 as before
finalize recovery health
```

Rules:

1. Recovery never lists all `tables/<branch-id>/...` objects and treats them as
   reachable by prefix. Manifest-listed objects are the durable table graph.
2. Extra table objects not listed by a trusted manifest are orphans for later
   L8S/L8M handling.
3. A corrupt table manifest fails closed in strict mode.
4. A missing table manifest is healthy only when there are no durable table
   objects expected for that branch. Otherwise it is a policy downgrade or data
   loss depending on available proof.
5. A missing manifest-listed table object is data loss in strict mode. In lossy
   mode it records data-loss health and does not install the missing table.
6. A corrupt manifest-listed table object is not reported as missing. Preserve
   the table reader/source chain.
7. A manifest/object fact mismatch is a typed recovery mismatch and does not
   install that table.
8. A branch id mismatch between object name and manifest payload fails before
   table objects are opened.
9. A checkpoint row snapshot and table manifest must not install conflicting
   duplicate internal keys. The recovery path must preflight the combined state.
10. If both checkpoint and table manifest can represent the same rows, recovery
    must be idempotent: exact duplicates are accepted only through explicit L6
    idempotence, not by silent overwrite.

## Durable Table Catalog

L8R needs lifecycle-owned facts that connect L6 table refs to L4 object facts.

Rules:

1. The catalog is keyed by `TableIdentity`.
2. Each catalog entry stores object name, object facts, table bounds,
   provenance, and publication status.
3. Exact identity/object/facts duplicates are idempotent.
4. Same identity with different object or facts is ambiguous and blocks manifest
   publication.
5. Catalog entries are rebuildable from trusted table manifests during recovery.
6. The catalog is not a replacement for durable manifests. It is an in-memory
   construction aid.

## L6 Recovery Surface

L8R should add or consume a narrow L6 recovery install API.

Required properties:

1. input is already decoded table-manifest refs plus validated
   `ImmutableTableReader` values;
2. L6 validates branch id, levels, ordering, duplicate internal keys, physical
   range invariants, inherited-layer ordering, and materialization status;
3. install is all-or-nothing at the branch-state boundary;
4. L6 returns branch recovery facts for health/outcome reporting;
5. L8 does not mutate `owned_levels` or `inherited_layers` directly.

Suggested shape:

```rust
pub(crate) struct BranchTableManifestRecoveryRequest {
    branch_id: BranchId,
    owned_tables: Vec<RecoveredBranchOwnedTable>,
    inherited_layers: Vec<RecoveredBranchInheritedLayer>,
}

pub(crate) struct BranchTableManifestRecoveryOutcome {
    table_count: usize,
    inherited_layer_count: usize,
    commit_max: Option<CommitVersion>,
    timestamp_max: Option<Timestamp>,
}
```

## Error And Health Vocabulary

Add typed lifecycle errors/faults instead of string-only maintenance failures.

Required categories:

1. table manifest missing;
2. table manifest corrupt;
3. table manifest future version;
4. table manifest branch mismatch;
5. table manifest publication failed;
6. table manifest publication uncertain;
7. table object missing;
8. table object corrupt;
9. table object fact mismatch;
10. table graph ambiguous;
11. table graph conflicts with checkpoint;
12. table manifest recovery unsupported for mode.

Every category must expose a stable `code()` value and preserve lower-layer
source chains.

## Source Boundaries

L8R code may import:

1. L4 manifest/table services;
2. L8Q table-manifest format types;
3. L6 branch recovery/install surfaces;
4. L5 table readers through L4 table object reader service;
5. storage object names and core storage atom types.

L8R code must not import:

1. raw filesystem APIs;
2. object layout path internals beyond `ObjectLayout`;
3. engine/product crates;
4. StrataHub code;
5. primitive DTOs;
6. query/index/autosearch modules;
7. retention purge/quarantine mutation APIs.

## Implementation Steps

1. Extend `TableManifestService` to load/decode and encode/publish
   `TableManifest`.
2. Add lifecycle table-manifest publication request/outcome types.
3. Add lifecycle durable table catalog types.
4. Add L6 table-manifest recovery install request/outcome types.
5. Wire durable flush success to publish a branch table manifest.
6. Wire durable recovery to load and validate branch table manifest before WAL
   replay finalization.
7. Add typed health/fault mapping for missing/corrupt/mismatched manifest and
   table object state.
8. Add tests, generated counters, source guards, and porting-log entry.

## Deferred Behavior

Deferred to L8S:

1. table-object reachability graph over all branch manifests;
2. orphan table-object quarantine candidates;
3. manifest-backed table-object retention proof.

Deferred to L8T:

1. using table manifests to advance flush watermark;
2. WAL truncation from table-manifest coverage;
3. replay-start shortening based on table manifests.

Deferred to L8U:

1. durable publication of compaction outputs;
2. durable publication of materialization outputs;
3. manifest updates for rewrite outputs.

Deferred to L8Y:

1. branch listing across all branch manifest objects;
2. branch delete/clear recovery policy;
3. mandatory branch generation reuse checks.

## Exit Gate

L8R is complete when:

1. table-manifest service load/publish APIs are typed;
2. durable flush publishes table manifests after table-object install;
3. durable recovery loads table manifests and validates every listed table
   object before L6 install;
4. corrupt manifests and orphan table objects cannot be loaded as live state;
5. missing/corrupt/mismatched table-object cases classify distinctly;
6. table-manifest publication uncertainty is visible in outcomes/health;
7. cache mode cannot publish or recover table manifests;
8. source guards prevent raw IO, product vocabulary, and prefix-based reachability
   inference;
9. tests cover old manifest recovery regressions;
10. L8S can consume recovered table-manifest facts without changing L8R
    semantics.
