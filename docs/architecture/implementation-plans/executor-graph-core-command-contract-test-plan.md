# Executor Graph Core Command Contract Test Plan

## Purpose

Prove that the executor crate exposes a stable serialized graph core command
boundary and remains a thin delegator over engine graph APIs. The graph command
tests should cover graph lifecycle, node and edge operations, neighbor lookup,
entity binding lookup, batch write atomicity, branch/space defaults, isolation,
and durable reopen behavior without reimplementing graph semantics in executor.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Output serde round-trip | Required | Required |
| Graph lifecycle | Required | Required |
| Node CRUD | Required | Required |
| Edge CRUD | Required | Required |
| Neighbor lookup | Required | Required |
| Entity binding lookup | Required | Required |
| Batch write | Required | Required |
| Branch and space defaults | Required | Required |
| Branch and space isolation | Required | Required |
| Error mapping | Required | Required |
| Reopen persistence | Not applicable | Required |
| Source guards | Required | Required |

## Contract Tests

### Command JSON Round Trip

- Serialize and deserialize `GraphCreate`.
- Serialize and deserialize `GraphDelete`.
- Serialize and deserialize `GraphList`.
- Serialize and deserialize `GraphGetMeta`.
- Serialize and deserialize `GraphAddNode`.
- Serialize and deserialize `GraphGetNode`.
- Serialize and deserialize `GraphRemoveNode`.
- Serialize and deserialize `GraphListNodes`.
- Serialize and deserialize `GraphAddEdge`.
- Serialize and deserialize `GraphGetEdge`.
- Serialize and deserialize `GraphRemoveEdge`.
- Serialize and deserialize `GraphNeighbors`.
- Serialize and deserialize `GraphBindingsForEntity`.
- Serialize and deserialize `GraphBatchWrite`.
- Include omitted branch/space.
- Include explicit branch/space.
- Include graph names, node IDs, edge types, and string cursors.
- Include node properties with nested JSON values.
- Include edge properties with nested JSON values.
- Include `outgoing`, `incoming`, and `both` directions.
- Include edge type filters.
- Include node prefix, cursor, and limit.
- Include entity binding targets for every primitive kind supported by engine.
- Include empty batch operations.
- Include mixed node and edge batch operations.
- Assert deserialized command equality.

### Output JSON Round Trip

- Serialize and deserialize `GraphInfo`.
- Serialize and deserialize present and missing `GraphInfoResult`.
- Serialize and deserialize `GraphNamePage` with and without cursor.
- Serialize and deserialize present and missing `GraphNodeResult`.
- Serialize and deserialize `GraphNodePage` with and without cursor.
- Serialize and deserialize present and missing `GraphEdgeResult`.
- Serialize and deserialize `GraphNeighborPage` with outgoing hits.
- Serialize and deserialize `GraphNeighborPage` with incoming hits.
- Serialize and deserialize `GraphBindingPage`.
- Serialize and deserialize `GraphNodeWriteResult` for create and update.
- Serialize and deserialize `GraphEdgeWriteResult` for create and update.
- Serialize and deserialize `GraphDeleteResult` for deleted and no-op.
- Serialize and deserialize `GraphBatchWriteResult` with positional item
  results.
- Include version and timestamp fields where the engine output provides them.
- Include optional fields omitted and present.

### Command Name Coverage

- Assert `Command::name()` returns the stable name for every graph command.
- The match must be exhaustive so adding a command without naming it fails
  compilation.
- Assert excluded old graph commands do not appear in `Command::name()`.

### Command-To-Output Mapping

- Execute each graph command on a small cache database.
- Assert the output variant exactly matches the documented mapping.
- `GraphCreate` returns `GraphInfo`.
- `GraphList` returns `GraphNamePage`.
- `GraphGetMeta` returns `GraphInfoResult`.
- `GraphGetNode` returns `GraphNodeResult`.
- `GraphListNodes` returns `GraphNodePage`.
- `GraphGetEdge` returns `GraphEdgeResult`.
- `GraphNeighbors` returns `GraphNeighborPage`.
- `GraphBindingsForEntity` returns `GraphBindingPage`.
- `GraphBatchWrite` returns `GraphBatchWriteResult`.

