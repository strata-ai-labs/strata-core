# M4P Implementation Plan: Storage-Next Parity Restoration

Status: draft program plan

## Goal

Restore the old storage engine mechanics that storage-next lost or weakened,
without uprooting the L1-L9 storage-next architecture.

This plan converts the audit findings into an ordered implementation program.
The order is deliberately L1 through L9 for ownership: every missing mechanic is
restored in the layer that owns it. Execution is dependency-aware rather than a
hard requirement to finish every lower-layer hardening item before the measured
L5/L6/L8 serving-path gaps. Benchmarks and L9 APIs are proof gates, not places
to add benchmark-only shortcuts.

## Inputs

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
3. `docs/architecture/storage/l1-backend-io.md`
4. `docs/architecture/storage/l2-object-layout.md`
5. `docs/architecture/storage/l3-durable-format-codec.md`
6. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
7. `docs/architecture/storage/l5-table-runtime.md`
8. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
9. `docs/architecture/storage/l7-commit-runtime.md`
10. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
11. `docs/architecture/storage/l9-storage-api-boundary.md`
12. `docs/architecture/storage/test-density-roadmap.md`
13. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
14. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
15. Existing M4 layer implementation and test plans under
    `docs/architecture/implementation-plans/M4/`.

## Audit Cross-Reference Rule

The audit documents are required source material for every M4P slice, not
optional background reading.

Required audit references:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`,
   especially the L1-L9 layer audit, Restoration Source Map, Audit Matrix, and
   Final Parity Matrix sections.
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`,
   especially the old invariants, proof slices, proof acceptance gates, and
   correction acceptance gates.
3. Supporting perf evidence under `docs/architecture/perf-tuning/perf-p*.md`
   and prior correction plans under `docs/architecture/perf-tuning/perf-i*.md`
   when a slice touches point-read, load, scan, compaction, or serving-path
   behavior.

A slice plan is incomplete until it cites the exact audit file and section
heading for every audit finding it closes or defers. Each cited finding must map
to old evidence files, storage-next target files, required proof or counters,
and any semantic decision register entry needed to reinterpret old behavior.

## Program Principles

1. Keep the existing storage-next layers. Missing mechanics must be assigned to
   the layer that owns them.
2. Do not create L9 fast paths to hide lower-layer inefficiency.
3. Do not port old code wholesale when old code mixed layer ownership. Port the
   invariant, not the mixed implementation shape.
4. The old storage engine is the executable reference for behavior and
   asymptotic cost. Storage-next remains the architectural target.
5. Every implementation slice must include its own proof: unit/model tests,
   source guards, generated/fuzz tests where applicable, and benchmark or
   counter gates for performance-sensitive mechanics.
6. If a slice requires durable format changes, stop and write a format-specific
   decision plan before editing code.
7. Do not weaken existing storage-next tests, fuzz targets, source guards,
   golden vectors, or crash harnesses to make restoration work pass.

## Program Decomposition

| Package | Layer | Objective | Primary exit gate |
| --- | --- | --- | --- |
| `M4P-L1` | Backend IO | Restore durable deletion/sync semantics and enforce the IO boundary. | Durable backend conformance covers publish, append, sync, lock, delete/sync, and fault classification; production storage code outside `backend/local_fs.rs` cannot use direct filesystem IO. |
| `M4P-L2` | Object layout | Close object-family layout and reachability naming gaps. | Every durable service object family has validated names, prefixes, diagnostics roles, and cleanup/recovery mapping tests. |
| `M4P-L3` | Durable format / codec | Verify format compatibility before any restored service uses new facts. | Goldens, fuzz targets, and codec mismatch tests cover any touched WAL, manifest, snapshot, table, sidecar, or checkpoint payloads. |
| `M4P-L4` | Log / manifest / snapshot services | Restore durable topology and restart proof across WAL, checkpoint, table manifest, rewrite publication, and quarantine. | Crash/restart tests classify each publication window without losing reachable rows or resurrecting unmanifested table objects. |
| `M4P-L5` | Table runtime | Restore table seek, cursor, reader, compaction, and source-layout observability. | Table point seeks and scan cursors are bounded by table/index shape, and table compaction output preserves sorted non-overlapping level facts. |
| `M4P-L6` | Branch-isolated LSM runtime | Restore old serving topology: bounded point source pruning, lazy scans, history, inheritance, and materialization. | Point reads and scans are bounded by active + frozen + L0 + level count, not total table count; independent model tests match old MVCC/branch behavior. |
| `M4P-L7` | Commit runtime | Closed commit parity narrowly: validation, timeline, generation, visibility, and pressure facts. | Commit correctness and timeline lookup are restored without absorbing L5/L6 read-path logic; remaining retry/scheduler policy is owned by L8/L9. |
| `M4P-L8` | Lifecycle / recovery / maintenance | Restore automatic maintenance, write admission, durable close, retention, budget, and recovery orchestration. | Sustained L9 load no longer strands unbounded L0/source fanout, and durable crash/recovery proof remains intact. |
| `M4P-L9` | Storage API boundary | Expose only the storage-shaped mechanics that future engine-next needs after lower layers are restored. | Engine-facing API supports read-set facts, diagnostics, mode contracts, and benchmarks through normal L9 paths only; engine-next dependency guards become mandatory when that crate exists. |

