# Engine Vector Indexing Implementation Plan

Status: draft implementation plan

Test plan:
`docs/architecture/implementation-plans/engine-vector-indexing-test-plan.md`

Design anchor:
`docs/architecture/engine/vector-indexing-design.md`

## Objective

Add vector indexing to the rebuilt engine vector primitive without changing the
public vector API contract or adding a separate search/query layer.

The current vector service has the right semantic baseline: it scans visible
vector rows, applies metadata filters, computes exact scores, sorts
deterministically, and returns top-k results. That exact path should remain the
source of truth and the fallback path. Indexing should make candidate
generation faster while preserving the same branch, space, timestamp, delete,
metadata-filter, and rerank semantics.

This plan intentionally keeps shadow vectors, auto-embedding, hybrid search,
global query APIs, and ontology/search control-plane state out of scope. A
small branch-owned vector index manifest in system space is in scope because it
is the branch selection catalog for vector artifacts, not a user-facing search
control plane.

## Current State

Relevant current files:

1. `crates/engine-next/src/data/vector/service.rs`
2. `crates/engine-next/src/data/vector/types.rs`
3. `crates/engine-next/src/data/vector/record.rs`
4. `crates/engine-next/src/data/vector/distance.rs`
5. `crates/engine-next/tests/engine_vector.rs`
6. `crates/storage-next/src/api/`
7. `crates/storage-next/src/lifecycle/`

The current query path is exact:

1. Load branch and collection config.
2. Validate query dimension.
3. Iterate visible vector entries for the requested read selector.
4. Apply vector-owned metadata filter logic.
5. Compute full-precision vector score.
6. Sort by score descending and key ascending.
7. Truncate to `k`.

That path is correct and must stay available for:

1. tiny collections;
2. low-memory profiles;
3. corrupt or missing derived artifacts;
4. unsupported index policies;
5. exact validation in tests and benchmarks.

## Old Engine Lessons

The old vector engine should be treated as evidence, not as a structure to port
wholesale.

Useful lessons to keep:

1. **KV rows were the source of truth.** The old `VectorRecord` stored the
   embedding, metadata, product version, timestamps, and internal vector id in
   versioned KV. Heap and graph state were accelerators.
2. **Collection-level backend selection was useful.** The old collection record
   persisted `BruteForce`, `Hnsw`, or `SegmentedHnsw`. That maps cleanly to an
   internal resolved indexing policy in the rebuilt engine.
3. **The active-buffer plus sealed-segment model was the right shape.**
   Inserts landed in a brute-force active buffer. Large buffers sealed into
   compact HNSW segments. Search fanned out across active and sealed sources.
4. **Inline result metadata mattered.** Storing key/source facts beside vector
   ids avoided an O(n) KV scan when resolving search candidates.
5. **Index files were caches.** `.vec` heap files and `.hgr` graph files were
   rebuildable from KV and never authoritative.
6. **Recovery fallback was essential.** Missing, corrupt, stale, or mismatched
   cache files caused rebuild from KV, not data loss.
7. **Filtered search needed overfetch.** Metadata filters were applied after
   candidate generation, with adaptive overfetch to avoid under-filled top-k
   results.

Tangles to avoid:

1. **Do not revive the mutable per-collection backend as the architecture.**
   The old `DashMap<CollectionId, Box<dyn VectorIndexBackend>>` made the index
   state a long-lived side structure that had to be kept coherent with KV.
2. **Do not rely on post-commit HNSW mutation as the primary consistency
   mechanism.** The old engine queued staged vector ops during transactions and
   applied them through commit observers because HNSW was not rollback-safe.
   That was pragmatic, but it split commit correctness from search freshness.
3. **Do not make branch/merge correctness depend on vector id remapping inside
   a graph backend.** The old merge path had to rebuild collections and rewrite
   vector ids to avoid collisions across independently-created branches.
4. **Do not make sidecar path layout part of product correctness.** Old `.vec`
   and graph cache purges had known delete/recreate races that were benign only
   because the files were write-through caches.
5. **Do not expose backend choice as an admin surface yet.** The product should
   resolve policy from collection shape and runtime budget first. User tuning
   can come later if benchmarks prove it is needed.

The rebuilt design should keep the active/sealed indexing lesson, but anchor it
to a branch-owned system-space manifest, storage-visible source identity, and
engine-owned derived artifacts instead of a separate mutable backend that must
shadow every commit.

