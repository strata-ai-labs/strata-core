# Engine Vector Parity Test Plan

## Purpose

Prove that the rebuilt engine owns vector semantics with the same structure used
for KV and JSON: public API types at the engine boundary, product behavior in
the vector service, and storage translation isolated in persistence. The first
slice proves exact nearest-neighbor behavior and durable source-of-truth rows.
Approximate indexes and derived caches are intentionally out of scope.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Type validation | Required | Required |
| Row-key encoding and decoding | Required | Required |
| Collection config envelope | Required | Required |
| Vector record envelope | Required | Required |
| Collection create/delete/list/count | Required | Required |
| Upsert/get/exists/delete | Required | Required |
| Key listing | Required | Required |
| Metadata patch | Required | Required |
| Batch upsert/get/delete | Required | Required |
| Filtered delete and delete-all | Required | Required |
| Exact latest search | Required | Required |
| Exact timestamp search | Required | Required |
| Metadata filters | Required | Required |
| Branch isolation and fork behavior | Required | Required |
| Space isolation | Required | Required |
| Version history | Required | Required |
| Reopen persistence | Not applicable | Required |
| Source and dependency guards | Required | Required |

## Unit Tests

### Collection Name Validation

- Accept ordinary ASCII names.
- Accept ordinary UTF-8 names if product-wide naming rules allow them.
- Accept maximum-length names.
- Reject empty names.
- Reject names longer than the configured limit.
- Reject null bytes.
- Reject `/` if the compatibility rule is retained.
- Reject leading `_` for user collections.
- Error class is stable and vector-owned.

### Vector Key Validation

- Accept ordinary keys.
- Accept keys containing `/`.
- Accept empty key if the old-compatible rule is retained.
- Accept maximum-length key.
- Reject keys longer than the configured limit.
- Reject null bytes.
- Preserve deterministic ordering for keys inside one collection.

### Embedding Validation

- Accept finite `f32` values.
- Accept zero values.
- Reject empty embedding on upsert.
- Reject NaN.
- Reject positive infinity.
- Reject negative infinity.
- Reject vectors above the configured dimension or serialized-size limit.
- Reject dimension mismatch against collection config.
- Error includes expected and actual dimension without leaking storage details.

### Distance Metrics

- Cosine returns `1.0` for identical non-zero vectors.
- Cosine returns `0.0` when either vector has zero norm.
- Cosine returns lower score for orthogonal vectors than identical vectors.
- Euclidean returns `1.0` for identical vectors.
- Euclidean score decreases as distance grows.
- Dot product returns the raw dot product.
- All metric scores are higher-is-better.
- Dimension mismatch in score helpers fails or panics only in private test-only
  helpers; public APIs return a typed engine error.

### Metadata Filter Validation

- Accept empty filter.
- Accept top-level scalar equality filters.
- Accept supported comparison operators if implemented in this slice.
- Reject nested-path filters unless explicitly supported.
- Reject object and array filter values.
- Reject malformed field names according to product limits.
- Empty filter matches vectors with no metadata.
- Non-empty filter does not match missing metadata.

### Metadata Patch Validation

- Accept a top-level object patch.
- Reject non-object patch values.
- Reject nested path syntax in patch field names unless explicitly supported.
- Reject field names above the configured limit.
- Reject patch payloads above the configured metadata size limit.
- Empty patch follows the documented no-op or invalid-input behavior.
- JSON null in a patch is stored as a value, not treated as field removal.

### Row-Key Encoding

- Encodes collection config rows deterministically.
- Encodes vector entry rows deterministically.
- Decodes collection names and vector keys containing separators.
- Decodes maximum-length fields.
- Rejects unknown key version.
- Rejects unknown discriminator.
- Rejects truncated length fields.
- Rejects truncated field bytes.
- Rejects empty decoded collection name.
- Rejects invalid decoded vector key.
- Does not decode KV, JSON, or control rows as vector rows.

### Collection Config Envelope

- Encodes a fixture config deterministically.
- Decodes the fixture config.
- Preserves dimension and metric.
- Rejects unknown format version.
- Rejects unknown metric discriminator.
- Rejects zero dimension.
- Rejects mismatched collection name.
- Rejects truncated payload.
- Does not include mutable count in the envelope.

### Vector Record Envelope

- Encodes a fixture vector record deterministically.
- Decodes the fixture vector record.
- Preserves collection, key, vector revision, embedding, and metadata.
- Rejects unknown format version.
- Rejects collection mismatch.
- Rejects key mismatch.
- Rejects truncated embedding bytes.
- Rejects corrupt metadata.
- Rejects NaN or infinite decoded embeddings.

