# Engine Crate Map

Current state as of 2026-05-10.

This is an internal evidence map for `crates/engine`. It describes the crate as
it exists now, after the storage-boundary cleanup and the consolidation of graph,
vector, search, security/open options, and product-open behavior into engine.

This document is not an engine-next architecture proposal. Its job is to make
the current crate navigable before the next architecture pass.

## Executive Summary

`strata-engine` is currently the semantic and runtime center of Strata. It is
not a thin wrapper around storage. It owns:

- database open, lifecycle, runtime configuration, health, shutdown, and
  recovery policy
- product open policy and built-in runtime composition
- transaction coordination, commit ordering, write backpressure, and manual
  transaction surfaces
- branch lifecycle, branch DAG records, fork, merge, diff, cherry-pick, revert,
  and branch retention reporting
- KV, JSON, event, graph, vector, and search behavior
- search recipes, deterministic retrieval, BM25, vector retrieval integration,
  graph search integration, expansion/rerank prompt machinery, and system recipe
  storage
- snapshot decode, checkpoint collection, and primitive-shaped recovery adapters
  around storage-owned mechanics
- branch-bundle import/export machinery
- engine-level public error taxonomy

That breadth is the main risk. The crate is coherent at the dependency level
because upper crates no longer bypass engine to talk to storage, but internally
engine has several distinct domains sharing one public surface.

## Size Snapshot

Source inventory:

- `149` Rust source files under `crates/engine/src`
- `21` Rust integration test files under `crates/engine/tests`
- `138,783` source lines under `crates/engine/src` by `wc -l`

Top-level source-file distribution:

| Area | File Count |
|---|---:|
| `database/` | 34 |
| `vector/` | 23 |
| `graph/` | 22 |
| `search/` | 21 |
| root files | 12 |
| `primitives/` | 10 |
| `bundle/` | 6 |
| `semantics/` | 5 |
| `transaction/` | 5 |
| `branch_ops/` | 5 |
| `branch_domain/` | 3 |
| `recovery/` | 2 |
| `branch_retention/` | 1 |

Largest files by line count:

| File | Lines | Meaning |
|---|---:|---|
| `branch_ops/mod.rs` | 6,212 | branch diff, merge, cherry-pick, revert, tags/notes, graph merge handling |
| `search/index.rs` | 5,208 | inverted index and search indexing behavior |
| `vector/store/mod.rs` | 4,180 | vector store facade, backend state, collection/search coordination |
| `error.rs` | 3,856 | public error taxonomy and conversion tests |
| `primitives/json/mod.rs` | 3,823 | JSON document API, path operations, indexing |
| `branch_ops/branch_control_store.rs` | 3,423 | branch control records and lifecycle metadata |
| `semantics/json.rs` | 3,393 | JSON path, patch, value semantics |
| `vector/segmented.rs` | 3,184 | segmented HNSW backend |
| `vector/hnsw.rs` | 3,056 | HNSW implementation |
| `graph/branch_dag.rs` | 2,974 | branch DAG graph record model and operations |

## Dependency Shape

Direct normal workspace dependencies:

```text
strata-engine
├── strata-core
└── strata-storage
```

Direct normal non-workspace dependencies include:

```text
base64, bincode, byteorder, chrono, crc32fast, dashmap, fs2, libc,
memmap2, once_cell, parking_lot, rand, rayon, rmp-serde, serde,
serde_json, sha2, smallvec, tar, thiserror, toml, tracing,
unicode-segmentation, uuid, xxhash-rust, zeroize, zstd
```

Crate features:

| Feature | Current Meaning |
|---|---|
| `perf-trace` | enables optional performance tracing hooks |
| `embed` | compiles engine-side auto-embedding integration points |
| `test-support` | exposes selected test hooks |

Important dependency facts:

- `strata-engine` is the only normal production crate above storage that should
  consume storage directly.
- `strata-executor` and `strata-intelligence` consume engine.
- `strata-cli` and root `stratadb` consume executor.
- Inference/model provider code remains above engine through intelligence.
- Retired peer crates for graph, vector, search, security, and legacy executor
  are no longer normal engine dependencies.

## High-Level Module Shape