## Delegation Tests

### Executor Uses Engine APIs

- Source guard rejects storage crate imports in executor graph sources.
- Source guard rejects storage row, storage commit, table, WAL, lifecycle, and
  compaction type names in executor graph sources.
- Source guard rejects engine graph data module internals in executor graph
  sources.
- Source guard rejects old `strata_engine::graph` imports.
- Source guard rejects old `strata_executor` imports.
- Source guard rejects search, embed runtime, ontology, analytics, semantic
  merge, branch DAG, and export imports in executor graph sources.

### Convenience Facade Uses Commands

- Any graph convenience method must call `execute(Command::Graph...)`.
- Convenience methods must not directly call engine graph service methods.
- Convenience methods must not scan graph rows, calculate adjacency, enforce
  edge endpoint existence, maintain indexes, or implement batch semantics.

### No Lower-Layer Bypass

- Graph smoke loaders and benchmarks use executor graph batch commands or
  public engine graph APIs.
- Source guard rejects direct storage writes from those binaries.
- Source guard rejects row-key construction from executor graph code.

### Excluded Surface Guard

- Source guard rejects these command variant names in `crates/executor-next`:
  - `GraphBulkInsert`
  - `GraphBfs`
  - `GraphDefineObjectType`
  - `GraphGetObjectType`
  - `GraphListObjectTypes`
  - `GraphDeleteObjectType`
  - `GraphDefineLinkType`
  - `GraphGetLinkType`
  - `GraphListLinkTypes`
  - `GraphDeleteLinkType`
  - `GraphFreezeOntology`
  - `GraphOntologyStatus`
  - `GraphOntologySummary`
  - `GraphListOntologyTypes`
  - `GraphNodesByType`
  - `GraphWcc`
  - `GraphCdlp`
  - `GraphPagerank`
  - `GraphLcc`
  - `GraphSssp`

## Behavior Tests

Run behavior tests in both cache and durable-local executor fixtures unless the
test specifically targets reopen.

### Graph Lifecycle

- Execute `GraphList` in an empty space and assert an empty page.
- Execute `GraphCreate`.
- Assert returned graph name matches input.
- Execute `GraphGetMeta` and assert metadata is present.
- Execute `GraphList` and assert the graph appears.
- Execute duplicate `GraphCreate` and assert documented idempotent or conflict
  behavior.
- Execute `GraphDelete` for an existing graph and assert `deleted=true`.
- Execute `GraphGetMeta` and assert missing result.
- Execute `GraphDelete` for a missing graph and assert documented no-op or
  not-found behavior.
- Recreate a deleted graph and assert it starts empty.

### Graph List Pagination

- Create multiple graphs with deterministic names.
- Execute `GraphList` with a limit smaller than the graph count.
- Assert deterministic ordering.
- Assert `has_more=true`.
- Use returned cursor for the next page.
- Assert no duplicate graph names across pages.
- Assert final page has `has_more=false`.
- Execute `GraphList` with `limit == 0` and assert empty page behavior.

### Node Upsert And Get

- Create a graph.
- Execute `GraphAddNode` with no properties and no binding.
- Assert `created=true`.
- Execute `GraphGetNode` and assert node data is present.
- Execute `GraphAddNode` for the same node with properties.
- Assert `created=false`.
- Execute `GraphGetNode` and assert properties were replaced.
- Execute `GraphGetNode` for a missing node and assert missing result.
- Execute `GraphAddNode` with nested object properties.
- Assert nested properties round-trip through executor output.

### Node Delete

- Create a node.
- Execute `GraphRemoveNode`.
- Assert `deleted=true`.
- Execute `GraphGetNode` and assert missing result.
- Execute `GraphRemoveNode` again.
- Assert documented no-op delete behavior.
- Delete a node with an entity binding.
- Assert binding lookup no longer returns the node.

