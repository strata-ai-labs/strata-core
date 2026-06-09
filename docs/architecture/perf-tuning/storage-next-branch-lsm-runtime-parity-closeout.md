# Storage-Next Branch LSM Runtime Parity Closeout

Status: implementation guards complete; benchmark rerun required

Date: 2026-06-08

Companion plans:

- `docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l6-branch-lsm-runtime-parity-test-plan.md`

Audit inputs:

- `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
- `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`

## Scope

This report records the closeout evidence for branch-local LSM runtime parity:
source topology, inherited-layer reads, fork gates, tombstones, timestamp reads,
bounded point probes, bounded scan setup, materialization, compaction install,
source guards, generated model coverage, and the benchmark gate.

The benchmark gate is not closed by historical throughput alone. It must compare
source-shape counters before throughput so that we can distinguish source-layout
drift from ordinary implementation cost.

## Model And Generated Coverage

The generated branch harness and direct branch tests now cover:

1. active and frozen mutable sources;
2. overlapping L0 tables;
3. non-overlapping nonzero levels;
4. many tables per nonzero level;
5. owned and inherited source chains;
6. fork-version caps;
7. child-local shadowing;
8. tombstones and timestamp visibility;
9. materialization and compaction transitions;
10. snapshot install into branch-local LSM sources.

Primary proof files:

- `crates/storage-next/src/testkit/branch_lsm.rs`
- `crates/storage-next/src/testkit/branch_lsm/model_store.rs`
- `crates/storage-next/src/testkit/branch_lsm/read_model.rs`
- `crates/storage-next/src/testkit/branch_lsm/compaction.rs`
- `crates/storage-next/tests/branch_lsm_properties.rs`
- `crates/storage-next/tests/branch_lsm_closeout.rs`
- `crates/storage-next/src/branch/tests/source_layout.rs`
- `crates/storage-next/src/branch/tests/point_pruning.rs`
- `crates/storage-next/src/branch/tests/scan_pruning.rs`
- `crates/storage-next/src/branch/tests/history_pruning.rs`
- `crates/storage-next/src/branch/tests/inheritance_materialization/validation_fork.rs`

## Source Guards

The branch runtime source guard suite proves the branch-local LSM runtime does
not import or construct:

1. commit orchestration;
2. lifecycle orchestration;
3. public API request/response DTOs;
4. backend IO operations;
5. filesystem paths;
6. object-layout names or path construction;
7. service/quarantine/checkpoint/WAL machinery;
8. product-level key, value, namespace, branch-name, dataset, or provider terms;
9. roadmap labels in Rust source.

The closeout suite also checks that the source guard itself covers those
categories, so a future weakening of the guard is visible in CI.

## Required Benchmark Commands

Run old and new engines serially. Use the public benchmark surface for the new
engine and the old cache benchmark for the historical engine.

New engine:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m,5m,10m --engines cache,standard --workloads load-seq,point-latest,point-throughput,scan-prefix,scan-range-throughput --samples 1000 --branch-samples 100 --scan-limit 64 --value-bytes 150 --flush-every 100000
```

Old engine:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-old-cache-scale -- --scales 100k,1m,5m,10m --workloads load-seq,point-latest,point-throughput,scan-prefix,scan-range-throughput --samples 1000 --branch-samples 100 --scan-limit 64 --value-bytes 150
```

Run 50M and 100M only after the 10M source-shape counters are clean.

Required metadata:

1. machine and hardware;
2. build profile;
3. git revision, branch, and dirty state;
4. mode and durability policy;
5. backend;
6. feature state;
7. key count;
8. value size;
9. sample count;
10. scan limit;
11. maintenance policy.

## Derived Source-Shape Metrics

The new benchmark runner records `source_shape_metrics` beside raw
`perf_trace`. Required fields:

1. `point_source_probes_per_read`;
2. `point_nonzero_table_probes_per_read`;
3. `scan_source_cursors_per_call`;
4. `scan_table_cursors_opened_per_call`;
5. `scan_rows_visited_per_row_returned`;
6. `l0_tables_per_million_rows_after_load`.

Compare source-shape counters before throughput. Throughput only becomes a
runtime-efficiency question after these counters are old-equivalent or the
remaining shape difference is documented with an owner and replacement proof.

## Historical Benchmark Anchors

The following checked-in reports are historical anchors. They are useful for
throughput trend comparison, but they predate the derived `source_shape_metrics`
object added by this closeout slice.

- `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-05T13-19-49Z-12d2790b.json`
- `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-05T07-13-25Z-12d2790b.json`
- `benchmarks/results/storage-old-cache/storage-old-cache-scale-2026-06-05T06-57-50Z-12d2790b.json`
- `benchmarks/results/storage-old-cache/storage-old-cache-scale-2026-06-05T07-14-14Z-12d2790b.json`

## Source-Counter-Aware Rerun Gate

Closeout is not complete until a fresh old-vs-new benchmark run records the
derived source-shape metrics for at least 100K, 1M, 5M, and 10M rows.

The rerun must verify:

1. point reads probe bounded sources;
2. point reads over nonzero levels probe at most one table per nonzero level per
   readable branch-local layer;
3. scan setup opens bounded source cursors;
4. scan setup over nonzero levels opens lazy level cursors rather than one eager
   cursor per table;
5. scan rows visited per row returned is explained by limit and source fanout;
6. post-load L0 table count per million rows is consistent with the maintenance
   policy.

## Deferred Findings

| Finding | Owner | Reason | Replacement Proof |
| --- | --- | --- | --- |
| Durable object-backed table and branch artifact persistence beyond the current branch runtime shape | Durable format and object-layout runtime | Branch runtime now owns source semantics; durable bytes remain a separate persistence contract. | Format goldens, object-layout validation, durable open/reopen tests, and old-vs-new benchmark reruns once durable bytes carry the same shape. |
| Background maintenance policy for automatic compaction cadence | Maintenance runtime | This slice restores branch-local source mechanics but does not decide compaction scheduling. | Maintenance tests must prove the same branch-local source layout after durable-only facts are ignored. |
| End-to-end public API benchmark closure at 50M and 100M | Public API benchmark surface | The test plan requires those scales only after 10M source-shape counters are clean. | Fresh benchmark reports with `source_shape_metrics`, raw `perf_trace`, metadata, and old-to-new throughput ratios. |

