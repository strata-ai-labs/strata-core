# Engine Vector Indexing Test Plan

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/engine-vector-indexing-implementation-plan.md`

Design anchor:
`docs/architecture/engine/vector-indexing-design.md`

## Purpose

Prove that vector indexing accelerates candidate generation without changing
the vector primitive's product semantics.

The exact search path remains the ground truth. Every indexed implementation
must either match exact results exactly, in the case of flat indexes, or meet an
explicit recall gate against exact results, in the case of HNSW.

The current durable-local scaffold uses engine-owned artifact bytes outside
ordinary logical KV values, not Storage Level 3 atomic source+artifact slots.
That boundary is acceptable only while tests prove that manifest/artifact loss,
corruption, staleness, partial writes, and memory-budget skips cannot change
query correctness. Storage Level 3 gets its own future test pass when storage
exposes generic opaque artifact slots.

This test plan covers only the vector primitive. It intentionally excludes
shadow vectors, auto-embedding, query languages, hybrid retrieval, and ontology
search.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Exact baseline guard | Required | Required |
| Policy resolution | Required | Required |
| Diagnostics | Required | Required |
| Planner with exact source | Required | Required |
| System-space manifest | Required | Required |
| Flat source correctness | Required | Required |
| Active delta source | Required | Required |
| Sealed source fan-out | Required | Required |
| Write/delete invalidation | Required | Required |
| Branch fork behavior | Required | Required |
| Timestamp reads | Required | Required |
| Metadata filters | Required | Required |
| Durable reopen | Not applicable | Required |
| Manifest corruption fallback | Not applicable | Required |
| Artifact corruption fallback | Not applicable | Required if artifacts persist |
| Memory budget fallback | Required | Required |
| Old-engine lesson guards | Required | Required |
| Storage Level 3 deferral guards | Required | Required |
| HNSW recall | Required when HNSW enabled | Required when HNSW enabled |
| Benchmarks | Required | Required |

## Ground Truth Fixtures

Create deterministic vector datasets used by both unit and behavior tests.

### Small 2D Fixture

Collection:

```text
dimension = 2
metric = cosine
```

Rows:

```text
a = [1.0, 0.0]
b = [0.0, 1.0]
c = [0.7, 0.7]
d = [-1.0, 0.0]
```

Queries:

1. `[1.0, 0.0]`, `k = 4`
2. `[0.0, 1.0]`, `k = 2`
3. `[0.7, 0.7]`, `k = 3`

Assertions:

1. exact ordering is deterministic;
2. flat ordering matches exact;
3. score tie-breaks use key ascending;
4. zero `k` returns empty without touching index state.

### Duplicate Revision Fixture

Rows:

1. insert `user-1 = [1, 0]`
2. update `user-1 = [0, 1]`
3. insert `user-2 = [1, 0]`
4. delete `user-2`
5. insert `user-3 = [0.8, 0.2]`

Assertions:

1. latest search keeps newest visible `user-1`;
2. latest search suppresses deleted `user-2`;
3. timestamp search before update sees old `user-1`;
4. timestamp search before delete sees `user-2`;
5. indexed and exact paths agree.

### Active and Sealed Fixture

Use a low test threshold so the fixture can exercise both source shapes.

Rows:

1. insert enough vectors to cross the flat sealing threshold;
2. query and capture exact results;
3. force or trigger test-only sealing into a flat source;
4. insert additional vectors that remain in active delta;
5. update one vector that already exists in the sealed source;
6. delete one vector that already exists in the sealed source.

Assertions:

1. query searches sealed source plus active delta;
2. active update wins over sealed older value;
3. active delete suppresses sealed older value;
4. final results match exact search;
5. source diagnostics report one sealed source and one active delta.

### Metadata Fixture

Rows:

```text
doc-1: vector [1, 0], metadata { "tenant": "a", "kind": "note", "rank": 1 }
doc-2: vector [0, 1], metadata { "tenant": "a", "kind": "task", "rank": 2 }
doc-3: vector [1, 1], metadata { "tenant": "b", "kind": "note", "rank": 3 }
doc-4: vector [1, 0], metadata none
```

Filters:

1. `tenant == "a"`
2. `kind == "note"`
3. `rank >= 2`
4. `tenant == "missing"`

Assertions:

1. filters are exact after candidate generation;
2. missing metadata does not match non-empty filters;
3. indexed underfill falls back to exact if needed;
4. flat and exact results match exactly.

## Unit Tests

### Index Policy

- Default policy resolves to exact-only until indexing is explicitly enabled in
  the test fixture or runtime policy.
- Exact-only policy never builds flat or HNSW artifacts.
- Flat-only policy chooses exact below the collection threshold.
- Flat-only policy chooses flat at or above the source threshold.
- HNSW-allowed policy still chooses flat below the HNSW threshold.
- HNSW-allowed policy chooses HNSW only above threshold and within memory
  budget.
- Invalid thresholds are rejected or normalized deterministically.
- Overfetch factor cannot resolve to zero.
- Memory caps can disable HNSW without disabling exact search.

### Index Identity

- Identity includes format version.
- Identity includes policy version.
- Identity includes branch identity.
- Identity includes space identity.
- Identity includes collection name.
- Identity includes collection generation when available.
- Identity includes source identity when available.
- Identity includes dimension and metric.
- Identity changes when dimension changes.
- Identity changes when metric changes.
- Identity changes when index kind changes.
- Identity does not include mutable vector count.
- Identity does not include latest query timestamp.

### Branch Manifest

- Manifest row encodes format version.
- Manifest row encodes manifest generation.
- Manifest row encodes branch identity and branch generation.
- Manifest row encodes space identity.
- Manifest row encodes collection name.
- Manifest row encodes collection generation.
- Manifest row encodes policy version.
- Manifest row encodes artifact refs.
- Manifest row encodes active-delta facts.
- Manifest row encodes inherited fork caps.
- Manifest row encodes checksum.
- Manifest rejects unknown format version.
- Manifest rejects mismatched branch identity.
- Manifest rejects mismatched collection generation.
- Manifest rejects duplicate artifact refs.
- Manifest rejects refs with wrong dimension.
- Manifest rejects refs with wrong metric.
- Manifest identity does not include mutable vector count.
- Manifest/artifact identity does not include latest query timestamp.
- Manifest does not encode flat/HNSW payload bytes.
- Manifest size remains proportional to artifact ref count, not vector count.
- Missing manifest records a miss and falls back to exact search.
- Corrupt manifest records an error and falls back to exact search.

### Exact Candidate Source

- Produces the same candidates as the current visible-row iterator.
- Honors latest reads.
- Honors timestamp reads.
- Includes commit version and timestamp facts needed by merge.
- Handles empty collections.
- Handles `k` larger than collection size.
- Handles zero `k`.
- Handles dimension validation outside the source.

### Flat Source

- Builds from an empty source.
- Builds from one vector.
- Builds from many vectors.
- Rejects vectors with wrong dimension.
- Rejects non-finite embeddings.
- Preserves vector key.
- Preserves vector revision.
- Preserves commit version.
- Preserves commit timestamp.
- Scores cosine, Euclidean, and dot-product collections correctly.
- Returns source-local top-k in deterministic order.
- Handles `k` larger than source size.
- Handles duplicate scores by key order after global rerank.
- Reports estimated bytes.

### Active Delta Source

- Starts empty.
- Accepts committed inserts.
- Accepts committed updates.
- Accepts committed deletes.
- Searches exactly.
- Carries vector key metadata.
- Carries commit version.
- Carries commit timestamp.
- Suppresses deleted entries during latest search.
- Preserves deleted-entry facts needed to suppress older sealed candidates.
- Can be rebuilt from committed vector rows.
- Does not require graph mutation during commit.
- Reports estimated bytes.

### Sealed Source

- Builds from committed rows.
- Does not mutate after publication.
- Has stable source identity when storage provides one.
- Has temporary open-local identity before storage source identity exists.
- Rejects stale identity.
- Rejects wrong collection identity.
- Rejects wrong metric.
- Rejects wrong dimension.
- Can be dropped without data loss.
- Can be rebuilt from committed rows.
- Does not expose storage table or lifecycle types through the public API.

### Candidate Merge

- Merges candidates from one source.
- Merges candidates from many sources.
- Deduplicates by vector key.
- Keeps newest visible candidate for a duplicated key.
- Suppresses tombstoned candidates.
- Applies timestamp read bounds.
- Applies branch fork caps.
- Applies metadata filters exactly.
- Reranks by full-precision score.
- Breaks score ties by key ascending.
- Truncates after rerank.
- Falls back to exact on filtered underfill when policy requires it.
- Merges active delta and sealed sources.
- Gives active update priority over older sealed candidate for the same key.
- Gives active delete priority over older sealed candidate for the same key.
- Avoids O(collection) key resolution for ordinary indexed candidates.

### Artifact Encoding

Required only when durable artifacts are implemented.

- Encodes flat artifact header.
- Encodes identity.
- Encodes vector count.
- Encodes vector payload.
- Encodes commit facts.
- Rejects unknown artifact format version.
- Rejects mismatched identity.
- Rejects mismatched checksum.
- Rejects truncated header.
- Rejects truncated vector payload.
- Rejects impossible vector count.
- Rejects wrong dimension.
- Missing artifact records a miss and falls back.
- Corrupt artifact records an error and falls back or rebuilds.
- Stale artifact identity records an error and falls back.
- Over-budget artifact records an error and falls back.
- Partial artifact writes are ignored on reopen.
- Artifact payloads are addressed by manifest refs.
- Artifact payloads are not committed as ordinary system-space KV values.
- Artifact cleanup is driven by unreachable refs/source identity, not by product
  row history.
- Read-path rebuild is not required for correctness.

### Old-Engine Lesson Guards

- The vector row remains the source of truth for embedding and metadata.
- A missing index artifact cannot make a durable vector unreadable.
- A corrupt index artifact cannot make search return wrong results.
- Candidate sources carry enough inline key metadata for O(k) result assembly.
- Metadata filtering uses overfetch or exact fallback; it does not trust the
  candidate generator as the final filter authority.
- Index policy can express brute-force/exact and flat choices without exposing
  old backend names as product API.
- HNSW, when enabled, is a source artifact or source implementation, not a
  mandatory mutable side structure for every collection.
- Search remains correct if an index build fails after a successful write.
- Delete/recreate collection does not reuse stale source or artifact identity.
- Branch merge or fork behavior does not require vector id remapping inside an
  index backend.
- Branch-owned manifests replace a process-global mutable backend map.
- System space contains only manifest/ref metadata, never graph payloads.

### Storage Level 3 Deferral Guards

These tests lock down the current Level 2.5 scaffold so Level 3 can be added
later without relying on it for correctness.

- Storage code has no vector, metric, or HNSW semantic dependency.
- Engine-owned artifact payloads are derived state outside ordinary logical KV
  rows.
- System-space manifest rows contain refs, checksums, byte counts, and identity
  facts only.
- Manifest row size is bounded by artifact ref count, not vector count.
- Durable artifact bytes can be missing after reopen without changing results.
- Durable artifact bytes can be corrupt after reopen without changing results.
- Durable artifact bytes can be stale after reopen without changing results.
- Durable artifact bytes can be partially written after reopen without changing
  results.
- Artifact durable-write failure does not fail committed query semantics.
- Artifact memory-budget denial does not change results.
- Manifest refs pointing at unavailable payloads route to exact or safe flat
  fallback.
- Query correctness does not require Storage Level 3 atomic source+artifact
  commit.
- Storage Level 3, when added, must preserve these fallback semantics.

### HNSW Source

Required only when HNSW is enabled.

- Builds above threshold.
- Does not build below threshold.
- Rejects wrong-dimension rows.
- Uses the configured metric.
- Returns no more than requested source-local candidates.
- Returns candidate keys present in the source.
- Reports graph bytes.
- Falls back when graph build fails.
- Respects memory-budget disablement.
- Meets recall gate against exact search on deterministic datasets.

## Engine Behavior Tests

Run behavior tests against both cache and durable-local fixtures unless the
test is specifically durable-only.

### Baseline Compatibility

- Existing vector search tests pass unchanged.
- Exact query helper returns the same output as public query with exact policy.
- Public query output shape does not change after planner routing.
- Public query errors do not expose index internals.
- Missing collection behavior is unchanged.
- Dimension mismatch behavior is unchanged.

### Planner Routing

- Query routes through exact source when policy is exact-only.
- Query routes through flat source when flat is enabled and threshold is met.
- Query loads branch-owned manifest before resolving artifact refs.
- Query falls back to exact when manifest is missing.
- Query falls back to exact when manifest is stale.
- Query falls back to exact when manifest checksum is invalid.
- Query falls back to exact when source discovery fails.
- Query falls back to exact when artifact is stale.
- Query falls back to exact when artifact is corrupt.
- Diagnostics record which path was used.
- Diagnostics record fallback reason.

### Active Delta and Sealed Source Behavior

- Fresh committed writes are searchable before sealing.
- Active delta remains exact below threshold.
- Active delta can seal into a flat source when the threshold is crossed.
- Sealing is a rebuild from committed rows.
- Sealing failure leaves exact active-delta search available.
- Sealed source is immutable after publication.
- New writes after sealing land in active delta.
- Query fan-out across active and sealed sources matches exact search.
- Active update of a sealed key wins.
- Active delete of a sealed key suppresses the sealed candidate.
- Rebuild after invalidation produces the same result as exact search.

### Flat Index Correctness

- Flat indexed latest search matches exact latest search.
- Flat indexed timestamp search matches exact timestamp search.
- Flat indexed search with metadata filter matches exact filtered search.
- Flat indexed search with `k = 0` matches exact.
- Flat indexed search with `k > count` matches exact.
- Flat indexed search after overwrite matches exact.
- Flat indexed search after delete matches exact.
- Flat indexed search after batch upsert matches exact.
- Flat indexed search after batch delete matches exact.
- Flat indexed search after delete-all matches exact.

### Invalidation

- Upsert invalidates affected collection index.
- Update invalidates affected collection index.
- Delete invalidates affected collection index.
- Upsert updates only the target branch manifest.
- Delete updates only the target branch manifest.
- Batch upsert invalidates once per collection.
- Batch delete invalidates once per collection.
- Delete by filter invalidates affected collection.
- Delete all invalidates affected collection.
- Delete collection drops collection index state.
- Recreate collection with same name does not reuse stale old index.
- Collection in another space is not invalidated.
- Collection in another branch is not invalidated unless shared source identity
  requires safe eviction.
- Parent branch manifest is not changed by child branch writes.
- Child branch manifest is not changed by parent branch post-fork writes.
- Failed invalidation does not corrupt committed data.
- Failed invalidation records diagnostics and forces exact fallback.
- Index manager state can be discarded and rebuilt from committed rows.

### Branch Behavior

- Branch fork can search inherited indexed vectors.
- Branch fork creates a child manifest with inherited refs and fork caps.
- Branch fork does not copy artifact payload bytes.
- Branch-local upsert overrides inherited vector with same key.
- Branch-local delete suppresses inherited vector with same key.
- Parent branch later write is not visible to child past fork cap.
- Child branch write is not visible to parent.
- Two sibling branches can search different visible top-k results.
- Two sibling branch manifests can diverge while sharing inherited artifact refs.
- High branch-local invalidation can materialize a child-owned artifact without
  changing the parent manifest.
- Flat indexed branch results match exact branch results.
- HNSW branch results meet recall gate when HNSW is enabled.

### Timestamp Behavior

- Query at timestamp before collection creation fails or returns not found
  according to current vector API semantics.
- Query at timestamp after create but before writes returns empty.
- Query at timestamp after first write returns first version.
- Query at timestamp after overwrite returns new version.
- Query at timestamp before delete sees deleted key.
- Query at timestamp after delete suppresses deleted key.
- Indexed timestamp results match exact timestamp results.

### Metadata Filter Behavior

- Indexed query with equality filter matches exact.
- Indexed query with comparison filter matches exact.
- Indexed query with missing field matches exact.
- Indexed query with no metadata rows matches exact.
- Indexed query with filter that keeps one row matches exact.
- Indexed query with filter that keeps no rows returns empty.
- Approximate underfill triggers exact fallback when enabled.

### Durable Reopen

- Durable database with flat indexing can reopen and query correctly.
- Reopen with persisted manifest loads artifact refs.
- Reopen with missing manifest falls back or rebuilds correctly.
- Reopen with corrupt manifest falls back or rebuilds correctly.
- Reopen with stale manifest does not return stale results.
- Reopen without persisted artifact rebuilds or falls back correctly.
- Reopen with persisted matching artifact reuses it when artifact persistence is
  implemented.
- Reopen with stale artifact does not return stale results.
- Reopen after collection delete does not resurrect an index.
- Reopen after branch fork preserves branch search behavior.
- Reopen after artifact deletion rebuilds or falls back.
- Reopen after artifact corruption rebuilds or falls back.
- Reopen after partial artifact write ignores the partial artifact.
- Reopen after delete/recreate does not attach the old collection's artifact.

### Memory Budget

- Low-memory policy disables HNSW.
- Exact search still works when indexing is disabled by budget.
- Flat build refuses to exceed build budget.
- Artifact eviction does not change query results.
- Cache mode obeys budget.
- Durable mode obeys budget.
- Diagnostics report disabled-by-budget.

### Commit and Recovery Boundaries

- Public write succeeds when index artifacts are disabled.
- Public write succeeds when an index build is deferred.
- Public write does not depend on HNSW graph mutation.
- Query after write sees the write through exact active-delta search.
- Query after delete suppresses the deleted key even if an old sealed source
  still contains the candidate.
- Cache mode can discard all index state and still answer exact search.
- Durable mode can discard all index artifacts and rebuild or answer exact
  search.
- Durable mode can discard the index manifest and rebuild or answer exact
  search.
- Recovery never trusts artifact-only data that is absent from committed rows.
- Recovery failure to rebuild an optional artifact does not mark source vector
  rows lost.
- Recovery failure to load a manifest does not mark source vector rows lost.
- A manifest committed without a matching artifact payload is safe because the
  payload is derived and exact fallback remains available.
- Non-atomic Level 2.5 artifact persistence cannot make query results
  incorrect.

## Source and Dependency Guards

- Engine vector indexing code does not call storage-next internals directly
  outside the persistence/storage API boundary.
- Storage code does not depend on vector modules.
- Storage code does not reference HNSW, cosine, Euclidean, dot product, vector
  collection, or vector metadata types.
- Executor code does not compute vector distances.
- Executor code does not inspect index artifacts.
- Benchmarks use public engine APIs, not lower-layer storage bypasses.
- Tests do not require product-visible flush, compact, or rebuild commands.
- No test depends on old `.vec` or `.hgr` path compatibility.
- No production code reintroduces a global mutable vector backend map as the
  source of vector search truth.
- No production code requires transaction rollback to undo a graph mutation.
- No production code stores large flat/HNSW payloads as normal system-space KV
  rows.
- Storage artifact code treats vector payload bytes as opaque.
- No test assumes Storage Level 3 atomic artifact slots are present.

## Recall Tests

Required only when HNSW is enabled.

Datasets:

1. deterministic synthetic clustered data;
2. deterministic random data with fixed seed;
3. update/delete mixed fixture;
4. branch fork fixture.

For each dataset:

1. compute exact top-k ground truth;
2. run HNSW query with configured `ef_search` and overfetch;
3. rerank candidates exactly;
4. calculate recall@1, recall@5, recall@10;
5. assert recall gates.

Initial gates:

```text
recall@1  >= 0.90
recall@5  >= 0.92
recall@10 >= 0.95
```

These gates may be tightened after real benchmark data. They should not be
loosened to hide implementation bugs.

## Benchmark Plan

Benchmarks must run through normal engine vector APIs.

### Datasets

Run deterministic generated datasets first:

1. 10K vectors;
2. 100K vectors;
3. 1M vectors if local resources allow.

Use dimensions:

1. 128;
2. 384;
3. 768.

Use metrics:

1. cosine;
2. Euclidean;
3. dot product.

### Measurements

Record:

1. vector count;
2. dimension;
3. metric;
4. runtime mode;
5. memory budget;
6. index policy;
7. index kind used;
8. manifest ref count;
9. inherited versus owned ref count;
10. build time;
11. derived bytes;
12. write throughput;
13. query p50/p95/p99;
14. recall@k against exact;
15. exact fallback count;
16. manifest miss/stale/corrupt count;
17. artifact miss/stale/corrupt count if applicable.

### Comparisons

Run:

1. exact-only;
2. flat indexing;
3. HNSW when implemented.

For flat indexing:

1. result equality with exact is mandatory;
2. speedup over exact is expected for repeated queries;
3. write throughput must not materially regress.

For HNSW:

1. recall gates must pass;
2. query latency should improve over flat at large sizes;
3. build cost must be reported, not hidden.

## Failure Cases

- Missing manifest falls back or rebuilds.
- Corrupt manifest falls back or rebuilds.
- Stale manifest generation falls back or rebuilds.
- Missing artifact falls back or rebuilds.
- Corrupt artifact falls back or rebuilds.
- Stale identity falls back or rebuilds.
- Wrong dimension artifact is rejected.
- Wrong metric artifact is rejected.
- Memory budget denial falls back to exact.
- HNSW dependency error falls back to flat or exact.
- Source discovery error falls back to exact.
- Partial artifact write is ignored on reopen.
- Manifest ref pointing to a missing artifact falls back or rebuilds.
- System-space row containing graph payload-sized data is rejected by guard
  tests.

## Definition of Done

The test slice is complete when:

1. exact baseline tests pass in cache and durable-local modes;
2. planner routing is covered;
3. active delta and sealed source behavior is covered;
4. branch-owned manifest behavior is covered across fork, reopen, corruption,
   and branch-local divergence;
5. flat indexed results match exact results across latest, timestamp, branch,
   delete, overwrite, batch, and metadata-filter cases;
6. invalidation prevents stale query results;
7. durable reopen behavior is covered;
8. memory-budget fallback is covered;
9. old-engine lesson guards pass;
10. source/dependency guards pass;
11. Storage Level 3 is either implemented with atomic source+artifact tests or
    explicitly deferred with Level 2.5 fallback guards;
12. HNSW recall tests exist before HNSW is enabled by default;
13. benchmarks report exact versus indexed behavior through normal engine APIs.
