# Engine-Next EntityRef And Relationship-Layer Contract

Status: V1 architecture draft

## Purpose

This document defines the identity and relationship contract for engine-next.

Storage-next persists branch-aware MVCC KV rows. Engine-next owns product
identity. `EntityRef` is the product identity layer that lets engine services,
users, search, graph traversal, diagnostics, clone, and future StrataHub
features refer to records without exposing storage keys or pretending that
JSON, event, vector, and graph are separate storage primitives.

The relationship layer is the graph capability's role as the connector across
Strata records. Graph remains a standalone data capability, but graph nodes can
also bind to KV, JSON, event, vector, graph, and supported control-plane
entities without copying source payloads into graph node properties.

## Related Documents

Read this with:

1. `docs/architecture/engine-next-architecture.md`
2. `docs/architecture/engine-next/README.md`
3. `docs/architecture/engine-next/primitive-implementation-contract.md`
4. `docs/product/strata-v1-graph-relationship-layer.md`
5. `docs/product/strata-v1-versioning-time-travel.md`
6. `docs/product/strata-v1-branching-direction.md`
7. `docs/architecture/storage-next/l9-storage-api-boundary.md`

Follow-up contracts that depend on this one:

1. Engine storage-space ID registry.
2. Engine persistence adapter contract.
3. Branch operation and capability adapter contract.
4. Temporal context and timeline resolver contract.
5. Control-plane layout contract.
6. Retrieval and derived-state contract.
7. Dataset clone artifact contract.

## Requirement Language

1. Must means V1 identity or relationship semantics are incomplete without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

Current code already contains most of the ingredients, but they are not yet a
clean contract:

1. `crates/core/src/contract/entity_ref.rs` defines variant-shaped
   `EntityRef` values for KV, event, branch, JSON, vector, and graph. Every
   data record variant carries a branch and space. `Branch` is branch-scoped
   but not space-scoped.
2. `crates/core/src/contract/primitive_type.rs` names these variants
   `PrimitiveType`. In engine-next language these are product data capability
   or control-plane identity kinds, not storage primitives.
3. Search re-exports `EntityRef` and returns it through search hits. Search
   already uses `EntityRef` as provenance, not as a storage key.
4. Graph node data contains `entity_ref: Option<String>`. Current values are
   opaque URI-like strings such as `kv://main/patient-4821`.
5. Graph maintains a reverse reference index with keys shaped like
   `__ref__/{encoded_uri}/{graph}/{node_id}`. This is the right mechanism in
   spirit, but the referenced value is still an arbitrary string.
6. Graph node keys, edge adjacency keys, ontology keys, and reference indexes
   are all ordinary graph storage rows. Storage does not understand graph
   identity or relationship semantics.
7. Retrieval already filters some BM25 hits by resolving the `EntityRef` back
   to capability-specific storage keys under a requested temporal context.

The target contract keeps the useful shape:

```text
product-facing identity -> capability-owned address -> persistence resolution
```

It removes the weak parts:

```text
opaque entity_ref strings
storage-shaped identity
branch-copy surprises
undocumented dangling-reference behavior
retrieval provenance without temporal context
```

## Definitions

### Entity

An entity is a product-addressable object in Strata.

Entities include:

1. KV records.
2. JSON documents.
3. JSON subpaths, when a path-level operation or result needs identity.
4. Event records.
5. Vector collections.
6. Vector records.
7. Graphs.
8. Graph nodes.
9. Graph edges.
10. Graph ontology object and link types.
11. Branches where diagnostics or product APIs need stable references.
12. Other control-plane objects only after a later contract gives them stable
    identity and access semantics.

Entities do not include:

1. Storage keys.
2. WAL records.
3. Table blocks.
4. Manifest entries.
5. Checkpoint sections.
6. Background job internals.

### EntityRef

An `EntityRef` is a stable engine/product reference to an entity.

An `EntityRef` is not:

1. A storage key.
2. A storage-space ID.
3. A row-family name.
4. A WAL payload type.
5. A bare graph-local node ID.
6. An opaque string note.

### Resolved Reference

A resolved `EntityRef` has an explicit branch identity. Engine APIs, search
results, graph traversal results, diagnostics, and provenance should return
resolved refs.

### Entity Binding

An entity binding is a stored relationship target. It may be branch-relative and
space-relative.

This distinction matters for branching and spaces. A graph node in branch
`feature` and space `app` that is bound to a JSON document in the same branch
and same space should usually remain bound to the corresponding JSON document
when the branch is forked or cloned. If the stored binding hard-codes the old
branch ID, forked relationship rows silently point back to the source branch. If
it omits space semantics, same-key records in different spaces can alias. Both
are wrong for normal branch-local, space-local relationships.

Stored relative bindings are resolved against the graph node that stores the
binding:

```text
graph branch + graph space + graph name + graph node id
```

They must never resolve against ambient process state, a current CLI selection,
or whichever branch happened to be active in a background job.

### Temporal Context

A temporal context defines how a ref is resolved:

1. Latest visible state.
2. At or before a commit version.
3. At or before a timestamp resolved by the commit timeline.
4. Historical result with a specific observed version.

The temporal context is usually external to `EntityRef`. A search hit or audit
event may carry `EntityRef + observed version/timestamp` as provenance.

## Binding Decisions

1. **EntityRef is product identity, not storage identity.**
   Storage must not need to parse or construct `EntityRef`.

2. **V1 keeps strongly typed, capability-shaped identity.**
   The public shape may remain enum-like for ergonomics and migration from the
   current code, but the target semantics are capability-shaped, not
   storage-primitive-shaped.

3. **Capability kind is not `TypeTag`.**
   A capability kind such as KV, JSON, event, vector, or graph describes product
   semantics. A storage-space ID describes row partitioning. A storage `TypeTag`
   is current implementation and durable-format vocabulary.

4. **Branch and space are part of data identity.**
   Data entities must carry branch and space context when resolved. Branch
   entities are branch-scoped but not space-scoped.

5. **Stored graph bindings are branch-relative and space-relative by default.**
   A graph node bound to an entity in the same branch and same space should
   store a local binding. API results resolve that binding through the graph
   node that stores it into a concrete `EntityRef` for the caller.

6. **Cross-branch bindings are not a V1 relationship target.**
   Normal relationship-layer behavior is branch-local. V1 APIs reject
   cross-branch relationship bindings with a structured unsupported or
   failed-precondition status. Provenance may still mention another branch as a
   fact about clone/import/history, but graph relationship targets stay within
   the graph node's branch.

7. **Cross-space bindings must be explicit.**
   Normal relationship-layer behavior is space-local. A binding from one space
   to another may be supported, but it must be visible in API output,
   diagnostics, clone artifacts, and access checks.

8. **Refs are unpinned by default.**
   A normal `EntityRef` means "this entity under the caller's temporal
   context." Pinned historical refs are provenance records, not the default
   relationship target.

9. **Graph node identity and entity identity are separate.**
   A graph node has graph-local identity. It may optionally bind to an entity
   elsewhere. Traversal can return both the graph node and the bound entity.

10. **Relationships are graph facts.**
   V1 relationships are represented by graph nodes and graph edges. Direct
   entity-to-entity relationship commands may exist as convenience APIs, but
   engine must materialize or resolve them through graph facts.

11. **Reverse maps are engine-owned derived indexes.**
    The authored fact is the graph node binding and graph edge. Reverse indexes
    such as "which graph nodes bind to this entity" are rebuildable engine rows,
    not storage concepts.

12. **Dangling references are explicit.**
    Missing, deleted, inaccessible, malformed, or history-trimmed targets must
    be reported as reference status. Traversal and retrieval must not silently
    drop them unless the API explicitly requests "live targets only."

13. **Relationship provenance is mandatory.**
    Search, graph traversal, graph-aware retrieval, vector hits, and RAG source
    material must be traceable back to an `EntityRef` plus enough version or
    relationship-path context to explain the result.

