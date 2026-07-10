# Strata V1 Non-Functional Requirements

Status: Draft NFR anchor

This document records the first-pass non-functional requirements for Strata V1.
It is intentionally brief. The architecture phase should expand these into
measurable conformance criteria, benchmarks, test suites, and backend capability
contracts.

The purpose here is to set the bar: Strata is not just a collection of data
features. It must be reliable, explainable, portable, testable, and safe enough
that users can trust it as embedded infrastructure.

## Requirement Language

1. Must means V1 is incomplete without it.
2. Should means expected for V1 unless architecture work finds a clear reason to
   defer.
3. May means allowed but not required for V1.

## Reliability And Durability

Strata must preserve committed data according to the selected runtime mode.

1. Durable databases must recover after ordinary process crashes.
2. Cache databases must be explicitly non-durable.
3. Standard durability must define its bounded sync/loss window.
4. Always durability must force the required durability barrier before
   acknowledging a commit.
5. Recovery must be deterministic and must not silently lose committed data
   outside the selected mode's documented guarantees.
6. Open must distinguish corruption, lock conflict, unsupported backend,
   invalid configuration, and IO failure.
7. Users should not need to run manual flush, compact, checkpoint, or retention
   commands during normal use.

## Correctness

Strata must make its core data guarantees explicit and testable.

1. Writes must commit atomically within the supported commit unit.
2. Reads must observe the correct branch, space, and temporal context.
3. Branch operations must preserve source branch state and record lineage.
4. Time-travel operations must fail clearly when requested history is not
   retained.
5. Derived state such as search indexes, graph indexes, and auto embeddings must
   not silently contradict authored data.

## Portability

Strata must be designed for more than a standard server filesystem.

1. Local filesystem remains the reference durable backend.
2. Backend support must be capability-driven, not assumed.
3. Object storage, browser/WASM cache targets, and OpenDAL-backed targets must
   prove the capabilities required by the selected mode.
4. Unsupported backend capabilities must fail at open or operation time with
   explicit errors.
5. Cloneable datasets must open as normal Strata databases after clone.

## Performance And Resource Use

V1 should be efficient enough for realistic embedded use, while avoiding
premature benchmark promises before architecture work is complete.

1. Common reads and writes should avoid unnecessary cross-crate or cross-layer
   work.
2. Large scans, diffs, searches, imports, exports, and graph operations should
   have bounded or paginated product behavior.
3. Background maintenance should avoid surprising user-visible stalls where
   possible.
4. Memory budgets should be explicit and respected.
5. The same binary should adapt from constrained edge devices to server-class
   machines through runtime resource profiling.
6. Auto-derived resource defaults must not clobber explicit user configuration.
7. Low-memory behavior should return typed resource errors, bounded pagination,
   or graceful derived-state degradation before uncontrolled out-of-memory
   behavior.
8. Benchmarks should cover local filesystem reference behavior before broader
   backend claims.

## Security And Privacy

Strata must avoid accidental data exposure.

1. Secrets in config, credentials, provider settings, and diagnostics must be
   redacted.
2. Network access must be explicit; Strata should not upload, sync, register,
   pull models, or call providers without user action.
3. Read-only mode must reject writes before mutation.
4. IPC access is required for same-machine application plus Strata AI workflows
   and must preserve local authorization and access-mode semantics.
5. Dataset clone, import, and export should make provenance and source explicit.

## Observability And Diagnostics

Users must be able to understand what Strata is doing without reading internals.
The V1 error taxonomy, stable code rules, retry policy, commit outcome model,
and diagnostic testing obligations are defined in
`docs/architecture/v1-error-and-diagnostics-contract.md`.

1. Open, recovery, health, metrics, search, indexing, and model operations
   should return structured status.
2. Errors should be specific, actionable, and stable enough for automation.
3. Describe and health output should be bounded.
4. Search and RAG output should expose the stages, models, indexes, and record
   versions used where available.
5. Backend capability errors should name the missing capability.

## Testability

Strata's architecture must make the implementation testable at multiple levels.
The top-level storage and product testing roadmap is defined in
`docs/architecture/v1-testing-and-conformance-plan.md`.

1. Core contracts should have focused unit tests.
2. Storage should support deterministic fault injection, crash recovery tests,
   corruption tests, retention tests, and backend conformance tests.
3. Engine should support cross-capability integration tests for branch, time,
   search, graph, vector, and event behavior.
4. CLI and SDK behavior should be tested as product surfaces, not just wrappers.
5. Fuzz, property, crash, and long-running tests should become part of the
   reference-grade roadmap.

## Compatibility And Evolution

Strata is pre-V1, so V1 architecture should not preserve poor historical
choices for compatibility alone. Once V1 ships, compatibility expectations
become stricter.

1. Pre-V1 cleanup may remove or redesign follower mode, public transaction
   commands, disk-backed cache, branch bundles, tags/notes, and manual
   maintenance commands.
2. V1 public data formats and command/API behavior should be documented before
   stabilization.
3. Pre-V1 development formats may be rejected by default. After V1 stabilizes,
   format changes must have explicit migration, rejection, or clone/export
   behavior.
4. Architecture should preserve room for storage-next, engine-next, core-next,
   and future StrataHub workflows.

## User Experience

Strata should make advanced capabilities feel natural.

1. Product APIs should use user concepts: database, branch, space, record,
   version, time, search, relationship, clone, and restore.
2. Users should not need to understand WAL, manifests, checkpoints, compaction,
   memtables, segments, or subsystem wiring for normal workflows.
3. CLI and SDK behavior should align.
4. Feature availability should be explicit when model runtimes, portable
   backends, or optional indexes are missing.
5. The default experience should be safe, local-first, and durable unless the
   user explicitly chooses cache mode.

## Architecture Phase Follow-Up

The architecture phase should turn this document into measurable requirements:

1. Storage backend capability matrix.
2. Durability and recovery conformance suite.
3. Time-travel and branch correctness suite.
4. Search/vector/graph derived-state correctness rules.
5. Performance benchmark plan.
6. Security and secret-handling checklist.
7. CLI and SDK product-surface compatibility checklist.
