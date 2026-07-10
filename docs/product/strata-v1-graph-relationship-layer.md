# Strata V1 Graph Relationship Layer

Status: Draft product direction

This document defines the new product direction for Strata's graph capability:
graph remains a first-class standalone data capability, but it also becomes the
relationship layer across all Strata data.

The goal is to avoid turning Strata into a worse graph database where users must
duplicate key-value records, JSON documents, events, vectors, and search results
inside graph payloads before they can connect them. The stronger product is a
database where ordinary Strata data can be related, traversed, searched, and
analyzed through graph semantics without losing its original shape.

## Thesis

Graph has two jobs in V1:

1. Standalone graph capability.
   Users can create named graphs, add graph-native nodes and edges, define
   ontology metadata, traverse neighborhoods, and run local graph analytics.

2. Relationship layer.
   Users can connect records from key-value, JSON, events, vectors, and graph
   itself through graph relationships without copying the source records into
   graph node payloads.

This is the same product stance Strata should take with vectors. Vectors remain
a first-class capability for users who bring their own embeddings or maintain
their own vector collections. Separately, Strata may maintain branch-local
shadow embeddings for auto-embedding and retrieval. Graph should follow that
same pattern: direct graph remains available, while relationship graph support
lets Strata connect records that live in other data capabilities.

## Non-Negotiables

1. Graph must not cease to be a standalone capability.
   Users should be able to use Strata as an embedded graph store without also
   using key-value, JSON, events, vectors, search, or auto-derived relationships.

2. Relationship modeling must not require payload duplication.
   A user should not need to copy a JSON document, event, or KV value into a
   graph node property just to connect it to other records.

3. User-authored graph data and Strata-derived relationship data are different.
   Strata may later infer relationships from documents, events, search hits, or
   model output, but that must be explicit and observable. It must not overwrite
   or blur user-authored graph facts.

4. Entity identity must become a real contract.
   The current `entity_ref` string is evidence of the right direction, but V1
   should not leave cross-capability identity as arbitrary strings with
   undocumented meaning.

5. Branch, space, and version semantics must apply to relationships.
   A relationship is not global by accident. It belongs to a database context,
   branch, space, graph, and version model.

6. Storage remains primitive-agnostic.
   Storage should persist key-value rows and durability state. It should not know
   graph, vector, JSON, event, ontology, or search semantics.

## Product Model

### Standalone Graph Mode

Standalone graph mode is the ordinary graph-store experience:

1. Create a named graph in a branch and space.
2. Add graph-native nodes with graph-local properties.
3. Add typed, weighted, property-bearing edges.
4. Define ontology object types and link types.
5. Traverse, query neighborhoods, and run supported local analytics.

In this mode, graph nodes do not have to refer to records elsewhere in Strata.
They can be the primary data.

### Relationship Layer Mode

Relationship layer mode connects records that already exist elsewhere:

1. A JSON document can be represented by a graph node that points to that
   document.
2. An event can be linked to the document, user, task, model run, or vector
   record it concerns.
3. A KV record can participate in graph traversal without being copied into
   graph properties.
4. A vector record can be connected to the source record it embeds, the cluster
   it belongs to, or a semantic neighborhood.
5. A search hit can be expanded through graph neighbors to retrieve related
   records.

The graph node is the relationship handle. The original data remains owned by
its original capability.

### Derived Relationship Mode

Derived relationship mode is a future layer where Strata can create or maintain
relationships from higher-level workflows:

1. Auto-link documents that mention the same entity.
2. Build relationship edges from events.
3. Store entity extraction output as candidate graph structure.
4. Generate graph context for RAG recipes.
5. Maintain graph-backed retrieval boosts.

This is directionally important, but it should not be a hidden V1 behavior.
Derived relationships must be opt-in, inspectable, repairable, and separable
from user-authored graph facts.

## Entity References

The relationship layer needs a typed `EntityRef` contract. The current graph
implementation already has the idea through node-level `entity_ref`, but product
and architecture should tighten it before V1.

An entity reference identifies a Strata record or sub-record outside the graph
node itself. It should include enough information to resolve the target under
the selected database context:

1. Data capability kind, such as `kv`, `json`, `event`, `vector`, or `graph`.
2. Branch context, or an explicit rule that the current branch is used.
3. Space context.
4. Capability-specific identity, such as key, document id, event sequence,
   vector collection and id, or graph and node id.
5. Optional version or time-travel pin.
6. Optional subpath, such as a JSON pointer into a document.

The exact encoding should be designed in the architecture phase. The important
V1 product rule is that the reference is typed and validated enough to avoid
being just an opaque note.

Storage should not need to decode this reference. Engine-next owns the
`EntityRef` type, encodes references into storage rows or values where needed,
and maintains any reverse lookup indexes as engine-owned rows. This lets graph
traversal return typed entity references without making storage understand KV,
JSON, events, vectors, graph nodes, or search records.

