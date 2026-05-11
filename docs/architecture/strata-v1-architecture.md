# Strata V1 Architecture

Status: V1 architecture draft

## Introduction

Strata V1 is an embedded, local-first database for applications and AI systems
that need durable state, versioned data, retrieval-oriented capabilities, and
portable deployment in one coherent product.

The V1 architecture must make that product model possible without preserving
historical crate boundaries by inertia. The architecture should be simple to
explain:

```text
core-next -> storage-next -> engine-next -> intelligence-next -> executor / cli / SDK / Strata AI
                                      intelligence-next -> inference-next
```

Only engine-next consumes storage-next directly in normal production code.
Intelligence-next consumes engine-owned database behavior and inference-owned
model/provider behavior. Everything above engine-next uses engine-owned product
APIs, intelligence APIs, SDK APIs, or the serializable command boundary.

This document is the high-level architecture anchor for V1. It defines the
layering model, responsibility boundaries, runtime modes, portability stance,
durability expectations, and testing obligations. It does not define exact Rust
traits, module layouts, byte formats, IPC wire formats, CLI syntax, or
implementation milestones. Those belong in follow-up architecture documents.

## Related Documents

Product anchors:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/strata-v1-user-pathways.md`
4. `docs/product/strata-v1-non-functional-requirements.md`

Focused product direction:

1. `docs/product/strata-v1-branching-direction.md`
2. `docs/product/strata-v1-graph-relationship-layer.md`
3. `docs/product/strata-v1-versioning-time-travel.md`
4. `docs/product/stratahub-product-direction.md`

Current architecture evidence:

1. `docs/engine/engine-consolidation-plan.md`
2. `docs/storage/v1-storage-consumption-contract.md`
3. `docs/storage/storage-engine-ownership-audit.md`

Cross-cutting V1 contracts:

1. `docs/architecture/v1-error-and-diagnostics-contract.md`
2. `docs/architecture/v1-testing-and-conformance-plan.md`
3. `docs/architecture/stratahub-substrate-architecture.md`
4. `docs/architecture/runtime-resource-profile-architecture.md`

The product documents are authoritative for user outcomes. This architecture
document is authoritative for the first-pass V1 layering model. Lower-level
architecture documents should refine this document, not bypass it.

## Requirement Language

1. Must means V1 architecture is incomplete without it.
2. Should means expected for V1 unless a later architecture decision records a
   clear deferral.
3. May means allowed but not required for V1.

## Product Constraints That Drive Architecture

The architecture exists to satisfy these product constraints:

1. Strata is embedded first.
   Normal use should not require a separate database server.

2. Durable local filesystem is the reference backend.
   V1 must preserve committed data after ordinary process crashes.

3. Cache mode is explicit and non-durable.
   Disk-backed cache mode is not a V1 product mode.

4. IPC is required for local multi-process access.
   Strata AI makes app plus assistant access a normal V1 workflow. IPC is the
   supported same-machine sharing path.

5. Follower mode is not a V1 product path.
   Multi-process local access must not depend on follower refresh semantics.

6. Storage portability is a V1 substrate requirement.
   Local filesystem, object storage direction, browser/WASM direction, and
   OpenDAL adapters must be expressed through Strata's own backend capability
   contract.

7. Storage is data-capability agnostic.
   KV, JSON, events, graph, vectors, search, recipes, and intelligence behavior
   belong above storage.

8. Branching and time travel are core.
   Branch, version, history, diff, restore, and branch-from-history behavior
   must shape engine architecture directly.

9. Retrieval is part of the database experience.
   Search, vector retrieval, graph-aware retrieval, recipes, and RAG are engine
   and higher-layer concerns, not storage concerns.

10. Users should not run maintenance commands during normal use.
    Flush, compact, checkpoint, retention, and recovery mechanics are internal
    lifecycle behavior with observability, not ordinary product workflows.

11. Public transaction commands are not a V1 product requirement.
    Internal commit machinery remains required. The public product should expose
    clear write and batch semantics, not manual begin/commit/rollback sessions.

12. V1 format changes must be explicit.
    The high-level architecture does not authorize hidden byte-format churn as
    a side effect of crate cleanup. Storage-next may intentionally introduce a
    row-native format revision where the storage architecture requires it, but
    that must be specified in the storage format spec. Because Strata is
    pre-launch, the default cutover policy is clear rejection of pre-V1
    development databases rather than migration or compatibility machinery.

13. Runtime resource profiling is a V1 product requirement.
    One binary must adapt from constrained edge devices to server-class
    machines. Engine-owned resource planning should translate host facts and
    user configuration into resolved storage, engine, derived-state, and
    maintenance budgets.

## Binding V1 Architecture Decisions

These decisions resolve the older `next-charter.md` direction where it conflicts
with the product requirements and storage-next layer documents.

1. Durable local filesystem is the reference durable backend.
   Object storage and OpenDAL remain architecture-aware substrate work, but V1
   does not make S3-like object durability the canonical first implementation.

2. Sync data movement is post-V1.
   V1 must provide dataset/instance identity, bundle and clone metadata,
   provenance, capability reports, and health reports for future StrataHub
   workflows. It does not ship hidden replication or branch sync. When sync is
   designed, it should be an optional layer consuming engine-owned semantics,
   not a direct storage consumer.

3. V1 branch merge is engine-owned, not CRDT-owned.
   The required V1 merge strategies are the product-documented Strict and
   SourceWins modes. Storage does not need HLCs, replica IDs, or CRDT tombstone
   GC to satisfy V1. If CRDT sync returns later, it needs a dedicated sync and
   merge design.

4. Commit timestamps and the commit timeline are storage-native V1 substrate.
   Storage-next should stamp every committed batch with one commit timestamp,
   persist a per-branch commit-version-to-timestamp timeline, and expose
   timestamp-to-version resolution facts to engine-next. Engine owns the product
   UX for `as_of`, branch-from-time, and timeline explanations.

5. Cache mode has no durable storage services.
   Cache mode still allocates versions, records commit timestamps, maintains
   branch state, and supports normal in-memory reads. It does not create WAL,
   MANIFEST, snapshot, checkpoint, or durable table objects. Any cache identity
   is ephemeral unless engine assigns product-level instance metadata.

6. V1 preserves `cache`, `standard`, and `always` durability modes.
   The clean architecture expresses these as two axes: storage mode and durable
   commit policy. `cache` is an ephemeral storage mode with no WAL and no crash
   durability. `standard` and `always` are durable commit policies used inside
   durable storage modes.

7. Engine encodes product references into storage rows.
   `EntityRef`, graph relationship references, primitive DTOs, and data
   capability identities are engine-owned. Storage sees physical keys, opaque
   storage space IDs, row values, and commit metadata.

8. The storage-next cutover is a canonical replacement, not a permanent
   parallel product.
   `*-next` names describe the design and build phase. The cutover plan must
   describe how the new crates become canonical and how old crates are retired.
   Stable V1 should reject pre-V1 development databases during normal open.
   Any conversion tool is developer tooling, not part of the normal product
   open path.

9. Runtime resource profiles are engine-owned policy.
   Engine-next owns host probing, profile classification, explicit override
   precedence, product-wide budget allocation, and diagnostics. Storage-next
   receives resolved storage budgets and owns storage-local spending.

## Architecture Principles

### 1. Product Semantics Live In Engine

Engine owns the meaning of database operations. Storage may expose raw facts and
mechanics, but it must not decide product behavior.

Engine owns:

1. Database open policy.
2. Branch, version, time-travel, and restore semantics.
3. Data capability semantics.
4. Derived-state consistency.
5. User-facing errors and diagnostics.
6. IPC behavior.
7. Product lifecycle policy over storage-owned lifecycle mechanics.

### 2. Storage Owns Persistence Mechanics

Storage owns the mechanics needed to persist and recover generic database rows.
It does not know why a row exists.

Storage owns:

1. Backend access.
2. Physical keyspace and row layout.
3. Commit-unit persistence.
4. WAL, manifest, checkpoint, snapshot, compaction, retention, and recovery
   mechanics.
5. Backend capability validation.
6. Storage-local fault injection and corruption simulation.
7. Raw storage health facts.

Storage must not know JSON paths, event chain meaning, vector embedding policy,
graph ontology, graph traversal, search ranking, recipe behavior, or Strata AI
behavior.

### 3. Core Contains Shared Contracts, Not Convenience

Core exists only for vocabulary and contracts that genuinely belong below both
storage and engine. It should not become a dumping ground for helpers.

Core may own:

1. Stable IDs and transparent newtypes.
2. Version and timestamp vocabulary.
3. Branch, space, database, and backend address identifiers where they are
   cross-layer concepts.
4. Shared error category vocabulary where the category is not engine-specific
   or storage-specific.
5. Serialization-neutral contract types needed by multiple lower layers.

Core must not own:

1. Storage IO.
2. Product behavior.
3. CLI, SDK, or command execution.
4. Search, graph, vector, JSON, event, or intelligence semantics.
5. Broad utility modules that exist only to avoid imports.

### 4. Upper Layers Do Not Reach Around Engine

Executor, CLI, SDK, intelligence, inference integration, and Strata AI must not
consume storage directly in normal production code. If they need storage-backed
behavior, engine exposes a semantic API or command.

Allowed exceptions must be explicit:

1. Tests.
2. Benches.
3. Fuzz targets.
4. Diagnostic tools.
5. Migration or verification tools.

### 5. Backend Capabilities Are Explicit

A backend is not "supported" because an adapter compiles. A backend is supported
for a runtime mode only when it declares and passes the required capabilities.

Open must fail clearly when a backend cannot satisfy the selected mode.

### 6. No Hidden Network Behavior

Strata must not upload, sync, register, call model providers, or contact
StrataHub without explicit user action or configuration. Embedded local use must
remain independent of network services.

### 7. Testability Is An Architecture Requirement

Every boundary must be designed so it can be tested directly. A clean crate
graph that cannot support fault injection, crash recovery tests, backend
conformance, fuzzing, or product-path tests is not acceptable.

## Target Crate Graph

The V1 target graph is:

```text
strata-core-next
        |
        v
