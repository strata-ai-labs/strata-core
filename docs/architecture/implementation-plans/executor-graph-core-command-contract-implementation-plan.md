# Executor Graph Core Command Contract Implementation Plan

## Problem

The executor crate is the serialized command boundary for SDKs, MCP servers,
CLIs, IPC clients, and smoke tools. Graph commands should use the same command
dispatch architecture restored for KV, JSON, vector, and event: clients send a
serialized `Command`, executor applies command-boundary defaults and wire
conversion, engine performs product semantics, and executor returns a
serialized `Output`.

The old executor exposed a wide graph command set that mixed core graph facts,
ontology, traversal, analytics, graph-boosted search, and transaction-session
helpers. The rebuilt graph command slice should expose only the reasonable core
surface from `engine-graph-core-implementation-plan.md`: named graphs, nodes,
edges, neighbor lookup, entity-binding lookup, and all-or-nothing batch writes.

## Old Evidence

- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/graph.rs`
- `crates/executor/src/handlers/graph_impl.rs`
- `crates/executor/src/session.rs`
- `crates/engine/src/graph/types.rs`
- `crates/engine/src/graph/lifecycle.rs`
- `crates/engine/src/graph/nodes.rs`
- `crates/engine/src/graph/edges.rs`
- `crates/engine/src/graph/traversal.rs`
- `crates/engine/src/graph/bulk.rs`

## Current Targets

- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`
- `crates/engine-next/src/api/graph.rs`
- `crates/engine-next/src/data/graph/`

## Required Engine Surface

Do not implement executor graph dispatch until the engine graph core API
exists. The executor implementation depends on these engine methods from
`engine-graph-core-implementation-plan.md`:

- `create_graph`
- `delete_graph`
- `list_graphs`
- `graph_info`
- `upsert_node`
- `get_node`
- `delete_node`
- `list_nodes`
- `upsert_edge`
- `get_edge`
- `delete_edge`
- `neighbors`
- `bindings_for_entity`
- `batch_write`

## Design Decisions

1. **Serialized command remains the public executor path.** Rust convenience
   methods for graph must build and execute `Command::Graph...` variants.

2. **Executor is a stateless delegator.** It may deserialize command payloads,
   default branch/space, validate public request shape, convert wire types,
   map errors, and shape outputs. It must not inspect graph storage rows,
   compute graph adjacency, enforce endpoint existence, maintain reverse
   indexes, maintain binding indexes, or implement graph batch semantics.

3. **Engine owns graph semantics.** Graph lifecycle, identifier validation,
   node upsert semantics, edge identity, incident-edge cleanup, neighbor
   direction behavior, entity binding behavior, batch atomicity, branch
   visibility, and durable persistence stay in engine.

4. **The first graph executor surface is intentionally narrow.** Do not port
   ontology commands, WCC, CDLP, PageRank, LCC, SSSP, BFS, semantic merge,
   graph-boosted search, export helpers, or branch DAG projection in this
   slice.

5. **Keep useful old command names where they still describe the new surface.**
   Preserve `GraphCreate`, `GraphDelete`, `GraphList`, `GraphGetMeta`,
   `GraphAddNode`, `GraphGetNode`, `GraphRemoveNode`, `GraphListNodes`,
   `GraphAddEdge`, `GraphRemoveEdge`, and `GraphNeighbors` unless the current
   executor naming style requires `Upsert` or `Info`. Do not preserve old
   commands whose semantics are intentionally excluded.

6. **Use graph-specific output variants.** Do not shape graph reads through
   generic KV, JSON, or heterogeneous value outputs. Graph metadata, nodes,
   edges, neighbor pages, binding pages, and batch outcomes need named output
   shapes.

7. **Branch and space defaults match other primitives.** Omitted branch
   resolves to the executor handle default branch. Omitted space resolves to
   `"default"`. Explicit branch and space override defaults.

8. **Entity binding is typed on the wire.** The old graph surface accepted an
   opaque `entity_ref` string. The rebuilt executor should expose the typed
   binding target shape used by engine graph core instead of encoding product
   identity into free-form strings.

9. **Edge identity order is stable.** Wire commands should use `src`,
   `edge_type`, and `dst`. The executor converts those fields into the engine
   graph edge identity and returns the same identity fields in outputs.

10. **Batch write is all-or-nothing through engine.** Executor validates only
    the serialized command shape and converts each operation into an engine
    batch operation. Engine decides graph validity and commits or rejects the
    whole batch.

11. **Temporal graph reads are deferred unless engine exposes them in this
    slice.** The first graph core engine plan targets latest reads. If engine
    graph later adds timestamp/version reads, executor should extend these
    commands with `as_of` using the same microsecond timestamp convention used
    by KV, JSON, vector, and event.