```text
crates/engine/src
├── lib.rs                      public module declarations and root re-exports
├── error.rs                    public StrataError model and code mapping
├── database/                   Database runtime, open/lifecycle/recovery/config
├── branch_domain.rs            branch domain re-export shell
├── branch_domain/              branch identity, lifecycle, DAG metadata
├── branch_ops/                 fork/merge/diff/revert/cherry-pick workflows
├── branch_retention/           branch-retention summary helpers
├── primitives/                 KV, JSON, event, branch, space facades
├── semantics/                  pure-ish event/json/value/vector semantics
├── graph/                      engine-owned graph primitive and relationship layer
├── vector/                     engine-owned vector primitive and ANN runtimes
├── search/                     engine-owned retrieval/index/recipe runtime
├── transaction/                Transaction wrappers, pooling, JSON txn state
├── recovery/                   Subsystem trait and lifecycle composition hook
├── bundle/                     branch bundle export/import format and workflow
├── coordinator.rs              transaction coordinator over storage manager
├── transaction_ops.rs          cross-primitive transaction trait
├── background.rs               background task scheduler
├── recipe_store.rs             recipe persistence in _system_ space
├── system_space.rs             reserved internal namespace helpers
├── sensitive.rs                zeroizing sensitive-string wrapper
├── limits.rs                   limits and limit errors
├── instrumentation.rs          operation timing metrics
└── test_path_key.rs            test-hook path normalization helper
```

Dependency direction inside the crate is roughly:

```text
lib.rs
  ├── database
  │   ├── storage adapters
  │   ├── coordinator
  │   ├── recovery/subsystem lifecycle
  │   └── branch/primitives/search/vector/graph hooks
  ├── primitives
  │   ├── transaction/context
  │   ├── semantics
  │   └── search interfaces
  ├── graph
  │   ├── database transaction APIs
  │   ├── storage keys
  │   └── branch DAG / merge hooks
  ├── vector
  │   ├── database transaction APIs
  │   ├── storage keys
  │   ├── sidecar index files
  │   └── commit/abort/replay observers
  ├── search
  │   ├── primitives
  │   ├── graph/vector facades
  │   ├── recipe_store
  │   └── deterministic retrieval substrate
  ├── branch_ops
  │   ├── database/storage scans
  │   ├── graph/vector merge handlers
  │   └── branch_domain records
  └── bundle
      ├── database scans
      ├── storage key/type tags
      └── tar/zstd branch archive format
```

## Public Surface

The root public surface is concentrated in `lib.rs` and is broad. It re-exports
implementation, product, domain, and lower storage-facing types from the same
namespace.

Current public groups:

| Group | Representative Exports |
|---|---|
| Database/open | `Database`, `AccessMode`, `OpenOptions`, `open_product_database`, `open_product_cache`, `ProductOpenOutcome`, `OpenSpec`, `DatabaseMode` |
| Config/profile | `StrataConfig`, `StorageConfig`, `ModelConfig`, `SensitiveString`, `Profile`, `HardwareInfo`, hardware-profile helpers |
| Errors | `StrataError`, `ErrorCode`, `ErrorDetails`, `PrimitiveDegradedReason`, `StrataResult` |
| Health/recovery | `HealthReport`, `SubsystemHealth`, `RecoveryError`, `RecoveryHealth`, `DegradationClass`, lossy recovery/reporting types |
| Branch domain | `BranchRef`, `BranchControlRecord`, `BranchGeneration`, `ForkRecord`, `MergeRecord`, `CherryPickRecord`, `RevertRecord`, branch DAG/status types |
| Branch services | `BranchService`, `ForkOptions`, `MergeOptions`, `RetentionReport`, branch-retention entries |
| Transactions | `Transaction`, `ScopedTransaction`, `TransactionPool`, `TransactionContext`, `TransactionOps` |
| Primitives | `KVStore`, `JsonStore`, `EventLog`, `SpaceIndex`, branch handles, extension traits |
| JSON/event/value semantics | `JsonPath`, `JsonPatch`, `JsonValue`, path helpers, event chain verification, text extraction |
| Graph | `GraphStore`, `PrimitiveGraphStore`, `GraphSubsystem`, graph types and helpers |
| Vector | `VectorStore`, `VectorConfig`, vector indexes, HNSW, segmented backend, quantization, filter/search types |
| Search | `SearchRequest`, `SearchResponse`, `SearchHit`, `SearchSubsystem`, `Searchable`, BM25/index/recipe/retrieval types |
| Bundle | branch bundle reader/writer/types, export/import info |
| Storage re-exports | `DurabilityMode`, `WalCounters`, `StorageIterator`, `VersionedEntry`, `DegradationClass`, `RecoveryHealth` |

