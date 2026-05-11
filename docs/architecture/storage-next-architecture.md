# Storage-Next Architecture

Status: V1 architecture draft

## Purpose

This document defines the conceptual architecture for `storage-next`.

It does not define Rust traits, module names, file formats, migration steps, or
implementation milestones. Its job is to define the storage layers Strata wants
before we start designing repeatable patterns or moving code.

The current storage crate map is documented in
[docs/storage/storage-crate-map.md](../storage/storage-crate-map.md). That map
is evidence. This document is the target layering model.

Detailed layer documents live under
[storage-next/](./storage-next/README.md). Those documents refine this map one
layer at a time.

Cross-cutting implementation guidance lives in
[storage-next/implementation-patterns.md](./storage-next/implementation-patterns.md).
That document records the current consistency checkpoint and the repeatable
type, trait, error, and test patterns storage-next should prefer.

The target crate/module layout and local test harness invocation model live in
[storage-next/target-crate-shape-and-test-harness.md](./storage-next/target-crate-shape-and-test-harness.md).
The L1-L9 names are conceptual layers, not intended Rust module names.

Durable storage-space allocation and timeline placement are pinned in:

1. [storage-next/storage-space-id-registry.md](./storage-next/storage-space-id-registry.md)
2. [storage-next/commit-timeline-substrate.md](./storage-next/commit-timeline-substrate.md)

## Related Documents

Product and architecture anchors:

1. [strata-v1-product-requirements.md](../product/strata-v1-product-requirements.md)
2. [strata-v1-architecture.md](./strata-v1-architecture.md)
3. [core-next-architecture.md](./core-next-architecture.md)
4. [stratahub-substrate-architecture.md](./stratahub-substrate-architecture.md)
5. [runtime-resource-profile-architecture.md](./runtime-resource-profile-architecture.md)

Current storage evidence:

1. [storage-crate-map.md](../storage/storage-crate-map.md)
2. [storage-charter.md](../storage/storage-charter.md)
3. [v1-storage-consumption-contract.md](../storage/v1-storage-consumption-contract.md)
4. [storage-engine-ownership-audit.md](../storage/storage-engine-ownership-audit.md)

Historical context:

1. [next-charter.md](./next-charter.md)

`next-charter.md` is explicitly superseded historical context. This document
follows the V1 product and architecture anchors when they conflict.

## Product Constraints

Storage-next exists to serve the V1 product model.

The important constraints are:

1. Strata is embedded first.
2. Durable local filesystem is the reference backend.
3. Cache mode is explicit and non-durable.
4. IPC is required for V1, but IPC belongs to engine, not storage.
5. Follower mode is not a V1 product path.
6. Storage portability is required.
7. OpenDAL can become an adapter family later, but Strata owns its storage
   capability contract and does not require OpenDAL for the first storage
   rewrite.
8. Storage is data-capability agnostic.
9. Users should not manually flush, compact, checkpoint, prune, or recover
   during normal use.
10. Public transaction commands are not a V1 product requirement.
11. V1 physical format changes must be explicit and specified.
12. The same binary must run from constrained edge devices to server-class
    machines through resolved runtime budgets supplied by engine-next.

The last point is important. Storage-next should be architected so format and
backend evolution are possible, but this document does not authorize hidden
changes to the WAL, manifest, checkpoint, snapshot, or row encodings as a side
effect of crate cleanup. Format changes need their own design.

Clarification after the L1-L9 pass: storage-next is allowed to define a
storage-row-native V1 format if the row/key/commit architecture requires it.
That is not incidental churn; it must be written in the storage format spec.
Because Strata is pre-launch, the default cutover decision is to reject pre-V1
development databases during normal open rather than preserve old format
compatibility.

## Binding Storage-Next Decisions

The layer documents resolve several choices that were open in the first pass.

1. **Storage modes.**
   V1 storage-next must implement cache mode and durable local filesystem mode.
   Object-store/OpenDAL durability is architecture-aware but not required as a
   production mode in the first rewrite.

2. **Cache mode.**
   Cache mode has L6 branch state and L7 commit/version/timestamp mechanics in
   memory. It has no durable WAL, MANIFEST, snapshot, checkpoint, or table
   object service. It may report ephemeral runtime identity for diagnostics, but
   persistent instance identity is engine/product metadata.

