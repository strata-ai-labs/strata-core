# Engine-Next Data Capability Implementation Contract

Status: V1 architecture draft

## Purpose

This document defines the repeatable implementation contract for Strata's
engine-owned data capabilities:

1. Key-value
2. JSON
3. Event
4. Vector
5. Graph

The codebase today still uses `PrimitiveType`, `EntityRef` variants, `TypeTag`,
and primitive-named snapshot sections. Those names are current implementation
and compatibility vocabulary. The actual storage mechanics are already closer
to the target model: Strata stores branch-aware, versioned `Key`/`Value` rows,
and higher-level capabilities encode their semantics into row keys, row values,
indexes, and derived state.

The goal is to prevent engine-next from becoming five unrelated mini-engines.
Each capability has different product semantics, but every capability should
map those semantics onto the same architectural shape:

```text
product operation
  -> capability facade
  -> capability validation and semantic types
  -> entity addressing
  -> row-family and value encoding
  -> internal commit batch
  -> persistence adapter
  -> storage-next L9
```

Storage remains data-capability agnostic. Storage sees physical keys, opaque
storage-space IDs, row bytes, versions, timestamps, tombstones, and branch
mechanics. Storage does not know whether a row represents KV, JSON, event,
vector, graph, search, recipe, ontology, or relationship data.

The target mental model is:

```text
branch-aware MVCC KV row substrate
        ^
        |
engine data capabilities: KV, JSON, event, vector, graph
        ^
        |
retrieval, relationships, autoembedding, recipes, branch workflows
```

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/strata-v1-user-pathways.md`
4. `docs/product/strata-v1-architecture-support-matrix.md`
5. `docs/product/strata-v1-graph-relationship-layer.md`
6. `docs/product/strata-v1-branching-direction.md`
7. `docs/product/strata-v1-versioning-time-travel.md`
8. `docs/architecture/strata-v1-architecture.md`
9. `docs/architecture/engine-architecture.md`
10. `docs/architecture/storage-architecture.md`
11. `docs/architecture/storage/l9-storage-api-boundary.md`

Follow-up contracts that depend on this one:

1. EntityRef and relationship-layer contract.
2. Engine storage-space ID registry.
3. Engine persistence adapter contract.
4. Branch operation and capability branch-adapter contract.
5. Temporal context and timeline resolver contract.
6. Control-plane layout contract.
7. Retrieval and derived-state contract.

## Requirement Language

1. Must means the capability contract is incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Definition

The only physical storage primitive is a branch-aware MVCC KV row:

```text
branch + space + storage_space_id + user_key
  -> versioned value bytes + timestamp + tombstone + optional retention facts
