# Engine-Next Product-Pathway Conformance Plan

Status: V1 architecture draft

## Purpose

This document maps Strata V1 product pathways to conformance tests.

The product documents say what users must be able to do. The engine-next
contracts say which layer owns each behavior. This document defines the
acceptance bar: which pathway tests must exist before V1 can claim the product
surface is ready.

The plan is intentionally straightforward:

1. Every required pathway gets at least one product-level conformance test.
2. Every optional pathway that ships gets feature-gated conformance tests.
3. Every removed non-pathway gets absence/guard tests.
4. Every test uses public API, CLI, IPC, or command DTOs unless it is explicitly
   a lower-layer contract test.
5. No product-path test reaches into storage directly.

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-user-pathways.md`
3. `docs/product/strata-v1-architecture-support-matrix.md`
4. `docs/product/strata-v1-non-functional-requirements.md`
5. `docs/architecture/v1-testing-and-conformance-plan.md`
6. `docs/architecture/v1-error-and-diagnostics-contract.md`
7. `docs/architecture/engine-next-architecture.md`
8. All contracts in `docs/architecture/engine-next/`

## Scope

In scope:

1. End-to-end product pathways over the engine public surface.
2. CLI, SDK, IPC, and serializable command-boundary behavior.
3. Product diagnostics and structured errors.
4. Required feature conformance.
5. Optional feature conformance when a feature is compiled or documented as
   available.
6. Removed-surface guard tests.

Out of scope:

1. Storage L1-L9 internal conformance. That belongs to
   `docs/architecture/v1-testing-and-conformance-plan.md` and layer-specific
   storage docs.
2. Exact benchmark thresholds.
3. Hosted StrataHub tests.
4. Strata AI assistant UX tests.
5. Production OpenDAL/S3 conformance for V1 unless a backend is explicitly
   shipped as production-ready.

## Conformance Rule

A pathway is V1-conformant only when all applicable layers agree:

1. SDK/API behavior passes.
2. Serializable command DTO behavior passes.
3. CLI behavior passes where the pathway has a CLI surface.
4. IPC behavior passes where same-machine sharing is relevant.
5. Structured errors match the error/diagnostics contract.
6. Read-only and access-mode behavior is correct.
7. JSON automation output is stable where applicable.
8. No test requires direct storage access or raw engine handles.

## Test Families

Product-pathway tests should reuse a small set of test families.

### SDK Path Tests

Use the public SDK surface as an application developer would.

These tests prove embedded use works without CLI-only behavior.

### Command Boundary Tests

Use serializable command DTOs directly.

These tests prove IPC, plugins, agent tools, and language bindings can use the
same product instruction set without relying on Rust-only helpers.

### CLI Tests

Run CLI commands in human and JSON modes.

These tests prove terminal, script, CI, and plugin subprocess use works with
stable exit codes and output shapes.

### IPC Tests

Start a local owner with `strata up`, connect a second handle, and verify
same-machine sharing behavior.

These tests prove IPC is the V1 replacement for follower mode.

### Fault And Failure Tests

Inject or construct failure states at the lowest practical layer, then assert
product errors and recovery outcomes through public surfaces.

These tests prove users and integrations see stable diagnostics.

### Golden Output Tests

Freeze JSON request/response/error examples for stable command-boundary and CLI
automation outputs.

These tests protect MCP servers, LangGraph integrations, ORMs, notebooks,
Codex/Claude Code plugins, and CI tools from silent schema drift.

### Removal Guard Tests

Assert removed surfaces are absent from CLI help, public command schema,
default SDK docs, and public re-exports.

These tests prevent old development-era concepts from leaking back in.

## Shared Fixtures

The conformance suite should use reusable fixtures, not bespoke setup per
pathway.

Minimum fixture families:

1. Empty durable database.
2. Empty cache database.
3. Read-only durable database.
4. Small mixed database with KV, JSON, events, vectors, graph relationships,
   spaces, recipes, and search-visible text.
5. Branching database with divergent branches, conflicts, copied records, and
   retained history.
6. Timeline database with known commit versions and timestamps.
7. Clone artifact fixture.
8. Corrupt or unsupported artifact fixtures.
9. Recovery fixtures for ordinary crashes and manifest/WAL/checkpoint faults.
10. IPC fixture with one local owner and one client.
11. Optional model fixture with fake local inference provider.
12. Optional backend-capability fixture with unsupported-feature responses.

## Pathway Matrix

### Runtime And Portability

| Pathway | Decision | Required conformance |
| --- | --- | --- |
| 1. Create or open a local embedded database | Required | SDK and CLI open create a durable database, reopen it, preserve committed writes, and report stable open errors for invalid path, lock conflict, permission failure, and corruption. |
| 2. Open an ephemeral cache database | Required | SDK and CLI cache opens create no durable objects, support normal data operations within process lifetime, report non-durable diagnostics, and lose data after close/drop. |
| 3. Open a database read-only | Required | Local, CLI, command-boundary, and IPC read-only handles allow reads/inspection and reject every write command before mutation. |
| 4. Share a local database through IPC | Required | `strata up` owns a database, a second local handle connects through IPC, reads/writes obey access mode, errors are structured, and no follower mode is involved. |
| 5. Clone a portable dataset | Required | `strata clone <source> <destination>` and SDK clone validate a `.strata` artifact, materialize a normal database, mint local identity, preserve provenance, and reject partial/corrupt artifacts before destination promotion. |
| 6. Use a cloned dataset offline | Required | A cloned database opens without source access, supports branch/search/mutation/export within included features, and marks omitted derived state for rebuild instead of treating it as corruption. |
| 39. Choose a storage backend intentionally | Required substrate | Unsupported backend or mode combinations fail before side effects with capability diagnostics; local filesystem and cache modes report supported capabilities honestly. |

### Data Capabilities

| Pathway | Decision | Required conformance |
| --- | --- | --- |
| 7. Write and read key-value data | Required | Put/get/delete/list/scan/batch operations work across branch and space context, expose versions/timestamps, and preserve read-only/write classification. |
| 8. Write and read JSON documents | Required | JSON set/get/delete/list/history operations preserve path semantics, branch/space context, version output, and typed errors for invalid paths or oversized documents. |
| 9. Append and query events | Required | Event append/range/type/time queries preserve ordering, immutability expectations, branch/space context, and bounded pagination behavior. |
| 10. Create and manage graphs | Required | Graph create/delete/list/meta operations work through public surfaces and remain branch/space scoped. |
| 11. Model graph entities and relationships | Required | Graph nodes/edges can bind to EntityRefs across KV/JSON/event/vector/graph records without duplicating payloads; dangling/deleted references surface as defined diagnostics. |
| 12. Define graph ontology | Required | Object/link type define/get/list/delete/freeze/status operations obey lifecycle rules and report validation errors without corrupting graph data. |
| 13. Traverse and query graph neighborhoods | Required | Neighbor and bounded BFS queries respect direction, edge type, branch, space, time context, result bounds, and missing-node errors. |
| 14. Run graph analytics | Optional | If shipped, connected-components, community, PageRank, clustering, and shortest-path commands are bounded, deterministic for fixed inputs, feature-gated, and honest about unsupported sizes. |
| 15. Store and query vectors | Required | Collection create/delete/list/stats, upsert/get/delete/query/batch operations validate dimensions/metrics, preserve metadata, and report missing or stale index state. |
| 31. Organize data with spaces | Required | Space create/list/exists/delete works across capabilities; reserved spaces are protected; deleting a non-empty space requires explicit safe behavior. |

### Retrieval And Intelligence

| Pathway | Decision | Required conformance |
| --- | --- | --- |
| 16. Run keyword search | Required | Keyword search returns deterministic ranked results over indexed source data, reports stats/provenance, and handles stale or missing indexes according to diagnostics. |
| 17. Run semantic or hybrid search | Required where runtime support exists | Semantic/hybrid search uses stored or supplied embeddings, fuses results according to recipe, and reports unsupported model/vector capability clearly. |
| 18. Run graph-aware retrieval | Required where graph data exists | Retrieval can expand, boost, or explain results using relationship context while preserving EntityRef provenance and result bounds. |
| 19. Use search recipes | Required | Built-in recipes seed correctly, branch-local recipe overrides work, invalid recipes fail validation, and recipe lookup uses the documented fallback order. |
| 20. Use query expansion and reranking | Optional | If shipped, expansion/rerank stages are explicit, model-dependent, bounded, and report missing runtime/provider support without hidden network behavior. |
| 21. Ask retrieval-backed questions | Optional | If shipped, generated answers carry retrieval provenance, model/runtime errors are structured, and answer generation never rewrites source data implicitly. |
| 22. Configure auto-embedding and indexing | Optional | If shipped, enabling/disabling auto-embedding is explicit, status and watermarks are visible, reindex is safe, and stale shadow vectors are not silently trusted. |
| 23. Manage models and inference configuration | Optional | If shipped, list/pull/configure operations are feature-gated, network-explicit, secret-redacted, and have stable missing-model/provider errors. |
| 24. Generate, tokenize, and detokenize text | Optional | If shipped, generation/tokenizer commands are deterministic where configured, bounded by limits, feature-gated, and independent of storage correctness. |

### Branching, Versioning, And Time Travel

| Pathway | Decision | Required conformance |
| --- | --- | --- |
| 25. Create and manage branch workspaces | Required | Empty branch create, branch-from-current, list/info/exists/delete work with safety checks, race handling, reserved-name rejection, required control-plane rows, and derived-state cleanup. |
| 26. Inspect record history | Required | KV/JSON/vector/graph relationship history returns versions, timestamps, values/tombstones, and retained-history errors consistently. |
| 27. Read data as of a point in time | Required | `as_of` and version reads resolve to retained state for KV/JSON/events/vector/graph/search where supported and fail clearly when history is unavailable. |
| 28. Scrub and explain a branch timeline | Required | Timeline range and point resolution expose available bounds, map timestamps to concrete versions, and explain retained-history gaps. |
| 29. Create a branch from historical state | Required | Branch-from-version and branch-from-time create a new branch from retained state and reject unresolved or pruned points without partial branch creation. |
| 30. Compare, promote, copy, and restore branch changes | Required | Diff/compare, promote/merge, copy/cherry-pick, and restore/revert preserve per-capability semantics, preview conflicts, and report strict/source-wins outcomes. |

### Operations And Interfaces

| Pathway | Decision | Required conformance |
| --- | --- | --- |
| 32. Import and export primitive data | Optional | If shipped, import/export is explicit about branch, space, capability, format, schema, partial-write behavior, and unsupported feature errors. |
| 33. Inspect database state | Required | Info/describe/health/metrics/durability counters are bounded, privacy-aware, stable in JSON mode, and usable in read-only/cache/IPC contexts. |
| 34. Recover from ordinary failures | Required | Reopen after ordinary crash recovers committed data, reports degraded or failed recovery honestly, and maps corruption/unsupported backend/config errors to stable diagnostics. |
| 35. Configure Strata safely | Required | Config load/set/get validates known keys, rejects invalid values before side effects, redacts secrets, and reports hidden-network policy violations. |
| 36. Run Strata from the CLI | Required | CLI help, exit codes, human output, JSON output, global flags, and command parsing match the V1 surface and do not expose removed workflows. |
| 37. Use Strata from application code | Required | SDK APIs expose required operations without CLI-only behavior, raw storage access, or IPC-incompatible panics. |
| 38. Use Strata in agent or sandbox workflows | Required | Public command/API surface supports open, clone, describe, search, branch, mutate, and error handling with explicit filesystem/network/model effects. |

## Explicit Non-Pathway Guards

The following are not V1 pathways. They need absence tests.

| Non-pathway | Guard |
| --- | --- |
| Follower mode | CLI help has no `--follower`; SDK open options expose no follower flag; lock-conflict messages do not recommend follower mode. |
| Public begin/commit/rollback | CLI help has no `begin`, `commit`, `rollback`, or `txn`; public command schema excludes transaction-session commands. |
| Legacy branch bundle workflow | CLI help has no branch-bundle commands; public command schema excludes `BranchExport`, `BranchImport`, and `BranchBundleValidate`; `.branchbundle.tar.zst` is not a V1 compatibility claim. |
| Disk-backed cache mode | Docs/help/config reject disk-backed cache as a mode; cache conformance proves no durable WAL/manifest/checkpoint objects are created. |
| Hidden network behavior | Clone/model/provider/sync-like commands require explicit user action and fail under disabled-network policy. |
| Public tags and notes | Public command schema excludes tag/note commands and branch docs do not rely on tag/note rows. |
| Manual database maintenance | Normal CLI help and SDK docs do not expose flush, compact, checkpoint, or retention-apply workflows as ordinary user actions. |

## Integration Boundary Conformance

The public surface is the substrate for MCP servers, LangGraph integrations,
ORMs, notebooks, Codex/Claude Code plugins, agent sandboxes, CI, and private
tools.

Required conformance:

1. Every stable command has golden JSON request, response, and error examples.
2. Every stable command declares read/write/access-mode behavior.
3. CLI JSON output matches command-boundary DTOs where they overlap.
4. IPC returns the same logical output and error classes as local execution.
5. SDK behavior matches command-boundary behavior for shared operations.
6. Example integration tests can open, describe, search, branch, clone, mutate,
   and handle errors without direct engine or storage access.
7. Guards reject production code above engine that imports storage directly
   without a documented exception.

## Fault And Recovery Coverage

Product-pathway conformance must include failure coverage, not just happy paths.

Minimum fault matrix:

1. Open: missing path parent, permission failure, lock conflict, invalid config,
   corruption, unsupported backend capability.
2. Cache: attempted durable behavior, process close/drop, read-only write.
3. IPC: stale socket, stale PID, owner unavailable, client disconnect, read-only
   write, malformed request.
4. Clone: missing source, unsupported scheme, invalid manifest, checksum
   mismatch, unsupported feature, destination exists, partial publish failure.
5. Data writes: invalid key/space/path, oversized value, type mismatch,
   unsupported capability, commit failure, ambiguous commit where applicable.
6. Branching: missing branch, existing destination, conflict, pruned history,
   derived-state cleanup failure.
7. Time travel: timestamp before/after retained range, pruned version, tombstone,
   TTL expiry, unsupported temporal query.
8. Retrieval: stale index, missing model, model failure, disabled network,
   unsupported recipe stage, result limit exceeded.
9. Config: invalid value, unknown key, secret redaction, provider disabled,
   runtime-only setting mutation.
10. Recovery: WAL fault, manifest fault, checkpoint fault, snapshot fault,
    degraded recovery, failed recovery.

Failure tests should assert error code/class, retry/action guidance, redaction,
and commit outcome. They should not parse prose messages except for human CLI
output tests.

## Readiness Gates

### Required Pathways

A required pathway is green only when:

1. SDK conformance passes.
2. Command-boundary conformance passes.
3. CLI conformance passes if the pathway has CLI surface.
4. IPC conformance passes if the pathway can run through IPC.
5. Failure tests cover expected failure cases.
6. JSON/golden fixtures exist where automation uses the pathway.
7. Docs describe behavior and errors without referencing historical milestones.

### Optional Pathways

An optional pathway may ship only when:

1. It is feature-gated or explicitly documented as available.
2. Missing-feature behavior is stable.
3. Success and failure tests pass under the enabled feature.
4. Disabled-feature tests prove the command/API fails clearly.
5. It does not distort required architecture or product guarantees.

If those conditions are not met, the pathway remains absent from V1 default
surface.

### Removed Surfaces

A removed surface is green only when:

1. It is absent from default CLI help.
2. It is absent from public command schema.
3. It is absent from default SDK docs.
4. Product docs do not recommend it.
5. Guard tests fail if it is reintroduced casually.

## Suggested Test Layout

The exact Rust module names can be decided during implementation, but the suite
should remain organized by product pathway, not by historical implementation
module.

Suggested grouping:

```text
tests/product_pathways/runtime_and_portability.rs
tests/product_pathways/data_capabilities.rs
tests/product_pathways/retrieval_and_intelligence.rs
tests/product_pathways/branching_versioning_time.rs
tests/product_pathways/operations_and_interfaces.rs
tests/product_pathways/non_pathway_guards.rs
tests/product_pathways/command_boundary_goldens.rs
```

Shared helpers should live in one common test module or testkit. Do not create
one-off fixtures for every pathway unless the pathway truly needs unique state.

## V1 Minimum

Before V1 surface freeze:

1. All required pathway tests pass for local filesystem durable mode.
2. Required cache-mode pathway tests pass.
3. Required read-only and IPC pathway tests pass.
4. Required CLI JSON and SDK conformance tests pass.
5. Required clone artifact tests pass for local file artifacts.
6. Required branch/version/time-travel tests pass over retained history.
7. Required search and graph relationship tests pass without direct storage
   access.
8. Required recovery tests pass for ordinary crashes.
9. Optional shipped pathways have enabled and disabled conformance tests.
10. Removed-surface guard tests pass.
11. Integration-boundary golden fixtures exist for command DTOs and errors.
12. No production path above engine accesses storage directly without a
    documented exception.
