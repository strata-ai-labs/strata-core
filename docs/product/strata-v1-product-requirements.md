# Strata V1 Product Requirements

Status: V1 product requirements

## Introduction

Strata V1 is an embedded, local-first database for applications and AI systems
that need durable state, versioned data, retrieval-oriented capabilities, and
portable deployment in one coherent product.

The core promise is:

> Open a Strata database where the application runs, store related data across
> multiple shapes, branch it, inspect it, search it, recover it, and move it
> without operating a separate database server.

This document is the product anchor for V1. It defines what Strata is trying to
be, which user outcomes matter, which capabilities are required, and which
boundaries future architecture must serve. Contributors should read this before
working on core, storage, engine, executor, intelligence, inference, CLI, tests,
or public documentation.

This is not an implementation plan, API reference, release checklist, or
marketing document. The implementation plans should be written from this
product model, not the other way around.

## Related Documents

These documents refine this product anchor:

1. `docs/product/strata-v1-feature-inventory.md`
2. `docs/product/strata-v1-user-pathways.md`
3. `docs/product/strata-v1-non-functional-requirements.md`
4. `docs/product/strata-v1-architecture-support-matrix.md`

Focused product direction documents:

1. `docs/product/strata-v1-branching-direction.md`
2. `docs/product/strata-v1-graph-relationship-layer.md`
3. `docs/product/strata-v1-versioning-time-travel.md`
4. `docs/product/strata-v1-cli-sdk-experience.md`
5. `docs/product/stratahub-product-direction.md`

Pathway expansions:

1. `docs/product/pathways/runtime-and-portability.md`
2. `docs/product/pathways/data-capabilities.md`
3. `docs/product/pathways/retrieval-and-intelligence.md`
4. `docs/product/pathways/branching-versioning-time-travel.md`
5. `docs/product/pathways/operations-and-interfaces.md`

## Requirement Language

1. Must means V1 is incomplete without it.
2. Should means V1 should include it unless later product or architecture work
   produces a clear deferral.
3. May means allowed but not required for V1.

## Product Thesis

Strata is not just a key-value store, vector database, graph store, search
index, or event log. Its product value is the integrated model:

1. Embedded database operation.
2. Durable local state.
3. Multiple data capabilities in one database.
4. Branches and versions as first-class product concepts.
5. Search, graph, and vector capabilities built around stored data.
6. Cloneable datasets for cold start and distribution.
7. Portable storage targets beyond standard disk-backed machines.

Most databases make users choose between local embeddability, rich retrieval,
branching, and portability. Strata V1 should make those capabilities feel like
parts of one natural database experience.

## Target Users

V1 is designed for:

1. Application developers who want an embedded database with predictable local
   behavior and a small operational footprint.
2. AI and agent developers who need durable memory, retrieval, events, graph
   relationships, vectors, and versioned experimentation in one place.
3. Local-first and edge developers who need a database that can run without a
   server and can adapt to constrained or portable storage environments.
4. Infrastructure and tooling developers who need branchable, inspectable, and
   reproducible data state for workflows, tests, and automation.
5. Dataset users who want to clone a useful database, open it locally, branch
   it, modify it, search it, and keep working offline.

Post-V1, StrataHub may add dataset discovery, publishing, public dataset
lineage, and fleet visibility. V1 should establish the local database and clone
model that makes those future products possible, but StrataHub Fleet is not a
V1 product requirement.

## Product Principles

1. Embedded first.
   A developer should be able to open Strata inside their process without
   running a separate service.

2. Durable by default, ephemeral only when explicit.
   Normal databases should prioritize crash safety and recovery. Cache mode
   should be visibly non-durable.

3. One database for related data.
   Users should not need separate systems for operational records, JSON
   documents, events, graph relationships, embeddings, and search metadata when
   those records belong to the same workflow.

4. Branching and versioning are core.
   Branches, versions, history, comparison, restore, and branch-from-history
   are product features, not debugging utilities.

5. Retrieval is part of the database experience.
   Search, vectors, graph-aware retrieval, recipes, and RAG should work with
   stored data rather than living as a disconnected side system.