```

A data capability is an engine-owned product capability that maps product
operations onto that row substrate through the engine persistence adapter.

Data capability examples:

1. KV record
2. JSON document or JSON path
3. Event stream entry
4. Vector collection and vector record
5. Graph, node, edge, ontology record, and relationship binding

Not data capabilities:

1. Search
2. Autoembedding
3. Query expansion
4. Reranking
5. RAG answer generation
6. Recipes
7. Derived search indexes
8. Shadow vector maintenance
9. Graph-aware retrieval
10. Branch merge workflow

Those are retrieval, orchestration, control-plane, intelligence, or branch
services over data capability contracts.

Current code evidence:

1. `strata_storage::Key` combines `Namespace`, `TypeTag`, and user-key bytes.
2. `TransactionContext` buffers generic `HashMap<Key, Value>` writes and
   generic deletes.
3. WAL transaction payloads serialize `Vec<(Key, Value)>`, deletes, and TTLs.
4. `SegmentedStore` applies generic rows into branch-local MVCC memtables and
   segments.
5. KV, JSON, event, vector, and graph facades each build typed storage keys
   and encode values on top of the same generic row path.

The target architecture preserves that insight while removing the need for
storage to understand product capability semantics.

## Binding Decisions

1. **There is one physical storage primitive: the branch-aware MVCC KV row.**
   Engine-next must not model JSON, event, vector, or graph as separate storage
   engines. They are product capabilities layered over the same row substrate.

2. **KV is the reference capability and the thinnest product layer over the row
   substrate.**
   KV should define the simplest version of latest read, version read,
   timestamp read, history, list, write, delete, branch diff, branch promotion,
   and search projection. Other capabilities explain what semantic rules,
   codecs, indexes, and derived state they add.

3. **JSON, event, vector, and graph are not peer storage primitives.**
   They are data capabilities that encode documents, append-only records,
   vector records/index metadata, and graph relationship facts into KV-shaped
   rows with capability-owned keys and values.

4. **Every capability must participate in branch, space, version, timestamp,
   and history semantics.**
   A specific operation may be unsupported, but unsupported behavior must be
   explicit and diagnosed. It must not silently fall back to current-state-only
   behavior.

5. **A capability owns its semantic validation and row encoding.**
   Branch, retrieval, and orchestration may ask a capability to interpret rows
   through capability traits. They must not decode capability row values by
   hand.

6. **Capabilities do not call sibling capability internals.**
   Graph cannot secretly fetch JSON. Vector cannot secretly fetch KV. Event
   cannot update graph directly. Cross-capability behavior goes through
   orchestration, branch services, retrieval services, or public capability
   traits.

7. **Authored data and derived data are separate.**
   User-authored capability rows participate in branch compare, promote, copy,
   restore, history, import/export, and clone. Derived rows usually rebuild,
   validate, or report stale status. They must not masquerade as primary user
   data.

8. **Graph remains a product data capability and becomes the relationship
   layer.**
   Graph supports native graph data and relationship bindings to KV, JSON,
   event, vector, and graph entities without requiring payload duplication.

9. **Vector remains a product data capability and becomes the
   shadow-embedding target.**
   User-managed vector collections and autoembedding shadow vectors must remain
   distinct in addressing, control-plane metadata, diagnostics, and branch
   behavior.

10. **Branch behavior is centrally coordinated but capability-aware.**
    Branch workflows live in the branch bucket. Capability adapters define how
    each capability diffs, merges, copies, restores, and cleans up.

11. **Search consumes capability adapters.**
    Retrieval may index, project, and query capability data through capability
    search/text/vector/graph adapters. Retrieval must not become a second
    storage implementation for authored data.

12. **Persistence is the only normal storage-facing path.**
    Capability production code should use the engine persistence adapter. It
    should not import storage-next internals directly.

## Capability Anatomy

Every data capability should be organized around the same conceptual parts.

### 1. Facade

The facade is the capability's product-facing handle or service.

It owns:

1. Public operation names.
2. Product input/output DTOs.
3. Default branch and space context interpretation where the API provides it.
4. Read-only/write classification metadata.
5. Mapping between public errors and capability diagnostics.

It must not own:

1. Storage row bytes.
2. Branch merge algorithms.
3. Background rebuild loops.
4. Model provider execution.
5. Sibling capability calls.

Examples:

1. KV facade: put, get, delete, list, scan, batch, history.
2. JSON facade: set, get, delete path, list, count, history.
3. Event facade: append, range, range by time, list types.
4. Vector facade: create collection, upsert, query, stats.
5. Graph facade: create graph, add node, add edge, neighbors, ontology.

### 2. Semantic Types

Each capability owns its semantic types:

1. User keys and record IDs.
2. Capability-specific values.
3. Metadata.
4. Configuration.
5. Validation limits.
6. Result types.
7. Capability-specific error fragments.

Semantic types should be stable enough for API and testing, but they should not
leak storage byte format details.

### 3. Entity Addressing

Every capability must define how product records become typed `EntityRef`s.

`EntityRef` is engine-owned. The capability contract must define what each
capability contributes to a reference and how that reference resolves back to a
record.

Target examples:

| Capability | Entity examples |
|---|---|
| KV | branch, space, key |
| JSON | branch, space, document key, optional path |
| Event | branch, space, event stream/type, sequence or event id |
| Vector | branch, space, collection, vector key |
| Graph | branch, space, graph name, node id, edge id, bound source entity |

Rules:

1. Entity addressing is a product concept, not a storage concept.
2. Storage does not need to reconstruct an `EntityRef`.
3. If a capability needs reverse lookup from row to entity, it owns engine rows
   or indexes for that purpose.
4. Entity references must carry enough branch and space context to avoid
   accidental cross-branch or cross-space resolution.
5. Relationship-layer references must distinguish graph-native identity from
   referenced source entity identity.

The exact `EntityRef` format belongs in the EntityRef contract.

### 4. Storage-Space Usage

Every capability must declare which engine-owned storage spaces it uses.

Storage-space IDs are opaque bytes from storage's perspective. Storage routes
and stores them; engine owns their meaning.

A storage-space ID partitions the KV row keyspace. It does not create a new
storage primitive or give storage ownership of product semantics.

The capability contract requires each capability to document:

1. Authored data spaces.
2. Metadata spaces.
3. Secondary index spaces.
4. Derived or rebuildable spaces.
5. Shadow/system spaces.
6. Reserved future spaces, if any.

The assignments themselves belong in the engine storage-space ID registry.

### 5. Key Encoding

Each capability owns the mapping from product identity to logical row key.

The key encoding must define:

1. Branch context.
2. Space context.
3. Capability-specific object identity.
4. Collection or graph identity where relevant.
5. Secondary-index identity where relevant.
6. System-space or derived-state marker where relevant.
7. Prefix/range behavior for list, scan, and cleanup.

Rules:

1. Keys must be deterministic.
2. Keys must support branch-local isolation.
3. Keys must support prefix/range queries needed by product APIs.
4. Keys must not require storage to decode capability semantics.
5. Keys must leave room for future format evolution through explicit versioning
   or registry rules.

### 6. Value Encoding

Each capability owns how semantic values become durable row bytes.

The value encoding must define:

1. Authored value bytes.
2. Metadata bytes.
3. Tombstone representation where the capability needs one above storage's
   tombstone fact.
4. TTL metadata where supported.
5. Schema or codec version.
6. Collection/graph/ontology config bytes.
7. Derived-state bytes, if the capability owns rebuildable state.

Rules:

1. Value bytes must be deterministic and documented before V1 format freeze.
2. Unknown value versions must fail clearly or be explicitly forward-compatible.
3. Capability value codecs must reject malformed bytes with typed corruption
   errors.
4. Derived value encodings must identify themselves as derived or rebuildable.
5. Value encodings must not depend on sibling capability internals.

### 7. Read Contract

Every capability must define these read forms:

1. Latest read.
2. Version read.
3. Timestamp read through the shared temporal context.
4. History read.
5. Existence read.
6. List or scan.
7. Count or bounded summary where product APIs expose it.

Rules:

1. Latest means latest visible committed value in the selected branch and
   space.
2. Version read means value visible at or before a specific commit version.
3. Timestamp read means value visible at or before the commit version resolved
   by the storage commit timeline.
4. History must distinguish missing records, deleted records, tombstones,
   trimmed history, malformed historical values, and unsupported history.
5. List/scan ordering must be deterministic enough for automation.
6. Read paths must surface stale derived state instead of returning
   misleading results.

### 8. Write Contract

Every capability must define how product writes produce internal commit batches.

Write operations include:

1. Insert or put.
2. Update.
3. Delete.
4. Batch write.
5. Append where applicable.
6. Metadata/config write where applicable.
7. Derived-state write where applicable.

Rules:

1. Public manual begin/commit/rollback sessions are not the product model.
2. Internal commit batches remain central and must be explicit.
3. Each normal public write is one commit.
4. Explicit capability batch APIs, such as KV batch put/delete, are atomic
   within that capability.
5. Cross-capability public batches are not a V1 requirement. Cross-capability
   internal work may still use engine commit machinery when orchestration owns
   the operation.
6. A capability write must validate before mutating visible state.
7. A capability write must respect read-only mode before mutation.
8. A capability write must classify authored rows and derived rows distinctly.
9. Batch atomicity or partial-failure behavior must be explicit.
10. Write conflicts must produce structured errors.
11. Derived writes triggered after authored commits must be observable and
   repairable if they fail.

### 9. Branch Adapter

Every capability must provide a branch adapter.

The branch adapter is used by branch workflows. It must define:

1. Diff behavior.
2. Three-way comparison behavior.
3. Promote/merge behavior.
4. Source-wins behavior, where supported.
5. Strict conflict behavior.
6. Copy selected record behavior.
7. Restore/revert behavior.
8. Branch delete cleanup.
9. Branch-from-version and branch-from-time implications.
10. Derived-state handling during branch operations.

Rules:

1. Branch workflows coordinate; capabilities interpret their own rows.
2. Strict conflict handling must be the safe baseline.
3. Source-wins must report what it overwrites.
4. Derived indexes should usually rebuild, validate, or mark stale instead of
   merging as authored data.
5. Branch delete must clean or invalidate branch-local derived state.
6. Same-name branch recreation must not inherit stale capability or derived
   rows.

The exact branch workflow contract belongs in the branch operation contract.

### 10. Search And Text Adapter

Every capability must declare whether and how it participates in retrieval.

Possible adapters:

1. Text projection.
2. Keyword indexing.
3. Snippet source.
4. Vector search source.
5. Graph expansion source.
6. Search result entity resolution.
7. Temporal search compatibility check.

Rules:

1. Retrieval consumes capability adapters; it does not decode capability rows by
   hand.
2. A capability must declare what source fields are searchable by default.
3. Search output must return traceable entity references.
4. Historical search must either be correct for the requested temporal context
   or fail/degrade explicitly.
5. Missing, stale, rebuilding, or incompatible indexes must be visible in
   diagnostics and result stats.

### 11. Relationship Adapter

Every capability must declare whether its records can participate in graph
relationships.

The relationship adapter must define:

1. Which records are entity-addressable.
2. How a graph-bound node references the source capability record.
3. How graph traversal results resolve back to the source record.
4. What happens when the source record is deleted.
5. What happens when the source record is not visible at a temporal point.
6. Whether sub-record references are allowed, such as JSON paths.
7. How branch and space context is preserved.

Rules:

1. Relationship modeling must not require payload duplication.
2. Native graph nodes and bound graph nodes must be distinguishable.
3. Dangling references must be explicit, not silently ignored.
4. Temporal relationship resolution must use the same temporal context as
   capability reads.
5. Relationship indexes or reverse maps are engine-owned rows, not storage
   concepts.

### 12. Derived-State Hooks

Capabilities may maintain derived state, but derived state must be explicit.

Derived-state examples:

1. JSON secondary indexes.
2. Vector ANN indexes.
3. Shadow embedding collections.
4. Graph traversal projections.
5. Graph relationship reverse maps.
6. Search projection rows.
7. Recipe expansion caches.

The capability must define:

1. What authored rows the derived state depends on.
2. Whether the derived state is authoritative or rebuildable.
3. Where its manifest lives.
4. How watermarks are recorded.
5. How stale state is detected.
6. How rebuild and repair happen.
7. How branch copy/delete/promote affects it.
8. How clone/import/export affects it.
9. How diagnostics report it.

Rules:

1. Rebuildable state should be marked rebuildable in control-plane metadata.
2. Authoritative metadata must participate in branch/history semantics.
3. Derived-state failure after source commit must not be hidden.
4. Users should not have to run manual maintenance in normal use.
5. Low-resource profiles may defer or limit derived-state work before
   weakening authored data correctness.

### 13. Diagnostics And Errors

Each capability must expose diagnostics that map into the V1 error and health
model.

Capability diagnostics include:

1. Validation failures.
2. Missing record.
3. Missing branch or space.
4. Read-only write rejection.
5. Unsupported temporal mode.
6. History trimmed or unavailable.
7. Conflict.
8. Malformed value bytes.
9. Stale or rebuilding derived state.
10. Degraded capability state.
11. Resource exhaustion.
12. Backend or persistence failure through mapped storage facts.

Rules:

1. Capability errors must map into stable product error codes.
2. Corruption must identify the capability, branch, space, and entity when
   known.
3. Derived-state degradation must not look like authored-data corruption unless
   authored rows are actually corrupt.
4. Retry policy must be explicit where ambiguous commit outcomes are possible.
5. Diagnostics must be bounded and safe in read-only mode.

### 14. Conformance Tests

Every capability must pass shared conformance tests plus capability-specific
tests.

Shared data capability conformance:

1. Latest read after write.
2. Delete and tombstone visibility.
3. Version read.
4. Timestamp read.
5. History read.
6. Missing record behavior.
7. Branch isolation.
8. Space isolation.
9. Branch copy/promote/restore participation.
10. Read-only write rejection.
11. Malformed value decoding.
12. Retained-history trimmed error.
13. Import/export compatibility where supported.
14. Diagnostics for stale or degraded derived state where applicable.

Capability-specific conformance:

1. KV prefix/range/list behavior.
2. JSON path mutation and document validity.
3. Event append ordering and time-range query.
4. Vector dimension, metric, metadata, query, and collection behavior.
5. Graph node, edge, ontology, traversal, relationship, and analytics behavior.

Tests must not rely on storage internals except through the engine persistence
test double or storage-next integration tests.

## Capability-Specific Direction

### KV

KV is the reference capability and the thinnest facade over the row substrate.

It should define:

1. Simple key identity.
2. Put/get/delete/list/scan semantics.
3. Batch behavior.
4. History output shape.
5. Basic text projection where values are text-like or metadata declares text.
6. Simple branch conflict behavior: same key changed differently.

KV should avoid special cases that other capabilities cannot follow.

### JSON

JSON is a structured document capability over KV-shaped rows.

It should define:

1. Document key identity.
2. Optional path identity for reads and mutations.
3. Whole-document and path-level update behavior.
4. JSON validity and type mismatch errors.
5. History output for document values and path-level queries.
6. Branch conflict behavior for document/path changes.
7. Secondary index behavior if indexes remain in V1.
8. Text projection for search.
9. Relationship references to documents and, if supported, paths.

V1 branch baseline:

1. JSON merge granularity is document-level.
2. Disjoint path edits still conflict if source and target changed the same
   document differently since the branch point.
3. Path-level merge is a post-V1 extension unless a dedicated JSON merge
   contract and conformance suite are written before freeze.

### Event

Event is an append-oriented capability over KV-shaped rows.

It should define:

1. Event stream/type identity.
2. Sequence allocation.
3. Event-domain timestamp vs commit timestamp.
4. Append and batch append atomicity.
5. Query by sequence, type, range, and time.
6. History meaning for append-only records.
7. Branch promotion behavior for divergent appends.
8. Search/text projection for event payloads.
9. Relationship references to events.

V1 event baseline:

1. How does V1 expose event-domain time in SDK/CLI output and filters?
2. Branch promotion refuses divergent appends in source-wins and strict modes.
   V1 does not reorder, delete, or rewrite append-only event history during
   promotion.

### Vector

Vector is a standalone product capability and the target for shadow embeddings.

It should define:

1. Collection identity.
2. Vector key identity.
3. Collection config: dimension, metric, dtype, metadata schema where present.
4. User-authored vector rows.
5. Shadow embedding rows as distinct derived state.
6. Metadata filtering.
7. Vector query determinism and tie behavior.
8. Temporal vector query behavior.
9. Branch conflict behavior for collection config and records.
10. Search integration for semantic and hybrid retrieval.
11. Relationship references to vector records.

Rules:

1. User-supplied embeddings must not be forced into autoembedding workflows.
2. Shadow vectors must be branch-local and observable.
3. ANN indexes must be rebuildable or clearly authoritative.
4. Dimension and metric mismatches must fail before mutation.

### Graph

Graph is a standalone product capability and the relationship layer across data
capabilities.

It should define:

1. Graph identity.
2. Node identity.
3. Edge identity.
4. Native graph nodes and edges.
5. Bound nodes that reference `EntityRef`s.
6. Node and edge properties.
7. Ontology object and link types.
8. Ontology lifecycle: draft, frozen, validation, or whatever V1 chooses.
9. Traversal and neighbor query behavior.
10. Temporal traversal behavior.
11. Graph analytics if enabled.
12. Branch conflict behavior for nodes, edges, ontology, and relationships.
13. Search projection and graph-aware retrieval adapters.
14. Relationship reverse maps and dangling-reference diagnostics.

Rules:

1. Graph must not require payload duplication from KV, JSON, event, or vector.
2. Graph-native nodes remain valid even when no source entity exists.
3. Bound graph nodes must resolve through `EntityRef`.
4. Relationship traversal must preserve branch, space, and temporal context.
5. Ontology must not become an undocumented side channel for validation.

## Cross-Capability Behavior

Cross-capability behavior must be service-executed, not hidden inside one
capability's CRUD implementation.

### Autoembedding

Target flow:

```text
text-projectable capability write
  -> commit fact
  -> orchestration reads branch-local embedding policy
  -> intelligence/inference produces embedding when explicitly configured
  -> vector capability writes shadow vector row
  -> control plane records watermark/status
