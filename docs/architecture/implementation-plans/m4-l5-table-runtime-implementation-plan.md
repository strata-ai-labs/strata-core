# M4-L5 Implementation Plan: Table Runtime

Status: draft implementation plan

## Objective

Build the reusable table mechanics layer for storage-next.

M4-L5 ports the existing table algorithms from `crates/storage` into
`crates/storage-next/src/table/`, retargeted to:

1. storage-next `StorageRow` and encoded internal-key bytes;
2. the M3G immutable table byte format;
3. L4 table object publication and object access boundaries;
4. direct L5 model/conformance tests.

M4-L5 must not recreate the whole storage engine. It must produce mechanical
table components that L6 can assemble into a branch-isolated LSM runtime.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/storage/implementation-patterns.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
7. `docs/spec/strata-storage-format-v1.md`
8. `crates/storage-next/src/format/table/`
9. `crates/storage-next/src/service/table.rs`

## Existing-Code Source Map

The current implementation evidence lives mainly in `crates/storage`.

| Current file | Relevant L5 evidence | Porting rule |
|---|---|---|
| `crates/storage/src/memtable.rs` | ordered mutable table, frozen behavior, sorted iteration, point/range seek, in-memory bloom use | Port mechanics; replace product `Value` with storage-next `StorageRow` facts. |
| `crates/storage/src/key_encoding.rs` | ordered internal-key byte model | Use storage-next key encoding and treat keys as ordered bytes in L5. |
| `crates/storage/src/segment_builder.rs` | immutable table builder, block construction, index/filter/property mechanics | Port builder mechanics only; do not port old `STRAKV` bytes or direct path writes. |
| `crates/storage/src/segment.rs` | immutable table reader, point lookup, range scans, index and block reads, corruption detection | Port reader mechanics; replace `pread` and path identity with object/range-backed sources. |
| `crates/storage/src/index.rs` | table index support | Port only if it still applies to M3G index facts. |
| `crates/storage/src/bloom.rs` | blocked bloom filter implementation | Reuse as optional non-authoritative accelerator; do not add durable filter bytes unless the table format spec is explicitly extended. |
| `crates/storage/src/block_cache.rs` | cache lookup, insertion, eviction, stats | Port algorithm carefully; replace process-global/path-hash identity with database-owned table-object/block keys. |
| `crates/storage/src/merge_iter.rs` | raw k-way merge iterator | Port `MergeIterator`; leave `MvccIterator` and branch rewriting for L6. |
| `crates/storage/src/seekable.rs` | seekable cursor mechanics | Port raw seek mechanics only; leave MVCC, fork gates, and inherited-layer rewriting for L6. |
| `crates/storage/src/compaction.rs` | sorted compaction iterator mechanics and tombstone cases | Convert policy into caller-supplied retention/tombstone/TTL decisions. |
| `crates/storage/src/segmented/compaction.rs` | output splitting and compaction execution evidence | Extract table mechanics only; branch level selection, install, scheduling, and manifest updates are L6/L8. |
| `crates/storage/src/segmented/mod.rs` | flush/read helpers, table scans, many regression cases | Use as behavioral evidence; do not port wholesale because it mixes L5, L6, L7, and L8. |

Storage-next already provides:

1. M3G immutable table bytes under `crates/storage-next/src/format/table/`.
2. L4 table object publication under `crates/storage-next/src/service/table.rs`.
3. A placeholder table module at `crates/storage-next/src/table/mod.rs`.

## L5 Boundaries

L5 owns:

1. mutable table mechanics;
2. frozen table views;
3. immutable table building;
4. immutable table reading;
5. raw point, prefix, and range cursors;
6. raw sorted merge cursors;
7. table-local block cache;
8. optional table-local accelerators;
9. generic table compaction execution;
10. table facts and stats needed by L6/L8.

L5 must not own:

1. branch topology;
2. branch-local level ownership;
3. inherited copy-on-write layers;
4. fork-version gates;
5. commit version allocation;
6. WAL-before-visible discipline;
7. visible-version publication;
8. open/recovery/checkpoint scheduling;
9. retention or quarantine policy;
10. object names, filesystem paths, or backend syscalls;
11. product capability semantics.

## Implementation Slices

| Slice | Title | Implementation scope | Test scope | Exit gate |
|---|---|---|---|---|
| `L5A` | Source map and module scaffold | Create the `table` module structure, table error types, config types, table facts/stats, and porting log entries. Promote only the L3 table-format APIs L5 needs. Detailed plan: `docs/architecture/implementation-plans/M4/l5a-table-runtime-scaffold-implementation-plan.md`. | Compile-only module tests and dependency/source guards. | Table module compiles with no behavior and no imports from upper layers. |
| `L5B` | Row and key adapters | Define the L5 row/key surface over `StorageRow`, encoded internal-key bytes, table-key ranges, and size accounting. Avoid product `Value` or old `Key` types. Detailed plans: `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-test-plan.md`. | Boundary tests for ordering, duplicate keys, range bounds, and storage-space opacity. | L5 can compare, bound, and report rows without interpreting branch/product meaning. |
| `L5C` | Mutable and frozen tables | Port mutable table insert, tombstone insert, sorted iteration, approximate memory accounting, point/range seek, and freeze. Use storage-next rows. Detailed plans: `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-test-plan.md`. | Mutable/frozen model tests over generated rows, duplicate internal-key rejection, freeze immutability, memory accounting sanity. | Mutable and frozen tables produce deterministic encoded-key order and stable facts. |
| `L5D` | Raw cursors and merge cursor | Port raw seekable cursors for mutable/frozen sources and a k-way merge cursor. Keep MVCC latest selection and branch rewriting out. Detailed plans: `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-test-plan.md`. | Cursor movement tests, seek boundary tests, prefix/range tests, generated merge model tests, linear/heap path coverage. | Raw cursors merge sorted sources deterministically and return exact encoded-key order. |
| `L5E` | Immutable table builder | Wrap M3G `encode_immutable_table` behind a table-builder API that accepts sorted L5 rows, target block sizing, and compression config. Return table artifact bytes and facts. Detailed plans: `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-test-plan.md`. | Builder round trips against L3 decode, bad-order/duplicate/empty rejection, deterministic bytes, one-block and multi-block output tests. | L5 can build valid M3G table artifacts without direct object IO. |
| `L5F` | Immutable table reader | Implement table readers over M3G bytes and a range-readable table source. Validate through L3, serve point/range/prefix cursors over decoded rows, and keep the API compatible with later lazy data-block decode. Detailed plans: `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-test-plan.md`. | Reader point/range/prefix tests, corruption routing, truncated block tests, checksum failures, bytes-backed and range-backed reader parity. | L5 can read M3G table artifacts without loading unrelated layers or using filesystem paths. |
| `L5G` | Block cache and accelerators | Add database-owned block cache with stable table-object/block keys. Add optional in-memory bloom/filter accelerators only if they remain non-authoritative. Detailed plans: `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-test-plan.md`. | Cache hit/miss/eviction/stats tests, cache isolation tests, no process-global behavior, accelerator false-negative guard. | Table reads are correct without cache and deterministic with cache enabled. |
| `L5H` | Generic table compaction | Implement compaction over raw sorted L5 cursors with caller-supplied row retention, tombstone, and TTL policies. Produce one or more table artifacts split by target size. Detailed plans: `docs/architecture/implementation-plans/M4/l5h-generic-compaction-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5h-generic-compaction-test-plan.md`. | Property tests over generated sorted inputs, policy-provided prune/tombstone/TTL cases, output ordering, split-size bounds, validation of produced artifacts. | L5 compaction preserves ordering and only drops rows when supplied policy says so. |
| `L5I` | Object-backed table access handoff | Add the L4/L5 adapter needed for table readers to consume published table objects without direct backend calls from production `table/` code. Keep object naming and durable publication in L4. Detailed plans: `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-test-plan.md`. | Memory and localfs object-backed reader tests, faulting range-read tests, publish-then-read through L4 table object service. | L5 can read a published table object through an object abstraction while L4 still owns object names and publication. |
| `L5J` | L5 conformance closeout | Consolidate direct conformance, fuzz, source guards, and documentation. Record retired old-code behavior and deferred L6/L8 policy. Detailed plans: `docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-implementation-plan.md` and `docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-test-plan.md`. | Full L5 test matrix, fuzz seeds or fuzz-adjacent generated coverage for table reader/cursor movement, source scans for product vocabulary and upper-layer imports. | M4-L5 closes and L6 can build branch state on top of L5 table mechanics. |

