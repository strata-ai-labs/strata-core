# M4P Test Plan: Storage-Next Parity Restoration

Status: draft test methodology

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

## Goal

Give the parity-restoration work the same testing rigor as the existing
storage-next program.

The test goal is not only "all tests pass." The suite must prove that restored
mechanics:

1. preserve old-storage behavior;
2. preserve storage-next L1-L9 boundaries;
3. preserve durable crash/recovery guarantees;
4. preserve cache/durable/wasm mode contracts;
5. restore old-equivalent asymptotic source behavior.

## Testing Principles

1. Every implementation slice ships with tests in the same change set.
2. Every layer owns tests for its own mechanics. Upper layers may test the
   integration, but they must not be the only proof.
3. Every performance fix needs mechanical counters, not just wall-clock
   throughput.
4. Every durable service change needs a fault-window or crash/restart test.
5. Every durable format change needs goldens and fuzz coverage before behavior
   depends on the new bytes.
6. Every source-boundary claim needs an executable source guard.
7. Generated and fuzz tests must compare against independent models whenever
   practical, not production-derived expectations.
8. Benchmarks must run through L9 and must not shape production algorithms.
9. Cache and durable modes must be compared after persistence overhead is
   separated from branch/table serving behavior.
10. Tests must use opaque storage bytes. Product DTOs stay above storage.

## Proof Ladder

Each package advances through this ladder:

1. **Source-map proof:** document old evidence files and storage-next target
   files before editing behavior.
2. **Audit proof:** cite the exact audit file and section heading for each
   closed or deferred finding, including the required old evidence files,
   storage-next target files, and proof or counter expectations.
3. **Boundary proof:** source guards prevent imports, direct IO, product DTOs,
   roadmap labels in production code, and lower-layer bypasses.
4. **Unit proof:** local constructors, validators, and error classifications
   cover edge cases.
5. **Model proof:** independent model tests cover stateful behavior.
6. **Property proof:** generated operation scripts exercise broad valid input.
7. **Fuzz proof:** fuzz targets cover decoders, state machines, and generated
   scripts where failures are meaningful.
8. **Fault proof:** backend/service/maintenance fault windows classify partial
   failures without silent corruption.
9. **Crash proof:** durable restart harness proves convergence or typed
   failure after partial publication.
10. **Mode proof:** cache, durable-local standard, durable-local always, and
   wasm-none-supported subsets behave as documented.
11. **Performance proof:** counters and benchmarks show old-equivalent
    asymptotic work through L9.

## Layer Test Matrix

| Layer | Required test families | Key failure the tests must catch |
| --- | --- | --- |
| L1 Backend IO | Backend conformance, localfs fault injection, writer-lock tests, durable-delete tests, source guard for direct filesystem IO. | Higher layers reintroduce direct filesystem operations or cleanup durability becomes unclassifiable. |
| L2 Object Layout | Object-name validation, prefix/family property tests, reserved-name tests, source guard for ad-hoc object-name construction. | A service publishes or deletes an object family outside validated layout rules. |
| L3 Format / Codec | Golden vectors, roundtrip tests, mismatch tests, decoder fuzz targets, corruption tests. | Durable bytes drift silently or a decoder accepts corrupt input. |
| L4 Durable Services | WAL/manifest/snapshot/table/quarantine service tests, publish fault windows, crash/restart harness, retention proof tests. | A partial publish loses committed rows, resurrects stale rows, or deletes reachable objects. |
| L5 Table Runtime | Table seek tests, cursor tests, block-cache tests, compaction model tests, table reader fuzz/properties. | Point lookup or scan work scales with unrelated rows or table count incorrectly. |
| L6 Branch LSM | Independent row-chain model, branch operation scripts, inheritance/materialization fuzz, source-class perf counters. | Branch reads, scans, history, fork gates, shadowing, or materialization diverge from old mechanics. |
| L7 Commit Runtime | Conflict/CAS/read-set tests, timeline tests, durable gate tests, replay tests, generation/quiesce tests, source guards, perf counters, and runtime-default durability mapping tests. | Commits become visible out of order, miss conflicts, classify durable ambiguity incorrectly, or accidentally absorb lower-layer source planning / maintenance scheduling. |
| L8 Lifecycle | Maintenance executor tests, pressure/backpressure tests, budget reservation tests, recovery/close/quarantine crash tests, soak scripts. | Normal writes strand unbounded backlog or recovery/close accepts unsafe state. |
| L9 API Boundary | API conformance, mode contracts, read-set API tests, diagnostics tests, dependency guards, L9 benchmarks. | Engine-facing storage APIs cannot express restored mechanics or upper crates bypass L9. |