### Node List

- Insert ordered node IDs.
- Execute `GraphListNodes` without prefix.
- Assert deterministic ordering.
- Execute `GraphListNodes` with prefix.
- Assert only matching IDs are returned.
- Execute `GraphListNodes` with cursor and limit.
- Assert stable pagination.
- Assert deleted nodes are suppressed.
- Execute against a missing graph and assert documented error mapping.

### Edge Upsert And Get

- Create a graph with source and destination nodes.
- Execute `GraphAddEdge`.
- Assert `created=true`.
- Execute `GraphGetEdge` and assert edge data is present.
- Execute `GraphAddEdge` for the same `(src, edge_type, dst)` with new
  weight and properties.
- Assert `created=false`.
- Execute `GraphGetEdge` and assert edge data was replaced.
- Execute `GraphGetEdge` for a missing edge and assert missing result.
- Insert multiple edge types between the same node pair.
- Assert each edge can be read independently.

### Edge Delete

- Create an edge.
- Execute `GraphRemoveEdge`.
- Assert `deleted=true`.
- Execute `GraphGetEdge` and assert missing result.
- Execute `GraphRemoveEdge` again.
- Assert documented no-op delete behavior.
- Assert neighbor lookup no longer returns the deleted edge.

### Edge Validation Mapping

- Execute `GraphAddEdge` with a missing source node.
- Assert executor maps the engine error to the documented error class.
- Execute `GraphAddEdge` with a missing destination node.
- Assert executor maps the engine error to the documented error class.
- Execute `GraphAddEdge` with non-finite weight through JSON input if serde can
  represent it; otherwise cover in Rust command construction.
- Assert invalid input error and no partial write.

### Neighbor Lookup

- Build a graph with outgoing, incoming, and unrelated edges.
- Execute `GraphNeighbors` with outgoing direction.
- Assert only destination neighbors are returned.
- Execute `GraphNeighbors` with incoming direction.
- Assert only source neighbors are returned.
- Execute `GraphNeighbors` with both direction.
- Assert incoming and outgoing neighbors are returned.
- Execute `GraphNeighbors` with edge type filter.
- Assert only matching edge types are returned.
- Execute with cursor and limit.
- Assert deterministic pagination.
- Execute for a node with no edges.
- Assert empty page.
- Execute for a missing node and assert documented empty or not-found behavior.

### Self-Loop Behavior

- Create a node with a self-loop edge.
- Execute outgoing neighbor lookup.
- Execute incoming neighbor lookup.
- Execute both-direction neighbor lookup.
- Assert documented self-loop duplication or de-duplication behavior exactly.
- Delete the self-loop.
- Assert all neighbor views are empty.

### Entity Binding Lookup

- Create a node with a typed entity binding.
- Execute `GraphBindingsForEntity`.
- Assert the node appears in the binding page.
- Create multiple nodes bound to the same entity.
- Assert all bound nodes are returned deterministically.
- Update one node to a different binding.
- Assert old entity lookup no longer returns that node.
- Assert new entity lookup returns that node.
- Delete a bound node.
- Assert lookup no longer returns it.
- Delete a graph.
- Assert graph deletion removes binding lookup results.

### Batch Write Success

- Execute empty `GraphBatchWrite`.
- Assert empty result and no graph data changes.
- Execute one batch that creates multiple nodes.
- Assert positional result count equals input operation count.
- Read each node back.
- Execute one batch that creates source node, destination node, and edge in
  that order.
- Read the edge back.
- Execute one batch that updates an existing node and existing edge.
- Assert `created=false` for updated items if engine exposes that fact.

### Batch Write Atomicity

- Create a graph and one valid node.
- Execute `GraphBatchWrite` with one valid node upsert and one invalid edge.
- Assert the command fails or returns a failed batch outcome according to the
  engine contract.
- Assert the valid node upsert from the failed batch did not commit.
- Execute `GraphBatchWrite` with invalid node ID after valid operations.
- Assert no partial writes.
- Execute `GraphBatchWrite` with invalid edge type after valid operations.
- Assert no partial writes.