## Binding Decisions

1. **The manifest is branch-owned; artifact payloads are source-owned.**
   A branch-owned system-space manifest records the artifact refs that a branch
   should search for each collection. Large flat/HNSW payloads attach to
   immutable row sources or logical vector segments outside logical KV values.
   Whether a source lives in L0, L1, or a lower level is a storage fact, not a
   vector-indexing semantic.

2. **Index kind is chosen by threshold and policy.**
   Tiny collections use exact scan. Small sealed segments use flat search.
   Large sealed segments may use HNSW. Do not bake in rules such as "L0 flat,
   L1 HNSW" as product semantics.

3. **Exact search remains the semantic ground truth.**
   Approximate search only proposes candidates. Final visibility, duplicate
   suppression, metadata correctness, and ordering are enforced by exact
   rerank over full-precision vectors.

4. **Storage stays semantics-free.**
   Storage may expose generic immutable source identity, row iteration, system
   space rows, and opaque derived-artifact persistence. It must not know what a
   vector, metric, HNSW graph, or recall target is.

5. **The first shippable index can be flat.**
   HNSW is useful, but the first indexing slice should prove source identity,
   invalidation, candidate merging, exact rerank, diagnostics, and fallback
   behavior before adding graph complexity.

6. **No benchmark-only lower-layer bypasses.**
   Benchmarks must exercise the same public engine vector APIs that executor
   and SDK layers use.

7. **Index artifacts are derived state.**
   Missing, corrupt, stale, or over-budget artifacts must degrade to exact
   search or rebuild. They must never affect data correctness or recovery.

8. **Graph and flat payloads are not system-space KV values.**
   System space stores the branch-local manifest and small refs/checksums. The
   large contiguous flat payloads and HNSW graphs are storage-managed artifact
   bytes so index rebuilds do not pollute MVCC history, WAL, or logical row
   compaction.

9. **Memory budget controls index availability.**
   Vector indexes are optional accelerators under memory pressure. The runtime
   budget may disable HNSW, cap flat payloads, delay builds, or evict artifacts.

10. **Public collection semantics do not change.**
   Existing collection config, dimension, metric, writes, deletes, history,
   branch behavior, and metadata filters remain compatible.

11. **Shadow-vector identity is deferred.**
    Index identity should be structured enough to add model/source identity
    later, but this slice must not implement shadow vectors or model lifecycle.

12. **Active writes stay exact until sealed.**
    The rebuilt engine should preserve the old lesson that fresh writes do not
    need immediate graph insertion. New mutable deltas are searched exactly and
    only become indexed artifacts when they are sealed into an immutable source.

13. **Index maintenance follows source publication.**
    The old engine had to stage backend mutations through commit observers. The
    rebuilt engine should instead treat source-row publication as authoritative
    and make indexing an invalidation/rebuild problem over committed sources.

14. **Candidate resolution must be O(k), not O(collection).**
    The old inline metadata optimization should survive in a cleaner form:
    candidate sources must carry enough key and commit metadata to avoid a full
    collection scan when resolving each vector id.

15. **Storage Level 3 is deferred from the current scaffold.**
    The target design still wants storage-owned opaque artifact slots committed
    with immutable source publication. The current implementation deliberately
    stops short of that contract: branch manifests live in system space, while
    flat/HNSW payload bytes live in an engine-owned derived-artifact boundary
    outside ordinary logical KV rows. This is acceptable only because artifacts
    are non-authoritative and every query can fall back to exact committed row
    search.

## Non-Goals

This slice must not implement:

1. shadow vectors or auto-embedding;
2. document/JSON search indexing;
3. hybrid retrieval;
4. a query language or general search API;
5. ontology or graph search;
6. distributed serving;
7. IVF or centroid indexes;
8. product-visible index administration commands;
9. explicit user commands for flush, compact, or rebuild;
10. benchmark-only storage access paths;
11. post-commit graph mutation as the primary consistency mechanism;
12. old `.vec` or `.hgr` path layout compatibility;
13. storing large flat/HNSW payloads as normal system-space KV values.

## Target Architecture

### Exact Baseline

Keep the current exact query path as a first-class implementation:

```text
exact_query(collection, q, k, filter, read_selector):
  rows = visible vector rows for branch/space/collection/read_selector
  rows = rows matching filter
  scored = full_precision_score(q, rows)
  return sort(score desc, key asc).take(k)
```

This is used directly when indexing is disabled or when an index cannot safely
answer a query.

### Candidate Source Abstraction

Introduce an internal candidate-source boundary under
`crates/engine-next/src/data/vector/index/`.

Suggested modules:

1. `policy.rs`
   - resolved index policy;
   - thresholds;
   - index kind selection;
   - overfetch and fallback controls.
2. `identity.rs`
   - index identity;
   - manifest identity;
   - source identity;
   - format version and policy version.
3. `manifest.rs`
   - branch-owned manifest row format;
   - artifact refs and active-delta facts;
   - fork/reference copy logic;
   - manifest generation and stale detection.
4. `source.rs`
   - vector candidate source trait;
   - exact source implementation;
   - flat source implementation;
   - HNSW source placeholder or implementation.
5. `flat.rs`
   - contiguous flat vector payload;
   - key and commit metadata side arrays;
   - exact source-local scoring.
6. `planner.rs`
   - source discovery;
   - manifest ref selection;
   - source selection;
   - fan-out query execution;
   - overfetch;
   - fallback decisions.
7. `merge.rs`
   - candidate dedupe;
   - visibility checks;
   - exact full-precision rerank;
   - deterministic tie-breaking.
8. `artifact.rs`
   - serialization format for derived artifacts;
   - stale/corrupt/missing handling;
   - memory accounting.
9. `diagnostics.rs`
   - public or test-visible index diagnostics.

The vector service should call this boundary from `query` and `query_at`.
Write APIs should not directly manipulate index internals. They should publish
ordinary commit facts and let the index manager invalidate or rebuild derived
state.

### Source Shapes

The implementation should support three source shapes:

1. **Mutable delta source**
   - newest unsealed writes visible to the read;
   - always exact;
   - never persisted as an index artifact.

2. **Sealed source**
   - immutable storage-backed row source;
   - eligible for flat or HNSW artifact;
   - artifact identity includes source identity and collection config identity.

3. **Logical fallback source**
   - current whole-collection exact scan path;
   - used until storage exposes enough source shape;
   - used whenever index source discovery fails closed.

The first implementation may route everything through the logical fallback
source while the planner and merge machinery land. The second implementation
should switch sealed portions to source-owned flat artifacts.

### Active Delta and Sealed Segments

The target shape intentionally mirrors the old `SegmentedHnswBackend` idea
without porting its mutable backend mechanics.

1. **Active delta**
   - contains fresh committed writes that are not yet represented by a sealed
     source artifact;
   - uses exact search;
   - carries commit metadata and key metadata;
   - is cheap to invalidate or rebuild from committed rows;
   - never requires rollback-aware graph mutation.

2. **Sealed segment**
   - represents an immutable committed source or source range;
   - may have no artifact, a flat artifact, or an HNSW artifact;
   - is identified by storage source identity and vector collection identity;
   - can be shared by inherited branches when storage visibility permits;
   - can be dropped and rebuilt without data loss.

3. **Fan-out**
   - query searches active delta plus sealed sources;
   - each source returns overfetched candidates;
   - merge performs exact visibility, metadata filtering, dedupe, and rerank.

Unlike the old backend, sealing should not be based only on an in-memory vector
count. It should be driven by committed source shape when available, and by a
temporary open-local threshold only until storage exposes stable source
identity.

### Branch-Owned Index Manifest

Each branch should own a small vector index manifest in system space. The
manifest is the branch-local catalog that says which vector artifacts and exact
deltas are searchable for a collection.

The manifest stores small metadata only:

```text
VectorIndexManifest
  format_version
  manifest_generation
  branch_id
  branch_generation
  space_name_or_id
  collection_name
  collection_generation
  policy_version
  artifact_refs[]
  active_delta_watermark
  invalidated_key_summary
  checksum
```

Each artifact ref stores:

```text
VectorArtifactRef
  artifact_id
  source_id
  source_branch_id
  source_generation
  fork_version_cap
  index_kind
  vector_dimension
  metric
  vector_count
  derived_bytes
  checksum
```

Large flat payloads and HNSW graphs are not stored in the manifest row. They
live in the storage-managed derived artifact layer and are referenced by
`artifact_id`.

Fork behavior:

1. Forking a branch creates a child manifest by copying artifact refs from the
   source manifest and recording the fork version cap on inherited refs.
2. The child does not copy artifact bytes.
3. Child-local writes land in the child active delta and later produce
   child-owned artifact refs.
4. Parent-local writes after the fork do not change the child manifest.
5. If child deletes/overwrites invalidate many inherited candidates, maintenance
   may materialize a child-owned artifact and replace inherited refs in the
   child manifest.

The manifest gives the planner a direct branch-local index catalog. Query
planning should not rediscover all reachable storage sources on every request
once a valid manifest exists.

### Index Identity

Define a stable internal identity that prevents stale reuse:

```text
VectorIndexIdentity
  format_version
  policy_version
  branch_id
  space_name_or_id
  collection_name
  collection_generation
  manifest_generation
  source_id
  source_generation
  artifact_id
  vector_dimension
  metric
  index_kind
```

Do not include mutable counts. Counts are diagnostics, not identity.

If a future vector collection config grows model/source identity, add it to this
identity then. Do not invent shadow-vector model semantics in this slice.

### Index Policy

Add an internal resolved policy with conservative defaults:

```text
VectorIndexPolicy
  mode: Auto | ExactOnly | FlatOnly | HnswAllowed
  collection_exact_threshold
  source_flat_threshold
  source_hnsw_threshold
  overfetch_factor
  filtered_underfill_fallback
  max_index_bytes
  build_budget_bytes
```

Initial default:

1. exact below a small collection threshold;
2. flat for sealed sources above the exact threshold;
3. HNSW disabled until flat indexing and exact-vs-index validation are stable.

Later default:

1. exact for tiny collections;
2. flat for small/medium sealed sources;
3. HNSW for large sealed sources if memory budget allows.

Policy should be resolved from runtime budget and collection config. It should
not require public user-facing index tuning in this slice.

### Flat Index

The flat index is the first derived artifact to implement.

It should store:

1. vector key;
2. vector revision;
3. commit version;
4. commit timestamp;
5. deletion state if the source can contain tombstones;
6. metadata presence or pointer needed for filter fallback;
7. full-precision embedding bytes or a zero-copy payload handle if storage can
   safely expose one;
8. optional normalized vector payload for cosine if it measurably helps.

This is the cleaner replacement for the old heap plus inline metadata cache.
The artifact must be self-sufficient for candidate generation and O(k)
candidate resolution, while the committed vector row remains authoritative for
historical reads and exact fallback.

Query behavior:

1. score all vectors in the source with full precision;
2. return the source-local top `k * overfetch_factor`;
3. do not make final visibility or duplicate decisions inside the source;
4. let merge/rerank enforce global correctness.

### HNSW Index

HNSW should be added after flat indexing proves the source and merge contract.

Implementation rules:

1. Use a maintained Rust HNSW implementation if it satisfies serialization,
   metric, memory, and licensing requirements. Do not hand-roll graph search
   unless existing crates fail the requirement review.
2. Keep HNSW behind the same `VectorCandidateSource` trait as flat search.
3. Store enough payload to rerank candidates exactly.
4. Treat HNSW as optional derived state. Missing or corrupt graph artifacts
   rebuild or fall back to exact/flat.
5. Use deterministic build options in tests where the library allows it.
6. If deterministic graph construction is not possible, tests should assert
   recall bounds against exact ground truth, not byte-identical graph shape.

Default starting parameters when enabled:

```text
M = 16
ef_construction = 200
ef_search = 64
overfetch_factor = 4
```

The old engine's segmented HNSW default used a 50,000-vector active-buffer seal
threshold and graph sidecar files. Treat those as benchmark starting evidence,
not as product constants. The rebuilt thresholds should be policy values
resolved from collection size, dimension, and memory budget.

### Query Flow

Target query flow:

```text
query(collection, q, k, filter, read_selector):
  config = require collection config
  validate q.dimension == config.dimension
  if k == 0: return empty

  policy = resolve index policy
  manifest = load branch index manifest(collection)
  sources = resolve manifest refs plus active delta
  if manifest/sources unavailable or policy exact-only:
      return exact_query(...)

  candidates = []
  for source in sources:
      source_k = k * policy.overfetch_factor
      candidates += source.search(q, source_k, filter_hint)

  candidates = enforce_read_visibility(candidates, read_selector)
  candidates = suppress_tombstones(candidates)
  candidates = dedupe_by_vector_key_keep_newest_visible(candidates)
  candidates = apply_metadata_filter_exactly(candidates, filter)
  candidates = rerank_full_precision(candidates, q, config.metric)

  if underfilled and policy.filtered_underfill_fallback:
      return exact_query(...)

  return sort(score desc, key asc).take(k)
```

