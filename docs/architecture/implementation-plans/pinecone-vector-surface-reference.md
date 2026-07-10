# Pinecone Vector Surface Reference

## Purpose

Record Pinecone's public vector database surface area for future Strata vector
design work. This is a reference document, not a parity target. Strata should
use it to identify high-value vector workflows, vocabulary, and gaps while still
preserving Strata-specific semantics: local durability, cache mode, branch
isolation, time-travel reads, version history, and executor command contracts.

Checked against Pinecone public docs for API version `2025-04` on 2026-06-17.
Re-verify before making implementation commitments because Pinecone's API
surface evolves.

## High-Level Model

Pinecone is organized around:

- **Project**: cloud administrative boundary.
- **Index**: deployed vector search resource. It has a vector type, metric,
  dimension for dense vectors, readiness status, host, tags, deletion
  protection, and deployment spec.
- **Namespace**: partition inside an index for tenant/data separation.
- **Record/vector**: ID plus vector data and optional metadata.
- **Metadata**: flat JSON-like key-value fields used for filtering and returned
  fields.
- **Integrated embedding**: optional Pinecone-hosted model attached to an index
  so users can upsert/search text directly.
- **Backup/collection/import**: cloud operational surfaces for large data
  movement and recovery.

For Strata, the closest conceptual mapping is:

| Pinecone | Strata direction |
| --- | --- |
| Index | Vector collection, plus database-level deployment/configuration outside the primitive |
| Namespace | Product space |
| Record ID | Vector key |
| Dense vector values | Vector embedding |
| Sparse vector values | Deferred sparse-vector feature |
| Metadata | Vector metadata |
| Metadata filter | Vector-owned filter helper |
| Integrated embedding | Inference/executor layer, not core vector primitive |
| Backups/imports | Database/storage/executor operational layer |

## Control Plane Surface

### Index Lifecycle

Pinecone exposes index management separately from record operations:

- list indexes
- create index
- create index with integrated embedding
- describe index
- delete index
- configure index
- get index stats

Important index fields and choices:

- name
- vector type: dense or sparse
- dimension for dense indexes
- metric: examples include cosine, euclidean, and dot product
- deployment spec: serverless, BYOC, and pod-era options
- cloud and region for serverless indexes
- host used by data-plane operations
- readiness state
- deletion protection
- tags
- integrated embedding model and field map

Strata implication:

- The vector primitive should not copy Pinecone's cloud control plane.
- Collection create should cover the semantic subset: collection name,
  dimension, metric, and possibly vector type.
- Database open/configuration should own local durable/cache mode choices.
- Tags, deletion protection, cloud region, host, and readiness are not vector
  primitive concerns.

### Index Stats

Pinecone stats include:

- dimension
- metric
- vector type
- total vector count
- count per namespace
- index fullness for pod-based indexes
- optional metadata-filtered stats, with serverless limitations

Strata implication:

- Collection info/count should expose dimension, metric, and visible record
  count.
- Space-local counts are useful.
- Fullness is not relevant to local exact-search correctness.
- Metadata-filtered count can be considered after base count and filter
  semantics are stable.

### Namespaces

Pinecone namespace operations include:

- list namespaces, paginated and sorted
- describe namespace, including record count
- delete namespace

Behavioral notes:

- The default namespace is named `__default__`.
- Pinecone does not support renaming namespaces directly.
- Pinecone does not support moving records between namespaces directly.
- Namespace deletion is irreversible and deletes all data in that namespace.
- Some namespace APIs are serverless-only.

Strata implication:

- Product spaces already cover the partitioning role.
- Space deletion should remain a product/database operation, not a vector-only
  command, unless executor needs a convenience command.
- Vector tests should prove collection identity is `(branch, space,
  collection)`.

### Backups, Collections, And Restore

Pinecone exposes operational backup and restore APIs:

- create backup
- list backups for all indexes
- list backups for an index
- describe backup
- delete backup
- create index from backup
- list restore jobs
- describe restore job
- create/list/describe/delete collections for pod-based snapshot workflows
- backup schedules in newer/unstable surfaces

Strata implication:

- This belongs to storage/database operational tooling, not the vector primitive.
- Durable-local reopen tests should prove vector correctness without a backup
  surface.
- Future export/import/backup plans should include vector rows, collection
  config rows, tombstones, branches, and spaces.

### Imports

Pinecone recommends import for very large loads, especially around 10M+ records.
The import path uses object storage and Parquet files with namespace
directories. Dense-vector imports require ID, values, and optional metadata
columns.

Strata implication:

- Import is a valuable executor/CLI surface, but not part of the first vector
  primitive.
- For the primitive, batch upsert is enough to establish correctness.
- Later import should use public engine/executor APIs or a carefully designed
  bulk ingestion API, not lower-layer bypasses.

