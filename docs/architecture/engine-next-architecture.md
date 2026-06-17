# Engine-Next Architecture

Status: V1 architecture draft

## Purpose

This document defines the conceptual architecture for `engine-next`.

It does not define Rust traits, module names, migration steps, API syntax, or
implementation milestones. Its job is to define the engine buckets Strata wants
before we start moving code or designing repeatable implementation patterns.

The current engine crate map is documented in
[docs/engine/engine-crate-map.md](../engine/engine-crate-map.md). That map is
evidence. This document is the target internal architecture model.

Engine-next should preserve the correct product boundary created by the engine
consolidation work while cleaning up the inside of the crate. The goal is not
bespoke feature development. The goal is a reference-grade engine structure that
is easier to reason about, test, and extend.

## Related Documents

Product anchors:

1. [strata-v1-product-requirements.md](../product/strata-v1-product-requirements.md)
2. [strata-v1-feature-inventory.md](../product/strata-v1-feature-inventory.md)
3. [strata-v1-user-pathways.md](../product/strata-v1-user-pathways.md)
4. [strata-v1-non-functional-requirements.md](../product/strata-v1-non-functional-requirements.md)

Focused product direction:

1. [strata-v1-branching-direction.md](../product/strata-v1-branching-direction.md)
2. [strata-v1-graph-relationship-layer.md](../product/strata-v1-graph-relationship-layer.md)
3. [strata-v1-versioning-time-travel.md](../product/strata-v1-versioning-time-travel.md)
4. [stratahub-product-direction.md](../product/stratahub-product-direction.md)

Architecture anchors:

1. [strata-v1-architecture.md](./strata-v1-architecture.md)
2. [core-next-architecture.md](./core-next-architecture.md)
3. [storage-next-architecture.md](./storage-next-architecture.md)
4. [storage-next/l9-storage-api-boundary.md](./storage-next/l9-storage-api-boundary.md)
5. [engine-next/README.md](./engine-next/README.md)
6. [engine-next/primitive-implementation-contract.md](./engine-next/primitive-implementation-contract.md)
7. [engine-next/entity-ref-and-relationship-layer-contract.md](./engine-next/entity-ref-and-relationship-layer-contract.md)
8. [engine-next/storage-space-id-registry.md](./engine-next/storage-space-id-registry.md)
9. [stratahub-substrate-architecture.md](./stratahub-substrate-architecture.md)
10. [runtime-resource-profile-architecture.md](./runtime-resource-profile-architecture.md)
11. [v1-error-and-diagnostics-contract.md](./v1-error-and-diagnostics-contract.md)
12. [v1-testing-and-conformance-plan.md](./v1-testing-and-conformance-plan.md)

Current engine evidence:

1. [engine-crate-map.md](../engine/engine-crate-map.md)
2. [engine-consolidation-plan.md](../engine/engine-consolidation-plan.md)
3. [follower-mode-removal-plan.md](../engine/follower-mode-removal-plan.md)
4. [v1-storage-consumption-contract.md](../storage/v1-storage-consumption-contract.md)

Historical implementation plans in `docs/engine/archive/` are evidence of how
the current code got here. They are not target architecture.

## Requirement Language

1. Must means V1 engine architecture is incomplete without it.
2. Should means expected for V1 unless a later architecture decision records a
   clear deferral.
3. May means allowed but not required for V1.

## Product Constraints

Engine-next exists to serve the V1 product model.

The important constraints are:

1. Strata is embedded first.
2. Durable local filesystem is the reference durable backend through
   storage-next.
3. Cache mode is explicit and non-durable.
4. Cache mode supports the full V1 product API and pathway matrix; durability
   is the only product difference.
5. IPC is required for V1 local multi-process access.
6. Follower mode is not a V1 product path.
7. Public manual transaction sessions are not a V1 product requirement.
8. Branching, versioning, history, diff, restore, and branch-from-history are
   core product capabilities.
9. Strata has one physical storage primitive: branch-aware MVCC KV rows.
10. Strata exposes V1 data capabilities over that substrate: KV, JSON, event,
   vector, and graph.
11. Graph is both a standalone product capability and the relationship layer
    across Strata records.
12. Vector is both a standalone product capability and the target for shadow
    embeddings.
13. Search and retrieval are engine services over data capabilities, not
    separate storage primitives.
14. Cross-capability behavior must be explicit, observable, and testable.
15. `_system_` branch and per-branch `_system_` space are first-class engine
    control-plane tools.
16. Users should not operate flush, compaction, checkpoint, retention, or
    recovery during normal use.
17. Runtime resource profiling is a product requirement. Engine-next owns
    product-wide resource policy and passes resolved storage budgets down.
18. Upper layers must not reach around engine to consume storage directly.

## Binding Engine-Next Decisions

These decisions pin the first-pass engine direction.

