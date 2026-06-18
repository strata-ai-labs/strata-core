# Engine Vector Parity Implementation Plan

## Problem

The rebuilt engine has a branch, KV, and JSON spine. Vector should now follow
the same structure: engine-owned product semantics, storage-owned durability,
and executor commands acting as a stateless serialized boundary. The old vector
engine contains useful behavior and invariants, but it also mixes durable facts
with index/cache machinery. The first rebuilt vector slice should port the
contract, not the incidental implementation shape.

The goal is an end-to-end vector primitive that can create collections, write
vectors, read vectors, delete vectors, list/count collections, run exact nearest
neighbor search, and preserve branch/space/time behavior in cache and
durable-local modes. Approximate indexes, quantization, mmap sidecars, hybrid
retrieval, auto-embedding, and export/import are later layers.

## Old Evidence

- `crates/engine/src/semantics/vector.rs`
- `crates/engine/src/vector/collection.rs`
- `crates/engine/src/vector/distance.rs`
- `crates/engine/src/vector/filter.rs`
- `crates/engine/src/vector/store/collections.rs`
- `crates/engine/src/vector/store/crud.rs`
- `crates/engine/src/vector/store/search.rs`
- `crates/engine/src/vector/types.rs`
- `crates/executor/src/command.rs`
- `crates/executor/src/handlers/vector.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`

## Current Targets

- `crates/engine-next/src/api/`
- `crates/engine-next/src/data/vector/`
- `crates/engine-next/src/persistence/`
- `crates/engine-next/tests/`
- later executor slice:
  - `crates/executor-next/src/`
  - `crates/executor-next/tests/`

## Current Status

Already present in the rebuilt engine:

- cache and durable-local database open
- branch create/list/lookup/delete/fork
- KV and JSON public services
- branch/space validation patterns
- persistence adapter and row-class structure
- read version, timestamp, history, batch, and source-guard patterns

Missing for vector:

- vector public API module and crate-root re-exports
- vector product type wrappers and validation
- collection config row class and key encoding
- vector entry row class and key encoding
- vector record envelope
- exact distance implementation owned by vector service
- create/delete/list/count collection behavior
- upsert/get/get-at/history/delete behavior
- batch upsert/get/delete behavior
- latest and timestamp exact nearest-neighbor query
- metadata filter evaluation
- branch/space isolation tests
- durable reopen tests
- source/dependency guards

## Non-Goals

This slice must not implement:

- HNSW, IVF, quantization, mmap graph files, segmented indexes, or sidecar
  vector caches
- background vector index maintenance
- source references for hybrid retrieval
- auto-embedding or model integration
- graph/search coupling
- Arrow import/export
- executor command variants, unless the engine slice is already complete and
  the user explicitly asks to continue into the executor slice
- benchmark-specific APIs or storage bypasses

## Design Decisions

1. **Vector follows the KV and JSON primitive shape.** Public API types live
   under `api`; validated product types and behavior live under `data/vector`;
   persistence translation lives under `persistence`; storage request types do
   not cross the public engine API.

2. **Collection config and vector rows are the source of truth.** Search
   indexes, heaps, sidecar files, and memory caches are derived state. The first
   implementation should not need any derived state to answer reads or exact
   search.

3. **Exact search is the semantic baseline.** Nearest-neighbor search scans
   visible vector rows, computes scores deterministically, sorts by score
   descending and key ascending, and returns the first `k` matches. Future
   approximate indexes must prove equivalence against this baseline for the
   covered cases.

4. **Scores are always higher-is-better.** The public score contract is:
   cosine similarity in `[-1, 1]`, Euclidean similarity as
   `1 / (1 + l2_distance)`, and dot product as the raw dot product.

5. **No implicit normalization.** Vectors are stored and scored exactly as
   provided. Cosine handles zero-norm inputs by returning `0.0` for that pair.

6. **Collection config is immutable.** Dimension and metric are fixed after
   collection creation. A later alter operation can be designed separately if
   needed, but this slice should reject conflicting duplicate creates.

7. **Vector entry version and commit metadata are separate.** Storage commit
   version and timestamp identify the durable row version. Product vector
   revision increments per successful upsert for a key. Public outcomes should
   expose both where useful.