## Data Plane Surface

### Upsert Vectors

Pinecone `upsert` writes one or more vectors into a namespace. Each item has:

- id
- dense values and/or sparse values depending on index type
- optional metadata

Behavioral notes:

- Upserting an existing ID overwrites the previous value.
- Recommended batch limit is up to 1000 vectors.
- The response reports the upserted count.
- Pinecone recommends import instead of upsert for very large ingestion.

Strata mapping:

- `VectorUpsert`
- `VectorBatchUpsert`
- `upsert(collection, key, embedding, metadata)`
- `batch_upsert(collection, entries)`

First-slice Strata decisions:

- Dense vectors only.
- Validate all batch entries before commit.
- One valid batch should be one engine commit.
- Duplicate keys in a batch should have documented input-order behavior.
- Upsert replacement should increment vector revision and preserve history.

### Upsert Text

Pinecone `upsert_records` is available for indexes with integrated embedding.
The request includes records with `_id` and the configured text field. Pinecone
embeds the text with the model associated with the index and stores other fields
as metadata.

Strata mapping:

- Not a vector primitive concern.
- Belongs in inference/executor orchestration after vector semantics are stable.
- A future command could accept text, call embedding, and then call vector
  upsert through the normal command path.

### Fetch Vectors

Pinecone `fetch` returns vectors by IDs from one namespace. It can return vector
data and/or metadata. Pinecone also documents a metadata-based fetch workflow in
the manage-data guide.

Strata mapping:

- `VectorGet`
- `VectorBatchGet`
- `get(collection, key)`
- `get_versioned(collection, key)`
- `batch_get(collection, keys)`

First-slice Strata decisions:

- Batch get should be positional.
- Missing IDs return `None`.
- Latest reads should suppress tombstones.
- Versioned reads should expose commit version, timestamp, vector revision, and
  metadata.
- Metadata-only fetch can be deferred unless it becomes a performance need.

### Update Vector

Pinecone `update` can overwrite vector values and/or merge supplied
`set_metadata` fields into existing metadata.

Strata mapping:

- Full replacement is covered by upsert.
- Metadata patch is not covered by the first vector plan.

Future Strata decision:

- Either keep replacement-only semantics for simplicity, or add
  `update_metadata(collection, key, patch)` with clear merge/delete-field rules.
- Avoid introducing partial update behavior unless the JSON/metadata semantics
  are explicit and testable.

### Delete Vectors

Pinecone `delete` supports:

- delete by IDs
- delete all vectors in a namespace
- delete by metadata filter

Filter delete is mutually exclusive with ID delete and delete-all.

Strata mapping:

- `VectorDelete`
- `VectorBatchDelete`
- `delete(collection, key)`
- `batch_delete(collection, keys)`

Future Strata additions worth considering:

- `delete_by_filter(collection, filter)`
- `delete_all(collection)`

Implementation caution:

- Filter and delete-all operations are scan-and-delete workloads in the source
  row model.
- They should validate the filter first, then commit deterministic tombstones.
- They need branch/space isolation and recovery tests.

### List Vector IDs

Pinecone `list` returns vector IDs in one namespace, optionally constrained by a
prefix. Results are paginated and sorted with bitwise C collation. The default
page size is up to 100 IDs.

Strata mapping:

- Missing from the initial vector plan and should be added.

Recommended Strata API:

- `list_keys(collection, prefix, cursor, limit) -> VectorKeyPage`

Semantics to define:

- deterministic key ordering
- cursor starts strictly after the supplied cursor
- prefix filtering
- tombstone suppression
- branch/space isolation
- durable reopen behavior

### Search With Vector

Pinecone `query` searches a namespace with:

- vector values
- or an existing record ID as the query vector
- optional sparse vector input
- `top_k`
- metadata filter
- include values flag
- include metadata flag

The response includes ordered matches with IDs, scores, namespace, optional
values, optional metadata, and usage facts.

Strata mapping:

- `VectorQuery`
- `query(collection, query, k, filter)`
- `query_at(collection, query, k, filter, timestamp)`

First-slice Strata decisions:

- Query by dense vector only.
- Query by existing key can be deferred or added as a small convenience wrapper
  over `get + query`.
- Sparse query is deferred.
- Include-values/include-metadata flags can be deferred if typed outputs remain
  small. Returning metadata by default is acceptable for local correctness, but
  executor can later add projection controls.
- Exact search is the semantic baseline.

### Search With Text

Pinecone `search` supports text queries only for indexes with integrated
embedding. It can also search with vector or record ID and optionally rerank
results. It supports selecting returned fields.

Strata mapping:

- Not a vector primitive concern.
- Text search belongs in inference/search/executor orchestration.
- Rerank belongs in inference/executor orchestration.
- Field projection belongs in executor output shaping or a future search layer.

## Metadata Model And Filters