1. **Engine-next is one semantic crate, not peer data-capability crates.**
   KV, JSON, event, vector, and graph are product capabilities over one KV row
   substrate. Graph and vector are not top-level engine architecture buckets,
   and JSON/event/vector/graph are not peer storage engines.

2. **Search is retrieval infrastructure, not another data capability.**
   Search indexes, BM25, fusion, recipes, and deterministic retrieval live in
   the retrieval bucket. Search consumes capability adapters and control-plane
   policies.

3. **Cross-capability behavior lives outside capability internals.**
   A capability must not directly call a sibling capability. Autoembedding,
   graph relationships across capabilities, search projection, triggers, and
   rebuild jobs live in orchestration services over data capability contracts.

4. **The control plane is first-class.**
   The global `_system_` branch and branch-local `_system_` space are target
   architecture, not incidental implementation detail. Engine-next uses them
   for recipes, capability registries, projection manifests, watermarks, derived
   state status, policies, and job metadata.

5. **Persistence is the only normal storage-facing bucket.**
   Engine-next should concentrate storage-next imports behind a persistence
   adapter. Other buckets should use engine-owned key, row, batch, read, write,
   and timeline abstractions rather than importing storage internals directly.

6. **Commit is internal but central.**
   Engine-next keeps an internal commit unit, batch semantics, version
   allocation coordination, observers, and write ordering. It does not expose
   ordinary manual begin/commit/rollback sessions as the main product model.
   Each normal public write is one commit. Explicit batch APIs such as KV batch
   put/delete are atomic inside one capability. Cross-capability public batches
   are not a V1 requirement.

7. **Branch product semantics stay in engine.**
   Storage-next owns generic branch-isolated physical mechanics. Engine-next
   owns branch names, fork, merge, diff, restore, revert, cherry-pick,
   branch-from-history, and user-facing conflict policy.

8. **Derived state must be rebuildable or explicitly authoritative.**
   Search indexes, vector indexes, graph projections, auto embeddings, and
   relationship projections need manifests, watermarks, health, and rebuild
   behavior. They must not silently contradict committed source rows.

9. **Intelligence and inference stay above engine.**
   Engine-next can store model configuration and expose deterministic
   retrieval. It should not call remote model providers or own inference
   execution. Intelligence-next supplies model-dependent orchestration.

10. **IPC command semantics belong to engine; IPC transport does not.**
    Engine-next owns command classification, access-mode validation, product
    errors, and serializable command behavior. Executor/CLI or a later IPC
    runtime owns transport and process management.

11. **Follower mode is excluded.**
    Engine-next must not preserve follower refresh, follower state files,
    follower watermarks, or follower-specific recovery semantics as target
    architecture.

12. **Branch tags, notes, and branch bundles are not V1 core.**
    Tags and notes are removed from the V1 product surface. Legacy branch
    bundles are removed from the V1 product surface and replaced by `.strata`
    clone artifacts for dataset movement.

13. **Pre-V1 database migration is not a V1 product promise.**
    Strata is pre-launch. V1 does not ship a general pre-V1 migration tool.
    Pre-V1 development databases are rejected at open with a structured
    format/layout error unless a specific implementation plan later approves a
    one-off developer conversion utility.

14. **Intelligence-next and inference-next are sequenced after engine-next.**
    Semantic search, RAG, query expansion, reranking, generation, and model
    management pathways that need provider execution depend on those follow-up
    architecture documents.

15. **Strata AI is a product client, not an engine bucket.**
    Intelligence-next is an in-process Rust crate engine consumers may depend
    on. Strata AI is a user-facing product that consumes engine plus
    intelligence either in-process when bundled or over IPC when external.

## Current Codebase Scan

The current `crates/engine` scan shows these major responsibility clusters:

1. Database runtime and lifecycle:
   - `database/mod.rs`
   - `database/open.rs`
   - `database/product_open.rs`
   - `database/spec.rs`
   - `database/config.rs`
   - `database/profile.rs`
   - `database/lifecycle.rs`
   - `background.rs`

2. Commit and transaction machinery:
   - `coordinator.rs`
   - `database/transaction.rs`
   - `transaction/context.rs`
   - `transaction/owned.rs`
   - `transaction/pool.rs`
   - `transaction/json_state.rs`
   - `transaction_ops.rs`

3. Branch domain and workflows:
   - `branch_domain.rs`
   - `branch_domain/`
   - `branch_ops/`
   - `branch_retention/`
   - `database/branch_service.rs`
   - `database/branch_mutation.rs`
   - `database/dag_hook.rs`
   - `database/merge_registry.rs`

4. Data capability implementations:
   - `primitives/kv.rs`
   - `primitives/json/`
   - `primitives/event.rs`
   - `primitives/space.rs`
   - `primitives/branch/`
   - `graph/`
   - `vector/`
   - `semantics/`

