# Strata V1 Implementation Roadmap

Status: V1 architecture draft

## Purpose

This document turns the V1 architecture documents into an implementation order.
It is the bridge between architecture and code.

The goal is not to create another conceptual layer. The goal is to make the
build sequence explicit enough that implementation can proceed without
compile-driven compromises, temporary facades, hidden compatibility machinery,
or accidental boundary erosion.

The roadmap has two jobs:

1. Confirm that the V1 architecture documents form one coherent stack.
2. Define the order in which `core-next`, `storage-next`, `engine-next`,
   `inference-next`, `intelligence-next`, executor, CLI, tests, and cutover
   work should land.

## Related Documents

Product anchors:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/strata-v1-user-pathways.md`
4. `docs/product/strata-v1-non-functional-requirements.md`
5. `docs/stratahub/docs/product/stratahub-user-pathways.md`
6. `docs/stratahub/docs/product/stratahub-v1-cli-commands.md`

Architecture anchors:

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/core-next-architecture.md`
3. `docs/architecture/storage-next-architecture.md`
4. `docs/architecture/engine-next-architecture.md`
5. `docs/architecture/inference-next-architecture.md`
6. `docs/architecture/intelligence-next-architecture.md`
7. `docs/architecture/stratahub-substrate-architecture.md`
8. `docs/architecture/runtime-resource-profile-architecture.md`
9. `docs/architecture/v1-error-and-diagnostics-contract.md`
10. `docs/architecture/v1-testing-and-conformance-plan.md`
11. `docs/architecture/v1-engineering-standards.md`
12. `docs/architecture/v1-document-inventory.md`
13. `docs/architecture/v1-open-question-register.md`
14. `docs/architecture/v1-existing-test-inventory-and-porting-plan.md`
15. `docs/architecture/v1-removed-surfaces.md`
16. `docs/architecture/v1-cutover-pr-series.md`
17. `docs/architecture/v1-progress-tracker.md`

Engine contracts:

1. `docs/architecture/engine-next/README.md`
2. `docs/architecture/engine-next/persistence-adapter-contract.md`
3. `docs/architecture/engine-next/primitive-implementation-contract.md`
4. `docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`
5. `docs/architecture/engine-next/retrieval-and-derived-state-contract.md`
6. `docs/architecture/engine-next/ipc-and-command-boundary-contract.md`
7. `docs/architecture/engine-next/product-pathway-conformance-plan.md`

Storage contracts:

1. `docs/architecture/storage-next/README.md`
2. `docs/architecture/storage-next/implementation-patterns.md`
3. `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
4. `docs/architecture/storage-next/l9-storage-api-boundary.md`
5. `docs/spec/strata-storage-format-v1.md`

Milestone implementation plans:

1. `docs/architecture/implementation-plans/m0-m0t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m1-m1t-implementation-plan.md`
3. `docs/architecture/implementation-plans/m2-m2t-implementation-plan.md`
4. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
6. `docs/architecture/implementation-plans/m5-m5t-implementation-plan.md`
7. `docs/architecture/implementation-plans/m6-m6t-implementation-plan.md`
8. `docs/architecture/implementation-plans/m7-m7t-implementation-plan.md`
9. `docs/architecture/implementation-plans/m8-m8t-implementation-plan.md`
10. `docs/architecture/implementation-plans/m9-m9t-implementation-plan.md`
11. `docs/architecture/implementation-plans/m10-m10t-implementation-plan.md`
12. `docs/architecture/implementation-plans/m11-m11t-implementation-plan.md`

## Architecture Integration Review

The V1 documents now describe one stack:

```text
core-next
  -> storage-next
  -> engine-next
  -> intelligence-next
  -> executor / cli / SDK / Strata AI

intelligence-next
  -> inference-next
