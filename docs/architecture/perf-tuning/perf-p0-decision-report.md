# PERF-P0 Decision Report

## Scope

This report records the proof run for storage-next performance tuning. The goal
was measurement only: identify whether the current 100K-key regression is caused
by cache-mode divergence, machine variance, or concrete storage-next hot-path
work that differs from the old engine.

No serving-path correction was implemented in this phase. The code changes for
this report add `perf-trace` counters and attach them to benchmark results.

## Environment

Machine reported by the benchmark:

| Field | Value |
| --- | --- |
| CPU | Apple M1 Pro |
| Cores | 8 |
| RAM | 16 GB |
| OS | macOS |
| Arch | aarch64 |

Commands:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k --engines cache,standard --workloads load-seq,point-throughput,scan-range-throughput --samples 1000 --scan-limit 10 --value-bytes 150

cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- --scales 100k --workloads load-seq,point-throughput,scan-range-throughput --samples 1000 --scan-limit 10 --value-bytes 150
```

Result files:

1. `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-03T19-29-51Z-56d9ac5e.json`
2. `benchmarks/results/storage-old-cache/storage-old-cache-scale-2026-06-03T19-30-12Z-56d9ac5e.json`

## Throughput

| Engine | Mode | Load | Point latest | Range scan |
| --- | --- | ---: | ---: | ---: |
| old storage | cache | 434,758 ops/s | 464,936 ops/s | 88,382 ops/s |
| storage-next | cache | 35,472 ops/s | 44 ops/s | 22 ops/s |
| storage-next | standard | 42,838 ops/s | 45 ops/s | 22 ops/s |

The storage-next cache and standard read results are effectively identical.
That rejects cache-mode divergence as the primary explanation for point and
range scan throughput.

## Hot-Path Counters

### Load

| Engine | read views | read-view rows cloned | append staging clones | append staging rows cloned | blind conflict sources |
| --- | ---: | ---: | ---: | ---: | ---: |
| storage-next cache | 100 | 4,959,900 | 100 | 4,959,900 | 100 |
| storage-next standard | 100 | 4,959,900 | 100 | 4,959,900 | 100 |

Finding: blind load commits still build conflict-validation read views, and each
commit clones the accumulated branch state. Append staging also clones the
accumulated branch state. This explains a large part of the load gap, but the
load regression is smaller than the read regression.

### Point Latest

| Engine | read views | read-view rows cloned | point rows visited | point candidates | table seeks |
| --- | ---: | ---: | ---: | ---: | ---: |
| storage-next cache | 1,000 | 100,200,000 | 100,200,000 | 1,000 | 0 |
| storage-next standard | 1,000 | 100,200,000 | 100,200,000 | 1,000 | 0 |

Finding: every point lookup clones the full read view and linearly visits the
full table contents. The candidate count is 1,000 for 1,000 lookups, so the
work is almost entirely locating one row by scanning all rows.

### Range Scan

| Engine | read views | read-view rows cloned | scan rows visited | scan candidates materialized | table seeks |
| --- | ---: | ---: | ---: | ---: | ---: |
| storage-next cache | 1,000 | 100,200,000 | 100,200,000 | 49,301,780 | 0 |
| storage-next standard | 1,000 | 100,200,000 | 100,200,000 | 49,301,780 | 0 |

Finding: limit-10 range scans clone the full read view, visit the full table,
and materialize about 49,302 candidates per scan before the API-level limit is
applied. This is a direct confirmation of full materialization before limit
pushdown.

## Confirmed Causes

1. Point reads are linear in total row count.
2. Range scans are linear in total row count and materialize far more rows than
   the requested limit.
3. Read-view capture clones every row for every point and scan request.
4. Blind load commits build conflict sources even when validation facts are
   empty.
5. Append staging clones the full branch state once per load batch.

## Rejected Hypotheses

1. Cache mode is the primary cause. Cache and standard read throughput and
   counters match.
2. Durable overhead is the primary cause for point and range reads. Standard
   mode is not meaningfully slower than cache mode for reads.
3. The current point path already uses ordered table seeks. `table_seeks` is 0
   while `point_rows_visited` is 100,200,000 for 1,000 lookups.
4. The current scan path already pushes the limit into the table walk.
   `scan_candidates_materialized` is 49,301,780 for 1,000 limit-10 scans.

## PERF-P0C Spike Decision

No benchmark-local spike was needed for the first decision. The counters are
direct observations from production hot paths and already prove the suspected
linear work. A spike would be useful only for estimating the exact movement of a
specific correction before landing it.

## Decision

Do not promote a broad correction bundle.

The proof run found multiple overlapping linear costs. In particular, point
reads pay both full read-view cloning and full candidate scanning. Fixing only
candidate scanning may still leave a row-proportional clone on every lookup;
fixing only read-view cloning may still leave row-proportional candidate
collection. Starting a large combined rewrite would violate the tuning
discipline this plan is meant to enforce.

The next step should be one additional isolation step, not a rearchitecture:

1. Add a benchmark-local spike that compares current point lookup against a
   direct ordered-key seek over the same in-memory table data.
2. Add a benchmark-local spike that compares current read-view capture against a
   shared-state/pinned read-view capture model, without changing production
   behavior.
3. Use those spike results to choose the first promoted correction slice:
   `PERF-T3` if read-view cloning dominates, `PERF-T4` if point candidate
   scanning dominates, or a deliberately scoped combined read-view-plus-point
   slice if neither change can move the benchmark alone.

## Stop Conditions

1. If a point-seek spike does not reduce row visits from 100K per lookup to
   bounded per-key work, do not promote `PERF-T4`.
2. If a pinned-read-view spike does not remove row-proportional capture work,
   do not promote `PERF-T3`.
3. If neither isolated spike predicts an order-of-magnitude point-read
   improvement, stop and profile CPU time before changing production code.

## Verification

Commands run:

```sh
cargo fmt --all -- --check
cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale
cargo test -p strata-storage-next --features perf-trace
```

Results:

1. `cargo fmt --all -- --check` passed after formatting.
2. `cargo check --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale`
   passed. The workspace still emits pre-existing warnings from older crates.
3. `cargo test -p strata-storage-next --features perf-trace` compiled and ran
   the storage-next suite; 2,623 tests passed, 2 failed, and 2 were ignored. The
   two failures were the pre-existing lifecycle compaction tests already known
   outside PERF-P0:
   `lifecycle::tests::compaction::remaining::failed_table_rewrite_attempt_does_not_close_runtime`
   and
   `lifecycle::tests::compaction::remaining::queued_table_rewrites_are_blocked_after_close_without_mutating_branch`.

