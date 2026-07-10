# Engine-Next Architecture Document Index

Status: V1 architecture draft

## Purpose

This directory is the working set for engine-next contracts.

The high-level engine architecture lives one level up in
`docs/architecture/engine-architecture.md`. This index lists the follow-up
documents that must exist before implementation starts, so the engine rewrite
does not proceed from informal memory or scattered notes.

These documents are architecture contracts, not implementation plans. They
define ownership, invariants, failure behavior, diagnostics, and conformance
expectations. Implementation plans can be written after these contracts exist.

## Reading Order

1. `docs/architecture/strata-v1-architecture.md`
2. `docs/architecture/core-architecture.md`
3. `docs/architecture/storage-architecture.md`
4. `docs/architecture/engine-architecture.md`
5. `docs/architecture/engine/README.md`
6. The contracts in the sequence below.

## Contract Sequence

### 1. Data Capability Implementation Contract

Path: `docs/architecture/engine/primitive-implementation-contract.md`

Status: Written.

Purpose: Defines the repeatable pattern for KV, JSON, event, vector, and graph
as engine data capabilities over the branch-aware MVCC KV row substrate.

### 2. EntityRef And Relationship-Layer Contract

Path: `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`

Status: Written.

Purpose: Defines product identity across data capabilities, graph relationship
bindings, dangling/deleted/temporal reference behavior, reverse maps, and
retrieval provenance.

### 3. Storage-Space ID Registry

Path: `docs/architecture/engine/storage-space-id-registry.md`

Status: Written.

Purpose: Defines engine-owned storage-space ID assignments for authored rows,
metadata rows, derived rows, control-plane rows, search rows, shadow vector
rows, and graph relationship rows.

### 4. Engine Persistence Adapter Contract

Path: `docs/architecture/engine/persistence-adapter-contract.md`

Status: Written.

Purpose: Defines the only normal engine-facing path to storage-next L9:
physical key construction, read forms, commit batches, branch mechanics,
timeline resolution, snapshot/recovery facts, and error mapping.

### 5. Branch Operation And Capability Adapter Contract

Path: `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`

Status: Written.

Purpose: Defines branch create, branch-from-version, branch-from-time, compare,
promote, copy, restore, delete, conflict strategies, derived-state cleanup, and
per-capability behavior.

### 6. Temporal Context And Timeline Resolver Contract

Path: `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`

Status: Written.

Purpose: Defines `version`, `as_of`, history, timestamp resolution,
retained-history errors, tombstone/TTL behavior, and temporal search
limitations.

### 7. Control-Plane Layout Contract

Path: `docs/architecture/engine/control-plane-layout-contract.md`

Status: Written.

Purpose: Defines `_system_` branch and branch-local `_system_` space records
for recipes, capability registry, storage-space registry, projection manifests,
watermarks, derived-state status, provenance, and capability facts.

### 8. Retrieval And Derived-State Contract

Path: `docs/architecture/engine/retrieval-and-derived-state-contract.md`

Status: Written.

Purpose: Defines source coverage, recipe schema, BM25/vector/graph stages,
temporal compatibility, stale indexes, autoembedding watermarks,
rebuild/repair, result stats, and provenance.

### 9. IPC And Serializable Command-Boundary Contract

Path: `docs/architecture/engine/ipc-and-command-boundary-contract.md`

Status: Written.

Purpose: Defines command DTOs, access mode, read-only write classification,
local vs IPC handle reporting, structured errors, and transport-independent
semantics.

### 10. Dataset Clone Artifact Contract

Path: `docs/architecture/engine/dataset-clone-artifact-contract.md`

Status: Written.

Purpose: Defines `.strata` artifact shape, validation, checksums, provenance,
branch/version metadata, derived-state rebuild markers, and partial-write
cleanup.

### 11. Public API And CLI Surface Cleanup Checklist

Path: `docs/architecture/engine/public-api-and-cli-surface-cleanup-checklist.md`

Status: Written.

Purpose: Defines the V1 product surface and cleanup targets for follower mode,
public transaction sessions, legacy branch bundles, disk-backed cache,
tags/notes, manual maintenance commands, and deprecated re-exports.

### 12. Product-Pathway Conformance Plan

Path: `docs/architecture/engine/product-pathway-conformance-plan.md`

Status: Written.

Purpose: Maps the V1 product pathways to engine conformance tests,
storage/engine fault-injection layers, diagnostics, and end-to-end acceptance
criteria.

## Supplemental Documents

### Engine-Next Error And Diagnostics Contract

Path: `docs/architecture/engine/error-and-diagnostics-contract.md`

Status: Written.

Purpose: Applies the V1 error and diagnostics vocabulary to engine-next:
product error ownership, storage mapping, capability diagnostics, command/IPC
status preservation, redaction, cutover requirements, and conformance tests.

### Engine-Next Testing And Conformance Plan

Path: `docs/architecture/engine/testing-and-conformance-plan.md`

Status: Written.

Purpose: Defines the engine-side testing strategy over storage-next: reusable
testkits, fake/faulting persistence, shared data-capability conformance,
branch/time model tests, command/IPC goldens, clone artifact tests,
error/status tests, removed-surface guards, and V1 readiness gates.

### Engine-Next Target Crate Shape And Test Harness

Path: `docs/architecture/engine/target-crate-shape-and-test-harness.md`

Status: Written.

Purpose: Defines the target one-crate engine-next module shape, crate-level
policy, domain ownership, test-support/testkit split, and forbidden shapes so
the implementation does not recreate cleanup-era vocabulary or scattered
storage access.

## Rule

If an implementation question cannot be answered by the high-level architecture
document plus the relevant contract here, write or amend the contract before
writing code.

## Vocabulary Discipline

Engine-next should minimize named concepts.

A contract should introduce a new named type, enum, trait, service, or adapter
only when the concept is durable enough to appear in code, tests, diagnostics,
or public docs. If plain language is enough, use plain language. Prefer a small
set of repeatable patterns over one-off names that make contributors learn a
private milestone vocabulary before they can read the crate.

Architecture documents may describe behavior without committing to future Rust
names. Implementation plans should justify every new named concept by showing
where it repeats, what invariant it protects, and why an existing pattern is not
enough.
