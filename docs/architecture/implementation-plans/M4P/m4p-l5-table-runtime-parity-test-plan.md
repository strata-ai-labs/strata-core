# M4P-L5 Test Plan: Table Runtime Parity

Status: draft

Companion implementation plan:
`docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-implementation-plan.md`

## Goal

Prove that storage-next L5 table runtime has the same table-local performance
shape as old storage while preserving the clean L5 boundary.

The suite must fail if:

1. immutable table open decodes all data blocks in the standard object-backed
   path;
2. point lookup scans unrelated rows inside a table;
3. range/prefix cursors decode blocks beyond their bounds or caller limit;
4. cache-enabled and cache-disabled readers diverge in results;
5. bloom/filter accelerators become authoritative or can produce false
   negatives;
6. compaction drops rows without caller policy;
7. compaction requires full input materialization in the standard path;
8. L5 imports backend/object/layout/service/branch/lifecycle code;
9. benchmarks improve only through a parallel fast path rather than the normal
   reader/cursor machinery.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/reader.rs`
2. `crates/storage-next/src/table/tests/cache.rs`
3. `crates/storage-next/src/table/tests/cursor.rs`
4. `crates/storage-next/src/table/tests/compaction.rs`
5. `crates/storage-next/src/table/tests/builder.rs`
6. `crates/storage-next/src/service/table.rs`
7. `crates/storage-next/src/testkit/table_runtime.rs`
8. `crates/storage-next/tests/table_runtime_properties.rs`
9. `crates/storage-next/tests/table_runtime_source_guard.rs`
10. `crates/storage-next/tests/table_runtime_closeout.rs` if a closeout
    inventory test is needed.

Do not add backend/object imports to production `src/table/`. Object-backed
tests belong in `service/table.rs`, testkit, or external integration tests.

## Old-Code Regression Sources

Use these files as old-behavior evidence:

1. `crates/storage/src/segment.rs`
2. `crates/storage/src/segment_builder.rs`
3. `crates/storage/src/index.rs`
4. `crates/storage/src/bloom.rs`
5. `crates/storage/src/block_cache.rs`
6. `crates/storage/src/merge_iter.rs`
7. `crates/storage/src/seekable.rs`
8. `crates/storage/src/compaction.rs`
9. `crates/storage/src/segmented/compaction.rs`

The tests should preserve table-local observable behavior, not old filesystem
or branch-runtime implementation details.

## Required Test Matrix

| Slice | Required proof |
| --- | --- |
| L5A counters/facts | Counter reset/snapshot tests and baseline eager-reader proof. |
| L5B lazy metadata/index reader | Open reads metadata/index/properties only; no data blocks. |
| L5C lazy data blocks/cache | Candidate block decode only; cache hit/miss/invalidation behavior. |
| L5D indexed point lookup | Present, absent, version-bound, timestamp-bound, and tombstone point lookups are block-bounded. |
| L5E lazy range/prefix cursors | Range and prefix scans decode only overlapping blocks until bound/limit. |
| L5F filter/bloom | No false negatives; negative filter avoids data-block reads; false positives remain correct. |
| L5G streaming compaction | Output is sorted, valid, policy-driven, and memory-bounded by active output. |
| L5H object-backed handoff | L4-backed readers use lazy table machinery without moving object/backend code into L5. |
| L5I closeout | Source guards, generated harness counters, benchmark reports, and porting log are complete. |

## L5A Tests: Counters And Baseline

Required tests:

1. Perf counters start at zero and can be reset.
2. Lazy-reader counters are absent or zero before lazy reader work lands.
3. Current eager object-backed open increments row/data-block decode counters.
4. Counter snapshots distinguish:
   - metadata reads;
   - index reads;
   - data-block reads;
   - rows decoded;
   - rows visited;
   - cache hits/misses;
   - filter probes.
5. Counter tests compile without `perf-trace` by using test-visible reader
   facts where needed.

Negative tests:

1. A point-read test must fail if all table rows are counted as visited for a
   single-key lookup after L5D.
2. A lazy-open test must fail if data-block reads occur during open after L5B.

## L5B Tests: Lazy Metadata And Index Reader

Required tests:

1. Lazy open of one-block table reads header/footer/index/properties and zero
   data blocks.
2. Lazy open of multi-block table reads zero data blocks.
3. Lazy open returns the same table facts as eager open.
4. Corrupt header fails before index/data reads.
5. Corrupt footer fails before index/data reads.
6. Corrupt index fails before data reads.
7. Corrupt properties fails before data reads.
8. Metadata/index fact drift fails before data reads.
9. Missing range from object source preserves source-chain error.
10. `open_bytes` eager mode remains available for format parity tests.

Required assertions:

1. `data_block_reads == 0`;
2. `rows_decoded == 0`;
3. `reader_mode == lazy`;
4. `metadata_loaded == true`;
5. `index_loaded == true`.

## L5C Tests: Lazy Blocks And Cache

Required tests:

1. First point hit reads and decodes one candidate block.
2. Second point hit in the same block records cache hit and no source read.
3. Point hits in different blocks read exactly those blocks.
4. Cache disabled returns identical rows with expected misses/skips.
5. Oversized block is not cached but still reads correctly.
6. Cache entry for table A does not satisfy table B with same block offset.
7. Cache entry for same table but different block length/ordinal does not
   collide.
8. Removing a table identity invalidates all of that table's cached blocks.
9. Corrupt block is not cached as valid data.
10. Cache stats remain deterministic under duplicate insertion.

Negative tests:

1. A cache hit must not bypass block-fact validation.
2. A disabled cache must not alter correctness.
3. Cache keys must not use filesystem paths or process-global file ids.

## L5D Tests: Indexed Point Lookup

Required point cases:

1. key before table range;
2. key after table range;
3. key between blocks but absent;
4. key absent inside candidate block;
5. key present once;
6. key present with multiple commit versions;
7. version-bound lookup;
8. timestamp-bound lookup;
9. tombstone row lookup;
10. expired-looking row preserved as raw row;
11. first key in table;
12. last key in table;
13. first key in a non-first block;
14. high-bit and embedded-zero user-key bytes.

Required counter assertions:

1. out-of-range miss reads zero data blocks;
2. present single-block hit reads one data block on cold cache;
3. repeated hit reads zero data blocks on warm cache;
4. rows visited are bounded to the target key chain plus local block boundary,
   not total table rows;
5. exact encoded-key lookup and physical-key lookup remain separate semantics.

Differential assertions:

1. Lazy reader result equals eager reader result for every generated point case.
2. Object-backed lazy reader result equals bytes-backed lazy reader result.

## L5E Tests: Lazy Range And Prefix Cursors

Required range cases:

1. empty table;
2. lower bound before first key;
3. lower bound inside first block;
4. lower bound in a later block;
5. upper bound inside same block;
6. upper bound in later block;
7. exclusive lower bound;
8. exclusive upper bound;
9. unbounded lower;
10. unbounded upper;
11. degenerate empty range;
12. prefix range with embedded-zero user key;
13. scan limit smaller than first block;
14. scan limit crossing block boundary;
15. scan limit reached before final table block.

Required assertions:

1. cursor output equals eager reader output;
2. decoded block count is bounded by overlapping blocks plus one boundary block;
3. scan limit stops additional block reads;
4. repeated scan over same blocks uses cache when enabled;
5. advancing after exhaustion is idempotent.

## L5F Tests: Filter/Bloom

Required tests:

1. Empty filter reports unavailable or definitely absent according to config.
2. Generated inserted keys never return definitely absent.
3. Generated absent keys may return definitely absent or maybe present.
4. Definitely absent point lookup performs zero data-block reads.
5. Maybe present point lookup still validates data blocks.
6. False-positive filter result returns correct empty row result.
7. Disabled filter returns identical point results with more data-block reads.
8. Corrupt or mismatched supplied filter table proof is rejected before the
   filter can hide rows.
9. Durable filter blocks stay deferred unless the L3 format amendment is
   present.
10. Tables with identical public table facts but different content cannot share
    a supplied runtime filter.

No-false-negative property:

For every generated table, build the filter from all physical keys represented
in the table. Every physical key in the table must probe as maybe present.

## L5G Tests: Streaming Compaction

Required tests:

1. zero sources produce no outputs;
2. one source copies rows when policy keeps all;
3. many disjoint sources merge in order;
4. overlapping sources preserve duplicate physical keys at distinct versions;
5. exact duplicate internal keys are rejected or resolved only by documented
   input rules;
6. caller policy can drop older versions;
7. caller policy can keep all tombstones;
8. caller policy can elide tombstones;
9. caller policy can drop expired-looking rows;
10. policy errors abort without partial success;
11. output splits by target byte/row size;
12. each output table decodes as valid M3G;
13. output key ranges are sorted and non-overlapping;
14. compaction does not import branch/retention/lifecycle policy.

Counter assertions:

1. rows streamed equals input rows consumed;
2. rows materialized at once is bounded by current output table;
3. output artifact count matches split decisions;
4. drop summaries match policy decisions exactly.

## L5H Tests: Object-Backed Reader Handoff

Required tests:

1. `TableObjectReaderService` opens object-backed lazy readers by default.
2. Object-backed reader uses range reads for metadata/index and candidate data
   blocks.
3. Object-backed point lookup matches bytes-backed lazy lookup.
4. Object-backed range cursor matches bytes-backed lazy cursor.
5. Read failures preserve L4 source-chain errors.
6. Table object fact mismatches are rejected before row results are exposed.
7. Cache mode and durable-local mode use the same L5 reader semantics above the
   source adapter.
8. L5 production code remains free of backend/object/layout imports.

## Source Guard Requirements

Extend `table_runtime_source_guard` so production `crates/storage-next/src/table`
rejects:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::object`;
4. `crate::service`;
5. `crate::branch`;
6. `crate::commit`;
7. `crate::lifecycle`;
8. engine crate imports;
9. `std::fs`, `std::path`, `File`, `pread`, `mmap`, `rename`, `remove_file`;
10. object path literals such as `tables/`, `wal/`, `snapshots/`,
    `manifest/current`;