## Slice Document Requirement

This document is the program index. Before implementation begins for any
non-trivial package item, write a detailed slice plan and matching test plan
under `docs/architecture/implementation-plans/M4P/`, following the existing M4
slice style and the local `M4P/README.md` naming convention.

Each slice plan must include:

1. objective;
2. audit finding references by file and section heading;
3. old-source map and storage-next target map;
4. predecessors and exact lower-layer dependencies;
5. implementation scope and non-goals;
6. correctness, crash/fault, source-guard, fuzz/generated, and benchmark gates;
7. expected mechanical counter movement for performance-sensitive work;
8. a stop condition if the measured counter movement does not appear.

## Execution Model

Layer order is ownership order. A higher layer may receive small API adapters
early only when a lower-layer test needs a proof surface, but the behavioral fix
still belongs to the owning lower layer.

Do not block all serving-path restoration on unrelated lower-layer hardening.
The audits classify L1-L4 as targeted correctness and boundary hardening, while
the known source-fanout and scan gaps sit in L5/L6/L8. A slice may proceed once
the lower-layer blockers on that exact path are closed or explicitly proven not
to apply.

Execution uses this sequence:

1. **Thin L1-L4 dependency pass.** Close only the lower-layer decisions needed
   by the first serving-path slices: IO/source guards, object-family/classifier
   helpers, "no durable format change" decisions, and publication/recovery proof
   for any touched table rewrite path.
2. **L5/L6 source-shape restoration.** Restore counters, level-aware point
   pruning, lazy scan source planning, history/timestamp/facts boundedness, and
   table reader/cursor mechanics in the order profiling justifies. Profiling
   decides priority; the audit backlog still decides what must close before
   program exit.
3. **L8 maintenance restoration.** Restore automatic flush drain, score-based
   compaction scheduling, materialization scheduling, pressure/admission, and
   sustained-load bounded fanout.
4. **L7/L9 API and validation closeout.** Restore public read-set facts,
   timeline lookup efficiency, diagnostics, mode contracts, and future
   engine-next dependency guards without moving read-path logic into L9.
5. **Durable hardening closeout.** Finish remaining L1-L4/L8 crash, cleanup,
   table-object reference recovery, flush-watermark, pending-release, close,
   quarantine, checkpoint extension, and format-spec work that was not on the
   first serving-path dependency chain.

Performance checkpoints run after L5, L6, L8, and L9. They do not reorder the
implementation ownership.

## First Executable Slices

These are the first slice-level plans to write and implement. They are not the
entire program. Later backlog slices are required for M4P completion; counters
decide their priority and expected performance effect.

| Slice | Owner | Objective | Exit gate |
| --- | --- | --- | --- |
| `M4P-L1A` | L1 | Add IO boundary guard and record durable-delete decision scope. | Production storage-next code outside `backend/local_fs.rs` cannot perform direct filesystem IO; durable-delete work is either in-scope for touched L4 cleanup or deferred with reason. |
| `M4P-L2A` | L2 | Add table-object/object-family classifier helpers for lifecycle use. | L8 cleanup/reachability code no longer parses canonical object names with raw string shape checks. |
| `M4P-L3A` | L3 | Record format-impact decision for serving-path restoration. | Source-shape slices prove they do not need durable byte changes, or they open a separate format decision plan. |
| `M4P-L4A` | L4 | Prove touched table rewrite/publication windows before L8 auto-maintenance relies on them. | Crash/restart tests classify the exact flush/compaction/materialization publication windows used by the L8 slice. |
| `M4P-L5A` | L5 | Add table/source perf counters and table facts needed by L6/L8. | Benchmarks and unit tests report table seeks, data-block reads, cursor opens, decoded rows, cache hits/misses, and bloom rejections where supported. |
| `M4P-L6A` | L6 | Add branch source-layout diagnostics and source-class counters. | Point/scan/history counters separate active, frozen, L0, nonzero levels, inherited L0, and inherited nonzero levels. |
| `M4P-L6B` | L6 | Restore nonzero-level point-read pruning. | Point reads probe all active/frozen/L0 sources and at most one table per nonzero level per readable layer. |
| `M4P-L6C` | L6 | Restore lazy nonzero-level scan planning. | Scan setup creates per-table cursors for L0 and lazy level cursors for L1+, with prefix/range overlap pruning before cursor creation. |
| `M4P-L6D` | L6 | Remove row-scan behavior from history, timestamp resolution, and hot branch facts. | History for one key, timestamp lookups, and normal facts calls do not scan unrelated user rows. |
| `M4P-L8A` | L8 | Restore automatic maintenance scheduling after mutating commits. | Sustained L9 loads no longer require benchmark-specific manual maintenance to keep source fanout bounded. |
| `M4P-L8B` | L8 | Restore score-based compaction drain. | L0 and nonzero-level shape remain bounded at 100K, 1M, 5M, and 10M through normal L9 writes. |
| `M4P-L8C` | L8 | Restore write-admission and pressure policy. | Mutating commits either drive maintenance, slow/stall/reject with typed facts, or document an intentional no-stall V1 policy with bounded-fanout proof. |
| `M4P-L9A` | L9 | Expose storage-shaped read-set facts. | L9 accepts conflict inputs needed by future engine-next without exposing product transaction sessions. |
| `M4P-L9B` | L9 | Expose diagnostics that lower layers already own. | L9 reports source shape, source probes, scan cursor setup, maintenance debt, and mode facts without exposing lower-layer table, WAL, or object types. |

