# Storage-Next Consistency And Implementation Patterns

Status: architecture checkpoint

## Purpose

This document records the consistency pass across the V1 product documents,
high-level V1 architecture, core-next architecture, storage-next layer
documents, the current storage consumption contract, and the draft storage
format spec.

It also defines implementation patterns for storage-next. The goal is to avoid
recreating the current problem: many one-off structs, enums, managers,
coordinators, and adapters that are individually reasonable but collectively
hard to understand, test, and evolve.

This is not a Rust API spec. Exact signatures should be written when each layer
is implemented. These notes define the vocabulary and repeatable shapes we
should prefer.

## Consistency Checkpoint

The current documents are aligned on the major product and architecture
decisions:

1. Storage is below engine. Engine-next is the only normal production consumer
   of storage-next.
2. Storage-next owns persistence mechanics, not product semantics.
3. KV, JSON, events, graph, vectors, search, recipes, RAG, and Strata AI remain
   above storage.
4. Local filesystem is the durable reference backend.
5. Cache/browser mode is explicit and non-durable.
6. OpenDAL and object storage are design constraints, not a reason to block the
   first storage rewrite on a production S3 backend.
7. IPC is required for V1, but belongs at engine/product boundaries, not in
   storage.
8. Follower mode is not a V1 target.
9. Users should not manually flush, compact, checkpoint, prune, or recover
   during normal use.
10. Public begin/commit/rollback transaction commands are not V1 product
    requirements, but storage still needs an internal commit unit.
11. The first storage-next pass is an ownership and architecture cleanup, not a
    broad physical format redesign.
12. StrataHub depends on Strata identity, clone, bundle, health, and capability
    surfaces, but Strata storage must not embed StrataHub business logic.

Two current-code documents need special interpretation:

1. `docs/storage/v1-storage-consumption-contract.md` describes the current
   consolidated engine/storage boundary. Follower references in that file are
   transitional current-code allowances only and are superseded by
   `docs/engine/follower-mode-removal-plan.md`.
2. `docs/spec/strata-storage-format-v1.md` is an unstable draft. It documents
   current format evidence and target requirements, but it is not yet a
   compatibility promise.

## Naming Discipline

New storage-next types should fall into a small set of names. If a proposed
type does not fit one of these categories, the implementation should justify
why a new shape is necessary.

Use these suffixes deliberately:

1. `Id`: opaque identity with stable equality and serialization.
2. `Name`: validated name or object-name component.
3. `Address`: parsed user or backend address.
4. `Key`: ordered storage key or backend object key.
5. `Options`: caller-supplied knobs before execution.
6. `Config`: runtime configuration after validation or resolution.
7. `Plan`: preflighted work that has not mutated state yet.
8. `Record`: durable log/table/snapshot unit.
9. `Entry`: key/value or iterator item.
10. `Facts`: observed durable state, recovery facts, or backend capability
    facts.
11. `Outcome`: result of an operation that performed work.
12. `Stats`: counters only.
13. `Report`: diagnostic or user-facing summary.
14. `Error`: typed failure for a layer or service.

Avoid using these names as vague defaults:

1. `Manager`, unless the type owns a long-lived resource and lifecycle.
2. `Coordinator`, unless the type orders multiple lower-layer services.
3. `Runtime`, unless the type owns active mutable execution state.
4. `Context`, unless it carries scoped execution dependencies and no business
   behavior.
5. `Helper`, `Util`, `Adapter`, or `Facade`, unless there is no clearer domain
   noun.

The name should say what the type owns. A type named only for the code it
calls is usually a design smell.

## Layer Vocabulary

### L1 Backend IO

Preferred concepts:

- `Backend`
- `BackendCapabilities`
- `BackendObject`
- `BackendMetadata`
- `BackendError`
- `FaultBackend` in testkit

Trait guidance:

`Backend` is a real trait boundary because storage-next needs multiple backend
families: cache/browser, local filesystem, and future object/OpenDAL-backed
providers.

The trait should be capability-driven. A method compiling does not mean a
backend can satisfy every storage mode. Open should validate capabilities
against the requested mode before exposing a database.

### L2 Object Layout

Preferred concepts:

- `ObjectName`
- `ObjectPrefix`
- `ObjectFamily`
- `Layout`
- `LayoutError`

Trait guidance:

Layout should usually be concrete, not a trait. It is Strata's namespace
contract. Backend-specific path or key translation belongs under L1.

Rules:

1. Layers above L2 should not format object paths with strings.
2. Object names are database-relative.
3. Local path conversion is an L1 local-filesystem concern.

### L3 Durable Format / Codec

Preferred concepts:

- `FormatVersion`
- `FormatCodec`
- `CodecId`
- `Encoder`
- `Decoder`
- `DecodeError`
- `StrictDecode`
- `GoldenVector`