The current public surface is useful for compatibility, but it also exposes
types from several layers that engine-next should probably separate:

- product APIs
- engine semantic APIs
- low-level runtime/test APIs
- storage-derived compatibility types

## Database Runtime Cluster

Directory: `crates/engine/src/database`

Top-level files:

```text
branch_mutation.rs
branch_service.rs
compaction.rs
compat.rs
config.rs
dag_hook.rs
lifecycle.rs
merge_registry.rs
mod.rs
observers.rs
open.rs
open_options.rs
primitive_degradation.rs
product_open.rs
profile.rs
recovery.rs
recovery_error.rs
refresh.rs
registry.rs
retention_report.rs
snapshot_install.rs
spec.rs
test_hooks.rs
transaction.rs
```

Responsibilities:

- `mod.rs` defines `Database`, major runtime fields, health types, disk usage,
  lossy recovery report types, observer registries, extension storage, and
  getters.
- `open.rs` owns primary, follower, cache, and low-level runtime open paths. It
  creates or reuses `Database`, applies hardware profiles, wires storage runtime
  config, opens WAL writers, runs recovery, registers lock files, and spawns WAL
  flush threads.
- `product_open.rs` owns product open policy. It composes graph, vector, and
  search subsystems, handles default-branch creation, seeds built-in recipes,
  and classifies primary-lock failures into local vs IPC outcomes.
- `spec.rs` defines `OpenSpec` and `DatabaseMode`. It still exposes low-level
  subsystem installation hooks for engine internals and tests.
- `config.rs` owns serialized `strata.toml` config, inference/generation
  provider fields, storage resource knobs, storage-runtime adapter creation,
  durability string parsing, snapshot retention settings, and environment
  overrides for secrets.
- `profile.rs` owns hardware-profile detection and default config adjustment.
- `recovery.rs` orchestrates storage recovery, snapshot install callbacks,
  degraded-storage policy, lossy recovery reporting, transaction-coordinator
  bootstrap, follower persisted-state restore, and watermarks.
- `snapshot_install.rs` decodes primitive snapshot sections and hands generic
  decoded rows to storage for install.
- `compaction.rs` owns public flush/checkpoint/compact/disk-usage methods and
  adapts storage-owned durability helpers into `Database` behavior.
- `transaction.rs` owns write backpressure, memtable flush scheduling,
  compaction scheduling, transaction begin/commit/abort helpers, branch
  generation guards, WAL append, observer notification, and write stall policy.
- `lifecycle.rs` owns GC, maintenance, snapshot pruning, follower refresh
  administration, shutdown, drop-time freeze behavior, and open-registry cleanup.
- `refresh.rs` defines follower refresh hooks, blocked-state records,
  watermarks, persistence of follower refresh state, and refresh outcomes.
- `branch_service.rs` and `branch_mutation.rs` provide branch-facing operations
  over branch control records and branch operation workflows.
- `dag_hook.rs` and `merge_registry.rs` are engine extension points for graph
  DAG projection and graph/vector merge behavior.
- `observers.rs` defines commit, abort, replay, branch-operation, and primitive
  degradation observers. Vector/search/graph use these for derived-state
  correctness.
- `compat.rs` computes runtime compatibility signatures for single-instance
  path reuse.
- `retention_report.rs` reports branch retention, orphan storage, reclaim
  status, and degraded primitive information.
- `primitive_degradation.rs` records primitive-level fail-closed degradation
  state.
- `registry.rs` owns the process-local open database registry.
- `test_hooks.rs` exposes fault injection hooks under test/test-support gates.

Current shape:

```text
Database open
  -> config/profile
  -> storage recovery
  -> subsystem recover/init/bootstrap
  -> default branch / recipe seed
  -> transaction + observer runtime

Database write
  -> TransactionCoordinator
  -> storage TransactionContext
  -> engine primitive writes
  -> WAL/storage commit
  -> commit/abort observers
  -> background flush/compaction

Database maintenance
  -> GC/TTL
  -> checkpoint/snapshot
  -> WAL compaction
  -> snapshot pruning
  -> subsystem freeze
```

Important tensions:

- `database/` mixes product open, internal open, storage adapters, public
  config, lifecycle policy, follower refresh, and health reporting.
- `OpenSpec::with_subsystem` remains a low-level subsystem instantiation hook.
  Product open now owns the default graph/vector/search composition, but the
  low-level hook still exists for tests and utilities.
- Follower mode remains deeply wired through open, recovery, refresh,
  lifecycle, health, and tests even though product direction favors IPC for
  multi-user access.
- Manual transaction APIs still exist as public engine surfaces even though the
  product direction is operation-level APIs for users.

## Transaction And Commit Cluster

Files:

```text
coordinator.rs
transaction_ops.rs
transaction/context.rs
transaction/json_state.rs
transaction/owned.rs
transaction/pool.rs
database/transaction.rs
```

Responsibilities:

- `coordinator.rs` wraps storage's transaction manager with active transaction
  tracking, transaction IDs, commit versions, GC safe points, write-buffer
  limits, timeout policy, metrics, and durable commit helpers.
- `database/transaction.rs` provides the database-level transaction lifecycle:
  accepting-state checks, begin/commit/abort, pooled context handling,
  backpressure, WAL interaction, flush scheduling, compaction scheduling,
  branch-generation validation, observer notification, and `DurableButNotVisible`
  handling.
- `transaction/context.rs` wraps `strata_storage::TransactionContext` with
  primitive-aware convenience methods and JSON snapshot behavior.
- `transaction/owned.rs` provides owned transaction handles.
- `transaction/pool.rs` pools transaction contexts.
- `transaction/json_state.rs` tracks materialized JSON writes inside a
  transaction.
- `transaction_ops.rs` defines a cross-primitive transaction trait for KV,
  event, and JSON operations.

Storage dependencies are intentional here today:

- `SegmentedStore`
- `TransactionContext`
- `TransactionManager`
- `CommitError`
- storage durable commit helpers
- WAL writer types

The conceptual split for a later architecture pass is:

```text
engine public write API
  -> engine commit unit
  -> storage transaction context / row batch
  -> storage durable commit mechanics
```

The current implementation has all four concepts close together.

## Branch Domain And Branch Operations

Files:

```text
branch_domain.rs
branch_domain/branch.rs
branch_domain/branch_dag.rs
branch_domain/branch_types.rs
branch_ops/mod.rs
branch_ops/branch_control_store.rs
branch_ops/dag_hooks.rs
branch_ops/json_merge.rs
branch_ops/primitive_merge.rs
branch_retention/mod.rs
database/branch_service.rs
database/branch_mutation.rs
database/dag_hook.rs
database/merge_registry.rs
```

Responsibilities:

- branch references, generation IDs, lifecycle status, fork anchors, branch DAG
  records, and branch metadata
- branch control records in storage-backed system rows
- branch creation, deletion, fork, merge, cherry-pick, revert, diff, and
  retention reporting
- merge handlers for JSON, graph, and vector behavior
- branch DAG projection through graph hooks
- observer events for branch operations

Current branch operation mechanics scan storage rows by type family:

```text
TypeTag::KV
TypeTag::Event
TypeTag::Json
TypeTag::Vector
TypeTag::Graph
```

This is engine-owned semantic behavior, but it means branch operation code is
one of the largest direct users of storage keys, type tags, versioned entries,
and raw scans.

Important residue:

- Branch tags and notes still exist inside `branch_ops/mod.rs` and observer
  event types. Product requirements currently do not treat branch tags/notes as
  V1-critical user pathways.
- Branch-bundle export/import is separate from branch operations but relies on
  the same storage row scanning model.

## Primitive Facades

Directory: `crates/engine/src/primitives`

Files:

```text
primitives/mod.rs
primitives/kv.rs
primitives/event.rs
primitives/extensions.rs
primitives/space.rs
primitives/branch/mod.rs
primitives/branch/handle.rs
primitives/branch/index.rs
primitives/json/mod.rs
primitives/json/index.rs
```

Responsibilities:

- `KVStore`, `JsonStore`, `EventLog`, `SpaceIndex`, branch handles, and
  transaction extension traits.
- Stateless facade pattern: each primitive holds an `Arc<Database>` and routes
  reads/writes through database transaction/storage APIs.
- Branch isolation through `BranchId` and storage namespaces.
- Space metadata and user-space validation.
- JSON secondary-index metadata and maintenance.
- Event append/read/range behavior.
- Convenience re-exports of search interfaces for primitive search behavior.

The primitive layer is currently split between:

- API facades in `primitives/`
- pure-ish semantic types and validators in `semantics/`
- raw storage keys and transaction code in storage-facing implementation paths

This split is useful, but not consistently enforced.

## Semantics

Directory: `crates/engine/src/semantics`

Files:

```text
semantics/event.rs
semantics/json.rs
semantics/value.rs
semantics/vector.rs
semantics/mod.rs
```

Responsibilities:

- event chain semantics and verification helpers
- JSON path, patch, parser, validation, and value behavior
- value text extraction and search-facing conversion helpers
- vector semantic types: config, metric, storage dtype, filters, collection and
  vector entry types

This is the closest thing engine currently has to a pure domain layer. It
should be treated as an important input to engine-next, because it contains
types that are easier to reason about without database lifecycle concerns.

## Graph Cluster

Directory: `crates/engine/src/graph`

Files:

```text
adjacency.rs
analytics.rs
boost.rs
branch_dag.rs
branch_status_cache.rs
bulk.rs
dag_hook_impl.rs
edges.rs
ext.rs
integrity.rs
keys.rs
lifecycle.rs
merge.rs
merge_handler.rs
mod.rs
nodes.rs
ontology.rs
packed.rs
snapshot.rs
store.rs
traversal.rs
types.rs
```

Responsibilities:

- public `GraphStore` and transaction extension trait
- graph node and edge CRUD
- graph traversal and analytics
- ontology/schema behavior
- adjacency and packed adjacency structures
- graph-specific branch DAG projection and branch status cache
- graph merge handler and lifecycle hooks
- graph search participation through `Searchable`
- graph storage keys under the reserved `_graph_` space

Current implementation model:

```text
GraphStore
  -> Database transactions
  -> graph storage keys in _graph_
  -> TypeTag::Graph rows
  -> graph runtime hooks / branch DAG projection
```

Important product direction:

- Graph is not just a standalone primitive. The current code already makes it
  useful as an engine-level relationship layer through graph search, branch DAG
  projection, and graph-backed relationships. The next architecture pass should
  make the relationship-layer story explicit rather than treating graph as only
  another isolated primitive.

## Vector Cluster

Directory: `crates/engine/src/vector`

Files:

```text
backend.rs
brute_force.rs
collection.rs
distance.rs
error.rs
ext.rs
filter.rs
heap.rs
hnsw.rs
merge_handler.rs
mmap.rs
mmap_graph.rs
quantize.rs
recovery.rs
segmented.rs
store/mod.rs
store/collections.rs
store/crud.rs
store/recovery.rs
store/search.rs
store/system.rs
types.rs
```

Responsibilities:

- public `VectorStore`
- vector collection lifecycle
- vector insert/get/delete/search
- vector configs, filters, distances, quantization, and dtype semantics
- brute-force, HNSW, and segmented HNSW backends
- mmap sidecar graph files
- vector recovery and subsystem lifecycle
- post-commit vector backend updates through database observers
- vector merge handler
- shadow collection support for auto embeddings

Current implementation model:

```text
VectorStore
  -> persistent config/vector rows in storage
  -> in-memory backend state in Database extensions
  -> commit/abort observers for deferred index mutation
  -> recovery subsystem rebuilds backend state on open
  -> optional sidecar graph files under data_dir/vectors/...
```

Important tension:

- Vector state is partly storage rows and partly derived runtime/index state.
  The derived-state lifecycle is correct but spread across store modules,
  observers, recovery, merge handlers, and sidecar files.