8. **Batch writes are one engine commit when valid.** Batch upsert and batch
   delete validate all inputs first, then commit all row changes together. Empty
   batches return empty outcomes without touching storage.

9. **Deletes write tombstones.** Latest reads and latest search suppress
   tombstoned vectors. History should preserve enough tombstone facts to prove
   deletion behavior, even if executor compatibility later chooses a values-only
   output shape.

10. **Metadata filtering is a vector-owned post-filter.** The first slice should
    support top-level scalar predicates with AND semantics. It should not become
    a general JSON query language.

11. **Branch and space are part of collection identity.** Two collections with
    the same name in different branches or spaces are independent. Forked branch
    reads must see inherited rows through storage branch visibility; later
    writes must be branch-local.

12. **Executor remains a delegator.** The future executor vector command slice
    should only deserialize commands, apply defaults, call engine APIs, and shape
    outputs. It must not compute distance, filter metadata, update collection
    counts, or inspect storage keys.

## Shared Primitive Structure Target

Vector should match the established primitive layout:

- `api/vector.rs` re-exports public service and outcome types.
- `data/vector/types.rs` owns validated collection names, vector keys,
  embeddings, distance metrics, metadata filters, and config wrappers.
- `data/vector/outcome.rs` owns collection info, vector read, history, search,
  and batch outcomes.
- `data/vector/record.rs` owns internal record envelopes.
- `data/vector/distance.rs` owns exact distance and score functions.
- `data/vector/service.rs` owns product operations, branch/space semantics, and
  search behavior.
- `persistence/key.rs` owns vector row-key encoding and decoding helpers.
- `persistence/row.rs` and `persistence/space.rs` own row-class assignment.
- Tests are grouped by public behavior, persistence translation, historical
  behavior, search correctness, and source guards.

## Public Engine API Target

Add public vector types:

- `VectorCollectionName`
- `VectorKey`
- `VectorEmbedding`
- `VectorMetadata`
- `VectorDistanceMetric`
- `VectorStorageDtype` only if it remains durable product metadata; otherwise
  defer it.
- `VectorConfig`
- `VectorFilter`
- `VectorFilterCondition`
- `VectorFilterOp`
- `VectorScalar`
- `VectorEntry`
- `VectorVersionedEntry`
- `VectorHistory`
- `VectorHistoryRow`
- `VectorSearchMatch`
- `VectorSearchResult`
- `VectorKeyPage`
- `VectorCollectionInfo`
- `VectorWriteOutcome`
- `VectorMetadataPatch`
- `VectorMetadataUpdateOutcome`
- `VectorDeleteOutcome`
- `VectorBulkDeleteOutcome`
- `VectorBatchUpsertOutcome`
- `VectorBatchGetOutcome`
- `VectorBatchDeleteOutcome`

Add `Database::vector(branch, space) -> EngineResult<VectorService>`.

Add `VectorService` methods:

- `create_collection(name, config) -> VectorCollectionInfo`
- `delete_collection(name) -> bool`
- `list_collections() -> Vec<VectorCollectionInfo>`
- `collection_info(name) -> Option<VectorCollectionInfo>`
- `count(name) -> u64`
- `upsert(collection, key, embedding, metadata) -> VectorWriteOutcome`
- `get(collection, key) -> Option<VectorEntry>`
- `get_versioned(collection, key) -> Option<VectorVersionedEntry>`
- `get_at(collection, key, timestamp) -> Option<VectorVersionedEntry>`
- `history(collection, key) -> Option<VectorHistory>`
- `exists(collection, key) -> bool`
- `list_keys(collection, prefix, cursor, limit) -> VectorKeyPage`
- `update_metadata(collection, key, patch) -> VectorMetadataUpdateOutcome`
- `delete(collection, key) -> VectorDeleteOutcome`
- `delete_by_filter(collection, filter) -> VectorBulkDeleteOutcome`
- `delete_all(collection) -> VectorBulkDeleteOutcome`
- `batch_upsert(collection, entries) -> VectorBatchUpsertOutcome`
- `batch_get(collection, keys) -> VectorBatchGetOutcome`
- `batch_delete(collection, keys) -> VectorBatchDeleteOutcome`
- `query(collection, query, k, filter) -> VectorSearchResult`
- `query_at(collection, query, k, filter, timestamp) -> VectorSearchResult`