11. old table vocabulary such as `KVSegment`, `SegmentBuilder`, `STRAKV`,
    `SegmentId`, `file_id`, `path_hash`, `global_cache`;
12. branch policy vocabulary in `table/compaction.rs`, including `fork`,
    `inherited`, `materialization`, `retention`, `quarantine`, and
    `checkpoint`;
13. public API leakage through bare `pub` production items where the existing
    storage-next rule expects `pub(crate)`.

Each forbidden category needs a probe proving the guard catches it.

## Generated And Fuzz-Adjacent Coverage

Extend `TableRuntimeScaffoldOutcome` or add a new M4P-L5 outcome with nonzero
counters for:

1. lazy reader opens;
2. lazy point hits;
3. lazy point misses;
4. lazy range cursors;
5. cache hits;
6. cache misses;
7. filter negative probes;
8. filter false-positive paths;
9. streaming compaction outputs;
10. object-backed reader parity.

Generated tests must compare lazy results to an independent sorted-vector model
or the existing eager reader, but performance assertions must use table-local
counters rather than elapsed wall-clock time.

Fuzz-adjacent tests should route through existing table artifact fuzz seeds and
add reader/cursor movement scripts when the lazy reader lands.

## Benchmark Proof

Benchmarks are not pass/fail unit tests, but every L5 behavior slice that
changes serving mechanics must record benchmark evidence.