### Batch Delete

- Create nodes and edges.
- Execute `GraphBatchWrite` with edge delete and node delete operations.
- Assert positional delete outcomes.
- Assert deleted edge is absent.
- Assert deleted node is absent.
- Assert incident edges for deleted nodes are absent.
- Execute duplicate delete operations and assert documented no-op behavior.

### Branch And Space Defaults

- Omit branch and space and assert executor default branch and `"default"`
  space.
- Repeat with explicit branch and explicit space.
- Set the executor default branch and assert omitted branch uses it.
- Explicit branch overrides executor default branch.
- Explicit space overrides `"default"`.

### Branch Isolation

- Create a source branch and graph.
- Add nodes and edges in the source branch.
- Fork the branch through executor branch commands.
- Read inherited graph facts from the fork.
- Add a node in the fork.
- Assert the source branch does not see fork-only node.
- Add an edge in the fork.
- Assert source neighbor lookup is unchanged.
- Delete a source-inherited node in the fork.
- Assert the source branch still sees the node.

### Space Isolation

- Create the same graph name in two spaces.
- Add different nodes in each space.
- Assert each space lists only its nodes.
- Add same edge identity in each space with different properties.
- Assert reads stay space-local.
- Assert binding lookups stay space-local.

### Durable Reopen

- Open a durable-local executor fixture.
- Create a graph.
- Add nodes, edges, and bindings.
- Close the executor/database cleanly.
- Reopen the same path.
- Assert graph metadata persists.
- Assert node reads persist.
- Assert edge reads persist.
- Assert neighbor lookup persists.
- Assert entity binding lookup persists.
- Delete data after reopen and close cleanly.

### Error Mapping

- Invalid graph name maps to invalid input.
- Invalid node ID maps to invalid input.
- Invalid edge type maps to invalid input.
- Invalid direction maps to invalid input.
- Non-object node properties map to invalid input when engine requires object
  properties.
- Non-object edge properties map to invalid input when engine requires object
  properties.
- Malformed entity binding maps to invalid input.
- Missing graph for read maps to documented missing behavior.
- Missing graph for write maps to documented missing behavior.
- Missing edge endpoint maps to documented conflict or invalid input behavior.
- Storage failure fixtures, if available, map to executor internal/storage
  errors without exposing row keys.

## Regression Tests Against Old Surface Width

- Assert graph command count matches the narrow core command list.
- Assert old ontology command names are absent.
- Assert old analytics command names are absent.
- Assert old BFS command name is absent.
- Assert old bulk-insert command name is absent.
- Assert executor graph outputs do not include analytics summary types.
- Assert executor graph outputs do not include ontology types.

## Suggested Test Files

- `crates/executor-next/tests/graph_command_contract.rs`
- `crates/executor-next/tests/graph_behavior.rs`
- Extend `crates/executor-next/tests/error_and_guards.rs`

Keep unit-style serde and source-guard tests separate from behavior tests so
failures point to either command shape or engine delegation.

## Gate Commands

Run focused graph executor gates first:

```sh
cargo test -p strata-executor-next graph
```

Run supporting engine graph gates:

```sh
cargo test -p strata-engine-next graph
```

Run the broader executor command contract after graph passes:

```sh
cargo test -p strata-executor-next
```

## Done Criteria

- All graph command variants round-trip through JSON.
- All graph output variants round-trip through JSON.
- Every graph command has stable command name coverage.
- Every graph command returns the documented output variant.
- Behavior tests pass in cache and durable-local modes.
- Durable reopen proves graph metadata, nodes, edges, neighbor indexes, and
  binding indexes are visible after reopening.
- Batch write tests prove success ordering and failure atomicity.
- Branch and space tests prove defaulting and isolation.
- Source guards prove executor graph code does not depend on storage internals,
  old engine graph internals, ontology, analytics, search, or export helpers.
- Excluded old graph surface remains absent from `executor-next`.