## Search Cluster

Directory: `crates/engine/src/search`

Files:

```text
expand/error.rs
expand/mod.rs
expand/parser.rs
expand/prompt.rs
fuser.rs
index.rs
manifest.rs
mod.rs
recipe.rs
recovery.rs
rerank/blend.rs
rerank/error.rs
rerank/mod.rs
rerank/parser.rs
rerank/prompt.rs
searchable.rs
segment.rs
stemmer.rs
substrate.rs
tokenizer.rs
types.rs
```

Responsibilities:

- public search request/response/hit types
- `Searchable` trait implemented by primitives
- inverted index, segments, manifest, tokenizer, stemmer, BM25 scoring
- search recipe model and built-in recipe definitions
- deterministic retrieval substrate
- BM25 + vector fusion
- graph/KV/JSON/event search fan-out
- query expansion and reranking prompt/parser helpers
- search subsystem recovery and branch cleanup

Current search path:

```text
SearchRequest / recipe
  -> search substrate
  -> BM25 primitive fan-out
  -> vector shadow/user collection search
  -> graph participation through Searchable
  -> fuser/reranker stages
  -> SearchResponse
```

The `substrate.rs` file explicitly treats model-dependent work as outside the
substrate: intelligence supplies embeddings, expansion, and reranking inputs;
engine executes deterministic retrieval.

Important tension:

- Search contains both deterministic engine behavior and prompt/parser helpers
  for model-assisted query expansion/reranking. Engine-next should keep the
  boundary between deterministic retrieval and intelligence-facing wrappers
  crisp.

## Product Runtime Composition

Current product open path:

```text
open_product_database(path, options)
  -> OpenSpec::primary(path) or OpenSpec::follower(path)
  -> product_runtime_subsystems()
       [GraphSubsystem, VectorSubsystem, SearchSubsystem]
  -> Database::open_runtime(spec)
  -> seed_builtin_recipes_warning_only()
  -> ProductOpenOutcome::{Local | Ipc}
```

Current product cache path:

```text
open_product_cache()
  -> OpenSpec::cache()
  -> product_runtime_subsystems()
       [GraphSubsystem, VectorSubsystem, SearchSubsystem]
  -> default branch bootstrap
  -> built-in recipe seed
```

Current low-level internal profiles:

```text
search_only_primary_spec(path)
search_only_follower_spec(path)
search_only_cache_spec()
```

These install only `SearchSubsystem` and exist for tests/utilities that do not
need full product behavior.

Important tension:

- The product path is clear, but the old subsystem-instantiation model still
  exists as a low-level engine API.
- The product path still exposes follower mode. Product direction now favors
  IPC for multi-user access, so follower mode is a major current-code feature
  that is likely to be removed before any next-generation port.

## Storage Boundary Touchpoints

Engine legitimately consumes storage today. The main storage-facing concepts
used across the crate are:

- `SegmentedStore`
- `TransactionContext`
- `TransactionManager`
- `Key`
- `Namespace`
- `TypeTag`
- `VersionedEntry`
- `StorageIterator`
- `StorageError`
- `StorageRuntimeConfig`
- durability helpers for WAL, checkpoint, snapshot install, manifest, recovery,
  compaction, and retention

Main storage-touching areas:

| Area | Storage Use |
|---|---|
| `database/open.rs` | WAL writer, storage creation/recovery, storage runtime config, filesystem locks |
| `database/recovery.rs` | storage recovery driver, manifest/codec/WAL facts, `SegmentedStore`, `RecoveryHealth` |
| `database/snapshot_install.rs` | decoded snapshot rows, type families, generic storage install helper |
| `database/compaction.rs` | checkpoint data collection, WAL compaction, snapshot pruning, manifest helpers |
| `database/transaction.rs` | transaction contexts, commit, WAL append, flush/compaction scheduling |
| `coordinator.rs` | storage transaction manager and durable commit helpers |
| `branch_ops/` | raw storage scans by `TypeTag`, `VersionedEntry`, merge inputs |
| `primitives/` | keys, namespaces, transaction context access |
| `graph/` | graph keys, graph rows, graph merge and traversal data |
| `vector/` | vector keys, config/vector rows, recovery scans, sidecar state |
| `search/` | search keys, system rows, substrate reads |
| `bundle/` | branch export/import by scanning storage rows and writing replay payloads |
| `recipe_store.rs` | recipe rows in `_system_` space |
| `system_space.rs` | internal namespace/key construction |