## Target Conceptual Shape

The target concept is a normalized identity with capability-specific address
payloads. Conceptually, an `EntityRef` carries:

1. Branch.
2. Optional space.
3. Entity kind.
4. Capability-specific address.
5. Optional subentity.

This is a conceptual contract, not a required Rust struct. Rust may keep an enum
if that is clearer and safer.

### Entity Kinds

V1 entity kinds:

| Kind | Category | Notes |
|---|---|---|
| `kv` | Data capability | KV record. |
| `json` | Data capability | JSON document, optionally subpath. |
| `event` | Data capability | Event record. |
| `vector` | Data capability | Vector collection or vector record. |
| `graph` | Data capability | Graph, node, edge, ontology object, or ontology link. |
| `branch` | Control plane | Branch identity and diagnostics. Not a data capability row. |

`branch` exists because users, diagnostics, and branch workflows need stable
branch references. It must not be used to justify "branch as a storage
primitive" in engine-next.

For V1, `branch` is the only control-plane entity kind this document allows as a
relationship target. Space records, recipe records, dataset records, projection
manifests, derived-state jobs, and fleet/StrataHub objects need their own
control-plane contract before they become valid relationship targets.

### Branch Scope

Rules:

1. Resolved API refs include an explicit branch.
2. Stored graph bindings default to the branch of the graph node that stores the
   binding.
3. Structured refs may name a branch explicitly for diagnostics, provenance,
   clone/import, and history output.
4. Graph relationship bindings reject cross-branch targets for V1.
5. Clone/import/export may remap explicit branch IDs through the dataset
   manifest if the artifact changes branch IDs.
6. A relative stored binding resolves only through the graph node that stores
   the binding. It does not mean the process-global current branch, CLI current
   branch, or latest branch selected by a caller.

### Space Scope

Rules:

1. KV, JSON, event, vector, and graph data refs must include a space when
   resolved.
2. Branch refs do not include a space.
3. System-space refs must be visibly system-scoped.
4. User-space refs must not accidentally resolve into `_system_` or other
   reserved spaces.
5. Stored graph bindings default to the space of the graph node that stores the
   binding.
6. Cross-space refs require an explicit space.
7. A relative stored binding resolves only through the graph node that stores
   the binding.

### Capability Addresses

Target address examples:

| Kind | Address |
|---|---|
| `kv` | key |
| `json` | document key; optional JSON pointer/path as subentity |
| `event` | sequence number in branch and space; stream/type context is metadata unless a later event contract promotes it |
| `vector` | collection; optional vector key |
| `graph` | graph name plus graph address kind |
| `branch` | branch id or branch name/ref, depending on API |

Vector address rule:

1. User-managed vector records are EntityRef-addressable and may be
   relationship targets.
2. Shadow vectors are EntityRef-addressable for diagnostics, provenance, and
   rebuild health, but they are not valid V1 relationship targets. Relationships
   should point at the source entity that produced the shadow vector.

Graph address kinds:

1. Graph metadata.
2. Node.
3. Edge.
4. Ontology object type.
5. Ontology link type.
6. Relationship binding.

V1 interim identity rules:

1. JSON document identity is the document key. JSON pointer/path is a subentity
   used for path-level operations, provenance, and diagnostics; it is not a
   separate storage row identity by default.
2. Event identity is branch, space, and sequence number. Event type, stream
   name, or user event id may be query metadata, but it is not the canonical V1
   event identity unless a later event contract changes it.
3. Ordinary graph edge identity is the deterministic compound identity
   `graph + source node + destination node + edge type`. Public support for
   multiple same-type parallel edges requires an explicit edge id before V1.
   Existing packed adjacency internals that can contain duplicates are current
   implementation evidence, not the target public identity contract.

## EntityRef And URI Encoding

V1 may expose a URI/string form for CLI output, import/export, logs, and
StrataHub artifacts. The URI form must be derived from the typed model, not the
authoritative model.