## Audit Coverage Backlog

The first executable slices are not the whole program. They cover the measured
source-fanout critical path first. The remaining audit findings must still be
closed through later slice plans before M4P can exit.

| Audit finding | Owner | Coverage plan |
| --- | --- | --- |
| Durable delete outcome, durable backend conformance, and IO source guard. | L1/L4 | `M4P-L1A`, then L4 cleanup-service use in durable hardening closeout. |
| L2 manifest-object docs drift, table-object classification, raw naming guard, and `tmp/` namespace decision. | L2/L4/L8 | `M4P-L2A`, plus L2 doc/spec closeout before durable cleanup/recovery slices. |
| Checkpoint row-section codec, retained-history extension codec, manifest-family specs/goldens, fuzz routing, and identity-codec boundary. | L3/L4/L8 | `M4P-L3A` decides format impact for serving-path work; later L3 format closeout moves or documents every remaining durable byte surface. |
| Durable cleanup reports, L4 manifest-service docs, service conformance, WAL policy tests, and object-durable fenced publication decision. | L4/L1/L8 | `M4P-L4A` covers touched rewrite windows; durable hardening closeout covers the remaining L4 service and future object-durable gates. |
| Immutable reader eagerness, block cache/bloom integration, block-backed cursors, streaming table compaction, overlap-aware splitting, frozen-table negative lookup, and L5 asymptotic counters. | L5 | `M4P-L5A` starts with counters/facts; later L5 reader/cursor/compaction slices restore table-runtime parity. Counters decide priority, not whether the gap must close. |
| L6 point pruning, lazy level scans, row-scanning history, timestamp lookup, hot facts recomputation, read-view cloning, eager compaction sources, eager materialization, fork ergonomics, and L6 source-count tests. | L6/L5/L7/L8 | `M4P-L6A` through `M4P-L6D` cover counters, point, scan, history/timestamp/facts; later L6 pinned-view and streaming compaction/materialization slices cover the remaining branch-runtime gaps. |
| Independent branch commit concurrency, pending-version visibility, conflict-source cost, WAL allocation cost, timeline lookup, cache-leaning internal defaults, vector branch registry, and quiesce retry integration. | L7/L6/L8/L9 | L7 is closed with documented V1 semantic decisions, source guards, and perf counters. L8/L9 own retry/deadline/scheduler behavior and future admission-policy changes. |
| Automatic maintenance scheduling, write admission, flush drain, compaction scoring, multi-branch flush-watermark proof, pending releases, close retry/deadline, budget/rate limiting, and lifecycle crash/perf proof. | L8/L4/L6/L5 | `M4P-L8A` through `M4P-L8C` cover the sustained-load critical path; later L8 durable/lifecycle slices close watermark, pending-release, close, budget, and crash windows. |
| Engine-next consumer absence, public read-set facts, pressure/stall facts, explicit maintenance policy, checkpoint extension payloads, diagnostics, wasm-none, and timeline lookup. | L9/L8/L7/L6 | `M4P-L9A` and `M4P-L9B` cover diagnostics/read-set; later L9 mode/API closeout covers checkpoint extension, wasm-none, pressure facts, and engine-next dependency guards when that crate exists. |
| Product decisions: latest/history TTL behavior, copied `Materializing` inherited-layer status, table-object reference recovery, pool budget model, and wasm-none mode shape. | Owning layer per decision | The semantic decision register in the test plan must record each decision before differential tests skip or reinterpret old behavior. |

## Layer Plans

### M4P-L1: Backend IO

Scope:

1. Add a durable-delete or namespace-sync contract to L1, with outcome facts
   that distinguish durable removal from visibility/durability uncertainty.
2. Thread the contract into L4 services that delete WAL segments, snapshots,
   sidecars, quarantine objects, and temporary artifacts.
3. Add reusable durable-backend conformance tests for memory and localfs
   capability differences.
4. Add a production source guard that rejects direct filesystem IO outside
   `crates/storage-next/src/backend/local_fs.rs`.

Old evidence:

- `crates/engine/src/database/open.rs`
- `crates/storage/src/durability/layout.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`

Storage-next targets:

- `crates/storage-next/src/backend/`
- `crates/storage-next/src/service/`
- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/src/lifecycle/quarantine.rs`

Exit gate:

- Durable cleanup no longer relies on implicit localfs behavior above L1.
- Fault tests cover delete-before-visible, delete-visible-but-unsynced, and
  unsupported delete durability on cache backends.

### M4P-L2: Object Layout

Scope:

1. Inventory every object family used by WAL, manifest, branch catalog, table
   manifest, pending releases, checkpoint, snapshot, sidecar, table object,
   quarantine, and temporary publication.
2. Ensure object roles are visible to diagnostics without exposing raw path
   construction to higher layers.
3. Add layout conformance tests for family prefixes, database id isolation,
   branch id isolation, reserved names, temp names, and cleanup prefixes.
4. Add source guards against ad-hoc object-name string assembly outside L2.

Old evidence:

- `crates/storage/src/durability/layout.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/quarantine.rs`

Storage-next targets:

- `crates/storage-next/src/object/`
- `crates/storage-next/src/layout/`
- `crates/storage-next/src/service/`

Exit gate:

- L4/L8 can ask L2 for every object family they publish, recover, retain, or
  delete. No durable service constructs object names by string concatenation.

### M4P-L3: Durable Format / Codec

Scope:

1. Verify existing goldens and fuzz targets still cover every durable format
   touched by M4P.
2. Add new goldens only if L4/L8 require new durable facts.
3. Preserve codec mismatch classification and identity-codec behavior.
4. Add compatibility tests before and after any payload change.

Old evidence:

- `crates/storage/src/durability/format/`
- `crates/storage/src/durability/payload.rs`
- `crates/storage/src/durability/wal/`

Storage-next targets:

- `crates/storage-next/src/format/`
- `crates/storage-next/fuzz/fuzz_targets/format_*.rs`
- `crates/storage-next/src/service/`

Exit gate:

- No L4/L8 restoration slice changes durable bytes without updated goldens,
  fuzz coverage, and an explicit compatibility decision.

### M4P-L4: Log / Manifest / Snapshot Services

Scope:

1. Restore table-manifest and rewrite-publication restart proof for flush,
   compaction, materialization, and branch lifecycle operations.
2. Prove WAL truncation, checkpoint publication, flush watermark persistence,
   snapshot pruning, sidecar cleanup, quarantine, purge, and repair under
   partial-publication windows.
3. Classify reachable, orphaned, missing, quarantined, corrupt, and
   unreferenced table objects without rejecting valid storage-next table
   topology that preserves old LSM mechanics.
4. Feed object-role and durable-delete facts from L1/L2 into service outcomes.

Old evidence:

- `crates/storage/src/durability/wal/`
- `crates/storage/src/durability/disk_snapshot/`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/recovery.rs`
- `crates/storage/src/durability/recovery_bootstrap.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segmented/recovery.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`

Storage-next targets:

- `crates/storage-next/src/service/`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/lifecycle/rewrite_publication.rs`
- `crates/storage-next/src/lifecycle/table_reachability.rs`
- `crates/storage-next/src/lifecycle/quarantine.rs`

Exit gate:

- Crash/restart tests cover every durable transition window used by branch
  flush, compaction, materialization, checkpoint, WAL truncation, branch
  clear/delete, quarantine, purge, and close.

### M4P-L5: Table Runtime

Scope:

1. Add table-level source-layout diagnostics required by L6/L8 benchmarks.
2. Prove point lookup uses keyed seek/filter behavior rather than unrelated row
   scanning.
3. Prove range and prefix cursors can support lazy level iteration.
4. Verify table compaction output ordering, non-overlap, tombstone retention
   input policy, and block-cache behavior.
5. Add perf-trace counters by table source class without making benchmarks the
   only proof.

Old evidence:

- `crates/storage/src/memtable.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/segment_builder.rs`

Storage-next targets:

- `crates/storage-next/src/table/`
- `crates/storage-next/src/observability/perf_trace.rs`
- `crates/storage-next/src/testkit/`

Exit gate:

- Table-local tests prove seek/cursor work is bounded by table/index shape.
- L6 can consume table cursors and table facts without adding table-specific
  logic to branch code.

### M4P-L6: Branch-Isolated LSM Runtime

Scope:

1. Restore point-read source pruning across active, frozen, owned L0, owned
   L1+, inherited L0, and inherited L1+ sources.
2. Restore lazy scan planning: L0 contributes table cursors and each nonzero
   level contributes a lazy level cursor, not one eager cursor per table.
3. Restore efficient history over one physical key without scanning unrelated
   keys.
4. Preserve MVCC, tombstone, TTL, branch fork, fork-version, child shadowing,
   materialization, and compaction semantics.
5. Add branch source-layout diagnostics and perf-trace counters.

Old evidence:

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/tests/fork.rs`
- `crates/storage/src/segmented/tests/leveled.rs`
- `crates/storage/src/segmented/tests/materialize.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`

Storage-next targets:

- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state/`
- `crates/storage-next/src/branch/tests/`
- `crates/storage-next/src/testkit/branch_lsm/`

Exit gate:

- Latest reads, version reads, timestamp reads, history, prefix scans, and
  range scans match an independent branch model.
- Source counters show nonzero-level point probes are bounded by level count
  and scan cursor setup is bounded by level count, not total table count.

### M4P-L7: Commit Runtime

Status: closed with documented V1 deltas; L8/L9 own retry/deadline and
automatic maintenance policy.

Scope:

1. Keep blind-write validation fast paths ahead of unnecessary source capture.
2. Preserve read-set and CAS validation internally, and prepare the L9 mapping
   for public storage-shaped read facts.
3. Prove commit timeline lookup does not become a scale bottleneck.
4. Verify branch generation, deletion, quiesce, durable-but-not-visible,
   unresolved durable, replay, and visibility invariants after L6/L8 changes.
5. Add write-stall/backpressure facts only as L7 facts consumed by L8/L9, not
   as read-path behavior.

Old evidence:

- `crates/storage/src/txn/context.rs`
- `crates/storage/src/txn/manager.rs`
- `crates/storage/src/txn/validation.rs`
- `crates/storage/src/txn/lock_ordering.rs`
- `crates/storage/src/durability/commit_adapter.rs`

Storage-next targets:

- `crates/storage-next/src/commit/`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/`
- `crates/storage-next/src/api/commit.rs`
- `crates/storage-next/src/api/runtime.rs`

Exit gate:

- Commit correctness tests pass under the restored L5/L6 topology.
- Perf counters and source guards prove conflict validation, timeline lookup,
  durable WAL facts, branch registry probes, replay classification, admission
  pressure facts, and runtime-default durability mapping.
- L7 does not absorb L5/L6 source planning or L8 maintenance scheduling.

### M4P-L8: Lifecycle / Recovery / Maintenance

Scope:

1. Restore automatic flush, compaction, and materialization scheduling after
   mutating commits.
2. Restore write-admission policy using L6/L5 pressure facts: slow, stall,
   reject, or drive maintenance with typed outcomes.
3. Ensure maintenance drains all eligible frozen and L0 backlog across branches
   without benchmark-specific manual loops.
4. Restore budget behavior and held reservations where cumulative concurrent
   work can exceed configured limits.
5. Complete pending-release durability, close quiesce retry/deadline behavior,
   retention, quarantine, and repair integration.

Old evidence:

- `crates/engine/src/background.rs`
- `crates/engine/src/database/transaction.rs`
- `crates/engine/src/database/lifecycle.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/pressure.rs`
- `crates/storage/src/rate_limiter.rs`
- `crates/storage/src/memory_stats.rs`
- `crates/storage/src/runtime_config.rs`

Storage-next targets:

- `crates/storage-next/src/lifecycle/`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/flush.rs`
- `crates/storage-next/src/lifecycle/budget.rs`
- `crates/storage-next/src/lifecycle/wal_growth.rs`
- `crates/storage-next/src/api/maintenance.rs`

Exit gate:

- Sustained 100K, 1M, 5M, and 10M L9 loads do not leave L0/source fanout
  growing linearly with total rows.
- Normal writes do not require user-visible manual maintenance to preserve
  healthy storage shape.

### M4P-L9: Storage API Boundary

Problem statement:

L9 is the storage-next boundary that future engine-next should consume. The old
engine preserved snapshot-isolation validation with product transaction wrappers
above storage, while old storage owned the storage-shaped mechanics: snapshot
read facts, CAS facts, version-bounded reads, mode/runtime configuration, and
maintenance-facing diagnostics. Storage-next must expose those same mechanics
without rebuilding product transaction sessions inside storage and without
letting benchmarks or future production crates reach below the L9 API to hide
lower-layer gaps.

Scope:

1. Add storage-shaped read-set facts to the public commit API so future
   engine-next can preserve old snapshot-isolation validation without exposing
   product transaction sessions.
2. Expose diagnostics needed to prove source shape, source probes, scan cursor
   setup, maintenance debt, and mode metadata.
3. Document and test cache, durable-local standard, durable-local always, and
   wasm-none-supported subsets.
4. Keep benchmarks on normal L9 APIs and remove any benchmark-only lower-layer
   bypasses.
5. Add source/dependency guards so, once engine-next exists, normal production
   crates above it do not depend on `strata-storage-next`.

Old evidence:

- `crates/storage/src/traits.rs`
- `crates/storage/src/runtime_config.rs`
- `crates/storage/src/txn/context.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/transaction.rs`
- `crates/engine/src/transaction/context.rs`

Storage-next targets:

- `crates/storage-next/src/api/`
- `crates/storage-next/src/lib.rs`
- `crates/storage-next/tests/api_conformance.rs`
- `benchmarks/src/bin/storage_next_l9_scale.rs`

Current status:

- `CommitCondition` already exposes explicit per-key compare-and-set checks.
  This is necessary but not sufficient: old snapshot isolation also validated
  read-set entries captured from snapshot reads and scans.
- `DiagnosticsOutcome` already reports many storage facts, but L9 does not yet
  guarantee all facts needed to interpret sustained-load shape: source probes,
  scan cursor setup, maintenance debt, and mode/subset metadata.
- `StorageOpenOptions` has explicit cache and durable-local modes, but the
  supported mode matrix and wasm-none subset are not yet a tested contract.
- `storage_next_l9_scale` uses `StorageRuntime` for normal load/maintenance
  paths and already has some source guards against explicit load-loop drains;
  L9 must make that a named gate and remove any remaining lower-layer bypasses.
- Dependency guards currently protect the storage-next API from exposing lower
  layer types. They do not yet prove that future production crates above
  engine-next avoid direct `strata-storage-next` dependencies once engine-next
  exists.

Mandatory design decisions:

1. **Storage-shaped read facts, not product sessions.** L9 may expose
   branch/space/key/version facts and commit options, but must not expose
   transaction handles, primitive names, JSON/event/vector/search concepts, or
   engine workflow vocabulary.
2. **Read-set validation is separate from CAS validation.** Existing
   `CommitCondition` remains the explicit CAS/current-version condition surface.
   The new read-set surface records snapshot observations from reads and scans.
   Both participate in commit validation when supplied.
3. **Scan read facts follow old semantics.** Old `TransactionContext::scan_prefix`
   tracked the snapshot rows returned by the scan, not a range-lock/phantom
   predicate. L9 should therefore model scan capture as point read facts for
   returned rows unless a later engine-next plan deliberately changes isolation.
4. **Diagnostics summarize storage mechanics without leaking storage internals.**
   L9 diagnostics may report counts, bytes, durations, modes, source classes,
   queue debt, and cursor/probe counters. They must not expose table object
   names, WAL records, manifest snapshots, backend service handles, or lower
   layer concrete types.
5. **Mode contracts are explicit.** Cache is volatile and must not perform WAL
   work. Durable-local standard and durable-local always are both supported
   native modes with documented durability summaries. Object-durable and
   distributed modes remain candidate/unsupported unless a later plan promotes
   them. Wasm-none support is a subset contract, not an implicit fallback mode.
6. **Benchmarks are API consumers.** Benchmarks must exercise the same public L9
   open, commit, read, diagnostics, and maintenance APIs that engine-next will
   use. A benchmark-only lower-layer bypass is a correctness bug.
7. **Future dependency direction is mechanical.** Until engine-next exists,
   storage-next conformance proves the intended boundary. Once engine-next
   exists, ordinary production crates above it must depend on engine-next rather
   than directly importing `strata-storage-next`.

Implementation plan:

1. **L9-A: Public Read-Set Fact Types**
   - Add storage-owned fact types in `crates/storage-next/src/api/commit.rs`:
     `CommitReadFact`, `CommitObservedVersion`, and a small grouping helper if
     useful, such as `CommitReadSet`.
   - Fields must stay storage-shaped: `StorageSpaceId`, `StorageKey`, and
     `CommitVersion`. `CommitVersion::ZERO` continues to represent an absent
     snapshot observation, matching old validation mechanics.
   - Add constructors for present and absent observations and accessors for all
     fields.
   - Reject duplicate read facts for the same `(storage_space, key)`.
   - Reject invalid present version zero; zero is only valid for absent.
   - Keep `CommitCondition` unchanged as the explicit CAS surface.
   - Add `CommitBatch::with_read_facts(...)` or equivalent builder API that can
     be combined with existing conditions and options.

2. **L9-B: Commit Runtime Mapping**
   - Map supplied read facts into the lower commit runtime's read-validation
     path before writes become visible.
   - Preserve old first-committer-wins behavior: if the current visible version
     differs from the observed version, reject the commit with a typed conflict
     error and no partial visibility.
   - Validate read facts independently from write duplicate checks. A key may be
     read and later written; that is the common read-modify-write case.
   - Preserve CAS behavior and error classification for `CommitCondition`.
   - Add commit summary/perf counters for read facts checked, read conflicts,
     CAS facts checked, and CAS conflicts, or confirm existing counters are
     complete and exported through L9.

3. **L9-C: Read Capture Helpers**
   - Add optional helper methods on read outcomes or a small API helper that
     converts `PointReadOutcome`, `ScanReadOutcome`, and `HistoryReadOutcome`
     rows into `CommitReadFact`s.
   - Keep helper behavior explicit; reads should not mutate hidden runtime
     transaction state.
   - For scans, capture one point read fact per returned snapshot row. Do not
     add range predicates or phantom checks in this slice.
   - For absent point reads, expose an absent fact so engine-next can preserve
     old missing-key conflict detection.

4. **L9-D: Diagnostics Completion**
   - Extend `DiagnosticsOutcome` and related API structs to report:
     source-shape facts, source-probe counters, scan cursor setup counters,
     maintenance debt, queue convergence, WAL/debt facts where supported, and
     mode/subset metadata.
   - Keep diagnostic fact state explicit: `Known`, `Unknown`, or `Unsupported`.
   - Surface cache-mode facts that prove WAL work is unsupported/not-run rather
     than silently zero because it was not measured.
   - Surface durable-local policy metadata and close/commit durability summaries
     needed to compare Standard vs Always.
   - Keep lower-layer concrete objects private; expose summaries and counters
     only.

5. **L9-E: Mode And Wasm-None Contract**
   - Document the supported mode matrix in `crates/storage-next/src/api/mod.rs`
     and the architecture docs:
     cache, durable-local standard, durable-local always, unsupported candidate
     modes, and wasm-none-supported subset.
   - Ensure cache open does not require localfs or WAL capabilities.
   - Ensure durable-local opens require a backend/localfs capability and never
     silently fall back to cache.
   - Ensure always durability reports stronger sync facts than standard where
     the backend can prove them.
   - Add cfg/source guards that the wasm-none-supported subset does not import
     localfs, threads, `std::time::Instant` control-flow dependencies, or other
     unavailable native-only pieces through the public cache/read/write API.

6. **L9-F: Benchmark Boundary Cleanup**
   - Audit `benchmarks/src/bin/storage_next_l9_scale.rs` for calls into
     `crates/storage-next/src/{branch,commit,lifecycle,table,service,...}` or
     `testkit` internals on the normal benchmark path.
   - Keep diagnostic source-shape observation after the timed load loop.
   - Keep explicit flush/compact/fixed-point drain work behind explicit
     diagnostic or maintenance flags, never in the normal measured load path.
   - Ensure result JSON records the L9 diagnostics rather than lower-layer
     structs.
   - Remove benchmark-only lower-layer bypasses; if a required fact is missing,
     add it to L9 diagnostics instead.

7. **L9-G: Dependency Direction Guards**
   - Keep existing guards that `src/lib.rs` publicly exposes only `api`.
   - Add a dormant/future-aware source guard: if an `engine-next` crate exists,
     ordinary production crates above it must not import `strata-storage-next`
     directly. Allowed exceptions should be explicit integration tests,
     benchmarks, storage-next itself, migration tools, and engine-next.
   - Add a guard that API sources do not use old product vocabulary or product
     transaction/session types.
   - Add a guard that public API signatures do not expose lower-layer concrete
     storage-next types.

8. **L9-H: Closeout And Matrix Run**
   - Run API conformance across supported feature sets.
   - Run storage-next L9 cache and durable-local standard/always smoke loads
     through normal APIs.
   - Run the old-vs-new benchmark matrix only after the API/source guards pass.
   - Record known deltas as mode facts, not as benchmark exceptions.

Test plan:

1. **Read-Set API Tests**
   - `CommitReadFact::observed_present` preserves space, key, and version.
   - `CommitReadFact::observed_absent` maps to the absent/zero observation.
   - Duplicate read facts in a batch fail with `InvalidArgument`.
   - Present zero fails with `InvalidArgument`.
   - Read facts can coexist with mutations for the same key.
   - Read facts can coexist with CAS conditions without merging the two
     concepts.

2. **Read-Set Validation Tests**
   - Present read fact succeeds when current version is unchanged.
   - Present read fact rejects when another commit advanced the key.
   - Absent read fact succeeds while the key remains absent.
   - Absent read fact rejects after another commit creates the key.
   - Multi-key read set rejects atomically: no mutation from the failed batch is
     visible after conflict.
   - Read-modify-write succeeds when the read fact still matches and fails when
     stale.
   - CAS-only, read-set-only, and combined CAS/read-set failures classify
     conflicts distinctly enough for engine-next retry policy.

3. **Read Capture Tests**
   - Point read of an existing key can produce a present read fact that protects
     a later write.
   - Point read miss can produce an absent read fact that protects a later
     create.
   - Prefix/range scan capture produces facts for returned rows and no hidden
     range predicate.
   - Scan-captured facts detect changes to returned rows before commit.
   - Scan-captured facts do not reject a concurrent insert outside the returned
     row set, matching old storage behavior.
   - History/timestamp helpers, if exposed, create only storage-shaped facts and
     do not expose engine transaction vocabulary.

4. **Diagnostics Tests**
   - Cache diagnostics report mode `Cache`, known runtime state, known source
     shape, known/unsupported WAL facts as appropriate, and no durable-local
     backend facts.
   - Durable-local standard diagnostics report durable mode metadata,
     checkpoint/WAL growth facts, maintenance queue facts, and source layout.
   - Durable-local always diagnostics report always policy metadata and close or
     commit durability summaries that differ from cache.
   - Source-probe counters distinguish active, frozen, owned L0, owned nonzero,
     inherited L0, and inherited nonzero probes where supported.
   - Scan cursor setup counters distinguish source setup, table cursor opens,
     lazy nonzero-level cursor setup, and rows returned.
   - Maintenance debt reports pending tasks, active tasks, max queue depth,
     WAL-retention debt, and checkpoint enqueue/coalesce facts.
   - Unsupported facts use `Unsupported`; missing measurement uses `Unknown`;
     successful measurement uses `Known`.

5. **Mode Matrix Tests**
   - `StorageOpenOptions::cache()` opens without backend/localfs and closes
     without durable sync.
   - Durable-local standard requires a backend/localfs-capable open path and
     rejects borrowed/unsupported backend shapes according to the existing
     policy.
   - Durable-local always opens, commits, closes, and reports durable sync.
   - Object-durable and distributed candidate modes fail before runtime
     construction with `UnsupportedCapability`.
   - Cache mode does no WAL append, WAL encode, WAL checkpoint, or WAL
     truncation work on normal mutating commits.
   - Wasm-none subset has compile/source guards for cache/open/read/write API
     availability and native-only exclusions.

6. **Benchmark Boundary Tests**
   - Source guard: timed load loop does not call explicit flush, compact,
     drain, or diagnostics polling.
   - Source guard: diagnostic final drain remains opt-in.
   - Source guard: benchmark normal path imports only public L9 API, benchmark
     helpers, and standard support crates; no lower-layer storage-next modules.
   - JSON schema test: result records mode, source shape, source probes, scan
     setup, maintenance debt, queue convergence, row/byte amplification, and
     durability facts from L9 diagnostics.
   - 1M cache smoke run with source-shape diagnostics proves final L0/nonzero
     shape and queue facts are present without explicit drain work.

7. **Dependency And Vocabulary Guards**
   - `crates/storage-next/src/lib.rs` exposes `pub mod api` but not lower
     storage modules.
   - Public API signatures do not expose lower-layer concrete types.
   - API source does not import engine/product crates.
   - API source avoids product primitive vocabulary.
   - Future guard: if `crates/engine-next` exists, normal production crates
     above engine-next do not depend on `strata-storage-next` directly.

Verification commands:

```bash
cargo fmt --all --check
cargo test -p strata-storage-next --test api_conformance --all-features
cargo test -p strata-storage-next --test api_source_guard --all-features
cargo test -p strata-storage-next --test api_properties --all-features
cargo test -p strata-storage-next --test commit_runtime_properties --all-features
cargo test --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 1m --engines cache --workloads load-seq --value-bytes 150 --batch-size 1000 --samples 1000 --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m,5m,10m --engines cache,standard,always --workloads load-seq --value-bytes 150 --batch-size 1000 --samples 1000 --diagnostic-source-shape
```

Non-goals:

1. Do not build product transaction sessions in storage-next.
2. Do not add range-lock or phantom-protection semantics beyond old storage
   read-set behavior.
3. Do not expose table objects, WAL records, manifests, lifecycle state
   machines, backend services, or lower-layer error types through L9.
4. Do not make object-durable or distributed writer modes production-supported
   in this slice.
5. Do not add benchmark-only APIs or lower-layer benchmark bypasses.
6. Do not fix lower-layer performance by changing L9 semantics.

Stop conditions:

1. If read-set facts require a lower commit-runtime behavior change that cannot
   be represented as key/version validation, stop and write the L7 owner plan.
2. If diagnostics need a lower-layer counter that does not exist, add the
   counter to the owning layer rather than deriving it with scans in L9.
3. If wasm-none support requires native-only lifecycle behavior for cache mode,
   stop and split the mode contract before adding cfg patchwork.
4. If benchmarks need a lower-layer bypass to preserve performance, stop and
   fix the missing L9 diagnostic or lower-layer behavior.
5. If engine-next exists and cannot avoid direct storage-next imports, stop and
   define the engine-next adapter boundary before widening exceptions.

Exit gate:

- Until engine-next exists, L9 conformance and testkit coverage prove the
  restored boundary. Once engine-next exists, it consumes restored storage
  mechanics through L9 only.
- L9 old-vs-new benchmarks at 100K, 1M, 5M, and 10M show old-equivalent
  asymptotic source behavior and documented mode facts.
- Public commit API can express old storage read-set validation and CAS
  validation separately.
- Diagnostics expose source shape, source probes, scan cursor setup,
  maintenance debt, and mode metadata through L9 only.
- Cache, durable-local standard, durable-local always, and wasm-none-supported
  subsets have documented behavior and executable conformance tests.
- Benchmarks use normal L9 APIs only; source guards fail on lower-layer
  benchmark bypasses or explicit normal-load drains.

## Performance Proof Gates

Performance-sensitive packages must report both wall-clock numbers and
mechanical counters.

Required checkpoints:

1. After `M4P-L5`: table seek/cursor counters prove table-local bounded work.
2. After `M4P-L6`: branch point/scan counters prove source work is bounded by
   level shape.
3. After `M4P-L8`: maintenance counters prove sustained load drains backlog.
4. After `M4P-L9`: old-vs-new L9 benchmark matrix proves restored behavior
   through the public boundary.

Required benchmark scales:

1. 100K keys.
2. 1M keys.
3. 5M keys.
4. 10M keys.

Larger scales can run after the 10M source-shape proof is clean.

## Non-Goals

1. No new secondary index for benchmark wins.
2. No L9 read or scan fast path that bypasses branch/table machinery.
3. No product JSON, graph, vector, event, search, inference, intelligence, or
   StrataHub semantics in storage-next.
4. No public transaction-session resurrection in storage.
5. No durable format change without a separate format decision plan.
6. No cutover from `storage-next` to `storage` in this program.
7. No follower-mode restoration unless a separate product decision reopens it.

## Program Exit Gate

M4P is complete when:

1. all L1-L9 packages have their layer-local exit gates closed;
2. lower-layer source guards prevent regression of the architecture boundary;
3. cache and durable-local modes share the same branch/table serving mechanics;
4. durable restart tests cover every restored publication and cleanup window;
5. L9 benchmarks show old-equivalent asymptotic source behavior at 100K, 1M,
   5M, and 10M keys;
6. until engine-next exists, L9 conformance proves the intended engine boundary;
   once engine-next exists, it uses storage-next through L9 without importing
   lower modules;
7. no existing storage-next conformance, fuzz, golden, crash, source-guard, or
   clippy gate has been weakened.