3. **Durability mode axes.**
   Storage-next preserves the product modes `cache`, `standard`, and `always`
   by splitting them into `StorageMode` and `DurabilityPolicy`. `cache` maps to
   `StorageMode::Cache` and has no durable policy. `standard` and `always` map
   to `StorageMode::Durable` with `DurabilityPolicy::Standard` or
   `DurabilityPolicy::Always`.

4. **Backend publish primitive.**
   L1 exposes a backend-owned durable publish/conditional publish contract.
   Local filesystem implements that with temp-write, sync, rename, and
   directory sync internally. L4 consumes publish outcomes; higher layers do not
   hand-roll POSIX publish sequences.

5. **Physical storage space.**
   Storage-next replaces primitive-aware `TypeTag` ownership with an opaque
   storage space/family id supplied by engine. Storage may route by the byte; it
   must not know whether the byte means KV, JSON, graph, vector, event, search,
   or a future capability. Storage reserves `0x00..=0x1f` for storage-internal
   row families, with `0x01` assigned to the commit timeline. Engine owns
   `0x20..=0xff` and must publish its own product-space registry before V1
   format freeze.

6. **Entity references.**
   `EntityRef` is engine-owned. Engine encodes entity references into storage
   keys or values when needed, and maintains any reverse maps as ordinary
   engine-owned rows. Storage does not need to reconstruct an `EntityRef`.

7. **Commit timeline.**
   L7 assigns one timestamp per commit and storage persists a per-branch commit
   timeline sufficient to resolve timestamps to retained commit versions.
   Product `as_of` and branch-from-time semantics live in engine, but the
   generic timeline substrate is storage-owned. The timeline is stored as
   storage-owned system rows under `storage_space_id = 0x01`, not as a separate
   L2 object family or L4 service.

8. **Snapshot boundary.**
   Core durable snapshots are row-native storage snapshots. Opaque engine-owned
   snapshot sections are allowed only for derived or rebuildable engine state
   and must not be required for recovering committed storage rows.

9. **Merge semantics.**
   Storage-next does not implement CRDT/HLC merge. V1 merge strategies are
   engine-owned product behavior over storage rows and retained history.

10. **Sync.**
   Sync data movement is not a storage-next layer. V1 storage exposes
   capability, health, identity, bundle, and clone substrate needed for future
   StrataHub and sync work. A future sync layer must consume engine-owned
   semantics unless a documented diagnostic/migration exception is approved.

11. **Terminology.**
    Current code uses "segment" for immutable KV table files and
    `SegmentedStore` for the branch-aware LSM runtime. Storage-next uses
    "table" for the same immutable-table concept. This is a rename for clarity,
    not a second durable object family.

12. **Primitive extension surface.**
    Future primitives extend storage by choosing storage spaces, key encodings,
    row values, secondary rows, and derived indexes in engine. They do not add
    new storage layers.

13. **Runtime resource budgets.**
    Storage-next does not detect host hardware or classify devices. Engine-next
    supplies a resolved storage runtime budget; storage-next owns storage-local
    spending across table cache, mutable-table sizing, table output targets,
    compaction rate, pressure facts, and maintenance scheduling.

## Current Codebase Scan

The current `crates/storage` scan shows these major responsibility clusters:

1. Public storage contract:
   - `lib.rs`
   - `traits.rs`
   - `layout.rs`
   - `error.rs`

2. MVCC and branch runtime:
   - `segmented/mod.rs`
   - `segmented/compaction.rs`
   - `segmented/recovery.rs`
   - `segmented/quarantine_protocol.rs`
   - `segmented/ref_registry.rs`

3. Mutable and immutable table mechanics:
   - `memtable.rs`
   - `segment.rs`
   - `segment_builder.rs`
   - `key_encoding.rs`
   - `stored_value.rs`
   - `merge_iter.rs`
   - `seekable.rs`
   - `index.rs`
   - `bloom.rs`
   - `block_cache.rs`
   - `ttl.rs`

