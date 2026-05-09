# EG8 Implementation Plan

## Purpose

`EG8` closes the intelligence side of engine consolidation.

After `EG4` through `EG7`, graph, vector, search, product-open behavior, and
executor's storage access have moved behind engine. Intelligence should now be a
model and inference adapter above engine, not a peer runtime layer and not a
place where storage or subsystem assembly leaks back in.

The target stack remains:

```text
core -> storage -> engine -> intelligence -> executor -> cli
```

In that stack, intelligence may depend on:

- `strata-core` for shared value/identity DTOs
- `strata-engine` for database, search, vector, graph, branch, and recipe
  contracts
- `strata-inference` for optional model execution behind the `embed` feature

Intelligence must not depend on retired peer runtime crates, storage, executor,
or CLI. Engine must not depend on intelligence or inference.

This is the single implementation plan for the `EG8` phase. Lettered sections
such as `EG8A`, `EG8B`, and `EG8C` are tracked here rather than in separate
letter-specific documents.

Read this with:

- [engine-consolidation-plan.md](./engine-consolidation-plan.md)
- [engine-crate-map.md](./engine-crate-map.md)
- [eg4-implementation-plan.md](./eg4-implementation-plan.md)
- [eg5-implementation-plan.md](./eg5-implementation-plan.md)
- [eg6-implementation-plan.md](./eg6-implementation-plan.md)
- [eg7-implementation-plan.md](./eg7-implementation-plan.md)
- [../storage/v1-storage-consumption-contract.md](../../storage/v1-storage-consumption-contract.md)

## Scope

`EG8` owns:

- rebaselining `strata-intelligence`'s normal dependency graph after graph,
  vector, search, security, and executor-legacy retirement
- confirming intelligence imports engine-owned graph/vector/search contracts
  rather than retired peer crates
- preserving the inference boundary: optional model/provider execution stays in
  intelligence/inference and does not move into engine
- removing intelligence test-only subsystem assembly where tests still name
  `GraphSubsystem`, `VectorSubsystem`, `SearchSubsystem`, `OpenSpec`, or
  `with_subsystem`
- tightening guard tests so intelligence cannot regain storage, retired peer
  crates, direct inference consumers above intelligence, or subsystem assembly
  outside engine
- updating crate-map and consolidation docs to describe the final intelligence
  boundary

`EG8` does not own:

- moving inference provider clients into engine
- redesigning generation, embedding, rerank, RAG, or query expansion semantics
- redesigning model registry/download behavior
- deleting `strata-intelligence`
- deleting `strata-inference`
- removing executor's dependency on intelligence
- removing CLI's optional embed dependency on intelligence; that is an `EG9`
  final graph decision unless it blocks this phase
- changing storage, WAL, manifest, checkpoint, snapshot, search segment, or
  vector sidecar formats

## Load-Bearing Boundary Rules

Engine is below intelligence. The forbidden dependency directions are:

```text
strata-engine -> strata-intelligence
strata-engine -> strata-inference
strata-engine -> provider HTTP/client crates
```

Intelligence may call engine's public product/runtime APIs and engine-owned
search/vector/graph contracts. It must not reconstruct engine runtime assembly
by naming subsystem structs. If a test needs a database with product runtime
hooks, add or use an engine-owned helper that expresses the desired runtime
shape directly.

Allowed intelligence imports:

- `strata_engine::Database`
- engine-owned primitive facades such as `VectorStore`, `KVStore`,
  `JsonStore`, `EventLog`, and `SpaceIndex`
- engine-owned search contracts such as `SearchHit`, `ExpandedQuery`,
  `QueryType`, `RerankConfig`, `Recipe`, `BlendWeights`, and `RerankScore`
- optional `strata_inference::*` imports behind `#[cfg(feature = "embed")]`

Forbidden intelligence imports:

- `strata_storage::*`
- `strata_graph::*`
- `strata_vector::*`
- `strata_search::*`
- `strata_security::*`
- `strata_executor_legacy::*`
- `strata_executor::*`
- `strata_cli::*`

Forbidden intelligence runtime assembly:

- `GraphSubsystem`
- `VectorSubsystem`
- `SearchSubsystem`
- `OpenSpec::with_subsystem`
- `OpenSpec::with_subsystems`

## Starting State