```

Rules:

1. The source capability exposes text projection.
2. Orchestration coordinates model work and shadow writes.
3. Vector stores shadow rows through its capability contract.
4. Model execution remains above engine.
5. Failure is visible through derived-state diagnostics.

### Relationship Layer

Target flow:

```text
entity-addressable capability record
  -> explicit relationship command or declared policy
  -> graph capability stores relationship node/edge
  -> traversal returns EntityRef values
  -> caller fetches source records through owning capability
```

Rules:

1. Entity-addressable capabilities expose references.
2. Graph stores graph facts.
3. Source payloads remain owned by source capabilities.
4. Dangling, stale, missing, and historical references are diagnosed.

### Search Projection

Target flow:

```text
searchable capability write
  -> projection fact
  -> retrieval/orchestration updates index
  -> control plane records manifest and watermark
  -> search uses index only when compatible with branch/time request
```

Rules:

1. Capability adapters expose searchable fields.
2. Retrieval owns index use and scoring.
3. Orchestration owns rebuild/repair where needed.
4. Temporal search must be correct or explicitly degraded.

## What This Contract Excludes

This contract does not define:

1. Exact Rust module layout.
2. Exact trait names.
3. Storage-space byte assignments.
4. Durable byte formats.
5. Exact `EntityRef` syntax.
6. IPC wire format.
7. CLI command syntax.
8. Model provider behavior.
9. StrataHub sync or fleet semantics.

Those belong in follow-up contracts.

## Implementation Anti-Patterns

Engine-next data capability implementation should avoid these patterns:

1. A capability imports storage internals directly.
2. Branch code decodes every capability's row format itself.
3. Retrieval decodes capability value bytes instead of using adapters.
4. Graph fetches sibling capability records directly.
5. Vector owns autoembedding policy.
6. Search becomes a row owner for user-authored data.
7. Derived rows participate in branch promotion as if they were authored rows.
8. Control-plane rows are treated as ad hoc strings with no registry.
9. Value codecs accept malformed bytes and return default values.
10. Tests use private storage details to prove capability behavior.
11. Public API names expose internal transaction, table, WAL, segment, or
    subsystem concepts.
12. Optional model-dependent features silently run network/provider work.

## Minimum V1 Capability Contract Checklist

Before a data capability is V1-ready, it must answer these questions:

1. What are its entity references?
2. Which storage spaces does it use?
3. What is its key encoding?
4. What is its value encoding?
5. What read forms does it support?
6. What write forms does it support?
7. What does history mean?
8. What does timestamp read mean?
9. What branch operations does it support?
10. What conflicts can it produce?
11. What user-authored rows participate in compare/promote/copy/restore?
12. What derived rows exist, and how are they rebuilt or diagnosed?
13. How does it participate in search?
14. How does it participate in graph relationships?
15. How does it behave in cache mode?
16. How does it behave in read-only mode?
17. How does it behave after clone?
18. What import/export formats does it support, if any?
19. What stable errors does it produce?
20. Which shared conformance tests does it pass?

## Related Contracts

This contract is intentionally paired with the EntityRef, storage-space
registry, branch-operation, retrieval, clone-artifact, and command-boundary
contracts. Those documents pin the shared identity, persistence, derived-state,
and public command rules that every capability adapter must obey.
