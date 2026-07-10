# Engine-Next Persistence Adapter Contract

Status: V1 architecture draft

## Purpose

This document defines the engine-owned persistence adapter contract.

The persistence adapter is the only normal production path from engine-next to
storage-next L9. It turns engine product intent into storage-shaped rows and
turns storage-shaped facts back into engine diagnostics and raw row results.

It exists to keep these boundaries intact:

```text
capability, branch, retrieval, orchestration, control plane
  -> persistence adapter
  -> storage-next L9
```

not:

```text
capability, branch, retrieval, orchestration, control plane
  -> storage keys, storage-space bytes, commit batches, LSM/storage internals
```

Storage-next persists branch-aware MVCC KV rows. Engine-next owns product
meaning. The persistence adapter is the firewall between those two facts.

## Related Documents

Read this with:

1. `docs/architecture/engine-architecture.md`
2. `docs/architecture/engine/README.md`
3. `docs/architecture/engine/primitive-implementation-contract.md`
4. `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`
5. `docs/architecture/engine/storage-space-id-registry.md`
6. `docs/architecture/storage/l9-storage-api-boundary.md`
7. `docs/architecture/v1-error-and-diagnostics-contract.md`

Follow-up contracts that depend on this one:

1. Branch operation and capability adapter contract.
2. Temporal context and timeline resolver contract.
3. Control-plane layout contract.
4. Retrieval and derived-state contract.
5. Dataset clone artifact contract.
6. Product-pathway conformance plan.

## Requirement Language

1. Must means the persistence boundary is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

Current engine code does not yet have this boundary. Storage concepts appear
directly in many engine locations:

1. Capability code constructs storage `Key` values and chooses `TypeTag`s.
2. Branch operations scan and compare storage-shaped rows directly.
3. Snapshot install and checkpoint code route primitive-shaped sections into
   storage row installation.
4. Vector, graph, search, branch, recovery, and compaction code all contain
   storage-facing logic.
5. Storage errors are sometimes mapped near the callsite instead of through a
   single engine boundary.

That direct access was useful while making the existing engine work. It should
not be the target architecture.

Engine-next should preserve the good part of the current design:

```text
all product data eventually becomes branch-aware versioned rows
```

and remove the weak part:

```text
every subsystem learns how to talk to storage
```

## Definitions

### Persistence Adapter

The persistence adapter is the engine service that consumes storage-next L9.

It owns:

1. Storage-space registry consumption.
2. Physical row-key construction.
3. Storage commit batch construction.
4. Storage latest/version/timestamp/history reads.
5. Storage prefix/range scans.
6. Storage timeline resolution calls.
7. Storage branch-mechanic calls.
8. Storage open/create adaptation.
9. Storage checkpoint/maintenance/close adaptation.
10. Storage snapshot/recovery fact adaptation.
11. Storage health and metrics fact adaptation.
12. Storage test/fault hook adaptation for tests.
13. Storage error mapping into engine diagnostics.

It does not own product semantics.

### Row Address

A row address is the engine-side description of a physical row target.

It contains:

1. Branch identity.
2. Product space.
3. Symbolic engine storage-space assignment.
4. Capability-owned key bytes.

It does not expose raw storage-space numeric IDs to capability code.

The adapter resolves the symbolic storage-space assignment through the engine
storage-space ID registry, then constructs the physical key expected by
storage-next.

Capabilities define how branch and space participate in product identity. The
adapter requires explicit branch and space fields and encodes them into the
physical row key. It must not infer branch or space from ambient runtime state.

### Row Mutation

A row mutation is a put or delete against a row address.

It carries:

1. Row address.
2. Value bytes for puts.
3. Tombstone/delete intent for deletes.
4. Optional TTL or expiry metadata.
5. Optional expected source/control/derived classification for diagnostics.
6. Operation origin for diagnostics.

It does not carry JSON operations, graph edges, vector embeddings, event stream
semantics, search scoring facts, or public transaction state.