The current boundary is conceptually:

```text
storage owns:
  row storage, WAL mechanics, manifest/checkpoint mechanics, segment mechanics,
  low-level recovery, generic decoded-row install, runtime storage resource
  derivation

engine owns:
  primitive decode, public database lifecycle, branch semantics, transaction
  policy, product config, recovery policy, graph/vector/search derived state,
  public errors, and product open behavior
```

This is healthier than the previous boundary, but engine still imports storage
types in many modules because current engine encodes product semantics directly
into storage row keys and values.

## Recovery, Lifecycle, And Derived State

Current lifecycle layers:

```text
storage recovery
  -> Database::run_recovery
  -> TransactionCoordinator bootstrap
  -> subsystem recover()
  -> subsystem initialize()
  -> subsystem bootstrap()
  -> product default branch and recipes

commit
  -> storage commit
  -> engine observers
  -> vector/search/graph derived state updates

follower refresh
  -> WAL read
  -> storage apply
  -> refresh hooks
  -> watermarks and blocked state

shutdown/drop
  -> stop accepting transactions
  -> drain/flush/checkpoint/freeze
  -> subsystems freeze in reverse order
  -> registry cleanup
```

Current subsystem trait:

```text
recover(&Database)
initialize(&Database)
bootstrap(&Database)
freeze(&Database)
name() -> &'static str
```

Graph, vector, and search use this path to rebuild or install runtime state.
This is the current integration mechanism for derived state.

## Bundle And Data Movement

Directory: `crates/engine/src/bundle`

Files:

```text
error.rs
mod.rs
reader.rs
types.rs
wal_log.rs
writer.rs
```

Responsibilities:

- `.branchbundle.tar.zst` archive format
- branch export and import workflows
- archive manifest, paths, checksums, and branch log records
- scanning a branch's storage rows into replayable payloads
- replaying imported bundle payloads as database transactions

Current tension:

- Product direction now favors StrataHub clone/sync and portable `.strata`
  datasets over branch bundles as a central V1 path. Bundle code is still a
  significant current engine module and should be accounted for before porting
  or retiring anything.

## System Space And Recipes

Files:

```text
system_space.rs
recipe_store.rs
search/recipe.rs
```

Responsibilities:

- `_system_` reserved internal space
- cached internal namespaces per branch
- built-in search recipes on the `_system_` branch
- user recipe shadowing on user branches
- recipe lookup order: user branch, then `_system_` branch fallback
- product-open recipe seeding

This is the main current mechanism for engine-owned internal metadata that is
stored in ordinary branch-scoped rows.

## Error Model

File: `crates/engine/src/error.rs`

Responsibilities:

- public `StrataError`
- canonical `ErrorCode`
- structured error details
- primitive degradation reason enum
- conversions from storage/core/serde/IO errors
- retryability and severity helpers
- extensive error tests

Current tension:

- The current engine error model is product-facing and stable-ish, but the
  storage-next work now has a more precise error/diagnostics direction. Engine
  needs an explicit mapping layer so storage errors are not flattened across
  important write-path ambiguity.

## Tests And Benches

Engine integration tests:

```text
adversarial_tests.rs
architecture_doc_truth.rs
branch_id_characterization.rs
branch_isolation_tests.rs
concurrency_tests.rs
config_matrix.rs
crash_simulation_test.rs
cross_primitive_tests.rs
database_open_test.rs
database_transaction_tests.rs
flush_pipeline_tests.rs
follower_tests.rs
m4_pooling_tests.rs
memory_profiling.rs
primitives_cross_tests.rs
recovery_parity.rs
recovery_storage_policy.rs
recovery_tests.rs
robustness_regressions.rs
surface_regression.rs
versioned_conformance_tests.rs
```

Database module tests:

```text
checkpoint.rs
codec.rs
contention.rs
open.rs
regressions.rs
search_branch_cleanup.rs
shutdown.rs
snapshot_retention.rs
transactions.rs
```