4. Transaction and commit runtime:
   - `txn/context.rs`
   - `txn/manager.rs`
   - `txn/validation.rs`
   - `txn/lock_ordering.rs`
   - `durability/commit_adapter.rs`

5. Durable services and formats:
   - `durability/wal/`
   - `durability/format/`
   - `durability/codec/`
   - `durability/disk_snapshot/`
   - `durability/compaction/`
   - `durability/layout.rs`
   - `durability/payload.rs`
   - `durability/recovery.rs`
   - `durability/recovery_bootstrap.rs`
   - `durability/checkpoint_runtime.rs`

6. Operational support:
   - `runtime_config.rs`
   - `pressure.rs`
   - `rate_limiter.rs`
   - `memory_stats.rs`
   - `contention.rs`
   - `quarantine.rs`
   - `manifest.rs`
   - `test_hooks.rs`

The scan changes the first-pass layer model in four ways:

1. Quarantine, ref tracking, and segment manifests are first-class recovery and
   retention mechanics, not miscellaneous helpers.
2. TTL is part of the table/runtime visibility and compaction contract, not a
   detached feature.
3. Contention profiling, memory stats, pressure, and rate limiting are
   observability/control surfaces that must be designed deliberately.
4. Follower state paths still exist in current storage layout code, but
   follower mode is not a storage-next concept.

## Target Layer Stack

The storage-next stack is ordered from bottom to top:

```text
strata-engine-next
        |
        v
+------------------------------------------------+
| L9. Storage API Boundary                       |
+------------------------------------------------+
| L8. Lifecycle / Recovery / Maintenance         |
+------------------------------------------------+
| L7. Commit Runtime                             |
+------------------------------------------------+
| L6. Branch-Isolated LSM Runtime                |
+------------------------------------------------+
| L5. Table Runtime                              |
+------------------------------------------------+
| L4. Log / Manifest / Snapshot Services         |
+------------------------------------------------+
| L3. Durable Format / Codec Layer               |
+------------------------------------------------+
| L2. Object Layout Layer                        |
+------------------------------------------------+
| L1. Backend IO Layer                           |
+------------------------------------------------+
        |
        v
storage backend
```

The core rule is:

> A lower layer must not know the concepts of a higher layer.

For example, backend IO must not know manifests. Formats must not know recovery
policy. Table code must not know product branch commands. Storage APIs must not
expose internal WAL or segment implementation details unless the exposed type is
part of the storage contract.

## L1. Backend IO Layer

This is the portability layer.

Detailed design: [storage-next/l1-backend-io.md](./storage-next/l1-backend-io.md).

It owns access to storage backends:

- local filesystem
- memory/test backends
- browser/WASM storage direction
- future OpenDAL-backed adapters
- future custom providers

It exposes backend operations in Strata's own terms:

- read object/range
- write object
- conditional publish where supported
- list prefix
- delete object
- create namespace/prefix if required
- sync/durability barrier where available
- lock/lease/capability hooks where available
- backend capability declaration

It must not know:

- branches
- versions
- WAL records
- manifest fields
- snapshots
- table segments
- engine primitives
- product policies

The local filesystem provider is the only place storage-next should call
`std::fs` in non-test code. Other layers should consume this backend interface.

## L2. Object Layout Layer

This layer maps database-relative concepts to backend object names.

Detailed design: [storage-next/l2-object-layout.md](./storage-next/l2-object-layout.md).

It owns:

- database root layout
- WAL object names
- segment/table object names
- manifest object names
- snapshot/checkpoint object names
- temporary object names
- quarantine object names
- lock/lease object names where a backend requires them

It should replace scattered path construction with a single layout contract.

Current evidence:

- `durability/layout.rs`
- `manifest.rs`
- snapshot path helpers in `durability/format/snapshot.rs`
- WAL segment filename parsing in `durability/wal/mod.rs`

Storage-next must not carry follower-state or follower-audit layout concepts.
Follower mode is not a V1 pathway.

## L3. Durable Format / Codec Layer

Detailed design: [storage-next/l3-durable-format-codec.md](./storage-next/l3-durable-format-codec.md)

This layer owns bytes.

It owns:

- WAL segment framing
- WAL record encoding
- manifest encoding
- snapshot header and section encoding
- segment metadata encoding
- writeset or commit-payload encoding
- checksums
- compression hooks
- encryption hooks
- format version checks
- strict decode behavior

It must not decide:

- when to checkpoint
- how to recover
- what a branch means
- whether a primitive section should exist
- how engine interprets a row

Current evidence:

- `durability/format/wal_record.rs`
- `durability/format/manifest.rs`
- `durability/format/snapshot.rs`
- `durability/format/segment_meta.rs`
- `durability/format/writeset.rs`
- `durability/format/watermark.rs`
- `durability/codec/`
- `durability/payload.rs`

Current storage also contains primitive snapshot DTOs and primitive section
tags. Storage-next should not treat those as proof that storage owns primitive
semantics. Stable V1 committed snapshots should be row-native storage state.
Primitive snapshot DTOs are current-code evidence only, not a V1 migration
format or storage-owned payload family.

## L4. Log / Manifest / Snapshot Services

Detailed design: [storage-next/l4-log-manifest-snapshot-services.md](./storage-next/l4-log-manifest-snapshot-services.md)

This layer turns backend IO, object layout, and durable byte formats into
usable durable services.

It owns:

- append/read WAL service
- manifest read/update/publish service
- snapshot write/read service
- checkpoint file publication mechanics
- WAL truncation and safe deletion mechanics
- durable tombstone or retention-proof publication mechanics, if retained
- durable cleanup primitives
- raw watermarks and durable file facts

It must not own:

- public database lifecycle policy
- user-facing checkpoint commands
- engine primitive checkpoint content
- product recovery messages

Current evidence:

- `durability/wal/`
- `durability/disk_snapshot/`
- `durability/compaction/`
- `durability/checkpoint_runtime.rs`
- `durability/format/manifest.rs`

This is the layer where the current filesystem-backed services should become
backend-backed services over L1/L2.

## L5. Table Runtime

Detailed design: [storage-next/l5-table-runtime.md](./storage-next/l5-table-runtime.md).

This is the table-primitives layer.

L5 is deliberately not Strata's full LSM architecture. It owns the reusable
mutable-table, immutable-table, iterator, cache, index, filter, and table
compaction machinery. The branch-aware LSM forest begins in L6, where those
table primitives are assembled into branch-local levels, inherited COW layers,
and MVCC visibility rules.

It owns:

- memtables
- frozen memtables
- immutable tables/segments
- ordered table-key comparison over bytes
- stored value representation
- tombstones
- TTL mechanics
- table builders
- table readers
- block cache
- bloom filters
- indexes
- merge cursors
- immutable-table construction from sorted/frozen entries
- generic table compaction algorithms

It should know about ordered table keys and stored row bytes. It may preserve
version metadata inside row values when the format requires it, but it must not
own the branch/version meaning of those bytes.

It must not know:

- branch-local level ownership
- copy-on-write inherited branch layers
- fork-version visibility gates
- branch materialization
- public branch commands
- graph/vector/search/JSON/event semantics
- IPC
- StrataHub
- inference
- backend-specific syscall behavior

Current evidence:

- `memtable.rs`
- `segment.rs`
- `segment_builder.rs`
- `key_encoding.rs`
- `stored_value.rs`
- `merge_iter.rs`
- `seekable.rs`
- `index.rs`
- `bloom.rs`
- `block_cache.rs`
- `ttl.rs`
- `compaction.rs`
- table-oriented parts of `segmented/compaction.rs`

The current process-global block cache should be replaced here. Storage-next
should make table/block cache ownership database-local for V1. Any future
provider-local or shared cache must be an explicit design, not hidden
process-global state.

The cache size should come from the resolved storage runtime budget, not from
table code probing host hardware or using process-global auto-detection.

## L6. Branch-Isolated LSM Runtime

Detailed design:
[storage-next/l6-branch-isolated-lsm-runtime.md](./storage-next/l6-branch-isolated-lsm-runtime.md).

This is where Strata's storage identity begins.