The engine storage-space ID registry is the authoritative source for
source/control/derived classification. If a caller supplies an expected
classification, the adapter treats it as an assertion and rejects mismatches
before storage mutation.

### Read Selector

A read selector describes the storage visibility boundary for a row read.

V1 selectors:

1. Latest visible state.
2. At or before commit version.
3. At or before commit timestamp.
4. Retained history for one row.

The product names `get`, `getv`, `as_of`, and `history` live above the adapter.
The adapter exposes storage-mechanical read selectors.

The version-bounded selector consumes a resolved branch frontier from the
temporal context layer. The temporal resolver validates that
`version:<commit-version>` names an exact retained branch timeline point; the
adapter then uses that commit version as the maximum visible row version.

### Scan Selector

A scan selector describes a deterministic prefix or range scan.

It contains:

1. Branch identity.
2. Product space.
3. Symbolic storage-space assignment.
4. Capability-owned prefix or range bytes.
5. Visibility selector.
6. Limit and pagination facts where retained.

The adapter constructs storage-shaped range bounds. Capability code owns the
meaning of the key prefix.

### Commit Plan

A commit plan is the engine's internal write unit before it becomes a storage
commit batch.

It contains:

1. Target branch for V1.
2. Row mutations.
3. Optional compare/CAS facts when a product operation requires them.
4. Operation origin for diagnostics.
5. Durability expectation inherited from the open runtime.
6. Write authority and access-mode facts inherited from runtime.

V1 commit plans are single-branch. Cross-branch atomic commits are deferred.

### Commit Outcome

A commit outcome is the engine-facing result of a storage commit.

It contains:

1. Committed version.
2. Commit timestamp.
3. Write and delete counts.
4. Durability classification.
5. Backpressure or stall facts where applicable.
6. Ambiguous-commit classification where storage cannot prove visibility.

Product services may attach product context to this outcome. They must not
reinterpret storage uncertainty as success.

## Binding Decisions

1. **Only persistence imports storage-next in normal production engine code.**
   Data capabilities, branch workflows, retrieval, orchestration, API, and IPC
   should not call storage-next L9 directly.

2. **Capability code owns key and value semantics, not physical storage keys.**
   A capability may encode its capability-local key bytes and value bytes. The
   adapter attaches branch, product space, and storage-space ID.

3. **Raw numeric storage-space IDs stay behind the registry.**
   Capability code uses symbolic assignments. The adapter resolves them through
   the central engine registry.

4. **Registry classification is authoritative.**
   Source/control/derived classification comes from the storage-space registry.
   Caller-supplied classification is an assertion, not a second source of
   truth.

5. **The adapter is the final write-access guard.**
   Capability code and API code should reject writes in read-only mode, but the
   adapter must also reject write commit plans for read-only, closed, closing,
   or otherwise write-disabled handles before storage mutation.

6. **The adapter is product-semantic blind.**
   It does not know JSON path behavior, vector distance metrics, graph
   traversal semantics, event ordering semantics beyond encoded bytes, or search
   ranking.

7. **The adapter is storage-internal blind below L9.**
   It does not know WAL record bytes, manifest mutation APIs, table objects,
   compaction internals, backend IO handles, or LSM level details.

8. **Reads and writes carry explicit branch, space, and temporal context.**
   Ambient process state, current CLI selection, or hidden global branch state
   must not determine persistence behavior.

9. **Internal commit batches remain central; public transaction sessions do not.**
   Engine services may group multiple row mutations into one commit plan. Users
   should not manage begin/commit/rollback sessions as a product workflow.

10. **Derived rows are visible to the adapter but not privileged.**
   The adapter records and scans derived rows like any other engine-owned rows.
   Rebuild, staleness, and correctness policy belongs to orchestration,
   retrieval, capability adapters, and diagnostics.

11. **Storage errors are mapped at this boundary.**
   Storage errors should cross into product layers as engine diagnostics with
   stable code/class/context. Write-path ambiguity must remain explicit.

