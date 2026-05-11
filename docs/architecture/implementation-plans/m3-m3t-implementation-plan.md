# M3 / M3T Implementation Plan: Storage-Next Backend, Layout, Format, And Durable Services

Status: draft implementation plan

## Goal

Implement the lower storage mechanics before table, branch, and commit behavior
depend on them.

## Inputs

1. `docs/architecture/storage-next-architecture.md`
2. `docs/architecture/storage-next/l1-backend-io.md`
3. `docs/architecture/storage-next/l2-object-layout.md`
4. `docs/architecture/storage-next/l3-durable-format-codec.md`
5. `docs/architecture/storage-next/l4-log-manifest-snapshot-services.md`
6. `docs/spec/strata-storage-format-v1.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M3A` | Backend operations | Implement local filesystem and memory backend operations required by V1 modes. | Capability validation rejects unsupported mode/backend combinations. |
| `M3B` | Object layout | Implement object names, prefixes, families, temp paths, lock paths, and quarantine paths. | Layout has no ad hoc string construction outside the layout module. |
| `M3C` | Format codec | Implement durable encoders and decoders for manifest, WAL envelope, table blocks, snapshots, and row records as specified. | Golden vectors match the storage format spec. |
| `M3D` | Durable publisher | Implement atomic durable publication for local filesystem and non-durable publication for cache mode. | Fault-window tests cover temp, sync, rename, parent sync, and cleanup behavior. |
| `M3E` | Durable services | Implement WAL, manifest, snapshot envelope, checkpoint, sidecar, and quarantine services. | Services return stable storage errors and do not leak product semantics. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M3TA` | Format golden tests | Lock durable bytes for every specified record/envelope. | Format spec and implementation cannot drift silently. |
| `M3TB` | Format fuzz tests | Fuzz decoders and malformed records. | Invalid bytes fail closed without panics. |
| `M3TC` | Durable fault-window tests | Inject failures around publish, append, sync, manifest update, snapshot publish, and quarantine. | Each fault produces either previous durable state or a classified recovery state. |
| `M3TD` | Cache-mode absence tests | Verify cache mode creates no WAL, manifest, snapshot, checkpoint, table, quarantine, or lock objects. | Cache mode remains explicitly non-durable. |
| `M3TE` | Backend conformance | Run lower-layer conformance over memory and local filesystem backends. | Backend behavior matches declared capabilities. |

## Convergence Notes

1. `M3TA` and `M3TB` land with `M3C`.
2. `M3C` format codec and golden vectors must close before `M3D` or `M3E`
   begin using durable bytes.
3. `M3TC` lands with `M3D` and `M3E`.
4. `M3TD` lands before cache mode is consumed by M4.

## Slice Policy

Slices should stay within one lower layer unless a fault-window test requires a
thin vertical path. Durable bytes must not be changed without updating the
format spec and golden vectors in the same slice.

## Non-Goals

1. No L5 table runtime.
2. No branch visibility.
3. No commit timeline.
4. No engine-facing L9 API.
5. No OpenDAL implementation beyond reserved architecture seams.

## Milestone Exit Gate

M3 is complete when lower storage services are durable, fault-testable,
cache-aware, and specified by golden bytes. The roadmap Test Gate Summary
remains the canonical milestone gate; this plan explains how M3 reaches it.
