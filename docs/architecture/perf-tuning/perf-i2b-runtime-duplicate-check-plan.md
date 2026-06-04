# PERF-I2B Runtime Duplicate Check Fix Plan

## Scope

PERF-I2B replaces the runtime duplicate mutation check in storage-next with a
non-quadratic implementation. This is a targeted load-path correction, not a
storage architecture change.

There is an older `PERF-I2B` subsection in
`perf-i2-load-path-fix-plan.md` for batch-local append validation. Treat that
older label as historical. This document is the active PERF-I2B plan for the
measured runtime duplicate-check bottleneck.

## Goal

Preserve current duplicate-key semantics while replacing the O(batch squared)
scan in `validate_duplicate_mutations` with a single-pass or sort-and-scan
implementation.

The fix must keep the existing commit/runtime boundaries:

1. no public L9 API shape change;
2. no duplicate-policy behavior change;
3. no benchmark-only shortcut;
4. no table, branch, compaction, or index redesign;
5. no removal of runtime validation for internal/testkit callers.

## Evidence

Latest 100K sequential cache-load comparison, 1,000 mutations per batch,
150-byte values:

| Engine | Throughput | Elapsed |
| --- | ---: | ---: |
| storage-next cache | 184,936 ops/s | 540.73 ms |
| old cache | 486,861 ops/s | 205.40 ms |

Measured storage-next load counters:

| Counter | Value |
| --- | ---: |
| commit batches | 100 |
| user mutation rows | 100,000 |
| timeline rows | 200 |
| prepared rows | 100,200 |
| append rows | 100,200 |
| public batch build time | 20.31 ms |
| commit call time | 516.81 ms |
| API runtime time | 512.47 ms |
| runtime validation time | 392.52 ms |
| runtime duplicate comparisons | 49,950,000 |
| append validation time | 45.06 ms |
| append insert time | 48.11 ms |

Measured old-cache load counters:

| Counter | Value |
| --- | ---: |
| public batch build time | 20.24 ms |
| commit call time | 185.07 ms |

Conclusion:

1. Public batch construction is not the primary gap; old and new both spend
   about 20 ms building the 100K load batches.
2. Runtime validation alone takes about 392 ms, which is more than the entire
   old-engine commit path.
3. The duplicate scan performs 49,950,000 comparisons because each 1,000-row
   batch compares each mutation to every earlier mutation.

Hot file:

`crates/storage-next/src/commit/batch.rs`

Hot helper:

`validate_duplicate_mutations`

## Current Behavior

The runtime duplicate check walks the batch in input order and compares each
mutation key with all prior mutation keys. On the first duplicate observed in
input order, it returns:

```rust
CommitRuntimeError::DuplicateMutationKey {
    space_id: mutation.physical_key().storage_space_id(),
}
```

That observable behavior is part of the correctness contract for this fix.

The old engine does not perform an equivalent quadratic preflight. Its write set
is keyed by storage key and insertion is map-shaped, so load cost is proportional
to inserted writes rather than all prior writes in the same batch.

## Correctness Contract

PERF-I2B must preserve these behaviors:

1. `CommitDuplicateKeyPolicy::Reject` still rejects duplicate mutation physical
   keys before the runtime mutates branch state.
2. Duplicate detection remains based on the full physical key, not only user
   key or storage-space id.
3. Error shape remains `CommitRuntimeError::DuplicateMutationKey`.
4. The reported `space_id` remains the later duplicate mutation's storage-space
   id, matching the current input-order scan.
5. Empty-batch, mutation-space, read-fact, CAS-fact, timestamp, branch, and
   durability validation stay unchanged.
6. Duplicate read facts and duplicate CAS facts are not changed in this patch.
7. Public `CommitBatch` duplicate validation stays in place; runtime validation
   still protects lower-level constructors and testkit paths.

## Implementation Shape

Preferred implementation: a single-pass borrowed identity set.

`PhysicalKey` currently derives `Eq`/`PartialEq` but not `Hash` or `Ord`.
Rather than changing the row model first, add a small private borrowed identity
wrapper inside `commit/batch.rs`:

```rust
struct PhysicalKeyIdentity<'a> {
    branch_id: BranchId,
    space: &'a str,
    storage_space_id: StorageSpaceId,
    user_key: &'a [u8],
}
```

The wrapper can implement or derive `Eq`, `PartialEq`, and `Hash`, because the
borrowed fields already support those traits. It must be private to
`commit/batch.rs`.

Then update `validate_duplicate_mutations`:

1. create `HashSet<PhysicalKeyIdentity<'_>>` with capacity
   `mutations.len()`;
2. iterate mutations in input order;
3. build a borrowed identity from `mutation.physical_key()`;
4. insert it into the set;
5. if insertion fails, return the same `DuplicateMutationKey` error as today;
6. record a perf counter representing one key check per mutation.

This keeps duplicate error ordering equivalent to the current implementation
while reducing 1,000-row batch work from 499,500 comparisons to 1,000 set
checks.

