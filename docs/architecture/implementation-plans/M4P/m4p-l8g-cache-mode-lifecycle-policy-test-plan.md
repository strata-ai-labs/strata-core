# M4P-L8G Test Plan: Cache Mode Lifecycle Policy

Status: draft

Implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l8g-cache-mode-lifecycle-policy-implementation-plan.md`

Parent test plans:

1. `docs/architecture/implementation-plans/M4P/m4p-l8-lifecycle-maintenance-parity-test-plan.md`
2. `docs/architecture/implementation-plans/M4P/m4p-l8f-load-performance-stabilization-test-plan.md`

## Goal

Prove that cache mode is volatile storage implemented through shared
storage-next mechanics, not durable table-managed storage with WAL disabled.

The suite must fail if cache mode:

1. builds or appends WAL records;
2. checkpoints, truncates WAL, or publishes durable table manifests;
3. schedules flush, table rewrite, compaction, or materialization work for
   ordinary writes;
4. slows or blocks writes because of source/table shape pressure;
5. requires background worker progress for read/write correctness;
6. preserves performance by adding a benchmark-only bypass or a separate
   storage engine path.

## Test Matrix

| Area | Required Proof | Failure Caught |
| --- | --- | --- |
| Cache absence counters | Cache load records zero WAL, checkpoint, flush, rewrite, compaction, and background-task work. | Cache still inherits durable lifecycle work. |
| Policy boundary | Cache and durable modes make lifecycle decisions through an explicit policy surface. | Ad hoc `if cache` checks or hidden durable behavior in shared code. |
| Admission | Cache writes ignore source-shape pressure and preserve true runtime/memory failures. | L0/nonzero/table pressure still throttles cache writes. |
| Scheduling | Ordinary cache commits do not enqueue source-table maintenance. | Background work remains required for cache progress. |
| Read correctness | Point, scan, history, timestamp, and conflict semantics pass without flush/compaction. | Table maintenance was masking a read-path correctness dependency. |
| Durable regression | Durable lifecycle tests still schedule and complete maintenance. | Cache cleanup weakens durable-local behavior. |
| Capability/wasm boundary | Cache mode requires no append, sync, durable publish, writer lock, thread, or wall-clock wait. | Cache cannot support browser/wasm deployment. |
| Benchmark closeout | 100K-10M cache runs complete with clean volatile counters. | The performance result is still explained by lifecycle maintenance. |

## Cache Absence Tests

Correctness tests:

1. A cache runtime opened with default options records:
   - storage mode `Cache`;
   - no durable policy;
   - no WAL append capability requirement;
   - no durable sync capability requirement;
   - no single-writer lock capability requirement.
2. A cache load of several batches records zero:
   - `commit_wal_records_built`;
   - `commit_wal_appends`;
   - `commit_wal_append_bytes`;
   - `lifecycle_checkpoint_executions`;
   - `lifecycle_wal_retention_samples`;
   - `lifecycle_wal_checkpoint_enqueue_events`;
   - `lifecycle_wal_truncation_deleted_segments`.
3. The same load records zero:
   - post-commit maintenance tasks enqueued for source-table work;
   - background maintenance tasks;
   - flush maintenance completions;
   - table rewrite completions;
   - compaction operations;
   - compaction input rows and bytes.
4. Cache close does not perform durable finalization, checkpoint, WAL
   truncation, table-manifest publication, or source-table drain.
5. Cache diagnostics report volatile in-memory shape explicitly instead of
   treating absent table shape as unknown durable shape.

Source guards:

1. Cache mode requirements must not include `AppendObject`, `DurableSync`,
   `DurablePublish`, or `SingleWriterLock`.
2. Cache open must not require a background worker thread for correctness.
3. Cache ordinary commit paths must not enqueue flush, table rewrite,
   compaction, checkpoint, or WAL truncation tasks.
4. Cache admission paths must not branch on L0 table count, nonzero level bytes,
   WAL retained bytes, checkpoint debt, or final-level table fanout.
5. Benchmark load loops must not call explicit compact, final drain, or retry
   failed commits.

Pass gates:

1. All absence counters remain zero after a cache load.
2. The source guards are mechanical and do not rely on benchmark output.

## Policy Boundary Tests

Correctness tests:

1. Cache policy returns false for:
   - post-commit source-table maintenance scheduling;
   - source-shape admission pressure;
   - background source-table maintenance;
   - flush to table source;
   - table rewrite or compaction;
   - checkpoint and WAL truncation.
2. Durable-local standard policy returns true for durable lifecycle operations
   that currently belong to durable mode.
3. Durable-local always policy preserves the stronger durable commit behavior.
4. Unsupported/object/distributed candidate modes continue to fail at their
   existing validation boundaries.
5. The policy surface takes mode/config facts, not benchmark scale or workload
   names.

Generated tests:

1. Random policy queries across all lifecycle operation kinds.
2. Random storage mode requests and backend capability sets.
3. Random maintenance task kinds to verify cache denial and durable ownership.

Pass gates:

1. Cache denial is centralized in the policy surface.
2. Durable behavior remains observable through the existing durable lifecycle
   tests.

## Admission Tests

Correctness tests:

1. A synthetic L0 urgent pressure snapshot does not slow cache writes.
2. A synthetic nonzero level byte pressure snapshot does not slow cache writes.
3. A synthetic final-level table fanout pressure snapshot does not slow cache
   writes.
4. A synthetic WAL-retention pressure snapshot does not slow cache writes.
5. Cache still rejects commits after runtime close.
6. Cache still reports injected panic/shutdown health failures.
7. If memory-budget pressure exists, cache reports it as memory pressure, not
   source-shape pressure.
8. Durable mode continues to slow or wait for the same source-shape pressure
   fixtures that require durable relief.

Manual-clock tests:

1. Cache source-shape pressure scripts do not advance the clock through
   admission sleeps.
2. Durable block-wait scripts still use the injected clock and deadline.

Pass gates:

1. Cache source-shape pressure produces zero slowdown nanoseconds.
2. Cache source-shape pressure produces zero block-wait nanoseconds.
3. Durable pressure behavior is unchanged.

## Scheduling Tests

Correctness tests:

1. Ordinary cache commit does not enqueue maintenance after:
   - one batch;
   - many batches;
   - large values;
   - many key ranges.
2. Cache `wait_background_idle` returns immediately or reports no background
   requirement for ordinary writes.
3. Explicit test-only table fixture helpers can still create table sources when
   a table-specific unit test asks for them.
4. Durable ordinary commit still enqueues required lifecycle work according to
   durable policy.

Counter tests:

1. Cache maintenance queue pending/active/completed counts remain zero after
   ordinary writes.
2. Durable maintenance queue counters continue to move in durable fixtures.

Pass gates:

1. Cache correctness does not require calling background drain.
2. Table-specific test helpers are visibly separate from product cache policy.

## Read Correctness Tests

Run every test without explicit flush, compaction, table rewrite, final drain,
or background worker progress.

Required cache tests:

1. latest point reads after single and repeated puts;
2. latest point reads after deletes/tombstones;
3. range scans across multiple batches;
4. reverse or bounded scans if supported by the existing read API;
5. history reads across multiple versions of the same key;
6. timestamp reads before, at, and after write timestamps;
7. conflict validation for blind writes and checked writes;
8. branch fork/read behavior if cache mode exposes branch operations;
9. branch generation rejection for stale branch handles;
10. read-after-large-load samples from early, middle, and late key ranges.

Comparison tests:

1. Cache read results before and after an explicit test-only flush are
   identical.
2. Cache read results before and after explicit test-only compaction are
   identical.
3. Durable read results remain identical before and after durable maintenance
   where existing tests require that proof.

Generated tests:

1. Random put/delete sequences over a small key space.
2. Random timestamps and history lookups.
3. Random scan bounds.
4. Random branch generation stale-handle attempts.

Pass gates:

1. No cache read correctness test needs source-table maintenance.
2. If a test fails only without flush/compaction, the implementation must stop
   and document the missing shared source invariant.

## Capability And Wasm Boundary Tests

Correctness tests:

1. Cache opens with a backend that supports only cache-mode object operations.
2. Cache rejects durable commit requests with the existing typed unsupported
   capability error.
3. Cache does not call backend append or sync methods in ordinary writes.
4. Cache does not call durable publish or writer lock methods in ordinary
   writes.
5. Cache-mode tests compile without local filesystem feature assumptions.

Source guards:

1. Cache policy and ordinary cache runtime code do not import thread, condvar,
   parking, or wall-clock wait types for correctness.
2. Cache policy does not require local filesystem APIs.

Pass gates:

1. Browser-like backend capability tests pass.
2. Durable-only capabilities remain absent from cache requirements.

## Benchmark Gates

Run storage-next cache benchmarks one scale at a time:

```text
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

