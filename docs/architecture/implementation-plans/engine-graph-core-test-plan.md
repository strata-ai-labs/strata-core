# Engine Graph Core Test Plan

## Purpose

Prove that the rebuilt engine owns a narrow, correct graph core before adding
ontology or executor commands. The graph core must persist graph metadata,
nodes, edges, reverse indexes, and entity-binding indexes through normal engine
APIs in cache and durable-local modes.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Type validation | Required | Required |
| Row-key encoding and decoding | Required | Required |
| Record envelope encoding | Required | Required |
| Graph lifecycle | Required | Required |
| Node CRUD | Required | Required |
| Edge CRUD | Required | Required |
| Neighbor lookup | Required | Required |
| Entity binding lookup | Required | Required |
| Batch write atomicity | Required | Required |
| Branch isolation and fork behavior | Required | Required |
| Space isolation | Required | Required |
| Durable reopen | Not applicable | Required |
| Source/dependency guards | Required | Required |

## Unit Tests

### Graph Name Validation

- Accept ordinary graph names.
- Accept maximum-length graph names.
- Reject empty graph names.
- Reject graph names above the configured limit.
- Reject graph names with null bytes.
- Reject names reserved for internal graph/control rows.
- Reject names that cannot be encoded in the durable key format.
- Error class is graph-owned and does not expose storage internals.

### Node ID Validation

- Accept ordinary node IDs.
- Accept maximum-length node IDs.
- Accept UTF-8 node IDs if the product naming rules allow them.
- Reject empty node IDs.
- Reject node IDs above the configured limit.
- Reject null bytes.
- Preserve deterministic sort order for list pagination.

### Edge Type Validation

- Accept ordinary edge types.
- Accept maximum-length edge types.
- Reject empty edge types.
- Reject edge types above the configured limit.
- Reject null bytes.
- Reject reserved internal edge types if any are introduced.

### Direction Validation

- Parse outgoing direction.
- Parse incoming direction.
- Parse both direction.
- Reject unknown direction strings at the public API boundary if strings are
  used in helper constructors.
- Default direction is explicit in tests, not assumed by hidden behavior.

### Node Data Validation

- Accept node with no properties and no binding.
- Accept node with properties object.
- Reject non-object properties if graph node properties are object-only.
- Accept node with typed entity binding.
- Reject malformed entity binding.
- Reject cross-branch binding if V1 binding semantics remain branch-local.
- Reject implicit cross-space binding unless cross-space binding is explicitly
  represented.

### Edge Data Validation

- Default weight is `1.0`.
- Accept custom finite weight.
- Reject NaN weight.
- Reject positive infinity.
- Reject negative infinity.
- Accept edge properties object.
- Reject non-object properties if edge properties are object-only.

### Row-Key Encoding

- Encode graph metadata row deterministically.
- Encode node row deterministically.
- Encode forward edge row deterministically.
- Encode reverse edge index row deterministically.
- Encode binding index row deterministically.
- Decode each fixture row back to product identity.
- Decode maximum-length identifiers.
- Reject unknown key version.
- Reject unknown graph row discriminator.
- Reject truncated key payload.
- Reject invalid UTF-8 when the field is defined as UTF-8.
- Do not decode KV, JSON, event, vector, or control rows as graph rows.

### Record Envelopes

- Encode and decode graph metadata.
- Encode and decode node record with properties and binding.
- Encode and decode node record without optional fields.
- Encode and decode edge record with weight and properties.
- Encode and decode reverse edge index record.
- Encode and decode binding index record.
- Reject unknown record version.
- Reject record identity mismatch against the row key.
- Reject corrupt JSON-like payloads.
- Reject non-finite edge weights on decode.

### Outcome Types

- Graph info exposes graph name and commit facts where available.
- Node read exposes graph, node id, data, and commit facts where available.
- Edge read exposes graph, source, edge type, destination, data, and commit
  facts where available.
