# Strata V1 Feature Inventory

Status: Draft product inventory

This document classifies Strata's current and intended product surface for V1.
It sits under `docs/product/strata-v1-product-requirements.md` and should be
read before architecture work on storage-next, engine-next, core-next, executor,
intelligence, inference, or CLI.

The goal is to prevent historical implementation surface from automatically
becoming sacred product scope. Every feature should either earn a place in V1,
be explicitly gated, be deferred, or be removed before it shapes the next
architecture.

## Source Documents

This inventory is based on:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/stratahub-product-direction.md`
3. Current executor command surface in `crates/executor/src/command.rs`
4. Current high-level API surface in `crates/executor/src/compat.rs`
5. Current CLI parser surface in `crates/cli/src/parse.rs`

The executor and CLI are evidence, not the sole product truth.

## Classification Labels

### V1 Required

V1 is incomplete without this feature. Required features must have clear user
semantics, tests, documentation, and failure modes before V1.

### V1 Required Substrate

The product needs the underlying capability in V1, but the full user-facing
experience may launch later. These features usually support portability,
StrataHub, conformance, identity, or future control-plane workflows.

### V1 Optional

The feature may ship in V1 if it is reliable and does not distort core
architecture. Optional features must be feature-gated, clearly documented, or
visibly marked as optional when they depend on extra runtime components.

### Experimental

The feature exists or is plausible, but its product semantics are not stable
enough to drive architecture. Experimental features should not become public V1
promises without a separate design pass.

### Post-V1

The feature is directionally aligned but should not block V1. Architecture
should avoid making it impossible, but V1 should not try to solve it.

### Remove Or Redesign

The feature should not be carried forward as-is. It may be removed, renamed,
collapsed into another feature, or redesigned before V1.

## Inventory Summary

| Area | V1 decision |
| --- | --- |
| Durable local database open | V1 Required |
| Ephemeral cache database | V1 Required |
| Disk-backed cache mode | Remove Or Redesign |
| Read-only open | V1 Required |
| IPC-backed local shared access | V1 Required |
| Storage backend capability contract | V1 Required Substrate |
| Local filesystem backend | V1 Required |
| OpenDAL adapter path | V1 Required Substrate |
| S3-compatible object storage target | V1 Required Substrate |
| Browser/WASM cache target | V1 Required Substrate |
| Every OpenDAL backend production-ready | Remove Or Redesign |
| Adaptive runtime resource profiling | V1 Required |
| Dataset bundle and clone workflow | V1 Required |
| StrataHub Library | Post-V1 product, V1 substrate required |
| StrataHub Fleet | Post-V1 product, V1 substrate required |
| Key-value | V1 Required |
| JSON documents | V1 Required |
| Events | V1 Required |
| Graph basics | V1 Required |
| Graph relationship layer | V1 Required, semantics must be tightened |
| Graph ontology metadata | V1 Required, semantics must be tightened |
| Graph analytics | V1 Optional |
| Vector collections and vector query | V1 Required |
| Search/retrieval | V1 Required |
| Auto-embedding | V1 Optional |
| Model management | V1 Optional |
| Text generation/tokenization | V1 Optional |
| Branches/history/diff/merge | V1 Required |
| Tags and notes | Remove Before V1 |
| Spaces | V1 Required |
| Atomic commit substrate | V1 Required |
| Public transaction commands | Remove Or Redesign |
| Primitive import/export | V1 Optional |
| Legacy branch bundles | Remove Or Redesign |
| Health/metrics/durability counters | V1 Required |
| Manual durability maintenance | Remove Or Redesign |
| Automatic durability maintenance | V1 Required Internal Behavior |
| Config and recipes | V1 Required for retrieval/configuration subset |
| CLI | V1 Required |
| Serializable command boundary | V1 Required Substrate |

## Database Open And Runtime Modes

### Durable Local Open

Decision: V1 Required

Required user outcome:

1. Open or create a durable local Strata database at a filesystem path.
2. Use it without a database server.
3. Recover deterministically after process or machine failure.
4. Close, inspect, and reopen safely.

Current evidence:

1. `Strata::open(path)`
2. `Strata::open_with(path, OpenOptions)`
3. CLI global `--db`
4. Commands: `Info`, `Health`, `Metrics`, `Flush`, `Compact`,
   `DurabilityCounters`, `Describe`

V1 notes:

1. Local filesystem is the reference durable backend.
2. V1 NFRs must define crash-recovery and durability expectations.
3. Open errors must distinguish corruption, lock conflict, unsupported backend,
   invalid configuration, and IO failures.

### Ephemeral Cache Database

Decision: V1 Required

Required user outcome:

1. Open an explicitly ephemeral Strata database.
2. Use the same product model where durable semantics are not required.
3. Avoid confusing cache mode with durable mode.

Current evidence:

1. `Strata::cache()`
2. CLI global `--cache`

V1 notes:

1. Cache means ephemeral.
2. Cache mode should not create WAL, manifest, or durable files.
3. Cache mode should still support the same data and branch model where
   possible.
4. Cache is a storage mode, not a durable commit policy.

### Standard And Always Durability

Decision: V1 Required

Required user outcome:

1. Use `standard` durability for normal durable databases with WAL-backed crash
   recovery and a bounded sync window.
2. Use `always` durability when each acknowledged commit must pass a durability
   barrier.
3. Understand that both modes are durable storage modes, unlike cache.

Current evidence:

1. `DurabilityMode::Standard`
2. `DurabilityMode::Always`
3. `Database::set_durability_mode`

V1 notes:

1. `standard` and `always` are durability policies inside durable storage mode.
2. Runtime switching may be supported between `standard` and `always`.
3. Runtime switching into or out of cache should not be supported.

### Disk-Backed Cache

Decision: Remove Or Redesign

V1 position:

1. Disk-backed cache is not a V1 concept.
2. If a database writes to disk, users should understand whether it is durable
   database mode or a temporary backend with explicitly weaker semantics.
3. Do not reintroduce disk-backed cache as a hidden third mode.

### Read-Only Open

Decision: V1 Required

Required user outcome:

1. Open a database for reads.
2. Reject writes before mutation.
3. Use this mode for inspection, analysis, dataset browsing, and safe tooling.

Current evidence:

1. `AccessMode::ReadOnly`
2. CLI global `--read-only`
3. Session write guard based on `Command::is_write()`

V1 notes:

1. The write classification must be tested as a product contract.
2. Read-only behavior should be consistent across local and portable backends.

### IPC-Backed Local Shared Access

Decision: V1 Required

Required user outcome:

1. A primary local process can own a durable database safely.
2. A second local process, including Strata AI, can access that database through
   IPC instead of opening an unsafe second writer.
3. Users can tell whether a handle is local or IPC-backed.

Current evidence:

1. Product open can return local or IPC-backed handles.
2. CLI includes `up`, `down`, `--follower`, and IPC server paths.
3. `Strata::is_ipc()` exposes whether a handle is IPC-backed.

V1 notes:

1. IPC is the required local multi-process story for V1.
2. IPC must preserve access mode, write rejection, structured errors, and
   command boundary semantics.
3. The final CLI shape may be `up`/`down` or a replacement command, but a
   supported IPC path is required.
4. Follower mode remains remove/redesign; do not keep it as a second local
   sharing mechanism.

## Storage Backends And Portability

### Backend Capability Contract

Decision: V1 Required Substrate

Required user outcome:

1. Users receive explicit errors when a backend cannot support the selected
   durability or concurrency mode.
2. Strata can explain which backend features are available.
3. Portable backends do not smuggle filesystem assumptions into product
   correctness.

V1 capability questions:

1. Does the backend support durable writes?
2. Does it support atomic publish or an equivalent commit protocol?
3. Does it support listing?
4. Does it support compare-and-swap or conditional writes?
5. Does it support locking or a safe single-writer substitute?
6. What are object-size and request-size limits?
7. What are expected latency and consistency classes?
8. Which Strata runtime modes are supported?

V1 notes:

1. This contract belongs to Strata, not OpenDAL.
2. OpenDAL should be an adapter family under this contract.
3. The NFR document must define conformance requirements.

### Local Filesystem Backend

Decision: V1 Required

Required user outcome:

1. The default durable Strata database works on normal local filesystems.
2. The local backend is the reference implementation for durability behavior.
3. Tests use local filesystem behavior to define baseline correctness.

### OpenDAL Adapter Path

Decision: V1 Required Substrate

Required user outcome:

1. Strata architecture can host OpenDAL-backed storage adapters.
2. S3-compatible object storage is the first object-store family to evaluate.
3. Unsupported OpenDAL services fail with explicit capability errors.

V1 notes:

1. OpenDAL does not define Strata's storage contract.
2. V1 does not promise every OpenDAL backend is production-ready.
3. A backend must pass Strata's conformance suite before production claims.

### Browser And WASM Cache Targets

Decision: V1 Required Substrate

Required user outcome:

1. The product model can run in browser or WASM-oriented environments.
2. Cache and durability semantics are explicit.
3. The architecture does not assume POSIX filesystem features in core product
   paths.

V1 notes:

1. Production browser persistence may be optional for V1.
2. The target should still influence backend contracts, identity, errors, and
   testing.

## Dataset Bundles, Clone, And StrataHub Substrate

### Clone Workflow

Decision: V1 Required

Required user outcome:

1. Fetch a portable Strata dataset from a supported source.
2. Place it at a destination selected by the user.
3. Open it with normal Strata APIs.
4. Use it without contacting the source again.

Canonical command shape:

```text
strata clone <source> <destination>
Strata.open("<destination>")
```

Current evidence:

1. Existing branch bundle import/export/validate commands as historical
   implementation evidence.
2. Product requirement for `.strata` dataset bundles.
3. StrataHub direction document.

V1 gaps:

1. Define `.strata` dataset bundle format.
2. Define the relationship between full dataset bundles, database bundles,
   snapshots, and backups.
3. Define clone identity and provenance behavior.
4. Add CLI command.
5. Validate package format, manifest, checksums, declared capabilities, and
   compatibility by default during clone.
6. Add validation and conformance tests.

### Legacy Branch Bundles

Decision: Remove Or Redesign

Current evidence:

1. `BranchExport`
2. `BranchImport`
3. `BranchBundleValidate`
4. CLI `branch export`, `branch import`, `branch validate`

V1 notes:

1. Branch bundles are not the V1 product artifact.
2. V1 clone should be based on `.strata` dataset bundles, not branch-specific
   tarballs.
3. Arrow import/export covers tabular interchange.
4. StrataHub Library covers dataset discovery and distribution.
5. If branch-scoped movement is needed later, it should be redesigned as part of
   branch-aware sync, dataset releases, or StrataHub workflows instead of
   preserving the current branch-bundle surface by inertia.
6. The current commands should be removed, hidden, or explicitly marked legacy
   before V1.

### StrataHub Library

Decision: Post-V1 product, V1 substrate required

V1 substrate required:

1. Dataset identity.
2. Bundle identity.
3. Provenance metadata.
4. Clone workflow.
5. Bundle validation.
6. Export/publish-ready metadata.

Post-V1 product:

1. Dataset pages.
2. Dataset search.
3. Publishing.
4. Forking.
5. Public/private/organization dataset visibility.

### StrataHub Fleet

Decision: Post-V1 product, V1 substrate required

V1 substrate required:

1. Instance identity.
2. Backend identity.
3. Capability report.
4. Health report.
5. Version and format metadata.
6. Optional registration hooks with no hidden telemetry.

Post-V1 product:

1. Fleet registry.
2. Health dashboard.
3. Backup and restore coordination.
4. Sync policies.
5. Governance and audit.

## Data Capabilities

### Key-Value

Decision: V1 Required

Required user outcome:

1. Store, read, delete, list, scan, count, sample, and batch key-value records.
2. Read current values and time-travel values where supported.
3. Inspect version history.
4. Provide documented atomicity for individual writes and batch operations.

Current evidence:

1. `KvPut`, `KvGet`, `KvDelete`, `KvList`, `KvScan`
2. `KvBatchPut`, `KvBatchGet`, `KvBatchDelete`, `KvBatchExists`
3. `KvExists`, `KvGetv`, `KvCount`, `KvSample`

V1 notes:

1. KV is the simplest product capability and should set the quality bar.
2. Version and commit semantics should be documented through KV examples.

### JSON Documents

Decision: V1 Required

Required user outcome:

1. Store structured JSON documents.
2. Read and modify by path.
3. Delete paths or documents.
4. List, count, sample, batch, and inspect history.
5. Support indexes where product semantics are clear.

Current evidence:

1. `JsonSet`, `JsonGet`, `JsonDelete`, `JsonGetv`, `JsonExists`
2. `JsonBatchSet`, `JsonBatchGet`, `JsonBatchDelete`, `JsonList`
3. `JsonCount`, `JsonSample`
4. `JsonCreateIndex`, `JsonDropIndex`, `JsonListIndexes`

V1 notes:

1. JSON path semantics must be documented.
2. JSON secondary indexes are V1 Optional unless the search/retrieval plan
   makes them required.
3. If indexes remain, rebuild and branch semantics must be clear.

### Events

Decision: V1 Required

Required user outcome:

1. Append events.
2. Read by sequence, type, sequence range, and time range.
3. List known event types.
4. Count events.
5. Preserve branch and time-travel semantics.

Current evidence:

1. `EventAppend`, `EventBatchAppend`
2. `EventGet`, `EventExists`, `EventGetByType`
3. `EventLen`, `EventRange`, `EventRangeByTime`
4. `EventListTypes`, `EventList`

V1 notes:

1. Event ordering and timestamp semantics must be precise.
2. Batch append should have atomic behavior or explicit partial-failure rules.

### Vectors

Decision: V1 Required

Required user outcome:

1. Create and delete vector collections.
2. Upsert, read, delete, count, batch, and sample vector records.
3. Query nearest neighbors.
4. Store metadata with vectors.
5. Use time-travel reads where supported.

Current evidence:

1. `VectorCreateCollection`, `VectorDeleteCollection`, `VectorListCollections`
2. `VectorUpsert`, `VectorGet`, `VectorDelete`, `VectorGetv`, `VectorExists`
3. `VectorQuery`, `VectorCollectionStats`, `VectorCount`
4. `VectorBatchUpsert`, `VectorBatchGet`, `VectorBatchDelete`,
   `VectorSample`

V1 notes:

1. Exact vector index implementation is architecture detail.
2. Product semantics for metric, dimension, metadata filter, branch behavior,
   and collection lifecycle are required.
3. Collection create/delete atomicity and failure behavior must be documented
   clearly.

### Graph Basics

Decision: V1 Required

Required user outcome:

1. Create, delete, list, and inspect graphs.
2. Add, read, remove, list, and page nodes.
3. Attach properties, optional entity references, and optional ontology object
   types to nodes.
4. Add and remove typed, weighted, property-bearing edges.
5. Query neighbors and run bounded traversal.
6. Use branch, space, and time-travel semantics where supported.
7. Use graph as a relationship layer over other Strata data without duplicating
   source payloads into graph node properties.

Current evidence:

1. `GraphCreate`, `GraphDelete`, `GraphList`, `GraphGetMeta`
2. `GraphAddNode`, `GraphGetNode`, `GraphRemoveNode`
3. `GraphListNodes`, `GraphListNodesPaginated`
4. `GraphAddEdge`, `GraphRemoveEdge`, `GraphNeighbors`
5. `GraphBulkInsert`, `GraphBfs`

V1 notes:

1. Graph CRUD and traversal are core to the integrated product model.
2. Bulk insert is V1 Optional unless the user pathways require it.
3. Graph delete and bulk insert atomicity and failure behavior must be
   documented.
4. Entity references should be documented as links from graph nodes back to
   other stored data, not as a separate identity system or a payload-copy
   requirement.
5. Traversal limits, direction semantics, edge-type filters, and time-travel
   behavior must be precise.
6. The graph relationship-layer direction is captured in
   `docs/product/strata-v1-graph-relationship-layer.md`.

### Graph Relationship Layer

Decision: V1 Required, semantics must be tightened

Required user outcome:

1. Connect KV records, JSON documents, events, vector records, and graph-native
   nodes through graph relationships.
2. Avoid copying source payloads into graph node properties just to make
   relationships possible.
3. Traverse from graph nodes back to the Strata records they represent.
4. Use relationship context in graph-aware retrieval where recipes are
   configured.
5. Preserve branch, space, and version context for relationships.

Current evidence:

1. Graph node `entity_ref` fields.
2. Reverse entity-reference indexes.
3. Graph referential-integrity hooks.
4. Graph-aware search boosting.

V1 notes:

1. `entity_ref` should become a typed `EntityRef` contract instead of remaining
   an undocumented string convention.
2. Graph node identity and referenced entity identity must be distinct.
3. Relationship-layer support should not remove standalone graph usage.
4. Deletion and missing-reference behavior must be explicit.
5. The detailed product direction is
   `docs/product/strata-v1-graph-relationship-layer.md`.

### Graph Ontology

Decision: V1 Required, semantics must be tightened

Required user outcome:

1. Define and inspect object types.
2. Define and inspect link types.
3. Freeze or otherwise stabilize ontology metadata when needed.
4. Query ontology status and summary.
5. List nodes by ontology type.

Current evidence:

1. `GraphDefineObjectType`, `GraphGetObjectType`,
   `GraphListObjectTypes`, `GraphDeleteObjectType`
2. `GraphDefineLinkType`, `GraphGetLinkType`, `GraphListLinkTypes`,
   `GraphDeleteLinkType`
3. `GraphFreezeOntology`, `GraphOntologyStatus`,
   `GraphOntologySummary`, `GraphListOntologyTypes`, `GraphNodesByType`

V1 notes:

1. Ontology metadata is useful for AI orientation and graph structure.
2. The V1 product must define whether ontology is validation, documentation, or
   both.
3. Ontology mutation atomicity and failure behavior must be redesigned or
   documented.
4. The current surface has draft/frozen lifecycle behavior. V1 should decide
   whether freezing is a user-facing concept or an internal safety mechanism.
5. Ontology summaries are important for agent and retrieval workflows because
   they let a caller understand graph shape without scanning every node and edge.
6. `GraphNodesByType` makes ontology operational, not just descriptive, so type
   assignment and type deletion semantics must be clear.

### Graph Analytics

Decision: V1 Optional

Required user outcome:

1. Identify weakly connected components.
2. Detect communities with label propagation.
3. Rank important nodes with PageRank.
4. Score local clustering.
5. Compute single-source shortest paths.

Current evidence:

1. `GraphWcc`
2. `GraphCdlp`
3. `GraphPagerank`
4. `GraphLcc`
5. `GraphSssp`

V1 notes:

1. Analytics are real product capabilities, but they should not drive the core
   storage or engine model.
2. These commands can be V1 Optional if deterministic, documented, bounded, and
   tested.
3. Analytics should return compact summaries by default and expose full results
   only when explicitly requested.
4. Direction, weighting, convergence, iteration limits, top-N output, and
   include-all behavior need stable documentation before V1.
5. Graph analytics are local in-process computations over stored graph state,
   not a promise to replace a specialized distributed graph analytics system.
6. If they are not mature, mark them experimental or feature-gated.

## Search, Retrieval, And Intelligence

### Search And Retrieval

Decision: V1 Required

Required user outcome:

1. Run keyword search across stored text through a product-level retrieval API.
2. Run semantic and hybrid retrieval where embeddings are configured.
3. Use graph-aware retrieval where graph data and graph recipes are configured.
4. Configure retrieval behavior through named recipes without understanding
   internal indexes.
5. Use branch, space, time-travel, and diff-aware retrieval where supported.

Current evidence:

1. `Search`
2. `SearchQuery`
3. `RecipeGetDefault`, `RecipeGet`, `RecipeSet`, `RecipeList`,
   `RecipeDelete`, `RecipeSeed`
4. CLI `search` and `recipe`

V1 notes:

1. Search is part of the integrated product model.
2. Built-in recipes currently include `keyword`, `semantic`, `hybrid`,
   `default`, `graph`, and `rag`.
3. The V1 search contract should define which data capabilities are searched by
   default and which require explicit recipe configuration.
4. Search recipes should be product-facing only if users can understand and
   safely edit them.
5. Recipe-controlled retrieval includes BM25, vector retrieval, graph retrieval,
   fusion, query expansion, reranking, result transforms, RAG prompt behavior,
   model routing, version output, and execution budgets.
6. Search quality features that require models must degrade honestly when model
   runtime support is unavailable.

### Auto-Embedding

Decision: V1 Optional

Current evidence:

1. `ConfigSetAutoEmbed`
2. `AutoEmbedStatus`
3. `EmbedStatus`
4. `ReindexEmbeddings`
5. High-level `Strata::set_auto_embed` and `Strata::embed_status`

V1 notes:

1. Auto-embedding is valuable but should be explicit.
2. It must not create hidden model, network, or background-runtime dependency.
3. Auto-generated embeddings are branch-local shadow data in system space, not
   user-authored records.
4. If shipped, indexing status, failure handling, and replay/reindex behavior
   must be observable.
5. Space deletion, branch movement, clone, and recovery must keep shadow
   embeddings coherent with the user data they represent.

### Query Expansion And Reranking

Decision: V1 Optional

Current evidence:

1. Recipe `expansion` configuration with lex, vector, and HyDE-style variants.
2. Recipe `fusion` configuration.
3. Recipe `rerank` configuration.
4. Search stats expose expansion and rerank usage where applicable.

V1 notes:

1. Expansion and reranking are search-quality features, not required database
   capabilities.
2. They require explicit model/runtime support and should degrade to ordinary
   retrieval when unavailable.
3. Expansion cache entries are system-space, branch-scoped implementation data
   and must not become user-visible records.
4. Search stats should report when expansion or reranking was used and which
   model was selected.

### Retrieval-Augmented Answers

Decision: V1 Optional

Current evidence:

1. Recipe `prompt` enables RAG-style answer generation.
2. Recipe `rag_context_hits` and `rag_max_tokens` bound answer context.
3. Search output can include both hits and an answer.
4. Search stats expose RAG usage, model, elapsed time, and token counts where
   available.

V1 notes:

1. RAG is an intelligence utility layered on retrieval, not a replacement for
   returning source hits.
2. If answer generation fails or model runtime support is unavailable, search
   should still return retrieval hits when possible.
3. Answers must be grounded in retrieved context and should expose source
   references.
4. Provider/network use must be explicit.

### Embedding API

Decision: V1 Optional

Current evidence:

1. `Embed`
2. `EmbedBatch`

V1 notes:

1. Embedding can ship if model/runtime requirements are explicit.
2. It should not be required for basic vector usage.

### Model Management

Decision: V1 Optional

Current evidence:

1. `ModelsList`
2. `ModelsLocal`
3. `ModelsPull`
4. `ConfigureModel`
5. `ConfigureSet`, `ConfigureGetKey`, `ConfigGet`

V1 notes:

1. Model management is useful for batteries-included local AI workflows.
2. It should be feature-gated or clearly optional.
3. API keys and model provider config must preserve sensitive-data rules.

### Text Generation And Tokenization

Decision: V1 Optional

Current evidence:

1. `Generate`
2. `Tokenize`
3. `Detokenize`
4. `GenerateUnload`

V1 notes:

1. Generation, tokenization, detokenization, and unload are intentional
   intelligence utilities when a compatible inference runtime is configured.
2. These commands should call the inference layer rather than embedding model
   execution logic in executor, engine, or storage.
3. They must stay separate from storage and engine correctness claims. Missing
   models, feature-disabled builds, provider failures, and runtime load failures
   are intelligence-runtime errors, not database durability errors.
4. Default CLI visibility should match the compiled product. A binary that
   cannot execute generation commands should hide them or clearly mark the
   required feature/runtime support rather than presenting them as ordinary
   commands that only return `NotImplemented`.
5. Network access and provider selection must be explicit. Local generation and
   remote-provider generation need clear user-visible behavior.

## Branching And Versioning

### Branch Lifecycle

Decision: V1 Required

Required user outcome:

1. Create a branch from existing database state.
2. Create, inspect, list, check, and delete branches.
3. Use branches as a normal part of local development and dataset
   experimentation.

Current evidence:

1. `BranchCreate`, `BranchGet`, `BranchList`, `BranchExists`,
   `BranchDelete`, `BranchFork`
2. CLI `branch create`, `branch info`, `branch list`, `branch exists`,
   `branch del`, `branch fork`

V1 notes:

1. Default branch behavior must be explicit.
2. Branch delete must have clear safety rules.
3. Product language should say "create branch from" or "create workspace from"
   instead of teaching "fork" as the primary concept.
4. The dedicated branching direction is
   `docs/product/strata-v1-branching-direction.md`.

### Compare, Promote, Copy, And Undo

Decision: V1 Required

Required user outcome:

1. Compare branches by space and data capability.
2. Preview promotion conflicts.
3. Promote one branch into another when conflicts can be resolved.
4. Copy selected records or selected changes between branches.
5. Undo a version range by writing a compensating change.

Current evidence:

1. `BranchDiff`, `BranchDiffThreeWay`, `BranchMergeBase`, `BranchMerge`
2. `BranchRevert`, `BranchCherryPick`

V1 notes:

1. This is a core product differentiator.
2. V1 must document merge strategies, conflict behavior, and data-capability
   coverage.
3. If any data capability has incomplete comparison or promotion behavior, the
   limitation must be explicit.
4. `BranchMergeBase` should be an explanation or diagnostic detail, not a
   primary user pathway.
5. Product language should prefer compare, promote, copy selected changes, and
   undo over diff, merge, cherry-pick, and revert.

### Tags And Notes

Decision: Remove Before V1

Current behavior:

1. Users can create, delete, list, and resolve branch-scoped tags.
2. Users can attach one note to a branch version, list notes, and delete notes.
3. Tags and notes are stored as system-branch metadata rows.

Current evidence:

1. `TagCreate`, `TagDelete`, `TagList`, `TagResolve`
2. `NoteAdd`, `NoteGet`, `NoteDelete`

V1 notes:

1. Tags and notes are not critical to V1 user pathways.
2. Strata branches are data timelines, not source-control collaboration branches,
   so Git-style tags and notes should not be carried forward by default.
3. Dataset releases, provenance, and StrataHub metadata may reintroduce a better
   version-labeling concept later.
4. Remove public tag/note commands before V1 unless a concrete V1 pathway
   depends on them.

## Spaces

Decision: V1 Required

Required user outcome:

1. Create, list, check, and delete spaces.
2. Use spaces as logical namespaces inside a database and branch.
3. Carry space context through CLI, session, and command APIs.

Current evidence:

1. `SpaceList`, `SpaceCreate`, `SpaceDelete`, `SpaceExists`
2. `Strata::current_space`
3. CLI `use <branch> <space>` meta command
4. Optional `space` fields on data commands

V1 notes:

1. The relationship between database, branch, space, and data capability must be
   documented.
2. Space delete semantics must be safe and understandable.

## Atomic Commits And Write Batches

Decision: V1 Required substrate; public transaction commands should be removed
or redesigned before V1.

Required user outcome:

1. Each supported write has a clear commit boundary.
2. Batch APIs document whether they are all-or-nothing or may partially succeed.
3. Reads observe committed versions consistently.
4. Conflicts, durability failures, and unsupported backend capabilities produce
   clear errors.
5. Users do not need to manage public begin/commit/rollback transaction state.

Current evidence:

1. Internal engine/storage commit machinery.
2. Public commands currently include `TxnBegin`, `TxnCommit`, `TxnRollback`,
   `TxnInfo`, and `TxnIsActive`.
3. Session state currently tracks transaction branch matching.
4. Batch APIs exist for KV, JSON, events, vectors, and graph bulk operations.

V1 decision:

1. Remove public transaction commands from the default V1 product surface.
2. Keep internal commit-unit machinery where the engine/storage layers need it.
3. Document atomicity and partial-failure behavior per write and batch API.
4. Do not claim ACID compliance unless the exact backend, durability mode,
   isolation behavior, and test suite are defined.

V1 notes:

1. Backend capability profiles must say which commit guarantees are available.
2. The NFR document must define crash safety, read consistency, conflict, and
   durability guarantees for individual writes and batch APIs.
3. A future public multi-operation transaction API can be reconsidered only after
   the V1 commit contract is proven.

## Import, Export, And Data Movement

### Primitive Import And Export

Decision: V1 Optional

Current evidence:

1. `DbExport`
2. `ArrowImport`
3. CLI `export`
4. CLI `import`
5. Formats include JSON, JSONL, CSV, Parquet, and Arrow-oriented import paths.

V1 notes:

1. Primitive import/export is valuable for interoperability.
2. The minimum V1 data movement requirement is clone and bundle semantics.
3. Primitive import/export can be optional if bundle movement is solid.
4. If kept, each format needs golden tests.

### Bundle Validation

Decision: V1 Required Guarantee

Required product behavior:

1. Clone and import validate artifacts before trusting or installing them.
2. Users receive useful compatibility errors without needing to run a separate
   command first.
3. Validation failures do not produce partial import, partial clone, or silent
   corruption.

Current evidence:

1. `BranchBundleValidate` exists today as historical evidence.

V1 notes:

1. Bundle validation should become a general dataset-bundle guarantee, not a
   branch-bundle command.
2. A manual validation command may exist later for debugging or CI, but it
   should not be the normal user pathway.
2. Validation is required for StrataHub Library trust.

## Operations And Inspection

### Health, Metrics, And Description

Decision: V1 Required

Required user outcome:

1. Check liveness.
2. Inspect database metadata.
3. Inspect health.
4. Inspect metrics.
5. Describe data shape for human and agent orientation.

Current evidence:

1. `Ping`
2. `Info`
3. `Health`
4. `Metrics`
5. `Describe`
6. `TimeRange`

V1 notes:

1. Health and metrics should not leak private record data by default.
2. Describe is important for agent and CLI workflows, but must stay bounded and
   privacy-aware.

### Durability Maintenance

Decision: V1 Required Internal Behavior; Public Surface Remove Or Redesign

Required product behavior:

1. Strata automatically handles flush, compaction, checkpointing, retention, and
   other storage maintenance needed for normal operation.
2. Users can inspect durability state and health without manually driving
   maintenance.
3. Manual maintenance commands, if retained, are admin/debug tools and not the
   normal product pathway.

Current evidence:

1. `Flush`
2. `Compact`
3. `DurabilityCounters`

V1 notes:

1. Cache mode should report durability counters in a clearly distinct way.
2. Portable backends need capability-specific automatic maintenance behavior.
3. Public flush, compact, checkpoint, or retention-apply commands should be
   removed, hidden, or clearly marked as diagnostics before V1.

### Retention

Decision: V1 Required Internal Behavior; Public Surface Remove Or Redesign

Current evidence:

1. `RetentionApply`
2. `RetentionStats`
3. `RetentionPreview`
4. Current command comments indicate stats and preview may be unimplemented.

V1 notes:

1. Retention is part of the database's automatic durability and space-management
   behavior, not a normal user chore.
2. Existing public retention commands must either become bounded diagnostics or
   be removed from the public surface.
3. Retention must be safe around branches, snapshots, bundles, clone, and
   recovery without requiring users to trigger it manually.

## Configuration And Recipes

### Core Configuration

Decision: V1 Required for supported runtime settings

Current evidence:

1. `ConfigGet`
2. `ConfigureSet`
3. `ConfigureGetKey`
4. `ConfigSetAutoEmbed`
5. `AutoEmbedStatus`

V1 notes:

1. Configuration keys should not be an unstructured dumping ground.
2. V1 should define typed configuration groups.
3. Secrets must preserve redaction and privacy rules.

### Search Recipes

Decision: V1 Required if search recipes remain user-facing

Current evidence:

1. `RecipeSet`
2. `RecipeGet`
3. `RecipeGetDefault`
4. `RecipeList`
5. `RecipeDelete`
6. `RecipeSeed`

V1 notes:

1. Recipes are useful for search/retrieval workflows.
2. Built-in recipes currently include `keyword`, `semantic`, `hybrid`,
   `default`, `graph`, and `rag`.
3. Built-in recipes and user overrides need clear precedence rules.
4. Recipes must not become an unbounded implementation escape hatch. If the raw
   JSON schema is too implementation-shaped, V1 should expose friendly recipe
   presets and keep raw recipe editing as advanced configuration.

## CLI And Command Boundary

### CLI

Decision: V1 Required

Required user outcome:

1. Open a database from the command line.
2. Use branch and space context.
3. Run core data, branch, search, import/export, inspection, and configuration
   commands.
4. Use JSON output for scripts and automation.
5. Use `strata clone <source> <destination>` for dataset cold start.

Current evidence:

1. CLI commands for KV, JSON, event, vector, graph, branch, space, current
   transaction surface, search, config, recipe, model, generation, export,
   import, health, metrics, flush, compact, and describe. Flush and compact are
   current implementation evidence, not required V1 user pathways.
2. REPL meta commands: `use`, `help`, `clear`, `quit`.
3. Render modes: human, JSON, raw.

V1 gaps:

1. Add clone.
2. Decide the final CLI shape for required IPC access, whether `up`/`down` or a
   replacement command.
3. Align default CLI help with compiled intelligence support so generation and
   model commands are either executable or visibly feature-gated.

### Serializable Command Boundary

Decision: V1 Required Substrate

Current evidence:

1. `Command` is self-contained, serializable, typed, and pure data.
2. `Output` is typed per command.
3. CLI and IPC use the command boundary.

V1 notes:

1. This is useful for CLI, IPC, language bindings, scripts, tests, and agents.
2. The command boundary should be treated as a product contract only after the
   feature inventory cut line is applied.
3. Experimental commands should not accidentally become stable because they are
   serializable today.

## Security, Privacy, And Network Behavior

### Sensitive Configuration

Decision: V1 Required

Required user outcome:

1. API keys and secrets do not leak through debug output, display output, health
   reports, bundle metadata, or StrataHub metadata.
2. Sensitive fields are explicit and redacted.

### Network Access

Decision: V1 Required policy

Required user outcome:

1. Strata does not upload data by default.
2. Strata does not register with StrataHub by default.
3. Strata does not call model providers by default.
4. Any network behavior is explicit and visible.

V1 notes:

1. This applies to OpenDAL, StrataHub, model providers, embedding providers, and
   future sync.
2. The NFR document must define privacy and telemetry requirements.

## Remove Or Redesign Before V1

These surfaces should not be carried forward casually:

1. Disk-backed cache mode.
2. Any hidden filesystem assumption in portable storage paths.
3. A public claim that all OpenDAL backends are production-ready.
4. Public transaction commands and transaction exclusions that appear only as
   runtime surprises.
5. Manual flush, compact, checkpoint, or retention-apply commands as normal user
   workflows.
6. Retention stats and preview commands if they remain unimplemented or too
   low-level for bounded diagnostics.
7. Branch bundle commands as a V1 product surface.
8. Generation/model commands that appear as default commands while imposing
   hidden runtime, feature, provider, or network assumptions.
9. Public tag/note commands as a V1 product surface.
10. State-cell as an advertised V1 primitive unless a real V1 product surface is
   designed and implemented.
11. IPC/server management as the only way to use Strata locally.
12. Any StrataHub behavior that uploads, registers, or syncs without explicit
   user action.

## Required Follow-Up Decisions

The user pathways and NFR documents must resolve:

1. Which CLI commands are part of the default V1 experience?
2. What is the exact `.strata` dataset bundle format?
3. Is any branch-scoped movement retained as an internal tool, or are current
   branch bundle commands removed before V1?
4. What identity is minted or preserved during clone?
5. What commit and batch atomicity guarantees are required for V1?
6. Which graph ontology semantics are validation vs. documentation?
7. Which graph analytics ship in V1?
8. Which intelligence features are default, optional, feature-gated, or
   deferred?
9. What backend conformance suite is required before OpenDAL/S3/browser targets
   are documented as production-ready?
10. What retention behavior is required for clone, bundles, branches, snapshots,
    and recovery?
11. Which errors and outputs become stable product contracts?
12. What privacy guarantees apply to describe, health, metrics, bundles,
    provenance, and future StrataHub metadata?