The default normal dependency graph for intelligence is already close to the
target:

```text
strata-intelligence
├── serde
├── serde_json
├── sha2
├── strata-core
├── strata-engine
└── tracing
```

With `--features embed`, intelligence adds `strata-inference`:

```text
strata-intelligence
├── strata-core
├── strata-engine
├── strata-inference
└── supporting crates
```

The current intelligence manifest has no normal dependency on:

- `strata-storage`
- `strata-graph`
- `strata-vector`
- `strata-search`
- `strata-security`
- `strata-executor-legacy`
- `strata-executor`
- `strata-cli`

The current production imports are also mostly correct:

- `expand.rs`, `expand_cache.rs`, `rerank.rs`, and `rag/*` import search
  contracts from `strata_engine::search`.
- `embed/runtime.rs` and `shadow.rs` import `VectorStore` and shadow constants
  from engine.
- `generate.rs`, `embed/mod.rs`, `embed/download.rs`, and `lib.rs` import or
  re-export inference types behind the `embed` feature.
- engine has no normal dependency on intelligence or inference.

The remaining cleanup is test/runtime-shape and guard work:

- `crates/intelligence/tests/expand_cache_fork_test.rs` directly names
  `GraphSubsystem` and calls `.with_subsystem(GraphSubsystem)`.
- `crates/intelligence/src/embed/runtime.rs` test code directly names
  `GraphSubsystem`, `VectorSubsystem`, `SearchSubsystem`, `OpenSpec`, and
  `.with_subsystem(...)`.
- some intelligence tests use engine's `search_only_*_spec` helpers, which is
  acceptable if the helper expresses an engine-owned runtime shape and does not
  require intelligence to name subsystem structs.

## EG8A - Rebaseline Intelligence Dependency And Import Surface

**Goal:**

Record the current intelligence graph and source import inventory before
tightening the boundary.

**Work:**

- run `cargo tree -p strata-intelligence --edges normal --depth 1`
- run `cargo tree -p strata-intelligence --features embed --edges normal --depth 1`
- scan intelligence source, tests, and manifest for retired peer crate names:
  both underscore and hyphen spellings of graph, vector, search, storage,
  security, and executor-legacy
- scan engine source and manifest for `strata_intelligence`,
  `strata-inference`, and `strata_inference`
- scan executor and CLI for direct `strata_inference` usage; executor should
  consume inference-facing public types through intelligence
- record any intentional exceptions in this file before implementing later
  sections

**Acceptance:**

- intelligence's default graph is `core + engine + local support crates`
- intelligence's `embed` graph adds inference only as an optional direct
  dependency
- engine has no intelligence or inference dependency
- executor and CLI have no direct inference dependency
- all retired peer-crate references are either absent or test/doc strings
  intentionally covered by guard tests

**Completed in EG8A:**

- `cargo tree -p strata-intelligence --edges normal --depth 1` shows only
  `serde`, `serde_json`, `sha2`, `strata-core`, `strata-engine`, and
  `tracing`.
- `cargo tree -p strata-intelligence --features embed --edges normal --depth 1`
  adds `strata-inference` as the only direct Strata crate beyond core and
  engine. `strata-storage` remains present transitively through engine, which
  is the intended stack shape.
- The retired peer-crate scan over `crates/intelligence` found no
  underscore or hyphen spellings for graph, vector, search, storage, security,
  or executor-legacy source or manifest matches.
- The engine scan found no `strata_intelligence`, `strata-inference`, or
  `strata_inference` source or manifest matches.
- The executor, CLI, and root `src` scan found no direct
  `strata_inference` or `strata-inference` matches. Executor's generation
  handler consumes `ProviderKind` through `strata_intelligence`, which is the
  intended facade.
- The remaining intentional exception is test-only runtime assembly in
  intelligence:
  - `crates/intelligence/tests/expand_cache_fork_test.rs` names
    `GraphSubsystem` and calls `.with_subsystem(GraphSubsystem)`.
  - `crates/intelligence/src/embed/runtime.rs` test code names `OpenSpec`,
    `GraphSubsystem`, `VectorSubsystem`, `SearchSubsystem`, and
    `.with_subsystem(...)`.
  These are the implementation inputs for `EG8D`; they are not production
  dependency or import violations.
