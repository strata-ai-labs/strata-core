# Executor Vector Command Contract Test Plan

## Purpose

Prove that the executor crate exposes a stable serialized vector command
boundary and remains a thin delegator over engine vector APIs.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Output serde round-trip | Required | Required |
| Collection commands | Required | Required |
| Upsert/get/exists/delete | Required | Required |
| List keys | Required | Required |
| Metadata patch | Required | Required |
| Batch upsert/get/delete | Required | Required |
| Filtered delete and delete-all | Required | Required |
| Exact latest query | Required | Required |
| As-of query and reads | Required | Required |
| Version history | Required | Required |
| Branch and space defaults | Required | Required |
| Error mapping | Required | Required |
| Reopen persistence | Not applicable | Required |
| Source guards | Required | Required |

## Contract Tests

### Command JSON Round Trip

- Serialize and deserialize every vector command variant.
- Include omitted branch/space.
- Include explicit branch/space.
- Include every distance metric.
- Include vectors with zero, positive, and negative finite values.
- Include metadata values.
- Include empty metadata.
- Include metadata filters.
- Include metadata patch payloads.
- Include empty batches.
- Include prefix, cursor, limit, and `as_of`.
- Assert deserialized command equality.

### Output JSON Round Trip

- Serialize and deserialize every vector output variant.
- Include collection info and collection lists.
- Include missing vector reads.
- Include latest vector reads with metadata.
- Include history rows with tombstones.
- Include vector matches with scores and metadata.
- Include key pages with and without next cursor.
- Include metadata update outcomes.
- Include single delete and bulk delete outcomes.
- Include batch upsert/get/delete item results.

### Command Name Coverage

- Assert `Command::name()` returns the stable name for every vector command.
- The match must be exhaustive so adding a command without naming it fails
  compilation.

### Command-To-Output Mapping

- Execute each vector command on a small cache database.
- Assert the output variant exactly matches the documented mapping.
- Latest `VectorGet` returns `VectorData`.
- Timestamp `VectorGet` returns `VectorData` with historical facts.
- `VectorGetv` returns `VectorVersionHistory`.
- `VectorListKeys` returns `VectorKeyPage`.
- `VectorDeleteByFilter` and `VectorDeleteAll` return
  `VectorBulkDeleteResult`.

## Delegation Tests

### Executor Uses Engine APIs

- Source guard rejects storage crate imports in executor sources.
- Source guard rejects storage row, storage commit, table, WAL, lifecycle, and
  compaction type names in executor vector code.
- Source guard rejects HNSW, mmap, quantization, sidecar, and vector index
  implementation imports in executor vector code.
- Source guard rejects engine persistence adapter imports in executor vector
  code.

### Convenience Facade Uses Commands

- Any vector convenience method must call `execute(Command::Vector...)`.
- Convenience methods must not directly call engine vector service methods.
- Convenience methods must not compute scores, apply filters, patch metadata,
  or scan keys.

### No Benchmark Bypass

- Vector smoke loaders and benchmarks use executor vector batch commands or
  public engine vector APIs.
- Source guard rejects direct storage writes from those binaries.

## Behavior Tests

Run behavior tests in both cache and durable-local executor fixtures unless the
test specifically targets reopen.

### Collection Commands

- Create a collection with cosine metric.
- Create a collection with Euclidean metric.
- Create a collection with dot-product metric.
- List collections and assert deterministic output.
- Read stats for one collection.
- Count a new collection and get zero.
- Duplicate create maps to executor conflict or invalid-input according to the
  engine contract.
- Delete collection returns true for existing collection.
- Delete missing collection returns false or maps to the documented error.

### Upsert And Get

- Execute `VectorUpsert` with metadata.
- Execute latest `VectorGet`.
- Assert key, embedding, metadata, version, timestamp, and vector revision.
- Upsert same key with a new embedding.
- Assert latest get returns the new embedding.
- Assert collection count remains one.
- Upsert without metadata stores no metadata.

### Exists

- Missing key returns false.
- Created key returns true.
- Deleted key returns false.
- Missing collection maps to the documented executor error.

### Delete

- Delete existing key.
- Assert `deleted=true`.
- Read the key and assert missing.
- Delete the key again and assert `deleted=false`.
- Count and query exclude the deleted key.

### List Keys

- Insert ordered vector keys with mixed prefixes.
- Execute `VectorListKeys` with no prefix.
- Execute `VectorListKeys` with a prefix.
- Assert deterministic ordering.
- Assert cursor and limit pagination.
- Assert `has_more` and cursor facts.
- Assert deleted keys are suppressed.
- Missing collection maps to the documented executor error.

### Metadata Patch

- Patch existing metadata by adding a field.
- Patch existing metadata by replacing a field.
- Assert unspecified fields are preserved.
- Assert embedding is preserved.
- Assert vector revision increments once.
- Assert latest get returns patched metadata.
- Patch missing key returns `updated=false` or not-found according to the
  documented engine contract.
- Non-object patch maps to executor invalid-input.
- Patch failure leaves existing vector unchanged.

### Batch Upsert

- Execute one `VectorBatchUpsert` with multiple vectors.
- Assert positional result count equals input count.
- Assert valid items have version, timestamp, and vector revision.
- Read each vector back.
- Empty batch returns an empty result list.
- Duplicate-key behavior matches the engine contract.
- Invalid vector shape maps to command failure or positional error according to
  the final batch contract.
- Failed validate-all batch leaves no partial writes.