## Test Plan

The reference-grade M4-L5 test plan is
`docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`.

M4-L5 tests must be direct table tests. They must not require engine primitives
or branch runtime behavior. Existing `crates/storage` tests are evidence and
regression input, not proof of storage-next L5 completeness.

## Required Generated Models

M4-L5 should have small reference models instead of only example tests:

1. **Mutable model:** generated rows sorted by encoded internal key; model uses a
   `BTreeMap<Vec<u8>, StorageRow>` or sorted vector.
2. **Cursor model:** generated table sources plus seek operations; expected
   output computed by independent sorted-vector filtering.
3. **Merge model:** generated sorted sources; expected output computed by
   stable k-way merge over vectors.
4. **Builder/reader model:** generated rows and block sizes; assert
   encode/decode/read identity.
5. **Compaction model:** generated sources plus generated policy decisions;
   assert L5 drops exactly the rows the policy permits and preserves order.

## Format Policy

M4-L5 must use the M3G table format as-is unless the storage format spec is
explicitly amended.

Consequences:

1. Do not port old `STRAKV` v7 bytes.
2. Do not add durable bloom/filter blocks in L5 unless M3G is intentionally
   extended.
3. Use M3G header, footer, data block, index block, properties block, checksum,
   compression, and allocation bounds.
4. Treat old `STRAKV` files as historical evidence only.

## Reader Access Policy

L5 production code must not call `std::fs`, construct paths, or call backend
syscalls directly.

Allowed reader inputs:

1. in-memory table bytes for tests and compaction outputs;
2. a small range-readable table source supplied by L4/L6;
3. table object facts supplied by L4/L6.

Object names and durable publication remain L4 responsibilities. Branch
reachability and table installation remain L6 responsibilities.

## Compaction Policy Boundary

L5 may execute compaction, but it must not decide compaction safety.

Caller-supplied policy must answer questions such as:

1. whether an older version may be dropped;
2. whether a tombstone may be elided;
3. whether an expired TTL row may be dropped;
4. whether an output split must stop before an overlap boundary.

L6/L8 will provide those decisions later. During M4-L5, tests should use
mechanical policy stubs that make the boundary explicit.

## M4-L5 Exit Gate

M4-L5 is complete when:

1. `crates/storage-next/src/table/` contains the mutable table, frozen table,
   immutable table builder, immutable table reader, raw cursors, merge cursor,
   block cache, and generic compaction mechanics.
2. All table bytes are M3G table bytes.
3. L5 table readers can operate over in-memory bytes and object/range-backed
   sources without direct filesystem/backend access.
4. L5 compaction can merge sorted sources and produce valid M3G table artifacts
   under caller-supplied retention/tombstone/TTL policies.
5. Direct model/property/conformance tests cover mutable, frozen, reader,
   cursor, merge, cache, and compaction behavior.
6. Source guards prove L5 does not import upper storage layers or product
   primitives.
7. The porting log records which old `crates/storage` mechanics were reused,
   rewritten, deferred to L6-L8, or retired.