L6 owns the true Strata LSM shape: a branch-indexed MVCC LSM forest. Each
branch has its own mutable/frozen table state and leveled immutable table view.
Forks attach inherited immutable table layers from ancestor branches, capped by
a fork-version frontier. Reads merge child-local data with inherited layers
without copying every row, while materialization can later turn inherited state
into child-owned tables.

It owns generic mechanics for:

- branch IDs
- branch-local physical state
- branch-local active/frozen table ownership
- branch-local immutable table levels
- branch-aware row-key construction
- commit/version IDs
- MVCC visibility
- version-bounded reads for product `getv`
- timestamp-bounded reads for product `as_of` over storage commit timestamps
- history reads
- prefix/range scans
- copy-on-write branch layers
- inherited layer key rewriting
- fork-version visibility gates
- inherited layer materialization
- branch-local segment retention facts
- shared table reachability/refcount facts
- snapshot-row install into generic storage rows

Storage can own branch mechanics. Engine owns branch product semantics.

Storage may know:

- a branch has a durable ID
- branch state can inherit physical rows from another branch
- a branch can be materialized
- a read can target a commit version
- a read can target a storage commit timestamp

Storage must not know:

- merge meaning
- cherry-pick behavior
- revert UX
- branch comparison presentation
- graph-aware or primitive-aware diffs
- product branch names beyond opaque validation required for storage layout

Current evidence:

- `segmented/mod.rs`
- `segmented/quarantine_protocol.rs`
- `segmented/ref_registry.rs`
- `segmented/recovery.rs`
- `durability/decoded_snapshot_install.rs`

## L7. Commit Runtime

This layer owns the internal commit unit.

It owns:

- transaction/commit IDs where storage-local
- version allocation
- commit ordering
- branch commit locks
- commit quiescing
- write conflict validation if retained
- WAL-before-visible discipline
- batch apply
- commit visibility
- branch deletion barriers
- lock-order contracts

This does not imply public user transactions. Product documentation has already
moved away from exposing manual transaction commands. Storage still needs an
internal commit unit so writes can be ordered, made durable, and made visible
correctly.

Current evidence:

- `txn/context.rs`
- `txn/manager.rs`
- `txn/validation.rs`
- `txn/lock_ordering.rs`
- `durability/commit_adapter.rs`

Storage-next should make the commit unit a central concept, not a public
transaction product.

## L8. Lifecycle / Recovery / Maintenance

This is the top storage-internal orchestration layer.

Detailed design:
[storage-next/l8-lifecycle-recovery-maintenance.md](./storage-next/l8-lifecycle-recovery-maintenance.md).

It owns:

- storage open mechanics below engine policy
- raw recovery execution
- WAL replay
- snapshot install orchestration
- segment recovery
- checkpoint mechanics
- compaction scheduling hooks
- retention/pruning mechanics
- quarantine and repair mechanics
- shutdown sync mechanics
- raw storage health facts
- raw storage metrics

It may coordinate lower layers:

- backend IO
- layout
- WAL
- manifest
- snapshot
- table runtime
- branch runtime
- commit runtime

It must not own:

- public database open policy
- IPC
- product recovery UX
- product lifecycle commands
- public maintenance workflows
- engine primitive reconstruction

Current evidence:

- `durability/recovery.rs`
- `durability/recovery_bootstrap.rs`
- `durability/checkpoint_runtime.rs`
- `segmented/recovery.rs`
- `segmented/quarantine_protocol.rs`
- `quarantine.rs`
- `pressure.rs`
- `rate_limiter.rs`
- `memory_stats.rs`
- `contention.rs`

This is where today's maintenance mechanics should become automatic,
observable database internals.

## L9. Storage API Boundary

This is the only normal production surface consumed by engine-next.

Boundary design:
[storage-next/l9-storage-api-boundary.md](./storage-next/l9-storage-api-boundary.md).

It should expose storage capabilities in storage language:

- open/create storage runtime from a backend and config
- commit a batch
- read latest
- read by version for product `getv`
- read by timestamp for product `as_of` over storage commit timestamps
- scan by physical key range/prefix
- read history
- fork/materialize branch mechanics
- checkpoint through a primitive-neutral payload boundary
- report raw recovery outcomes
- expose maintenance drain/status/control hooks
- expose raw health/metrics
- close/shutdown storage safely

