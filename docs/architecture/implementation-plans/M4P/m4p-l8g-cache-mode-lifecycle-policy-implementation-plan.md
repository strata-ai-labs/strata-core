# M4P-L8G Implementation Plan: Cache Mode Lifecycle Policy

Status: draft

Parent implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-implementation-plan.md`

Predecessor plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8e-background-maintenance-executor-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-implementation-plan.md`

Follow-up test plan:
`docs/architecture/implementation-plans/M4P/m4p-l8g-cache-mode-lifecycle-policy-test-plan.md`

## Objective

Cleanly restore the `StorageMode::Cache` contract without adding a second
storage engine or a benchmark-only bypass.

The storage-next cache runtime currently satisfies the narrow no-WAL invariant:
cache load results show zero WAL record build, zero WAL append, zero WAL
retention samples, and zero checkpoint executions. That is necessary for
`wasm32-none-none`, but it is not sufficient.

The stronger cache-mode invariant is:

1. cache mode must not perform durable/source-table lifecycle work during
   ordinary writes;
2. cache mode must not depend on background scheduling for correctness;
3. cache mode must not throttle writes because of L0/nonzero/table-source
   shape pressure;
4. cache mode must preserve the shared commit, branch, conflict, timestamp,
   and read semantics that motivated the current shared runtime shape.

This slice is a policy cleanup, not a new volatile path. The implementation
must understand which parts of the current cache lifecycle are correctness
requirements and which parts are accidental inheritance from durable/table
storage policy. Shared mechanics stay shared. Mode-specific lifecycle policy
becomes explicit.

## Corrected Diagnosis

The load-performance stabilization slice correctly exposed the counters but
started from an incomplete diagnosis. The 10M cache run was not slow because
cache mode was doing WAL work. It was slow because cache mode was maintaining
durable-style source/table shape during the timed load:

1. cache open creates `LifecycleCacheRuntime`;
2. cache runtime is wrapped in a background-capable runtime slot;
3. cache background maintenance drains flush and table rewrite work;
4. cache admission observes table-source pressure and slows or waits;
5. cache compaction rewrites rows even though cache mode has no durable source
   shape to preserve.

The old engine cache benchmark is materially different: it opens an ephemeral
cache database and applies batched blind writes in memory. It is not a proof
that old compaction is better; it is evidence that cache mode should not carry
durable source-shape maintenance into the write hot path.

## Current Evidence

Storage-next cache 10M load facts from the corrected benchmark run:

| Fact | Value |
| --- | ---: |
| Throughput | 65,741 ops/s |
| Elapsed | 152.11s |
| WAL records built | 0 |
| WAL appends | 0 |
| Checkpoint executions | 0 |
| Admission slowdown | 57.42s |
| Admission block wait | 35.27s |
| Automatic maintenance | 137.21s |
| Background maintenance tasks | 2,332 |
| Completed compactions | 308 |
| Compaction input rows | 41,656,820 |
| Compaction input bytes | 11,863,316,915 |
| Final L0 tables | 227 |

Old engine cache 10M load in the same environment:

| Fact | Value |
| --- | ---: |
| Throughput | 257,158 ops/s |
| Elapsed | 38.89s |

The important conclusion is not that storage-next needs more compaction tuning
for cache. It is that cache mode should not be under table-source pressure in
the first place unless a specific cache correctness or memory-bound invariant
requires it.

## Why The Current Shape Exists

Do not assume the current implementation is arbitrary. It likely exists because
it reuses valuable shared mechanics:

1. branch catalog and branch-generation safety;
2. commit fact allocation and timestamp ordering;
3. conflict validation and read-set semantics;
4. latest/history/timestamp read behavior;
5. lifecycle state-machine coverage used by durable work;
6. table-source tests that were easier to exercise without WAL/recovery.

The cleanup must preserve those benefits. It must remove accidental durable
policy from cache mode, not fork the engine into unrelated cache and durable
implementations.

## Required Invariants

1. `StorageMode::Cache` never builds or appends WAL records.
2. `StorageMode::Cache` never schedules checkpoint, WAL truncation, table
   manifest publication, durable retention, quarantine, or durable purge work.
3. Ordinary cache writes do not enqueue flush, table rewrite, compaction, or
   materialization tasks solely to maintain source/table shape.
4. Ordinary cache writes do not enter admission slowdown or block wait because
   of L0 table count, nonzero level bytes, table fanout, WAL retention, or
   checkpoint debt.
5. Cache mode can run without a background executor, worker thread, condvar,
   parking primitive, or wall-clock wait.
6. Cache mode retains shared commit and read semantics:
   - point reads;
   - range scans;
   - latest-value precedence;
   - history reads;
   - timestamp reads;
   - conflict validation;
   - branch generation safety.
7. Durable modes keep the existing lifecycle machinery unless a test proves a
   durable regression or a separate durable cleanup slice owns the change.
8. Any cache pressure policy must be memory-budget oriented, not table-shape
   oriented.
9. No benchmark-specific fast path, scale check, retry loop, or final-drain
   shortcut is allowed.
10. No second public storage engine path is introduced.

## Scope Summary

