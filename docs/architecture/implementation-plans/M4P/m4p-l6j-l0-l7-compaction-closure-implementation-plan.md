# M4P-L6J Implementation Plan: LSM Level Compaction Closure

Status: draft follow-on implementation plan

Parent plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l6j-l0-l7-compaction-closure-test-plan.md`

Related L6 test plan:
`docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-test-plan.md`

Benchmark context:
`docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

## Objective

Close the remaining L6 compaction gap before evaluating point-read performance.
After manual flush plus explicit compaction, storage-next must be able to drain
branch-owned table data through the configured LSM levels instead of stopping at
L0 -> L1.

This plan is intentionally narrow:

1. make explicit/manual compaction work from L0 through the last configured
   compaction level;
2. preserve branch isolation, MVCC, tombstone, TTL, inherited-layer, and
   reachability semantics;
3. measure 100K, 1M, 5M, and 10M loads only after the compacted level shape is
   correct;
4. defer automatic scheduling, compaction scoring policy, durable Bloom/filter
   bytes, and point-read accelerator decisions until the level drain is working.

## Current Gap

The current implementation has useful pieces, but not the full manual drain.

1. `BranchCompactionKind::CompactLevel` exists and can plan nonzero-level
   compaction into the next level.
2. `compaction_request_from_maintenance_task` can map a table-level maintenance
   task for level `N > 0` into `CompactLevel { level: N, table_index: 0 }`.
3. The public `StorageRuntime::compaction_maintenance` path still constructs
   only `CompactL0ToLevelOne`.
4. `collect_storage_pressure` only suggests level-zero compaction. That is fine
   for L8 automatic scheduling, but it does not close L6 manual compaction.
5. `plan_nonzero_level_compaction` currently returns `NotEnoughInputTables` for
   a single non-overlapping input table. Old storage handled that as a
   metadata-only move to the next level.
6. `plan_l0_to_l1_compaction` also needs a manual-drain path for a single L0
   table with no L1 overlap, otherwise an explicit drain can leave L0 populated.
7. `BranchRuntimeConfig::default()` currently uses `max_level_count = 7`, which
   means levels L0 through L6. If the parity target is L0 through L7, the first
   implementation step must normalize the configured level count to eight
   levels.

## Layer Ownership

L6 owns this follow-on because it is about branch-local table topology and
manual compaction install shape.

L6 owns:

1. selecting branch-owned LSM sources for explicit compaction;
2. preserving non-overlap invariants in nonzero levels;
3. preserving all-or-nothing branch state installation;
4. moving or rewriting branch-owned table descriptors across levels;
5. reporting source layout after the manual drain.

L6 does not own:

1. automatic maintenance scheduling or background queues;
2. score-based compaction picking under write pressure;
3. durable table-format changes, including durable Bloom/filter blocks;
4. backend object naming, checkpoint policy, or WAL policy;
5. benchmark-only fast paths;
6. public API semantics beyond the existing explicit maintenance operation.

## Implementation Plan

### L6J-A. Normalize Configured Compaction Levels

Goal: make the runtime's configured levels match the intended target before
building a drain algorithm.

Steps:

1. Verify whether the parity target is seven levels (L0 through L6, old
   `NUM_LEVELS = 7`) or eight levels (L0 through L7, current requested target).
2. If the target is L0 through L7, change the storage-next default branch
   runtime config to eight levels.
3. Audit recovery, manifest, source-layout, and branch-state tests that assume
   the old default level count.
4. Keep custom `BranchRuntimeConfig::new(max_level_count, ...)` behavior intact
   for tests that deliberately use fewer levels.
5. Add a level-count assertion in L6 closeout tests so the target cannot drift
   silently.

Exit gate:

1. A default branch reports levels L0 through L7 if that is the accepted target.
2. Explicit small-level-count configs still work and reject compaction past the
   configured last level.

### L6J-B. Add Metadata-Only Level Promotion

Goal: support old-storage trivial moves when a selected table has no overlap in
the next level.

Steps:

1. Extend the branch compaction planner so a single input table with no
   next-level overlap can produce a candidate when the request is part of an
   explicit level compaction or drain.
2. Represent this as a branch-state operation, not as a table rewrite:
   - remove the selected table from its current level;
   - rebuild its `BranchTableDescriptor` at the next level;
   - preserve the same table identity, table facts, reader, and materialization
     source;
   - insert it into the next level in physical-key order.