Trait guidance:

Use traits for codec families and test conformance, not for every format
record. Many format encoders can be simple associated functions or concrete
modules.

Rules:

1. L3 never performs IO.
2. L3 never decides recovery policy.
3. L3 never owns engine data capability semantics.
4. Strict decode behavior should be explicit and tested.

### L4 Log / Manifest / Snapshot Services

Preferred concepts:

- `DurablePublisher`
- `PublishPlan`
- `PublishOutcome`
- `PublishWindow`
- `WalService`
- `ManifestService`
- `SnapshotService`
- `ServiceHealth`
- `ServiceError`

Trait guidance:

`DurablePublisher` is the repeatable primitive. WAL, manifest, snapshot,
table-object, quarantine, and sidecar publication should consume it instead of
reimplementing temp-write, sync, rename, directory-sync, conditional publish,
or cleanup logic.

`WalService`, `ManifestService`, and `SnapshotService` can start as concrete
services over a backend and publisher. Introduce traits only when conformance
tests, fault wrappers, or multiple implementations require them.

Rules:

1. L4 owns durable publication mechanics.
2. L4 does not schedule checkpoint, retention, or recovery policy.
3. L4 does not interpret snapshot section payloads.
4. L4 WAL truncation/deletion is separate from L5 table compaction.

### L5 Table Runtime

Preferred concepts:

- `MemTable`
- `FrozenTable`
- `TableBuilder`
- `TableReader`
- `TableIndex`
- `BloomFilter`
- `BlockCache`
- `TableCompactor`
- `TableStats`

Trait guidance:

Keep read/write algorithms concrete until there are multiple table formats or
test backends that need a trait. The key abstraction here is not a broad
`StorageEngine`; it is small table readers, builders, iterators, and caches
that can be tested directly.

Rules:

1. L5 works with ordered table-key bytes and stored row bytes.
2. L5 may produce immutable table objects, but L4 publishes them.
3. L5 compaction is table compaction, not WAL deletion.
4. L5 does not own branch-local level state, inherited COW layers, or
   fork-version visibility gates.

### L6 Branch-Isolated LSM Runtime

Preferred concepts:

- `StorageRow`
- `RowKey`
- `RowValue`
- `VersionedRow`
- `BranchState`
- `BranchLayer`
- `BranchTableSet`
- `InheritedLayer`
- `Visibility`
- `HistoryCursor`
- `RangeCursor`
- `MaterializationPlan`
- `InstallPlan`

Trait guidance:

Avoid one trait per read path. Latest, `getv`, `as_of`, history, and scans
should be different queries over one versioned row model unless a later design
proves otherwise.

L6 is the first layer that may assemble L5 table primitives into Strata's
branch-aware LSM forest: branch-local active/frozen tables, branch-local
immutable levels, inherited COW layers, fork-version gates, inherited key
rewriting, and lazy materialization.

Rules:

1. L6 can know branch IDs and commit versions.
2. L6 can route by opaque storage space ids but cannot know product data
   capability meaning.
3. L6 owns storage commit timestamp visibility and branch timeline facts needed
   for timestamp-to-version resolution.
4. L6 cannot know branch product workflows like merge UX, cherry-pick meaning,
   graph-aware diff, or restore presentation.
5. L6 should make versioned-row behavior testable without engine primitives.
6. L6 owns branch/table reachability facts needed for COW safety; L8 may use
   those facts during recovery, retention, quarantine, and repair.

### L7 Commit Runtime

Preferred concepts:

- `CommitBatch`
- `CommitId`
- `CommitVersionAllocator`
- `CommitValidator`
- `CommitGuard`
- `CommitOutcome`
- `Conflict`

Trait guidance:

The commit runtime is an internal storage contract. Do not expose public
transaction-session traits by default. Batch atomicity, durability, and
conflict behavior should be explicit operation facts.

Rules:

1. L7 owns WAL-before-visible discipline.
2. L7 owns storage-local commit ordering.
3. L7 owns one commit timestamp per committed batch.
4. L7 owns the storage-native per-branch commit timeline substrate.
5. L7 does not imply a public ACID transaction product.

### L8 Lifecycle / Recovery / Maintenance

Preferred concepts:

- `OpenPlan`
- `RecoveryPlan`
- `RecoveryOutcome`
- `MaintenancePlan`
- `MaintenanceOutcome`
- `RetentionPlan`
- `HealthFacts`
- `RepairPlan`
- `ShutdownOutcome`

Trait guidance:

L8 should coordinate lower-layer services. It should not hide them behind
catch-all lifecycle traits that are impossible to test. Prefer concrete
orchestrators with injectable lower-layer dependencies in testkit.

Rules:

1. L8 owns raw storage lifecycle sequencing.
2. L8 does not own engine product open policy.
3. L8 maintenance is automatic internal behavior with observability, not a
   normal user workflow.
4. L8 validates storage mode capabilities before durable side effects.
5. L8 treats cache mode as in-memory L6/L7 state with no durable services.

### L9 Storage API Boundary

Preferred concepts:

- `Storage`
- `StorageOptions`
- `StorageConfig`
- `StorageCapabilities`
- `ReadRequest`
- `ReadOutcome`
- `CommitRequest`
- `CommitOutcome`
- `StorageHealth`
- `StorageError`

Trait guidance:

L9 is the public storage-next boundary consumed by engine-next. It may be a
trait, a concrete handle, or both. The deciding factor should be testability
and backend substitution, not symmetry with the old storage crate.

Rules:

1. L9 exposes storage mechanics in storage language.
2. L9 hides WAL, table, manifest, block cache, and backend implementation
   detail by default.
3. L9 does not expose public transaction sessions, follower refresh, IPC, or
   engine primitive DTOs.

## Struct And Enum Rules

Use structs when the value crosses a layer boundary, needs validation, or
records durable facts. Use direct function parameters for small private helper
calls.

Preferred operation shapes:

1. `Options` for caller knobs that may be invalid.
2. `Config` for resolved runtime settings.
3. `Plan` for validated, preflighted work before mutation.
4. `Outcome` for facts after mutation or attempted work.
5. `Stats` only for counters.
6. `Report` only for diagnostic presentation.

Avoid creating a unique `FooArgs`, `FooInput`, `FooResult`, `FooRuntime`,
`FooContext`, and `FooState` for every operation. If the operation is private
and small, keep the function small instead.

Error enums should be typed at layer or service boundaries. They should:

1. Use `#[non_exhaustive]` on public enums.
2. Preserve source errors.
3. Include object names, branch IDs, versions, or publish windows when that
   information helps recovery or diagnostics.
4. Avoid embedding telemetry that callers ignore.
5. Avoid converting impossible states into defaults.

## Test Pattern

Storage-next testing should use repeatable harnesses instead of per-feature
bespoke tests.

Required test families:

1. Backend conformance suite for every backend.
2. Fault backend suite for read, write, list, publish, delete, sync, and stale
   metadata failures.
3. Object layout property tests for object names and prefixes.
4. Format golden-vector tests and strict decode fuzz targets.
5. Durable publisher failure-window tests.
6. WAL service roundtrip, corruption, and truncation tests.
7. Manifest publish and fencing tests.
8. Snapshot roundtrip and partial-publication tests.
9. Table builder/reader/compaction property tests.
10. Versioned-row visibility, history, and timestamp-bound tests.
11. Commit atomicity and conflict tests.
12. Recovery crash-window tests.
13. Maintenance scheduling and retention tests.
14. L9 API conformance tests against cache/browser and local filesystem.

Testkit should be a first-class module. Fault injection should be centralized
through backend and service wrappers, not scattered ad hoc hooks.

## Implementation Order Notes

The implementation should preserve the layer order:

1. Build testkit primitives early, especially the fault backend.
2. Implement L1 backend traits and local/cache backends.
3. Implement L2 object names and layout validation.
4. Stabilize L3 format APIs with golden vectors before broad L4 service work.
5. Implement `DurablePublisher`.
6. Build WAL, manifest, snapshot, and table-publication services on the
   publisher.
7. Build L5 table runtime over L4 publication instead of filesystem paths.
8. Build L6 versioned branch rows over L5.
9. Build L7 commit runtime over L4 and L6.
10. Build L8 recovery/maintenance over L4-L7.
11. Finalize L9 only after the lower layers prove the surface.

`RecoveryHealth`, `DegradationClass`, and `RecoveryFault` are storage-next-owned
V1 recovery facts. See
`docs/architecture/storage/l8-lifecycle-recovery-maintenance.md` and
`docs/architecture/core-architecture.md` for the ownership decision.

This order should keep storage-next from growing temporary facades just to make
the architecture compile.

## Review Checklist

For every new storage-next type or trait, ask:

1. Which layer owns this?
2. Which layer consumes this?
3. What higher-layer concept must it not know?
4. Is this a stable boundary type, or a private helper?
5. Can an existing `Options`, `Config`, `Plan`, `Outcome`, `Facts`, `Stats`, or
   `Report` shape cover it?
6. How is it tested without engine data capability semantics?
7. Does it work for both cache/browser and local filesystem?
8. Does it leave room for a future object/OpenDAL backend without forcing that
   backend into the first rewrite?
9. Does it expose a user maintenance task that product docs say should be
   automatic?
10. Does it make follower mode, public transactions, branch bundles, or
    StrataHub business logic leak into storage?