Fallback implementation: sort borrowed identities and preserve error ordering.

Use the fallback only if the borrowed `HashSet` wrapper becomes awkward in
practice. If sorting is used, it must still report the earliest later duplicate
in input order, not an arbitrary sorted neighbor. That means carrying original
indices and choosing the smallest later duplicate index before returning.

## Work Steps

### PERF-I2B-A: Pin Semantics

Files:

1. `crates/storage-next/src/commit/batch.rs`
2. `crates/storage-next/src/commit/tests/batch.rs`

Work:

1. Add or update a focused test proving duplicate runtime validation reports
   the later duplicate mutation's storage-space id.
2. Add a multi-duplicate test if one does not already exist, so the first
   duplicate in input order remains the returned error.
3. Do not edit benchmark harnesses in this step.

Exit gate:

Existing duplicate mutation tests pass before the implementation change.

### PERF-I2B-B: Replace Quadratic Scan

Files:

1. `crates/storage-next/src/commit/batch.rs`
2. `crates/storage-next/src/observability/perf_trace.rs`

Work:

1. Add the private borrowed physical-key identity wrapper.
2. Replace the nested loop in `validate_duplicate_mutations` with a
   single-pass set check.
3. Avoid cloning `PhysicalKey`, `String`, `Vec<u8>`, or value payloads.
4. Keep all other runtime validation helpers unchanged.
5. Update perf tracing so the metric clearly distinguishes old comparisons
   from new key checks. Either rename the counter on the trace branch or add a
   new counter and keep the old one at zero after the fix.

Exit gates:

1. duplicate key tests pass;
2. `runtime duplicate comparisons` no longer scales as O(batch squared);
3. no public API or durable-format diff is introduced.

### PERF-I2B-C: Measure

Run:

```sh
cargo fmt --all -- --check
cargo check -p strata-storage-next --features perf-trace
cargo test -p strata-storage-next --lib --features perf-trace duplicate_mutation
cargo test -p strata-storage-next --lib --features perf-trace conflict
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache --workloads load-seq --samples 1000 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- --scales 100k --workloads load-seq --samples 1000 --value-bytes 150
```

Record the new storage-next result JSON and compare against the current result:

`benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-04T20-52-59Z-6e4ccce4.json`

Also keep the old-cache comparison result:

`benchmarks/results/storage-old-cache/storage-old-cache-scale-2026-06-04T20-53-15Z-6e4ccce4.json`

## Decision Gates

Proceed only if the fix moves the measured bottleneck:

1. duplicate check work drops from 49,950,000 comparisons to at most one check
   per user mutation row, about 100,000 checks for this benchmark;
2. runtime validation time drops by at least 10x, from about 392 ms to under
   40 ms for the 100K cache load;
3. storage-next cache load improves materially, with an initial target of at
   least 350K ops/s or commit-call time below 250 ms;
4. if throughput improves by less than 25%, stop and profile again before
   implementing any further load-path changes.

## Risks

1. A sort-based implementation can accidentally change which duplicate is
   reported. Preserve input-order behavior.
2. Adding `Hash`/`Ord` directly to `PhysicalKey` may be reasonable later, but it
   is broader than this fix. Prefer a private borrowed wrapper first.
3. Removing runtime validation because public `CommitBatch` already validates
   duplicates would leave lower-level runtime/testkit constructors unprotected.
4. Renaming perf counters can make before/after JSON harder to compare. If a
   new counter is added, record both names in the benchmark report.

## Completion Criteria

PERF-I2B is complete when:

1. duplicate semantics are pinned by tests;
2. runtime duplicate validation is non-quadratic;
3. the 100K cache load benchmark is rerun;
4. the decision gates are recorded in the perf-tuning notes;
5. any remaining storage-next vs old-cache gap has a fresh profile before the
   next implementation plan is written.

## Implementation Result

Implemented on the `perf/storage-next-traces-fixes` branch.

Storage-next 100K cache load after the runtime duplicate-check fix:

| Metric | Before | After |
| --- | ---: | ---: |
| throughput | 184,936 ops/s | 705,607 ops/s |
| elapsed | 540.73 ms | 141.72 ms |
| commit call time | 516.81 ms | 118.25 ms |
| runtime validation time | 392.52 ms | 4.63 ms |
| duplicate check work | 49,950,000 comparisons | 100,000 key checks |

Same-session old-cache comparison:

| Engine | Throughput | Elapsed | Commit Call Time |
| --- | ---: | ---: | ---: |
| storage-next cache | 705,607 ops/s | 141.72 ms | 118.25 ms |
| old cache | 494,481 ops/s | 202.23 ms | 181.67 ms |

Benchmark artifacts:

1. `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-04T21-13-37Z-6e4ccce4.json`
2. `benchmarks/results/storage-old-cache/storage-old-cache-scale-2026-06-04T21-13-48Z-6e4ccce4.json`

Decision:

PERF-I2B met the decision gates. The runtime duplicate check is no longer the
remaining 100K cache-load bottleneck.