Required after L5D:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads point-throughput --samples 1000 --value-bytes 150
```

Required after L5E:

```bash
cargo run --release --manifest-path benchmarks/Cargo.toml --bin storage-next-l9-scale -- --scales 100k,1m --engines cache,standard --workloads scan-prefix,scan-range --samples 100 --scan-limit 64 --value-bytes 150
```

Required after L5F:

1. rerun point-throughput;
2. compare cold-cache and warm-cache point misses if benchmark support exists;
3. record filter-negative data-block read counters.

Store results in `docs/architecture/perf-tuning/`.

## Stop Conditions

Stop L5 work and re-diagnose if:

1. lazy open still reads or decodes data blocks;
2. point lookup still visits rows proportional to table size;
3. range cursor still decodes blocks beyond the requested bound/limit;
4. cache hit counters move but source read counters do not fall;
5. filter negative probes occur but data-block reads do not fall;
6. table-local counters look correct but L9 point/scan throughput remains
   dominated by L6 source fanout or L8 maintenance debt;
7. implementing durable bloom requires a table-format change that has not been
   accepted by an L3 plan.

Do not proceed to L6 source-pruning work until L5 counters prove table-local
point/range work is bounded.

## Verification Commands

Narrow checks:

```bash
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --locked --lib service::table
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo test -p strata-storage-next --locked --features testkit --test table_runtime_properties
```

Feature and hygiene checks:

```bash
cargo clippy -p strata-storage-next --locked --lib --features perf-trace -- -D warnings
cargo test -p strata-storage-next --locked --lib service
cargo test -p strata-storage-next --locked --lib lifecycle
```

Wasm/no-default checks should be run before L5 closeout if the touched code
changes feature-gated imports:

```bash
cargo test -p strata-storage-next --locked --target wasm32-unknown-unknown --no-default-features --lib
```
