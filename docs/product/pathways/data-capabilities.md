# Data Capability Pathways

Status: Draft pathway group

This document expands the V1 pathways for Strata's user-visible data
capabilities: KV, JSON, events, graph, vectors, and spaces.

## Pathway 7: Write And Read Key-Value Data

### Goal

A user stores and retrieves simple binary or string-like records.

### Flow

1. Select a database, branch, and space.
2. Put a key-value record.
3. Read the key back.
4. List or scan keys by prefix when needed.
5. Delete keys when they are no longer needed.

### Surface

KV SDK methods, CLI KV commands, serializable command boundary, branch and space
context, versioned output.

### Guarantees

KV writes must commit atomically, current reads must return the latest visible
value, historical reads must respect versions and timestamps, and list/scan
output must be deterministic enough for automation.

### Failures

Invalid key, missing branch or space, read-only access, write conflict, capacity
limit, serialization error, and history unavailable should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep KV as the simplest data capability and use it as a reference for output
metadata, temporal reads, and error behavior.

## Pathway 8: Write And Read JSON Documents

### Goal

A user stores structured documents and retrieves or filters document content
through product APIs.

### Flow

1. Select branch and space.
2. Create or update a JSON document or path.
3. Read a whole document or path.
4. List documents by prefix or context.
5. Delete document fields or documents when needed.

### Surface

JSON SDK methods, CLI JSON commands, JSON path syntax, branch and space context,
versioned output, import/export formats.

### Guarantees

JSON updates must preserve document validity, path operations must be
predictable, historical reads must return the document visible at the selected
point, and concurrent document updates must not silently overwrite incompatible
state.

### Failures

Invalid JSON, invalid path, missing document, type mismatch, write conflict,
read-only access, and history unavailable should surface as user-facing errors.

### V1 Decision

Required.

### Cleanup

Keep JSON as a first-class data capability. Align JSON history and `as_of`
metadata with KV rather than making JSON a special case.

## Pathway 9: Append And Query Events

### Goal

A user records ordered event data and reads it back for history, audit, or
timeline workflows.

### Flow

1. Select branch and space.
2. Append an event with type, payload, and timestamp.
3. Read events by sequence, type, range, or time window.
4. Inspect counts and event types.
5. Use events as audit or retrieval context.

### Surface

Event SDK methods, CLI event commands, event sequence numbers, event type
filters, time range filters, branch and space context.

### Guarantees

Events must be append-oriented, ordered, durable in durable mode, and queryable
by sequence and time. Event timestamp semantics must be explicit and consistent
with time-travel behavior.

### Failures

Invalid event type, invalid payload, missing sequence, malformed range,
read-only access, branch missing, and unsupported temporal query should surface
clearly.

### V1 Decision

Required.

### Cleanup

Keep events as append-oriented data. Clarify whether event time means occurrence
time, commit time, or both before expanding temporal event features.

## Pathway 10: Create And Manage Graphs

### Goal

A user creates, deletes, lists, and inspects named graphs in a branch and space.

### Flow

1. Select branch and space.
2. Create or reference a named graph.
3. Add graph data through nodes, edges, or relationship-layer operations.
4. List or inspect graphs.
5. Delete graph data when intended.

### Surface

Graph SDK methods, CLI graph commands, graph names, branch and space context,
graph metadata.

### Guarantees

Graph names must be scoped predictably, graph data must be branch-local, graph
operations must preserve storage consistency, and graph deletion must not
silently delete unrelated records.

### Failures

Invalid graph name, missing graph, duplicate graph, read-only access, malformed
graph data, and branch or space errors should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep graph as a standalone capability while also promoting the relationship
layer direction.

## Pathway 11: Model Graph Entities And Relationships

### Goal

A user stores graph-native nodes and edges or connects KV, JSON, event, vector,
and graph records through entity references without duplicating source payloads.

### Flow

1. Select a graph in a branch and space.
2. Create native nodes or bound nodes that reference existing Strata records.
3. Add typed relationships between nodes.
4. Traverse or query relationships.
5. Resolve bound nodes back to their source records when needed.

### Surface

Graph node and edge APIs, relationship-layer entity references, graph traversal,
search/RAG integration, graph docs.

### Guarantees

Relationship modeling must not require payload duplication. Entity references
must be typed enough to resolve safely, respect branch and space context, and
avoid confusing user-authored graph facts with derived graph facts.

### Failures

Invalid entity reference, missing source record, dangling relationship,
ontology mismatch, branch mismatch, and unsupported temporal reference should
surface clearly.

### V1 Decision

Required.

### Cleanup