The final result ordering must remain byte-for-byte compatible with the exact
baseline for flat indexing. HNSW may return approximate candidates, but final
ordering over returned candidates must still use exact scores and key tie-breaks.

### Metadata Filters

Do not add a metadata bitmap index in this slice.

Required behavior:

1. Existing vector metadata filters continue to be exact.
2. Candidate sources may receive a filter hint, but they must not be trusted to
   enforce final filter correctness.
3. If an approximate source underfills after filtering, the planner falls back
   to exact search for that query.

Later, metadata filter acceleration can be added as its own search/indexing
pass.

### Storage Boundary

The plan has four storage integration levels. Levels 1 and 2 establish
correctness. Level 2.5 is the current durable-local scaffold. Level 3 is the
future storage integration target.

Level 1: no new storage contract

1. Planner uses the current logical exact iterator.
2. Flat artifacts may be open-local and invalidated on writes.
3. This validates API behavior, policy, diagnostics, and exact fallback.

Level 2: system-space manifest contract

1. Engine writes a branch-owned vector index manifest row in system space.
2. Manifest rows contain artifact refs, policy facts, generation, and checksums.
3. Manifest rows do not contain flat/HNSW payload bytes.
4. Fork creates a child manifest by copying refs and recording fork caps.
5. Manifest loss, corruption, or stale generation triggers exact fallback or
   manifest rebuild from committed vector rows.

Level 2.5: current engine-owned artifact boundary

1. Engine owns flat/HNSW artifact payload files or memory blobs outside normal
   logical KV values.
2. Durable-local artifact files are not atomically committed with storage
   source publication.
3. Branch-owned system-space manifests store only small refs, identity facts,
   byte counts, and checksums.
4. Query may reuse a matching artifact, but must skip missing, corrupt, stale,
   or over-budget artifacts.
5. A skipped artifact must route to another safe source, normally flat fallback
   or exact committed-row search.
6. Artifact loss, failed durable artifact writes, or partial artifact writes
   must never make committed vector rows unreadable or make query results
   incorrect.
7. Read-path artifact rebuild is an optimization, not a correctness
   requirement.

Level 3: deferred generic source and artifact contract

1. Storage exposes immutable source identity and source row iteration for a
   branch/space/key range.
2. Storage exposes opaque derived-artifact slots addressed by artifact id and
   tied to source identity.
3. Storage commits source and artifact metadata atomically when it has enough
   lifecycle context to do so.
4. Storage recovery treats artifacts as rebuildable.

Level 3 is not required for the current vector indexing scaffold to be correct.
It is the point where durable artifact reuse becomes cleaner and less
engine-local: source identity, artifact placement, and atomic source+artifact
publication move behind a generic storage contract while storage remains
semantics-free.

Storage must not:

1. parse vector rows;
2. compute vector metrics;
3. choose HNSW parameters;
4. know collection names beyond ordinary key ranges;
5. reject data because an artifact is unavailable;
6. store large graph payloads as ordinary logical KV rows.

### Current Correctness Contract

Until Level 3 exists, the current vector indexing scaffold is correct only under
this contract:

1. Committed vector rows remain the source of truth for embedding, metadata,
   tombstones, branch visibility, and timestamp reads.
2. The branch-owned manifest is a search catalog, not authoritative data.
3. Manifest identity must match branch, space, collection generation,
   dimension, and metric before refs are searched.
4. Artifact identity must match manifest refs, collection generation, source
   identity, dimension, metric, bytes, and checksum before payloads are
   searched.
5. Mutable vector count and query timestamp are not artifact identity fields.
   Fresh writes are covered by exact active-delta search or exact fallback.
6. Missing, corrupt, stale, over-budget, or unavailable artifacts must be
   skipped and reported in diagnostics.
7. Flat indexed results must match exact results byte-for-byte after final
   visibility, tombstone, filter, dedupe, and rerank checks.