### Outcome Types

- Collection outcomes expose name, dimension, metric, count, commit version, and
  timestamp where applicable.
- Vector read outcomes expose key, embedding, metadata, vector revision, commit
  version, and timestamp.
- Key-page outcomes expose keys, cursor, and `has_more`.
- Delete outcomes expose key and deleted flag.
- Bulk-delete outcomes expose deleted count and commit facts when rows are
  deleted.
- Metadata-update outcomes expose key, updated flag, vector revision, commit
  version, and timestamp when updated.
- Search outcomes expose matches, score, metadata, commit facts if included, and
  deterministic order.
- Batch outcomes preserve positional mapping.
- Fields remain private with public accessors.
- Outcome structs do not expose storage request or row-key types.

## Engine Behavior Tests

Run each behavior test against both cache and durable-local fixtures unless the
test is specifically about reopen.

### Create Collection

- Create a collection with cosine metric.
- Create a collection with Euclidean metric.
- Create a collection with dot-product metric.
- Read collection info after create.
- List collections returns the created collection.
- Count for a new collection is zero.
- Duplicate create with the same config returns stable conflict behavior.
- Duplicate create with a different dimension returns stable conflict behavior.
- Duplicate create with a different metric returns stable conflict behavior.
- Creating a collection in a missing branch fails.
- Creating a collection in an invalid space fails.

### Delete Collection

- Delete an existing empty collection.
- Delete an existing non-empty collection.
- Deleted collection disappears from list.
- Deleted collection count is unavailable or not found according to the
  documented API.
- Vectors in a deleted collection are no longer readable.
- Search in a deleted collection fails with collection not found.
- Deleting a missing collection returns false or not found according to the
  documented API.
- Deleting a collection in one space does not affect another space.
- Deleting a collection in one branch does not affect another branch.

### List Collections

- List empty branch/space returns empty list.
- List multiple collections in deterministic order.
- List excludes internal collections if any internal names exist.
- Counts reflect visible vector rows.
- Counts exclude tombstones.
- Counts update after upsert, overwrite, delete, and collection delete.

### Upsert

- Upsert into an existing collection creates a visible vector.
- Upsert returns commit version and timestamp.
- Upsert stores metadata.
- Upsert without metadata stores no metadata.
- Upsert same key replaces embedding and metadata.
- Upsert same key increments vector revision.
- Upsert same key does not increment collection count.
- Upsert wrong dimension fails without writing.
- Upsert into missing collection fails.
- Upsert into missing branch fails.
- Upsert after collection delete fails.

### Get

- Get an existing vector returns embedding and metadata.
- Get a missing key returns `None`.
- Get after overwrite returns the latest embedding and metadata.
- Get after delete returns `None`.
- Get in missing collection fails.
- Get in missing branch fails.
- Get includes vector revision, commit version, and timestamp in versioned form.

### Exists

- Missing key returns false.
- Created key returns true.
- Overwritten key remains true.
- Deleted key returns false.
- Missing collection fails or returns false according to the documented API.

### List Keys

- Empty collection returns an empty page.
- List all keys in deterministic order.
- Prefix filters keys.
- Cursor starts strictly after the cursor key.
- `has_more` and next cursor facts are correct.
- `limit == 0` follows the documented empty-page or invalid-input behavior.
- Deleted keys are suppressed.
- Overwritten keys appear once.
- Duplicate-looking prefixes do not leak keys from another collection.
- Missing collection fails.

### Update Metadata

- Patch existing vector metadata by adding a new field.
- Patch existing vector metadata by replacing an existing field.
- Patch preserves unspecified fields.
- Patch preserves embedding.
- Patch increments vector revision once.
- Patch returns commit version and timestamp.
- Patch missing metadata materializes an object metadata value.
- Patch existing non-object metadata fails unless the implementation documents a
  replacement rule.
- Patch missing key returns not found or `updated=false` according to the
  documented API.
- Patch missing collection fails.
- Patch failure leaves the original vector unchanged.

### Delete Vector

- Delete existing key returns `deleted=true`.
- Deleted key is absent from latest get.
- Deleted key is absent from latest search.
- Delete missing key returns `deleted=false`.
- Delete same key twice returns true then false.
- Delete wrong collection fails.
- Delete writes a tombstone and preserves historical visibility.
- Delete does not remove the collection config.
- Delete decrements derived count exactly once.

### Batch Upsert