## Differential Testing

Differential tests compare storage-next against old storage where the old
behavior is still executable.

Required workloads:

1. blind puts and deletes;
2. put/delete/put resurrection;
3. latest reads;
4. version-bounded reads;
5. timestamp-bounded reads where old behavior is comparable;
6. history reads;
7. prefix and range scans;
8. branch fork and child-local shadowing;
9. materialization;
10. compaction after sustained load;
11. restart after commit, flush, compaction, checkpoint, branch clear/delete,
    quarantine, and purge.

Comparison rules:

1. Compare visible rows and ordering first.
2. Compare retained history only where both engines document the same
   retention/TTL behavior.
3. Compare source-shape counters for performance-sensitive paths.
4. Compare wall-clock throughput only after source-shape counters are
   explainable.
5. Record deliberate semantic differences in the implementation plan before
   allowing a test to skip them.
6. Link every skipped, reinterpreted, or newly asserted behavior to the audit
   finding or semantic decision that owns it.

## Semantic Decision Register

Some old-storage behavior is evidence, not a required storage-next V1 target.
Before a differential test may skip or reinterpret one of these areas, the
owning slice must record the decision, owner layer, reason, and replacement
proof:

1. old durable artifact compatibility, including old table, manifest, WAL, and
   snapshot bytes;
2. TTL, tombstone, retained-history, and timestamp-bounded read differences;
3. fork ergonomics when active or frozen rows exist in the source branch;
4. global versus independent-branch commit admission;
5. public read-set facts versus product transaction sessions;
6. wasm-none-supported subset and unsupported durable-local behavior;
7. cache, durable-local standard, and durable-local always mode defaults;
8. object-durable and distributed candidate modes;
9. engine-next dependency guards before the engine-next crate exists;
10. copied `Materializing` inherited-layer status behavior;
11. table-object reference recovery versus manifest/catalog replacement;
12. pool-based budget model versus old unified memory-budget derivation;
13. checkpoint extension payload API need and ownership.

The test harness should treat an unrecorded semantic difference as a failure,
not as a reason to weaken the oracle.

## Performance Testing Methodology

Performance runs must be serial, not simultaneous, unless the run is explicitly
testing interference. Running old and new engines at the same time can distort
CPU cache, memory bandwidth, IO, and scheduler behavior.

Every benchmark result must include:

1. git revision;
2. machine and target architecture;
3. build profile;
4. engine name;
5. storage mode;
6. durability policy;
7. backend kind;
8. localfs feature state;
9. budget policy and resolved budgets;
10. key count;
11. value size;
12. scan sample count and scan limit;
13. flush/maintenance policy;
14. perf-trace enabled state.

Required derived metrics:

1. `point_source_probes_per_read`;
2. `point_nonzero_table_probes_per_read`;
3. `scan_source_cursors_per_call`;
4. `scan_table_cursors_opened_per_call`;
5. `scan_rows_visited_per_row_returned`;
6. `l0_tables_per_million_rows_after_load`;
7. `compaction_tasks_per_flush_task`;
8. `load_maintenance_ms_per_million_rows`;
9. old-to-new throughput ratio for load, point latest, point throughput, scan
   prefix, and scan range.

Required scales:

1. 100K keys for fast local validation.
2. 1M keys for early scale shape.
3. 5M keys for maintenance and compaction pressure.
4. 10M keys for parity gate.
5. 50M and 100M keys only after 10M source-shape counters are clean.

Fail-fast performance invariants:

1. After maintenance drain, L0 table count must not scale linearly with total
   row count.
2. Point reads over nonzero levels must probe at most one table per nonzero
   level.
3. Scan setup over nonzero levels must create lazy level cursors rather than
   one eager cursor per table.
4. History for a single key must not scan unrelated physical keys.
5. Cache and durable modes must have identical branch-local source layout for
   the same workload after durable persistence facts are ignored.

## Mode Testing

Cache mode:

1. Opens explicitly.
2. Does not claim crash durability.
3. Uses the same branch/table serving mechanics as durable mode.
4. Supports cache maintenance needed to bound source fanout.

Durable-local standard:

1. Opens through `StorageRuntime::open_local(root)`.
2. Does not fall back to cache.
3. Reports durable capability and recovery facts.
4. Persists committed rows across restart.

Durable-local always:

1. Requires explicit policy.
2. Rejects weaker commit durability requests where documented.
3. Reports sync/close facts.
4. Handles ambiguous durability with typed outcomes.

Wasm-none-supported subset:

1. Builds with `default-features = false` where applicable.
2. Cache mode opens.
3. Durable-local returns unsupported capability.
4. No localfs behavior is assumed.

## Source Guards

Required guards:

1. Production storage-next code outside `backend/local_fs.rs` does not use
   direct filesystem APIs.
2. L3 format code does not perform IO.
3. L5 table code does not import branch, commit, lifecycle, API, or engine
   modules.
4. L6 branch code does not import commit, lifecycle, API, backend, filesystem,
   or product DTOs.
5. L7 commit code does not expose public transaction sessions or product DTOs.
6. L8 lifecycle code does not import product primitives or public UX wording.
7. L9 API code does not expose WAL records, table readers, object constructors,
   JSON paths, graph/vector/search/event DTOs, IPC, inference, intelligence, or
   StrataHub concepts.
8. Production code avoids architecture roadmap labels in names and comments.
9. Once engine-next exists, normal production crates above it do not depend
   directly on `strata-storage-next`. Until then, source guards must still prove
   storage-next public APIs do not leak lower-layer implementation types, and new
   production crates must not add direct storage-next dependencies without a
   recorded implementation-plan exception.

## Fuzz And Generated Testing

Required targets or equivalent generated suites:

1. L3 format decoders.
2. L4 service publish/recovery scripts.
3. L5 table cursor and compaction scripts.
4. L6 branch read/inheritance/materialization scripts.
5. L7 commit validation/replay/durable-gate scripts.
6. L8 lifecycle maintenance/recovery/close scripts.
7. L9 API operation scripts covering open, commit, read, scan, branch,
   maintenance, diagnostics, and close.

Every generated script suite must have:

1. bounded operation count;
2. deterministic seed recording;
3. independent expected-result model or explicit oracle;
4. corpus seeds for known audit gaps;
5. regression file updates only when a real failing seed is captured.

## Closeout Requirements

Each package closes only when:

1. source-map evidence is recorded;
2. cited audit findings are closed by tests, counters, or explicit deferrals
   with owner layer and reason;
3. layer-local source guards pass;
4. unit/model/property tests pass;
5. fuzz or generated tests cover the stateful surface;
6. durable fault/crash tests pass when the layer touches durable services;
7. mode tests pass when the layer affects open, commit, read, maintenance, or
   recovery behavior;
8. performance counters prove the intended mechanical shape for
   performance-sensitive paths;
9. relevant package tests pass, including `cargo test -p strata-storage-next`
   for storage-next changes and feature-specific tests for touched feature
   gates;
10. `cargo clippy -p strata-storage-next --lib --features perf-trace -- -D warnings`
   passes if the package touches storage-next code;
11. L9 benchmarks are rerun when the package changes serving path, maintenance,
   or API behavior;
12. deferred items are listed with owner layer and reason.
