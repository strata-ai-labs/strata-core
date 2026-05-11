# M4 / M4T Implementation Plan: Storage-Next Table, Branch, Commit, Recovery, And L9 API

Status: draft implementation plan

## Goal

Finish the storage substrate that engine-next consumes through the L9 boundary.

## Inputs

1. `docs/architecture/storage-next/l5-table-runtime.md`
2. `docs/architecture/storage-next/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage-next/l7-commit-runtime.md`
4. `docs/architecture/storage-next/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/storage-next/l9-storage-api-boundary.md`
6. `docs/architecture/storage-next/commit-timeline-substrate.md`
7. `docs/architecture/storage-next-architecture.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M4A` | Table runtime | Implement mutable tables, immutable tables, cursors, table manifests, compaction inputs, cache hooks, TTL metadata, and tombstones. | Table reads and compaction inputs are model-tested. |
| `M4B` | Branch visibility | Implement branch-aware row resolution for latest, versioned, history, and timestamp substrate reads. | Visibility model handles branch inheritance, tombstones, and retention boundaries. |
| `M4C` | Commit pipeline | Implement version allocation, timestamp stamping, durable commit ordering, and commit timeline rows. | Ambiguous commit outcomes are classified and recoverable. |
| `M4D` | Open and recovery | Implement open, replay, repair classification, checkpoint loading, retention, maintenance, and recovery health facts. | Crash recovery converges to committed visible state or a structured failure. |
| `M4E` | L9 API | Implement the storage API boundary engine-next is allowed to consume. M4 ships with explicit storage defaults; M5 wires engine-resolved runtime budgets through this boundary. | Engine-facing storage calls require no knowledge of lower services. |
| `M4F` | Durability policies | Implement cache, standard, and always durability policy behavior over the commit pipeline. | Each mode has separate conformance and recovery coverage. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M4TA` | Table model tests | Compare table reads, merges, cursors, TTL, and tombstones against a reference model. | Table behavior is deterministic across operation orderings. |
| `M4TB` | Branch model tests | Exercise branch inheritance, copy-on-write visibility, history, and as-of substrate behavior. | Branch reads match the branch visibility contract. |
| `M4TC` | Commit pipeline tests | Cover version allocation, timestamp stamping, commit timeline, ordering, and ambiguous commit classification. | Commit ordering and timeline behavior are deterministic. |
| `M4TD` | Crash recovery tests | Kill/reopen around WAL append, table publish, manifest publish, checkpoint, and truncation windows. | Recovery never invents committed data or loses acknowledged durable data. |
| `M4TE` | L9 conformance tests | Test the public storage boundary using memory and local filesystem backends. | Engine-next can rely on L9 without reaching lower modules. |
| `M4TF` | Durability mode tests | Cover cache, standard, and always mode behavior separately. | Each mode has distinct conformance tests. |

## Convergence Notes

1. `M4TA` lands with `M4A`.
2. `M4TB` lands with `M4B`.
3. `M4TC` lands with `M4C`.
4. `M4TF` lands with `M4F`.
5. `M4TD` closes after `M4C`, `M4D`, and `M4F` are wired together.
6. `M4TE` closes the milestone after lower storage behavior is complete.

## Slice Policy

Slices may be vertical only when storage semantics require table, branch, and
commit cooperation. Otherwise keep slices aligned to one domain module and one
test harness.

## Non-Goals

1. No product capability semantics.
2. No `EntityRef`.
3. No graph, vector, JSON, event, or search behavior.
4. No engine error mapping except storage-owned diagnostics.

## Milestone Exit Gate

M4 is complete when storage-next opens, commits, recovers, maintains, and serves
branch-aware row reads exclusively through L9. The roadmap Test Gate Summary
remains the canonical milestone gate; this plan explains how M4 reaches it.