12. **Storage health facts are adapted, not hidden.**
    Recovery health, backend capability facts, and maintenance state should be
    preserved enough for diagnostics, tests, and future StrataHub reporting.

## Open, Lifecycle, And Fault Boundary

The persistence adapter is also the normal engine boundary for storage lifecycle
operations. Row reads and writes are not enough.

Runtime owns product open policy:

1. Access mode.
2. IPC fallback.
3. Resource profile selection.
4. User-facing recovery policy.
5. StrataHub or Strata AI behavior.

The adapter consumes the runtime-resolved storage facts and calls storage-next
L9 for:

1. Open or create storage with storage mode, durability policy, codec config,
   recovery config, backend descriptor, and resolved storage budget.
2. Checkpoint operations when engine diagnostics or tests require them.
3. Storage maintenance drain/status/control hooks.
4. Safe close/shutdown.
5. Feature-gated test/fault hooks.

Rules:

1. Storage open/create must validate backend capability and codec facts before
   durable side effects.
2. The adapter maps storage open outcome into engine diagnostics while
   preserving raw recovery, capability, and recovered-version facts.
3. Product users should not need manual checkpoint, flush, compaction, prune, or
   repair workflows. Maintenance controls are engine/test/diagnostic hooks, not
   normal product pathways.
4. Close must stop new commit plans, drain or report maintenance state, call
   storage close, and preserve close timeout/retry facts.
5. Fault hooks must be unavailable or inert in normal production builds.
6. Capability code must not call lifecycle, maintenance, close, or fault hooks
   directly.

## Write Path

The normal write path is:

```text
product operation
  -> capability validation and value encoding
  -> row mutations
  -> commit plan
  -> persistence adapter
  -> storage-next commit batch
  -> commit outcome
  -> diagnostics and post-commit hooks
```

Rules:

1. Capability validation must happen before the adapter receives row mutations.
2. The adapter must reject storage-owned IDs in engine row mutations before
   sending them to storage.
3. The adapter must reject raw or unregistered engine storage-space IDs in
   stable V1 operation paths.
4. The adapter must derive source/control/derived classification from the
   registry and reject caller assertions that disagree.
5. The adapter must preserve classification for diagnostics, branch operations,
   and clone/export.
6. The adapter must reject write commit plans for read-only, closed, closing, or
   otherwise write-disabled handles before storage mutation.
7. Control-plane storage-space assignments require control-plane write
   authority. Data capability write paths may not write `Registry`, `Dataset`,
   `Recipe`, `Branch`, or `Space` control rows by selecting those symbolic
   assignments directly.
8. The adapter must carry operation origin into storage diagnostics where
   useful, without leaking secrets or large user payloads.
9. If storage reports an ambiguous commit outcome, the adapter must surface that
   classification. It must not retry blindly unless the product operation is
   explicitly idempotent.
10. Derived-state writes that happen after an authored commit must be observable
   as derived-state lag, failure, or rebuild debt. They must not make the source
   commit look failed unless the owning contract requires atomic derived state.

The adapter should not expose public transaction handles. It may expose an
internal commit builder or equivalent if that keeps engine service code
disciplined.

## Read Path

The normal read path is:

```text
product read
  -> capability resolves row address
  -> persistence adapter applies read selector
  -> storage-next returns storage row bytes and metadata
  -> capability decodes value bytes
  -> product result or diagnostic
```

The adapter must support these storage-mechanical reads:

1. Latest row read.
2. Version-bounded row read.
3. Timestamp-bounded row read.
4. Retained per-row history.
5. Existence checks.
6. Prefix/range scans with deterministic ordering.
7. Bounded scan pagination where retained.

The adapter returns row bytes, tombstone/absence facts, commit version, commit
timestamp, and retained-history facts. Capability code decodes row values and
turns malformed capability bytes into capability diagnostics.

Read rules:

1. `getv` means version-bounded read.
2. `as_of` means timestamp-bounded read through the storage commit timeline.
3. History reads must distinguish absent, deleted, malformed historical value,
   and history-trimmed states.
4. Prefix and range scans must be bounded by an explicit storage-space
   assignment. A scan must not accidentally cross capability or derived-state
   row families.
5. The adapter may expose raw row scans to branch, clone/export, diagnostics,
   and conformance tests. Product API code should normally read through
   capability facades.

Timestamp-bounded multi-row reads must be snapshot-consistent. For any operation
that touches more than one row, such as scans, relationship resolution,
retrieval, compare, or branch-from-time, the adapter must resolve the requested
timestamp once per branch into a retained version frontier before issuing row
reads or scans. The same frontier must be used for every row in that logical
operation unless storage-next exposes an atomic timestamp selector with the same
guarantee.

The adapter must also hold a storage read view, retention pin, snapshot guard,
or equivalent guarantee for the lifetime of any multi-row temporal operation. A
resolved frontier is not safe if retention or compaction can reclaim rows before
the operation finishes.

## Temporal Context

Storage-next owns the generic commit timeline substrate. Engine-next owns
product time-travel behavior.

The persistence adapter consumes storage timeline resolution for:

1. Timestamp-bounded reads.
2. Branch-from-time substrate.
3. History diagnostics.
4. Retained-history boundary reporting.

The adapter exposes the resolved frontier and retained-history facts to the
temporal context layer. The temporal context layer decides product wording,
after-latest diagnostics, and timeline scrub UI behavior; the adapter ensures
storage operations share the same frontier.

The adapter must not decide product explanations such as "nearest available
time" or timeline scrub UI behavior. Those belong in the temporal context and
product API contracts.

If storage reports that the requested timestamp is outside retained history, the
adapter should preserve:

1. Requested timestamp.
2. Branch identity.
3. Retained lower and upper bounds where available.
4. Whether the failure is before-history, after-latest, pruned gap, corruption,
   or unsupported backend mode.

## Branch Mechanics

Branch workflows are product semantics. Storage branch mechanics are substrate.

The adapter may expose storage-mechanical operations to the branch bucket:

1. Create branch storage state.
2. Fork branch storage state using inherited COW layers.
3. Fork branch storage state at an explicit retained commit version.
4. Materialize inherited rows when requested or scheduled.
5. List storage-known branches.
6. Delete branch storage state with reachability safety.
7. Read or scan physical row ranges for branch diff/copy/export.

The adapter must not implement:

1. Merge policy.
2. Cherry-pick semantics.
3. Revert semantics.
4. Source-wins behavior.
5. Strict conflict interpretation.
6. User-facing branch diagnostics.
7. Capability-specific diff interpretation.

Branch services coordinate product behavior and ask capability adapters to
interpret rows. Persistence supplies storage-shaped access and storage
mechanics.

## Control Plane And Registry Access

The adapter is the normal path for engine control-plane reads and writes.

It must support:

1. Global `_system_` branch rows.
2. Branch-local `_system_` space rows.
3. Registry bootstrap through storage-space ID `0x32`.
4. Control-plane row reads before ordinary product services start.
5. Registry validation during open.
6. Control-plane writes during database creation and format/cutover operations.
7. Scoped write authority for control-plane row families.

Bootstrap rule:

1. The compiled V1 registry seed knows that `0x32` is the registry control ID.
2. The adapter uses that compiled bootstrap fact to locate persisted registry
   rows.
3. The persisted registry validates the full active assignment table.
4. A conflict between compiled bootstrap meaning and durable control rows fails
   open with a structured format/layout error.

The adapter must not let ordinary capability code write control-plane rows
without going through the relevant control-plane service.

Control-plane services receive a scoped authority from runtime or open/create
sequencing. That authority permits the relevant control storage-space
assignments and should be narrow: registry code can write registry rows, recipe
code can write recipe rows, dataset/provenance code can write dataset rows, and
branch/space services can write branch or space control rows. The adapter
rejects control-plane writes made under a data-capability scope.

