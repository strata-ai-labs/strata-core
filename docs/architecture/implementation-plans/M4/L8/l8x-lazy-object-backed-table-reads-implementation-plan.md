# L8X Implementation Plan: Lazy Object-Backed Table Reads

Status: split implementation plan

Implementation note: the shipped storage-next change implements bounded
range-backed durable table open and removes the single full-object read, but it
does not yet implement branch-resident lazy cursors. `BranchOwnedTable` still
requires a row-slice reader contract for validation, reads, compaction,
checkpointing, manifest publication, and materialization. Full lazy query-time
block loading is therefore deferred until that branch/table contract is changed.

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8x-lazy-object-backed-table-reads-test-plan.md`

Predecessors:

1. `docs/architecture/implementation-plans/M4/L8/l8q-durable-table-manifest-format-implementation-plan.md`
2. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-implementation-plan.md`

## Objective

Stop loading whole durable table objects just to open or query them.

Storage-next currently has a range-readable table object source, but the table
reader still performs a full object read before it can serve queries. That is
correct for small test fixtures, but it breaks the storage architecture for
large tables and low-memory profiles. L8X introduces object-backed lazy table
readers: open reads only table metadata and indexes, point/range/prefix queries
read the necessary data blocks on demand, and decoded blocks flow through the
database-local cache and memory-budget accounting.

L8X does not change Strata's MVCC semantics. L5 still owns raw table reader and
block-cache mechanics. L6 still owns branch visibility, inherited layers, fork
gates, and timestamp selection. L8 wires durable object-backed readers into
open/recovery/maintenance without bypassing L6.

## Inputs

1. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
2. `docs/architecture/storage/l5-table-runtime.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
5. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L8/l8r-table-manifest-publication-recovery-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L8/l8u-durable-rewrite-publication-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L8/l8w-memory-cache-budget-enforcement-implementation-plan.md`
10. `crates/storage-next/src/table/reader.rs`
11. `crates/storage-next/src/table/cache.rs`
12. `crates/storage-next/src/service/table.rs`
13. `crates/storage-next/src/format/table/artifact.rs`
14. `crates/storage-next/src/format/table/index.rs`
15. `crates/storage-next/src/format/table/data.rs`
16. `crates/storage-next/src/branch/read.rs`
17. `crates/storage-next/src/branch/state.rs`
18. `crates/storage-next/src/lifecycle/recovery.rs`
19. `crates/storage/src/segment.rs`
20. `crates/storage/src/block_cache.rs`

## Existing-Code Source Map

| Current file | Evidence | L8X action |
|---|---|---|
| `table/reader.rs` | `open_source` calls `read_full_source`, decodes every data block, and stores all rows in memory. | Add an object-backed lazy reader that opens metadata/index first and loads data blocks on demand. Keep byte-reader path for tests and small in-memory use. |
| `service/table.rs` | `TableObjectByteSource` supports bounded `read_exact_at`, but `TableObjectReaderService::open_reader` calls `read_full`. | Route durable table objects to lazy reader open. Preserve source-chain errors for backend range reads. |
| `format/table/artifact.rs` | Table bytes contain header, data blocks, index frame, properties frame, and footer. Whole-table decode validates all blocks. | Add metadata/index/properties decode helpers and per-data-block validation helpers without changing table bytes. |
| `format/table/index.rs` | Index entries map key ranges to data-block offsets and frame lengths. | Use index binary search for point seeks and range/prefix cursor start positions. |
| `table/cache.rs` | Cache is the L5 owner for table block caching. | Cache decoded data blocks by table identity and block offset/address under L8W budget. |
| `branch/read.rs` | Branch readers merge table cursors with MVCC/inheritance rules. | Ensure lazy table cursors implement the same raw cursor contract as eager readers. |
| `lifecycle/recovery.rs` | Recovery rebuilds branch state from table manifests and table objects. | Reopen manifest-listed table objects lazily so recovery does not load every table into memory. |

## Old Codebase Porting Map

The old `KVSegment` reader is the main source of behavior to preserve. It
opened table metadata, used indexes/bloom filters for seeking, read data blocks
with `pread`, and cached blocks. L8X ports those mechanics without porting local
path ownership or process-global cache state.