It should not expose:

- table file internals
- WAL record internals
- manifest mutation internals
- block cache mutation internals
- primitive section DTOs as product concepts
- public transaction sessions
- follower refresh
- IPC behavior

Engine-next should be the only normal production crate that consumes this
boundary directly.

## Cross-Cutting Test And Fault Framework

Testing is not a layer in the stack. It cuts across every layer.

Storage-next must be designed so each layer has direct tests:

1. Backend conformance tests.
2. Object layout tests.
3. Format roundtrip and fuzz tests.
4. Codec compatibility and corruption tests.
5. WAL append/read/truncation tests.
6. Manifest publish/failure tests.
7. Snapshot roundtrip tests.
8. Table builder/reader/property tests.
9. MVCC visibility tests.
10. Branch COW/materialization tests.
11. Commit ordering/conflict tests.
12. Crash recovery tests.
13. Quarantine/repair tests.
14. Maintenance scheduling tests.
15. Metrics and health fact tests.

Fault injection must be first-class:

- failed read
- failed write
- failed publish/rename/conditional write
- torn object
- stale manifest
- checksum mismatch
- codec failure
- WAL gap
- WAL tail corruption
- partial snapshot
- segment missing
- segment corrupt
- fsync/sync failure where the backend exposes sync
- backend capability mismatch
- process crash between every durable state transition

The test framework should not depend on engine data capability semantics. Engine can
have its own product-path tests above storage-next.

## Layer Placement Rules

1. Backend IO does not know Strata database concepts.
2. Object layout does not parse durable formats.
3. Durable formats do not perform IO.
4. Durable services do not interpret engine primitives.
5. Table runtime does not call backend IO directly.
6. Versioned branch runtime does not own product branch workflows.
7. Commit runtime does not expose public user transactions.
8. Lifecycle/recovery does not own public engine policy.
9. The storage API boundary hides implementation detail unless the detail is a
   stable storage contract.
10. Test/fault hooks are explicit and feature-gated or test-scoped.

## Current Code To Target Layer Mapping

| Current area | Target layer | Notes |
|---|---|---|
| `block_cache.rs` | L5 | Replace process-global state with database-local cache ownership. |
| `bloom.rs` | L5 | Table-local read optimization. |
| `compaction.rs` | L5/L8 | Split table algorithms from scheduling/lifecycle. |
| `contention.rs` | Cross-cutting/L8 | Observability, not core runtime logic. |
| `error.rs` | L9/cross-cutting | Storage-local errors should remain storage-owned. |
| `index.rs` | L5 | Table/index support. |
| `key_encoding.rs` | L5/L6 | Internal key ordering; branch/version encoding crosses L6. |
| `layout.rs` | L6/L9 | Physical keyspace; revisit which IDs belong in core-next. |
| `manifest.rs` | L2/L4/L6/L8 | Segment manifest mixes layout, durable publication, branch/table reachability, and recovery proof. |
| `memory_stats.rs` | Cross-cutting/L8 | Raw metrics. |
| `memtable.rs` | L5 | Mutable table. |
| `merge_iter.rs` | L5/L6 | Generic sorted merge belongs in L5; MVCC and inherited-layer rewriting belong in L6. |
| `pressure.rs` | L8 | Maintenance trigger/control fact. |
| `quarantine.rs` | L8 | Recovery/repair support. |
| `rate_limiter.rs` | L8 | Maintenance control. |
| `runtime_config.rs` | L9/L8 | Boundary config plus internal runtime application. |
| `seekable.rs` | L5/L6 | Raw cursor mechanics belong in L5; MVCC and inherited-layer wrappers belong in L6. |
| `segment.rs` | L5 | Immutable table reader. |
| `segment_builder.rs` | L5 | Immutable table writer. |
| `segmented/mod.rs` | L5/L6/L7/L8 | Needs decomposition; currently owns table primitives, branch LSM state, commit apply, and lifecycle helpers. |
| `segmented/compaction.rs` | L5/L6/L8 | Table compaction algorithms belong in L5, branch-level selection/state mutation in L6, scheduling policy in L8. |
| `segmented/quarantine_protocol.rs` | L6/L8 | Branch/table reachability proof plus retention/quarantine mechanics. |
| `segmented/recovery.rs` | L8 | Raw recovery health/fault facts. |
| `segmented/ref_registry.rs` | L6/L8 | Shared table reachability for COW branches, rebuilt and used by recovery/cleanup. |
| `stored_value.rs` | L5/L6 | Storage row value and MVCC metadata. |
| `test_hooks.rs` | Cross-cutting | Should become explicit fault framework. |
| `traits.rs` | L9 | Public storage boundary, likely redesigned. |
| `ttl.rs` | L5/L8 | Visibility/compaction behavior plus maintenance trigger. |
| `txn/` | L7 | Internal commit unit, not public transaction product. |
| `durability/codec/` | L3 | Byte transformation. |
| `durability/format/` | L3 | Durable bytes; primitive DTOs need boundary review. |
| `durability/wal/` | L4 | Durable log service. |
| `durability/disk_snapshot/` | L4/L8 | Snapshot service plus checkpoint orchestration. |
| `durability/compaction/` | L4/L8 | WAL truncation/deletion service mechanics; table compaction stays in L5/L8. |
| `durability/layout.rs` | L2 | Replace path-specific API with backend object layout. |
| `durability/payload.rs` | L3/L7 | Commit payload encoding. |
| `durability/commit_adapter.rs` | L7/L4 | WAL-before-visible bridge. |
| `durability/recovery*.rs` | L8 | Recovery orchestration. |
| `durability/checkpoint_runtime.rs` | L8/L4 | Split orchestration from durable services. |