```

The stack is coherent if these rules remain true during implementation:

1. Only engine-next consumes storage-next directly in normal production code.
2. Storage-next owns generic persistence mechanics and storage-local recovery
   facts.
3. Engine-next owns product semantics, data capabilities, branch behavior,
   command semantics, IPC classification, derived-state meaning, and product
   diagnostics.
4. Intelligence-next owns model-assisted Strata behavior but does not own
   provider execution or persistence.
5. Inference-next owns provider execution and model artifact resolution but
   knows nothing about databases.
6. Executor, CLI, SDK surfaces, and Strata AI consume engine and intelligence
   surfaces, not storage internals.
7. Runtime resource profiling is engine-owned policy; storage and inference
   receive resolved budgets and execution hints.
8. StrataHub V1 integration is the strata-core side of a read-only Hub path:
   clone/info commands, Hub-compatible manifest/object fetch, dataset-card
   rendering, provenance remote refs, opt-in telemetry client behavior, and a
   deterministic client conformance fixture. StrataHub hosting, catalog
   management, publishing, push, auth, hosted runtime, sync, and fleet
   management are not strata-core responsibilities in V1.

## Product Coverage Check

The current architecture covers the V1 product model as follows:

| Product area | V1 architecture owner | Roadmap dependency |
|---|---|---|
| Embedded durable local database | storage-next + engine-next | Storage L1-L9 before engine product cutover |
| Cache mode | storage-next + engine-next | Storage cache backend, WAL-free commit path, engine parity tests |
| Standard/always durability | storage-next | Durable commit policy and recovery tests |
| Read-only open | engine-next over storage-next | Engine open/access-mode tests |
| IPC same-machine sharing | engine-next command semantics + executor/IPC runtime | Engine command boundary before CLI cutover |
| Follower mode removal | engine-next cleanup | Guarded during engine cutover |
| KV/JSON/event/vector/graph capabilities | engine-next | Persistence adapter and capability contracts |
| Graph relationship layer | engine-next | EntityRef contract, graph capability, reverse maps |
| Branching and time travel | storage timeline + engine semantics | Storage commit timeline before branch-from-time |
| Search/retrieval/RAG substrate | engine-next + intelligence-next + inference-next | Engine retrieval before intelligence stages |
| Autoembedding | intelligence-next over engine surfaces | Engine shadow-vector surfaces before intelligence runtime |
| Runtime resource adaptation | engine-next policy + storage/inference hints | Resource profile implementation before product cutover |
| Dataset clone artifacts | engine-next + storage bundles | Clone contract before CLI `strata clone` |
| StrataHub V1 clone/info | engine-next metadata + clone/provenance + CLI/Hub protocol | Clone substrate before M9 Hub integration |

## Out Of V1 Scope

The following are explicitly out of V1 implementation scope. They may return
post-V1 once V1 engine and CLI APIs are stable.

1. **Strata Foundry** (SwiftUI macOS app at
   `/Users/aniruddhajoshi/Documents/GitHub/strata-foundry/`).
   On ice for V1. The FFI bridge will be revisited post-V1 once engine APIs
   stabilize. V1 milestone slices must not couple themselves to Foundry, and
   M10 product cutover does not include the Foundry bridge update.
2. Network server mode.
3. Cross-machine sync and fleet management. V1 includes StrataHub clone/info
   over a read-only Hub protocol; push, pull, sync, deploy, and fleet remain
   post-V1.
4. Migration tooling for pre-V1 development databases.
5. OpenAI-compatible on-prem endpoint adapters (vLLM, NIM, Ollama, LM Studio,
   llama.cpp server). Extension point reserved in inference-next; adapter is
   post-V1.
6. Streaming generation unless explicitly pulled forward by product.
7. Autosearch optimizer. Substrate is preserved in intelligence-next; the
   optimizer is post-V1.
8. The surfaces listed in `docs/architecture/v1-removed-surfaces.md`.

## Open Question Ownership

`docs/architecture/v1-open-question-register.md` is the canonical M0B owner map
for active architecture questions. This roadmap keeps the high-level summary;
the register owns the detailed source coverage, owner milestones, and closure
points.

1. Core-next public surface is closed as the V1 baseline.
   See `V1Q-001`.
2. Storage durable bytes are owned by `M3C` and `M3TA`.
   See `V1Q-002`.
3. Storage test harness names and invocation are owned by `M2D`, `M2E`,
   `M2TB`, and `M2TD`.
   See `V1Q-003`.
4. Engine `StageOutcome` shape is owned by `M6E` and `M6TD`.
   See `V1Q-004`.
5. Cutover PR series is owned by `M10G`.
   See `V1Q-005`.
6. Layer-specific storage, engine, inference, intelligence, product, and spec
   questions map to `V1Q-006` through `V1Q-037`.

No stable V1 crate construction should start from an unowned architecture
question. Each milestone begins by filtering the register for its owner code.

## Implementation Principles

Implementation should follow these constraints:

1. Develop V1 on a dedicated integration branch while `main` is frozen for old
   architecture feature work.
2. Allow V1-line implementation slices to break old crate compatibility while
   still closing each slice and milestone cleanly.
3. Do not keep both old and new architectures alive through permanent adapters.
4. Do not introduce migration machinery unless a focused plan proves it is
   necessary.
5. Reject pre-V1 development databases by default after cutover.
6. Build test harnesses before relying on the code they are meant to test.
7. Keep every phase reviewable: contract, implementation, tests, guardrails,
   then move to the next phase.
8. Avoid one-off vocabulary. New types should fit repeatable patterns named in
   the target architecture documents.
9. Every public type must justify its crate.
10. Every boundary must have a dependency guard or conformance test before
   cutover.
11. Cache mode, local durable mode, read-only mode, and IPC mode must stay
    visible in tests throughout the build, not only at the end.

## Roadmap Summary

The critical path is:

```text
Phase 0: Architecture freeze and tracking
Phase 1: Core-next
Phase 2: Storage-next testkit and crate skeleton
Phase 3: Storage-next backend, layout, format, and durable services
Phase 4: Storage-next table, branch, commit, recovery, and L9 API
Phase 5: Engine-next persistence adapter and control plane
Phase 6: Engine-next capabilities, branch/time, retrieval, IPC, and clone
Phase 7: Inference-next hardening
Phase 8: Intelligence-next orchestration
Phase 9: StrataHub V1 integration
Phase 10: Executor, CLI, SDK, tests, benches, and docs cutover
Phase 11: V1 readiness hardening
```

Inference-next can proceed in parallel with storage-next because it does not
depend on storage or engine. Intelligence-next should wait for engine surfaces
to stabilize, but its fake-provider/testkit work can start earlier.

Milestone scheduling is a DAG, not a strict serial chain:

1. `M0` gates the start of planned implementation.
2. `M1` gates all crates that need core-next atoms.
3. `M2`, `M3`, and `M4` form the storage-next path.
4. `M7` may start after `M1` and can run in parallel with `M2` through `M6`.
5. `M5` depends on enough of `M4` to consume storage-next L9.
6. `M6` depends on `M5`.
7. `M8` depends on the required engine surfaces from `M6` and the inference
   task contracts from `M7`.
8. `M9` depends on the clone substrate and public command conventions from
   `M6`.
9. `M10` depends on the product surfaces from `M6`, `M8`, and `M9`.
10. `M11` depends on all previous milestone gates.

## Progress Nomenclature

Each roadmap phase is a milestone. Each milestone has epics. Each epic is split
into bite-sized implementation slices.

Use this implementation-track code shape:

```text
M{milestone}
M{milestone}{epic-letter}
M{milestone}{epic-letter}{slice-number}
```

Examples:

1. `M1`
   Core-next milestone.
2. `M1A`
   First core-next epic.
3. `M1A1`
   First implementation slice inside that epic.

Every milestone also has a parallel test track:

```text
M{milestone}T
M{milestone}T{test-epic-letter}
M{milestone}T{test-epic-letter}{slice-number}
```

Examples:

1. `M1T`
   Core-next test track.
2. `M1TA`
   First core-next test epic.
3. `M1TA1`
   First implementation slice inside that test epic.

The code is only an identifier. Every plan, issue, PR, and commit message
should include the plain-English title next to it.

These identifiers are planning metadata only. They must not appear in
production crate names, module names, file names, type names, function names,
test names, feature flags, error codes, metric names, telemetry fields, CLI
commands, config keys, or user-facing text. Implementation code should use the
domain vocabulary from the target architecture documents and
`docs/architecture/v1-engineering-standards.md`.

Recommended use:

1. Milestone codes are stable and map to the roadmap phases below.
2. Epic letters are assigned in each milestone implementation plan.
3. Slice numbers are assigned only when an epic is ready to implement.
4. A slice should be small enough to implement, review, fix, and test in one
   focused pass. Aim for no more than about 1,500 lines of net source change;
   larger slices should split unless generated data, golden fixtures, or crate
   renames make that impractical.
5. Review and fix passes do not need separate permanent codes. In conversation
   they can be referenced as "review `M1A1`" and "fix `M1A1`".
6. If a slice grows beyond one focused pass, split it before implementation
   rather than creating temporary code to make it compile.
7. A milestone is not complete until both the implementation track and its test
   track pass their exit gates.
8. Test-track slices may land before, alongside, or after implementation
   slices, but each milestone plan must show how they converge.

Milestone code map:

| Roadmap phase | Milestone code | Milestone title |
|---|---|---|
| Phase 0 | `M0` | Architecture freeze and tracking |
| Phase 1 | `M1` | Core-next |
| Phase 2 | `M2` | Storage-next testkit and crate skeleton |
| Phase 3 | `M3` | Storage-next backend, layout, format, and durable services |
| Phase 4 | `M4` | Storage-next table, branch, commit, recovery, and L9 API |
| Phase 5 | `M5` | Engine-next persistence adapter and control plane |
| Phase 6 | `M6` | Engine-next product semantics |
| Phase 7 | `M7` | Inference-next hardening |
| Phase 8 | `M8` | Intelligence-next orchestration |
| Phase 9 | `M9` | StrataHub V1 integration |
| Phase 10 | `M10` | Executor, CLI, SDK, tests, benches, and docs cutover |
| Phase 11 | `M11` | V1 readiness hardening |

Each milestone implementation plan should start with an epic table:

| Epic code | Epic title | Exit gate |
|---|---|---|
| `M1A` | Example epic title | Example gate |

Each milestone test plan should start with a test epic table:

| Test epic code | Test epic title | Exit gate |
|---|---|---|
| `M1TA` | Example test epic title | Example gate |

Each epic should then define slices:

| Slice code | Slice title | Scope | Required tests |
|---|---|---|---|
| `M1A1` | Example slice title | Files or modules touched | Tests or guards |

Each test epic should define test slices:

| Test slice code | Test slice title | Scope | Required implementation link |
|---|---|---|---|
| `M1TA1` | Example test slice title | Test files or harnesses touched | `M1A1` or milestone-level gate |

This gives the project a stable way to say "we are in `M3B2`" without
requiring anyone to remember cleanup-era names or read historical docs.

`docs/architecture/v1-progress-tracker.md` is the current execution ledger. It
records milestone status, issue/PR label shapes, and the update protocol for
slice status. The roadmap defines the order and gate rules; the tracker records
where execution currently stands.

## Phase 0: Architecture Freeze And Tracking

Goal: make the document set implementation-ready.

Work:

1. Add this roadmap to the V1 architecture reading path.
2. Confirm the resolved core-next surface still satisfies storage-next and
   engine-next implementation plans.
3. Mark `next-charter.md` as historical context only wherever needed.
4. Confirm every engine-next contract listed in `engine-next/README.md` exists
   and has no unowned load-bearing decisions.
5. Confirm storage-next L1-L9 documents agree with the format spec, L9
   boundary, runtime profiles, errors, and testing plan.
6. Create `docs/architecture/v1-progress-tracker.md` as the lightweight
   milestone, epic, slice, issue/PR label, and test-track execution ledger.

Exit criteria:

1. No architecture document has a contradiction that affects crate boundaries.
2. Each remaining open question is either assigned to a phase or explicitly
   post-V1.
3. The first implementation phase can start without guessing ownership.
4. The progress tracker identifies the current milestone status and next ready
   work.

## Phase 1: Core-Next

Goal: build the smallest shared contract crate.

Work:

1. Create the `core-next` crate skeleton.
2. Add only the agreed cross-layer atoms.
3. Keep construction explicit and free of filesystem, network, runtime, or
   database behavior.
4. Add serialization, parse/display, boundary, and property tests for owned
   identifiers and values.
5. Add dependency guard tests proving core-next has no Strata crate dependency.

Expected contents:

1. Branch identity where truly shared.
2. Commit/version/timestamp representation where truly shared.
3. Type-local validation errors for core-owned atoms.
4. No user value model, product entity references, storage transaction IDs, or
   backend address syntax unless a later approved phase reopens ownership.

Exit criteria:

1. Public surface fits in one short table.
2. Every public type has an owner justification.
3. Storage-next and engine-next can depend on it without inheriting product
   policy.

## Phase 2: Storage-Next Testkit And Crate Skeleton

Goal: make storage implementation testable before durable behavior lands.

Work:

1. Create `storage-next` crate skeleton using the target crate-shape document.
2. Add crate-level policy, feature gates, and dependency rules.
3. Build the memory/cache backend skeleton.
4. Build the local filesystem backend skeleton.
5. Build backend conformance harnesses.
6. Build the faulting backend wrapper.
7. Build golden-vector fixture infrastructure.
8. Build fuzz target scaffolding.
9. Build crash harness scaffolding.

Exit criteria:

1. Cache/memory backend passes non-durable L1 conformance.
2. Local filesystem backend passes basic capability declaration tests.
3. Testkit exists without becoming a production API.
4. `wasm32-unknown-unknown` cache/memory compile path is protected.

## Phase 3: Storage-Next Backend, Layout, Format, And Durable Services

Goal: implement the lower storage mechanics before table/commit semantics rely
on them.

Work:

1. Implement L1 backend capability contract.
2. Implement L2 object layout and object naming.
3. Freeze and implement L3 durable format encoding.
4. Implement L4 durable publisher, WAL, manifest, snapshot envelope, and
   checkpoint services.
5. Add fault-window tests for durable publish, WAL append, manifest publish,
   snapshot publish, and quarantine behavior.
6. Add golden vectors and fuzz targets before format bytes are treated as
   stable.

Exit criteria:

1. Durable local publish behavior is crash-testable.
2. Cache mode has no WAL, manifest, snapshot, checkpoint, durable table, or
   quarantine objects.
3. Durable format failures produce stable storage errors.
4. The storage format spec matches the implemented bytes.

## Phase 4: Storage-Next Table, Branch, Commit, Recovery, And L9 API

Goal: finish the storage substrate engine-next will consume.

Work:

1. Implement L5 table runtime: mutable tables, immutable tables, block/cache
   behavior, compaction inputs, TTL metadata, and cursors.
2. Implement L6 branch visibility over storage rows.
3. Implement L7 commit pipeline, version allocation, timestamp stamping, and
   commit timeline rows.
4. Implement `cache`, `standard`, and `always` durability policy behavior.
5. Implement L8 open, recovery, retention, maintenance, repair, and health.
6. Implement L9 storage API boundary and conformance tests.
7. Add model/property tests for table equivalence, branch visibility,
   retention, tombstones, TTL, and timeline resolution.
8. Add crash tests for WAL-before-visible, manifest publish, recovery replay,
   checkpoint/truncation, and ambiguous publish outcomes.

Exit criteria:

1. Engine-next can consume storage through L9 only.
2. Storage recovery health facts are storage-owned.
3. Branch-aware storage row reads support current, version, history, and
   timestamp-to-version substrate needed by engine.
4. Cache, durable local standard, and durable local always modes have separate
   conformance coverage.
5. No product data-capability semantics leak into storage-next.

## Phase 5: Engine-Next Persistence Adapter And Control Plane

Goal: establish the only normal engine path to storage-next.

Work:

1. Create the engine-next crate skeleton and target module buckets.
2. Implement the persistence adapter over storage-next L9.
3. Implement engine-owned physical key construction and storage-space ID
   routing.
4. Implement the control-plane layout: global `_system_` branch and
   branch-local `_system_` space.
5. Implement runtime resource profile resolution and pass resolved storage
   budgets downward.
6. Implement engine error mapping from storage diagnostics to product errors.
7. Add guard tests preventing storage-next imports outside persistence.

Exit criteria:

1. Engine-next opens cache and durable local databases through storage-next.
2. Engine-next can read/write storage-shaped rows only through the persistence
   adapter.
3. Control-plane rows for capability registry, storage-space registry,
   resource profile, and derived-state manifests are created and validated.
4. Product errors preserve storage source chains without exposing storage enum
   names as public API.

## Phase 6: Engine-Next Product Semantics

Goal: implement the V1 database behavior over the persistence adapter.

Work:

1. Implement KV, JSON, event, vector, and graph as repeatable data capability
   adapters over the KV row substrate.
2. Implement EntityRef and graph relationship bindings across capabilities.
3. Implement branch create, branch-from-version, branch-from-time, compare,
   promote, copy, restore, revert, cherry-pick, delete, and conflict handling.
4. Implement temporal context resolution for latest, `getv`, history, `as_of`,
   and branch-from-time.
5. Implement search/retrieval substrate, recipes, BM25 rows, shadow-vector
   rows, graph-aware retrieval, derived-state manifests, and freshness checks.
6. Implement public write and batch semantics without public manual
   transaction sessions.
7. Implement engine command classification for local and IPC-backed handles.
8. Implement dataset clone artifact validation and export/import substrate.
9. Remove the public surfaces listed in
   `docs/architecture/v1-removed-surfaces.md` from the V1 public surface.

Exit criteria:

1. Product-pathway conformance tests pass over engine-next for required V1
   pathways.
2. Engine-next exposes the surfaces intelligence-next requires for
   autoembedding, shadow-vector writes, recipe execution, and derived-state
   freshness.
3. IPC command semantics are transport-independent and serializable.
4. Removed surfaces have guard tests.
5. Cache mode supports the full V1 product API, with durability as the only
   product difference.

## Phase 7: Inference-Next Hardening

Goal: stabilize provider and local model execution before intelligence-next
depends on it.

Work:

1. Keep inference-next independent of storage, engine, and intelligence.
2. Add task-specific `Generator`, `Embedder`, and `Reranker` traits.
3. Add explicit `EmbedRequest`, `EmbedResponse`, `RankRequest`, and
   `RankResponse` DTOs with item-level outcomes.
4. Implement deterministic model-spec parsing.
5. Implement `InferenceCapability` reporting.
6. Enforce network-disabled policy before cloud/provider execution.
7. Harden error mapping into the global `inference.*` registry.
8. Isolate llama.cpp unsafe code under the local runtime boundary and complete
   the focused unsafe audit.
9. Add fake-provider testkit support.

Exit criteria:

1. No-default build passes.
2. Required feature matrix builds pass.
3. Provider mapping, parser, registry, redaction, capability, and fake-provider
   tests pass.
4. Inference-next exposes enough stable surface for intelligence-next without
   knowing Strata database concepts.

## Phase 8: Intelligence-Next Orchestration

Goal: implement model-assisted Strata behavior over engine-next and
inference-next.

Work:

1. Implement Strata-shaped model APIs without broad inference re-exports.
2. Implement query embedding helpers over engine configuration and inference
   `Embedder`.
3. Implement autoembedding queue, flush, reindex, cleanup, status, and
   model-mismatch handling.
4. Implement query expansion with branch-local cache.
5. Implement reranking with recipe-owned top-N and blend weights.
6. Implement RAG prompt/context/citation behavior over engine retrieval hits.
7. Implement generation lifecycle for local and cloud providers.
8. Implement structured stage diagnostics and degradation outcomes.
9. Add dependency guards proving intelligence does not import storage and
   executor/CLI do not import inference directly.

Exit criteria:

1. Intelligence-next consumes only named engine surfaces and inference task
   traits.
2. Autoembedding failures do not affect source write success and are visible
   through health/status.
3. Model-assisted retrieval degrades according to recipe policy and reports
   degradation.
4. Fake-provider tests cover expansion, rerank, RAG, generation, and
   autoembedding paths.

## Phase 9: StrataHub V1 Integration

Goal: make the V1 read-only Hub path part of the release surface before final
crate and CLI cutover.

Work:

1. Implement StrataHub V1 protocol types for Hub URLs, dataset names, content
   hashes, manifests, object references, dataset cards, remote refs, and
   protocol errors.
2. Implement clone transport over Hub manifests and content-addressed objects,
   including hash verification, interrupted-clone handling, destination
   conflict behavior, atomic assembly, local database open-check, and
   engine-owned `origin` remote-ref writes.
3. Add `strata clone <source> <destination>` with `--hub`, `--force`, and
   `--format` behavior through existing CLI conventions.
4. Add `strata info <dataset>` with `--hub`, `--format`, and `--field`
   behavior as the CLI rendering of dataset cards.
5. Register `hub.default` and `telemetry.enabled` with the existing config
   system.
6. Add a deterministic Hub-client conformance fixture for tests: dataset info,
   refs, manifests, objects, telemetry capture, and fixture serving. This is
   not a production StrataHub server.
7. Enforce telemetry default-off behavior and the V1 privacy allowlist.
8. Add hub-neutrality guards: no hidden network behavior, no hard dependency on
   hosted `stratahub.io` outside defaults/docs/tests, no Hub imports from
   storage, and no hosted-Hub/catalog/publish responsibilities in strata-core.
9. Update docs and cutover prerequisites so M10/M11 treat clone/info as V1
   release pathways.

Exit criteria:

1. `strata clone` can clone a deterministic Hub fixture, verify the local
   database, and preserve provenance in an engine-owned remote ref.
2. `strata info` renders dataset-card text/json/short/field output through
   stable CLI fixtures.
3. Clone failures for missing data, hash mismatch, destination conflict,
   interrupted download, and write/open-check failure are structured and do not
   leave a trusted partial destination.
4. Telemetry is opt-in, hub-neutral, and cannot collect dataset names, URLs,
   Hub URLs, local paths, error text, contents, or identifying data.
5. V1 exposes no Hub push, auth, list/search, fork, deploy, sync, or fleet
   commands.

## Phase 10: Executor, CLI, SDK, Tests, Benches, And Docs Cutover

Goal: make the V1 integration line ready for promotion without exposing
`*-next` architecture to users.

Work:

1. Cut product crates over to engine-next and intelligence-next APIs on the V1
   integration branch.
2. Replace old canonical crate implementations with the new stack within the
   V1 line.
3. Shed the `next` suffix from public crate names before the V1 line is
   canonical.
4. Update CLI commands to the V1 product surface.
5. Route IPC-backed access through the command boundary.
6. Add product-path tests through executor and CLI.
7. Update benches and performance regression harnesses.
8. Update docs and remove stale implementation-era terminology.
9. Add workspace dependency guards for retired crates and forbidden edges.
10. Complete `docs/architecture/v1-cutover-pr-series.md` before crate rename
    work begins.

Exit criteria:

1. Public crate graph returns to normal names on the V1 line.
2. Users do not need to know the rewrite happened after promotion.
3. Pre-V1 development databases are rejected with structured format/layout
   errors by default.
4. Executor and CLI do not reach around engine.
5. Product-path tests cover durable local, cache, read-only, IPC, branching,
   time travel, retrieval, graph, vectors, clone, and model-assisted pathways.
6. The cutover PR series has a reviewed execution checklist.

## Phase 11: V1 Readiness Hardening

Goal: move from functionally complete to release-grade.

Work:

1. Run the full storage fault-injection and crash-recovery matrix.
2. Run engine product-path conformance and long-running randomized tests.
3. Run inference feature matrix and opt-in smoke tests where configured.
4. Run intelligence fake-provider and product-path tests.
5. Re-run dependency graph audits.
6. Re-run public API surface audits.
7. Re-run docs terminology scans.
8. Re-run performance benchmarks and compare against threshold policy.
9. Validate runtime resource profiles across fake edge, desktop, server,
   unknown, and explicit-profile hosts.
10. Validate every V1 required product pathway has tests and documentation.
11. Promote the `v1` line to `main` only after the readiness gate and the
    reviewed cutover PR series allow it.

Exit criteria:

1. No V1 required pathway is untested.
2. No upper product crate imports storage directly in normal production code.
3. No hidden network behavior exists.
4. No removed public surface remains except documented compatibility shims, if
   any are explicitly approved.
5. Durable local crash recovery is demonstrably correct for committed data and
   rejected partial state.
6. Cache mode is explicit and never claims crash durability.
7. Error codes, classes, retry policy, and redaction behavior are stable.
8. The crate graph is understandable to a new engineer without reading old
   cleanup-era plans.
9. The `v1` branch is ready for promotion to `main` under the reviewed cutover
   PR series.

## Cutover Strategy

The V1 rewrite should be developed as its own effort on a dedicated integration
branch. `main` should be frozen for old-architecture feature work while the V1
line is active.

Recommended branch model:

1. `main`
   Frozen old architecture. Allow only critical correctness fixes, build fixes,
   security fixes, and documentation updates that do not expand the old
   architecture.
2. `v1`
   Active M0-M11 development line. It may break old crate compatibility and
   old consumers while a slice is in progress.

This strategy is intentionally different from an incremental mainline cutover.
The V1 line optimizes for the target architecture, not for preserving the old
crate graph at every intermediate commit.

The V1 branch still needs discipline:

1. Each slice such as `M3B2` should end with its targeted compile/test gate
   passing.
2. Each milestone such as `M3` should end with its milestone gate passing.
3. Old/new compatibility facades should not be introduced merely to keep the
   old architecture compiling inside the V1 line.
4. Critical fixes merged to `main` should be ported to the V1 branch when
   relevant.
5. The final promotion to `main` should be a release-line promotion, not the
   first time the design is reviewed.

Promotion should be planned, not a single uncontrolled rewrite.

Recommended sequence:

1. Freeze `main` for old-architecture feature work.
2. Create the V1 integration branch.
3. Implement M1-M11 on the V1 branch using milestone, epic, and slice plans.
4. Keep the V1 branch green at slice and milestone gates, not necessarily at
   every transient edit.
5. Cut executor and CLI to the new engine/intelligence APIs on the V1 branch
   after engine product pathways are stable.
6. Remove build-phase crate names from user-facing documentation before
   promotion.
7. Delete retired crates and add guards preventing reintroduction.
8. Reject old pre-V1 databases through a structured open error.
9. Promote the V1 line to `main` only after the V1 readiness gate passes.

Do not add a permanent compatibility layer between old storage and new engine,
or old engine and new storage. If temporary bridge code is unavoidable inside
the V1 branch, it must have an owner, a deletion condition, and a guard
preventing new dependencies on it.

## Test Gate Summary

Each phase should have an explicit test gate:

Every milestone has a parallel test track. The implementation track proves the
code exists. The test track proves the code is trustworthy and that valuable
existing tests have either been ported, rewritten, archived, or intentionally
deleted. A milestone cannot close until both tracks close.

| Phase | Required gate |
|---|---|
| Core-next | public surface, serialization, property, dependency guards |
| Storage skeleton | backend conformance and testkit compile gates |
| Storage durable services | golden, fuzz, fault-window, crash harness gates |
| Storage L5-L9 | model/property, recovery, mode conformance, L9 gates |
| Engine persistence | forbidden storage import guard and storage-error mapping |
| Engine product semantics | product-path conformance and removed-surface guards |
| Inference | feature matrix, parser, provider mapping, redaction, fake providers |
| Intelligence | fake-provider product paths, dependency guards, degradation tests |
| StrataHub V1 integration | Hub protocol/client conformance, clone/info CLI, telemetry privacy, hub-neutrality |
| Cutover | workspace graph audit, CLI/executor pathways, docs terminology scan |
| V1 readiness | full conformance, crash, fuzz, performance, and NFR checks |

This table is the canonical milestone gate summary. The per-milestone plans
explain how each milestone reaches the relevant row; they should not invent a
weaker gate.

## Risk Register

The major implementation risks are known:

1. Storage format drift.
   Mitigation: freeze format specs, golden vectors, and fuzz decoders before
   downstream dependencies build on them.

2. Temporary compatibility structures becoming permanent.
   Mitigation: use parallel crates and deletion gates instead of facades.

3. Engine reintroducing data-capability silos.
   Mitigation: implement shared capability adapter patterns and forbid
   top-level graph/vector/search peer crates.

4. Upper layers reaching around engine.
   Mitigation: dependency guards and product-path APIs before consumer cutover.

5. Cache mode accidentally inheriting durable behavior.
   Mitigation: cache-mode conformance tests that assert no WAL, manifest,
   snapshot, checkpoint, or durable table objects.

6. Runtime resource profiles becoming advisory only.
   Mitigation: fake-host tests that assert resolved budgets are applied to
   storage, engine, derived-state, and inference hints.

7. Intelligence features hiding model/provider degradation.
   Mitigation: structured stage outcomes and fake-provider failure tests.

8. IPC becoming a server product by accident.
   Mitigation: command-boundary tests and local same-machine scope assertions.

## Implementation Planning Rule

Before each phase starts, write a short phase implementation plan with:

1. Files or crates touched.
2. Public types added or moved.
3. Tests added before or with implementation.
4. Dependency guards.
5. Cutover or deletion steps.
6. Known non-goals.
7. Review checklist.

The milestone plans live under `docs/architecture/implementation-plans/`. Each
plan pairs the implementation track and matching test track, for example
`docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`.

The milestone plan should be much smaller than the architecture documents. Its
job is to keep each implementation slice honest.

Before each milestone starts, update the matching test-track section in the
same plan with:

1. Existing tests classified for the milestone.
2. Tests ported unchanged.
3. Tests rewritten against V1 contracts.
4. Tests archived as evidence.
5. Tests deleted because the behavior is removed.
6. New conformance, property, fuzz, fault, crash, product-path, benchmark, and
   guard tests needed for the milestone.
7. Commands required to close the milestone test gate.

## Acceptance Criteria For This Roadmap

This roadmap is sufficient when:

1. A contributor can see the implementation order without reconstructing it
   from scattered documents.
2. Every major V1 architecture layer has a phase.
3. Every phase has an exit gate.
4. Cutover is described as deliberate replacement, not indefinite coexistence.
5. Testing is front-loaded rather than treated as cleanup.
6. Known remaining decisions are assigned to the phase that needs them.
7. The roadmap preserves the core boundary: storage mechanics below engine,
   product semantics in engine, model orchestration in intelligence, provider
   execution in inference.