| Old source | Behavior to preserve | Rewrite decision | Test focus |
|---|---|---|---|
| `storage/src/segment.rs::KVSegment` | Opens metadata and serves point/range reads without reading every data block. | Open table header/footer/index/properties only; read data blocks on demand through L4 range reads. | Open of multi-block table does not read data blocks. |
| `FlatIndex::search` | Binary search chooses the block containing or preceding a target key. | Use table index entries for lazy point/range seek. | Point lookup reads only target block(s). |
| Data block read path | Data blocks are read and decoded on demand. | Add per-block read/decode helper with checksum/frame validation. | Untouched corrupt block fails only when queried. |
| Block cache integration | Repeated reads hit cache rather than backend. | Use database-local cache from L8W and stable table identity keys. | Second query hits cache and does not range-read again. |
| Prefix/range iteration | Cursor scans from seek block and advances block-by-block. | Lazy cursor holds only current decoded block and reads next block when needed. | Long range scan reads sequential blocks and respects bounds. |
| Path hash cache keys | Old cache keys derived from local paths. | Do not port. Cache keys use table identity plus block address. | Two objects with same path-like suffix do not collide. |

Do not port:

1. raw file handles or `pread`;
2. path-derived cache identity;
3. process-global block cache;
4. mmap behavior;
5. host-memory auto detection;
6. product read policy;
7. primitive/query/vector/graph index behavior.

## Scope

L8X current implementation implements:

1. bounded range-backed immutable table open over `TableByteSource`;
2. metadata-only table open from header/footer/index/properties;
3. per-data-block range read and decode during materialized open;
4. eager query parity after materialized open;
5. reader reservations using the materialized table-object budget;
6. range-backed recovery/open for manifest-listed table objects;
7. source-chain preservation for backend range reads;
8. no-default/wasm-compatible memory backend coverage where possible;
9. source guards preventing full-object reads in durable range-backed paths,
   raw IO, path cache keys, product imports, and milestone labels in Rust code.

The follow-up branch-resident lazy reader work implements:

1. index-assisted point lookup;
2. bounded prefix/range cursor that reads blocks on demand;
3. block cache integration with database-local budget;
4. touched-block reservations using L8W budget pools;
5. query-scoped data-block corruption classification;
6. lazy recovery/open for manifest-listed table objects without collecting all
   rows before branch installation;
7. corruption and backend-range-read error classification by phase;
8. parity with eager byte readers for point, prefix, range, tombstone, TTL, and
   timestamp metadata.

L8X does not implement:

1. new table byte format;
2. bloom/filter sidecar format;
3. row pruning;
4. branch lifecycle completion;
5. public query/index API;
6. object-store production provider tuning;
7. async reader traits unless required by existing backend contracts.

## Reader Model

Suggested shape:

```rust
pub(crate) enum ImmutableTableReader {
    Eager(EagerTableReader),
    Lazy(LazyTableReader),
}

pub(crate) struct LazyTableReader {
    identity: TableIdentity,
    source: Arc<dyn TableByteSource>,
    facts: TableRuntimeFacts,
    index: TableIndexBlock,
    properties: TableProperties,
    cache: TableBlockCacheHandle,
    budget: TableReaderBudgetHandle,
}
```

Exact names can change. Required properties:

1. eager and lazy readers expose the same public crate-private query/cursor
   behavior;
2. lazy open does not read data-block ranges;
3. lazy cursor holds at most bounded metadata plus the current decoded block
   unless the caller explicitly collects rows;
4. reader drop releases budget reservations;
5. cache is optional and zero capacity means uncached reads.

## Open Protocol

Target open sequence:

```text
validate table object facts and backend range-read capability
reserve reader metadata budget
read table header
read table footer
read index frame and properties frame ranges named by footer
decode and validate index/properties against header and object facts
construct lazy reader
```

Rules:

1. Open must not read data-block frames.
2. Header/footer/index/properties decode errors are open-time table errors.
3. Data-block corruption is discovered when the corresponding block is read.
4. Object metadata mismatch rejects before range reading when metadata is
   available.
5. Range-read short/long responses preserve backend/source classification.

## Query Protocol

Point lookup:

```text
use index to find candidate data block
check block cache
if miss, read and decode candidate block
binary search/scan rows inside block
return exact row or none
```

Prefix/range cursor:

```text
use index to find first candidate block
read/decode block
seek within block
yield rows until bound end
read next block only when current block is exhausted and bounds require it
```