5. Retrieval and search:
   - `search/index.rs`
   - `search/substrate.rs`
   - `search/recipe.rs`
   - `search/recovery.rs`
   - `search/searchable.rs`
   - `search/segment.rs`
   - `search/types.rs`
   - `search/expand/`
   - `search/rerank/`

6. Control-plane metadata:
   - `system_space.rs`
   - `recipe_store.rs`
   - `_system_` branch usage
   - per-branch `_system_` space rows
   - branch control records
   - search/vector/graph manifests and runtime flags

7. Storage-boundary adapters:
   - `database/recovery.rs`
   - `database/snapshot_install.rs`
   - `database/compaction.rs`
   - `database/retention_report.rs`
   - storage calls spread through primitives, graph, vector, search, branch
     operations, and bundle code

8. Diagnostics and error handling:
   - `error.rs`
   - `database/primitive_degradation.rs`
   - `database/observers.rs`
   - health/reporting types in `database/mod.rs`
   - metrics and instrumentation helpers

9. Data movement:
   - `bundle/`

The scan changes the naive first-pass model in six ways:

1. Storage is already fundamentally a generic `Key`/`Value` MVCC substrate.
   KV, JSON, event, vector, and graph build typed keys, values, indexes, and
   derived state over that substrate.
2. The current `PrimitiveType`, `TypeTag`, and primitive snapshot-section names
   are implementation and durable-format vocabulary, not proof that the target
   architecture has five physical storage primitives.
3. Graph and vector are too important to be treated as afterthoughts, but they
   are data capability implementations, not architecture layers.
4. The `_system_` branch and `_system_` space are load-bearing architecture for
   cross-capability behavior.
5. Search/retrieval is broad enough to be its own bucket, but it should consume
   capability adapters rather than own capability state directly.
6. The storage adapter surface must be pulled inward. Current storage imports
   are spread widely because the current implementation evolved by feature
   accretion and consolidation.

## Target Bucket Graph

The engine-next buckets are conceptual architecture buckets, not required Rust
module names.

```text
strata-intelligence-next / executor / cli / SDK / Strata AI
        |
        v
+------------------------------------------------+
| API                                            |
+------------------------------------------------+
| Runtime                                        |
+------------------------------------------------+
| Branch     Commit     Data Capability          |
| Retrieval                                      |
|                                                |
| Control Plane        Orchestration             |
+------------------------------------------------+
| Persistence                                    |
+------------------------------------------------+
        |
        v
strata-storage-next L9
```

Diagnostics cuts across every bucket:

```text
API / Runtime / Branch / Commit / Data Capability / Control Plane /
Orchestration / Retrieval / Persistence
        |
        v
Diagnostics
```

The core rule is:

> A bucket may depend on lower-level contracts and sibling public contracts, but
> it must not reach into a sibling's implementation details.

For example, orchestration may call capability traits. It must not open
`data/vector/store/mod.rs` internals. Retrieval may call `Searchable` or
`TextProjectable` adapters. It must not decode JSON index rows itself unless
that is part of the capability contract. Branch may ask a capability for merge
behavior. It must not hand-roll each capability's row interpretation in one
giant branch operation file.

## API Bucket

API is the executor-facing engine contract consumed by executor-next,
intelligence-next, tests, and internal engine harnesses. It is not the public
product/API layer. Executor-next owns the public command/API surface and adapts
engine outcomes into CLI, SDK, IPC, and product-facing shapes.

It owns:

- `Database` handle shape
- open options and access modes
- durable/cache/read-only open APIs
- engine config DTOs
- engine data capability handles
- branch and time-travel command DTOs
- retrieval request/response DTOs
- engine health/status/report DTOs
- engine error surface consumed by executor-next

It must not own:

- storage keys
- executor command DTOs
- SDK/CLI/IPC wire DTOs
- storage row encodings
- WAL/manifest/checkpoint DTOs
- data capability implementation internals
- background job implementation details
- inference provider clients

The API bucket should expose product concepts:

```text
database
branch
space
record
entity reference
version
time
commit
search request
relationship
health
```

It should not force users or upper layers to know:

```text
SegmentedStore
TransactionContext
TypeTag
WAL segment
manifest watermark
checkpoint section
subsystem list
```

Current evidence:

- `lib.rs`
- `database/open_options.rs`
- `database/product_open.rs`
- public re-exports from `database/mod.rs`
- `error.rs`

## Runtime Bucket

Runtime owns database process/runtime lifecycle above storage.

It owns:

- local durable open
- cache open
- read-only open
- open registry and same-path reuse policy
- database lifecycle state
- shutdown and drop behavior
- background scheduler ownership
- product bootstrap ordering
- default branch bootstrap
- runtime resource profile application
- resolved engine/runtime budgets
- IPC fallback classification, not IPC transport
- access mode enforcement at runtime boundaries

It must not own:

- data capability row encodings
- branch merge semantics
- search ranking
- model provider calls
- storage backend IO
- raw WAL or manifest publication mechanics