6. Product APIs should be explainable.
   A new user should understand database, branch, space, record, version, time,
   search, relationship, clone, and restore without reading implementation
   history.

7. Reliability outranks feature count.
   A smaller trustworthy surface is better than a broad surface with ambiguous
   recovery, indexing, or retention behavior.

8. Portability is a requirement.
   Strata should own its storage capability contract. OpenDAL can be a
   first-class adapter family, but Strata must not tie its correctness model to
   any single storage abstraction.

9. Architecture serves the product.
   Storage, engine, core, executor, intelligence, inference, and CLI boundaries
   should exist because they make product behavior easier to satisfy, test, and
   evolve.

10. Runtime adaptation is a product feature.
    The same Strata binary should scale from constrained edge devices to
    server-class machines by detecting the runtime envelope, respecting explicit
    user budgets, and choosing safe resource defaults.

## Core Product Concepts

### Database

A Strata database is an embedded database rooted at a local path or supported
storage address. It contains branches, spaces, user records, derived state,
configuration, indexes, and durability metadata.

### Cache Database

A cache database is explicitly ephemeral. It is useful for temporary state,
tests, and local computation. It must not be confused with durable database
mode.

### Branch

A branch is a named line of database state. Branches let users isolate work,
compare outcomes, restore from mistakes, and create new workspaces from current
or historical state.

### Version

A version is a committed point in database history. Versions are the basis for
history, time travel, comparison, restore, branch points, and reproducibility.

### Time

Time is a user-facing way to select historical state. Time-based operations must
resolve to retained database state rather than guessing.

### Space

A space is a logical namespace inside a database and branch. Spaces let users
separate application domains without opening separate databases.

### Data Capability

A data capability is a user-visible way to store, relate, or retrieve data. V1
data capabilities include key-value, JSON, events, graph, vectors, and search.

The term primitive may still appear in implementation code. Product
documentation should prefer data capability unless the lower-level distinction
matters.

### Commit

A commit is the supported unit of change. Users should be able to rely on clear
commit behavior without managing public begin/commit/rollback sessions.

### Derived State

Derived state includes search indexes, vector indexes, graph relationship
indexes, auto embeddings, and recipe outputs. Derived state must not silently
contradict user-authored data.

### Storage Backend

A storage backend is the persistence substrate behind a database. Local
filesystem storage is the reference durable backend. Other backends must prove
that they satisfy Strata's capability and correctness requirements before they
are documented as production-ready.

### Dataset

A dataset is a portable Strata artifact that can be cloned into a database
location. After clone, the destination is a normal Strata database under the
user's control.

## V1 Required Product Experience

### Opening Databases

Users must be able to:

1. Open or create a durable local database at a filesystem path.
2. Open an explicit ephemeral cache database.
3. Open an existing database read-only.
4. Receive clear errors for lock conflicts, unsupported backends, invalid
   configuration, corruption, permission failures, and recovery failures.
5. Use the same product model for supported storage addresses, not just local
   paths.

### Local Multi-Process Access

IPC-backed local access is required for V1. Strata AI makes same-machine shared
access a normal product path: an application and the agentic assistant may both
need to use the same database at once. IPC is the supported way to share an
already-open local database without introducing unsafe concurrent writers or
follower refresh semantics.

Follower mode is not a V1 product pathway.

### Durable Behavior And Recovery

Durable Strata databases must recover committed data after ordinary process
crashes. Recovery must be deterministic, observable, and honest about degraded
or failed states.

Users should not have to manually flush, compact, checkpoint, or apply
retention during normal use.

### Data Capabilities

V1 must support:

1. Key-value records for simple application state.
2. JSON documents for structured records.
3. Append-only events for audit, history, and timeline workflows.
4. Graph nodes, edges, ontology metadata, relationship modeling, traversal, and
   optional bounded analytics.
5. Vector collections, vector records, metadata, and nearest-neighbor search.
6. Search and retrieval over stored data.
7. Spaces as a first-class namespace across data capabilities.

Storage must remain data-capability agnostic. Product semantics for KV, JSON,
events, graph, vectors, search, and recipes belong above storage.

### Graph Relationships

