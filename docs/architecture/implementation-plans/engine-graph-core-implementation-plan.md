# Engine Graph Core Implementation Plan

## Problem

Graph is the final product primitive to bring into the rebuilt engine. The old
engine already has a broad graph module, but that module includes several
concerns that should not all be restored in the first slice: core graph facts,
ontology, analytics, search boosting, export helpers, branch DAG projection,
and semantic merge.

This plan restores only the reasonable core graph surface: named graphs,
nodes, edges, neighbor lookup, batch graph writes, branch/space isolation,
durable persistence, and entity-binding lookup. Ontology is intentionally a
separate follow-up. Executor commands are also a follow-up after the engine API
is stable.

## Old Evidence

- `crates/engine/src/graph/mod.rs`
- `crates/engine/src/graph/types.rs`
- `crates/engine/src/graph/keys.rs`
- `crates/engine/src/graph/lifecycle.rs`
- `crates/engine/src/graph/nodes.rs`
- `crates/engine/src/graph/edges.rs`
- `crates/engine/src/graph/traversal.rs`
- `crates/engine/src/graph/bulk.rs`
- `crates/engine/src/graph/ext.rs`
- `crates/executor/src/command.rs`
- `crates/executor/src/handlers/graph.rs`
- `crates/executor/src/handlers/graph_impl.rs`
- `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`

## Current Targets

- `crates/engine-next/src/api/graph.rs`
- `crates/engine-next/src/api/mod.rs`
- `crates/engine-next/src/data/graph/`
- `crates/engine-next/src/data/mod.rs`
- `crates/engine-next/src/persistence/key.rs`
- `crates/engine-next/src/persistence/row.rs`
- `crates/engine-next/src/persistence/space.rs`
- `crates/engine-next/src/lib.rs`
- `crates/engine-next/tests/engine_graph.rs`
- `crates/engine-next/tests/dependency_guards.rs`

## Existing Old Behavior To Keep

1. Graphs are branch- and space-scoped product entities.
2. Graph names, node IDs, and edge types are validated product identifiers.
3. Node add is an upsert and returns created-versus-updated.
4. Edge add is an upsert by `(src, edge_type, dst)` and returns
   created-versus-updated.
5. Edge add rejects missing source or destination nodes.
6. Node delete removes incident edges.
7. Neighbor lookup supports outgoing, incoming, and both directions.
8. Entity binding is a first-class graph-node fact.
9. Reverse binding lookup returns graph nodes bound to an entity.
10. Batch write exists so relationship loads do not require one commit per
    graph fact.

## Old Behavior Not To Carry Into This Slice

Do not implement these in the core graph slice:

- ontology object and link types
- ontology draft/frozen lifecycle
- ontology validation
- WCC, CDLP, PageRank, LCC, SSSP
- graph snapshot export helpers
- graph-boosted search scoring
- graph BM25 indexing
- branch DAG projection
- semantic graph merge
- append-only duplicate edge bulk behavior
- monolithic packed adjacency rows
- executor command variants
- CLI commands
- benchmark-only lower-layer bypasses

## Design Decisions

1. **Core graph is small.** The first public surface should be easy to reason
   about and should not force analytics, ontology, or search coupling into the
   storage model.

2. **Engine owns graph semantics.** Storage persists rows. Engine validates
   graph names, node IDs, edge identity, entity bindings, neighbor direction,
   batch semantics, and branch/space behavior.

3. **No opaque entity-ref strings.** The old `NodeData.entity_ref: Option<String>`
   was useful but too weak. The rebuilt graph API should use typed product
   identity. If the existing `strata_core::EntityRef` remains branch-absolute,
   add a graph-owned binding wrapper that can represent the intended V1
   relationship contract without storing arbitrary URI strings.

4. **Edge identity is unique.** A graph can have at most one edge for
   `(src, edge_type, dst)`. Upsert replaces the edge payload. The old
   append-only bulk duplicate behavior is not part of the rebuilt contract.