Runtime consumes:

- API options
- persistence open/recovery facts
- control-plane bootstrap services
- commit runtime
- diagnostics

Runtime should replace the old subsystem-instantiation pattern with explicit
engine services. A service can have `open`, `recover`, `start`, `stop`,
`snapshot`, or `rebuild` lifecycle hooks, but product behavior should not be a
caller-supplied `Vec<Box<dyn Subsystem>>`.

Current evidence:

- `database/open.rs`
- `database/product_open.rs`
- `database/spec.rs`
- `database/lifecycle.rs`
- `database/profile.rs`
- `background.rs`
- `database/registry.rs`

## Commit Bucket

Commit owns the internal unit of change.

It owns:

- commit context
- batch construction
- commit ID and version coordination with storage
- write ordering
- branch commit locks
- write conflict policy where engine-owned
- write backpressure policy
- commit observer dispatch
- abort observer dispatch
- replay observer dispatch where still needed
- durable-but-not-visible classification
- internal batch API used by capabilities and orchestration

It must not own:

- public manual transaction sessions as a product path
- capability-specific validation
- branch product workflows
- storage WAL implementation details
- derived index internals

Commit should expose two internal shapes:

1. **Operation commit.**
   A simple product operation or batch uses one commit context and writes one
   atomic batch.

2. **Derived commit.**
   Orchestration or rebuild work writes derived state through explicit commit
   metadata so diagnostics can distinguish user-authored data from derived
   state.

Commit observers should be narrow. They should enqueue or notify services, not
hide complex business logic inside callbacks.

Current evidence:

- `coordinator.rs`
- `database/transaction.rs`
- `transaction/`
- `transaction_ops.rs`
- `database/observers.rs`

## Branch Bucket

Branch owns product branch semantics.

It owns:

- branch names and branch refs
- branch lifecycle
- branch DAG product model
- fork
- branch-from-current
- branch-from-version
- branch-from-time after timestamp resolution
- merge
- diff
- revert
- cherry-pick
- branch copy/promotion behavior
- branch retention reporting
- branch conflict strategy
- branch operation audit events

It must not own:

- storage COW table mechanics
- storage timeline implementation
- data capability row codecs
- graph-specific ontology or traversal internals
- vector index internals

Branch consumes:

- persistence branch mechanics
- storage commit timeline facts
- capability branch adapters
- control-plane branch metadata
- diagnostics

Branch should not be one giant module that manually understands every capability
row shape. The target pattern is:

```text
Branch operation
  -> resolve branch/time/version frontiers
  -> ask persistence for candidate physical rows where needed
  -> ask capability adapters to interpret/merge/diff their own rows
  -> write branch metadata and resulting operations through commit
```

Current evidence:

- `branch_domain.rs`
- `branch_domain/`
- `branch_ops/`
- `branch_retention/`
- `database/branch_service.rs`
- `database/branch_mutation.rs`
- `database/dag_hook.rs`
- `database/merge_registry.rs`

## Data Capability Bucket

Data Capability owns the product data capability implementations over the KV
row substrate:

```text
data/
├── kv
├── json
├── event
├── vector
└── graph
```

These are not five storage engines. They are five product-facing capability
families implemented over engine-owned row families. Storage sees physical
keys, opaque storage-space IDs, row bytes, versions, timestamps, and tombstones.
Storage does not know KV, JSON, event, vector, or graph semantics.

### Capability Contract

Every data capability should have the same conceptual parts:

1. **Facade.**
   Product-facing handle or API methods.

2. **Types.**
   Capability semantic DTOs, configuration, validation, and error fragments.

3. **Entity addressing.**
   How the capability maps user objects to typed `EntityRef` values.

4. **Row families.**
   Which engine-owned storage-space IDs and logical row groups the capability
   uses.

5. **Key encoding.**
   How user identity maps to physical row keys through the persistence adapter.

6. **Value encoding.**
   How capability values, metadata, tombstones, and derived rows are encoded.

7. **Read operations.**
   Latest, by-version, by-time, history, prefix/range, and branch-aware reads.

8. **Write operations.**
   Put/insert/update/delete/append semantics over the internal commit unit.

9. **Branch adapter.**
   Diff, merge, revert, cherry-pick, and copy behavior for the capability.

10. **Search/text adapter.**
    Optional text projection and search participation.

11. **Relationship adapter.**
    Optional entity resolution and relationship-layer participation.

12. **Derived-state hooks.**
    Optional hooks for index rebuild, shadow state, recovery, and diagnostics.

13. **Conformance tests.**
    Shared capability tests plus capability-specific tests.

### Required Data Capability Semantics