Pinecone metadata is a flat JSON document associated with a record. Nested
objects are not supported for filtering. Pinecone filter operators include:

- `$eq`
- `$ne`
- `$gt`
- `$gte`
- `$lt`
- `$lte`
- `$in`
- `$nin`
- `$exists`
- `$and`
- `$or`

Notes:

- Equality and comparisons operate on scalar metadata values.
- `$and` and `$or` are the top-level logical operators.
- `$in` and `$nin` accept bounded arrays.
- Metadata can be used in search, delete-by-filter, and some fetch workflows.

Strata first-slice recommendation:

- Start with a vector-owned filter model, not a general JSON query language.
- Support top-level scalar equality first.
- Add comparison and set-membership operators only after equality behavior,
  deletion, search, and count tests are stable.
- Avoid nested metadata paths in the vector primitive.

## Dense, Sparse, And Hybrid

Pinecone supports:

- dense vectors for semantic search
- sparse vectors for lexical/sparse retrieval
- single-index hybrid configurations in some API shapes
- separate dense and sparse index hybrid workflows with client-side merge
- document-schema indexes with full-text searchable fields in newer surfaces

Strata first-slice recommendation:

- Dense vectors only.
- Do not implement sparse or hybrid in the first correctness spine.
- Treat sparse, BM25/full-text, and hybrid result merging as search/retrieval
  layer work.

## Operational And Admin Surface

Pinecone also exposes surfaces that should not drive vector primitive design:

- API keys
- organizations/projects/service accounts
- billing/cost/read-unit/write-unit concerns
- region/cloud placement
- pod scaling and replicas
- BYOC
- deletion protection
- index tags
- backup schedules
- model catalog, embedding, and reranking APIs
- CLI and SDK-specific helpers

Strata may eventually need analogous operational capabilities, but they belong
outside the vector primitive.

## Comparison To Current Strata Plan

Already aligned:

- create/delete/list/count collection
- upsert and batch upsert
- get and batch get
- delete and batch delete
- list keys with prefix/cursor/limit
- metadata patch
- delete by filter
- delete all
- top-k vector query
- metadata filters, at least as a vector-owned concept
- branch/space isolation as a stronger form of namespace isolation
- durable reopen as a local source-of-truth guarantee

Strata already goes beyond Pinecone's core vector surface in:

- branch fork behavior
- time-travel reads
- timestamp search
- version history
- cache mode
- durable-local mode
- command boundary for local executor/SDK/CLI/MCP reuse

Potential gaps not yet in the vector plan:

1. query by existing key as a convenience command
2. projection flags for values and metadata if result size becomes an issue
3. collection/space stats with filter, after filters are stable

Deferred features:

- sparse vectors
- hybrid search
- integrated embedding
- reranking
- large object-storage import
- backup/restore API
- field projection on record search
- ANN index acceleration

## Suggested Strata Priority

For the first vector implementation, keep the existing plan:

1. collection config rows
2. vector source rows
3. dense upsert/get/list-keys/delete/batch
4. metadata patch
5. delete by filter and delete all
6. exact latest search
7. exact timestamp search
8. branch/space/reopen tests

After the first spine is correct, evaluate:

1. query by key
2. projection flags
3. filtered stats/count

Only after those core workflows are stable should sparse/hybrid/index
acceleration be considered.

## Source Links

- Pinecone create index:
  <https://docs.pinecone.io/reference/api/2025-04/control-plane/create_index>
- Pinecone manage indexes:
  <https://docs.pinecone.io/guides/manage-data/manage-indexes>
- Pinecone index stats:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/describeindexstats>
- Pinecone indexing overview:
  <https://docs.pinecone.io/guides/index-data/indexing-overview>
- Pinecone data modeling:
  <https://docs.pinecone.io/guides/index-data/data-modeling>
- Pinecone upsert vectors:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/upsert>
- Pinecone upsert text:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/upsert_records>
- Pinecone fetch vectors:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/fetch>
- Pinecone update vector:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/update>
- Pinecone delete vectors:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/delete>
- Pinecone list vector IDs:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/list>
- Pinecone search with vector:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/query>
- Pinecone search with text:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/search_records>
- Pinecone metadata filters:
  <https://docs.pinecone.io/guides/search/filter-by-metadata>
- Pinecone namespaces:
  <https://docs.pinecone.io/guides/manage-data/manage-namespaces>
- Pinecone list namespaces:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/listnamespaces>
- Pinecone describe namespace:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/describenamespace>
- Pinecone delete namespace:
  <https://docs.pinecone.io/reference/api/2025-04/data-plane/deletenamespace>
- Pinecone import:
  <https://docs.pinecone.io/guides/index-data/import-data>
- Pinecone backups:
  <https://docs.pinecone.io/guides/manage-data/backups-overview>
