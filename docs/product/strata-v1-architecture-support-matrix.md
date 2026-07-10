# Strata V1 Architecture Support Matrix

Status: V1 product/architecture cross-check

## Purpose

This document checks the V1 product requirements, feature inventory, and user
pathways against the current target architecture:

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/core-architecture.md`
3. `docs/architecture/storage-architecture.md`
4. `docs/architecture/engine-architecture.md`
5. `docs/architecture/runtime-resource-profile-architecture.md`
6. `docs/architecture/v1-error-and-diagnostics-contract.md`
7. `docs/architecture/v1-testing-and-conformance-plan.md`

The question is not "is this implemented today?" The question is:

> Does the target architecture have the right owner, boundary, and substrate for
> each V1 product feature and pathway?

The answer is mostly yes. The architecture can support the V1 product model if
the follow-up contracts listed here are written before implementation.

## Scope

In scope:

1. V1 local database behavior.
2. V1 storage modes and durability modes.
3. V1 data capabilities.
4. V1 branching, versioning, time travel, and clone substrate.
5. V1 retrieval, graph, vector, search, configuration, diagnostics, CLI, SDK,
   and command-boundary behavior.

Out of scope for this pass:

1. StrataHub Library as a hosted product.
2. StrataHub Fleet as a hosted product.
3. Strata AI assistant experience.
4. Hidden sync, autosync, fleet registration, and cloud coordination.
5. Exact CLI syntax, SDK method names, IPC wire format, and Rust module layout.

This pass still includes the V1 substrate that future StrataHub and Strata AI
work depends on: cloneable datasets, identity/provenance metadata, IPC-required
local multi-process access, explicit network behavior, and structured
diagnostics.

## Legend

| Status | Meaning |
|---|---|
| Supported | Target architecture has a clear owner and boundary. |
| Supported, contract needed | The architecture is right, but a follow-up contract must pin exact behavior. |
| Optional substrate | Architecture can support it, but the product feature is optional for V1. |
| Deferred or removed | Product docs exclude it from V1, and target architecture should not carry it forward. |
| Gap | No clear owner or boundary exists yet. |

## Architecture Buckets

This document uses the target buckets from `engine-next` and the target layers
from `storage-next`.

Engine buckets:

1. API
2. Runtime
3. Commit
4. Branch
5. Data Capability
6. Control Plane
7. Orchestration
8. Retrieval
9. Persistence
10. Diagnostics

Storage layers:

1. L1 Backend I/O
2. L2 Object Layout
3. L3 Format/Codec
4. L4 Durable Services
5. L5 Table Runtime
6. L6 Branch-Isolated Row Runtime
7. L7 Commit Runtime
8. L8 Lifecycle/Recovery/Maintenance
9. L9 Storage API Boundary

## Feature Inventory Cross-Check

| Feature | Product decision | Architecture support | Follow-up |
|---|---|---|---|
| Durable local database open | Required | Supported by storage L1-L9, engine Runtime, Persistence, Commit, Diagnostics. | Storage open contract and engine persistence adapter. |
| Ephemeral cache database | Required | Supported by storage `StorageMode::Cache`, in-memory L6/L7 mechanics, engine Runtime. | Cache-mode capability and diagnostics tests. |
| Standard durability | Required | Supported as durable storage mode plus `DurabilityPolicy::Standard`. | Exact bounded sync/loss guarantee. |
| Always durability | Required | Supported as durable storage mode plus `DurabilityPolicy::Always`. | Exact durability barrier definition. |
| Disk-backed cache mode | Remove or redesign | Deferred or removed. Architecture explicitly excludes hidden disk-backed cache. | Remove old product surface. |
| Read-only open | Required | Supported by engine API/Runtime command classification and storage open access mode. | Product command write-classification contract. |
| IPC-backed local shared access | Required | Supported at engine API/Runtime command semantics; transport remains above engine. | IPC command-boundary contract. |
| Storage backend capability contract | Required substrate | Supported by storage L1/L8/L9 and diagnostics. | Backend capability matrix and conformance suite. |
| Local filesystem backend | Required | Supported as reference durable backend in storage L1/L4. | Local FS publish and crash conformance tests. |
| OpenDAL adapter path | Required substrate | Supported as architecture-aware backend adapter family, not core contract. | Post-local adapter trait and feature-gate plan. |
| S3-compatible object storage target | Required substrate | Supported as future object-backend target behind Strata capability contract. | Do not claim production readiness until conformance passes. |
| Browser/WASM cache target | Required substrate | Supported by cache mode and backend capability split. | WASM cache compile/test target. |
| Every OpenDAL backend production-ready | Remove or redesign | Deferred or removed. Architecture rejects blanket backend claims. | None for V1. |
| Adaptive runtime resource profiling | Required | Supported by runtime resource profile architecture; engine owns policy, storage consumes budgets. | Resolved runtime plan contract. |
| Dataset bundle and clone workflow | Required | Supported by engine API, Orchestration/Data Movement, Control Plane, Persistence, storage clone substrate. | `.strata` dataset artifact/clone contract. |
| StrataHub Library | Post-V1 product, V1 substrate required | Hosted product deferred; identity/provenance/clone substrate supported. | Excluded from this pass beyond substrate. |
| StrataHub Fleet | Post-V1 product, V1 substrate required | Hosted product deferred; instance/backend/capability/health substrate supported. | Excluded from this pass beyond substrate. |
| Key-value | Required | Supported by engine Data Capability/KV over Commit, Persistence, Branch adapters. | Data capability implementation contract. |
| JSON documents | Required | Supported by engine Data Capability/JSON and shared temporal/branch contracts. | JSON path, index, branch adapter contract. |
| Events | Required | Supported by engine Data Capability/Event and storage timeline substrate. | Event timestamp and ordering semantics. |
| Graph basics | Required | Supported by engine Data Capability/Graph. | Graph capability conformance and temporal traversal contract. |
| Graph relationship layer | Required, semantics must tighten | Supported by engine Data Capability/Graph, EntityRef, Orchestration, Control Plane, Retrieval. | EntityRef and relationship-layer contract. |
| Graph ontology | Required, semantics must tighten | Supported by engine Data Capability/Graph and Control Plane. | Ontology lifecycle and validation contract. |
| Graph analytics | Optional | Optional substrate through graph capability and diagnostics. | Bound execution and feature-gating decision. |
| Vector collections and query | Required | Supported by engine Data Capability/Vector and Retrieval integration. | Vector collection/index and temporal query contract. |
| Search and retrieval | Required | Supported by engine Retrieval over data capability adapters and Control Plane recipes. | Retrieval temporal compatibility and index manifest contract. |
| Auto-embedding | Optional | Optional substrate through Orchestration, Control Plane, Data Capability/Vector, intelligence/inference above engine. | Derived-state consistency and model execution contract. |
| Query expansion and reranking | Optional | Optional substrate through Retrieval recipes and intelligence/inference above engine. | Explicit model/runtime fallback contract. |
| Retrieval-backed answers | Optional | Optional substrate through Retrieval plus intelligence/inference above engine. | RAG provenance and failure-mode contract. |
| Embedding API | Optional | Optional substrate through intelligence/inference and vector capability ingestion. | Keep separate from mandatory vector support. |
| Model management | Optional | Supported outside engine core through API/config/intelligence/inference; engine may store config only. | Intelligence/inference architecture. |
| Text generation and tokenization | Optional | Supported above engine through inference; engine should not own execution. | Intelligence/inference architecture. |
| Branch lifecycle | Required | Supported by engine Branch plus storage L6/L7/L9 mechanics. | Branch operation contract and capability adapters. |
| Compare, promote, copy, undo | Required | Supported by engine Branch over capability branch adapters and storage history. | Capability merge/diff/revert/copy contracts. |
| Tags and notes | Remove before V1 | Deferred or removed. Engine-next does not assume them as core. | Remove or mark legacy before V1. |
| Spaces | Required | Supported by engine API/Data Capability/Control Plane and capability row encoding. | Space naming/reserved-space contract. |
| Atomic commit substrate | Required | Supported internally by engine Commit and storage L7. | Public batch/write semantics replacing public transaction sessions. |
| Public transaction commands | Remove or redesign | Deferred or removed. Architecture keeps internal commit machinery only. | Remove public begin/commit/rollback path. |
| Data capability import/export | Optional | Supported by engine API/Orchestration and capability codecs. | Stable format coverage by capability. |
| Legacy branch bundles | Remove or redesign | Deferred or removed. Dataset clone replaces branch bundle as V1 artifact. | Remove, hide, or redesign as dataset movement. |
| Health, metrics, durability counters | Required | Supported by engine Diagnostics over storage health/capability facts. | Stable diagnostic DTOs and error codes. |
| Manual durability maintenance | Remove or redesign | Deferred or removed. Maintenance is storage L8/internal lifecycle behavior. | Remove public flush/compact/checkpoint dependence. |
| Automatic durability maintenance | Required internal behavior | Supported by storage L8 and engine Runtime/Diagnostics. | Maintenance observability and fault tests. |
| Config and recipes | Required for retrieval/config subset | Supported by engine API/Control Plane/Retrieval and sensitive config handling. | Config schema and branch-local recipe contract. |
| CLI | Required | Supported above engine through API/serializable command boundary. | CLI product-surface checklist. |
| Serializable command boundary | Required substrate | Supported by engine API and IPC command semantics. | Command DTO and access classification contract. |

## Pathway Cross-Check

### Runtime And Portability

| Pathway | Architecture status | Owner and boundary | Follow-up |
|---|---|---|---|
| 1. Create or open a local embedded database | Supported | Engine API/Runtime opens through Persistence; storage L8 recovers durable local backend. | Open outcome/error contract and local FS conformance. |
| 2. Open an ephemeral cache database | Supported | Engine Runtime selects cache mode; storage maintains in-memory branch/commit state only. | Cache-mode diagnostics and no-durable-object tests. |
| 3. Open a database read-only | Supported, contract needed | Engine command classification rejects writes; storage open/recovery respects read-only constraints. | Write-classification matrix across CLI/SDK/IPC. |
| 4. Share a local database through IPC | Supported, contract needed | Engine owns command semantics and access-mode validation; executor/CLI own transport. | IPC command-boundary and handle-outcome contract. |
| 5. Clone a portable dataset | Supported, contract needed | Engine Orchestration/Data Movement owns clone semantics; storage persists resulting database. | `.strata` artifact spec, validation, provenance. |
| 6. Use a cloned dataset offline | Supported | Clone produces normal database; engine Branch/Data Capability/Retrieval operate locally. | Derived-state rebuild rules after clone. |
| 39. Choose a storage backend intentionally | Supported, contract needed | Storage owns capability validation; engine exposes product diagnostics. | Backend capability table and conformance suite. |

### Data Capabilities

| Pathway | Architecture status | Owner and boundary | Follow-up |
|---|---|---|---|
| 7. Write and read key-value data | Supported | Data Capability/KV over Commit, Persistence, Branch, storage L9. | KV conformance becomes reference capability suite. |
| 8. Write and read JSON documents | Supported, contract needed | Data Capability/JSON owns path semantics and row encoding. | JSON path, secondary index, conflict semantics. |
| 9. Append and query events | Supported, contract needed | Data Capability/Event owns append/order semantics; storage timeline supplies commit time. | Event occurrence-time vs commit-time contract. |
| 10. Create and manage graphs | Supported | Data Capability/Graph owns graph lifecycle and data model. | Graph lifecycle and deletion atomicity. |
| 11. Model graph entities and relationships | Supported, contract needed | Graph capability plus EntityRef, Orchestration, Control Plane. | EntityRef encoding/resolution and dangling-reference policy. |
| 12. Define graph ontology | Supported, contract needed | Graph capability and Control Plane own ontology metadata. | Ontology lifecycle: validation, freeze, mutation rules. |
| 13. Traverse and query graph neighborhoods | Supported, contract needed | Graph capability owns traversal; Branch/Temporal context comes from engine contracts. | Temporal traversal and bounded-result behavior. |
| 14. Run graph analytics | Optional substrate | Graph capability can host analytics without shaping storage. | Feature gate, bounds, determinism. |
| 15. Store and query vectors | Supported, contract needed | Data Capability/Vector owns collections and vector query; Retrieval integrates semantic search. | Vector index, dimension, metric, metadata, temporal query contract. |
| 31. Organize data with spaces | Supported, contract needed | Engine API/Data Capability row encodings and Control Plane reserve system space. | Space naming, reserved system space, branch/clone behavior. |

### Retrieval And Intelligence

| Pathway | Architecture status | Owner and boundary | Follow-up |
|---|---|---|---|
| 16. Run keyword search | Supported, contract needed | Retrieval owns BM25/search over capability adapters and Control Plane manifests. | Source coverage, temporal correctness, stale-index behavior. |
| 17. Run semantic or hybrid search | Supported, contract needed | Retrieval integrates vector capability and model-provided query embeddings. | Fusion, model dependency, and fallback contract. |
| 18. Run graph-aware retrieval | Supported, contract needed | Retrieval consumes graph relationship adapters and EntityRef outputs. | Graph-aware recipe and provenance contract. |
| 19. Use search recipes | Supported, contract needed | Control Plane stores recipes; Retrieval interprets deterministic stages. | Recipe schema, branch-local override, versioning. |
| 20. Use query expansion and reranking | Optional substrate | Retrieval can expose recipe stages; intelligence/inference execute model-dependent work. | Explicit optional model runtime behavior. |
| 21. Ask retrieval-backed questions | Optional substrate | Retrieval returns context; intelligence/inference generate answers above engine. | RAG grounding/provenance and failure behavior. |
| 22. Configure auto-embedding and indexing | Optional substrate | Orchestration coordinates, Control Plane records policy, Vector stores shadow rows. | Derived-state consistency, watermark, repair/reindex contract. |
| 23. Manage models and inference configuration | Optional substrate | Engine may store config; intelligence/inference own execution and provider behavior. | Intelligence/inference architecture, secret handling. |
| 24. Generate, tokenize, and detokenize text | Optional substrate | Inference owns execution; engine/CLI expose intentional command boundary where compiled. | Feature visibility and model-runtime errors. |

### Branching, Versioning, And Time Travel

| Pathway | Architecture status | Owner and boundary | Follow-up |
|---|---|---|---|
| 25. Create and manage branch workspaces | Supported, contract needed | Engine Branch owns product semantics; storage L6/L7/L9 owns mechanics. | Branch lifecycle, delete, same-name race, derived cleanup. |
| 26. Inspect record history | Supported, contract needed | Capability adapters expose history over Persistence/storage L9 versioned rows. | Shared history output shape and retained-history errors. |
| 27. Read data as of a point in time | Supported, contract needed | Storage timeline resolves timestamps to versions; engine applies temporal context. | Shared temporal context and capability temporal conformance. |
| 28. Scrub and explain a branch timeline | Supported, contract needed | Storage persists commit timeline; engine Branch/API explains it. | Timeline resolver API and retention/error semantics. |
| 29. Create a branch from historical state | Supported, contract needed | Engine Branch owns product operation; storage L6/L7 provides retained branch state. | Branch-from-version first, branch-from-time through timeline. |
| 30. Compare, promote, copy, and restore branch changes | Supported, contract needed | Engine Branch coordinates capability branch adapters and internal Commit. | Strict/SourceWins strategy and per-capability conflict rules. |

### Operations And Interfaces

| Pathway | Architecture status | Owner and boundary | Follow-up |
|---|---|---|---|
| 32. Import and export capability data | Optional substrate | Engine API/Orchestration and capability codecs own product semantics. | Stable import/export formats and partial-write rules. |
| 33. Inspect database state | Supported, contract needed | Engine Diagnostics exposes engine/storage/runtime/derived facts. | Stable diagnostic DTOs, health levels, bounded output. |
| 34. Recover from ordinary failures | Supported | Storage L8 owns recovery mechanics; engine Runtime/Diagnostics expose outcomes. | Crash/corruption conformance and recovery error mapping. |
| 35. Configure Strata safely | Supported, contract needed | Engine API/Runtime/Control Plane and sensitive config types own product config. | Config schema, secret redaction, provider/network rules. |
| 36. Run Strata from the CLI | Supported, contract needed | CLI consumes engine command boundary. | CLI command cleanup and JSON-output contract. |
| 37. Use Strata from application code | Supported, contract needed | SDK/API consume engine product surface. | Public API surface/re-export policy. |
| 38. Use Strata in agent or sandbox workflows | Supported, contract needed | CLI/SDK command boundary plus explicit backend/network/model behavior. | Bounded describe/search output and no-hidden-network checks. |

## Explicit Non-Pathway Cross-Check

| Non-pathway | Architecture status | Notes |
|---|---|---|
| Follower mode | Deferred or removed | V1 architecture excludes follower mode. IPC is the local multi-process story. |
| Public begin/commit/rollback workflow | Deferred or removed | Internal commit remains central; public manual transaction sessions are not V1 product model. |
| Legacy branch bundle workflow | Deferred or removed | Dataset clone becomes V1 product artifact. Branch bundles should be removed, hidden, or redesigned. |
| Disk-backed cache mode | Deferred or removed | Cache is ephemeral. Durable disk use is durable database mode. |
| Hidden network behavior | Deferred or removed | Network, provider, clone, model, sync, or registration effects must be explicit. |
| Public tags and notes | Deferred or removed | Not required for local branch model; may return for dataset releases/provenance later. |
| Manual database maintenance | Deferred or removed | Flush, compact, checkpoint, retention, and recovery are internal lifecycle behavior with diagnostics. |

## Findings

No V1 required pathway currently lacks an architecture owner.

The target architecture is equipped to support the V1 product model because:

1. Storage-next owns generic persistence mechanics and commit timeline
   substrate without product capability knowledge.
2. Engine-next owns product semantics: data capabilities, branch behavior, retrieval,
   orchestration, IPC command semantics, diagnostics, and data movement.
3. Intelligence/inference remain above engine for model execution, so search
   and generation features do not corrupt the database boundary.
4. Runtime resource profiling keeps the same-binary edge-to-server promise
   without pushing hardware detection into storage.
5. Removed features are excluded from target architecture instead of preserved
   by inertia.

The main risk is not missing architecture buckets. The main risk is starting
implementation before the follow-up contracts are written. Most V1 pathways
are "Supported, contract needed" because the architecture gives them a home but
does not yet pin exact behavior.

## Required Follow-Up Contracts

Before implementation, the next architecture/product pass should write these
contracts in roughly this order:

1. Data capability implementation contract.
   Repeatable KV/JSON/event/vector/graph pattern over the KV row substrate:
   facade, entity addressing, row families, codecs, reads, writes, branch
   adapter, search adapter, relationship adapter, derived-state hooks, tests.
   Defined in
   `docs/architecture/engine/primitive-implementation-contract.md`.

2. EntityRef and relationship-layer contract.
   Stable entity forms, branch/space/version meaning, graph node binding,
   dangling references, temporal references, reverse maps, and search
   provenance.

3. Engine storage-space ID registry.
   Engine-owned byte assignments for KV, JSON, event, vector, graph, search,
   control-plane, shadow, and relationship rows.

4. Engine persistence adapter contract.
   The only normal storage-facing engine surface: physical key construction,
   latest/version/timestamp/history reads, commit batches, branch mechanics,
   timeline resolution, snapshot/recovery facts, and error mapping.

5. Branch operation and capability branch-adapter contract.
   Branch create, branch-from-version, branch-from-time, compare, promote,
   copy, restore, delete, conflict strategies, derived-state cleanup, and
   per-capability behavior.

6. Temporal context and timeline resolver contract.
   Shared `version`, `as_of`, history, timestamp resolution, retained-history
   errors, tombstone/TTL handling, and temporal search limitations.

7. Control-plane layout contract.
   `_system_` branch and branch-local `_system_` space records for recipes,
   capability registry, storage-space registry, projection manifests,
   watermarks, derived-state status, provenance, and capability facts.

8. Retrieval and derived-state contract.
   Source coverage, recipe schema, BM25/vector/graph stages, temporal
   compatibility, stale indexes, autoembedding watermarks, rebuild/repair, and
   stats/provenance.

9. IPC and serializable command-boundary contract.
   Command DTOs, access mode, read-only write classification, local vs IPC
   handle reporting, structured errors, and transport-independent semantics.

10. Dataset clone artifact contract.
    `.strata` bundle shape, validation, checksums, provenance, branch/version
    metadata, derived-state rebuild markers, and partial-write cleanup.

11. Public API and CLI surface cleanup checklist.
    Remove or hide follower mode, public transaction sessions, legacy branch
    bundles, disk-backed cache, tags/notes, and manual maintenance paths.

12. Product-pathway conformance plan.
    End-to-end tests for the 39 pathways, mapped to the architecture buckets
    and storage/engine fault-injection layers.

## V1 Readiness Rule

For a feature to be V1-ready, it must satisfy all four conditions:

1. The product document says it is Required or intentionally Optional.
2. This support matrix gives it an architecture owner.
3. The relevant follow-up contract exists and names failures, diagnostics, and
   tests.
4. The implementation passes product-pathway tests without upper layers
   bypassing engine or storage learning data capability semantics.