| Capability | KV | JSON | Event | Vector | Graph |
|---|---:|---:|---:|---:|---:|
| Entity addressable | Yes | Yes | Yes | Yes | Yes |
| Latest read | Yes | Yes | Yes | Yes | Yes |
| Version read | Yes | Yes | Yes | Yes | Yes |
| Timestamp read | Yes | Yes | Yes | Yes | Yes |
| History | Yes | Yes | Yes | Yes | Yes |
| Branch diff | Yes | Yes | Yes | Yes | Yes |
| Merge adapter | Simple | Structured | Append/ordered | Config + record aware | Relationship/ontology aware |
| Text projection | Optional | Yes | Yes | Metadata/source dependent | Node/edge dependent |
| Search adapter | Yes | Yes | Yes | Vector search | Graph search |
| Relationship participation | As entity | As entity/subpath | As entity | As entity/source link | Native + entity-bound |
| Derived runtime state | No | Indexes | Optional | ANN indexes | Traversal/index projections |

The table describes target architecture, not a promise that every current code
path already meets the same quality bar.

### Data Capability Rules

1. A data capability may own its own semantics and row encodings.
2. A data capability may expose capability traits to branch, retrieval, and
   orchestration.
3. A data capability may maintain derived runtime state if it declares rebuild and
   health behavior.
4. A data capability must not call sibling capability internals.
5. A data capability must not import storage-next directly except through
   persistence adapter types explicitly approved for row encoding.
6. A data capability must not hide cross-capability behavior in its own CRUD
   methods.

Current evidence:

- `primitives/`
- `semantics/`
- `graph/`
- `vector/`

## Control Plane Bucket

Control plane owns engine metadata that governs behavior.

Strata has two control-plane locations:

```text
_system_ branch
  database-global metadata

_system_ space inside each user branch
  branch-local metadata
```

This split is target architecture.

The global `_system_` branch should own:

- built-in recipes
- database-level capability registry
- engine-owned storage-space ID registry
- product capability facts
- default orchestration policies
- database identity and provenance metadata
- StrataHub substrate metadata where V1 requires it
- global background job catalog where appropriate

Per-branch `_system_` space should own:

- branch recipe overrides
- search index manifests
- vector shadow collection manifests
- graph relationship-layer manifests
- projection watermarks
- derived-state rebuild status
- branch-local orchestration policies
- branch-local trigger declarations, if triggers are retained
- branch-local feature/capability state

Control-plane data should branch with user branch state where the behavior is
branch-local. For example, a branch-specific recipe, embedding policy, graph
relationship policy, or projection watermark belongs in that branch's
`_system_` space.

Control-plane data should be global only when it describes database-wide facts:
database identity, product capabilities, built-in recipes, global defaults,
format/cutover facts, and future StrataHub substrate identity.

Current evidence:

- `system_space.rs`
- `recipe_store.rs`
- branch control records
- search recipes and manifests
- vector config and shadow collection conventions
- graph DAG and relationship metadata

## Orchestration Bucket

Orchestration owns cross-capability workflows and derived work.

It owns:

- autoembedding coordination
- shadow vector maintenance
- graph relationship-layer coordination
- derived relationship projections
- search projection and reindex coordination
- background rebuild jobs
- explicit triggers, if retained for V1 or post-V1
- derived-state repair
- projection watermarks
- derived-state consistency policy

It must not own:

- data capability CRUD internals
- storage IO
- model provider execution
- public command syntax
- hidden network sync

Orchestration consumes:

- data capability traits
- control-plane policies and manifests
- commit observer facts
- runtime background scheduler
- diagnostics
- intelligence-provided model outputs where model execution is needed

### Cross-Capability Patterns

Autoembedding:

```text
TextProjectable capability change
  -> commit observer emits fact
  -> orchestration reads branch-local embedding policy
  -> intelligence/inference produces embedding when enabled
  -> orchestration writes shadow vector rows through vector capability contract
  -> control plane records watermark/status
```

Relationship layer:

```text
EntityAddressable capability records
  -> relationship policy or explicit link command
  -> graph capability stores relationship nodes/edges
  -> graph traversal returns EntityRef values
  -> callers fetch source records through their owning capability
```

Search projection:

```text
Searchable/TextProjectable capability change
  -> orchestration or retrieval indexer updates search index
  -> control plane records index manifest and watermark
  -> retrieval uses index only when compatible with requested branch/time
```

Trigger or projection:

```text
explicit declaration in _system_
  -> commit fact matches declaration
  -> orchestration executes a named workflow
  -> writes happen through normal capability/commit APIs
```

The rule is explicit:

> Cross-capability behavior is metadata-declared and service-executed.
> Data capabilities expose traits. Orchestration coordinates.

Current evidence:

- auto-embedding/shadow collection configuration
- vector commit/abort observers
- search recovery and cleanup
- graph DAG hooks
- `database/observers.rs`
- `recipe_store.rs`

## Retrieval Bucket

Retrieval owns deterministic search and retrieval behavior inside engine.

It owns:

- search request/response types where they are engine API DTOs
- recipe resolution and deterministic interpretation
- BM25 indexing and scoring
- data capability search fan-out
- vector retrieval integration
- graph-aware retrieval expansion
- result fusion
- retrieval stats
- temporal retrieval behavior over versions/timestamps
- index compatibility checks against branch/time
- retrieval diagnostics

It must not own:

- model provider calls
- remote inference
- hidden query expansion or reranking network calls
- data capability write semantics
- storage row decoding outside capability/search index contracts

Retrieval may contain deterministic helpers for query expansion/rerank parsing
or prompt templates only if those are treated as configuration assets consumed
by intelligence. Model execution belongs above engine.

Current evidence:

- `search/`
- `search/substrate.rs`
- `search/index.rs`
- `search/recipe.rs`
- `search/searchable.rs`
- `search/expand/`
- `search/rerank/`

## Persistence Bucket

Persistence is the engine-owned adapter over storage-next L9.

It owns:

- engine storage-space ID registry consumption
- engine row family registry
- physical key construction
- value codec dispatch for capability rows where centralized
- `EntityRef` to row-key/value encoding helpers
- storage read/write adapters
- latest/version/timestamp/history reads over storage-next
- branch physical operation adapters
- storage timeline resolution adapters
- snapshot collect and decode adapters
- checkpoint/recovery adapter policy
- storage error mapping into engine errors
- storage health fact mapping into diagnostics

It must not own:

- data capability semantics
- engine API DTOs
- storage internals below L9
- WAL/manifest/table implementation details
- product branch merge policy

Only persistence should normally import storage-next in production engine code.
Temporary exceptions during migration must be documented and removed.

Current evidence:

- `database/recovery.rs`
- `database/snapshot_install.rs`
- `database/compaction.rs`
- `database/config.rs` storage runtime adapter
- storage imports throughout current capability implementations, graph, vector, search, branch
  operations, coordinator, and bundle code

Target direction:

```text
data capability/branch/retrieval/orchestration
  -> engine persistence contract
  -> storage-next L9
```

not:

```text
data capability/branch/retrieval/orchestration
  -> strata_storage::{Key, TypeTag, SegmentedStore, TransactionContext}
```

## Diagnostics Bucket

Diagnostics cuts across engine.

It owns or coordinates:

- public health reports
- recovery diagnostics
- data capability degradation reports
- derived-state status
- branch retention reports
- storage fact mapping
- runtime resource profile explanations
- trace/event context
- error status mapping
- test/fault hook exposure at engine boundary
- operator-readable state reports

It must not own:

- recovery mechanics
- data capability semantics
- storage internals
- product command execution

Diagnostics should make hidden state visible:

- which runtime mode is active
- whether a handle is local or IPC-backed
- selected resource profile and resolved budgets
- storage recovery health
- derived-state rebuild status
- projection watermarks
- index temporal compatibility
- branch retention blockers
- degraded data capability reasons
- storage backend capability mismatches

Current evidence:

- `error.rs`
- health types in `database/mod.rs`
- `database/primitive_degradation.rs`
- `database/retention_report.rs`
- `instrumentation.rs`
- storage recovery health re-exports

## Data Movement And StrataHub Substrate

Engine-next should treat clone/export/import/sync substrate as product semantics
above storage, but this document does not create a separate target bucket yet.

For the first architecture pass:

- existing branch bundle code is legacy data-movement evidence only; the V1
  product surface removes branch-bundle commands and artifacts
- future `.strata` dataset clone/import/export maps to API + orchestration +
  control plane + persistence
- StrataHub fleet metadata maps to control plane + diagnostics
- actual network sync remains post-V1 unless a dedicated sync architecture
  says otherwise

Storage-next persists bytes and rows. Engine-next understands branches, data
capabilities, entity references, recipes, derived state, provenance, and product
errors. Therefore dataset movement is an engine/product concern, not a storage
concern.

Current evidence:

- `bundle/`
- StrataHub product direction documents

## Target Dependency Rules

1. API may call runtime and product services. It must not call storage-next.
2. Runtime may coordinate commit, branch, control plane, orchestration,
   retrieval, persistence, and diagnostics.
3. Commit may use persistence and diagnostics. It must not know data capability
   implementation internals.
4. Branch may use capability branch adapters, control plane, commit, persistence
   branch mechanics, and diagnostics.
5. Data Capability may use commit, persistence contracts, control plane
   metadata, and diagnostics. It must not call sibling capability internals.
6. Control plane may use capability storage contracts, commit, persistence, and
   diagnostics. It should not contain business logic that belongs to
   orchestration.
7. Orchestration may use data capability traits, control plane, commit,
   retrieval, runtime scheduling, and diagnostics. It must not own capability
   row semantics directly.
8. Retrieval may use data capability search/text/vector/graph adapters, control
   plane recipes/manifests, persistence read contracts, and diagnostics.
