# Storage Deferred Work Ledger

Status: draft deferred-work ledger

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Related planning docs:

1. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8s-table-object-reachability-retention-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8t-table-manifest-backed-flush-watermarks-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8v-retention-aware-row-pruning-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8x-lazy-object-backed-table-reads-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4P/m4p-l8-automatic-maintenance-scheduling-followup.md`
12. `docs/architecture/storage/l9-storage-api-boundary.md`

## Purpose

This ledger records storage architecture work that is intentionally deferred or
owned by another milestone. It is not a list of hidden implementation bugs.

Use this document to answer two questions:

1. Is this storage capability already planned in L8Q-L8Z, L9, or another
   named milestone?
2. If not, is it an explicit post-V1 or later-workstream deferral?

## Classification Rules

1. **Planned storage work** means the capability has a named slice and exit
   gate.
2. **L9 work** means lower layers should expose raw storage facts, but public
   API shape and engine mapping remain above L8.
3. **Product work** means engine-next, CLI, or StrataHub owns semantics and user
   wording.
4. **Post-V1 storage work** means V1 remains correct without the optimization or
   distributed guarantee.
5. **Not storage-owned** means the feature belongs to query, intelligence,
   product branch workflows, remote sync, or another upper layer.

## Planned Before L9

These are not deferred gaps anymore. They are planned L8Q-L8Z work.

| Area | Owner | Status |
|---|---|---|
| Durable table-manifest format | L8Q | Planned before L9. |
| Table-manifest publication and recovery | L8R | Planned before L9. |
| Table-object reachability and retention proof | L8S | Planned before L9. |
| Table-manifest-backed flush watermarks | L8T | Planned before L9. |
| Durable compaction/materialization output publication | L8U | Planned before L9. |
| Retention-aware row pruning | L8V | Planned before L9. |
| Memory and cache budget enforcement | L8W | Planned before L9. |
| Lazy object-backed table reads | L8X | Planned before L9. |
| Branch lifecycle completeness | L8Y | Planned before L9. |
| Commit hardening and pre-L9 readiness | L8Z | Planned before L9, including minimal checkpoint/WAL-growth policy. |
| Automatic maintenance scheduling and score-based compaction drain | M4P-L8A/L8B/L8C | Follow-up before L9 scale benchmark closeout. See `m4p-l8-automatic-maintenance-scheduling-followup.md`. |

## L9-Owned Work

| Deferred item | Why not L8 | First owner | Required lower-layer handoff |
|---|---|---|---|
| Public storage open/read/commit API | L8 remains crate-private lifecycle and commit mechanics. | L9 | Open outcomes, commit outcomes, read views, branch facts, maintenance facts. |
| Public branch API shape | L8Y only supplies storage-internal create/list/fork/clear/delete mechanics. | L9 | Branch catalog outcomes, generation facts, pinned-view/release facts. |
| Public maintenance/checkpoint/recovery commands | L8 supplies raw hooks and outcomes, not product command policy. | L9/engine-next | Stable storage request/outcome DTOs and error codes. |
| Engine-facing response mapping | Storage must not emit product wording or DTOs. | L9/engine-next | Stable raw facts and source chains. |
| Raw storage diagnostics surface | L8W/L8Z should produce facts; L9 decides what becomes a public diagnostics API. | L9 | Memory, cache, level, lifecycle, recovery, retention, quarantine, and commit stats. |

## L10-Owned Work

| Deferred item | Why not L8/L9 | First owner | Required guard |
|---|---|---|---|
| Physical storage format freeze and compatibility | This is a full compatibility workstream over WAL, snapshot, manifest, table-manifest, quarantine, table-object naming, table bytes, golden vectors, migration, and rejection policy. It should not be hidden inside commit hardening. | L10 | L8/L9 docs must not claim a finalized SQLite-style compatibility guarantee until L10 lands. |

## Product Or Engine Work

| Deferred item | Why not storage | First owner | Notes |
|---|---|---|---|
| Product branch names and permissions | Storage uses branch ids and generations only. | Engine-next/L9 | No product access policy below L9. |
| Merge/cherry-pick/revert/restore/compare | These are semantic product workflows over storage reads and commits. | Engine-next/product branch workflow | Storage only supplies raw branch mechanics. |
| Product recovery assistant UX | Storage returns health facts and source chains. | Engine-next/CLI | No user-facing recovery copy in L8. |
| Product maintenance UX | Storage exposes raw maintenance facts. | Engine-next/CLI | Manual command policy remains above storage. |
| Primitive-aware recovery | Storage stores row bytes and storage metadata. | Engine-next/primitive layers | No JSON, graph, vector, search, embedding, or event semantics in storage recovery. |
| Query sort/filter/index/search behavior | Storage preserves ordered MVCC rows. | Query/index workstream | A query layer can build side indexes without changing storage row format if it writes storage-owned rows/facts through L9. |

## Clone, Import, And Bulk Load

Correct clone/import behavior is planned in the clone and product milestones.
Optimized storage bulk-load mode is not yet a storage-next slice.

| Deferred item | Why deferred | First owner | Required decision |
|---|---|---|---|
| Correct Hub clone/import assembly | It is product/transport-facing, not L8 lifecycle. | M9/M10 clone/import work | Use L9 storage open/check and ordinary validated storage writes or manifest import substrate. |
| Optimized storage bulk-load mode | V1 correctness does not require a specialized ingestion fast path. | Post-V1 storage optimization unless M9/M10 sets a performance gate | Decide whether clone/import performance requires direct table-manifest ingestion, sorted-run building, or WAL bypass. |
| Bulk-load memory budget | Depends on L8W budget accounting and L8X lazy reads. | L8W plus future bulk-load slice | Must not allocate unbounded rows or table artifacts. |
| Bulk-load durability shortcut | Depends on L8Q-L8U table manifests and WAL/flush proof. | Future storage optimization | Must prove crash safety before bypassing normal commit/WAL paths. |

Default V1 position:

1. clone/import must be correct and crash-safe;
2. optimized bulk-load is explicitly deferred unless a later milestone makes it
   a release performance requirement;
3. any future bulk-load mode must have its own implementation and test plan.

## Post-V1 Storage Work

| Deferred item | Why V1 remains correct | Required guard |
|---|---|---|
| Production object-store/OpenDAL/S3 durability | V1 targets local durable semantics. Object-store fencing, multipart upload, consistency, and provider-specific recovery need a separate design. | L8/L9 docs must not claim production object-store durability. |
| Distributed locks or consensus | L4 writer lock is the local coordination boundary. | Reject or explicitly mark unsupported distributed writer modes. |
| Multi-process/global commit version allocation | V1 commit allocator is local runtime scoped. | Do not expose distributed commit claims. |
| Public transaction sessions | Internal commit batches are enough for V1 operations. | Source guards prevent transaction-session surface leakage. |
| Durable transaction ids | Commit version is the V1 durable ordering identity. | L8Z guards transaction-id absence or records a future private optimization. |
| Serializable isolation claims | V1 conflict validation is storage snapshot-style validation, not a broad product ACID claim. | Docs/errors avoid user-facing serializability wording. |
| Cross-branch atomic commits | Requires deterministic multi-branch lock ordering and product semantics. | Reject or defer cross-branch atomic API requests. |
| Threaded maintenance executor | Deterministic single-threaded executor is sufficient for V1 correctness. | No background nondeterminism in tests unless explicitly added later. |
| Rich/background automatic checkpoint scheduler | L8Z owns the minimal bounded-WAL checkpoint trigger. Adaptive scheduling, background threads, product timing policy, and provider-specific tuning are not needed for V1 correctness. | V1 must include the minimal L8Z WAL-growth guard; richer policy remains unsupported/deferred. |

## Observability Boundary

Storage-next needs raw diagnostics, but not product telemetry.

Minimum storage-owned facts expected before L9:

1. memory budget and cache stats;
2. table level and object reachability stats;
3. branch catalog and generation facts;
4. commit allocator, visible-version, timeline, and unresolved durable facts;
5. recovery health and fault counters;
6. maintenance queue, retention, quarantine, purge, and repair facts;
7. flush/checkpoint/WAL retention facts.

Deferred beyond L8:

1. public diagnostics API shape;
2. product telemetry naming;
3. hosted metrics reporting;
4. user-facing health summaries;
5. fleet or StrataHub reporting.

Owner split:

1. L8W/L8Z should ensure the raw facts exist and are budget/source guarded.
2. L9 should decide the stable storage diagnostics surface.
3. Engine-next/CLI should decide product wording and presentation.

## Object-Store And Remote Work

These are intentionally outside local storage V1:

1. OpenDAL/S3 production durability;
2. object-store multi-writer fencing;
3. distributed lease/lock renewal;
4. remote manifest CAS protocol;
5. cloud-side merge or conflict resolution;
6. StrataHub push/pull protocol;
7. hosted telemetry and fleet health.

Local storage may keep provider-neutral facts that make this future work
possible, but it must not claim these guarantees in V1.

## Exit Criteria For This Ledger

This ledger is current when:

1. every known storage gap is assigned to L8Q-L8Z, L9, a product milestone, or
   post-V1 storage work;
2. optimized bulk-load/import mode is explicitly deferred unless a later
   milestone creates a dedicated slice;
3. minimal checkpoint/WAL-growth protection is assigned to L8Z while richer
   checkpoint scheduling remains post-V1;
4. physical format freeze and backwards compatibility is assigned to L10;
5. object-store/distributed durability remains outside V1 claims;
6. observability is split between raw storage facts and product diagnostics;
7. future closeout reviews update this ledger instead of leaving implicit gaps.