Possible examples:

```text
strata://branch/main/space/app/kv/user:42
strata://branch/main/space/app/json/orders/ord_123
strata://branch/main/space/app/json/orders/ord_123#/items/0
strata://branch/main/space/app/event/order-created/00000042
strata://branch/main/space/app/vector/doc-embeddings/doc_123
strata://branch/main/space/app/graph/knowledge/node/person:ada
```

These examples are illustrative, not a final URI specification.

## Graph Nodes

V1 should distinguish graph node identity from entity identity.

1. `GraphNodeId`
   Identifies a node inside a named graph.

2. `EntityRef`
   Identifies the Strata record that a graph node is about, when the node is
   bound to another data capability.

3. Graph-local properties.
   Store relationship-layer annotations, labels, weights, evidence, display
   metadata, and graph-specific facts. They should not become a forced copy of
   the source record.

A node can be:

1. Native.
   It has no entity reference. The node itself is the graph data.

2. Bound.
   It has an entity reference to a KV, JSON, event, vector, or graph target.

3. Mixed.
   It has an entity reference and graph-local properties that add relationship
   context without replacing the source record.

The current implementation already points in this direction through node
`entity_ref`, `object_type`, properties, and a reverse reference index. V1 should
promote this from an incidental field to a documented relationship-layer
contract.

## Edges And Relationships

Edges are graph facts. They should support:

1. Source and destination graph nodes.
2. Link type.
3. Direction.
4. Weight.
5. Properties.
6. Optional evidence or provenance in graph-local metadata.

The simplest V1-compatible model is:

1. Edges connect graph node ids.
2. Graph nodes may be bound to entity references.
3. Traversal can return both graph node ids and entity references for bound
   nodes.

A future model may allow edges to be declared directly between entity
references, with Strata materializing graph nodes internally. That is valuable,
but it should be designed carefully. Direct entity-ref endpoints should not be
introduced until identity, deletion, branch, and import/export semantics are
clear.

## Relationship Semantics

Relationships should answer questions like:

1. Which records are connected to this record?
2. Which events caused, referenced, superseded, or contradicted this document?
3. Which vector records represent this source document?
4. Which entities should retrieval expand around this search hit?
5. Which branch-local graph facts changed between versions?
6. Which records are central, isolated, duplicated, or part of the same
   component?

The relationship layer is not just a visualization tool. It is a query and
retrieval primitive that should work with search, RAG, analytics, branch diff,
and dataset exploration.

## Ontology

Ontology becomes more important in the relationship-layer model.

For standalone graph usage, ontology describes graph-native object and link
types. For relationship-layer usage, ontology describes how Strata entities are
being modeled:

1. A JSON order document may be represented by an `Order` object type.
2. A KV user profile may be represented by a `User` object type.
3. An event may be represented by an `Event` or domain-specific event type.
4. A vector record may be represented as an `Embedding`, `Chunk`, or
   `SemanticView`.
5. Edges may express domain relationships such as `created_by`, `mentions`,
   `derived_from`, `embeds`, `supersedes`, or `supports`.

V1 must decide how much ontology is documentation and how much is validation.
The minimum acceptable V1 stance is:

1. Object and link type metadata has stable meaning.
2. Nodes can be listed by type.
3. Link types are visible to traversal, search recipes, and analytics.
4. Frozen or stabilized ontology state has clear user-facing semantics if the
   feature remains public.

Full schema validation can be optional if it would slow down V1, but the
product should not pretend ontology is merely decorative if commands and search
features depend on it.

## Branch, Space, And Version Behavior

Relationship data must follow Strata's core product model.

1. Branch.
   Graphs and relationships are branch-local unless explicitly designed
   otherwise. Forking a branch should fork relationship state with the rest of
   the branch.

2. Space.
   Entity references must include or inherit space context. Relationships across
   spaces should be possible only if the product explicitly supports them.

3. Version.
   Traversal, graph reads, and relationship expansion should respect current
   version or time-travel context where supported.

4. Live versus pinned references.
   Some relationships should point to the latest value of a key or document.
   Others should point to a specific version. V1 must define whether pinned
   references are supported now, deferred, or represented as future metadata.

5. Import, export, and clone.
   Portable datasets must preserve relationship identity. Clone should not break
   graph references by accidentally baking in machine-local paths or unstable
   runtime ids.

## Deletion And Broken References

The current graph implementation already has evidence of a deletion policy
through cascade, detach, and ignore behavior. V1 should make this explicit.

When a referenced entity is deleted, Strata should have a clear policy:

1. Cascade.
   Remove graph nodes and edges bound to the deleted entity.

2. Detach.
   Keep the graph node and remove or mark the entity reference.

3. Ignore.
   Preserve the relationship fact even if the referenced entity is currently
   missing.