9. Persistence may use storage-next L9 and core-next. It must not call upper
   product code.
10. Diagnostics may consume facts from every bucket but should avoid becoming a
    hidden control path.

## Cross-Cutting Test And Fault Framework

Testing is not a bucket. It cuts across every bucket.

Engine-next must be designed so each bucket has direct tests:

1. API surface and command classification tests.
2. Runtime open/cache/read-only/IPC-classification tests.
3. Commit ordering, observer, backpressure, and ambiguous-commit tests.
4. Branch fork/merge/diff/revert/cherry-pick/branch-from-time tests.
5. Data capability conformance tests applied to KV, JSON, event, vector, and graph.
6. EntityRef and relationship-layer conformance tests.
7. Control-plane branch/global metadata tests.
8. Orchestration projection and rebuild tests.
9. Retrieval temporal/index compatibility tests.
10. Persistence adapter tests against storage-next L9 test doubles.
11. Diagnostics and error-status tests.
12. End-to-end product pathway tests over engine APIs.

Fault injection must cover:

- storage recovery degraded/lossy facts
- storage read/write/commit failures
- ambiguous durable commit outcomes
- derived-state update failure after source commit
- rebuild interruption
- stale projection watermark
- incompatible index at historical point
- invalid control-plane metadata
- relationship target deletion or history loss
- background worker failure
- shutdown during commit or rebuild
- IPC access-mode mismatch at command boundary

Engine tests should not depend on storage internals except through explicitly
declared persistence test doubles or integration tests that exercise the real
storage boundary.

## Bucket Placement Rules

1. Storage imports belong in persistence unless an exception is documented.
2. Data capability implementations live under the capability contract, not as top-level
   architecture buckets.
3. Search/retrieval code does not own authored capability data.
4. Cross-capability behavior belongs in orchestration.
5. `_system_` metadata belongs in control plane.
6. Branch workflows ask capabilities for capability-specific behavior.
7. Commit observers notify or enqueue; they should not become hidden workflow
   engines.
8. Runtime opens services; it should not contain data capability business logic.
9. Engine API types should be engine-owned, even when backed by storage facts.
10. Diagnostics reports facts; it should not decide product behavior.

## Current Code To Target Bucket Mapping

| Current area | Target bucket | Notes |
|---|---|---|
| `lib.rs` | API | Re-export surface should narrow and organize around product concepts. |
| `error.rs` | API/Diagnostics | Product error surface plus mapping from lower layers. |
| `sensitive.rs` | API/Runtime | Public config secret wrapper. |
| `limits.rs` | API/Data Capability | Limits used by product and capability validation. |
| `instrumentation.rs` | Diagnostics | Operation timing and metrics. |
| `background.rs` | Runtime/Orchestration | Scheduler ownership in runtime; jobs owned by services. |
| `database/mod.rs` | API/Runtime/Diagnostics | Needs decomposition; currently defines the central `Database` state and many reports. |
| `database/open.rs` | Runtime/Persistence | Runtime sequencing plus storage open/recovery adapter use. |
| `database/product_open.rs` | API/Runtime | Product open policy and IPC fallback classification. |
| `database/spec.rs` | Runtime | Low-level subsystem construction should be retired or replaced by service registry. |
| `database/config.rs` | API/Runtime/Persistence | Public config, resource profile inputs, storage runtime adapter. |
| `database/profile.rs` | Runtime | Host profile and default adjustment policy. |
| `database/lifecycle.rs` | Runtime/Diagnostics/Persistence | Shutdown, maintenance, follower remnants, snapshot pruning. |
| `database/recovery.rs` | Runtime/Persistence/Diagnostics | Storage recovery adapter plus engine policy. |
| `database/snapshot_install.rs` | Persistence/Data Capability | Snapshot row decode should centralize through persistence and capability codecs. |
| `database/compaction.rs` | Runtime/Persistence | Public maintenance wrappers should become internal lifecycle behavior. |
| `database/transaction.rs` | Commit/Runtime/Persistence | Commit pipeline, backpressure, observers, storage commit bridge. |
| `coordinator.rs` | Commit | Transaction/commit coordinator. |
| `transaction/` | Commit/Data Capability | Internal commit context and capability transaction helpers. |
| `transaction_ops.rs` | Data Capability/Commit | Cross-capability transaction trait should become internal or product-batch contract. |
| `database/observers.rs` | Commit/Orchestration/Diagnostics | Event dispatch, not hidden workflow implementation. |
| `database/primitive_degradation.rs` | Diagnostics/Data Capability | Capability degradation facts and reports. |
| `database/retention_report.rs` | Diagnostics/Branch/Persistence | Branch retention and reclaim reporting. |
| `database/branch_service.rs` | Branch/API | Product branch service. |
| `database/branch_mutation.rs` | Branch | Branch mutation workflow helpers. |
| `database/dag_hook.rs` | Branch/Data Capability | Branch DAG projection should move to graph/branch adapter contracts. |
| `database/merge_registry.rs` | Branch/Data Capability | Replace ad hoc registry with capability branch adapters. |
| `branch_domain.rs`, `branch_domain/` | Branch | Branch identity, lifecycle, DAG records. |
| `branch_ops/` | Branch/Data Capability | Split product workflows from capability-specific diff/merge adapters. |
| `branch_retention/` | Branch/Diagnostics | Branch retention summaries. |
| `primitives/kv.rs` | Data Capability/KV | KV capability implementation. |
| `primitives/json/`, `semantics/json.rs` | Data Capability/JSON | JSON capability, path semantics, indexes. |
| `primitives/event.rs`, `semantics/event.rs` | Data Capability/Event | Event capability and event semantics. |
| `graph/` | Data Capability/Graph | Graph capability and relationship-layer behavior. |
| `vector/` | Data Capability/Vector | Vector capability, ANN runtime, shadow vector target. |
| `semantics/value.rs` | Data Capability/API | Shared value projection/extraction semantics. |
| `semantics/vector.rs` | Data Capability/Vector | Vector semantic DTOs and validation. |
| `primitives/space.rs` | Data Capability/API/Control Plane | User spaces plus reserved system-space validation. |
| `primitives/branch/` | Branch/API/Control Plane | Current branch facade/index should move under branch/control plane. |
| `system_space.rs` | Control Plane/Persistence | Reserved system namespace helpers. |
| `recipe_store.rs` | Control Plane/Retrieval | Recipe metadata in `_system_`. |
| `search/` | Retrieval/Orchestration | Deterministic retrieval, index, recipe, temporal search behavior. |
| `recovery/` | Runtime/Orchestration | Current subsystem trait should become explicit service lifecycle contracts. |
| `bundle/` | Orchestration/Data Movement | Legacy branch-bundle path; remove from V1 product surface, and reuse code only when it serves the `.strata` clone-artifact contract without preserving branch-bundle compatibility. |
| `test_path_key.rs`, `database/test_hooks.rs` | Diagnostics/Test | Test/fault support should become explicit and scoped. |