- Node write outcome exposes created-versus-updated.
- Edge write outcome exposes created-versus-updated.
- Delete outcome exposes deleted-versus-no-op.
- Page outcomes expose items and continuation cursor.
- Batch outcome preserves operation order.
- Outcome structs do not expose storage keys, row classes, or storage request
  types.

## Engine Behavior Tests

Run each behavior test against both cache and durable-local fixtures unless the
test specifically targets durable reopen.

### Graph Lifecycle

- Create graph returns graph info.
- Duplicate create with the same name follows documented idempotent or conflict
  behavior.
- List graphs on an empty space returns empty list.
- List graphs returns deterministic order.
- Graph info returns metadata for an existing graph.
- Graph info returns `None` for a missing graph.
- Delete missing graph returns no-op delete outcome.
- Delete existing empty graph removes it from list.
- Delete existing non-empty graph removes graph, nodes, edges, reverse indexes,
  binding indexes, and catalog facts.
- Recreate after delete starts with an empty graph.

### Node CRUD

- Upsert node in existing graph creates a node.
- Upsert same node replaces properties.
- Upsert same node replaces binding and removes old binding index row.
- Get existing node returns latest data.
- Get missing node returns `None`.
- List nodes returns deterministic order.
- List nodes with prefix returns only matching node IDs.
- List nodes with cursor and limit returns stable pages.
- Delete existing node removes the node.
- Delete missing node returns no-op delete outcome.
- Delete node removes its binding index row.
- Delete node removes outgoing incident edges.
- Delete node removes incoming incident edges.
- Delete node handles self-loop without double-decrement or stale index rows.

### Edge CRUD

- Upsert edge between existing nodes creates an edge.
- Upsert same `(src, edge_type, dst)` updates weight and properties.
- Upsert edge with missing source fails without writing.
- Upsert edge with missing destination fails without writing.
- Get existing edge returns latest data.
- Get missing edge returns `None`.
- Delete existing edge removes forward row.
- Delete existing edge removes reverse index row.
- Delete missing edge returns no-op delete outcome.
- Multiple edge types between the same pair can coexist.
- Same edge type between different destinations can coexist.
- Same edge type from different sources to one destination can coexist.

### Neighbor Lookup

- Outgoing neighbors return destination nodes.
- Incoming neighbors return source nodes.
- Both direction returns outgoing plus incoming neighbors.
- Self-loop behavior is explicit and tested.
- Edge type filter includes only matching edges.
- Missing node returns empty page or stable not-found behavior according to the
  documented API.
- Cursor and limit paginate deterministic neighbor order.
- Updating an edge is reflected in outgoing and incoming neighbor reads.
- Deleting an edge is reflected in outgoing and incoming neighbor reads.
- Deleting a node removes the node from all neighbor results.

### Entity Binding Lookup

- Node with binding appears in `bindings_for_entity`.
- Multiple nodes can bind to the same entity.
- Bindings from different graphs in the same space are all returned.
- Bindings from different spaces do not leak.
- Bindings from different branches do not leak.
- Updating a node binding removes the old lookup and adds the new lookup.
- Deleting a node removes its binding lookup.
- Deleting a graph removes all binding lookups for that graph.
- Malformed binding targets cannot be written.

### Batch Write

- Empty batch returns an empty outcome and does not touch storage.
- Batch node upserts commit atomically.
- Batch node and edge upserts can create endpoints and edges in one commit.
- Batch rejects edge whose endpoint is neither pre-existing nor created earlier
  in the batch.
- Batch rejects invalid node ID without writing earlier valid operations.
- Batch rejects invalid edge type without writing earlier valid operations.
- Batch duplicate node upserts apply in documented order.
- Batch duplicate edge upserts apply in documented order.
- Batch node delete removes incident batch-created and pre-existing edges.
- Batch edge delete of missing edge returns no-op outcome.
- Batch failure leaves graph exactly as it was before the call.

### Branch Behavior