5. **Do not port monolithic packed adjacency first.** The old packed adjacency
   format reduced row count but made point updates rewrite whole adjacency
   blobs and had a hard high-degree-node limit. The first rebuilt graph should
   use sorted edge rows and transactional secondary index rows. If performance
   later requires adjacency compaction, add segmented adjacency as an internal
   optimization behind the same API.

6. **Forward edge row is authoritative.** Incoming-neighbor lookup may use a
   reverse index row, but the forward edge row is the source of truth. Writes
   update forward and reverse rows atomically in one storage commit.

7. **Batch write is all-or-nothing.** Validate every graph operation before
   commit. If any operation is invalid, write nothing. Empty batches return an
   empty outcome without touching storage.

8. **Branch and space are identity.** Graphs with the same name in different
   branches or spaces are independent. Forked branches inherit visible graph
   rows through storage branch visibility, and later writes are branch-local.

9. **Temporal reads use the standard engine temporal model.** Do not add a
   separate graph-only time-travel path. Graph reads should use the same latest
   and timestamp/version context shape as KV, JSON, event, and vector.

10. **Executor remains a delegator later.** The later executor graph command
    slice should deserialize commands, apply defaults, call this engine API,
    and shape outputs. It must not inspect graph row keys or implement graph
    semantics.

## Public Engine API Target

Add `Database::graph(branch, space) -> EngineResult<GraphService>`.

Add public graph types:

- `GraphName`
- `GraphNodeId`
- `GraphEdgeType`
- `GraphDirection`
- `GraphNodeData`
- `GraphEdgeData`
- `GraphNode`
- `GraphEdge`
- `GraphInfo`
- `GraphNodePage`
- `GraphNeighbor`
- `GraphNeighborPage`
- `GraphEntityBinding`
- `GraphBindingTarget`
- `GraphBinding`
- `GraphBindingPage`
- `GraphWriteOutcome`
- `GraphEdgeWriteOutcome`
- `GraphDeleteOutcome`
- `GraphBatchWrite`
- `GraphBatchWriteOutcome`
- `GraphBatchOpOutcome`

Add `GraphService` methods:

- `create_graph(name) -> GraphInfo`
- `delete_graph(name) -> GraphDeleteOutcome`
- `list_graphs(cursor, limit) -> GraphNamePage`
- `graph_info(name) -> Option<GraphInfo>`
- `upsert_node(graph, node_id, data) -> GraphWriteOutcome`
- `get_node(graph, node_id) -> Option<GraphNode>`
- `delete_node(graph, node_id) -> GraphDeleteOutcome`
- `list_nodes(graph, prefix, cursor, limit) -> GraphNodePage`
- `upsert_edge(graph, src, edge_type, dst, data) -> GraphEdgeWriteOutcome`
- `get_edge(graph, src, edge_type, dst) -> Option<GraphEdge>`
- `delete_edge(graph, src, edge_type, dst) -> GraphDeleteOutcome`
- `neighbors(graph, node_id, direction, edge_type, cursor, limit) -> GraphNeighborPage`
- `bindings_for_entity(target, cursor, limit) -> GraphBindingPage`
- `batch_write(graph, batch) -> GraphBatchWriteOutcome`

The method names can be adjusted to match existing engine style, but the
semantic surface should stay this narrow.

## Data Model

### Graph Metadata

Store one graph metadata row per `(branch, space, graph)`.

Metadata should include:

- graph name
- created commit facts if available
- updated commit facts if useful
- optional user-facing metadata map only if other primitives already expose one

Do not include ontology status in this slice.

### Node Rows

Store one node row per `(graph, node_id)`.

Node payload:

- node id
- optional entity binding
- optional JSON-like property object
- product revision or row revision if consistent with other primitives

Node properties are application data. They are not schema-validated until the
ontology slice.

### Edge Rows

Store one authoritative forward edge row per `(graph, src, edge_type, dst)`.

Edge payload:

- source node id
- destination node id
- edge type
- weight, default `1.0`
- optional JSON-like property object
- product revision or row revision if consistent with other primitives

Store one reverse index row per `(graph, dst, edge_type, src)` for incoming
neighbor lookup. The reverse row may contain either the full edge payload or a
pointer to the forward edge row. Prefer full payload only if it makes reads
substantially simpler and all writes keep rows atomic.