## What Engine-Next Must Exclude

Engine-next must not include:

1. Storage backend IO implementation.
2. WAL, manifest, table, checkpoint, and storage recovery internals below L9.
3. Follower mode as a product or internal architecture path.
4. Hidden model-provider calls.
5. Inference execution.
6. StrataHub network sync as a hidden background feature.
7. Public manual transaction sessions as the primary product model.
8. Product behavior implemented by caller-supplied subsystem lists.
9. Capability-to-capability hidden dependencies.
10. User-operated flush, compact, checkpoint, and low-level maintenance flows as
    ordinary product pathways.

Engine-next may expose engine-owned diagnostics about lower-layer facts. That
does not make the lower-layer fact an engine implementation detail.

## Design Consequences

1. The current `database/` module should not be recreated as one large center
   of gravity. Its responsibilities map across API, runtime, commit,
   persistence, diagnostics, and branch.
2. The current `branch_ops/mod.rs` should not remain the universal branch
   operation file. Branch workflows should use capability branch adapters.
3. The current graph and vector directories should move under the data
   capability contract rather than remain architectural peers.
4. Search should become a retrieval bucket over capability adapters and control
   plane recipes.
5. The `_system_` branch/space model should become explicit before rewriting
   autoembedding, recipes, search manifests, vector shadows, graph relationship
   metadata, or derived-state watermarks.
6. The old subsystem lifecycle should be replaced with explicit service
   lifecycle contracts owned by runtime and orchestration.
7. Storage imports should be reduced by introducing a persistence adapter before
   moving capability code wholesale.
8. Data capability conformance tests should be designed before rewriting all
   capability implementations.
9. Engine re-exports should expose only the executor-facing contract; the
   public product/API surface belongs in executor-next.
10. Data movement should be designed with StrataHub substrate in mind, but sync
    must remain explicit and opt-in.

## Follow-Up Documents

The first-pass engine architecture is now supported by the contract index in
[engine-next/README.md](./engine-next/README.md).

That index owns the detailed contracts for data capabilities, EntityRef,
storage-space IDs, persistence, branch operations, temporal context, control
plane, retrieval, IPC, clone artifacts, public surface cleanup, product-pathway
conformance, errors, testing, and target crate shape.

The remaining architecture sequence after engine-next is:

1. Final public API and CLI implementation plans.

Inference-next is now documented in
[inference-next-architecture.md](./inference-next-architecture.md). It is listed
here as a completed lower model-execution anchor. Intelligence-next is now
documented in
[intelligence-next-architecture.md](./intelligence-next-architecture.md) as the
Strata-aware model orchestration layer.

Those documents are intentionally sequenced after the core/storage/engine
contracts because they consume the engine boundary rather than redefining it.