Rules:

1. Missing key in a table reads at most the candidate block unless index proves
   no candidate.
2. Prefix/range bounds must not read blocks outside the key interval.
3. Large values are returned from the decoded block without keeping unrelated
   blocks resident.
4. Cursor advancement must be deterministic and match eager reader ordering.
5. Branch/MVCC visibility remains in L6, not in lazy reader code.

## Cache And Budget Integration

Rules:

1. Cache key = table identity plus block offset or stable block ordinal.
2. Cache value = decoded data block or authoritative block bytes, whichever is
   chosen consistently by L5.
3. Cache hit avoids backend range read.
4. Cache decode failure is typed and does not poison unrelated blocks.
5. Reader metadata budget is separate from block cache budget.
6. A block larger than cache capacity is served uncached.
7. Pinned/current cursor block counts against reader/block budget and releases
   on cursor drop or block advance.

## Recovery And Lifecycle Integration

Current rules:

1. Durable recovery of table manifests opens table objects by bounded ranges,
   but still materializes rows before branch installation.
2. Recovery validates table facts from metadata/index/properties before reading
   data blocks.
3. Optional deep validation can remain a maintenance/repair operation that scans
   every block under budget.
4. Flush/rewrite publication may still use eager bytes immediately after build,
   but reopened durable objects still use materialized readers until the
   branch table contract accepts lazy cursors.
5. Cache mode can use eager in-memory byte readers for small tables, but must
   pass the same query parity tests.

## Error And Health Vocabulary

Current implementation covers:

1. backend lacks range read;
2. table metadata range read failed;
3. table data block range read failed during materialized open;
4. short/long metadata range read;
5. short/long data block range read;
6. corrupt header/footer/index/properties.

Follow-up lazy cursor work adds typed errors/facts for:

1. corrupt data block discovered on query;
2. cache decode failure;
3. reader budget exceeded;
4. block budget exceeded;
5. lazy reader deep validation debt.

Every error must expose a stable code and preserve source chains.

## Source Boundaries

L8X may import:

1. L4 table object byte sources and range-read services;
2. L5 table format/index/data/cache APIs;
3. L6 raw table cursor interfaces;
4. L8W budget/cache handles;
5. L8 recovery health/outcome types.

L8X must not import:

1. `std::fs`, `File`, `OpenOptions`, mmap, or path IO;
2. host-memory probes;
3. process-global cache singleton state;
4. product/query/index policy modules;
5. backend delete/quarantine/purge APIs;
6. StrataHub code;
7. primitive/vector/graph modules.

Rust code, test names, fixture bytes, and user-facing error strings must not
include milestone labels.

## Implementation Steps

1. Add format helpers for header/footer/index/properties metadata-only decode.
2. Add format helper for decoding one data block by index entry.
3. Add lazy reader type implementing the existing raw reader/cursor contracts.
4. Wire `TableObjectReaderService` to return lazy readers for durable objects.
5. Add cache lookup/insert around decoded data blocks.
6. Add reader/block budget reservations from L8W.
7. Update branch table state/recovery to accept lazy readers without collecting
   all rows. This remains the required follow-up before full lazy query-time
   loading can be claimed.
8. Preserve eager reader path for byte fixtures and small in-memory tables.
9. Add direct tests, generated scripts, source guards, and porting-log entry.

## Deferred Behavior

Deferred to later table/runtime work:

1. bloom/filter sidecars;
2. partitioned indexes beyond the current table index;
3. async streaming reader traits;
4. remote object-store read-ahead and multipart tuning;
5. product query/index APIs.

## Exit Gate

L8X is complete when:

1. durable table open reads metadata/index only, not all data blocks;
2. point reads fetch only needed block ranges;
3. prefix/range cursors fetch blocks lazily and respect bounds;
4. lazy and eager readers have identical query results;
5. corrupt untouched data blocks do not fail open but fail when queried;
6. cache hit/miss/eviction behavior is covered under L8W budgets;
7. recovery opens large manifest-listed tables without whole-object loading;
8. no-default/wasm smoke remains compatible with memory-backed range reads;
9. source guards block full-object durable reads, raw IO, global caches, product
   imports, and milestone labels in Rust code;
10. generated tests cover query order, fault windows, cache, and budget release.
