# L8S Implementation Plan: Table-Object Reachability And Retention

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`

## Objective

Build the durable table-object reachability graph and turn it into retention
decisions.

L8Q defines table-manifest bytes. L8R publishes and recovers those manifests.
L8S consumes trusted table-manifest facts, table-object inventory, checkpoint
facts, WAL facts, quarantine facts, and recovery health to decide which durable
table objects are:

1. live and must be retained;
2. not provably live but unsafe to touch because proof is incomplete;
3. orphaned and eligible for quarantine staging;
4. already quarantined and delegated to purge/repair;
5. unsupported for the requested scope.

L8S must not delete table objects. It is the proof and classification slice.
Object movement, source deletion, purge, and repair remain L8M responsibilities.

## Inputs

1. `docs/architecture/storage/l2-object-layout.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`
11. `crates/storage-next/src/format/table_manifest.rs`
12. `crates/storage-next/src/service/manifest.rs`
13. `crates/storage-next/src/service/table.rs`
14. `crates/storage-next/src/service/quarantine.rs`
15. `crates/storage-next/src/lifecycle/retention.rs`
16. `crates/storage-next/src/lifecycle/recovery.rs`
17. `crates/storage-next/src/lifecycle/quarantine.rs`
18. `crates/storage-next/src/lifecycle/outcome.rs`
19. `crates/storage/src/segmented/quarantine_protocol.rs`
20. `crates/storage/src/segmented/ref_registry.rs`
21. `crates/storage/src/segmented/tests/gc_under_degradation.rs`
22. `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`

## Existing-Code Source Map

| Current file | Evidence | L8S action |
|---|---|---|
| `format/table_manifest.rs` | Table-manifest entries name branch-owned and inherited table objects. | Consume decoded manifest refs as durable reachability evidence. Do not inspect table rows. |
| `service/manifest.rs` | Branch table manifests live under `tables/<branch-id>/manifest`. | Load trusted manifests through L8R service facts, not raw backend reads. |
| `service/table.rs` | Table object service can validate object metadata and table facts. | Use L8R-recovered object facts; optionally validate inventory facts for candidate objects without making them live. |
| `service/quarantine.rs` | Quarantine inventory records source objects already staged for safe deletion. | Treat quarantined objects as delegated to L8M; do not reclassify them as live or delete candidates. |
| `lifecycle/retention.rs` | Existing retention handles snapshots and delegated families but table-object scope has been conservative or no-op. | Add a table-object proof family with explicit retain/quarantine-candidate/deferred outcomes. |
| `lifecycle/recovery.rs` | Recovery health distinguishes healthy, telemetry, policy downgrade, data loss, and failed states. | Block table-object mutation candidates under unsafe health; allow read-only graph reporting when safe. |
| `lifecycle/quarantine.rs` | L8M consumes quarantine candidates and performs staging/purge. | Emit candidate facts with enough object/family/proof tokens for L8M to consume. |

## Old Codebase Porting Map

The old storage engine separated durable reachability from runtime
accelerators. L8S ports that split.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `crates/storage/src/segmented/quarantine_protocol.rs::retention_snapshot` | Walks recovery-trusted manifests to compute exclusive/shared/detached/quarantined bytes. | Build a table-object reachability graph from trusted table manifests and quarantine inventory. | Live object counts/bytes are deterministic and manifest-backed. |
| `quarantine_segment_if_unreferenced` | Refuses reclaim if manifest proof is incomplete or degraded recovery makes truth unsafe. | L8S returns deferred/blocked decisions before any L8M mutation. | Unsafe health and incomplete manifests keep objects. |
| `SegmentRefRegistry` | Runtime registry accelerates refcounts but cannot replace durable manifests. | Use runtime facts only as consistency checks or hints. Durable table manifests are the proof. | Runtime-only object with no manifest proof is not deleted. |
| `gc_orphan_segments` | Orphan files are collected only after trusted manifests and recovery health make reclaim safe. | Classify unreferenced table objects as quarantine candidates, not direct deletes. | Orphan candidate is named for L8M, no backend delete occurs. |
| `gc_under_degradation.rs` | Corrupt/missing manifest recovery blocks orphan GC. | DataLoss/PolicyDowngrade health blocks table-object quarantine candidates. | Degraded health produces retained/deferred decisions. |
| `quarantine_reconciliation.rs` | Quarantine inventory disagreement blocks unsafe purge and produces repair facts. | Quarantined table objects are delegated to L8M with current inventory token. | Inventory mismatch prevents table-object deletion candidates. |
| `retention_report.rs` in engine | Converts raw storage retention into product reports. | Do not port. L8S emits raw storage decisions only. | Source/vocabulary guards reject product reporting imports. |

Do not port:

1. direct filesystem scans as durable truth;
2. direct backend delete of table objects;
3. product retention reports or branch-name attribution;
4. logs-only health debt;
5. row-version/tombstone/TTL pruning;
6. snapshot pruning, already owned by L8L;
7. quarantine mutation, already owned by L8M;
8. object-store/S3 production fencing.

## Scope

L8S implements:

1. table-object retention proof request and outcome types;
2. table-object inventory facts from durable local table object listings;
3. durable reachability graph construction from trusted table manifests;
4. cross-branch shared-object classification;
5. inherited-layer table-object reachability classification;
6. quarantine inventory overlay for already-staged objects;
7. decision types:
   - `RetainLive`;
   - `RetainProofIncomplete`;
   - `RetainUnsafeRecovery`;
   - `QuarantineCandidate`;
   - `AlreadyQuarantined`;
   - `UnsupportedScope`;
8. affected object names and byte counts for maintenance outcomes;
9. freshness/proof tokens that L8M can verify before quarantine mutation;
10. deterministic graph ordering and stable summaries;
11. generated/testkit counters for live, orphan, shared, incomplete, unsafe,
    quarantined, and unsupported decisions;
12. source guards blocking raw IO deletion, product vocabulary, and runtime-only
    reachability.

L8S does not implement:

1. table manifest publication or recovery;
2. table object deletion;
3. quarantine inventory mutation;
4. purge;
5. repair;
6. snapshot pruning;
7. WAL truncation;
8. row pruning;
9. lazy table reads;
10. public storage API exposure.

## Reachability Inputs

The graph builder consumes only trusted storage facts:

1. L8R recovered or published table-manifest facts;
2. L8R table-object validation facts;
3. L8M quarantine inventory load/reconciliation facts;
4. L8J checkpoint and WAL watermarks as retention barriers, not table-object
   liveness by themselves;
5. L8 recovery health;
6. optional backend object inventory facts for candidate discovery.

Runtime-only facts are insufficient for deletion or quarantine eligibility.
They may detect disagreement, but disagreement becomes proof debt.

## Table-Object Inventory

Durable local mode may list table-object names as candidate discovery input.

Rules:

1. Prefix-listed objects are candidates only. They are not live until a trusted
   table manifest references them.
2. Listing failure produces proof-incomplete health debt.
3. Object names outside the table-object namespace are ignored by this family.
4. Malformed table-object names are reported as repair candidates, not deleted.
5. Objects named by quarantine inventory are delegated to L8M.
6. Cache mode has no durable table-object inventory and returns unsupported for
   this scope.

## Reachability Graph

The graph is keyed by `ObjectName` and records all durable reasons an object is
live.

Required reasons:

1. branch-owned table manifest entry;
2. inherited-layer table manifest entry;
3. materialization/rewrite replacement provenance still retained by manifest;
4. checkpoint recovery dependency until L8T/L8U prove table-manifest coverage;
5. quarantine inventory entry;
6. unsafe recovery or incomplete proof barrier.

Cross-branch shared objects remain retained until every trusted branch manifest
that references them stops doing so and no inherited-layer manifest retains
them.

## Decision Rules

Rules:

1. Manifest-referenced table object -> `RetainLive`.
2. Object referenced by more than one branch/layer -> `RetainLive` with shared
   reason count.
3. Prefix-listed object absent from every trusted manifest -> `QuarantineCandidate`
   only when recovery health is safe and proof is complete.
4. Prefix-listed object absent from manifest but proof incomplete ->
   `RetainProofIncomplete`.
5. Any object under `DataLoss`, unsafe `PolicyDowngrade`, or `Failed` health ->
   `RetainUnsafeRecovery`.
6. Quarantined object -> `AlreadyQuarantined`.
7. Unsupported scope -> `UnsupportedScope`, never clean `Completed`.
8. No direct delete decision exists in L8S.
9. Every non-live candidate includes a proof token naming the manifest set,
   inventory generation, recovery health generation, and object fact generation
   used to classify it.

## Freshness Tokens

L8S must give L8M enough information to reject stale quarantine requests.

Suggested shape:

```rust
pub(crate) struct TableObjectReachabilityProofToken {
    manifest_epoch: u64,
    table_inventory_epoch: u64,
    quarantine_inventory_epoch: u64,
    recovery_health_epoch: u64,
    object: ObjectName,
    object_fingerprint: TableObjectFingerprint,
}
```

The exact fields can change, but the proof must be bound to current manifest,
inventory, and health facts. A hand-constructed "fresh" proof with no epoch is
not sufficient.

## Maintenance Integration

L8S extends retention maintenance handling for table objects.

Rules:

1. Table-object retention scope runs proof construction and returns decisions.
2. `QuarantineCandidate` decisions are not automatically mutated unless the
   caller explicitly requests L8M staging.
3. Completed proof with only `RetainLive` decisions is `Completed`.
4. Completed proof with quarantine candidates is `CompletedCheckpointRequired`
   only if the implementation records new durable recovery facts; otherwise it
   is `Completed` with candidate state changes.
5. Incomplete or unsafe proof is `Deferred` or `CompletedWithHealthDebt`, not
   clean success.
6. Unsupported scope is explicit.

## Error And Health Vocabulary

Add typed lifecycle codes for:

1. table-object inventory unavailable;
2. table-manifest proof incomplete;
3. table-object reachability ambiguous;
4. table-object candidate stale;
5. table-object malformed name;
6. table-object unsupported scope;
7. table-object unsafe recovery;
8. table-object quarantine proof required.

Every error preserves lower-layer source chains.

## Source Boundaries

L8S may import:

1. L8R manifest recovery facts;
2. L8M quarantine inventory facts;
3. L4 table object listing/metadata services;
4. storage object names and layout constructors;
5. lifecycle recovery health types.

L8S must not import:

1. raw filesystem APIs;
2. backend delete APIs directly;
3. quarantine mutation/purge functions;
4. engine/product crates;
5. StrataHub code;
6. primitive DTOs;
7. query/index/autosearch modules.

## Implementation Steps

1. Add table-object retention proof types to `lifecycle/retention.rs` or split
   `lifecycle/table_reachability.rs` if the file grows too large.
2. Add durable table-object inventory facts and deterministic ordering.
3. Add graph builder over L8R table-manifest facts.
4. Add quarantine inventory overlay.
5. Add decision generation and maintenance outcome mapping.
6. Add proof-token construction for L8M.
7. Add cache-mode unsupported behavior.
8. Add source guards and generated counters.
9. Add porting-log entry after implementation.

## Deferred Behavior

Deferred to L8M:

1. object movement into quarantine;
2. source object deletion after quarantine;
3. purge;
4. repair/reconciliation mutation.

Deferred to L8T:

1. table-manifest proof as flush watermark coverage;
2. WAL truncation after durable table coverage.

Deferred to L8U:

1. compaction/materialization output publication into table manifests.

Deferred to L8V:

1. row-version, tombstone, and TTL pruning.

## Exit Gate

L8S is complete when:

1. table-object retention scope is no longer a silent no-op;
2. trusted table manifests produce a deterministic live-object graph;
3. orphan table objects become quarantine candidates only with complete safe
   proof;
4. unsafe recovery and incomplete proof retain objects with health debt;
5. quarantined objects are delegated to L8M;
6. no L8S path deletes or purges table objects;
7. proof tokens are bound to current manifest/inventory/health facts;
8. cache mode explicitly rejects or defers durable table-object retention;
9. source guards block raw IO, product imports, and mutation APIs;
10. tests cover old orphan-GC and degraded-recovery regressions.