- Empty batch returns an empty outcome and does not touch storage.
- Batch upsert multiple keys in one collection.
- Batch upsert returns positional outcomes.
- Batch upsert writes all valid entries in one commit.
- Batch upsert duplicate keys applies entries in input order.
- Last duplicate key is visible after the batch.
- Duplicate-key vector revision increments according to documented semantics.
- One invalid key fails the whole batch.
- One invalid embedding fails the whole batch.
- One dimension mismatch fails the whole batch.
- Failed batch leaves no partial writes.

### Batch Get

- Empty batch returns empty result.
- Batch get existing keys.
- Batch get mixed existing and missing keys.
- Results are positional.
- Duplicate reads are preserved.
- Batch get after overwrites returns latest values.
- Batch get after deletes returns `None` for deleted keys.
- Missing collection fails.

### Batch Delete

- Empty batch returns empty result.
- Batch delete multiple existing keys.
- Batch delete mixed existing and missing keys.
- Results are positional.
- Duplicate deletes return true for the first visible delete and false for later
  duplicates if duplicates are processed in input order.
- Batch delete removes keys from latest search.
- Batch delete writes one commit when valid.
- One invalid key fails the whole batch.
- Failed batch leaves no partial tombstones.

### Delete By Filter

- Equality filter deletes matching visible vectors.
- Non-matching vectors remain visible.
- Missing metadata does not match non-empty filter.
- Multiple filter conditions use AND semantics.
- Empty filter is rejected.
- Filtered delete writes all matched tombstones in one commit.
- Outcome deleted count matches the number of newly tombstoned vectors.
- Re-running the same filtered delete reports zero deleted rows.
- Deleted vectors are absent from latest get, list-keys, count, and search.
- Deleted vectors remain represented in history.
- Invalid filter leaves all vectors unchanged.
- Missing collection fails.

### Delete All

- Delete all in an empty collection returns zero deleted rows.
- Delete all in a non-empty collection tombstones every visible vector.
- Delete all is idempotent.
- Collection config remains present after delete-all.
- Count becomes zero.
- List keys returns empty.
- Latest search returns empty.
- History for deleted keys remains available.
- Delete all in one collection does not affect another collection.

## Exact Search Tests

### Latest Search

- Search empty collection returns empty matches.
- `k == 0` returns empty matches.
- `k` larger than collection size returns all visible vectors.
- Search validates query dimension.
- Search rejects NaN and infinite query values.
- Search returns matches sorted by score descending.
- Equal scores tie-break by key ascending.
- Deleted vectors are excluded.
- Overwritten vectors use latest embedding and metadata.
- Missing collection returns collection not found.

### Cosine Search

- Identical vector ranks first.
- Orthogonal vector ranks below identical vector.
- Opposite vector ranks below orthogonal vector.
- Zero vector receives score `0.0` against non-zero query.
- Scores match deterministic fixture values within a small tolerance.

### Euclidean Search

- Identical vector ranks first with score `1.0`.
- Nearest vector by L2 distance ranks ahead of farther vector.
- Scores match `1 / (1 + l2_distance)` within a small tolerance.
- Ties sort by key.

### Dot Product Search

- Highest dot product ranks first.
- Negative dot product sorts below positive dot product.
- Non-normalized vectors are not implicitly normalized.
- Ties sort by key.

### Metadata Filter Search

- Empty filter matches all visible vectors.
- Equality filter matches exact top-level scalar values.
- Multiple conditions use AND semantics.
- Missing metadata does not match non-empty filter.
- Non-object metadata does not match non-empty filter.
- Filtered search still returns score-sorted, key-tie-broken results.
- Filtered search returns fewer than `k` when fewer matches exist.
- Filtered search excludes tombstones.

### Timestamp Search

- Capture timestamps for create, overwrite, and delete.
- Query before collection creation returns not found or empty according to the
  documented API.
- Query after first upsert sees the first embedding.
- Query after overwrite sees the overwritten embedding.
- Query after delete excludes the vector.
- Metadata filter uses historical metadata at the timestamp.
- Timestamp search does not read current embeddings from derived state.

## Historical Tests

### Get At Timestamp

- Read before first write returns `None`.
- Read at first write timestamp returns first value.
- Read after overwrite timestamp returns updated value.
- Read between overwrite and delete returns updated value.
- Read at or after delete timestamp returns `None`.
- Missing collection returns collection not found.

### Version History

- Create a vector.
- Overwrite it multiple times.
- Delete it.
- History returns newest-first rows.
- History includes vector revision, commit version, timestamp, and tombstone
  facts.
