# M4P-L5 Implementation Plan: Table Runtime Parity

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4p-storage-next-parity-restoration-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-test-plan.md`

## Objective

Restore storage-next table-runtime performance mechanics without changing the
L1-L9 architecture.

L5 is not missing wholesale. Storage-next already has `src/table/` surfaces for
row/key adapters, mutable/frozen tables, cursors, immutable table builders,
readers, cache structs, and generic compaction. The parity gap is that the
standard table serving path does not yet match old storage's asymptotic shape:

1. immutable table open eagerly decodes/materializes table rows;
2. point lookup does not use a metadata/index/filter-first path;
3. range and prefix cursors are not block-lazy;
4. table block cache and bloom/filter accelerators are not wired into the
   reader hot path;
5. compaction still has row-collection paths where old storage had streaming
   iterator/build mechanics.

M4P-L5 restores those mechanics inside L5. It must not move branch source
planning, MVCC latest selection, inherited-layer rewriting, compaction
scheduling, durable publication, or recovery policy into table code.

## Audit Finding References

Primary audit and perf evidence:

1. `docs/architecture/perf-tuning/storage-next-mechanics-parity-audit.md`
2. `docs/architecture/perf-tuning/storage-next-serving-path-parity-plan.md`
3. `docs/architecture/perf-tuning/perf-p0-decision-report.md`
4. `docs/architecture/perf-tuning/perf-p1-point-read-mechanics-comparison.md`
5. `docs/architecture/perf-tuning/perf-p2-point-read-isolation-report.md`
6. `docs/architecture/perf-tuning/perf-i1-point-read-fix-plan.md`
7. `docs/architecture/perf-tuning/perf-i3-scan-seek-limit-plan.md`
8. `docs/architecture/perf-tuning/perf-i4-branch-scan-iterator-plan.md`

Relevant parent-plan sections:

1. `M4P-L5: Table Runtime`
2. `Implementation Slices / L5/L6 source-shape restoration`
3. `Performance Gates`
4. `Source Boundary Rules`
5. `Known Gaps And Owners`

Findings covered by this plan:

1. Immutable table readers are too eager and make table open/read cost scale
   with total rows.
2. Point reads need keyed seek/filter behavior rather than unrelated-row
   scanning.
3. Range and prefix scans need lazy table cursors that can serve bounded scans
   without decoding full tables.
4. Block cache and filter/bloom mechanics exist only as disconnected L5
   surfaces until they are wired into readers.
5. Compaction needs streaming table cursor/build mechanics so large inputs do
   not require full row materialization before output production.
6. L5 needs table-source counters that make table-local work visible to L6/L8/L9
   benchmarks.

## Old-Code Source Map

Use old storage as behavioral evidence, not as a shape to copy blindly.

| Old source | Behavior to preserve | Storage-next target |
| --- | --- | --- |
| `crates/storage/src/segment.rs` | Open reads header/footer/index/properties and bloom/filter metadata eagerly, but data blocks lazily. Point lookup uses filter, index search, and one or a few data blocks. | `crates/storage-next/src/table/reader.rs` lazy metadata/index reader and lazy block cursor. |
| `crates/storage/src/index.rs` | Index records key ranges and data-block offsets for binary search and range positioning. | M3G table index payload consumed by `ImmutableTableReader` without decoding all rows. |
| `crates/storage/src/bloom.rs` | Non-authoritative bloom filter avoids data-block reads for absent point keys. | `TableBloomFilter` plus a durable-filter decision before object-backed bloom is enabled. |
| `crates/storage/src/block_cache.rs` | Data blocks are cached by table/file identity and block offset; cache misses read/decode one block. | `TableBlockCache` keyed by table identity and block address; database-owned, not process-global. |
| `crates/storage/src/segment_builder.rs` | Builder creates sorted block/index/properties artifacts and supports compaction-friendly splitting. | `ImmutableTableBuilder` and future streaming output builder over M3G table bytes. |
| `crates/storage/src/segment.rs` iterators | Range and prefix iteration start from an indexed block and advance block-by-block. | Lazy immutable table cursors with `TableKeyBounds`. |
| `crates/storage/src/compaction.rs` | Sorted compaction preserves ordering and drops rows only according to retention/tombstone/TTL policy. | `TableCompactor` streaming over `TableCursor` with caller-supplied policy. |
| `crates/storage/src/segmented/compaction.rs` | Output splitting and compaction IO mechanics. | L5 uses only table-local streaming/splitting mechanics; L6/L8 own level selection, scheduling, and installation. |

Do not port:

1. old `STRAKV` table bytes;
2. direct filesystem, `pread`, path hashing, or process-global cache state;
3. old branch/level ownership from segmented storage;
4. MVCC latest selection or fork/inheritance rewriting;
5. retention policy decisions;
6. durable publication or object names.

## Storage-Next Source Map

Current storage-next surfaces:

| Surface | Current file | Parity action |
| --- | --- | --- |
| Row/key adapters | `crates/storage-next/src/table/key.rs` | Preserve as L5 key surface; add counters/tests only if needed. |
| Mutable/frozen tables | `crates/storage-next/src/table/mutable.rs` | Preserve ordered seek; ensure negative lookup/facts are exposed to L6 without branch semantics. |
| Raw cursors/merge | `crates/storage-next/src/table/cursor.rs` | Preserve raw mechanics; extend only where lazy immutable cursors need shared movement helpers. |
| Immutable builder | `crates/storage-next/src/table/builder.rs` | Keep M3G output; later add streaming builder if compaction needs bounded memory. |
| Immutable reader | `crates/storage-next/src/table/reader.rs` | Replace standard object-backed path with lazy metadata/index/data-block reader; keep eager reader as test/simple path if useful. |
| Cache/accelerators | `crates/storage-next/src/table/cache.rs` | Wire `TableBlockCache` and non-authoritative filter probes into reader hot path. |
| Compaction | `crates/storage-next/src/table/compaction.rs` | Remove full-source collection from the standard compaction path; stream cursors into output builders. |
| L4/L5 handoff | `crates/storage-next/src/service/table.rs` | Open object-backed lazy readers through L4 range sources; L5 still must not import backend/object/layout. |
| Perf tracing | `crates/storage-next/src/observability/perf_trace.rs` | Add table-local counters behind existing feature gates. |

## Layer Ownership Check

L5 owns:

1. table metadata/index interpretation;
2. table-local point seek, range seek, prefix seek, and raw cursor movement;
3. data-block decode and validation;
4. block-cache lookup/insert/invalidation semantics;
5. non-authoritative filter/bloom probes;
6. immutable table building and table-local compaction mechanics;
7. table-local facts, counters, and stats.

L5 does not own:

1. source pruning across active/frozen/L0/nonzero levels;
2. branch, fork, inheritance, materialization, or level topology;
3. MVCC latest-row selection across sources;
4. retention/tombstone/TTL policy decisions;
5. WAL/manifest/checkpoint/recovery mechanics;
6. durable object publication or object names;
7. public L9 APIs or benchmark-only shortcuts.

## Predecessors

Required:

1. M4P-L1 delete/IO boundary remains stable.
2. M4P-L2 table object naming/classification remains in L2/L4.
3. M4P-L3 M3G table artifact format remains authoritative for table bytes.
4. M4P-L4 table object publication/open service remains the object-backed
   boundary.

Conditional predecessor:

1. Durable bloom/filter bytes require a separate L3 format decision before L5
   accepts a filter block in M3G table artifacts. Until that decision is made,
   L5 may support supplied or in-memory filters as non-authoritative
   accelerators, but it must not silently change durable table bytes.

## Execution Plan

### M4P-L5A. Table Source Counters And Current-Path Proof

Goal: measure table-local work before changing reader mechanics.

Steps:

1. Add table-local perf counters for:
   - table opens;
   - metadata bytes read;
   - index bytes read;
   - data-block reads;
   - data-block decode count;
   - rows decoded;
   - rows visited during point lookup;
   - rows visited during range/prefix cursor output;
   - cache hits, misses, inserts, and skipped inserts;
   - filter probes, negative probes, positive probes, and absent-filter probes.
2. Add reader facts that can be inspected by tests without enabling
   `perf-trace`:
   - eager versus lazy mode;
   - metadata/index loaded;
   - data blocks loaded;
   - whether a filter is available;
   - cache enabled/disabled.
3. Add baseline tests that prove current object-backed reads still materialize
   rows, so later changes have an executable before/after proof.
4. Record baseline L9 benchmark numbers for 100K and 1M point-read workloads.

Exit gate:

1. Counters can distinguish metadata/index work from data-block work.
2. Baseline tests prove the current eager path before it is replaced.
3. No production behavior changes beyond counters/facts.

### M4P-L5B. Lazy Metadata And Index Reader

Goal: open immutable tables without decoding data blocks.

Steps:

1. Split `ImmutableTableReader` into a reader core that can hold:
   - table identity;
   - header/footer facts;
   - properties facts;
   - decoded index entries;
   - a `TableByteSource`;
   - optional cache/filter handles;
   - optional eager rows for test/simple mode.
2. Add an explicit lazy open path:
   - read header/footer;
   - read index and properties frames;
   - validate cross-block facts that do not require data-block decode;
   - do not read data blocks during open.
3. Keep `open_bytes` and current eager validation available for format and
   parity tests.
4. Make L4 object-backed reader creation use lazy open by default.
5. Preserve current error vocabulary and source chains.

Exit gate:

1. Object-backed table open performs zero data-block reads.
2. Metadata/index corruption still fails before data-block reads.
3. Eager and lazy readers report the same facts for valid tables.

### M4P-L5C. Lazy Data-Block Access And Block Cache Wiring

Goal: decode only the data blocks needed by point/range work.

Steps:

1. Add a data-block loader that accepts an index entry and returns decoded
   `TableRow` values for that block only.
2. Route all data-block reads through `TableBlockCache` when cache is enabled.
3. Key cache entries by table identity plus block offset/length/ordinal.
4. Validate cached blocks against expected index facts before yielding rows.
5. Preserve correctness when cache is disabled or too small for the block.
6. Add cache invalidation hooks for table identity removal, but keep lifecycle
   policy outside L5.

Exit gate:

1. Repeated point/range reads over the same block hit cache after the first
   miss.
2. Cache disabled and cache enabled return identical rows.
3. Corrupt blocks fail even when cache is populated from prior valid reads only
   by the same table identity and block address.

### M4P-L5D. Indexed Point Lookup

Goal: make point lookup bounded by table metadata, index shape, and target key
chain, not total table rows.

Steps:

1. Use table key range facts to reject impossible point lookups before data
   reads.
2. Use index binary search to locate the first data block that may contain the
   target physical key and version/timestamp bound.
3. Decode only candidate blocks needed to walk the target physical-key chain.
4. Stop once the candidate key range is past the target physical key.
5. Keep MVCC "winner across sources" out of L5. L5 returns raw matching table
   rows or the best row within this one table according to a mechanical bound
   requested by L6.
6. Preserve `get_exact` semantics separately from physical-key latest seek.

Exit gate:

1. Point lookup for a present key reads at most the needed candidate blocks.
2. Point lookup for a missing key outside table bounds reads zero data blocks.
3. Point lookup for a missing key inside table bounds reads bounded candidate
   blocks, not all rows.
4. Rows visited are proportional to the target key chain plus one boundary
   block, not table row count.

### M4P-L5E. Lazy Range And Prefix Cursors

Goal: make range/prefix cursors block-lazy and suitable for L6 level scans.

Steps:

1. Add a lazy immutable table cursor that:
   - seeks to the first candidate block by lower bound;
   - decodes one block at a time;
   - yields rows within `TableKeyBounds`;
   - stops when index key range is past upper bound.
2. Preserve existing `TableCursor` behavior for eager/memory sources.
3. Add direct cursor parity tests across eager and lazy reader modes.
4. Add cursor counters for blocks opened, rows decoded, rows yielded, and rows
   skipped by bounds.
5. Keep branch-level source merge and MVCC latest grouping in L6.

Exit gate:

1. A range limited to one block reads one block.
2. A scan limit can stop before later blocks are decoded.
3. Prefix/range output matches eager reader output exactly.

### M4P-L5F. Filter/Bloom Accelerator Decision And Integration

Goal: restore old point negative-lookup shape without making filters
authoritative or smuggling an L3 format change into L5.

Steps:

1. Define a table-reader filter interface:
   - unavailable;
   - definitely absent;
   - maybe present.
2. Wire existing `TableBloomFilter` into point lookup when a filter is supplied.
3. Add no-false-negative tests for generated keys.
4. Treat every filter result as non-authoritative:
   - definitely absent may avoid data-block reads;
   - maybe present must still validate data blocks.
5. Decide durable object-backed filter support:
   - if V1 accepts durable filter blocks, open a small M4P-L3 follow-up to
     amend M3G table artifact validation, goldens, fuzz, and specs before L5
     reads filter bytes;
   - if V1 defers durable filter blocks, document that object-backed first-open
     negative lookups can only use metadata/index pruning until a supplied
     runtime filter exists.
6. Do not extend M3G bytes inside this L5 slice without the L3 follow-up.

Decision for this slice: V1 defers durable object-backed filter blocks. L5 may
build and attach a supplied runtime filter only from canonical table bytes that
decode to the same table facts and exact table-content digest as the reader.
Object-backed first-open negative lookups continue to rely on metadata/index
pruning unless the source can provide an exact content proof without changing the
lazy read profile. Durable filter bytes require a separate L3 table-format
amendment with validation, goldens, fuzz coverage, and spec text.

Exit gate:

1. Filter false negatives are impossible in tests.
2. Point misses with an available negative filter perform zero data-block reads.
3. Point results are correct when filters are disabled, absent, or return false
   positives.
4. Durable-filter support is either implemented through a documented L3
   amendment or explicitly deferred with a benchmark-visible limitation.

### M4P-L5G. Streaming Table Compaction

Goal: remove full-source materialization from standard table compaction.

Steps:

1. Add a compaction input abstraction over `TableCursor` instead of
   `Vec<TableRow>` only.
2. Stream sorted rows through the existing caller-supplied
   `TableCompactionPolicy`.
3. Keep row dropping fully policy-provided.
4. Build output tables incrementally, splitting by target byte/row limits.
5. Preserve M3G table artifact validation for every output.
6. Keep overlap/level selection and output installation in L6/L8.

Exit gate:

1. Compaction memory is bounded by active cursors plus current output table,
   not total input rows.
2. Output tables are sorted, valid M3G artifacts.
3. Drops match policy decisions exactly.
4. Multi-output compaction preserves non-overlapping output key ranges.

### M4P-L5H. Object-Backed Reader Handoff Closeout

Goal: ensure normal durable table reads use the lazy reader through the L4/L5
boundary.

Steps:

1. Update `TableObjectReaderService` to construct lazy object-backed readers by
   default.
2. Keep object names, backend reads, and range-source adaptation in L4 service
   code.
3. Add tests that prove production `src/table/` still has no backend/object/
   layout imports.
4. Add cache-mode and durable-local parity tests using the same table reader
   mechanics.
5. Add diagnostics that let L6 report table source shape without exposing L5
   internals through public L9 APIs.

Exit gate:

1. Durable object-backed readers are lazy by default.
2. L5 production code remains free of object/backend/layout dependencies.
3. Cache and durable modes share table serving mechanics above L4 source
   adaptation.

### M4P-L5I. Conformance, Benchmarks, And Stop Conditions

Goal: close L5 parity with proof before starting L6 source-shape changes that
depend on it.

Steps:

1. Add an M4P-L5 section to `m4-l5-porting-log.md`.
2. Add or update source guards for:
   - no backend/object/layout/service imports in production `src/table/`;
   - no old `KVSegment`/`STRAKV`/path-hash/process-global cache vocabulary;
   - no branch/MVCC/retention policy terms in L5 compaction code.
3. Add generated tests that exercise:
   - lazy point lookup;
   - lazy range cursor;
   - cache hit/miss;
   - filter available/absent/false-positive/negative;
   - streaming compaction.
4. Run L9 100K and 1M point-read benchmarks after L5D and after L5F.
5. Run L9 100K and 1M range/prefix scan benchmarks after L5E.
6. Record counter movement in a perf closeout report.

Exit gate:

1. Table-local point reads no longer decode full table objects.
2. Table-local scans no longer decode blocks beyond requested bounds/limit.
3. Cache counters show hits on repeated block access.
4. Filter counters show zero-block negative point misses when filters are
   available.
5. Benchmarks improve materially or counters identify a remaining L6/L8
   bottleneck before more architecture work starts.

## Expected Counter Movement

After L5B/L5C:

1. object-backed table open data-block reads: `0`;
2. object-backed table open rows decoded: `0`;
3. metadata/index bytes read: nonzero and bounded by table metadata/index size.

After L5D:

1. point rows decoded: bounded to candidate data block rows;
2. point rows visited: bounded to target physical-key chain plus local block
   boundary rows;
3. point data-block reads: usually `0` for out-of-range misses and `1` for
   single-block hits.

After L5E:

1. scan data-block reads: bounded by returned rows and block density;
2. scan rows decoded: not proportional to full table rows when limit is small;
3. scan cursor setup: does not decode all rows.

After L5F:

1. filtered negative point misses: zero data-block reads;
2. false positives: bounded data-block reads and correct empty result.

## Benchmark Gates

Use L9 benchmarks as proof gates, not as implementation shortcuts.

Required comparison points:

1. 100K cache point-throughput, old versus new;
2. 100K durable-local standard point-throughput, old versus new;
3. 1M cache point-throughput, old versus new;
4. 1M durable-local standard point-throughput, old versus new;
5. 100K and 1M range/prefix scans with scan limit 64;
6. optional 5M point/read scan after L5F if local time allows.

Stop condition:

If L5 counters prove lazy opens, bounded point data-block reads, cache hits, and
filter negative skips, but L9 throughput remains far behind old storage, stop
L5 work and move diagnosis to L6 source fanout or L8 maintenance/compaction.

If L5 counters do not move as expected, do not start L6 fixes. The table reader
path is still not restored.

## Non-Goals

M4P-L5 must not:

1. add a benchmark-only fast path;
2. add a secondary point index outside table artifacts;
3. change public L9 API shapes;
4. move branch source pruning into L5;
5. implement nonzero-level table selection;
6. implement compaction scheduling or automatic maintenance;
7. change durable table bytes without an L3 plan;
8. reintroduce direct filesystem/path/backend calls into production L5 code;
9. change storage-row or internal-key semantics.

## Verification Commands

Run narrow tests first:

```bash
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --locked --lib service::table
cargo test -p strata-storage-next --locked --test table_runtime_properties --features testkit
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
```

Run package health checks after behavior changes:

```bash
cargo test -p strata-storage-next --locked --lib service
cargo test -p strata-storage-next --locked --lib lifecycle
cargo clippy -p strata-storage-next --locked --lib --features perf-trace -- -D warnings
```

Run benchmark proof after L5D, L5E, and L5F:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads scan-prefix,scan-range --samples 100 --scan-limit 64 --value-bytes 150
```

Record results in `docs/architecture/perf-tuning/` before closing the slice.