- Graph created on one branch is not visible on an unrelated branch.
- Forked branch sees graph metadata from parent.
- Forked branch sees parent nodes and edges.
- Child node write does not modify parent node.
- Child edge write does not modify parent edge.
- Parent write after fork does not unexpectedly appear over child-local writes.
- Deleting a node in child does not delete it in parent.
- Deleting a graph in child does not delete it in parent.
- Binding lookup follows branch visibility and branch-local writes.

### Space Behavior

- Same graph name can exist independently in two spaces.
- Nodes in one space are not visible in another space.
- Edges in one space are not visible in another space.
- Neighbor lookup stays within the selected space.
- Binding lookup stays within the selected space unless explicit cross-space
  binding is implemented and requested.
- Deleting graph in one space does not affect another space.

### Durable Reopen

- Created graph survives close and reopen.
- Nodes survive close and reopen.
- Edges survive close and reopen.
- Reverse edge index rows survive close and reopen.
- Binding index rows survive close and reopen.
- List pagination remains deterministic after reopen.
- Neighbor lookup remains correct after reopen.
- Delete outcomes remain durable after reopen.
- Recreate after durable delete does not observe stale rows.

### Temporal Behavior

If the engine has a standard public temporal read context when this slice is
implemented, add tests for:

- Node latest versus historical read after update.
- Node historical read after delete.
- Edge latest versus historical read after update.
- Edge historical read after delete.
- Neighbor historical read after edge update.
- Neighbor historical read after node delete.
- Binding lookup historical read if supported by the shared temporal API.

If no shared temporal API exists yet, do not add graph-only temporal commands.
Record this as deferred shared infrastructure, not a graph gap.

## Property And Differential Tests

### In-Memory Model Differential

Build a simple in-memory model with:

- graph set
- node map
- edge map keyed by `(src, edge_type, dst)`
- binding reverse map

Generate operation sequences:

- create graph
- delete graph
- upsert node
- delete node
- upsert edge
- delete edge
- list nodes
- neighbors
- binding lookup
- batch write

After each successful operation, compare engine reads with the model.

### Generated Edge Cases

- Random graph names at validation boundaries.
- Random node IDs at validation boundaries.
- Random edge types at validation boundaries.
- Dense node with many outgoing edges.
- Dense node with many incoming edges.
- Self-loops.
- Mixed edge types.
- Node delete in dense subgraph.
- Batch failure after many valid planned operations.

## Regression Tests

Add named regression tests for:

- stale reverse edge index after edge update
- stale reverse edge index after edge delete
- stale reverse edge index after node delete
- stale binding index after node binding update
- stale binding index after graph delete
- child branch mutating inherited graph rows
- duplicate edge upsert creating duplicate neighbor entries
- batch partially applying before validation failure

## Source And Dependency Guards

- `crates/engine-next/src/data/graph` must not depend on
  `strata_engine::graph`.
- `crates/engine-next` graph tests must use public engine APIs, not persistence
  row writes, except dedicated row-key/record unit tests.
- No executor graph command variants are added in this slice.
- No benchmark-only graph APIs are added.
- No ontology keys, types, or validation are introduced in the core graph
  module.
- No analytics algorithms are introduced in the core graph module.
- No branch DAG code is introduced in the user graph module.

## Gate Commands

Run the focused graph tests first:

```sh
cargo test -p strata-engine-next engine_graph
cargo test -p strata-engine-next dependency_guards
```

Then run the engine crate tests:

```sh
cargo test -p strata-engine-next --all-features
```

Before closing the slice, run workspace formatting and the normal feature-gated
checks used for the current branch.

## Done Criteria

- Every required cache and durable-local test passes.
- Durable reopen proves graph rows and secondary indexes survive restart.
- Batch failure tests prove all-or-nothing behavior.
- Branch and space isolation are covered.
- Source guards prove the implementation does not reuse old graph internals.
- No ontology, analytics, search boost, branch DAG, semantic merge, executor
  commands, or benchmark bypasses are included.