- Verification: `cargo check -p strata-intelligence` and `cargo fmt --check`
  pass. `cargo check -p strata-intelligence --features embed` currently fails
  during `strata-inference`'s custom build script/CMake setup, before
  `strata-intelligence` library compilation, because
  `crates/inference/vendor/llama.cpp` is present but does not contain
  `CMakeLists.txt`; treat that as an inference vendor setup issue to resolve or
  document in `EG8C`, not as an intelligence dependency surface regression.

## EG8B - Preserve Engine-Owned Search, Vector, And Graph Contracts

**Goal:**

Make intelligence's database-domain imports intentionally engine-owned.

**Work:**

- review `expand.rs`, `expand_cache.rs`, `rerank.rs`, `rag/*`,
  `embed/runtime.rs`, and `shadow.rs`
- remove any stale comments that describe graph/vector/search as peer crates
- keep query expansion, rerank blending, search hit DTOs, recipes, and vector
  facades imported from `strata_engine`
- avoid creating intelligence-local mirror DTOs for engine contracts unless
  they are serialization bridges for persisted intelligence-owned state
- preserve current behavior for cache serialization, rerank blending, RAG
  prompt formatting, shadow embedding cleanup, and embed queueing

**Acceptance:**

- no production intelligence module imports retired graph/vector/search crates
- persisted cache DTOs remain compatible with existing engine-owned
  `ExpandedQuery` and `QueryType`
- `shadow.rs` and `embed/runtime.rs` use engine primitive APIs, not storage keys
  or namespaces
- `cargo test -p strata-intelligence` passes without behavior changes

**Completed in EG8B:**

- Production intelligence imports for expansion, expansion cache, rerank, RAG,
  shadow cleanup, and auto-embed runtime all use engine-owned contracts:
  `strata_engine::search::*`, `strata_engine::Database`,
  `strata_engine::VectorStore`, and other engine primitive facades.
- The persisted expansion-cache bridge remains intelligence-owned only for JSON
  serialization compatibility and converts directly to/from engine-owned
  `ExpandedQuery` and `QueryType`.
- `shadow.rs` and `embed/runtime.rs` continue to use engine primitive APIs for
  shadow vector collections; they do not import storage or construct storage
  keys directly.
- Stale comments that described search as a substrate or cache inheritance as a
  storage-layer concern now describe engine-owned search retrieval and engine
  branch/version semantics without naming storage/COW mechanics above engine.
- The only remaining intelligence references to subsystem structs or
  `.with_subsystem(...)` are the test-only runtime assembly cases already
  recorded in `EG8A`; those remain assigned to `EG8D`.
- Verification: `cargo test -p strata-intelligence` passes, but the
  embed-gated `expand_cache_fork_test.rs` integration tests are not exercised
  by that command. `cargo test -p strata-intelligence --features embed`
  currently reaches the pre-existing `strata-inference` llama.cpp vendor setup
  failure documented in `EG8A`; feature-enabled behavioral verification remains
  blocked until `EG8C` resolves or explicitly documents that vendor setup.

## EG8C - Preserve The Inference Boundary

**Goal:**

Keep model execution above engine and keep executor/CLI using intelligence as
the inference facade.

**Work:**

- verify inference imports in intelligence are behind `#[cfg(feature = "embed")]`
  or feature-gated modules
- keep `strata-inference` optional in `crates/intelligence/Cargo.toml`
- preserve feature forwarding:
  - `embed -> dep:strata-inference + strata-inference/local +
    strata-inference/download`
  - `anthropic -> embed + strata-inference/anthropic`
  - `openai -> embed + strata-inference/openai`
  - `google -> embed + strata-inference/google`
- keep executor free of direct `strata-inference` dependency and imports
- keep engine free of direct `strata-inference` and provider ownership
- decide whether the existing intelligence re-exports of inference types are
  still the desired public facade; if they remain, document them as the
  intentional boundary that prevents executor from depending on inference

**Acceptance:**

- `cargo check -p strata-intelligence`
- `cargo check -p strata-intelligence --features embed` when the local
  inference vendor tree is populated; in this workspace, use
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1` for boundary compile
  checks
- `cargo check -p strata-executor --features embed` when the local inference
  vendor tree is populated; in this workspace, use
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1` for boundary compile
  checks