## What Storage-Next Must Exclude

Storage-next must not include:

1. JSON document semantics.
2. Event chain product semantics.
3. Vector collection or embedding semantics.
4. Graph ontology, traversal, analytics, or relationship semantics.
5. Search ranking, indexing policy, BM25 semantics, or RAG behavior.
6. Strata AI behavior.
7. IPC server/client behavior.
8. StrataHub dataset or fleet behavior.
9. Follower mode.
10. Branch bundle workflows.
11. Public manual transaction sessions.
12. User-operated flush/compact/checkpoint/maintenance commands.
13. Host hardware detection, product resource-profile classification, or
    graph/search/vector/intelligence budget policy.

Storage-next may persist rows that engine uses for those features. It does not
define their meaning.

## Design Consequences

1. The current `SegmentedStore` should not be recreated as one large center of
   gravity. Its responsibilities map across L5, L6, L7, and L8.
2. The current durability subtree should not be copied wholesale. It maps
   across L2, L3, L4, L7, and L8.
3. The storage API should be designed after the lower layers are named, not
   before.
4. Backend portability must start at L1/L2, not as an adapter bolted onto
   filesystem-shaped services.
5. Testing must be designed per layer from the start.
6. Physical format compatibility must be explicit. Ownership cleanup must not
   hide format changes, and any row-native format revision needs a format spec.
   Pre-v1 development formats are rejected by default.
7. Product semantics should move upward or stay upward even if doing so makes
   storage APIs less convenient.
8. Storage runtime configuration should consume resolved budgets rather than
   mutate product config or inspect the host directly.

## Remaining Architecture Inputs

The L1-L9 pass answered the first-order storage layering questions. The
remaining inputs should be resolved by the detailed implementation roadmap and
the engine-next architecture:

1. Which identifiers and version types belong in core-next versus storage-next?
2. What is the exact backend capability vocabulary for local FS, memory/cache,
   browser/WASM, and future object/OpenDAL-backed providers?
3. What exact engine-owned storage-space ID assignments live in
   `0x20..=0xff`?
4. What is the minimal L9 API engine-next needs for branch/time-travel
   mechanics without leaking product branch semantics down?
5. Which current hardening structs are real implementation contracts and which are
   cleanup-era scaffolding?
6. Which concurrency testing tool or deterministic scheduler should L7 use?
7. What exact resolved storage budget shape does engine-next pass through L9?

## Next Step

The next document should be the storage-next implementation roadmap. It should
order the work, pair each implementation phase with a concrete test plan, and
avoid introducing temporary facades that exist only to keep an incomplete crate
compiling.