Do not expose storage keys, row classes, storage read sets, or internal record
bytes through this API.

## Storage Shape

Add two durable row families:

```text
vector collection config:
  version byte | vector collection discriminator | space length | space bytes |
  collection length | collection bytes

vector entry:
  version byte | vector entry discriminator | space length | space bytes |
  collection length | collection bytes | key length | key bytes
```

Use length-delimited fields even if collection validation rejects separators.
Vector keys may contain slashes, spaces, and other ordinary UTF-8; the key
encoding must not rely on textual separators.

The storage branch namespace remains outside the product row key, as it does
for KV and JSON.

## Collection Config Envelope

The collection config row stores:

- format version
- collection name
- dimension
- distance metric
- optional storage dtype only if the rebuilt engine needs it as product metadata
- created timestamp if already available from commit metadata, otherwise omit it

Do not store mutable count in the config envelope in this slice. Count can be
derived from visible vector entry rows. A future counter row can be added only
after correctness is established and tests prove it is transactionally updated.

## Vector Record Envelope

The vector entry row stores:

- format version
- collection name
- key
- vector revision
- embedding values as finite `f32`
- optional JSON metadata

Commit version and timestamp come from storage row metadata, not the envelope.
Source references are deferred. If source references are added later, they must
remain optional metadata on the vector record and must not affect core vector
search semantics.

## Validation Rules

Collection name:

- non-empty
- at most 256 bytes
- no null bytes
- no `/` if the public contract keeps old compatibility
- no leading `_` for ordinary user collections

Vector key:

- empty string is allowed for old compatibility unless a product-wide key rule
  supersedes it
- at most 1024 bytes
- no null bytes

Embedding:

- dimension must match collection config
- non-empty collection dimensions only
- no NaN or infinite values
- max dimension and max serialized byte size should use engine limits

Metadata:

- optional JSON value
- max serialized byte size should use engine limits
- filters apply only to top-level scalar fields in this slice

Metadata patch:

- patch input is a top-level JSON object
- patch keys are ordinary metadata field names, not paths
- each supplied field overwrites or adds that top-level metadata field
- unspecified fields are preserved
- JSON null is stored as a value, not treated as field removal
- missing metadata is treated as an empty object
- existing non-object metadata rejects patch unless the implementation first
  documents a replacement rule

Key listing:

- optional prefix filters visible vector keys
- cursor starts strictly after the supplied key
- ordering is deterministic byte/string ordering matching the vector row-key
  order
- tombstones are suppressed
- missing collection is a stable not-found error

Bulk delete:

- `delete_by_filter` uses the same vector-owned metadata filter as search
- `delete_by_filter` rejects an empty filter
- `delete_all` is the explicit operation for deleting every visible vector in a
  collection
- both operations validate first, then write all tombstones in one commit
- outcomes include deleted count and, if exposed, the commit version/timestamp

## Exact Search Semantics

For latest search:

1. Load the collection config.
2. Validate query vector and dimension.
3. Scan visible vector entry rows for the branch/space/collection.
4. Suppress tombstones and corrupt rows according to engine error policy.
5. Apply metadata filter if present.
6. Compute score using the collection metric unless an explicit metric override
   is intentionally added to the public engine API.
7. Sort by score descending, then key ascending.
8. Return at most `k` matches.

For timestamp search:

1. Use the same rules as latest search.
2. Read visible vector entries as of the timestamp.
3. Suppress entries not alive at the timestamp.
4. Return scores based on the historical embedding and historical metadata.

`k == 0` returns an empty result. Missing collection is a stable not-found
error. Empty collection returns an empty result.

## Error Mapping

Add engine-owned vector error cases or reuse existing engine error categories
with vector-specific context:

- invalid collection name
- collection already exists
- collection not found
- invalid vector key
- invalid embedding
- dimension mismatch
- invalid metadata filter
- invalid metadata patch
- empty filter for filtered delete
- corrupt collection config row
- corrupt vector record row
- missing branch
- invalid space
- closed database handle