Rules:

1. The typed `EntityRef` or equivalent structured DTO is authoritative.
2. URI parsing must reject invalid or ambiguous refs.
3. URI display must round-trip through the typed model.
4. URI strings must not be stored as the only form in new engine-next graph
   relationship rows.
5. Current graph `entity_ref: Option<String>` is a migration input, not the V1
   target.

Illustrative URI examples:

```text
strata://branch/main/space/app/kv/user:42
strata://branch/main/space/app/json/orders/ord_123
strata://branch/main/space/app/json/orders/ord_123#/items/0
strata://branch/main/space/app/event/order-created/00000042
strata://branch/main/space/app/vector/doc-embeddings/doc_123
strata://branch/main/space/app/graph/knowledge/node/person:ada
```

These are examples only. The actual URI grammar belongs in the public API and
CLI surface contract or dataset clone artifact contract.

## Relationship Layer Model

### Graph Node Identity

A graph node is identified by:

```text
branch + space + graph name + node id
```

A graph node can be:

1. Native.
   It has no entity binding. The node itself is the primary graph data.

2. Bound.
   It has an entity binding to a KV, JSON, event, vector, graph, or supported
   control-plane entity.

3. Mixed.
   It has an entity binding plus graph-local properties that annotate the
   relationship context.

Graph-local properties must not become a forced copy of the source payload.

### Graph Edge Identity

An edge is a graph fact between graph nodes.

Edge identity must account for:

1. Source node.
2. Destination node.
3. Link type.
4. Direction.
5. Deterministic compound identity for V1 ordinary edges.

V1 ordinary edge identity is:

```text
branch + space + graph + source node + destination node + edge type
```

Multiple same-type parallel edges between the same endpoints are not a V1
identity guarantee unless engine-next introduces an explicit edge id. Any API
that admits duplicate packed adjacency entries must either collapse them into
the ordinary edge identity or allocate explicit edge ids before those edges
become user-addressable.

Edge payload may include:

1. Weight.
2. Properties.
3. Evidence.
4. Provenance.
5. Authored vs derived marker.

### Relationship Binding

A bound graph node stores an entity binding.

Binding fields:

1. The graph branch, graph space, graph name, and graph node id that contain
   the binding.
2. Branch reference: relative to the graph node by default, explicit branch only
   when requested.
3. Space reference: relative to the graph node by default, explicit space only
   when requested.
4. Entity kind.
5. Capability address.
6. Optional subentity.
7. Optional binding policy.

Binding policies:

1. Strict live target.
   Target must exist when the binding is created.

2. Allow dangling.
   Used for import, external references, incomplete datasets, or graph-first
   modeling. Must surface as dangling until resolved.

3. Pinned provenance.
   Target is intentionally tied to a version or timestamp. Used for audit,
   search evidence, and generated explanations, not normal mutable
   relationships.

Default V1 behavior should be strict live target for interactive user-created
relationships and allow dangling for import/export only when explicitly
requested.

### Direct Entity Relationship Commands

Direct entity-to-entity relationship commands are allowed only as API sugar.
They must produce graph facts.

If V1 exposes an operation such as "connect entity A to entity B", engine-next
must define all of these before implementation:

1. Target graph selection.
2. Graph node materialization policy for each endpoint.
3. Node identity policy.
4. Idempotency rule for repeated commands.
5. Whether one canonical bound node exists per entity per graph, or whether
   multiple graph nodes may bind to the same entity for different contexts.
6. How graph-local properties are merged or rejected on repeated materialized
   node creation.

The default V1-safe rule is one canonical bound node per
`graph branch + graph space + graph name + normalized entity binding`, with
explicit override required for multiple contextual nodes bound to the same
entity.

## Resolution Semantics

Resolving an entity requires:

1. An `EntityRef` or stored binding.
2. Temporal context.
3. Access context.
4. The capability-specific resolver for the target kind.

Resolution result:

1. Resolved `EntityRef`.
2. Resolution status.
3. Optional observed commit version.
4. Optional observed timestamp.
5. Optional bounded value summary.
6. Optional structured diagnostics.

Resolution statuses:

| Status | Meaning |
|---|---|
| `Present` | Target exists and is visible in the requested context. |
| `Deleted` | Target has a visible tombstone or deleted state. |
| `Missing` | No target exists in the requested context. |
| `Dangling` | Relationship explicitly permits a missing target. |
| `HistoryTrimmed` | Target may have existed, but retained history cannot answer. |
| `Inaccessible` | Access mode, branch visibility, or policy prevents resolution. |
| `MalformedRef` | Ref cannot be parsed or violates the capability contract. |
| `MalformedTarget` | Target row exists but capability value decoding failed. |
| `Unsupported` | Capability does not support the requested resolution mode. |
| `DerivedStateStale` | Authored target may exist, but derived index state is stale. |

Rules:

1. Latest resolution uses latest visible committed state.
2. Version resolution uses value visible at or before the requested commit
   version.
3. Timestamp resolution first uses the timeline resolver, then version
   resolution.
4. Capability adapters own value decoding and target interpretation.
5. Persistence resolves rows. It does not interpret product identity.
6. Storage never resolves `EntityRef`.

## Temporal Behavior

Relationships are temporal because both graph facts and target entities are
versioned.

V1 must distinguish:

1. Relationship existence time.
   Was the graph node or edge visible at the requested time?

2. Target existence time.
   Was the bound target visible at the requested time?

3. Binding interpretation time.
   Was the entity binding itself valid and parseable at the requested time?

Traversal under temporal context should:

1. Read graph nodes and edges under the requested temporal context.
2. Resolve bound entity refs under the same temporal context unless the binding
   is explicitly pinned.
3. Return target status for each bound node.
4. Avoid mixing latest targets into historical traversal without an explicit
   "latest target" option.

Examples:

1. A relationship exists now but did not exist at version 10.
   Historical traversal at version 10 must not show it.

2. A relationship existed at version 10 but the target document was deleted at
   version 8.
   Traversal at version 10 may return the graph node with target status
   `Deleted`.

3. A target exists now but history was trimmed before the requested timestamp.
   Traversal must return `HistoryTrimmed` rather than fabricating a value.

## Dangling And Deleted References

Dangling references are not automatically corruption.

They can arise from:

1. Importing a graph before importing source records.
2. Deleting a target while keeping graph context.
3. Branch operations that intentionally preserve graph facts.
4. External references.
5. History retention.
6. Partial clone or filtered export.

Rules:

1. Interactive relationship creation should validate targets by default.
2. Import/export may preserve dangling refs when explicitly configured.
3. Traversal APIs must expose target status.
4. Search/RAG APIs may filter dangling refs by default, but diagnostics must
   count and report filtered refs.
5. Delete policies must be explicit: detach, cascade graph node, or keep
   dangling.
6. The default target delete policy should preserve graph facts and mark target
   status as deleted/dangling unless a capability-specific contract chooses a
   stricter behavior.

Policy matrix:

| Binding policy | Target state | Traversal behavior | Search/RAG behavior |
|---|---|---|---|
| Strict live target | Present | Return node and resolved target. | Candidate may participate normally. |
| Strict live target | Missing or deleted | Return node with `Missing` or `Deleted`; diagnostics should flag broken strict binding. | Filter by default and count/report filtered strict bindings. |
| Allow dangling | Missing | Return node with `Dangling`. | Filter by default unless recipe asks for unresolved graph context. |
| Allow dangling | Present | Return node and resolved target. | Candidate may participate normally. |
| Pinned provenance | Present at pin | Return pinned target observation. | Candidate may participate with observed version/timestamp. |
| Pinned provenance | History trimmed | Return `HistoryTrimmed`. | Filter or degrade explicitly; never replace with latest target silently. |

V1 delete-policy vocabulary:

1. Cascade deletes the bound graph node and incident edges.
2. Detach preserves the graph node and removes the entity binding.
3. Keep dangling preserves the binding and reports `Deleted`, `Missing`, or
   `Dangling` during resolution.

Reject-delete is not a V1 policy. It may return after reverse maps become
transactional or after target deletion can perform an authoritative bounded
scan. Until then, reject would often depend on derived-state completeness and
would degrade to fail-closed too frequently for a clean product guarantee.

Current `CascadePolicy::Ignore` maps to keep-dangling behavior in this target
contract. It should not remain an undocumented "do nothing" mode.

Current graph has an integrity helper that can detach or cascade nodes by
opaque entity URI. Engine-next should convert that into typed policy over
structured bindings.

## Reverse Maps And Indexes

The relationship layer needs reverse lookup:

```text
entity -> graph nodes bound to entity
entity -> relationships touching entity
entity -> derived search/retrieval expansions
```

Rules:

1. Reverse maps are engine-owned rows.
2. Reverse maps are derived from graph node bindings and edges.
3. Reverse maps should be rebuildable unless a later contract explicitly makes
   them authoritative.
4. Stale reverse maps must be diagnosable.
5. Branch copy/delete/promote must update, rebuild, or invalidate reverse maps.
6. Clone/import/export must either include reverse maps with validation
   metadata or mark them for rebuild.
7. Storage-space ID assignments for reverse maps belong in the storage-space
   ID registry.

Trust rules:

1. The graph node binding is authoritative.
2. Reverse maps are candidate indexes unless a later contract explicitly makes
   them transactional secondary indexes.
3. APIs that use reverse maps for correctness-sensitive behavior, such as target
   deletion hooks, must verify candidate rows against the current authoritative
   graph node binding before mutating data.
4. Stale positive reverse-map entries must be ignored after verification and
   scheduled for repair.
5. Stale missing reverse-map entries must be detectable by validation/rebuild
   jobs; normal point reads are not required to scan every graph to prove
   absence.
6. A reverse-map health record or watermark must report whether a graph's
   reverse maps are current, rebuilding, stale, or corrupt.
7. Absence from a reverse map is proof of no bindings only when reverse-map
   health is current for the relevant graph scope. Otherwise absence is unknown.

Current `__ref__/{encoded_uri}/{graph}/{node_id}` keys are evidence for the
mechanism. The target must replace opaque URI keys with canonical typed binding
keys or a documented URI derived from typed bindings.

## Search, Retrieval, And RAG Provenance

Every retrieval result should have traceable provenance.

Search hit provenance:

```text
EntityRef
observed version or requested temporal context
source capability
source fields or projection id where available
index/watermark facts where relevant
```

Graph-aware retrieval provenance:

```text
anchor EntityRef
relationship path
graph node ids traversed
edge types traversed
target EntityRef
target resolution status
temporal context
```

RAG source provenance:

```text
EntityRef
subentity or snippet span where available
retrieval recipe stage
score/fusion facts
relationship expansion facts where used
observed version/timestamp
```

Rules:

1. Retrieval must not return untraceable payload fragments.
2. Graph expansion must not hide dangling or stale relationship state.
3. Derived search indexes must map back to `EntityRef`.
4. EntityRef identity must include space to prevent cross-space leakage.
5. Historical retrieval must either prove index compatibility or degrade
   explicitly.

## Branch, Merge, And Clone Behavior

### Fork

When a branch is forked:

1. Graph relationship rows follow normal branch mechanics.
2. Owning-branch entity bindings resolve against the forked branch.
3. V1 authored rows must not contain active explicit cross-branch relationship
   bindings. If such rows are encountered during validation, import, or
   recovery, they are rejected or surfaced as unsupported legacy/corrupt state
   before fork state is accepted.
4. Reverse maps are rebuilt, copied, or invalidated according to derived-state
   policy.

### Merge And Promote

Branch workflows coordinate. Capability adapters interpret.

Rules:

1. Graph owns graph node, edge, ontology, and relationship-binding conflicts.
2. Source capabilities own source record conflicts.
3. A relationship conflict and a source record conflict are separate facts.
4. Strict mode should refuse ambiguous relationship changes.
5. SourceWins may overwrite graph facts only when it reports what changed.
6. Derived reverse maps should rebuild after merge rather than merge as
   authored state.

### Branch-From-History

Branch-from-version and branch-from-time must preserve historical graph facts
and historical entity resolution semantics.

Rules:

1. Graph rows visible at the source version/time seed the new branch.
2. Owning-branch and owning-space bindings resolve inside the new branch and
   space.
3. If a target was not visible at the source version/time, the new branch gets
   the relationship fact plus target status according to the selected policy.
4. Pinned provenance remains pinned.

### Clone, Import, And Export

Dataset artifacts must preserve relationship intent.

Rules:

1. Structured refs must be included in artifact metadata or graph rows.
2. If branch IDs are remapped, same-dataset refs must be remapped.
3. External refs must be marked external.
4. Missing target records must be reported as dangling during validation.
5. Derived reverse maps may be excluded if the artifact marks them for rebuild.
6. URI forms must be versioned if they are included.

## Capability Responsibilities

Every data capability must provide an entity adapter.

Entity adapter responsibilities:

1. Validate capability-specific addresses.
2. Convert public inputs to structured entity refs.
3. Resolve latest/version/timestamp state through persistence.
4. Produce bounded summaries for diagnostics and traversal.
5. Identify malformed target rows.
6. Report retained-history limitations.
7. Provide branch adapter hooks for refs if branch operations need them.
8. Provide retrieval provenance hooks where searchable.

Minimum adapters:

| Capability | Required entity support |
|---|---|
| KV | key refs, latest/version/timestamp/history resolution. |
| JSON | document refs, optional path refs, path-not-found diagnostics. |
| Event | sequence/event refs, append-order and event-time implications. |
| Vector | collection refs, vector refs, source-ref provenance where present. |
| Graph | graph/node/edge/ontology refs and binding resolution. |
| Branch | branch refs for diagnostics and branch workflows. |

## Errors And Diagnostics

Entity and relationship errors must map to the V1 error model.

Required error classes or codes should cover:

1. Invalid entity ref syntax.
2. Unsupported entity kind.
3. Invalid capability address.
4. Missing branch.
5. Missing space.
6. Missing target.
7. Deleted target.
8. Dangling relationship target.
9. History trimmed.
10. Temporal resolution unsupported.
11. Malformed target value.
12. Stale reverse map.
13. Relationship conflict.
14. Cross-branch reference not allowed.
15. Cross-space reference not allowed where policy forbids it.
16. System-space access denied.

These names are semantic categories. Before implementation, the V1 error and
diagnostics contract must either allocate concrete stable error codes for them
or map each category to an existing code with a structured context field. The
relationship layer must not invent one-off string errors.

Diagnostics should be able to answer:

1. Which graph nodes bind to this entity?
2. Which relationships are dangling?
3. Which reverse maps are stale?
4. Which search results were filtered because their target was not visible?
5. Which branch operation changed relationship facts?
6. Which clone/import refs could not be resolved?

## Conformance Tests

Shared EntityRef tests:

1. Entity refs include branch and space where required.
2. Branch refs are not space-scoped.
3. URI/string form round-trips through structured refs where supported.
4. Invalid refs fail with typed errors.
5. Same keys in different spaces do not alias.
6. Same entity address in different branches does not alias when resolved.
7. Branch-relative bindings resolve through the graph node's branch, not ambient
   current branch.
8. Space-relative bindings resolve through the graph node's space, not ambient
   current space.
9. Cross-branch relationship bindings are rejected for V1.
10. Explicit space bindings remain explicit after fork/import.
11. Version and timestamp resolution use the same temporal context rules as
   capability reads.
12. History-trimmed targets surface as history-trimmed, not missing.

Relationship-layer tests:

1. Native graph node has no entity binding.
2. Bound graph node resolves to target entity.
3. Bound graph node does not copy source payload.
4. Reverse map finds graph nodes bound to an entity.
5. Reverse map updates when binding changes.
6. Reverse map clears when binding is removed.
7. Reverse-map candidates are verified against authoritative node bindings
   before correctness-sensitive deletes.
8. Stale reverse-map positives are ignored and diagnosed.
9. Target delete surfaces deleted or dangling status according to policy.
10. Cascade, detach, and keep-dangling delete policies are distinct.
11. Traversal returns graph node id and target `EntityRef`.
12. Temporal traversal does not mix latest target state into historical graph
   state.
13. Branch fork retargets branch-relative bindings to the forked branch.
14. Branch fork rejects or diagnoses invalid explicit cross-branch relationship
    bindings rather than preserving them as active graph facts.
15. Branch merge reports relationship conflicts separately from source record
   conflicts.
16. Direct entity relationship sugar is idempotent under the selected
   materialization policy.
17. Clone/import validates unresolved refs and marks rebuildable reverse maps.
18. Search/RAG provenance always includes `EntityRef`.
19. Graph-aware retrieval reports relationship path provenance.

Fault tests:

1. Corrupt binding bytes.
2. Corrupt reverse map row.
3. Missing target branch.
4. Missing target space.
5. Target exists but malformed capability value.
6. Reverse map stale after interrupted rebuild.
7. Clone artifact missing referenced target.
8. Clone/import or legacy row containing unsupported cross-branch relationship
   refs.

## Deferred Or Open Questions

1. Exact Rust type names.
   This contract defines semantics. It does not require replacing the current
   enum immediately.

2. Exact URI grammar.
   Needed for CLI, clone artifacts, logs, and StrataHub, but the structured
   model comes first.

3. Cross-branch relationships.
   Closed for V1: relationship targets are branch-local. Explicit
   cross-branch relationship targets are post-V1.

4. Direct entity-to-entity relationship commands.
   Useful as syntax sugar, but graph facts remain the underlying relationship
   model.

5. JSON path refs.
   V1 pins JSON document identity and allows JSON pointer/path as a subentity.
   A later contract may decide which APIs permit path-level relationship targets
   rather than document-only targets.

6. Event identity.
   V1 pins branch, space, and sequence as canonical event identity. A later
   event contract may promote stream/type/user event id into canonical identity.

7. Edge identity.
   V1 pins deterministic compound identity for ordinary edges. Explicit edge IDs
   are required before public same-type parallel edges become user-addressable.

8. Additional control-plane entity refs.
   Branch refs are V1. Space, recipe, dataset, projection, derived-state, and
   fleet refs are deferred until control-plane and StrataHub contracts define
   stable identity and access behavior.

## V1 Minimum

For V1, the minimum acceptable implementation is:

1. Structured, typed entity refs for KV records, JSON documents, optional JSON
   path subentities where APIs support them, events by sequence, vector
   collections and records, graph nodes, compound-identity graph edges, and
   branches.
2. Branch and space included in all resolved data refs.
3. Branch-relative and space-relative graph bindings by default.
4. Graph nodes can bind to structured entity refs.
5. Graph traversal can return both graph node identity and bound entity refs.
6. Dangling/deleted/history-trimmed target status is explicit.
7. Reverse maps are rebuildable, diagnosable, and verified before
   correctness-sensitive use.
8. Search and graph-aware retrieval results include entity provenance.
9. Storage never parses entity refs.
10. Clone/import/export preserves or reports relationship refs.

## Next Step

The engine storage-space ID registry is defined in
`docs/architecture/engine-next/storage-space-id-registry.md`.

The engine persistence adapter contract is defined in
`docs/architecture/engine-next/persistence-adapter-contract.md`.

The branch operation and capability adapter contract is defined in
`docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`.

The temporal context and timeline resolver contract is defined in
`docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`.

The next contract should be the control-plane layout contract.