strata-storage-next
        |
        v
strata-engine-next
        |
        v
strata-intelligence-next ----> strata-inference-next
        |
        +--> strata-executor
        +--> strata-cli
        +--> SDK surfaces
        +--> Strata AI
```

Rules:

1. `storage-next` may depend on `core-next`.
2. `engine-next` may depend on `storage-next` and `core-next`.
3. `intelligence-next` may depend on `engine-next`, `core-next`, and
   `inference-next`.
4. Product crates above engine must not depend on `storage-next`.
5. `inference-next` must not depend on `engine-next` or `storage-next`.
6. Executor, CLI, SDK surfaces, and Strata AI consume engine and intelligence
   APIs rather than storage APIs.
7. Optional provider/runtime features must be feature-gated and observable.

The names `core-next`, `storage-next`, `engine-next`, `intelligence-next`, and
`inference-next` describe the design phase. Once the new architecture becomes
canonical, the crates should shed the `next` suffix rather than preserve
permanent parallel names.

## Core-Next Responsibility

Core-next is the smallest shared contract layer.

It should define:

1. Cross-layer identifiers.
2. Version, timestamp, and branch-point vocabulary.
3. Backend address vocabulary if storage and engine both need it.
4. Error category building blocks that are truly shared.
5. Serialization conventions for cross-layer contract types.

It should avoid:

1. Runtime ownership.
2. Storage mechanics.
3. Product policy.
4. CLI or SDK affordances.
5. Data capability objects.
6. Generic helper sprawl.

Core-next must justify every public type by naming which lower layers need the
same concept and why the concept is not owned more cleanly by storage or engine.

## Storage-Next Responsibility

Storage-next is the persistence substrate.

It must provide:

1. A generic storage model over physical keys, values, commit units, and
   retained history.
2. Backend provider abstraction owned by Strata.
3. Capability checks for durable local, cache, object storage direction,
   browser/WASM direction, and OpenDAL-backed adapters.
4. WAL, manifest, snapshot, checkpoint, compaction, retention, and recovery
   mechanics.
5. Crash-recovery and corruption behavior that can be tested without engine
   data-capability semantics.
6. Fault-injection hooks that make torn writes, failed fsyncs, partial objects,
   stale manifests, checksum failures, and backend capability failures
   testable.
7. Storage metrics and health facts as raw facts.

It must not provide:

1. JSON document semantics.
2. Event product semantics.
3. Vector collection or embedding semantics.
4. Graph ontology, traversal, analytics, or relationship semantics.
5. Search ranking or indexing policy.
6. Recipe, model, RAG, or Strata AI behavior.
7. Public product errors.

Storage-next may expose storage-local diagnostics and fault-injection surfaces,
but those are not automatically product APIs.

## Engine-Next Responsibility

Engine-next is the database semantics layer.

It must provide:

1. Product open APIs for durable local, cache, read-only, and IPC-backed access.
2. Branch, space, version, history, diff, restore, copy, promote, and
   branch-from-history behavior.
3. Data capability APIs for KV, JSON, events, graph, vectors, search, and
   retrieval.
4. Graph as both a standalone data capability and a relationship layer across
   Strata records.
5. Derived-state management for search indexes, vector indexes, graph
   relationship indexes, auto embeddings, and recipe outputs.
6. Commit-unit and batch semantics exposed as product operations rather than
   public transaction sessions.
7. Product lifecycle policy for recovery, maintenance, retention,
   checkpoints, compaction, shutdown, and observability, using storage-owned
   lifecycle mechanics through the L9 boundary.
8. Engine-owned public errors and diagnostics.
9. The serializable command boundary used by CLI, IPC, tests, and agents.

Engine-next must hide storage mechanics above engine unless a storage fact is
intentionally converted into an engine-owned public diagnostic.

## Intelligence-Next Responsibility

Intelligence-next is the database-aware AI and retrieval orchestration layer. It
depends on engine for database state and on inference for model/provider
execution.

It should provide:

1. Retrieval recipes.
2. Query expansion and reranking orchestration where enabled.
3. RAG and answer-generation workflows where enabled.
4. Auto-embedding orchestration and reindex workflows where enabled.
5. Explanations of which branches, spaces, records, versions, indexes, models,
   and retrieval stages contributed to an answer.
6. Strata AI-facing workflows that need database context.

It must not provide:

1. Storage durability behavior.
2. Database recovery behavior.
3. Backend capability checks.
4. Engine bypasses for reads or writes.
5. Hidden network or model-provider calls.

Intelligence features may be optional or feature-gated, but the architecture
must treat the layer as part of the V1 product stack so retrieval and Strata AI
do not get mischaracterized as external add-ons.

## Inference-Next Responsibility

Inference-next is the model and provider execution layer. It is not a database
layer.

It should provide:

1. Model provider adapters.
2. Local or remote inference execution where configured.
3. Tokenization, detokenization, embedding, and generation utilities where
   supported.
4. Provider errors and model availability facts that intelligence can translate
   into product behavior.

It must not provide:

1. Storage access.
2. Engine access.
3. Branch, space, version, or record semantics.
4. Database lifecycle behavior.
5. Implicit network calls without explicit configuration or user action.

Inference-next may be consumed by intelligence and by explicitly documented
model-management product surfaces. It must remain independent of storage and
durability correctness.

## Runtime Modes

### Durable Local

Durable local open is the reference V1 mode. It must:

1. Open or create a database at a local filesystem path.
2. Acquire the required local writer protection.
3. Run deterministic recovery before serving traffic.
4. Preserve committed data after ordinary crashes.
5. Expose clear health, metrics, and recovery diagnostics.

### Cache

Cache mode is explicit and non-durable. It must:

1. Avoid hidden disk durability.
2. Avoid WAL, manifest, checkpoint, and durable file promises where the selected
   runtime is non-durable.
3. Support normal data capability behavior where durability is not required.
4. Be visibly different from durable mode in info and health output.

Disk-backed cache mode is removed or redesigned before V1.

### Read-Only

Read-only mode must:

1. Open an existing database without permitting mutation.
2. Reject writes before mutation.
3. Explain when recovery or repair would require write access.
4. Preserve the same branch, space, search, graph, and time-travel read
   semantics as writable open where possible.

### IPC-Backed Local Shared Access

IPC is required for V1. It is the supported local multi-process access model for
an application plus Strata AI, tools, or another local process.

IPC must:

1. Route secondary local access through the already-open database owner.
2. Preserve access-mode semantics.
3. Reject writes according to the same command classification as local handles.
4. Surface stale socket, permission, protocol, lock, and server failures
   explicitly.
5. Expose whether a handle is local or IPC-backed where that matters.
6. Avoid becoming a mandatory server mode for ordinary embedded use.

Follower mode must not remain as a parallel local sharing mechanism.

## Backend Portability Model

Strata owns the backend capability contract. OpenDAL is an adapter family, not
the definition of Strata storage correctness.

The backend model must define:

1. Backend address syntax and normalization.
2. Capability declarations.
3. Required capabilities per runtime mode.
4. Explicit unsupported-capability errors.
5. Backend conformance tests.
6. Durability classes.
7. Read, write, listing, conditional update, locking, and sync assumptions.
8. Object storage and browser/WASM constraints.

Required V1 stance:

1. Local filesystem is the reference durable backend.
2. Browser/WASM and cache targets must be explicit about durability limits.
3. Object-storage-backed targets should be designed into the architecture.
4. OpenDAL-backed targets may be supported when their declared capabilities pass
   Strata's conformance tests.
5. V1 must not claim every OpenDAL backend is production-ready.

No engine or storage module should assume POSIX filesystem behavior unless that
assumption is isolated behind a backend implementation and represented in the
capability contract.

## Runtime Resource Profile Model

The detailed contract lives in
`docs/architecture/runtime-resource-profile-architecture.md`.

V1 must preserve Strata's ability to run the same binary on constrained edge
devices, laptops, cloud VMs, and large servers.

Architecture consequences:

1. Engine owns host probing, resource profile selection, user override
   precedence, product-wide budget allocation, and diagnostics.
2. Storage receives resolved storage budgets. It does not classify the host or
   mutate product defaults.
3. Graph, vector, search, retrieval, and intelligence features receive
   engine-owned budget guidance instead of independently probing the machine.
4. Resolved runtime plans are observable but are not persisted as user-selected
   configuration.
5. Low-memory behavior should produce typed resource errors, bounded operation
   shapes, or derived-state degradation before uncontrolled out-of-memory
   behavior.

## Durability, Recovery, And Atomicity

V1 architecture must make durability honest and testable.

V1 preserves three product durability modes:

1. `cache`
   Ephemeral, WAL-free, no crash durability, and no durable files. Cache mode is
   a storage mode, not a weaker durable policy.

2. `standard`
   Durable storage mode with WAL-backed crash recovery and background or
   periodic durability barriers. It provides a bounded crash-loss window
   according to its configured interval and backend capability.

3. `always`
   Durable storage mode with WAL-backed crash recovery and a force-durability
   barrier before acknowledging each committed write.

The underlying architecture should treat this as:

```text
StorageMode = Cache | Durable
DurabilityPolicy = Standard | Always
```

`DurabilityPolicy` only applies when `StorageMode = Durable`. Runtime switches
may occur between `standard` and `always` where the backend supports both.
Switching into or out of `cache` is not a runtime durability-policy transition.

Durable mode must define:

1. What counts as a committed write.
2. When a write is visible.
3. What survives an ordinary process crash.
4. What open does after a crash.
5. How corruption, incompatible format, unsupported backend, permission errors,
   and IO failure surface.
6. What health and metrics report.

The product should expose commit and batch semantics. It should not expose
manual transaction sessions unless a later product decision defines an ACID
claim, isolation contract, backend requirements, and test suite.

V1 architecture should preserve the current physical durability rules unless a
dedicated format design changes them explicitly. Architecture work may move
ownership, improve tests, and isolate format assumptions, but it must not hide a
format rewrite inside crate cleanup.

Users should not need manual flush, compact, checkpoint, or retention commands
for normal operation. Those mechanics should run through engine-owned lifecycle
policy with observable status and bounded diagnostics.

## Data Capability Ownership

Engine owns data capability semantics. Storage stores generic physical rows.

### KV

KV is a product data capability for simple records. Engine defines public key
semantics, branch and space behavior, time-travel behavior, and error surfaces.

### JSON

JSON is a structured document capability. Engine owns document identity, path
behavior, filtering behavior, indexing policy, and branch/time semantics.

### Events

Events are ordered history or timeline records. Engine owns append semantics,
event identity, ordering guarantees, branch/time behavior, and query behavior.

### Graph

Graph has two roles:

1. Standalone graph nodes, edges, ontology metadata, traversal, and optional
   analytics.
2. Relationship layer across KV, JSON, event, vector, graph, and search
   records.

Engine owns entity references, relationship semantics, ontology validation,
branch/time behavior, and graph-derived state.

### Vectors

Vectors are a product capability for embeddings and similarity search. Engine
owns collections, dimensions, metadata, nearest-neighbor behavior, index
availability, and model-related derived state.

### Search And Retrieval

Search and retrieval are product capabilities over stored data. Engine and
higher retrieval layers own keyword search, semantic search, hybrid search,
query expansion, reranking, graph-aware retrieval, recipes, RAG, and result
explanations.

Model-dependent features must be explicit, optional where appropriate, and
observable. They must not affect storage durability correctness.

## IPC And Strata AI

Strata AI changes IPC from a convenience feature into a required V1 architecture
path.

The normal local AI workflow is:

1. The user application opens a Strata database.
2. Strata AI needs to inspect, search, explain, or mutate that same database
   according to user intent and access mode.
3. The AI process connects through IPC instead of opening a second direct
   writer.
4. Engine enforces the same command, branch, space, access-mode, and error
   semantics as local use.

Architecture consequences:

1. IPC belongs at the engine/product boundary, not in storage.
2. IPC should use the serializable command boundary where possible.
3. IPC must not bypass engine validation.
4. IPC must not require a daemon for ordinary single-process embedded use.
5. IPC must be testable as a product path, not just as transport plumbing.

## Error, Observability, And Diagnostics Model

V1 needs stable error categories and bounded diagnostics.

The detailed contract lives in
`docs/architecture/v1-error-and-diagnostics-contract.md`.

Architecture must support:

1. Typed open failures.
2. Typed backend capability failures.
3. Typed corruption and recovery failures.
4. Read-only write rejection before mutation.
5. IPC transport and protocol failures.
6. Search/index/model availability failures.
7. Structured health, metrics, describe, and durability counters.

Storage errors may be detailed and mechanic-specific. Engine decides which
errors become public product errors and how they are explained.

Diagnostics must avoid requiring users to understand WAL, manifests,
checkpoints, memtables, segment compaction, subsystem wiring, or cleanup history
for normal workflows.

## Testing And Conformance Model

Reference-grade testing is a V1 architecture requirement.

The top-level testing plan lives in
`docs/architecture/v1-testing-and-conformance-plan.md`.

The target test model includes:

1. Core contract unit tests.
2. Storage unit tests for physical layout, commit units, retention, corruption,
   and recovery mechanics.
3. Storage fault-injection tests for torn writes, failed sync, partial objects,
   stale manifests, checksum failure, lock conflicts, and backend failures.
4. Backend conformance tests for local filesystem, cache/memory, object storage
   direction, browser/WASM direction, and OpenDAL-backed adapters.
5. Engine integration tests for branches, spaces, versions, time travel,
   restore, graph relationships, vectors, search, events, and derived state.
6. Intelligence tests for retrieval recipes, RAG stages, query expansion,
   reranking, auto-embedding orchestration, and explanation provenance where
   those features are enabled.
7. Inference contract tests for provider availability, tokenization,
   generation, embedding, network gating, and provider error mapping.
8. IPC product tests for local plus secondary process access.
9. CLI and SDK tests for the user pathways.
10. Fuzz tests for parsers, codecs, command boundaries, storage manifests,
   snapshot/install paths, and import/export formats.
11. Crash-recovery tests that verify committed data and rejected partial state.
12. Runtime resource profile tests with fake host probes for edge, desktop,
    server, unknown, and explicitly configured hosts.
13. Long-running and randomized tests that exercise branch, search, graph,
    vector, and retention behavior together.

Testing must not depend on hidden production backdoors. If a behavior is
critical, the architecture should expose a testable contract or fault-injection
surface with clear gating.

## Migration Strategy

The next phase should proceed in documents before code:

1. Freeze this high-level architecture draft.
2. Write `core-next` architecture.
3. Write `storage-next` architecture.
4. Write `engine-next` architecture.
5. Write `inference-next` architecture.
6. Write `intelligence-next` architecture.
7. Write the backend capability and conformance contract.
8. Write the IPC runtime contract.
9. Write the testing and conformance plan.
10. Only then start implementation plans.

During implementation, existing crates may remain canonical until the new path
is ready. The purpose of `*-next` work is to avoid compile-driven compromises in
the current architecture. Cutover should be deliberate, documented, and tested.

When the new implementation becomes canonical, the public crate names should be
normal names again. Users should not learn that an internal rewrite happened.

Expected cutover sequence:

1. Build `core-next`, `storage-next`, and `engine-next` in parallel with the
   current crates while their contracts are still changing.
2. Keep product crates on the current canonical crates until the new stack can
   satisfy the required V1 pathways and conformance tests.
3. Avoid adapter or migration tooling unless a focused cutover plan proves it
   is necessary. Do not create temporary facades to keep both architectures
   alive indefinitely.
4. In the cutover PR series, replace the current canonical crates so the public
   graph returns to `strata-core`, `strata-storage`, and `strata-engine` rather
   than preserving permanent `next` names.
5. Update executor, CLI, intelligence, inference integration, tests, benches,
   fuzz targets, and docs in the same cutover plan or in explicitly ordered
   follow-up PRs.
6. Reject old pre-V1 development databases by default. If a one-off developer
   conversion tool is ever needed before launch, document it separately from
   normal product open. Silent best-effort reopen is not a cutover policy.
7. Move storage recovery health ownership deliberately. Current engine D4
   surface types such as `RecoveryHealth`, `DegradationClass`, and
   `RecoveryFault` become storage-next-owned V1 recovery facts; engine-next may
   re-export or wrap them, but storage owns the source definitions.

## Non-Goals

V1 architecture does not attempt to deliver:

1. StrataHub fleet management as a V1 product.
2. A managed cloud database service.
3. Distributed active-active multi-writer operation.
4. A SQL layer.
5. A universal ORM.
6. A storage-format rewrite hidden inside architecture cleanup.
7. Public manual transaction sessions as the core product model.
8. Follower mode.
9. Disk-backed cache mode.
10. Branch bundles as the V1 dataset artifact.
11. Tags and notes as V1 branch features.
12. Claims that every OpenDAL backend is production-ready.
13. Hidden network sync, model calls, telemetry, upload, or registration.

## Follow-Up Architecture Documents

This document should be followed by focused architecture documents:

1. `docs/architecture/core-next-architecture.md`
2. `docs/architecture/storage-next-architecture.md`
3. `docs/architecture/engine-next-architecture.md`
4. `docs/architecture/inference-next-architecture.md`
5. `docs/architecture/intelligence-next-architecture.md`
6. `docs/architecture/v1-backend-capability-contract.md`
7. `docs/architecture/v1-ipc-runtime-contract.md`
8. `docs/architecture/v1-testing-and-conformance-plan.md`
9. `docs/architecture/v1-error-and-diagnostics-contract.md`
10. `docs/architecture/v1-command-boundary-contract.md`
11. `docs/architecture/v1-cutover-plan.md`

Each follow-up document should state which product requirement it serves, which
layer owns the behavior, which lower layers it may call, how failures surface,
and how the behavior is tested.

## Architecture Decision Rules

When a design choice is ambiguous:

1. If it puts data capability semantics into storage, reject it.
2. If an upper layer needs storage directly, first ask what engine API is
   missing.
3. If a type cannot justify its crate, move it or delete it.
4. If a feature cannot be tested under fault, crash, or backend variation,
   redesign it.
5. If a backend works only because of POSIX assumptions, isolate those
   assumptions behind a backend capability.
6. If a public concept exists only because of implementation history, rename or
   remove it.
7. If an optional intelligence or model feature can affect durability
   correctness, the boundary is wrong.
8. If a cleanup requires temporary architecture that becomes hard to remove,
   prefer a parallel `*-next` path.
9. If a decision broadens V1 beyond the product requirements, defer it.
10. If a decision improves V1 reliability and simplifies the user model, prefer
    it even if it breaks pre-V1 compatibility.