12. **Transaction-session graph commands are deferred.** The executor command
    boundary remains the public SDK/CLI/MCP path. Session-level graph command
    support can be added after engine transaction participation is defined for
    graph core.

## Public Graph Command Set

Add these command variants:

| Command | Inputs | Output |
| --- | --- | --- |
| `GraphCreate` | branch?, space?, graph | `GraphInfo` |
| `GraphDelete` | branch?, space?, graph | `GraphDeleteResult` |
| `GraphList` | branch?, space?, cursor?, limit? | `GraphNamePage` |
| `GraphGetMeta` | branch?, space?, graph | `GraphInfoResult` |
| `GraphAddNode` | branch?, space?, graph, node_id, properties?, binding? | `GraphNodeWriteResult` |
| `GraphGetNode` | branch?, space?, graph, node_id | `GraphNodeResult` |
| `GraphRemoveNode` | branch?, space?, graph, node_id | `GraphDeleteResult` |
| `GraphListNodes` | branch?, space?, graph, prefix?, cursor?, limit? | `GraphNodePage` |
| `GraphAddEdge` | branch?, space?, graph, src, edge_type, dst, weight?, properties? | `GraphEdgeWriteResult` |
| `GraphGetEdge` | branch?, space?, graph, src, edge_type, dst | `GraphEdgeResult` |
| `GraphRemoveEdge` | branch?, space?, graph, src, edge_type, dst | `GraphDeleteResult` |
| `GraphNeighbors` | branch?, space?, graph, node_id, direction, edge_type?, cursor?, limit? | `GraphNeighborPage` |
| `GraphBindingsForEntity` | branch?, space?, target, cursor?, limit? | `GraphBindingPage` |
| `GraphBatchWrite` | branch?, space?, graph, operations | `GraphBatchWriteResult` |

Preserve old field names where they match the rebuilt semantics: `branch`,
`space`, `graph`, `node_id`, `src`, `dst`, `edge_type`, `weight`,
`properties`, and `direction`.

Use `cursor`, `limit`, `prefix`, `binding`, `target`, and `operations` for the
newer paginated and batch operations.

## Commands Intentionally Excluded

Do not add these old command variants in this slice:

- `GraphListNodesPaginated` as a separate command; fold pagination into
  `GraphListNodes`.
- `GraphBulkInsert`; use `GraphBatchWrite` with explicit operation outcomes.
- `GraphBfs`.
- `GraphDefineObjectType`.
- `GraphGetObjectType`.
- `GraphListObjectTypes`.
- `GraphDeleteObjectType`.
- `GraphDefineLinkType`.
- `GraphGetLinkType`.
- `GraphListLinkTypes`.
- `GraphDeleteLinkType`.
- `GraphFreezeOntology`.
- `GraphOntologyStatus`.
- `GraphOntologySummary`.
- `GraphListOntologyTypes`.
- `GraphNodesByType`.
- `GraphWcc`.
- `GraphCdlp`.
- `GraphPagerank`.
- `GraphLcc`.
- `GraphSssp`.

These names should remain absent from `executor-next` until their own engine
plans exist.

## Wire Types

Add serializable request helper types:

- `GraphDirection`
  - `outgoing`
  - `incoming`
  - `both`
- `GraphBindingTarget`
  - primitive kind
  - branch if the engine binding contract requires it
  - space
  - key or product identifier
- `GraphEntityBinding`
  - target
  - optional label or role only if engine graph core exposes one
- `GraphNodeData`
  - properties
  - binding
- `GraphEdgeData`
  - weight
  - properties
- `GraphBatchOperation`
  - node upsert
  - node delete
  - edge upsert
  - edge delete
- `GraphBatchNodeUpsert`
  - node_id
  - properties
  - binding
- `GraphBatchEdgeUpsert`
  - src
  - edge_type
  - dst
  - weight
  - properties

Add serializable output helper types:

- `GraphInfoData`
  - graph
  - created_version
  - created_timestamp
  - updated_version
  - updated_timestamp
  - node_count
  - edge_count
- `GraphNodeDataOutput`
  - graph
  - node_id
  - properties
  - binding
  - version
  - timestamp
- `GraphEdgeDataOutput`
  - graph
  - src
  - edge_type
  - dst
  - weight
  - properties
  - version
  - timestamp
- `GraphNeighborHit`
  - node
  - edge
  - direction
- `GraphBindingHit`
  - graph
  - node_id
  - binding
  - version
  - timestamp
