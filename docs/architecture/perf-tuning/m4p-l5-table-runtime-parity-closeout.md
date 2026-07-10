# M4P-L5 Table Runtime Parity Closeout

Status: closed with L6/L8 follow-up

Date: 2026-06-08

Companion plans:

- `docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-test-plan.md`

## Scope

This report records the M4P-L5 closeout gates for table-local lazy point reads,
bounded scans, cache/filter behavior, generated conformance, and L9 benchmark
evidence.

M4P-L5 does not add dedicated benchmark fast paths. The proof must come from the
normal L5 reader, cursor, cache, filter, object-backed source, and compaction
machinery.

## Generated Conformance

Generated table-runtime coverage now requires nonzero counters for:

1. lazy reader opens;
2. lazy point hits;
3. lazy point misses;
4. lazy range cursors;
5. object-backed reader parity;
6. cache hits;
7. cache misses;
8. filter available paths;
9. filter absent paths;
10. filter negative probes;
11. filter false-positive paths;
12. streaming compaction outputs.

The generated reader checks assert lazy open facts before materialization, then
compare point-hit, point-miss, and bounded cursor output to the generated sorted
model. The object-backed generated route runs the same lazy point/range checks
through the L4 service handoff. The bloom/filter generated route covers
no-false-negative, negative, unavailable, available, and deterministic
false-positive paths.

Verification:

```sh
cargo test -p strata-storage-next --locked --features testkit --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_closeout
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
```

Result:

- `table_runtime_properties`: passed, 2 tests.
- `table_runtime_closeout`: passed, 5 tests.
- `table_runtime_source_guard`: passed, 15 tests.

## Source Guards

The source guard suite still proves production `crates/storage-next/src/table`
does not import or embed:

1. backend/object/layout/service APIs;
2. branch/commit/lifecycle/engine layers;
3. filesystem/path APIs;
4. object layout literals;
5. old `KVSegment`/`STRAKV`/`SegmentBuilder` vocabulary;
6. old path-hash/file-id/process-global cache vocabulary;
7. branch/MVCC/retention policy terms in L5 compaction;
8. bare public table-runtime API surface.

## Required L9 Benchmark Commands

The test plan names `scan-range`, but the current benchmark binary exposes the
range workload as `scan-range-throughput`. The closeout used the current binary
name.

### Point Throughput

Command:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
```

Result:

- 100K cache completed.
- 100K standard completed.
- 1M cache failed during load before serving benchmarks.

100K results:

| Scale | Engine | Load | Point Throughput | Point Rows Visited | Point Candidates | Table Seeks |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 100K | cache | 601,689 ops/s | 615,922 ops/s | 1,000 | 1,000 | 1,000 |
| 100K | standard | 500,716 ops/s | 934,725 ops/s | 1,000 | 1,000 | 1,000 |

1M failure:

```text
storage budget exceeded for active_mutable: requested 376506 bytes/0 count,
used 67033412 bytes/0 count, limit 67108864 bytes: commit would exceed active
mutable storage budget
```

### Prefix And Range Scans

Command:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads scan-prefix,scan-range-throughput --samples 100 --scan-limit 64 --value-bytes 150
```

Result:

- 100K cache completed.
- 100K standard completed.
- 1M cache failed during load before serving benchmarks.

100K results:

| Scale | Engine | Load | Prefix p50 | Prefix Ops/s | Range Ops/s | Scan Rows Visited | Scan Candidates | Cursor Seeks |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 100K | cache | 604,672 ops/s | 31.42 us | 29,361 | 13,429 | 6,400 range | 6,400 range | 100 range |
| 100K | standard | 510,537 ops/s | 31.21 us | 31,183 | 13,900 | 6,400 range | 6,400 range | 100 range |

1M failure:

```text
storage budget exceeded for active_mutable: requested 376506 bytes/0 count,
used 67033412 bytes/0 count, limit 67108864 bytes: commit would exceed active
mutable storage budget
```

## Supplemental 1M Runs With Maintenance

The required commands use `flush_every=off`, which cannot load 1M rows under the
current active mutable budget. To get serving-path evidence at 1M, the benchmark
was rerun with `--flush-every 100000`.

### Point Throughput

Command:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 1m --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150 --flush-every 100000
```

Result file:

- `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-08T02-08-53Z-7d124aeb.json`

Results:

| Scale | Engine | Load | Point Throughput | Point Rows Visited | Point Candidates | Table Seeks |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1M | cache | 87,709 ops/s | 47,629 ops/s | 10,000 | 1,000 | 11,000 |
| 1M | standard | 76,957 ops/s | 45,469 ops/s | 10,000 | 1,000 | 11,000 |

Interpretation:

- Table-local work is bounded per table: `point_rows_visited / point_candidates`
  is 10 rows per sampled point.
- L9 still performs about 11 table seeks per point sample at 1M after
  maintenance. That is source fanout above a single L5 table lookup.
- The remaining point-throughput gap should move to L6 source selection and L8
  maintenance/compaction shape, not another L5 reader fast path.

### Prefix And Range Scans

Command:

```sh
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 1m --engines cache,standard --workloads scan-prefix,scan-range-throughput --samples 100 --scan-limit 64 --value-bytes 150 --flush-every 100000
```

Result file:

- `benchmarks/results/storage-next-l9/storage-next-l9-scale-2026-06-08T02-09-28Z-7d124aeb.json`

Results:

| Scale | Engine | Load | Prefix p50 | Prefix Ops/s | Range Ops/s | Scan Rows Visited | Scan Candidates | Cursor Seeks |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1M | cache | 94,687 ops/s | 68.42 us | 13,250 | 13,182 | 6,400 range | 6,400 range | 1,100 range |
| 1M | standard | 78,290 ops/s | 68.33 us | 14,351 | 14,905 | 6,400 range | 6,400 range | 1,100 range |

Interpretation:

- Range serving returns bounded row counts: 100 samples with limit 64 produced
  6,400 visited rows.
- Cursor setup scales with source count: 1,100 cursor seeks for 100 range
  samples means about 11 source cursors per sample.
- Remaining scan gap is source fanout and merge setup above L5, not evidence
  that L5 range cursors are decoding full tables.

## Filter And Cache Counter Notes

The L9 benchmark output currently reports high-level point/scan counters but
does not print table block-cache hit/miss or filter positive/negative counters.
The closeout therefore uses:

1. generated `TableRuntimeScaffoldOutcome` counters for cache hit/miss and
   filter available/absent/negative/false-positive coverage;
2. table-local direct tests and perf-trace tests for zero-block negative filter
   point misses;
3. L9 point/scan counters to identify remaining source fanout above L5.

## Stop Condition Decision

L5 closeout passes generated conformance and source-guard gates. The mandatory
1M benchmark commands do not complete with `flush_every=off`; that is a load/
maintenance stop condition, not a table-reader stop condition.

With maintenance enabled, 1M serving counters show bounded table-local work but
multiple table seeks/cursor setups per sample. The next performance diagnosis
should move to L6 source-shape planning and L8 maintenance/compaction behavior.

Do not add L5-specific benchmark fast paths unless a future table-local counter
proves L5 itself regressed again.
