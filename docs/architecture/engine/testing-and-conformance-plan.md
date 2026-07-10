# Engine-Next Testing And Conformance Plan

Status: V1 architecture draft

## Purpose

Engine-next should be built around conformance from the start. The engine is
where Strata turns storage rows into product behavior: data capabilities,
branching, time travel, retrieval, graph relationships, clone artifacts, IPC,
runtime policy, and diagnostics.

The storage testing plan proves the storage substrate. This document defines
the engine-side test plan over that substrate:

1. Which test families engine-next needs.
2. Which reusable harnesses must exist.
3. What each engine architecture bucket must prove.
4. How API, command, CLI, and IPC surfaces stay aligned.
5. How errors, redaction, commit outcomes, and diagnostics are tested.
6. What must be green before V1 can claim engine conformance.

The goal is not just code coverage. The goal is a test suite that makes product
semantics hard to accidentally corrupt.

## Related Documents

Read this with:

1. `docs/architecture/v1-testing-and-conformance-plan.md`
2. `docs/architecture/v1-error-and-diagnostics-contract.md`
3. `docs/architecture/engine/error-and-diagnostics-contract.md`
4. `docs/architecture/engine-architecture.md`
5. `docs/architecture/engine/README.md`
6. `docs/architecture/engine/primitive-implementation-contract.md`
7. `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`
8. `docs/architecture/engine/storage-space-id-registry.md`
9. `docs/architecture/engine/persistence-adapter-contract.md`
10. `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
11. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
12. `docs/architecture/engine/control-plane-layout-contract.md`
13. `docs/architecture/engine/retrieval-and-derived-state-contract.md`
14. `docs/architecture/engine/ipc-and-command-boundary-contract.md`
15. `docs/architecture/engine/dataset-clone-artifact-contract.md`
16. `docs/architecture/engine/product-pathway-conformance-plan.md`
17. `docs/product/strata-v1-product-requirements.md`
18. `docs/product/strata-v1-non-functional-requirements.md`

## Relationship To Storage Testing

`docs/architecture/v1-testing-and-conformance-plan.md` owns storage-next L1-L9
testing. This document starts at engine-next.

Storage tests prove:

1. Backend IO.
2. Object layout.
3. Durable formats.
4. WAL, manifest, snapshot, and table publication.
5. Branch-aware storage mechanics.
6. Commit durability and recovery mechanics.
7. L9 storage API behavior.

Engine tests prove:

1. Product API behavior.
2. Data capability semantics over storage rows.
3. EntityRef and relationship behavior.
4. Branch workflows and capability adapters.
5. Version and timestamp time-travel semantics.
6. Control-plane behavior.
7. Derived-state, retrieval, and recipe behavior.
8. Runtime, access mode, IPC, clone, and diagnostics behavior.

Engine tests may use storage-next through the persistence adapter. Normal
engine tests must not reach below storage-next L9 or assert on storage
implementation internals.

## Current Code Evidence

The current repository already has many useful tests, but their organization
reflects the historical implementation:

1. `tests/engine/` covers database behavior, data operations, concurrency,
   branch isolation, stress, and adversarial cases.
2. `crates/engine/tests/` covers open, recovery, branch isolation,
   transactions, config, crash simulation, codec, and surface regressions.
3. `tests/executor/` covers command dispatch, serialization, errors, product
   surface, branch invariants, and current transaction-session behavior.
4. `tests/intelligence/` covers search, hybrid retrieval, fusion, scoring,
   budgets, identity, explainability, and indexing.
5. `tests/durability/` and `tests/integration/` cover recovery, branching,
   degradation, retention, merge lineage, and mode behavior.
6. Guard tests such as `tests/storage_surface_imports.rs` and
   `tests/engine_surface_imports.rs` already enforce architecture boundaries.

The target is not to discard this evidence. The target is to port or rewrite it
into a smaller number of repeatable conformance suites:

```text
data capability conformance
branch and temporal conformance
command-boundary conformance
runtime and IPC conformance
retrieval and derived-state conformance
error and diagnostics conformance
product-pathway conformance
```

## Goals

1. Make engine behavior testable through contracts, not historical module
   structure.
2. Use shared conformance suites for KV, JSON, event, vector, and graph instead
   of bespoke tests for every operation.
3. Prove branch, version, timestamp, history, diff, merge, restore, and
   branch-from-history semantics with model-based tests.
4. Prove local API, serializable command DTOs, CLI, and IPC expose the same
   product semantics.
5. Prove structured errors, retry policy, commit outcome, and redaction through
   every public boundary.
6. Prove derived state is either correct, stale-refused, rebuilding, or clearly
   unavailable.
7. Prove clone artifacts materialize normal databases and reject corrupt or
   partial artifacts before promotion.
8. Keep storage mechanics out of engine tests unless the test is explicitly for
   the persistence adapter boundary.
9. Build reusable harnesses: fake storage L9, faulting persistence adapter,
   deterministic clocks, command goldens, IPC harnesses, and capability models.
10. Keep removed surfaces absent with guard tests.

## Non-Goals

1. This document does not redefine storage-next testing.
2. This document does not require hosted StrataHub tests for V1.
3. This document does not require Strata AI assistant UX tests.
4. This document does not freeze exact Rust module names.
5. This document does not define benchmark thresholds.
6. This document does not require production OpenDAL/S3 engine conformance for
   V1 unless such a backend is explicitly shipped.
7. This document does not preserve public transaction-session tests as V1
   product conformance.
8. This document does not require optional model-dependent pathways to ship.

Performance regression benchmarks remain governed by current project rules and
future benchmark plans. Engine-next must not use this document to weaken those
requirements.

## Test Taxonomy

Engine-next should use a small set of repeatable test families.

### Unit Tests

Unit tests prove one local invariant:

1. Input validation.
2. EntityRef construction and parsing.
3. Storage-space registry lookup.
4. Capability-local key/value codecs.
5. Branch name and lifecycle validation.
6. Temporal selector validation.
7. Recipe validation.
8. Access-mode classification.
9. Error status construction.
10. Redaction.

### Shared Conformance Tests

Shared conformance tests run the same behavioral suite against multiple
implementations or surfaces.

Required shared suites:

1. Data capability suite.
2. Capability branch-adapter suite.
3. Temporal read suite.
4. Command-boundary suite.
5. Error/status suite.
6. Read-only/access-mode suite.
7. Derived-state health suite.

Shared suites are the main defense against five different capability styles
emerging inside engine-next.

### Model And Property Tests

Property tests generate state and compare engine behavior to a simpler model.

Target models:

1. Branch DAG and branch lifecycle model.
2. Per-key history model.
3. Timestamp-to-version timeline model.
4. Capability latest/version/as-of/history model.
5. Branch diff/merge/copy/restore model.
6. Relationship graph reachability model.
7. Search result deterministic-order model for fixed scores.
8. Command read/write classification model.

The model can be slower and simpler than production code. Its job is to make
semantic drift obvious.

### Golden Tests

Golden tests freeze public wire and automation surfaces:

1. Command request JSON.
2. Command response JSON.
3. Command error status JSON.
4. CLI JSON output.
5. CLI exit-code table.
6. IPC protocol handshake/status examples when defined.
7. Clone artifact manifest examples.
8. Health/describe output examples.

Golden regeneration must be explicit. Normal tests must not update fixtures
incidentally.

### Fuzz Tests

Engine fuzzing targets untrusted product input and public payloads:

1. Command DTO decoders.
2. CLI parsers where practical.
3. EntityRef parser.
4. Branch and space name parsers.
5. JSON path parser.
6. Recipe parser.
7. Filter/query/search request parser.
8. Clone artifact manifest parser.
9. Import payload parser.
10. Error-status deserializer.

Fuzz targets must prove:

1. No panic.
2. No unbounded allocation from user-controlled lengths.
3. Invalid input returns registered error codes.
4. Secrets are not echoed from malformed payloads.
5. Unknown future fields are handled according to the command-boundary contract.

### Fault-Injection Tests

Engine fault tests inject failures at the persistence, runtime, command, and
derived-state boundaries:

1. Persistence read failure.
2. Persistence write failure before commit starts.
3. Persistence ambiguous commit.
4. Durable-but-not-visible commit.
5. Post-commit hook failure.
6. Derived-state update failure.
7. Recovery health degradation.
8. IPC disconnect before command execution.
9. IPC disconnect after possible write commit.
10. Clone artifact read/validate/materialize failure.
11. Disabled network/model/provider failure for optional features.
12. Runtime resource profile detection failure.

Fault tests should use deterministic injection. Random fault schedules can be
added after deterministic cases exist.

### Crash-Recovery Product Tests

Storage owns low-level crash recovery. Engine still needs product-level
crash-recovery tests:

1. Committed KV/JSON/event/vector/graph rows survive reopen.
2. Branch DAG and branch metadata survive reopen.
3. Timeline bounds survive reopen.
4. Derived state is either recovered, rebuilt, marked stale, or refused.
5. Clone materialization does not promote partial destinations.
6. IPC owner crash leaves clear reopen behavior and stale socket handling.
7. Recovery health appears in public diagnostics.
8. Ambiguous commit outcomes remain visible where storage reports uncertainty.

These tests can reuse storage crash harnesses, but assertions happen through
engine/product surfaces.

### Integration And Product-Path Tests

Integration tests prove full product workflows over public boundaries:

1. API-only embedded use.
2. Command DTO execution.
3. CLI human and JSON modes.
4. IPC local sharing.
5. Agent/plugin style command execution.
6. Clone then open.
7. Branch then time travel.
8. Search then branch.
9. Graph relationship traversal then entity fetch.
10. Recovery then diagnostics.

The product-pathway matrix is defined in
`docs/architecture/engine/product-pathway-conformance-plan.md`. This
document defines the harnesses and bucket tests that make that matrix feasible.

### Long-Running And Adversarial Tests

Long-running tests explore interactions after deterministic cases exist:

1. Mixed capability writes across many branches.
2. Random fork/merge/copy/restore/revert operations.
3. Random time-travel reads and history retention.
4. Random derived-state invalidation/rebuild schedules.
5. Mixed API/IPC clients.
6. Crash/reopen cycles during product workflows.
7. Large search/vector/graph result sets within configured budgets.
8. Runtime profiles from small edge budgets to server budgets.

These tests are not a substitute for targeted conformance. They catch
interactions after contracts are already pinned.

## Required Test Infrastructure

### Engine Testkit

Engine-next should have a reusable testkit for:

1. Opening cache, durable local, read-only, and IPC-backed databases.
2. Seeding multi-capability datasets.
3. Building branch DAGs and known timelines.
4. Creating clone artifact fixtures.
5. Generating command DTOs.
6. Capturing CLI JSON and exit codes.
7. Running local IPC owner/client scenarios.
8. Asserting error status.
9. Asserting redaction.
10. Asserting no direct storage access above persistence.

The testkit must not become a second public API. Feature-gated cross-crate test
helpers should be hidden and clearly marked test-only.

### Fake And Faulting Persistence

Engine-next needs two persistence test doubles:

1. A fake L9-compatible persistence implementation for fast semantic tests.
2. A faulting wrapper around real persistence for boundary/failure tests.

The fake persistence should support:

1. Deterministic branch row state.
2. Latest/version/timestamp/history reads.
3. Commit version allocation.
4. Branch create/fork/delete mechanics needed by engine tests.
5. Configurable conflicts.
6. Configurable retained-history bounds.
7. Configurable recovery health facts.
8. Configurable backend capability facts.

The faulting wrapper should inject:

1. Read failure.
2. Write validation failure.
3. Write failure before storage mutation.
4. Ambiguous write outcome.
5. Durable-but-not-visible outcome.
6. Post-commit failure.
7. Recovery degradation.
8. Maintenance failure.

Tests using fake persistence prove engine semantics. Tests using real
storage-next L9 prove adapter integration.

### Deterministic Clock And Version Source

Branch-from-time, `as_of`, history, search temporal compatibility, and
diagnostics require deterministic time.

The testkit should provide:

1. Fixed clock.
2. Step clock.
3. Commit-version allocator.
4. Timeline fixture builder.
5. Retention-window fixture builder.
6. Timestamp collision cases.
7. Timestamp gap cases.

No semantic time-travel test should depend on wall-clock timing.

### Data Capability Conformance Harness

Every data capability should plug into the same conformance harness.

The harness should exercise:

1. Create or initialize capability state.
2. Put/insert/append/upsert.
3. Latest read.
4. Version read.
5. Timestamp read.
6. History.
7. Delete/tombstone.
8. List/scan where applicable.
9. Branch fork.
10. Branch diff.
11. Branch merge/promote.
12. Branch copy/cherry-pick.
13. Restore/revert.
14. Search/text projection where supported.
15. Relationship participation where supported.
16. Read-only rejection.
17. Error status mapping.

Capability-specific suites add details. They must not replace the shared suite.

### Branch And Temporal Model Harness

Branch workflows need a simple independent model.

The model should represent:

1. Branch DAG.
2. Branch lifecycle and generation.
3. Per-capability records as opaque values.
4. Commit versions.
5. Commit timestamps.
6. Tombstones.
7. Retention lower bounds.
8. Conflict strategies.
9. Derived-state invalidation facts.

The model does not need storage-level COW or LSM behavior. It models product
truth.

### Command And IPC Golden Harness

The command-boundary harness should:

1. Encode command request fixtures.
2. Decode command request fixtures.
3. Execute fixtures locally.
4. Execute the same fixtures through IPC where applicable.
5. Compare output shape.
6. Compare error status.
7. Verify access-mode behavior.
8. Verify protocol/version mismatch behavior.
9. Verify frame-limit behavior.
10. Verify redaction.

This harness is what future MCP, LangGraph, ORM, notebook, Codex/Claude Code,
and agent integrations depend on.

### Derived-State Harness

Search, vector indexes, graph projections, recipes, and autoembedding need a
dedicated derived-state harness.

It should simulate:

1. Up-to-date derived state.
2. Missing derived state.
3. Stale derived state.
4. Rebuild in progress.
5. Rebuild failed.
6. Source rows newer than watermark.
7. Derived rows newer than source rows, which should be rejected.
8. Clone artifact with omitted derived state.
9. Branch copy/restore invalidating derived state.
10. Read-only health inspection.

The harness should prove stale derived state never masquerades as correct
source data.

### Fake Model Provider

Optional model-dependent pathways need a fake provider, not real network calls.

The fake provider should support:

1. Deterministic embeddings.
2. Deterministic generated text.
3. Tokenization and detokenization.
4. Provider unavailable.
5. Model missing.
6. Disabled network policy.
7. Malformed provider response.
8. Timeout.
9. Redacted secret configuration.

Engine-next itself should not own provider execution. This harness is for
command-boundary and integration tests where optional upper-layer behavior is
present.

## Engine Bucket Test Plan

### API Bucket

API tests prove the public engine surface exposes product concepts and hides
implementation details.

Required tests:

1. Open cache, durable, read-only, and IPC-fallback outcomes through public
   options.
2. Public handles expose product operations, not storage handles.
3. SDK APIs and command DTOs agree on branch, space, time, and access-mode
   defaults.
4. Removed public surfaces are absent from default API docs and re-exports.
5. Public errors expose status, not debug-only variants.
6. Public DTOs are serializable where they cross command/IPC boundaries.
7. Public APIs never require users to flush, compact, checkpoint, or manage
   retention for normal use.

Acceptance criteria:

1. API tests do not import storage-next directly.
2. API errors assert class/code/status fields.
3. Public transaction-session surface is not a required V1 pathway.

### Runtime Bucket

Runtime tests prove database lifecycle and resource policy.

Required tests:

1. Local durable open creates/reopens a database.
2. Cache open creates no durable database objects.
3. Read-only open rejects writes before mutation.
4. Same-path reuse follows documented runtime signature rules.
5. Locked primary classifies IPC fallback correctly.
6. Locked primary without socket returns a structured `ipc_not_running` status.
7. Default branch bootstrap is deterministic.
8. Built-in recipe bootstrap is idempotent and failure-warning-only where
   specified.
9. Runtime resource profile resolves budgets deterministically with fake host
   probes.
10. Shutdown quiesces commits and derived work.
11. Close/drop behavior is idempotent.
12. Runtime diagnostics report effective mode, access, budgets, and health.

Fault tests:

1. Open validation failure before side effects.
2. Storage open failure mapped through persistence.
3. Recovery degradation during open.
4. Runtime resource profile probe failure.
5. Shutdown timeout.
6. IPC stale socket.

Acceptance criteria:

1. Follower mode is absent.
2. Product open hints do not mention follower mode.
3. Runtime does not instantiate caller-supplied subsystem lists.

### Persistence Adapter Bucket

Persistence adapter tests prove the engine/storage boundary.

Required tests:

1. Capability code cannot import storage-next directly in production paths.
2. Row addresses resolve symbolic storage-space assignments through the
   registry.
3. Read selectors map to latest, version, timestamp, and history reads.
4. Scan selectors preserve branch, space, storage-space, range, and visibility.
5. Commit plans reject read-only, closed, closing, and invalid branch states
   before storage mutation.
6. Storage errors map to engine statuses with operation and commit phase.
7. Storage recovery health maps into engine diagnostics.
8. Storage capability facts map into open/runtime diagnostics.
9. Ambiguous storage commit outcomes stay ambiguous through engine status.
10. Durable-but-not-visible stays distinct from ordinary IO failure.

Fault tests:

1. Storage read failure.
2. Storage write failure before mutation.
3. Storage conflict.
4. Storage ambiguous commit.
5. Storage corruption.
6. Storage unsupported backend capability.
7. Storage recovery degraded or failed.

Acceptance criteria:

1. Persistence is the only normal engine bucket that consumes storage-next L9.
2. Write-path errors do not use lossy blanket conversion.
3. Storage internals below L9 do not appear in engine tests except adapter
   characterization tests.

### Commit Bucket

Commit tests prove internal write units without preserving public manual
transaction workflows.

Required tests:

1. Single operation commit writes one atomic batch.
2. Multi-row capability operation writes one atomic batch.
3. Commit versions are monotonic.
4. Commit timestamps are assigned once per commit.
5. Read-set/CAS validation fails before mutation.
6. Branch commit guards serialize conflicting writes.
7. Commit observer dispatch is deterministic and bounded.
8. Derived commits are tagged as derived state.
9. Post-commit hook failures report `committed_post_commit_failed`.
10. Aborted internal commits leave no visible source rows.

Fault and property tests:

1. Generated conflict schedules match the model.
2. Generated commit sequences preserve version monotonicity.
3. Persistence ambiguous commit returns `maybe_committed`.
4. Durable-but-not-visible tells the caller to inspect/reopen rather than retry
   blindly.
5. Observer failure cannot hide a successful source commit.

Acceptance criteria:

1. Public begin/commit/rollback sessions are not V1 required conformance.
2. Commit machinery remains internally testable.
3. Commit errors carry commit outcome.

### Data Capability Bucket

Shared capability conformance runs against KV, JSON, event, vector, and graph.

Required shared tests:

1. Entity addressing.
2. Branch and space scoping.
3. Latest read.
4. Version read.
5. Timestamp read.
6. History.
7. Tombstone/delete behavior.
8. Read-only rejection.
9. Branch fork visibility.
10. Branch diff.
11. Branch copy/cherry-pick.
12. Branch restore/revert.
13. Merge or promote behavior.
14. Search/text projection declaration.
15. Relationship participation declaration.
16. Import/export participation, if shipped.
17. Error status mapping.
18. Malformed value handling.

Capability-specific additions:

| Capability | Additional conformance |
| --- | --- |
| KV | Key validation, put/get/delete/list/scan/batch, missing-key behavior, simple merge. |
| JSON | Path validation, set/delete path, document history, structured merge behavior, oversized document errors. |
| Event | Append ordering, immutability expectations, type/time ranges, pagination, branch copy semantics. |
| Vector | Collection config, dimension validation, upsert/query, metadata filters, index health, shadow-vector separation. |
| Graph | Relationship bindings, ontology lifecycle, dangling references, traversal bounds, graph analytics feature gating. |

Acceptance criteria:

1. A new capability cannot ship without passing the shared suite.
2. Capability tests use product semantics and persistence adapter contracts, not
   raw storage internals.
3. Authored rows and derived rows are tested separately.

### EntityRef And Relationship Layer

Relationship tests prove graph can connect Strata records without duplicating
payloads.

Required tests:

1. EntityRef roundtrip for every data capability.
2. EntityRef rejects invalid branch, space, capability, and key shapes.
3. Relationship endpoints can bind to KV, JSON, event, vector, and graph
   entities.
4. Traversal returns EntityRefs that can fetch source records.
5. Dangling references surface structured diagnostics.
6. Deleted endpoints behave correctly under latest, version, and timestamp
   reads.
7. Branch fork preserves relationship visibility.
8. Branch merge/copy/restore treats relationships through graph capability
   adapters.
9. Reverse maps rebuild or report degraded derived state.
10. Relationship errors are not generic graph errors when endpoint context is
    known.

Property tests:

1. Generated relationship graphs preserve reachability against a simple model.
2. Generated deletes/tombstones produce expected dangling/deleted diagnostics.
3. Generated branch forks preserve historical endpoint resolution.

Acceptance criteria:

1. Graph is both native graph data and cross-capability relationship layer.
2. Source data remains owned by its original capability.
3. Traversal never requires storage to understand EntityRef semantics.

### Branch Bucket

Branch tests prove product branch workflows.

Required tests:

1. Create/list/info/exists/delete branch.
2. Branch-from-current.
3. Branch-from-version.
4. Branch-from-time.
5. Diff/compare.
6. Promote/merge.
7. Copy/cherry-pick.
8. Restore/revert.
9. Conflict preview.
10. Strict and source-wins behavior where supported.
11. Branch lifecycle and generation guards.
12. Derived-state cleanup or invalidation after branch changes.
13. Branch operation audit/control-plane records.

Model/property tests:

1. Generated branch DAG operations match the branch model.
2. Generated capability rows diff correctly by capability adapter.
3. Generated merge conflicts are stable and explainable.
4. Generated branch-from-history rejects unresolved or pruned points without
   partial branch creation.

Acceptance criteria:

1. Branch code does not manually decode every capability row.
2. Capability branch adapters own capability-specific interpretation.
3. Branch errors are structured and status-compatible.

### Temporal Context And Timeline

Temporal tests prove `getv`, `as_of`, history, timeline scrub, and
branch-from-history.

Required tests:

1. Latest selector.
2. Version selector.
3. Timestamp selector.
4. History selector.
5. Timestamp before earliest retained commit.
6. Timestamp after latest commit.
7. Timestamp gap resolution.
8. Multiple commits with same timestamp.
9. Tombstone and TTL interaction.
10. Timeline bounds and summaries.
11. Branch-from-version.
12. Branch-from-time.
13. Search/retrieval temporal compatibility.
14. Derived-state temporal refusal when index cannot answer the requested time.

Property tests:

1. Generated commit timelines resolve timestamps to expected versions.
2. Generated retained-history windows reject pruned reads.
3. Generated branch forks preserve parent timeline bounds.

Acceptance criteria:

1. `as_of` means timestamp.
2. `getv` means version.
3. Every temporal error has retained-bound details.

### Control Plane Bucket

Control-plane tests prove `_system_` branch and branch-local `_system_` space
behavior.

Required tests:

1. Global `_system_` branch stores database-level metadata.
2. Branch-local `_system_` space stores branch-local metadata.
3. Built-in recipes bootstrap globally.
4. Branch recipe overrides branch correctly.
5. Storage-space ID registry loads before capability rows are interpreted.
6. Capability registry rejects unknown or conflicting capability assignments.
7. Projection manifests and watermarks branch with user branch state when
   branch-local.
8. Derived-state health rows do not masquerade as source rows.
9. Control-plane records are redacted in public describe output where needed.
10. Clone/import omits or rebuild-marks derived control-plane rows as specified.

Fault tests:

1. Corrupt control-plane row.
2. Unknown storage-space ID.
3. Missing capability registry.
4. Conflicting derived-state manifest.
5. Branch-local metadata missing after branch copy.

Acceptance criteria:

1. Control-plane data has clear global versus branch-local ownership.
2. Control-plane failures are structured, not internal strings.
3. Ordinary users do not need to manage control-plane records directly.

### Orchestration Bucket

Orchestration tests prove cross-capability derived work is explicit and
observable.

Required tests:

1. Commit observer emits bounded facts.
2. Autoembedding policy reads branch-local control-plane state.
3. Shadow vector writes are distinct from user-managed vector collections.
4. Graph relationship coordination uses EntityRef contracts.
5. Search projection records watermarks.
6. Rebuild jobs are idempotent.
7. Rebuild failure records health.
8. Branch operations invalidate or rebuild derived state as specified.
9. Read-only handles can inspect orchestration health but cannot mutate it.
10. Disabled optional model/runtime support fails clearly.

Fault tests:

1. Derived write fails after source commit.
2. Rebuild interrupted.
3. Watermark corrupt.
4. Missing model.
5. Disabled network.
6. Provider unavailable in optional upper-layer tests.

Acceptance criteria:

1. Cross-capability behavior does not hide inside capability CRUD methods.
2. Derived state has health and provenance.
3. Source commits remain authoritative.

### Retrieval Bucket

Retrieval tests prove deterministic search and derived-state behavior.

Required tests:

1. Keyword search over indexed source data.
2. Semantic/vector search where runtime support exists.
3. Hybrid search with deterministic fusion.
4. Graph-aware retrieval expansion or boost where graph data exists.
5. Recipe lookup order.
6. Recipe validation.
7. Query expansion and reranking feature gating.
8. Result limits and pagination.
9. Temporal search compatibility.
10. Provenance includes EntityRefs, branch, space, recipe, and index facts.
11. Stale index refused or clearly marked according to recipe policy.
12. Missing index reports structured diagnostics.

Property and golden tests:

1. Deterministic ordering for equal scores.
2. Fusion/rerank reproducibility for fixed inputs.
3. Search request/response JSON goldens.
4. Recipe JSON goldens.
5. Error status goldens for stale/missing indexes and unsupported stages.

Acceptance criteria:

1. Retrieval consumes capability adapters.
2. Retrieval does not mutate source data implicitly.
3. Optional model-dependent stages are feature-gated and network-explicit.

### IPC And Command Boundary

Command and IPC tests prove transport-independent semantics.

Required tests:

1. Every command declares read/write/access-mode behavior.
2. Local and IPC execution of the same command return equivalent outputs.
3. Local and IPC execution of the same failure return equivalent status when
   failure is not transport-specific.
4. Read-only clients reject every write before mutation.
5. `strata up` owns the local database.
6. Stale socket is diagnosed.
7. Protocol mismatch is diagnosed.
8. Oversized frame is diagnosed.
9. Server shutdown is diagnosed.
10. Disconnect during possible write commit preserves ambiguity.
11. Command request, response, and error goldens exist.
12. No separate user-facing `strata ipc ...` command family is required.

Acceptance criteria:

1. IPC remains local same-machine sharing.
2. IPC does not expose storage internals.
3. `database()`-style local-only APIs do not panic in V1 public paths.

### Data Movement And Clone Artifacts

Clone tests prove `.strata` artifacts are portable datasets, not live database
files.

Required tests:

1. Valid artifact materializes a normal database directory.
2. Artifact manifest validates before destination promotion.
3. Unsupported artifact version fails before side effects.
4. Checksum mismatch fails before promotion.
5. Destination exists fails safely.
6. Partial materialization is cleaned or quarantined.
7. Clone mints local database identity while preserving provenance.
8. Branch/version bounds are preserved.
9. Derived state omitted from artifact is marked for rebuild.
10. Cloned database opens offline.
11. Clone does not require StrataHub specifically.

Fault tests:

1. Source read failure.
2. Manifest decode failure.
3. Payload checksum failure.
4. Destination publish failure.
5. Cleanup failure after partial materialization.

Acceptance criteria:

1. `.strata` is a clone artifact, not the normal live database shape.
2. Clone has no hidden network behavior.
3. Hub credentials and signed URLs are redacted.

### Diagnostics And Errors

Diagnostics tests prove status is stable and useful.

Required tests:

1. Every emitted engine code is registered.
2. Every emitted code maps to exactly one class.
3. Retry policy is present.
4. Commit outcome is present.
5. Source chains exist where useful.
6. Redaction is enforced.
7. CLI JSON errors include status.
8. Command errors include status.
9. IPC transport errors include status.
10. Product open errors include status.
11. Storage mapping includes operation and commit phase where relevant.
12. Derived-state errors do not look like source corruption.
13. Corruption errors do not collapse to generic IO.
14. Tests do not assert prose messages when status fields exist.

Acceptance criteria:

1. Normal product failures are not `internal`.
2. Display text is not the automation contract.
3. Error goldens cover representative domains.

## Cross-Surface Matrix

Every required product behavior should be checked across the surfaces that
claim to support it.

| Surface | Required coverage |
| --- | --- |
| SDK/API | Embedded application use, typed outputs, typed errors, no storage access. |
| Command DTO | Serializable request/response/error, read/write classification, golden fixtures. |
| CLI | Human output, JSON output, exit codes, help text, removed-surface absence. |
| IPC | Same-machine local sharing, same command semantics, transport errors. |
| Integration adapters | Command/API examples for MCP, LangGraph, ORMs, notebooks, plugins, agents. |

If a behavior exists on one surface but not another, the difference must be
documented and tested.

## Runtime Mode Matrix

Engine conformance must cover the modes Strata claims for V1.

| Mode | Required engine tests |
| --- | --- |
| Cache | Non-durable open, normal operations within lifetime, no durable claims, read-only behavior, close/drop loss. |
| Standard durable local | Open/reopen, source commits durable after success, ordinary recovery, branch/time/retrieval conformance. |
| Always durable local | Durability failure mapping, sync/commit outcome mapping, recovery diagnostics. |
| Read-only | All read/inspect operations work, every write rejects before mutation. |
| IPC owner | Owns writable local handle, serves local clients, structured lifecycle diagnostics. |
| IPC client | Same command semantics, access-mode enforcement, transport error mapping. |
| Future backend | Fails unsupported combinations before side effects unless explicitly implemented. |

## Removed-Surface Guards

Guard tests must prevent old development-era features from returning by
accident:

1. Follower mode.
2. Public transaction-session commands.
3. Branch bundles as V1 product surface.
4. Disk-backed cache mode.
5. Public tags and notes.
6. Normal-user flush/compact/checkpoint/retention commands.
7. Raw storage imports above engine.
8. Broad executor re-exports of non-product types.
9. Subsystem-instantiation hooks as public product architecture.
10. Message-only errors without status at public boundaries.

## Suggested Test Layout

Exact paths can be decided during implementation, but the structure should be
contract-oriented:

```text
tests/engine_next/api.rs
tests/engine_next/runtime.rs
tests/engine_next/persistence_adapter.rs
tests/engine_next/commit.rs
tests/engine_next/data_capability_conformance.rs
tests/engine_next/entity_relationships.rs
tests/engine_next/branching.rs
tests/engine_next/temporal.rs
tests/engine_next/control_plane.rs
tests/engine_next/orchestration.rs
tests/engine_next/retrieval.rs
tests/engine_next/command_boundary.rs
tests/engine_next/ipc.rs
tests/engine_next/clone_artifacts.rs
tests/engine_next/errors_and_diagnostics.rs
tests/engine_next/product_pathways.rs
tests/engine_next/removed_surface_guards.rs
```

Shared helpers should live in a single testkit. Avoid one fixture module per
feature unless the feature truly has unique setup.

## Readiness Gates

### Contract Ready

A bucket contract is ready when:

1. Required behavior is listed.
2. Required errors are listed.
3. Required conformance tests are listed.
4. Required fixtures or harnesses are identified.
5. Open questions that affect tests are explicit.

### Implementation Ready

An engine-next implementation phase is ready when:

1. Characterization tests exist for current behavior being ported.
2. Target conformance tests exist or are written with the implementation.
3. Fault/error tests exist for expected failure cases.
4. Removed-surface guards exist if the phase deletes or hides a surface.
5. The phase does not require test-only storage internals above persistence.

### V1 Surface Freeze

Before V1 surface freeze:

1. Required product-pathway tests pass.
2. Required data capability conformance tests pass.
3. Required branch and temporal model tests pass.
4. Required command, CLI JSON, and IPC goldens exist and pass.
5. Required error/status tests pass.
6. Required clone artifact tests pass.
7. Required runtime mode tests pass.
8. Required recovery product tests pass.
9. Removed-surface guards pass.
10. Optional shipped features have enabled and disabled conformance tests.
11. Upper layers do not import storage directly without a documented exception.

## Open Questions

Resolve before engine-next implementation freezes:

1. What is the exact location and feature gating for the engine testkit?
2. Property-test framework.
   Closed for V1: use `proptest` for engine model/property tests. Byte fuzzing
   can use a separate fuzz harness where appropriate.
3. Concurrency-testing approach.
   Closed baseline: use a hand-rolled deterministic scheduler for commit,
   branch, and IPC interleaving tests. Revisit `loom` or `shuttle` only if the
   hand-rolled scheduler cannot express the needed interleavings. The
   hand-rolled scheduler must stay a small test harness, not grow into a hidden
   concurrency framework; if it starts modeling executor/runtime semantics,
   switch to a standard tool.
4. Which CLI output fields are stable in human mode versus JSON mode?
5. Which optional retrieval/model pathways ship in V1 and therefore require
   enabled conformance?
6. How much of the fake model provider lives in engine tests versus
   intelligence/inference tests?
7. Which current tests are characterization-only and should be deleted after
   porting?
8. Which error detail keys are stable public fixtures versus diagnostic-only
   facts?

## Acceptance Criteria

This plan is satisfied when:

1. Engine-next has reusable harnesses for persistence, capabilities, branch
   models, temporal models, command goldens, IPC, clone artifacts, and errors.
2. Each engine architecture bucket has direct tests for its contract.
3. Shared data capability conformance prevents five unrelated implementation
   styles.
4. Branch and time-travel behavior is model-tested.
5. Public API, command, CLI, and IPC surfaces agree.
6. Structured error status is tested through every public boundary.
7. Derived state cannot silently contradict source data.
8. Clone artifacts are validated and materialized safely.
9. Removed surfaces stay removed.
10. Product-pathway conformance can run without direct storage access.