8. HNSW results must meet recall gates against exact ground truth before HNSW
   can be treated as a default accelerator.
9. A read query must not be required to mutate committed engine state in order
   to be correct.
10. Artifact persistence, artifact eviction, and artifact rebuild failures are
    performance events, not data-correctness events.

### Write, Delete, and Collection Lifecycle

Writes and deletes remain source-of-truth row commits.

Index manager behavior:

1. Upsert updates the branch-owned manifest's active delta facts or invalidates
   affected collection refs.
2. Delete records branch-local suppression facts or invalidates affected
   collection refs.
3. Delete collection marks the branch manifest generation obsolete and drops
   all open-local artifacts for that collection.
4. Durable artifacts for deleted, compacted-away, or unreferenced sources
   become unreachable through manifest refs/source identity and can be
   reclaimed by ordinary maintenance.
5. Batch upsert/delete produces one manifest/invalidation event per collection,
   not per row.

Do not block a successful write because index invalidation or artifact eviction
fails. Index state is derived.

Do not apply graph mutations inside the storage commit path. If a write commits
and the index manager fails to invalidate or rebuild, the query planner must
detect the stale or missing index state and use exact fallback. This preserves
the old durability lesson without inheriting the old split-brain risk between
KV commit and backend freshness.

### Branch and Timestamp Semantics

Indexing must preserve current branch behavior:

1. A forked branch can search inherited vectors.
2. Branch-local writes override inherited vectors with the same key.
3. Deletes suppress inherited vectors for that branch when storage visibility
   says they are deleted.
4. Timestamp reads see only rows visible at the requested timestamp.
5. Fork creates a branch-owned manifest with inherited refs capped at the fork
   version.
6. Index artifacts may be shared for immutable inherited sources, but final
   visibility checks still enforce fork caps and read bounds.
7. Parent branch post-fork writes update only the parent manifest.
8. Child branch writes update only the child manifest until explicit branch
   materialization/maintenance creates a new child-owned artifact.

### Diagnostics

Add diagnostics that are useful in tests and benchmarks without exposing storage
internals as public product API.

Suggested diagnostics:

```text
VectorIndexDiagnostics
  collection
  manifest_generation
  manifest_ref_count
  manifest_inherited_ref_count
  manifest_owned_ref_count
  active_delta_count
  policy_mode
  resolved_index_kind_summary
  exact_fallback_count
  indexed_source_count
  exact_source_count
  flat_source_count
  hnsw_source_count
  indexed_vector_count
  derived_bytes
  last_build_duration
  last_build_error
  last_query_used_index
  last_query_fallback_reason
```

These should be accessible through engine diagnostics or testkit, not executor
commands in this slice.

### Resource Budget

Vector indexing must honor the runtime memory budget plan.

Rules:

1. Index artifacts must report estimated bytes.
2. Building an artifact must check build budget before allocating large buffers.
3. Cache mode must obey the same budget controls as durable mode.
4. Low-memory profiles should prefer exact or flat over HNSW.
5. Evicting an artifact is allowed and must not affect correctness.
6. A failed build records diagnostics and falls back to exact search.

## Implementation Order

### 1. Exact Baseline Guard

1. Add helper tests or testkit utilities that run exact query explicitly.
2. Capture deterministic ground-truth results for representative collections.
3. Make the existing query path call the exact helper without behavior change.
4. Confirm no public API output changes.

Exit criteria:

1. Existing vector tests pass.
2. Exact query helper is available to later index tests.

### 2. Index Policy and Diagnostics

1. Add internal vector index policy types.
2. Add default resolved policy.
3. Add diagnostics structs and testkit accessors.
4. Wire policy resolution into `VectorService::query` and `query_at`.
5. Keep policy in exact-only mode by default during this step.

Exit criteria:

1. Query behavior is unchanged.
2. Diagnostics show exact fallback and policy facts.

### 3. Planner and Merge Boundary

1. Introduce `VectorCandidateSource`.
2. Implement exact candidate source over the existing visible-entry iterator.
3. Implement merge, dedupe, visibility, filter, and rerank helpers.
4. Route query through planner with a single exact source.
5. Assert byte-for-byte output equality with the old exact path.

Exit criteria:

1. No behavior change.
2. Planner can be tested independently.
3. The old direct query body is reduced to planner invocation plus validation.

### 4. Branch-Owned System-Space Manifest Skeleton