3. Adjust output-identity validation so reusing the moved table identity is
   allowed only when that identity belongs to the candidate's removed input.
4. Preserve nonzero-level non-overlap validation after the move.
5. Keep pruning disabled for metadata-only promotion because no rows are
   rewritten.

Exit gate:

1. A single non-overlapping L1 table can move to L2.
2. A single non-overlapping table can repeat this movement until the configured
   last level.
3. Last-level compaction remains a no-op.
4. Promotion does not create a duplicate reachable table identity.

### L6J-C. Close L0 Manual Drain

Goal: explicit drain must not strand a single L0 table just because there is no
overlap and no second input table.

Steps:

1. Keep normal L0 behavior for multiple tables: compact all L0 inputs with any
   overlapping L1 tables.
2. Add a single-table L0 path for explicit drain:
   - if the lone L0 table overlaps L1, rewrite it with overlapping L1 tables;
   - if it does not overlap L1, promote it to L1 without rewriting.
3. Preserve L0 ordering semantics: newer L0 tables remain newer until they are
   compacted or promoted.
4. Ensure the branch install phase only removes the exact L0 snapshot inputs so
   concurrently added L0 tables are preserved where that path exists.

Exit gate:

1. Manual drain can reduce L0 table count to zero when no concurrent writes add
   new L0 tables.
2. One L0 table plus empty L1 moves to L1.
3. One L0 table plus overlapping L1 rewrites and preserves latest-visible rows.

### L6J-D. Add Explicit Fixed-Point Branch Drain

Goal: make explicit compaction drive existing level operations until the branch
is stable, without implementing automatic scheduling.

Steps:

1. Add a lifecycle-level drain helper for one branch.
2. Drive levels from lower to higher:
   - compact or promote L0 into L1 while L0 has tables;
   - compact or promote L1 into L2 while L1 has selected work;
   - continue through the penultimate configured level;
   - never compact the configured last level.
3. Repeat passes until a full pass installs no candidate.
4. Add a strict progress guard:
   - every successful operation must remove at least one table from the input
     level or reduce total overlap work;
   - abort with a typed error if the pass limit is reached.
5. Return a drain summary:
   - operations attempted;
   - operations installed;
   - levels touched;
   - tables removed from input levels;
   - output tables installed or promoted;
   - final source layout.
6. Keep single-level queued maintenance working as a separate operation. The
   drain helper must not force automatic queues to compact every level.

Exit gate:

1. Calling explicit branch compaction reaches a fixed point.
2. Repeating explicit compaction immediately after a fixed-point drain is
   idempotent.
3. The fixed-point drain cannot loop indefinitely.

### L6J-E. Wire The Manual Boundary

Goal: make the existing explicit maintenance call useful for closing L6 without
adding new public API surface.

Steps:

1. Change branch-scoped explicit `MaintenanceTask::Compact` to run the
   fixed-point drain.
2. Preserve table-level maintenance tasks as single-level work for L8 queue
   consumers.
3. Keep flush maintenance separate:
   - manual flush writes L0;
   - explicit compact drains L0 through the configured levels;
   - automatic flush/compaction orchestration remains L8.
4. If the temporary flush-follow-up compaction hook remains, limit it to the
   existing L0 pressure behavior and do not treat it as the L6 closeout proof.

Exit gate:

1. The benchmark can run manual flushes and then an explicit compact drain.
2. Explicit compact works in cache and durable runtimes.
3. Queued single-level compaction still maps level `N` to `N -> N+1`.

### L6J-F. Preserve Durable Publication And Reachability

Goal: metadata-only promotion and rewrite compaction must remain safe in durable
mode.

Steps:

1. For rewrite compaction, keep the existing durable rewrite publication path.
2. For metadata-only promotion, update only branch/table-manifest level facts;
   do not republish identical table bytes.
3. Ensure table reachability treats the moved identity as continuously reachable:
   - no orphan debt for promoted tables;
   - no duplicate live reference in old and new levels;
   - manifest publication records the new level.
4. On failure before manifest publication, reads must continue from the old
   level.
5. On failure after branch state change but before durable proof, use existing
   uncertain-debt or recovery path consistently.

Exit gate:

1. Durable promotion survives restart with the table in the promoted level.
2. Durable rewrite compaction still publishes, reopens, installs, and manifests
   output tables before old tables become reclaim candidates.

### L6J-G. Verification And Benchmark Readout

Goal: evaluate point-read performance only after compaction shape is correct.

Steps:

1. Run focused compaction tests before any benchmark:
   - branch compaction unit tests;
   - lifecycle compaction tests;
   - durable compaction publication tests;
   - branch LSM generated tests if touched.
2. Run formatting and compile gates:
   - `cargo fmt --manifest-path crates/storage-next/Cargo.toml`;
   - `cargo check -p strata-storage-next --all-features`;
   - focused `cargo test -p strata-storage-next ...` targets.
3. Run scale benchmarks at 100K, 1M, 5M, and 10M with manual flush plus explicit
   compact drain.
4. Capture source-shape counters before throughput interpretation:
   - final L0 table count;
   - table counts by nonzero level;
   - point owned L0 table probes;
   - point owned nonzero level searches;
   - point owned nonzero table probes;
   - table seeks;
   - rows visited;
   - data-block reads and decodes when `perf-trace` is enabled.
5. Only after source shape is correct, evaluate point-read acceleration gaps:
   - missing durable Bloom/filter bytes;
   - runtime filter attachment;
   - block-cache hit/miss behavior;
   - table count by level;
   - selected data-block reads for negative lookups.

Exit gate:

1. After manual flush plus explicit compact drain, final L0 table count is zero
   for quiescent benchmark loads.
2. Point reads over compacted data do not probe L0.
3. Nonzero point probes are bounded by configured level count, not table count.
4. Read results remain correct before and after compaction.
5. The benchmark report separates compaction-shape failures from Bloom/filter or
   cache acceleration gaps.

## Test Plan

Add focused tests before running scale benchmarks.

Branch-state tests:

1. L0 single-table no-overlap promotion to L1.
2. L0 single-table overlap rewrite with L1.
3. L0 multi-table rewrite into L1.
4. L1 single-table no-overlap promotion to L2.
5. L1 overlap rewrite with L2.
6. Repeated promotion reaches the configured last level.
7. Last-level compaction returns no candidate.
8. Promotion preserves table identity, facts, rows, and materialization source.
9. Promotion rejects duplicate reachable identity outside the removed input.
10. Nonzero levels remain sorted and non-overlapping after every install.

Lifecycle/API tests:

1. Branch-scoped explicit compact drains all levels to fixed point.
2. Repeated explicit compact after fixed point is idempotent.
3. Table-level queued compact still performs one level operation.
4. Drain progress guard fails safely if a candidate does not change topology.
5. Cache and durable runtimes report equivalent final source layout.

Durable tests:

1. Metadata-only promotion updates manifest level facts without rewriting table
   bytes.
2. Promoted table survives restart at the new level.
3. Rewrite compaction still publishes new output before old inputs become
   reclaim candidates.
4. Simulated manifest failure leaves reads unchanged or records existing durable
   debt consistently.

Benchmark gates:

1. 100K smoke: source layout shows compacted data and no L0 backlog.
2. 1M scale: point-read counters remain level-count bounded.
3. 5M scale: compaction drain finishes without unbounded pass count or memory
   growth.
4. 10M scale: throughput is interpreted only after source-shape counters pass.

## Stop Conditions

Stop implementation and re-plan if any of these appear:

1. The configured level target remains ambiguous between L0-L6 and L0-L7.
2. Single-table promotion requires table byte rewrites in durable mode without a
   clear correctness reason.
3. Explicit compact still only performs L0 -> L1.
4. Manual drain can leave a quiescent branch with L0 tables.
5. A promoted table identity is reachable from two levels.
6. Durable promotion requires a table-format change.
7. Benchmarks show table-count-scaled point probes after the final source layout
   is compacted.

## Follow-On After This Plan

Once L6J exits:

1. run the 100K, 1M, 5M, and 10M benchmark set with manual flush and explicit
   compaction drain;
2. classify remaining point-read performance gaps as:
   - durable Bloom/filter bytes;
   - runtime Bloom/filter attachment;
   - block-cache behavior;
   - table/data-block size tuning;
   - automatic scheduling and pressure policy;
3. open the next plan only for the measured bottleneck.