| Group | Required Work | Exit Gate |
| --- | --- | --- |
| A. Cache Lifecycle Audit | Trace every cache path that creates source/table lifecycle work and classify it as correctness, memory-bound, test scaffolding, or accidental durable policy. | A policy table names each cache lifecycle operation and its required disposition. |
| B. Policy Boundary | Separate lifecycle mechanics from per-mode lifecycle policy. | Cache and durable share mechanics, but cache policy cannot schedule durable/source-table maintenance. |
| C. Cache Admission Policy | Remove table-source pressure from cache write admission. | Cache writes do not slowdown or block on L0/nonzero/table/WAL/checkpoint facts. |
| D. Cache Maintenance Scheduling | Stop ordinary cache writes from enqueuing flush/rewrite/compaction work. | Cache load counters show zero background tasks and zero table maintenance. |
| E. Shared Read Correctness | Preserve shared commit/read semantics without source-table maintenance. | Point, scan, history, timestamp, conflict, and branch tests pass in cache mode. |
| F. Closeout Benchmarks | Re-run cache and old-engine load benchmarks after the policy cleanup. | Cache mode behaves as volatile storage, not table-managed durable storage without WAL. |

## Implementation Order

Execute in this order. Do not remove scheduling calls until the audit has
identified whether they protect a real correctness invariant.

1. **Audit and counters first**
   - Add or confirm counters that distinguish:
     - WAL work;
     - checkpoint/WAL-retention work;
     - flush work;
     - table rewrite/compaction work;
     - materialization work;
     - admission slowdown/block wait;
     - background executor task execution.
   - Add temporary or permanent diagnostic assertions that can prove cache load
     is paying source-table lifecycle costs even when WAL counters are zero.
   - Document every cache lifecycle call site and its current reason.
2. **Policy model second**
   - Add an explicit lifecycle policy object or equivalent mode-owned decision
     point.
   - Keep lifecycle mechanics shared.
   - Make cache policy reject source-shape maintenance as a default, not by
     sprinkling `if cache` checks through compaction internals.
   - Make durable policy keep the existing flush/rewrite/checkpoint/retention
     decisions.
3. **Admission cleanup third**
   - Ensure cache write admission ignores table-source pressure classes.
   - Keep typed pressure for true memory-budget exhaustion if such a budget
     exists.
   - Preserve durable admission behavior.
   - Keep all waits behind the injected maintenance clock where waits remain
     valid.
4. **Maintenance scheduling cleanup fourth**
   - Stop post-commit cache scheduling from enqueuing flush or table rewrite
     work for ordinary writes.
   - Stop cache background drain from being required for ordinary write/read
     correctness.
   - Keep explicit test-only helpers for source/table fixtures if they are still
     needed, but do not let them be product cache policy.
5. **Read correctness fifth**
   - Prove cache reads remain correct from active/in-memory branch state:
     point, scan, history, timestamp, latest precedence, and conflict paths.
   - If a read currently depends on flush/compaction for correctness, refactor
     the shared read path so the invariant is represented directly instead of
     relying on source-table maintenance.
6. **Benchmark closeout last**
   - Run storage-next cache 100K, 1M, 5M, and 10M load sequentially with source
     diagnostics.
   - Run old-engine cache 100K, 1M, 5M, and 10M in the same environment.
   - Close only if cache-mode lifecycle counters prove the mode is volatile and
     correctness tests pass.

## A. Cache Lifecycle Audit

Goal: produce a source-owned explanation of why cache currently flushes,
rewrites, compacts, schedules, and throttles.

Required audit table columns:

1. call site;
2. lifecycle operation kind;
3. current trigger;
4. correctness dependency, if any;
5. memory-bound dependency, if any;
6. durable-only policy inherited by accident;
7. proposed cache disposition;
8. proposed durable disposition;
9. test proving the disposition.

Minimum call sites to audit:

1. cache open and runtime-slot construction;
2. post-commit maintenance suggestion and enqueue;
3. cache write admission pressure collection;
4. cache background drain;
5. cache flush maintenance;
6. cache table rewrite and compaction maintenance;
7. cache diagnostics that observe source/table shape;
8. explicit test helpers that create source/table fixtures.

Exit gates:

1. The plan records why each cache lifecycle operation exists today.
2. No code removal occurs before the operation has a disposition.
3. The audit distinguishes product cache policy from test fixture helpers.

## B. Policy Boundary

Goal: make mode-specific lifecycle policy explicit without duplicating the
engine.

Implementation tasks:

1. Introduce a small lifecycle policy surface that answers:
   - may this mode schedule post-commit maintenance?
   - may this mode apply source-shape admission pressure?
   - may this mode run background maintenance?
   - may this mode flush active/frozen state to table sources?
   - may this mode rewrite or compact tables?
   - may this mode checkpoint or truncate WAL?
2. Wire cache and durable modes through that policy surface.
3. Move durable-only decisions out of generic cache/durable shared drive code
   where possible.
4. Keep table algorithms reusable for durable mode and explicit table-fixture
   tests.
5. Keep public cache open behavior simple: cache is volatile and does not
   require durable lifecycle services.

Exit gates:

1. A source guard can identify the cache policy and durable policy.
2. Cache policy denies source-table lifecycle work by default.
3. Durable policy still permits the existing durable lifecycle operations.
4. The policy surface does not mention benchmark scale, workload name, or
   command-line flags.

## C. Cache Admission Policy

Goal: cache writes are not slowed by durable source-shape pressure.

Implementation tasks:

1. Classify pressure reasons into:
   - durable/source-shape pressure;
   - WAL/checkpoint pressure;
   - memory-budget pressure;
   - runtime-shutdown or panic pressure.
2. For cache mode:
   - ignore durable/source-shape pressure;
   - ignore WAL/checkpoint pressure;
   - preserve shutdown/panic protection;
   - preserve memory-budget pressure if implemented.
3. For durable mode:
   - preserve current source-shape and WAL pressure behavior.
4. Ensure cache counters make ignored durable/source-shape pressure visible
   during transition, then remove or zero them once policy is clean.

Exit gates:

1. Cache load records zero admission slowdown from source-shape pressure.
2. Cache load records zero block waits from source-shape pressure.
3. Durable pressure tests continue to pass.
4. Deterministic inline executor tests do not need real time for cache mode.

## D. Cache Maintenance Scheduling

Goal: ordinary cache writes do not require flush, table rewrite, compaction, or
background scheduling.

Implementation tasks:

1. Stop ordinary cache post-commit paths from enqueuing flush/table-rewrite
   tasks.
2. Stop cache background drain from running source-table maintenance as product
   policy.
3. Keep explicit manual maintenance helpers only where needed for tests that
   intentionally exercise table mechanics.
4. Ensure cache close does not attempt durable source-shape cleanup.
5. Ensure diagnostics report cache volatile shape accurately rather than
   treating absent table shape as an error.

Exit gates:

1. Cache load records zero background maintenance tasks.
2. Cache load records zero flush maintenance tasks.
3. Cache load records zero compaction/table-rewrite operations.
4. Cache diagnostics distinguish volatile in-memory shape from unknown table
   shape.

## E. Shared Read Correctness

Goal: removing accidental cache source-table maintenance does not create a
second read engine or weaken semantics.

Implementation tasks:

1. Audit read paths that currently consult active, frozen, L0, nonzero, and
   inherited sources.
2. Ensure active/in-memory cache state carries enough facts for:
   - latest reads;
   - scans;
   - history reads;
   - timestamp reads;
   - conflict validation;
   - branch generation checks.
3. If a read requires a table source for correctness, refactor the shared
   source abstraction so volatile sources and table sources implement the same
   semantic contract.
4. Keep table-source code available for durable mode.
5. Keep branch/fork semantics consistent between cache and durable modes.

Exit gates:

1. Cache read tests pass without flushing.
2. Cache conflict-validation tests pass without source-table materialization.
3. Durable read tests still pass with table sources.
4. No public API behavior differs except performance/diagnostics that reflect
   cache volatility.

## F. Benchmark Closeout

Required commands:

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 100k \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 1m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 5m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-next-l9-scale -- \
  --scales 10m \
  --engines cache \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000 \
  --diagnostic-source-shape
```

Repeat the same scales with `storage-old-cache-scale` for same-machine
comparison.

Hard gates:

1. Cache load completes at 100K, 1M, 5M, and 10M.
2. Cache load records zero WAL records and zero WAL appends.
3. Cache load records zero checkpoint executions and zero WAL retention
   samples.
4. Cache load records zero source-table maintenance tasks from ordinary
   writes.
5. Cache load records zero compaction input rows and bytes.
6. Cache load records zero source-shape admission slowdown and zero source-shape
   block wait.
7. Cache read correctness tests pass after large unflushed loads.
8. Durable lifecycle tests pass unchanged.
9. Storage-next cache throughput is compared to old-engine cache only after the
   above policy counters are clean.

Soft targets:

1. 10M storage-next cache throughput should be within 2x of old-engine cache in
   the same environment.
2. If throughput remains below that target with zero lifecycle maintenance,
   open a commit/read hot-path slice instead of changing cache lifecycle policy.

## Stop Conditions

1. If a cache read correctness test requires flush or compaction to pass, stop
   and document the semantic invariant before removing that maintenance.
2. If cache memory growth becomes unbounded after source-table maintenance is
   disabled, stop and define a memory-budget pressure policy. Do not reintroduce
   L0/nonzero/table-shape pressure as a proxy.
3. If durable lifecycle behavior changes, stop and split the durable regression
   into a separate fix before continuing cache cleanup.
4. If benchmark throughput improves only when a scale-specific condition is
   added, reject the patch.
5. If the cleanup requires a second public cache engine path, stop and revisit
   the policy/mechanics boundary.

## Non-Goals

1. No new public storage mode.
2. No second storage engine or duplicate commit/read implementation.
3. No benchmark retry loop.
4. No scale-specific fast path.
5. No weakening durable-local storage semantics.
6. No changing table format.
7. No changing conflict semantics.
8. No removing table-source tests that are still needed for durable mode.
9. No adding thread, condvar, wall-clock, or local filesystem requirements to
   cache mode.