- `GraphBatchItemResult`
  - operation_index
  - operation
  - created
  - deleted
  - version
  - timestamp
  - error

JSON-like `properties` fields should use `serde_json::Value` but must be
object-shaped if engine graph core requires object-only properties.

## Output Variants

Add graph-specific output variants:

- `GraphInfo(GraphInfoData)`
- `GraphInfoResult(Option<GraphInfoData>)`
- `GraphNamePage { graphs, has_more, cursor }`
- `GraphNodeResult(Option<GraphNodeDataOutput>)`
- `GraphNodePage { nodes, has_more, cursor }`
- `GraphEdgeResult(Option<GraphEdgeDataOutput>)`
- `GraphNeighborPage { neighbors, has_more, cursor }`
- `GraphBindingPage { bindings, has_more, cursor }`
- `GraphNodeWriteResult { graph, node_id, created, version, timestamp }`
- `GraphEdgeWriteResult { graph, src, edge_type, dst, created, version, timestamp }`
- `GraphDeleteResult { graph, key, deleted, version, timestamp }`
- `GraphBatchWriteResult { graph, results, version, timestamp }`

If the current output style prefers tuple variants for pages, keep the same
semantic facts and stable JSON field names. Do not reuse old generic `Keys`
for graph names or node IDs because graph pages need cursor facts and type
clarity.

## Handler Flow

For each graph command:

1. Resolve branch from command field or executor default branch.
2. Resolve space from command field or `"default"`.
3. Convert wire identifiers into engine graph identifiers.
4. Convert wire JSON values and binding targets into engine graph data types.
5. Call `database.graph(branch, space)` or the equivalent service accessor.
6. Call exactly one engine graph service method for non-batch commands.
7. Convert the engine outcome into a graph-specific executor output.
8. Map engine validation and not-found errors into executor errors without
   exposing storage row keys or storage error internals.

For `GraphBatchWrite`, conversion may iterate over input operations to build
one engine batch request, but executor must call one engine batch method.

## Error Mapping

Map engine graph errors into stable executor errors:

- invalid graph name -> invalid input
- invalid node ID -> invalid input
- invalid edge type -> invalid input
- invalid direction -> invalid input
- non-object node properties when object-only -> invalid input
- non-object edge properties when object-only -> invalid input
- non-finite weight -> invalid input
- malformed entity binding -> invalid input
- missing graph -> not found or documented no-op depending on engine method
- missing edge endpoint -> conflict or invalid input according to engine
  contract
- duplicate graph create -> conflict if engine create is not idempotent
- storage or durability failure -> internal or storage-backed executor error

The executor should not rewrite engine graph policy decisions. It should only
translate them into the executor error vocabulary.

## Source And Dependency Guards

Add or extend tests so `crates/executor-next` graph command sources do not
depend on:

- `strata-storage-next`
- storage row, commit, WAL, table, lifecycle, compaction, or source-shape
  internals
- engine graph data module internals outside public API re-exports
- old `strata-executor`
- old `strata-engine::graph`
- ontology modules
- analytics modules
- search or embedding modules

The executor may depend on `strata-engine-next` public API types and shared
wire helper types in its own crate.

## Implementation Order

1. Confirm engine graph core public API exists and is re-exported from
   `crates/engine-next/src/lib.rs`.
2. Add graph wire helper types in `crates/executor-next/src/types.rs`.
3. Add graph output variants in `crates/executor-next/src/output.rs`.
4. Add graph command variants in `crates/executor-next/src/command.rs`.
5. Add command `name()` coverage and branch/space helper coverage.
6. Add conversion helpers from executor wire graph types to engine graph API
   types.
7. Add executor dispatch for graph commands in `crates/executor-next/src/executor.rs`.
8. Add graph convenience methods only if existing executor style has
   primitive-specific convenience methods. Those methods must call
   `execute(Command::Graph...)`.
9. Add source guards preventing lower-layer bypasses and excluded old graph
   commands.
10. Run the graph command contract tests in cache and durable-local fixtures.

## Done Criteria

- All documented graph core commands serialize and deserialize.
- Every graph command has stable `Command::name()` coverage.
- Every graph command maps to one graph-specific output shape.
- Omitted branch and space follow the same defaults as other primitives.
- Executor graph dispatch delegates to engine graph API and does not implement
  graph storage semantics.
- Batch write reaches engine as one batch request and preserves positional
  output.
- Ontology, analytics, BFS, search, semantic merge, and branch DAG commands are
  absent from `executor-next`.
- Source guards prevent storage and old-engine dependencies from entering the
  executor graph command path.
- Cache and durable-local behavior tests pass through the serialized command
  boundary.