Bench targets:

```text
transaction
primitive
vector
```

Test coverage is broad but not architecturally uniform. Large subsystems have
many module-local tests, while cross-layer guarantees are spread through
integration tests, database tests, and guard tests.

## Current Architectural Hotspots

These are not findings to fix in this document; they are map markers for the
engine-next architecture pass.

1. `database/` is doing too much.
   It combines public database API, product open, storage adaptation, lifecycle,
   follower refresh, transaction runtime, health, recovery, config, and test
   hooks.

2. The public surface is broader than the product surface.
   `lib.rs` exports product APIs, low-level runtime APIs, test-adjacent
   compatibility types, storage-derived types, and implementation types from one
   namespace.

3. Storage-shaped concepts are spread through engine.
   This is partly necessary in the current implementation, but engine-next needs
   a clearer rule for row-key encoding, storage family allocation, and what
   storage DTOs may cross into engine.

4. Subsystem composition still exists.
   Product open hides it for normal users, but `OpenSpec` still accepts
   explicit subsystems. Graph/vector/search lifecycle wiring depends on this
   model.

5. Follower mode is deeply integrated.
   It touches open, recovery, refresh, watermarks, health, lifecycle, and tests.
   Removing it will simplify durability semantics but needs its own cleanup.

6. Public transaction surfaces remain.
   Internal commit units are necessary, but user-facing transaction commands are
   no longer central to the product direction unless the product commits to an
   ACID story.

7. Branch operation code is too concentrated.
   `branch_ops/mod.rs` contains multiple domains: diff, merge, revert,
   cherry-pick, tags, notes, graph merge handling, storage scans, and tests.

8. Graph/vector/search are real subsystems now.
   They are no longer peer crates, but each has enough runtime state and
   domain-specific behavior to deserve explicit internal boundaries inside
   engine.

9. Branch bundles remain a full data-movement path.
   The product direction is moving toward StrataHub clone/sync and portable
   dataset files. Bundle code should be classified as keep, migrate, or retire
   before engine-next implementation.

10. System metadata is stored as ordinary rows.
    `_system_`, recipe rows, branch control rows, graph DAG rows, shadow vectors,
    and primitive metadata all use storage rows with engine conventions. This is
    powerful, but needs a stable registry/story for engine-owned internal row
    families.

## Current Ownership Summary

| Responsibility | Current Owner | Notes |
|---|---|---|
| Storage rows, WAL, manifest, segment mechanics | storage | engine calls storage helpers and owns public policy |
| Product open behavior | engine | graph/vector/search composition, default branch, recipe seeding |
| IPC transport | executor | engine returns IPC outcome classification only |
| Branch lifecycle and branch operations | engine | branch control records and branch DAG semantics are engine-owned |
| KV/JSON/event primitives | engine | stateless facades over `Database` |
| Graph primitive and relationship layer | engine | stored through storage rows and graph runtime hooks |
| Vector primitive and ANN indexes | engine | persistent rows plus derived backend state and sidecars |
| Search and retrieval substrate | engine | deterministic retrieval; intelligence supplies model-dependent inputs |
| Inference/model providers | intelligence/inference | configured through engine config today, executed above engine |
| Internal recipes | engine | stored in `_system_` space |
| Public error model | engine | storage errors are mapped upward |
| Runtime resource profiling | engine + storage | engine product config/profile, storage effective runtime knobs |
| Branch bundles | engine | current export/import workflow, product direction may supersede |

## How To Use This Map

Use this map as the starting point for engine-next design:

- If a feature is listed under database runtime, decide whether it is product
  API, engine policy, or storage adapter.
- If a feature is listed under graph/vector/search, decide whether it is core
  engine domain behavior, derived-state runtime, or intelligence-facing wrapper.
- If a feature uses storage keys or `TypeTag`, decide whether the current storage
  shape should remain visible to engine-next or be hidden behind a storage
  consumption contract.
- If a public type is listed only because it is convenient today, decide whether
  it belongs in the future product surface.

The main lesson from the scan is simple: engine is now the right dependency hub,
but it is internally overloaded. The next architecture work should keep the
crate as the product behavior owner while carving its internals into stable,
repeatable domains.
