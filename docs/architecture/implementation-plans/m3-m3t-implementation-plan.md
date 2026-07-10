# M3 / M3T Implementation Plan: Storage-Next Backend, Layout, Format, And Durable Services

Status: draft implementation plan

## Goal

Implement the lower storage mechanics before table, branch, and commit behavior
depend on them.

## Inputs

1. `docs/architecture/storage-architecture.md`
2. `docs/architecture/storage/l1-backend-io.md`
3. `docs/architecture/storage/l2-object-layout.md`
4. `docs/architecture/storage/l3-durable-format-codec.md`
5. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
6. `docs/spec/strata-storage-format-v1.md`
7. `docs/architecture/implementation-plans/m3-porting-log.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Porting Discipline

M3 is a port-and-tighten milestone, not a greenfield rewrite. Existing storage
code is the default source material unless a slice explicitly records why the
old behavior is obsolete, out of scope, or unsafe for the V1 architecture.

Every implementation slice must begin with a source-map note in
`docs/architecture/implementation-plans/m3-porting-log.md` that identifies:

1. Existing files and tests being ported, split, or retired.
2. Behavior that must be preserved exactly.
3. Behavior intentionally changed by the V1 architecture.
4. Behavior deferred to M4, M5, or M6.
5. Old code that can be deleted because the new owner is tested and no current
   workspace crate still depends on it.

Deletion is allowed only after replacement tests exist and references are gone.
If the old storage crate still needs a module to keep existing consumers
building, leave it in place and record the retirement in the progress tracker
instead of adding compatibility glue to storage-next.

## Current Storage Source Map

This is the starting map for M3. Each slice should refine it before editing
code.

| Target area | Current source material | Notes |
|---|---|---|
| Backend filesystem behavior | `crates/storage/src/durability/layout.rs`, `crates/storage/src/manifest.rs`, `crates/storage/src/segment_builder.rs`, `crates/storage/src/durability/wal/writer.rs`, `crates/storage/src/durability/disk_snapshot/writer.rs` | M2 already created backend traits. M3 ports the proven filesystem behavior behind those traits instead of spreading raw filesystem calls. |
| Object layout | `crates/storage/src/durability/layout.rs`, `crates/storage/src/layout.rs`, `crates/storage/src/quarantine.rs`, `crates/storage/src/segmented/quarantine_protocol.rs` | Layout decisions move behind `storage-next::layout`; no ad hoc object-name construction outside that module. |
| Durable format codec | `crates/storage/src/durability/format/*`, `crates/storage/src/key_encoding.rs`, `crates/storage/src/stored_value.rs`, `crates/storage/src/durability/payload.rs`, `crates/storage/src/segment.rs`, `crates/storage/src/segment_builder.rs` | Port codec knowledge in pieces and lock each piece with golden vectors before services consume it. |
| WAL service | `crates/storage/src/durability/wal/*`, `crates/storage/src/durability/format/wal_record.rs`, `crates/storage/src/durability/recovery.rs`, `crates/storage/src/durability/recovery_bootstrap.rs` | Preserve fault and recovery behavior where it matches V1. Public transaction semantics remain out of scope. |
| Manifest and watermark service | `crates/storage/src/durability/format/manifest.rs`, `crates/storage/src/durability/format/watermark.rs`, `crates/storage/src/manifest.rs`, `crates/storage/src/durability/commit_adapter.rs` | M3 owns durable manifest mechanics only; branch and commit meaning waits for later milestones. |
| Snapshot and checkpoint service | `crates/storage/src/durability/disk_snapshot/*`, `crates/storage/src/durability/format/snapshot.rs`, `crates/storage/src/durability/checkpoint_runtime.rs`, `crates/storage/src/durability/decoded_snapshot_install.rs` | M3 owns container/envelope mechanics. Engine-owned primitive snapshot payload meaning is not reintroduced here. |
| Quarantine and recovery classification | `crates/storage/src/quarantine.rs`, `crates/storage/src/segmented/quarantine_protocol.rs`, `crates/storage/src/segmented/recovery.rs`, `crates/storage/src/durability/recovery.rs` | Port quarantine mechanics as storage diagnostics, not product semantics. |
| Existing lower-layer tests | `crates/storage/src/segmented/tests/publish_failures.rs`, `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`, `crates/storage/src/segmented/tests/post_restart_branch.rs`, `crates/storage/src/segmented/tests/gc_under_degradation.rs`, `crates/storage/src/segmented/tests/lifecycle.rs` | Treat these as evidence. Port useful cases into M3T tests, but do not preserve old behavior blindly. |

## Priority Execution Order

1. `M3A1`: Close backend capability validation and compare M2 backend behavior
   against the filesystem assumptions used by current storage durability code.
2. `M3B1`: Port object families, temp paths, lock paths, and quarantine paths.
3. `M3B2`: Add layout property tests and remove any lower-layer ad hoc path
   construction found during the source-map pass.
4. `M3C1`: Port key, row, primitive-tag, and stored-value codec decisions with
   golden vectors.
5. `M3C2`: Port manifest, watermark, and segment metadata codecs with golden
   vectors.
6. `M3C3`: Port WAL envelope and record codecs with malformed-input tests.
7. `M3C4`: Port snapshot envelope and section codecs with golden vectors.
8. `M3C5`: Add the first cargo-fuzz package only if M3C has real byte parsers
   to fuzz; otherwise keep the documented fuzz scaffold.
9. `M3D1`: Implement the local durable publisher and cache-mode non-durable
   publisher over the backend and layout modules.
10. `M3TC1`: Add fault-window tests for temp write, sync, rename, parent sync,
    and cleanup behavior.
11. `M3E1`: Implement database manifest and payload-opaque table manifest
    service mechanics.
12. `M3TE1`: Expand manifest service tests to reference-grade recovery-pointer
    coverage.
13. `M3E2`: Implement WAL service mechanics.
14. `M3TC2`: Expand WAL service tests to reference-grade durability coverage.
15. `M3TB2`: Add service-level WAL fuzz target if `M3TC2` justifies a narrow
    testkit surface; otherwise record the deferral.
16. `M3E3`: Implement snapshot, checkpoint, and sidecar service mechanics.
17. `M3TC3`: Expand snapshot, checkpoint, and sidecar service tests to
    reference-grade durability coverage.
18. `M3E4`: Implement quarantine service mechanics and recovery
    classifications.
19. `M3TC4`: Expand quarantine and recovery-classification tests to
    reference-grade durability coverage.
20. `M3TD1`: Prove cache mode creates none of the durable object families.
21. `M3F`: Replace opaque WAL commit payload bytes with row-native commit
    payload format before L7 depends on WAL replay.
22. `M3G`: Implement immutable table format bytes before L5 depends on table
    builder/reader mechanics.
23. `M3TF1`: Re-run lower-layer conformance across memory and local filesystem
    backends and record any retired old-code files.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M3A` | Backend operations | Implement local filesystem and memory backend operations required by V1 modes. | Capability validation rejects unsupported mode/backend combinations. |
| `M3B` | Object layout | Implement object names, prefixes, families, temp paths, lock paths, and quarantine paths. | Layout has no ad hoc string construction outside the layout module. |
| `M3C` | Format codec | Implement durable encoders and decoders for manifest, WAL envelope, table blocks, snapshots, and row records as specified. | Golden vectors match the storage format spec. |
| `M3D` | Durable publisher | Implement atomic durable publication for local filesystem and non-durable publication for cache mode. | Fault-window tests cover temp, sync, rename, parent sync, and cleanup behavior. |
| `M3E` | Durable services | Implement WAL, database manifest, payload-opaque table manifest, snapshot envelope, checkpoint, sidecar, and quarantine services. | Services return stable storage errors and do not leak product semantics. |
| `M3F` | WAL commit payload format | Replace the M3C3/M3E2 opaque WAL payload with a bounded storage-row commit payload. | WAL records cannot be validly constructed from arbitrary engine-shaped payload bytes. |
| `M3G` | Immutable table format | Implement stable V1 immutable table header, footer, block frame, data entry, index, properties, compression, checksum, golden, and fuzz coverage. | Table bytes are storage-row-native, strict, documented, and ready for L5 table runtime work. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M3TA` | Format golden tests | Lock durable bytes for every specified record/envelope. | Format spec and implementation cannot drift silently. |
| `M3TB` | Format fuzz tests | Fuzz decoders and malformed records. | Invalid bytes fail closed without panics. |
| `M3TC` | Durable fault-window tests | Inject failures around publish, append, sync, manifest update, snapshot publish, and quarantine. | Each fault produces either previous durable state or a classified recovery state. |
| `M3TD` | Cache-mode absence tests | Verify cache mode creates no WAL, manifest, snapshot, checkpoint, table, quarantine, or lock objects. | Cache mode remains explicitly non-durable. |
| `M3TE` | Manifest service tests | Harden database manifest and table manifest service coverage. | Manifest services preserve recovery facts and publish uncertainty precisely. |
| `M3TF` | Backend conformance | Run lower-layer conformance over memory and local filesystem backends. | Backend behavior matches declared capabilities. |

## Convergence Notes

1. `M3TA` and `M3TB` land with `M3C`.
2. `M3C` format codec and golden vectors must close before `M3D` or `M3E`
   begin using durable bytes.
3. `M3TC` lands with `M3D` and `M3E`.
4. `M3TD` lands before cache mode is consumed by M4.
5. `M3TE1` lands after `M3E1` and before later recovery layers treat database
   manifest behavior as stable recovery-pointer infrastructure.
6. `M3TC3` lands after `M3E3` and before `M3E4`; quarantine recovery should not
   assume snapshot or sidecar behavior until M3TC3 closes.
7. `M3TC4` lands after `M3E4` and before `M3TD1`; cache-mode absence should
   test the final quarantine object families and durable mutation paths.
8. `M3TF` closes after durable services have enough backend behavior to
   validate end-to-end capability claims.
9. `M3G` closes before M4A table runtime work. L5 should consume stable table
   bytes rather than inventing a private table object format.

## Slice Briefs

1. `M3E1`: `docs/architecture/implementation-plans/m3e1-manifest-service-implementation-brief.md`
2. `M3TE1`: `docs/architecture/implementation-plans/m3e1-manifest-test-suite-plan.md`
3. `M3TE1` implementation: `docs/architecture/implementation-plans/m3te1-manifest-test-implementation-plan.md`
4. `M3E2`: `docs/architecture/implementation-plans/m3e2-wal-service-implementation-brief.md`
5. `M3TC2` / `M3TB2`: `docs/architecture/implementation-plans/m3e2-wal-test-suite-plan.md`
6. `M3TC2` implementation: `docs/architecture/implementation-plans/m3tc2-wal-test-implementation-plan.md`
7. `M3E3`: `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-implementation-brief.md`
8. `M3TC3`: `docs/architecture/implementation-plans/m3e3-snapshot-checkpoint-sidecar-test-suite-plan.md`
9. `M3E4`: `docs/architecture/implementation-plans/m3e4-quarantine-recovery-implementation-brief.md`
10. `M3TC4`: `docs/architecture/implementation-plans/m3e4-quarantine-recovery-test-suite-plan.md`
11. `M3TD1`: `docs/architecture/implementation-plans/m3td1-cache-mode-absence-test-plan.md`
12. `M3F`: `docs/architecture/implementation-plans/m3f-wal-commit-payload-implementation-brief.md`
13. `M3F` tests: `docs/architecture/implementation-plans/m3f-wal-commit-payload-test-plan.md`
14. `M3G`: `docs/architecture/implementation-plans/m3g-immutable-table-format-implementation-brief.md`
15. `M3G` tests: `docs/architecture/implementation-plans/m3g-immutable-table-format-test-plan.md`

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