Run old-engine cache benchmarks one scale at a time in the same environment:

```text
cargo run --release --manifest-path benchmarks/Cargo.toml \
  --bin storage-old-cache-scale -- \
  --scales 100k \
  --workloads load-seq \
  --value-bytes 150 \
  --batch-size 1000 \
  --samples 1000
```

Repeat for `1m`, `5m`, and `10m`.

Hard gates:

1. Storage-next cache completes every scale.
2. WAL counters remain zero at every scale.
3. Checkpoint and WAL-retention counters remain zero at every scale.
4. Source-table maintenance counters remain zero at every scale.
5. Source-shape admission slowdown and block wait remain zero at every scale.
6. Cache read correctness samples pass after the 10M load without final drain.
7. Benchmark output distinguishes volatile cache shape from durable table shape.

Soft targets:

1. 10M storage-next cache throughput is within 2x of old-engine cache.
2. If the soft target fails with all lifecycle absence counters clean, the next
   owner is commit/read hot-path performance, not lifecycle maintenance.

## Regression Commands

Required before closeout:

```text
cargo fmt --all
cargo clippy -p strata-storage-next --all-targets --all-features -- -D warnings
cargo test -p strata-storage-next --all-features
cargo test -p strata-storage-next cache_mode_lifecycle_policy
cargo test -p strata-storage-next cache_mode_read_correctness_without_maintenance
cargo test -p strata-storage-next lifecycle::tests::capability
```

The named test filters are required discoverability targets. If the actual
module names differ, keep them equally descriptive and update this plan.

## Failure Interpretation

1. Nonzero WAL/checkpoint counters mean cache violated the narrow durability
   boundary.
2. Nonzero flush/rewrite/compaction/background counters mean cache still
   inherits source-table lifecycle policy.
3. Nonzero source-shape admission slowdown means pressure classification is
   still wrong for cache.
4. Read failures without flush/compaction mean the shared read/source semantic
   boundary needs refactoring before scheduling is removed.
5. Durable regressions belong to durable lifecycle cleanup, not cache
   benchmark tuning.
6. Throughput misses with clean lifecycle absence counters belong to a
   commit/read hot-path slice.