Graph must have two V1 roles:

1. A standalone graph data capability.
2. A relationship layer across Strata records.

Users should be able to connect KV records, JSON documents, events, vector
records, graph-native nodes, and search results without copying source payloads
into graph properties.

Entity references, branch semantics, space semantics, version semantics, and
derived relationship behavior must be explicit enough to trust.

### Search, Retrieval, And Intelligence

Users must be able to search stored data through product-level retrieval APIs.
V1 should support keyword, semantic, hybrid, graph-aware, recipe-driven, and
retrieval-augmented workflows where the required indexes and model runtimes are
available.

The database must not depend on a specific model provider. Model-dependent
features should be explicit, observable, and optional where runtime support is
not configured.

Generation, tokenization, detokenization, model management, query expansion,
reranking, and RAG are allowed V1 product utilities when supported by the
compiled product and configuration. They must not be confused with storage or
durability correctness.

### Branching

Users must be able to:

1. Create and inspect branches.
2. Create a branch from existing data.
3. Select branch context for reads and writes.
4. Compare branches by space and data capability.
5. Preview conflicts before promoting branch changes.
6. Promote branch changes where conflicts can be resolved.
7. Copy selected records or selected changes between branches.
8. Delete branches safely.

Product language should describe user actions directly: create branch, compare,
promote, copy, and restore. Git-like terms may remain in APIs where useful, but
they should not be the primary mental model.

### Versioning And Time Travel

Users must be able to:

1. Inspect record history.
2. Read data as of a timestamp or retained version where supported.
3. Compare branch state at a point in time.
4. Restore a bad version range by writing a compensating change.
5. Create a branch from historical state once the selected point can be resolved
   to retained database state.

Commit versions are the authoritative ordering. Timestamps are user-facing
selectors. Branch-from-time must resolve to a concrete retained commit point; it
must not guess.

### Atomic Commit Units

V1 must provide clear commit boundaries for supported write operations and
batch APIs.

Users should not have to use public begin/commit/rollback commands. Internal
transaction machinery may remain, but public transaction sessions are not a V1
product requirement.

Strata should not claim broad ACID compliance unless the scope, backend
requirements, isolation behavior, and durability mode are defined and tested.

### Dataset Clone And Data Movement

Clone is the cold-start path:

```text
strata clone <source> <destination>
```

After clone, the destination must be a normal Strata database. Users should be
able to open it, branch it, modify it, search it, export it, and work offline
without contacting the source.

V1 should also support explicit import/export paths where semantics are clear,
including Arrow or other structured formats where supported.

Branch bundles are not a V1 product artifact.

### Storage Portability

V1 must define Strata's backend capability contract.

Required direction:

1. Local filesystem is the reference durable backend.
2. Cache/browser/WASM targets must have explicit non-durable or constrained
   semantics.
3. Object-storage-backed targets should be designed into the product model.
4. OpenDAL should be supported as an adapter family where backend capabilities
   can satisfy Strata's requirements.
5. Unsupported backend capabilities must fail with explicit errors.

V1 must not claim every OpenDAL backend is production-ready.

### Operations And Inspection

Users must be able to understand a database through bounded product surfaces:

1. Info.
2. Describe.
3. Health.
4. Metrics.
5. Durability counters.
6. Structured errors.

Runtime resource profiling should be visible through these surfaces. Users
should be able to see the selected resource profile, effective memory budgets,
cache and maintenance settings, and whether values were auto-derived or
user-specified.

These surfaces should help users understand state without exposing internal
crate history or requiring manual maintenance.

### CLI And SDK

V1 must provide:

1. A CLI suitable for terminals, scripts, and structured JSON automation.
2. SDK APIs suitable for embedded application use.
3. Consistent behavior between CLI, SDK, and the serializable command boundary
   where they overlap.
4. Stable error categories for product workflows.
5. Explicit feature availability when optional intelligence or backend features
   are missing.

## V1 Optional Capabilities

The following may ship in V1 if they are reliable, documented, and do not
distort the required architecture:

1. Graph analytics beyond core traversal and relationship behavior.
2. Query expansion and reranking.
3. Retrieval-backed answer generation.
4. Auto-embedding and reindex workflows.
5. Model management and generation utilities.
6. Data capability import/export beyond clone-critical paths.

Optional capabilities must degrade honestly when runtime support is missing.

## Remove Or Redesign Before V1

These surfaces should not define V1:

1. Follower mode.
2. Public begin/commit/rollback transaction commands.
3. Disk-backed cache mode.
4. Legacy branch bundle workflow.
5. Public tags and notes.
6. Normal-user flush, compact, checkpoint, or retention workflows.
7. Hidden network behavior.
8. Claims that every OpenDAL backend is production-ready.

Some of these may return later in redesigned form. They should not shape the
V1 product model by inertia.

## Non-Goals

Strata V1 is not:

1. A distributed active-active multi-writer database.
2. A managed cloud database service.
3. A replacement for every specialized graph, vector, search, or analytics
   system.
4. A data warehouse or lakehouse.
5. A universal ORM.
6. A public StrataHub fleet-management product.
7. A storage-format redesign.
8. A compatibility-preserving release for pre-V1 internal APIs.
9. A general-purpose public multi-command ACID transaction product.

Strata is pre-V1. We may break earlier experimental or internal surfaces when
doing so produces a better V1 product.

## Non-Functional Requirements Summary

The dedicated NFR document is the authority for this area. V1 architecture must
serve at least these expectations:

1. Reliability and durability.
   Durable data must recover predictably after ordinary crashes.

2. Correctness.
   Branch, version, time-travel, commit, and derived-state behavior must be
   explicit and testable.

3. Portability.
   Backend support must be capability-driven, not assumed.

4. Performance and resource use.
   Common embedded workflows must be efficient, and large operations must be
   bounded or paginated where needed.

5. Security and privacy.
   Secrets must be redacted and network behavior must be explicit.

6. Observability.
   Errors, health, metrics, search stats, indexing status, and recovery state
   must be understandable.

7. Testability.
   The architecture must support deterministic unit tests, integration tests,
   crash recovery tests, fault injection, fuzzing, and backend conformance.

## Architecture Implications

Future architecture work should follow these implications:

1. Storage-next should be a world-class persistence substrate, not a
   data-capability-aware product engine.
2. Engine-next should own product semantics: branches, data capabilities,
   versioning, time travel, retrieval behavior, lifecycle orchestration, and
   user-facing errors.
3. Core-next should define shared vocabulary and contracts that genuinely
   belong below both storage-next and engine-next.
4. Executor, intelligence, inference, and CLI should be shaped after the V1
   product model is stable.
5. Production code above engine should not access storage directly unless there
   is a documented product or testing reason.
6. Backend portability should be designed through Strata's capability contract,
   not scattered filesystem assumptions.

The next architecture pass should start from the product model in this
document, not from the current crate graph.

## Decision Rules

When a proposed feature, refactor, or architecture change is ambiguous:

1. If it does not serve the V1 product thesis, defer it.
2. If it makes reliability or recovery harder to test, reject it or redesign it.
3. If it puts data capability semantics into storage, reject it.
4. If it creates a public concept users cannot understand, rename or reshape it.
5. If it exists only to preserve pre-V1 internal compatibility, question it.
6. If it blocks future portability without improving V1 reliability, avoid it.
7. If it broadens scope beyond embedded local-first Strata, move it to post-V1.
8. If it assumes POSIX filesystem semantics, prove the assumption belongs in the
   product or isolate it behind the backend capability contract.

## V1 Definition Of Done

Strata V1 is ready when:

1. Required product pathways are documented and implemented or explicitly
   deferred out of V1.
2. Remove/redesign surfaces no longer define the public product.
3. Required data capabilities have clear branch, space, version, and error
   semantics.
4. Durable local filesystem behavior passes the recovery and conformance bar.
5. Portable backend claims match tested backend capabilities.
6. CLI and SDK surfaces match the product model.
7. Search, graph, vector, and intelligence features expose honest availability
   and failure behavior.
8. The architecture can support storage-next, engine-next, and core-next without
   preserving historical crate boundaries by inertia.