Different workloads may need different policies. The product should avoid
silent behavior. At minimum, graph metadata should expose the configured policy,
and APIs should document how traversal and search treat missing references.

## Search And RAG Integration

The relationship layer should make search better without forcing users into a
single retrieval strategy.

Search and RAG can use graph relationships to:

1. Expand from a search hit to neighboring records.
2. Boost records close to known entities.
3. Ground generated answers in related evidence.
4. Explain why a result was included.
5. Traverse from vectors back to source records.
6. Combine keyword, semantic, graph, and ontology signals through recipes.

This should remain recipe-driven and explicit. Graph-aware retrieval should
degrade honestly when graph data, embeddings, models, or recipes are missing.

## Analytics

Graph analytics should work over both native nodes and bound nodes.

When a node has an entity reference, analytics results should be able to report
both:

1. The graph node id used by the algorithm.
2. The entity reference that lets the caller fetch the underlying record.

This makes analytics useful for ordinary Strata data. PageRank, connected
components, shortest paths, clustering, and community detection should help
users understand the relationship structure of their data, not just isolated
graph payloads.

## Existing Implementation Evidence

The current codebase already contains pieces of this direction:

1. Graph nodes can carry `entity_ref`.
2. Graph nodes can carry graph-local properties and `object_type`.
3. Graph edges carry weight and properties.
4. Graph metadata includes cascade policy and ontology status.
5. A reverse entity-reference index exists for graph bindings.
6. Referential integrity hooks can react to entity deletion.
7. Graph-aware search boosting can use entity references and graph distance.
8. Executor and CLI surfaces already expose graph CRUD, ontology, traversal, and
   analytics commands.

These are useful foundations, but they are not yet a complete product contract.
The next architecture pass should preserve the good ideas and replace loose
string conventions with explicit types and documented behavior.

## V1 Scope

### Required

V1 should require:

1. Standalone graph capability remains supported.
2. Graph nodes can reference records in other Strata data capabilities without
   duplicating source payloads.
3. Entity-reference semantics are documented and typed at the product or engine
   API boundary.
4. Traversal can expose entity references for bound nodes.
5. Search and retrieval docs recognize graph-aware expansion as a first-class
   integrated capability.
6. Deletion behavior for referenced entities is explicit.
7. Branch and space behavior for graph relationships is documented.
8. Storage remains unaware of graph and relationship semantics.

### Optional For V1

V1 may include:

1. Direct edge creation between entity references.
2. Automatic node materialization for entity references.
3. Full ontology validation.
4. Relationship repair or orphan-reference inspection commands.
5. Graph-derived search boosts in default recipes.
6. Derived relationship generation from events, documents, or model output.

These features are valuable, but they should not block V1 unless the product
requirements are tightened later.

### Not V1

V1 should not promise:

1. Distributed graph analytics.
2. Hidden model-generated relationship extraction.
3. Automatic upload, sync, or fleet-level relationship indexing.
4. A universal RDF or property-graph compatibility layer.
5. Cross-database relationship consistency.

Those may become future directions, but they should not distort the embedded V1
database architecture.

## Architecture Implications

1. `EntityRef` should live at the product or engine contract layer, not in
   storage.
2. Graph should remain engine-owned behavior over storage rows.
3. Search and retrieval should consume entity references as source identity.
4. Vector auto-embedding and graph relationship support should share the same
   principle: generated shadow data is optional, explicit, branch-local, and
   inspectable.
5. Clone, import, export, and dataset bundles must preserve relationship identity
   without machine-local assumptions.
6. Testing must include broken references, deletion policies, branch forks,
   time-travel reads, clone/import/export, and graph-aware retrieval.

## Open Questions

1. What is the final `EntityRef` encoding?
2. Should entity references default to the current branch and space, or always
   include them explicitly?
3. Are version-pinned entity references required for V1?
4. Should graph edges only connect graph node ids in V1, or should entity-ref
   endpoints be public?
5. What deletion policy should be the default: cascade, detach, or ignore?
6. How should missing references appear in traversal, analytics, and search?
7. How deep should ontology validation go in V1?
8. How should JSON subpaths, event identities, and vector collection identities
   be represented?
9. Should relationship repair be a user command, a health diagnostic, or both?
10. How should derived relationships be labeled so users can distinguish them
    from authored relationships?

## Acceptance Criteria

The relationship-layer direction is working when all of these are true:

1. A user can attach a graph node to a KV record, JSON document, event, vector
   record, or another graph node without copying the source payload.
2. A user can still create a graph made only of graph-native nodes and edges.
3. Traversal can return enough information to fetch connected Strata records.
4. Search can use graph context where configured without requiring graph data for
   ordinary search.
5. Deleting or missing referenced records has documented behavior.
6. Branch, space, and version context are not ambiguous.
7. Clone and export do not break relationship identity.
8. Storage remains a primitive-agnostic persistence layer.