## Snapshot, Clone, And Recovery

The adapter should keep storage snapshots row-native and product-neutral.

It owns or coordinates:

1. Engine validation of storage-space IDs in row-native snapshot/import data.
2. Mapping storage snapshot/checkpoint facts into engine diagnostics.
3. Decoding row-native recovery payloads into rows for capability validation
   where needed.
4. Preserving registry facts in clone/export/import artifacts.
5. Reporting derived-state rows that were omitted and must be rebuilt.

It must not reintroduce primitive snapshot DTOs as the normal V1 recovery
format.

Rules:

1. Source rows are required in dataset clone/export unless explicitly filtered.
2. Derived rows may be omitted only when the artifact records rebuild
   requirements.
3. `0x45` health/watermark rows must not survive import for omitted derived row
   families.
4. Unknown engine-owned storage-space IDs fail import/open/recovery unless the
   format/cutover policy explicitly marks the database as pre-freeze developer
   data.
5. Clone/import that preserves retained history must preserve the
   version-to-timestamp timeline mapping for included commits. If an artifact
   intentionally rebases or resets history, it must say so explicitly and engine
   must surface that as a product-visible history change.
6. Storage-owned timeline rows are not engine-authored clone content. The
   adapter consumes storage-defined artifact rules for preserving, validating,
   or rebuilding them, but it must not silently rebuild a timeline in a way that
   changes `as_of`, history, or branch-from-time semantics for retained commits.

The dataset clone artifact contract will define the external artifact shape.
This contract defines the engine/storage handoff.

## Health, Metrics, And Diagnostics

The adapter maps storage facts into engine diagnostics without turning
diagnostics into control flow.

It should preserve:

1. Storage mode and backend capability facts.
2. Open/create facts.
3. Recovered visible version and commit timeline bounds.
4. Recovery health and degradation facts.
5. WAL/snapshot/table/quarantine facts where exposed by L9.
6. Maintenance debt and writer/sync health.
7. Cache/runtime budget facts.
8. Storage-space ID validation failures.
9. Ambiguous commit outcome details.

Error mapping rules:

1. Ordinary storage input/capability failures map to structured engine errors,
   not `internal`.
2. Storage corruption maps to corruption-class engine diagnostics.
3. Backend capability mismatch maps to unsupported/precondition diagnostics.
4. Ambiguous write outcomes preserve commit ambiguity.
5. Write-path errors should not use a blanket conversion that loses operation
   phase, commit outcome, branch, or storage-space context.
6. Redaction rules from the V1 error contract apply at this boundary.

## Forbidden Dependencies And Shortcuts

Engine-next production code outside the persistence bucket must not:

1. Import storage-next L9 directly.
2. Import storage-next internals below L9.
3. Construct physical storage keys directly.
4. Use raw numeric engine storage-space IDs outside the central registry.
5. Decide storage durability behavior.
6. Parse WAL, manifest, table, snapshot, or checkpoint bytes.
7. Infer storage recovery state from object names.
8. Reach into backend IO handles.
9. Decode sibling capability values to perform branch, retrieval, or
   orchestration work.
10. Treat derived rows as authoritative unless the owning contract says so.
11. Call storage lifecycle, checkpoint, maintenance, close, or fault hooks
    directly.

Allowed exceptions:

1. Persistence adapter implementation.
2. Engine tests that intentionally characterize the adapter/storage boundary.
3. Migration or verification tools with explicit documentation.
4. Temporary cutover shims listed in an implementation plan with removal gates.

## Conformance Tests

Adapter conformance tests should prove:

1. Capability code cannot construct physical storage keys in normal production
   paths.
2. Raw numeric storage-space IDs appear only in the central registry and tests.
3. Symbolic assignments resolve to the registry's expected IDs.
4. Engine rows using storage-owned IDs are rejected before storage commit.
5. Unknown engine-owned IDs fail during open, recovery, or import with
   structured diagnostics.