- `cargo check -p strata-cli --features embed` when the local inference vendor
  tree is populated; in this workspace, use
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1` for boundary compile
  checks
- `rg "strata_inference|strata-inference" crates/engine crates/executor crates/cli src`
  returns no production dependency/import matches outside feature strings that
  route through intelligence

**Completed in EG8C:**

- `strata-inference` remains an optional intelligence dependency. The manifest
  now sets `default-features = false` and forwards the `local` and `download`
  inference features explicitly from `embed`, so the default-feature behavior is
  no longer hidden.
- Intelligence remains the only crate that imports `strata_inference` directly.
  All inference imports are behind `#[cfg(feature = "embed")]` modules,
  feature-gated tests, or the feature-gated re-export facade in `lib.rs`.
- The existing `strata_intelligence` re-exports of `GenerateRequest`,
  `GenerateResponse`, `StopReason`, `InferenceError`, `ProviderKind`,
  `ModelRegistry`, `ModelTask`, and inference engine types remain intentional:
  executor and CLI consume inference-facing types through intelligence rather
  than depending on `strata-inference` directly.
- Engine has no intelligence or inference dependency/import matches. Executor,
  CLI, and root `src` have no direct `strata_inference` or `strata-inference`
  source/import matches; their embed feature strings route through
  `strata-intelligence`.