Status: implemented as the current scaffold.

1. Add vector index manifest records under branch-local system/control space.
2. Store only manifest metadata, refs, generations, active-delta facts, and
   checksums.
3. Prohibit large flat/HNSW payload bytes from manifest rows.
4. Load manifest refs during `query` and `query_at`.
5. Validate manifest identity against branch, space, collection generation,
   dimension, and metric.
6. Fall back to exact search when the manifest is missing, corrupt, stale, or
   over budget.
7. Keep default policy in exact-only mode.

Exit criteria:

1. Missing, corrupt, stale, and loaded manifests cannot change query results.
2. Timestamp reads use timestamp-visible manifest facts.
3. Diagnostics expose manifest status, ref counts, inherited/owned counts, and
   active-delta counts.
4. Manifest loss or corruption cannot change query correctness.

### 5. Artifact Boundary and Flat Payload Format

Status: implemented as the current scaffold.

The engine now has an internal flat-artifact identity, payload codec, checksum
validation, and private artifact store boundary. Testkit can build artifacts
from committed visible rows and verify loaded, missing, corrupt, stale, and
over-budget states. Production vector APIs still use exact search by default;
this slice only establishes the artifact boundary and payload format.

1. Add internal artifact identity types for flat vector payloads.
2. Define the engine-side artifact handle/load/store interface without exposing
   it through public product APIs.
3. Keep artifact payloads outside normal system-space KV rows.
4. Add a compact flat payload format carrying key, commit version, timestamp,
   vector revision, embedding, and small metadata facts needed for candidate
   resolution.
5. Add payload checksum/version validation.
6. Add testkit builders that create flat artifacts from committed visible rows.
7. Do not enable automatic artifact building for production queries yet.

Exit criteria:

1. Flat payloads round-trip deterministically.
2. Missing, corrupt, stale, or over-budget artifacts are reported and skipped.
3. Artifact refs remain small manifest records.
4. No public vector API behavior changes.

### 6. Flat Artifact Candidate Source

1. Implement a `VectorCandidateSource` over loaded flat artifacts.
2. Search manifest-selected flat artifacts plus exact fallback where needed.
3. Keep exact rerank over full vector values as the final ordering authority.
4. Preserve tombstone-aware merge semantics so active deletes suppress older
   artifact candidates.
5. Keep flat artifacts disabled below the exact threshold.
6. Route tests through normal `VectorService::query` and `query_at` APIs.

Exit criteria:

1. Flat-artifact queries match exact results byte-for-byte.
2. Filters, ties, timestamp reads, deletes, updates, and branches match exact.
3. Diagnostics prove which artifact refs were loaded, skipped, or searched.
4. Query correctness does not depend on artifact availability.

### 7. Active Delta and Sealing Policy

1. Add an explicit active-delta exact source for committed writes not covered by
   a sealed artifact.
2. Add threshold policy for when an active delta should become a sealed flat
   artifact.
3. Ensure sealing is a rebuild over committed rows, not a mutation of query
   graph state in the commit callback.
4. Update the branch-local manifest after successful sealing.
5. Keep sealing best-effort; it must never be required for commit success.

Exit criteria:

1. Fresh writes are searchable immediately through exact active-delta search.
2. Active updates win over older artifact candidates.
3. Active deletes suppress older artifact candidates.
4. Sealed flat sources produce the same results as exact search.
5. Diagnostics separate active-delta, flat artifact, and exact fallback paths.

### 8. Branch Fork Manifest Materialization

1. Create child manifests on fork by copying refs and applying the fork version
   cap.
2. Search inherited refs plus branch-local active delta without copying artifact
   payload bytes.
3. Let parent and child manifests diverge independently after fork.
4. Preserve exact fallback for missing or stale inherited refs.

Exit criteria:

1. Branch manifests select the same visible rows as exact branch reads.
2. Forked branches inherit usable refs without payload copying.
3. Parent writes do not affect child query results after fork.
4. Child writes do not affect parent query results.

### 9. Immutable Source Discovery

1. Add or consume a generic storage source-shape API if available.
2. Discover immutable row sources for a vector collection range.
3. Build flat artifacts per source and write artifact refs into the branch
   manifest instead of one whole-collection artifact when source identity is
   available.
4. Preserve the whole-collection exact fallback when source identity is absent.

Exit criteria:

1. Source-owned flat indexing produces exact-equivalent results.
2. Branch inherited sources can be searched without rebuilding for every child.
3. Cache and durable modes both pass the same behavior tests.

### 10. Durable Artifact Reuse

Status: implemented as the current scaffold.

Durable-local opens now use an internal flat artifact directory outside logical
KV values. Branch-owned manifests still store only refs, checksums, byte counts,
and source facts. Query-time artifact loading can reuse matching durable
payloads. Missing, stale, corrupt, partial, or over-budget payloads are skipped
and the planner falls back to flat or exact committed-row search. Cache mode
remains memory-only. Storage Level 3's generic opaque artifact attachment is
explicitly deferred.

1. Use an engine-owned artifact boundary until storage exposes a generic opaque
   artifact attachment.
2. Serialize flat artifacts with identity and checksum outside logical KV
   values.
3. Store only artifact refs and checksums in the system-space manifest.
4. Load matching artifacts on open or first query.
5. Skip missing, stale, corrupt, partial, or over-budget artifacts and route to
   safe fallback.
6. Record diagnostics for load, miss, corrupt, stale, skip, and fallback.

Exit criteria:

1. Durable reopen can reuse a matching artifact.
2. Corrupt artifact does not corrupt query results.
3. Missing artifact falls back or rebuilds through explicit maintenance.
4. No large graph/flat payload is committed as a normal system-space row.
5. Level 3 atomic source+artifact commit is documented as a future storage
   integration, not assumed by current correctness.

### 11. HNSW Candidate Source

1. Evaluate and add a maintained HNSW dependency.
2. Implement HNSW build and search behind `VectorCandidateSource`.
3. Persist HNSW payloads through the opaque artifact layer or explicitly
   rebuild on open when serialization is not viable.
4. Store HNSW refs in the branch-owned manifest.
5. Enable HNSW only above threshold and within memory budget.
6. Keep exact and flat fallbacks always available.

Exit criteria:

1. HNSW meets recall gates against exact ground truth.
2. Under memory pressure HNSW disables cleanly.
3. No public API changes.

### 12. Benchmarks

Add or extend benchmarks to exercise normal engine APIs:

1. exact scan baseline;
2. flat index query latency;
3. HNSW query latency and recall once enabled;
4. write latency with indexing enabled;
5. delete/update workloads;
6. branch fork search;
7. durable reopen with artifacts.

Benchmark results should report:

1. collection size;
2. dimension;
3. metric;
4. index policy;
5. index kind used;
6. manifest ref count;
7. inherited versus owned ref count;
8. build time;
9. derived bytes;
10. recall@k versus exact;
11. p50/p95/p99 query latency;
12. write throughput.

## Stop Conditions

Stop and revisit the design if any of these occur:

1. flat-indexed query results differ from exact results;
2. branch fork or timestamp reads require index-specific special cases;
3. storage must understand vector semantics;
4. write latency regresses materially before HNSW is enabled;
5. memory-budget enforcement requires process-global mutable state;
6. HNSW cannot meet recall gates without overfetch so high that exact scan is
   competitive;
7. artifact persistence creates recovery risk instead of rebuildable derived
   state;
8. the design requires a long-lived mutable backend to mirror every commit;
9. candidate resolution falls back to collection-wide scans in ordinary indexed
   searches;
10. graph/flat payloads must be stored as normal system-space KV rows to make
    the branch manifest work.

## Definition of Done

The indexing slice is complete when:

1. exact baseline remains available and tested;
2. vector queries route through a candidate-source planner;
3. flat indexing is available for qualifying collections or sources;
4. indexed flat results match exact results;
5. writes, deletes, branch forks, timestamp reads, and metadata filters remain
   correct;
6. branch-owned system-space manifests select inherited and owned refs
   correctly without storing graph payloads as logical KV;
7. diagnostics expose index use and fallback reasons;
8. memory budget can disable or evict indexes without correctness loss;
9. durable artifact persistence is implemented as engine-owned derived state
   with exact fallback, while Storage Level 3 atomic artifact slots are
   explicitly deferred;
10. HNSW is either implemented behind the same source abstraction or left as a
   clearly isolated follow-up with exact/flat foundations in place;
11. old-engine lessons are covered by tests without reviving the old mutable
    backend, post-commit graph mutation, or cache-path coupling.