### Binding Index Rows

If a node has an entity binding, store one reverse binding index row:

```text
target -> graph -> node_id
```

Binding rows are storage-coupled indexes. They are updated in the same commit
as node upsert/delete. They are not search indexes, not executor state, and not
storage concepts.

### Catalog Rows

Store graph names in a graph catalog row or use key-prefix listing if the
engine's persistence layer already provides cheap bounded list support. Pick
one approach and make list ordering deterministic.

## Row-Key Target

Add graph row classes and key helpers in persistence:

- graph metadata row
- node row
- forward edge row
- reverse edge index row
- binding index row
- optional graph catalog row

Key encoding must be binary-safe and versioned. Do not use slash-delimited
string keys as the durable format unless the existing engine persistence layer
already standardizes that format for product rows.

## Batch Write Semantics

`GraphBatchWrite` should support:

- node upserts
- edge upserts
- node deletes
- edge deletes

Validation rules:

1. Graph must exist.
2. All identifiers must be valid.
3. Edge endpoints must exist either before the batch or be created earlier in
   the same batch.
4. Edge deletes of missing edges are no-op delete outcomes.
5. Node deletes remove incident edges, including edges created earlier in the
   same batch unless the batch explicitly recreates them after the delete.
6. Duplicate operations on the same node or edge are applied in batch order.
7. The whole batch commits atomically after validation.

If order-dependent validation becomes hard to implement cleanly, restrict the
first batch shape to node upserts followed by edge upserts and document that
delete batches are deferred. Do not silently accept partially-applied batches.

## Implementation Order

1. **Module skeleton**
   - Add `api/graph.rs`.
   - Add `data/graph/{mod.rs,types.rs,outcome.rs,record.rs,service.rs}`.
   - Re-export from `api/mod.rs`, `data/mod.rs`, and crate root.

2. **Validated product types**
   - Implement graph name, node ID, edge type, direction, binding target, node
     data, and edge data.
   - Reuse existing engine validation helpers where possible.

3. **Persistence keys and rows**
   - Add graph row classes.
   - Add versioned key encoding and decoding.
   - Add record envelopes for graph metadata, nodes, edges, reverse indexes,
     and binding indexes.

4. **Graph lifecycle**
   - Implement create, delete, list, and info.
   - Delete must remove graph metadata, nodes, edges, reverse edge rows, binding
     rows, and catalog facts.

5. **Node operations**
   - Implement upsert/get/delete/list.
   - Maintain binding index rows.
   - Node delete must plan incident edge cleanup.

6. **Edge operations**
   - Implement upsert/get/delete.
   - Enforce endpoint existence.
   - Maintain reverse edge rows.
   - Ensure edge update changes both forward and reverse facts atomically.

7. **Neighbor lookup**
   - Implement outgoing, incoming, and both directions.
   - Add optional edge type filtering and cursor/limit pagination.
   - Make ordering deterministic.

8. **Binding lookup**
   - Implement `bindings_for_entity`.
   - Return graph and node identity plus resolved binding facts.

9. **Batch write**
   - Implement all-or-nothing batch validation and commit.
   - Include created/updated/deleted/no-op outcomes.

10. **Temporal and branch behavior**
    - Wire latest reads through standard visible-row reads.
    - Wire timestamp/version reads only if the existing engine API has a
      consistent public temporal context for all primitives.
    - Prove branch fork and branch-local writes.

11. **Source/dependency guards**
    - Ensure engine graph code does not call old `strata_engine::graph`.
    - Ensure executor code is not touched in this slice.
    - Ensure no benchmark-only graph API exists.

## Done Criteria

- Core graph API exists and is crate-root reachable.
- Cache and durable-local modes can create graphs, write nodes and edges, read
  them back, list them, and query neighbors.
- Branch and space isolation are covered by tests.
- Durable reopen preserves graph facts and indexes.
- Batch writes are atomic and covered by failure tests.
- Node delete removes incident edges and index rows.
- Entity binding lookup works without opaque URI strings.
- No ontology, analytics, search boost, branch DAG, semantic merge, executor
  commands, or benchmark bypasses are introduced.