Errors exposed through executor later must be stable and public. Internal row
details, storage keys, and lower-layer error strings should not leak.

## Implementation Order

### 1. Type Skeleton

- Add `data/vector` module skeleton.
- Add `api/vector.rs`.
- Add crate-root re-exports.
- Add public wrappers for collection name, key, embedding, metric, config,
  metadata filters, and scalar filter values.
- Add outcome structs with private fields and accessors.
- Add unit tests for validation.

### 2. Persistence Key Encoding

- Add vector collection and vector entry key encode/decode helpers.
- Add row-class entries for vector config and vector rows.
- Add malformed-key tests.
- Add guards that vector service does not construct storage requests directly.

### 3. Record Envelopes

- Add collection config encode/decode.
- Add vector record encode/decode.
- Add deterministic fixtures.
- Reject unknown format versions, truncated payloads, mismatched names, corrupt
  metadata, and non-finite embeddings.

### 4. Collection Operations

- Implement `Database::vector`.
- Implement create, delete, list, info, and count.
- Ensure duplicate create, missing delete, and missing stats behavior is
  documented and stable.
- Derive count from visible entry rows.

### 5. Basic Vector CRUD

- Implement upsert, get, get-versioned, exists, list-keys, delete, metadata
  patch, and history.
- Upsert creates or replaces the visible value for the key and increments vector
  revision.
- List-keys scans visible source rows and returns a deterministic key page.
- Metadata patch updates only metadata, preserves embedding, and increments
  vector revision once.
- Delete writes a tombstone and returns whether a visible row existed.
- History returns newest-first rows with enough facts to distinguish value rows
  from tombstones.

### 6. Batch Operations

- Implement batch upsert, batch get, and batch delete.
- Validate all batch entries before write commit.
- Keep outputs positional.
- Define duplicate-key behavior explicitly:
  - batch upsert applies entries in input order inside one commit, so the last
    duplicate key is the latest visible value after the batch;
  - batch get preserves duplicate reads;
  - batch delete preserves duplicate outcomes, with later duplicate deletes
    reporting not deleted.

### 7. Exact Search

- Implement metric score functions.
- Implement latest exact search.
- Implement timestamp exact search.
- Implement metadata filtering as a vector-owned helper.
- Add deterministic tie-breaking by key.

### 8. Filtered And Bulk Deletes

- Implement `delete_by_filter`.
- Implement `delete_all`.
- Reuse the vector-owned metadata filter helper.
- Reject empty filters for filtered delete so full collection deletion remains
  explicit.
- Commit tombstones for every matched visible vector in one valid operation.
- Ensure filtered delete and delete-all update list, count, get, history, and
  exact search visibility consistently.

### 9. Branch, Space, And Reopen

- Prove collection and vector rows isolate by branch and space.
- Prove branch fork reads inherited vector rows and later writes are branch-local.
- Prove durable-local reopen restores collection config, entries, deletes,
  history, and exact search from storage rows only.

### 10. Source And Dependency Guards

- Guard that engine vector modules do not depend on old engine vector modules.
- Guard that vector service does not import executor types.
- Guard that persistence does not import vector command/output types.
- Guard that exact search does not use benchmark-only APIs or lower-layer
  bypasses.

### 11. Executor Slice Hand-Off

After the engine tests pass, write or execute the executor vector command
contract plan. The executor surface should include the old command families:

- create/delete/list/stats/count collection
- upsert/get/get-versioned/exists/list-keys/delete
- metadata patch
- filtered delete/delete-all
- query
- batch upsert/get/delete

The executor should map those commands onto the engine API without owning vector
semantics.

## Completion Criteria

- Cache and durable-local engine tests pass for vector collection, CRUD,
  list-keys, metadata patch, batch, filtered delete, delete-all, history,
  branch/space, exact search, and reopen behavior.
- Exact search has deterministic order and documented scores for all metrics.
- Durable-local reopen does not require sidecar files, search indexes, or
  memory-only state to recover vector rows.
- No benchmark-only or lower-layer storage bypass is used by public vector APIs.
- Executor vector implementation can be layered on top without needing new
  engine semantics.