### Batch Get

- Batch get existing and missing keys.
- Assert positional results preserve input order.
- Missing keys have empty value fields, not command failure.
- Duplicate reads are allowed.
- Empty batch returns an empty result list.

### Batch Delete

- Batch delete existing and missing keys.
- Assert positional results preserve input order.
- Empty batch returns an empty result list.
- Duplicate deletes match the engine contract.
- Deleted keys are absent from latest get, list-keys, count, and query.

### Delete By Filter

- Insert vectors with metadata groups.
- Execute `VectorDeleteByFilter`.
- Assert deleted count matches visible matching vectors.
- Assert non-matching vectors remain visible.
- Assert matching vectors are absent from get, list-keys, count, and query.
- Re-run the same command and assert deleted count is zero.
- Empty filter maps to executor invalid-input.
- Invalid filter maps to executor invalid-input and leaves data unchanged.

### Delete All

- Execute `VectorDeleteAll` on a non-empty collection.
- Assert deleted count equals previous count.
- Assert collection still exists.
- Assert count is zero.
- Assert list-keys returns empty.
- Assert latest query returns empty.
- Execute again and assert deleted count is zero.

### Query

- Query empty collection and get no matches.
- Query with `k == 0` and get no matches.
- Query with cosine metric and assert ordering.
- Query with Euclidean metric and assert ordering.
- Query with dot-product metric and assert ordering.
- Equal scores tie-break by key.
- Metadata filter restricts matches.
- Deleted vectors are excluded.
- Missing collection maps to executor not-found.

### As-Of Reads And Query

- Capture timestamps from two upserts and a delete.
- Execute `VectorGet` with `as_of`.
- Assert historical embeddings and metadata.
- Execute `VectorQuery` with `as_of`.
- Assert historical visibility and ranking.
- Deleted vectors remain visible before their delete timestamp and absent after.

### Version History

- Upsert a vector multiple times.
- Patch metadata.
- Delete the vector.
- Execute `VectorGetv`.
- Assert newest-first history.
- Assert commit versions, timestamps, vector revisions, values, metadata, and
  tombstone facts.

### Branch And Space Defaults

- Omit branch and space and assert executor default branch and `"default"`
  space.
- Repeat with explicit branch and explicit space.
- Set the executor default branch and assert omitted branch uses it.

### Branch Isolation

- Create a second branch through executor branch commands.
- Create the same collection on both branches if needed.
- Write the same vector key to both branches.
- Assert reads stay branch-local.
- Assert list-keys, count, query, and history stay branch-local.
- Metadata patch in one branch does not affect the other branch.
- Filtered delete and delete-all in one branch do not affect the other branch.

### Space Isolation

- Write the same collection and key to two spaces.
- Assert reads stay space-local.
- Assert list-keys, count, query, and history stay space-local.
- Metadata patch in one space does not affect the other space.
- Filtered delete and delete-all in one space do not affect the other space.

## Durable Tests

### Durable Open/Reopen

- Open durable-local executor handle.
- Create vector collections.
- Execute vector batch upsert.
- Patch metadata.
- Delete a subset by key.
- Delete a subset by filter.
- Close.
- Reopen.
- Assert reads, list-keys, count, query, history, and collection list survived.

### Large Batch Smoke

- Execute repeated `VectorBatchUpsert` commands for a scaled row count.
- Assert no per-row command loop in benchmark or smoke-loader code.
- Assert final count equals expected visible vectors.

## Error Tests

### Invalid Collection

- Empty collection name fails with executor invalid-input.
- Reserved collection name fails with executor invalid-input.
- Missing collection maps to executor not-found or documented command error.

### Invalid Key

- Overlong key fails with executor invalid-input.
- Null-byte key fails with executor invalid-input.
- Batch commands return positional item errors only if the batch contract allows.

### Invalid Embedding

- Wrong dimension maps to executor dimension mismatch with expected and actual
  dimension when available.
- NaN maps to executor invalid-input.
- Infinity maps to executor invalid-input.
- Empty vector maps to executor invalid-input.

### Invalid Filter

- Unsupported operator maps to executor invalid-input.
- Non-scalar comparison value maps to executor invalid-input.
- Empty filter for `VectorDeleteByFilter` maps to executor invalid-input.

### Invalid Metadata Patch

- Non-object patch maps to executor invalid-input.
- Oversized patch maps to executor invalid-input.
- Patch field-name violation maps to executor invalid-input.

### Missing Branch And Invalid Space

- Missing branch maps to executor not-found.
- Invalid space maps to executor invalid-input.
- Error messages mention public branch/space facts only.

### Closed Handle

- Close executor handle.
- Execute every vector command variant.
- Assert every command returns the same public closed-handle error class.

## Source Guard Tests

- Command module includes every vector variant in name/default helpers.
- Output module includes every vector output variant.
- Executor vector implementation does not import storage crates.
- Executor vector implementation does not import engine persistence modules.
- Executor vector implementation does not mention table, WAL, compaction,
  lifecycle, HNSW, mmap, quantization, or sidecar internals.
- Convenience methods delegate through serialized commands.
- Benchmarks use vector batch commands or public engine vector APIs.

## Completion Criteria

- Executor vector command and output serde fixtures cover every variant.
- Cache and durable-local vector behavior tests pass.
- Durable reopen proves vector command behavior survives close/open.
- Source guards prove executor remains a delegator.
- Error tests prove public error mapping is stable and storage internals do not
  leak.