Tighten entity reference semantics and make relationship-layer behavior a
documented product contract, not an incidental graph field.

## Pathway 12: Define Graph Ontology

### Goal

A user defines object and link types, freezes ontology metadata, inspects
status, and lists nodes by type.

### Flow

1. Select a graph.
2. Define object types and link types.
3. Attach types to graph nodes and edges.
4. Freeze or inspect ontology state where supported.
5. Use ontology metadata for validation, browsing, search, and analytics.

### Surface

Graph ontology APIs, CLI graph ontology commands, graph metadata, branch and
space context.

### Guarantees

Ontology metadata must be branch-local, versioned with graph state, and applied
predictably. Freezing must have clear consequences.

### Failures

Duplicate type, invalid type definition, frozen ontology mutation, missing type,
incompatible edge type, and historical ontology mismatch should surface
clearly.

### V1 Decision

Required.

### Cleanup

Keep ontology, but define its product semantics before relying on it for
relationship-layer validation or temporal graph behavior.

## Pathway 13: Traverse And Query Graph Neighborhoods

### Goal

A user queries neighbors or runs bounded traversal with direction, edge-type,
branch, space, and time-travel context.

### Flow

1. Select graph, branch, space, and optional temporal point.
2. Choose a starting node or entity reference.
3. Choose direction, edge type, and traversal bound.
4. Run neighbors or traversal query.
5. Inspect connected nodes, relationships, and source entity references.

### Surface

Graph traversal APIs, CLI graph query commands, temporal context, relationship
layer, search expansion.

### Guarantees

Traversal must honor branch, space, graph, direction, edge filters, and
supported temporal context. Output must be bounded and deterministic enough for
applications.

### Failures

Missing graph, missing start node, invalid traversal bound, unsupported temporal
mode, ontology mismatch, and result-size limits should surface clearly.

### V1 Decision

Required.

### Cleanup

Extend existing graph point-in-time node and neighbor support into a consistent
temporal traversal story.

## Pathway 14: Run Graph Analytics

### Goal

A user runs connected-components, community detection, PageRank, clustering, or
shortest-path analytics on graph data.

### Flow

1. Select graph, branch, space, and optional filters.
2. Choose an analytics algorithm.
3. Run the algorithm with bounded parameters.
4. Receive results with graph node and entity-reference context.
5. Store or export results only when explicitly requested.

### Surface

Graph analytics APIs, CLI graph analytics commands, algorithm options, graph
docs, metrics.

### Guarantees

Analytics must be bounded, deterministic enough for repeatable use, explicit
about whether they run on current or temporal graph state, and honest about
algorithm limits.

### Failures

Unsupported algorithm, graph too large, invalid parameters, missing graph,
unsupported temporal state, and timeout should surface clearly.

### V1 Decision

Optional.

### Cleanup

Keep graph analytics only if they do not distort the core graph architecture.
Do not let analytics block the relationship-layer contract.

## Pathway 15: Store And Query Vectors

### Goal

A user stores embeddings or numeric vectors and performs similarity search.

### Flow

1. Create or select a vector collection.
2. Insert vectors with keys, embeddings, and optional metadata.
3. Query by vector or generated embedding.
4. Filter or inspect matches.
5. Update or delete vectors when needed.

### Surface

Vector SDK methods, CLI vector commands, vector collection config, metadata
filters, search integration, temporal query support.

### Guarantees

Vector dimensions must be enforced, query results must be deterministic enough
for users to trust, metadata must remain attached to the visible vector version,
and temporal search behavior must be explicit.

### Failures

Dimension mismatch, missing collection, invalid vector value, NaN or infinity,
metadata filter error, read-only access, unsupported index state, and history
unavailable should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep vectors as a standalone capability. Preserve support for user-supplied
embeddings separate from auto-embedding shadow data.

## Pathway 31: Organize Data With Spaces

### Goal

A user separates logical namespaces while preserving branch, temporal, and
command context.

### Flow

1. Choose or create a space.
2. Run data commands inside that space.
3. Switch context or override space per command.
4. Compare, export, search, or inspect data by space.
5. Preserve space semantics through branch and clone workflows.

### Surface

Space APIs, CLI context commands, branch-aware data commands, search filters,
export/import filters.

### Guarantees

Spaces must isolate logical user data within a branch, participate in branch
operations, and avoid leaking system-space implementation details into ordinary
user output.

### Failures

Invalid space name, missing space where explicit existence is required,
reserved system space, branch mismatch, and unsupported cross-space operation
should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep spaces as a first-class namespace concept. Make branch, space, and
temporal context flow consistently through CLI, SDK, and command protocols.