- Value rows preserve historical embedding and metadata.
- Missing key returns `None` or empty history according to the documented API.
- Corrupt historical record maps to the documented corruption error or is
  skipped only if the implementation explicitly documents skip semantics.

## Branch And Space Tests

### Branch Isolation

- Create a collection on the default branch.
- Upsert vectors on the default branch.
- Fork a child branch.
- Child branch reads inherited collection and vectors.
- Upsert same key differently on parent and child.
- Parent and child reads return branch-local values.
- Parent and child search return branch-local rankings.
- Parent and child key listing returns branch-local keys.
- Metadata patch in child does not mutate parent metadata.
- Filtered delete in child does not delete parent vectors.
- Delete-all in child does not delete parent vectors.
- Delete in child does not hide parent value.
- Delete collection in child does not delete parent collection.

### Space Isolation

- Create same collection name in two spaces.
- Upsert same key with different embeddings in both spaces.
- Reads return space-local values.
- Search returns space-local values.
- Counts are space-local.
- Key listing is space-local.
- Metadata patch is space-local.
- Filtered delete and delete-all are space-local.
- Delete vector in one space does not affect the other.
- Delete collection in one space does not affect the other.

## Durable Reopen Tests

### Reopen Collection Metadata

- Create multiple collections.
- Close and reopen durable-local database.
- List collections returns the same collection names, dimensions, and metrics.
- Counts after reopen are derived from visible vector rows.

### Reopen Vector Rows

- Upsert vectors with metadata.
- Overwrite one vector.
- Patch metadata on one vector.
- Delete one vector.
- Filter-delete one group of vectors.
- Close and reopen.
- Latest get returns visible vectors.
- Deleted key remains absent.
- List keys suppresses deleted keys.
- History remains available.
- Exact search works without sidecar files or prebuilt indexes.

### Reopen Branch And Space

- Create branch-local and space-local vector data.
- Close and reopen.
- Branch and space isolation remains intact.
- Fork visibility remains intact.

## Persistence And Corruption Tests

### Malformed Collection Rows

- Unknown config envelope version reports corruption.
- Truncated config reports corruption.
- Invalid metric reports corruption.
- Mismatched collection name reports corruption.
- Invalid dimension reports corruption.

### Malformed Vector Rows

- Unknown vector envelope version reports corruption.
- Truncated vector payload reports corruption.
- Mismatched collection/key reports corruption.
- Invalid metadata reports corruption.
- Non-finite embedding reports corruption.
- Wrong value type in a vector row reports corruption.

### Tombstone Handling

- Tombstoned vector is absent from latest get.
- Tombstoned vector is absent from latest search.
- Tombstoned vector reduces derived count.
- Tombstone remains visible in history facts.
- Collection delete tombstones or removes every vector row according to the
  documented storage plan and remains durable after reopen.

## Source And Dependency Guards

- Vector API module does not import storage request types.
- Vector service does not import executor command or output types.
- Persistence adapter does not import executor types.
- Rebuilt vector modules do not import old vector modules.
- Exact search does not import HNSW, quantization, mmap, or sidecar modules.
- Tests and code do not introduce benchmark-only storage bypasses.
- Public production crates above the executor layer do not depend directly on
  storage internals for vector behavior.

## Executor Hand-Off Tests

These are not required for the engine slice, but the engine API must make them
straightforward:

- command JSON round-trip for vector create/upsert/get/query/delete/batch
- default branch and default space application
- typed outputs for vector entries, matches, collections, and batch results
- typed outputs for key pages, metadata update, filtered delete, and delete-all
- closed-handle behavior for every vector command
- public error mapping for dimension mismatch, invalid vector, missing
  collection, invalid collection, invalid metadata patch, invalid filter, empty
  filtered-delete filter, and missing branch
- source guard proving executor vector helpers call `execute(Command::Vector...)`
  instead of calling lower-level services directly

## Completion Criteria

- All vector engine tests pass in cache mode.
- All vector engine tests pass in durable-local mode.
- Durable reopen proves collection config, visible rows, tombstones, history,
  and exact search are recovered from storage rows.
- Exact search has deterministic scores and order for cosine, Euclidean, and
  dot product.
- Branch and space tests prove collection identity is `(branch, space,
  collection)`.
- Key listing, metadata patch, filtered delete, and delete-all work in cache,
  durable-local, branch, space, and reopen tests.
- No approximate index, sidecar cache, or benchmark bypass is required for
  correctness.