- Local inference still requires a populated `crates/inference/vendor/llama.cpp`
  tree for real native builds. For boundary compile checks in this workspace,
  use the existing build-script escape hatch:
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check ...`.
  This keeps EG8C honest without changing product runtime behavior.
- Verification: `cargo fmt --check`, `cargo check -p strata-intelligence`, and
  the embed compile checks pass with
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1` for intelligence,
  executor, and CLI. The cloud-provider feature edge was checked with
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p
  strata-intelligence --features anthropic,openai,google`. Direct inference
  scans over engine, executor, CLI, and root `src` return no matches outside
  intelligence.

## EG8D - Remove Intelligence Subsystem Assembly

**Goal:**

Stop intelligence tests from assembling engine runtime subsystems directly.

**Work:**

- replace direct `GraphSubsystem`, `VectorSubsystem`, and `SearchSubsystem`
  usage in intelligence tests with engine-owned open helpers
- replace direct `OpenSpec::with_subsystem(...)` calls in intelligence tests
  with helpers such as:
  - an existing `Database::cache()` / `Database::open(...)` path when it
    already creates the required engine-owned runtime hooks
  - an existing engine-owned `search_only_*_spec` helper when the test
    intentionally needs search-only runtime behavior
  - a new engine test-support helper if the test needs full product runtime
    hooks without exposing subsystem structs above engine
- preserve disk-backed branch fork coverage in
  `expand_cache_fork_test.rs`; it must still exercise real branch/version
  inheritance through durable engine branch state, not a memory-only shortcut
- preserve auto-embed runtime tests in `embed/runtime.rs`; they still need
  graph/vector/search hooks if the tested behavior requires them, but
  intelligence should ask engine for that runtime shape instead of constructing
  subsystem lists

**Acceptance:**

- no intelligence source or test file names:
  - `GraphSubsystem`
  - `VectorSubsystem`
  - `SearchSubsystem`
  - `.with_subsystem`
  - `.with_subsystems`
- intelligence tests still pass without `embed`; embed-gated tests compile in
  this workspace and execute when the local inference vendor/native build is
  available
- engine-owned test helpers, if added, are named by runtime intent rather than
  by subsystem list

**Completed in EG8D:**

- `crates/intelligence/tests/expand_cache_fork_test.rs` now opens its
  disk-backed runtime through engine-owned `open_product_database()` and
  unwraps the local product-open outcome. The test still uses a real
  disk-backed primary database, creates branches through `BranchService`, and
  exercises branch fork/version inheritance.
- `crates/intelligence/src/embed/runtime.rs` test setup now opens an ephemeral
  product runtime through engine-owned `open_product_cache()` instead of
  constructing graph, vector, and search subsystems directly.
- No new engine helper was needed; the existing product-open API already names
  the required runtime intent and keeps subsystem composition inside engine.
- Verification: `cargo fmt --check`, `cargo test -p strata-intelligence`, and
  `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p
  strata-intelligence --features embed --tests` pass. A real embed-gated test
  run is blocked in this workspace by the known missing
  `crates/inference/vendor/llama.cpp/CMakeLists.txt` vendor setup, so EG8D only
  claims embed test compilation here. The subsystem assembly scan for
  graph/vector/search subsystem names and `with_subsystem` builders under
  `crates/intelligence` returns no matches.

## EG8E - Guard And Documentation Closeout

**Goal:**

Make the intelligence boundary enforceable before `EG9`.

**Work:**

- extend `tests/storage_surface_imports.rs` or add a focused guard so
  intelligence cannot regain:
  - direct storage imports
  - retired graph/vector/search/security/executor-legacy dependencies
  - direct `strata-inference` use outside the intelligence crate
  - direct subsystem assembly
- update [engine-crate-map.md](./engine-crate-map.md)
- update [engine-consolidation-plan.md](./engine-consolidation-plan.md) if
  EG8 implementation changes the summary
- verify the final graph:

```text
strata-storage      -> strata-core
strata-engine       -> strata-core, strata-storage
strata-intelligence -> strata-core, strata-engine, strata-inference (optional)
strata-executor     -> strata-core, strata-engine, strata-intelligence
strata-cli          -> strata-executor, strata-intelligence (optional embed)
stratadb            -> strata-executor
```

**Acceptance:**

- guard tests fail if intelligence imports storage or a retired peer runtime
  crate
- guard tests fail if engine imports intelligence or inference
- guard tests fail if executor or CLI directly import inference
- guard tests fail if intelligence source/tests instantiate subsystem structs
- EG9 starts from dependency graph closeout only, not intelligence cleanup

**Completed in EG8E:**

- Added focused guards in `tests/storage_surface_imports.rs` for intelligence's
  boundary:
  - intelligence Rust files and manifests under the full crate tree may not
    import storage or retired graph/vector/search/security/executor-legacy
    runtime crates
  - intelligence Rust files under the full crate tree may not name `OpenSpec`,
    `GraphSubsystem`, `VectorSubsystem`, `SearchSubsystem`, `with_subsystem`,
    or `with_subsystems`
  - engine may not import intelligence or inference
  - executor, CLI, and the root package may not import inference directly
- Updated [engine-crate-map.md](./engine-crate-map.md) to separate the default
  normal graph from optional `embed`/`strata-inference` edges.
- Updated [engine-consolidation-plan.md](./engine-consolidation-plan.md) so
  `EG8E` is complete and `EG9` starts from dependency graph closeout, not
  intelligence cleanup.
- Verification: `cargo test -p stratadb --test storage_surface_imports` passes
  with 29 guard/self-tests.

## Verification

Run targeted checks as each section lands:

```bash
cargo tree -p strata-intelligence --edges normal --depth 1
cargo tree -p strata-intelligence --features embed --edges normal --depth 1
cargo tree -p strata-engine --edges normal --depth 1
cargo tree -p strata-executor --edges normal --depth 1
cargo tree -p strata-cli --edges normal --depth 1
rg -n "strata_(graph|vector|search|storage|security|executor_legacy)|strata-(graph|vector|search|storage|security|executor-legacy)" crates/intelligence -g '*.rs' -g 'Cargo.toml'
rg -n "strata_intelligence|strata-inference|strata_inference" crates/engine -g '*.rs' -g 'Cargo.toml'
rg -n "strata_inference|strata-inference" crates/executor crates/cli src -g '*.rs' -g 'Cargo.toml'
rg -n "OpenSpec|GraphSubsystem|VectorSubsystem|SearchSubsystem|with_subsystem|with_subsystems" crates/intelligence -g '*.rs'
cargo test -p strata-intelligence
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-intelligence --features embed
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-executor --features embed
STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1 cargo check -p strata-cli --features embed
# Requires a real local inference vendor/native build and model assets.
cargo test -p strata-intelligence --features embed
cargo test -p stratadb --test storage_surface_imports
```

Feature-enabled tests require the local inference vendor/native build and model
assets. In workspaces without a populated `crates/inference/vendor/llama.cpp`
tree, use `STRATA_INFERENCE_SKIP_LLAMA_CPP_BUILD_FOR_CHECK=1` only for
compile-only boundary checks; do not use that check-only escape hatch for tests.