6. Latest, version, timestamp, and history reads all operate over the same
   versioned row chain.
7. Prefix scans cannot cross storage-space boundaries accidentally.
8. Commit outcomes preserve version, timestamp, durability, and ambiguity
   facts.
9. Storage error mappings preserve source chains, public codes, retry policy,
   and commit outcome.
10. Registry bootstrap uses compiled `0x32` and rejects conflicting durable
    registry rows.
11. Clone/import drops or reinitializes `0x45` records for omitted derived row
    families.
12. Clone/import preserves version-to-timestamp mappings for retained history or
    reports an explicit history reset/rebase.
13. Branch mechanical operations do not implement product merge policy inside
    persistence.
14. Cache mode write/read paths do not require WAL, manifest, snapshot, or
    durable local filesystem assumptions.
15. Read-only mode rejects write commit plans before storage mutation.
16. Timestamp-bounded scans and multi-row reads resolve one branch-local version
    frontier and use it for the whole logical operation.
17. Classification mismatches between caller assertions and the registry fail
    before storage mutation.
18. Data-capability write scopes cannot write control-plane row families.
19. Open/create, checkpoint, maintenance, close, and fault-hook calls pass
    through the adapter rather than direct storage imports.
20. Redaction tests cover backend addresses, operation origin, and error
    context at the adapter boundary.

## Deferred Or Open Questions

1. Exact Rust names.
   The implementation should keep the vocabulary small. The names in this
   document are conceptual, not final type names.

2. Storage key type.
   Storage-next L9 still needs to decide whether it exposes typed storage keys,
   opaque key bytes behind constructors, or both. The adapter must consume
   whichever shape L9 stabilizes.

3. Commit builder shape.
   The adapter may expose a builder if that reduces misuse, but V1 does not
   require one builder type per capability.

4. Control-plane row keys.
   The control-plane layout contract must define exact keys for registry,
   dataset, recipe, capability, and derived-state rows.

5. Timeline product policy.
   The temporal context contract must define product behavior for nearest
   timestamp, retained-history trimming, timeline scrub, and branch-from-time.

6. Derived-state validation depth.
   The retrieval and derived-state contract must decide which derived rows are
   verified synchronously on read and which are diagnosed asynchronously.

7. Cutover shims.
   The implementation plan must list temporary direct-storage imports allowed
   during migration and the tests that remove them.

## V1 Minimum

For V1, the minimum acceptable implementation is:

1. One normal production storage-facing adapter inside engine-next.
2. Capability code emits row addresses, value bytes, and row mutations instead
   of physical storage keys.
3. The adapter consumes the storage-space ID registry and centralizes raw IDs.
4. The adapter supports latest, version, timestamp, history, prefix/range scan,
   and existence reads.
5. The adapter supports single-branch commit plans and maps them to storage
   commit batches.
6. The adapter exposes storage branch mechanics to branch services without
   implementing product branch policy.
7. The adapter validates registry/bootstrap facts on open.
8. The adapter maps storage errors and health facts into engine diagnostics.
9. The adapter handles cache, standard, and always durability modes through
   storage L9 facts without capability code branching on storage internals.
10. The adapter handles storage open/create, checkpoint, maintenance, close, and
    test/fault hooks for runtime and diagnostics.
11. The adapter enforces read-only and scoped control-plane write authority
    before storage mutation.
12. Timestamp-bounded multi-row operations use one resolved branch frontier.
13. Clone/import preserves retained timeline semantics or reports an explicit
    history reset/rebase.
14. Direct storage imports outside persistence are blocked by tests or explicit
    temporary migration exceptions.

## Next Step

The branch operation and capability adapter contract is defined in
`docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`.

The temporal context and timeline resolver contract is defined in
`docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`.

The next contract should be the control-plane layout contract.
