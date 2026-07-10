# M4-L5 Porting Log

Status: active during M4-L5

## Purpose

This document records how table-runtime behavior moves from the current
`crates/storage` implementation into `crates/storage-next` during M4-L5.

The M4-L5 implementation plan owns order and scope. This log owns the porting
audit trail: what was read, what was preserved, what changed, what was deferred,
and what old code became eligible for retirement.

## Rules

1. Add or update a slice entry before changing storage-next table code.
2. Prefer porting, splitting, and tightening existing storage behavior over
   fresh implementation.
3. Fresh implementation is allowed only when the entry records why existing
   behavior is obsolete, out of scope, or inconsistent with V1.
4. Do not delete old storage code until replacement tests exist and workspace
   references are gone.
5. If old code cannot be deleted because current crates still depend on it,
   record it as legacy-retained instead of adding compatibility glue to
   storage-next.
6. Treat old tests as evidence, not authority. Preserve the cases that still
   match V1 semantics; reject or rewrite cases that freeze obsolete behavior.

## Entry Template

```md
## <Slice>: <Title>

### Current Files Read

- `crates/storage/src/...`

### Behavior Preserved

- ...

### Intentional V1 Changes

- ...

### Deferred

- ...

### Tests Ported Or Added

- ...

### Retirement

- Deleted:
- Legacy-retained:
- Follow-up:
```

## Baseline Source Map

| Target area | Current source material | Initial disposition |
|---|---|---|
| Mutable and frozen tables | `crates/storage/src/memtable.rs` | Port ordered-table mechanics over storage-next rows. |
| Ordered key bytes | `crates/storage/src/key_encoding.rs` | Preserve ordering facts, but keep key meaning outside L5. |
| Immutable table building | `crates/storage/src/segment_builder.rs` | Port builder mechanics onto M3G table bytes. |
| Immutable table reading | `crates/storage/src/segment.rs`, `crates/storage/src/index.rs` | Port reader/index mechanics without direct file or path access. |
| Optional accelerators | `crates/storage/src/bloom.rs` | Reuse only as non-authoritative table-local accelerators. |
| Block cache | `crates/storage/src/block_cache.rs` | Port into database-owned cache state, not a process-global singleton. |
| Raw cursors and merge | `crates/storage/src/merge_iter.rs`, `crates/storage/src/seekable.rs` | Port raw cursor mechanics; leave MVCC/COW wrappers to L6. |
| Generic compaction | `crates/storage/src/compaction.rs`, `crates/storage/src/segmented/compaction.rs` | Extract policy-free merge/build mechanics; defer reachability and install policy. |

## Slice Entries

## M4-L5A: Table Runtime Scaffold

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/storage/target-crate-shape-and-test-harness.md`
- `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
- `docs/architecture/implementation-plans/M4/l5a-table-runtime-scaffold-implementation-plan.md`
- `crates/storage-next/src/table/mod.rs`
- `crates/storage-next/src/format/table/`
- `crates/storage-next/src/service/table.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/block_cache.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/compaction.rs`

### Behavior Preserved

- The table runtime remains a separate storage-domain module under
  `crates/storage-next/src/table/`.
- Table mechanics are prepared as internal crate surfaces only. No public API is
  exposed.
- Table configuration, facts, and statistics are table-local shells that later
  slices can reuse.
- Build/decode error paths preserve the underlying L3 format error as an error
  source.

### Intentional V1 Changes

- L5A does not port old table behavior yet. It creates the scaffolding and
  guardrails needed before behavior moves.
- Table identity is an opaque table-local component. It does not construct
  object names and does not encode branch or level ownership.
- Table cache configuration is database-owned configuration, not a
  process-global singleton.
- Source guards enforce that table runtime code does not import upper layers,
  engine crates, product vocabulary, filesystem APIs, or backend APIs.

### Deferred

- Row/key adapters move to L5B.
- Mutable and frozen table behavior moves to L5C.
- Raw cursor and merge behavior moves to L5D.
- Immutable table building moves to L5E.
- Immutable table reading moves to L5F.
- Block cache behavior moves to L5G.
- Generic compaction moves to L5H.
- Object-backed read handoff moves to L5I.

### Tests Ported Or Added

- Add table module smoke tests for default config construction.
- Add invalid config tests with typed `TableRuntimeError` results.
- Add table facts construction and impossible-fact rejection tests.
- Add error display/source-chain tests for wrapped `FormatError`.
- Add table stats smoke tests.
- Add a `table_runtime_properties` generated harness routed through hidden
  testkit for executable L5A scaffold-contract coverage.
- Add `table_runtime_source_guard` integration tests for layer imports, product
  vocabulary, filesystem/backend APIs, process-global cache state, and public
  API leakage.

### Sensitivity Probes

- Add executable source-guard probes proving forbidden upper-layer and testkit
  imports such as `crate::branch`, `crate::commit`, `crate::lifecycle`, and
  `crate::testkit` are rejected.
- Add executable source-guard probes proving product payload vocabulary in
  production table modules is rejected.
- Add executable source-guard probes proving filesystem/path APIs, direct
  backend calls, and environment reads such as `std::env::var` are rejected.
- Add executable source-guard probes proving bare `pub ` production forms are
  rejected, including `pub struct`, `pub enum`, `pub trait`, `pub type`,
  `pub fn`, `pub async fn`, `pub unsafe fn`, `pub const`, `pub static`,
  `pub extern`, `pub macro`, `pub union`, `pub mod`, and `pub use`, while
  scoped crate-private visibility such as `pub(crate)` is allowed.

### Retirement

- Deleted: none.
- Legacy-retained: all current `crates/storage` table files remain in use by
  existing storage consumers.
- Follow-up: L5B-L5J should record each behavior family as it is ported and
  identify the old code that becomes eligible for retirement after cutover.

## M4-L5B: Row And Key Adapters

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5b-row-key-adapters-test-plan.md`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/format/key.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `crates/storage-next/src/format/table/data.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/memtable.rs`

### Behavior Preserved

- Encoded internal keys still sort by physical key ascending and commit version
  descending.
- Duplicate physical keys at distinct commit versions remain valid table input.
- Duplicate encoded internal keys are rejected by helpers that require unique
  rows.
- User keys with embedded zero bytes are ordered by the V1 byte-stuffed key
  encoding.
- Row metadata such as tombstone marker, commit timestamp, expiry, and value
  bytes is preserved exactly by the table row adapter.

### Intentional V1 Changes

- L5B adapts storage-next `StorageRow` and `InternalKey` types only. It does
  not port old `Key`, `TypeTag`, or `StoredValue` surfaces.
- Table key helpers operate on encoded V1 bytes. They do not interpret branch
  ids, storage space ids, timestamps, expiry, tombstones, or row values.
- Size accounting is explicitly approximate and table-runtime local. It is not
  a durable byte-format fact.
- Raw key construction verifies canonical V1 internal-key bytes through the
  storage-next format codec instead of accepting arbitrary old table bytes.

### Deferred

- Mutable and frozen table insertion/storage move to L5C.
- Cursor movement over key bounds moves to L5D.
- Immutable table builder input validation consumes these adapters in L5E.
- Immutable table reader output adapters move to L5F.
- Compaction policy and row dropping remain deferred to L5H and higher-layer
  callers.

### Tests Ported Or Added

- Add module-local `table::tests::key` tests for V1 internal-key ordering,
  physical-key prefix behavior, row metadata preservation, sorted-unique
  validation, key-bound filtering, and approximate size accounting.
- Extend the hidden testkit table-runtime property route with generated row/key
  adapter checks.
- Extend `table_runtime_properties` to require generated L5B coverage under the
  `testkit` feature.
- Extend `table_runtime_source_guard` with old key/value import checks and
  executable probes for old `crates/storage` key surfaces.

### Sensitivity Probes

- Add tests that fail if same-physical-key versions sort oldest first.
- Add tests that fail if storage space id, branch id, or zero-byte user-key
  encoding stops affecting encoded key order.
- Add tests that fail if duplicate encoded internal keys are accepted.
- Add tests that fail if duplicate physical keys at distinct commit versions
  are rejected.
- Add tests that fail if inclusive/exclusive key bounds drift.
- Add tests that fail if size estimates become zero or non-monotonic for larger
  key/value bytes.
- Add source-guard probes proving old key/value imports and product vocabulary
  are rejected.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/key_encoding.rs` and
  `crates/storage/src/memtable.rs` remain in use by current storage consumers.
- Follow-up: L5C should consume `TableRow` and sorted-key validation instead of
  reintroducing old memtable key types.

## M4-L5C: Mutable And Frozen Tables

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-test-plan.md`
- `crates/storage-next/src/table/key.rs`
- `crates/storage-next/src/table/mutable.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/bloom.rs`

### Behavior Preserved

- Mutable tables store rows by encoded internal-key order.
- Duplicate physical keys at distinct commit versions remain valid table input.
- Duplicate exact encoded internal keys are rejected.
- Sorted full iteration is deterministic and suitable as later builder input.
- Approximate byte accounting increases only on successful insertion.
- Freezing produces a read-only in-memory table with the same rows and facts.

### Intentional V1 Changes

- L5C uses storage-next `StorageRow` and L5B `TableRow` only. It does not port
  old `Key`, `Namespace`, `TypeTag`, `Value`, or `VersionedValue` surfaces.
- The first storage-next mutable table uses a deterministic `BTreeMap` instead
  of the old concurrent skiplist. L6/L7 own placement and write serialization;
  a later performance slice may swap the internal structure behind the same
  contract.
- Freeze consumes `MutableTable` and returns `FrozenTable`. The old
  panic-on-write-after-freeze behavior is not ported.
- L5C exposes mechanical exact, range, and physical-prefix reads only. It does
  not port MVCC latest-selection, snapshot filtering, TTL filtering, or
  tombstone hiding.
- Frozen table bloom filters are deferred to L5G. Frozen tables are correct
  without accelerators.

### Deferred

- Raw cursor traits and merge cursors move to L5D.
- Immutable table building from frozen rows moves to L5E.
- Immutable table reading moves to L5F.
- Bloom filters and block cache behavior move to L5G.
- Generic compaction over mutable/frozen/table sources moves to L5H.
- Branch-local active/frozen ownership moves to L6.
- Commit visibility, snapshots, and WAL ordering move to L7.
- Recovery and retention policy move to L8.

### Tests Ported Or Added

- Add module-local mutable/frozen tests for empty facts, insert behavior,
  duplicate rejection, sorted iteration, exact lookup, range filtering,
  physical-prefix filtering, and freeze preservation.
- Expand module-local coverage for put rows, empty-value puts, tombstones,
  storage-owned ids, engine-owned ids, expired-looking rows, all key-bound
  shapes, present and absent exact lookup, branch/space/storage-id prefix
  isolation, and min/max commit facts independent of key order.
- Extend the hidden testkit table-runtime property route with generated
  mutable/frozen model checks against an ordered map.
- Strengthen the generated route so every generated script includes deterministic
  edge rows for empty values, tombstones, expired-looking rows, embedded NUL
  user-key bytes, storage-owned ids, engine-owned ids, duplicate physical keys
  at distinct commit versions, and exact duplicate insert attempts.
- Extend `table_runtime_properties` to require L5C generated coverage under the
  `testkit` feature.
- Keep `table_runtime_source_guard` covering the new production table module.

### Sensitivity Probes

- Add tests that fail if duplicate inserts mutate row count, facts, bytes, or
  existing row bytes.
- Add tests that fail if rows iterate in insertion order instead of encoded
  internal-key order.
- Add tests that fail if duplicate physical keys at distinct commit versions are
  collapsed into one row.
- Add tests that fail if tombstones or expired-looking rows are hidden by
  mechanical reads.
- Add tests that fail if exact lookup fabricates a latest-visible row, if range
  bounds drop inclusive/exclusive endpoints, or if physical-prefix reads include
  adjacent user keys, other branches, other spaces, or other storage-space ids.
- Add generated checks that fail if mutable and frozen table facts diverge from
  the ordered-map model.
- Add generated checks that fail if mutable or frozen closed-range filtering
  diverges from independent ordered-map filtering.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/memtable.rs` and `crates/storage/src/bloom.rs`
  remain in use by current storage consumers.
- Follow-up: Immutable table building from frozen rows moves to L5E.

## M4-L5D: Raw Cursors And Merge Cursor

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5d-raw-cursors-merge-cursor-test-plan.md`
- `crates/storage-next/src/table/key.rs`
- `crates/storage-next/src/table/mutable.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/segment.rs`

### Behavior Preserved

- Raw cursors can seek to the first encoded internal key greater than or equal
  to a target key.
- Raw cursors can advance through sorted source rows without consuming the
  source.
- Merge cursors combine sorted child cursors into deterministic global
  encoded-key order.
- Equal encoded internal keys across sources are preserved and tie-broken by
  ascending source index.
- Merge uses a linear path for small source counts and a heap path above the
  old threshold of four sources.
- Re-seeking a merge cursor repositions child cursors instead of rebuilding
  source tables.

### Intentional V1 Changes

- L5D cursors operate on storage-next `TableRow` and `StorageRow` facts only.
  They do not port old `InternalKey`, `MemtableEntry`, `Key`, `Namespace`,
  `TypeTag`, or `Value` surfaces.
- Cursor `current()` and `current_key()` return `Option` for exhausted state.
  The old unwrap-style current access is not ported.
- Memory cursors borrow rows from `MutableTable` and `FrozenTable`; row payloads
  are not cloned during ordinary cursor movement.
- `BoundedTableCursor` applies `TableKeyBounds` mechanically over any raw
  cursor instead of adding source-specific range logic.
- Merge source-index ordering is deterministic only. It is not a visibility or
  source-age policy.

### Deferred

- Immutable table reader cursors move to L5F.
- Cache-aware cursor movement moves to L5G.
- Compaction policy over cursors moves to L5H.
- Object-backed table access moves to L5I.
- Latest-version selection, fork gates, inherited-layer reads, and branch-id
  rewriting move to L6/L7.
- Recovery, retention, WAL replay, and manifest installation stay outside L5.

### Tests Ported Or Added

- Add module-local cursor tests for empty and one-row state transitions,
  exhausted behavior, re-seek behavior, mutable/frozen parity, exact seeks,
  gap seeks, before-first seeks, and after-last seeks.
- Add bounded cursor tests for exact bounds, missing exact bounds, open and
  closed ranges, physical-prefix bounds, tombstones, expired-looking rows, and
  duplicate physical-key versions.
- Expand bounded cursor tests to cover unbounded ranges, lower-unbounded and
  upper-unbounded ranges, equal inclusive and equal exclusive bounds, and
  bounded `seek` repositioning.
- Add merge cursor tests for zero, one, disjoint, equal-key, source-index tie
  break, re-seek after partial consumption, re-seek after exhaustion, linear
  path, heap path, and the threshold boundary.
- Expand merge cursor tests to cover empty child cursors, raw tombstone and
  expired-row preservation, stable `current()` before `advance`, selected-child
  advance behavior, and 16-source heap shared-key ordering.
- Extend the hidden testkit table-runtime property route with generated raw
  cursor and merge checks against independent sorted-vector models.
- Expand generated L5D checks with single-empty, mixed-empty linear,
  mixed-empty heap, and 16-source heap scenarios.
- Extend `table_runtime_properties` to require L5D generated coverage under the
  `testkit` feature.
- Extend `table_runtime_source_guard` with cursor-policy vocabulary checks for
  MVCC, snapshot, fork, inherited-layer, rewrite, visibility, latest-row, and
  old memtable-entry leakage.

### Sensitivity Probes

- Add tests that fail if seek returns the previous row instead of the first
  greater-or-equal row.
- Add tests that fail if exhausted cursor state panics or changes after
  repeated advance.
- Add tests that fail if bounded cursors drop tombstones, expired-looking rows,
  or duplicate physical-key versions.
- Add tests that fail if merge drops duplicate exact keys across sources.
- Add tests that fail if equal-key merge ordering stops using source index.
- Add tests that fail if either the linear or heap merge path loses coverage.
- Add generated checks that fail if merge re-seek output diverges from the
  sorted-vector model.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/merge_iter.rs` and
  `crates/storage/src/seekable.rs` remain in use by current storage consumers.
- Follow-up: L5E can consume frozen/mutable cursor output as sorted builder
  input; L5F should implement the same `TableCursor` contract for immutable
  table readers.

## M4-L5E: Immutable Table Builder

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-test-plan.md`
- `crates/storage-next/src/format/table/artifact.rs`
- `crates/storage-next/src/table/builder.rs`
- `crates/storage-next/src/table/config.rs`
- `crates/storage-next/src/table/facts.rs`
- `crates/storage-next/src/table/key.rs`
- `crates/storage-next/src/table/mutable.rs`
- `crates/storage/src/segment_builder.rs`

### Behavior Preserved

- Sorted table rows can be built into immutable table bytes.
- Empty input is rejected.
- Unsorted input is rejected.
- Duplicate encoded internal keys are rejected.
- Tombstones, expired-looking rows, empty values, duplicate physical-key
  versions, commit timestamps, expiry timestamps, branch bytes, and
  storage-space ids are preserved as row facts.
- Builder compression config is applied mechanically.
- Output is deterministic for identical sorted rows and config.
- Built-table facts are derived from decoded byte-format facts.

### Intentional V1 Changes

- L5E uses the M3G `encode_immutable_table` path as the only byte writer. It
  does not port old `STRAKV` or segment-v7 bytes.
- L5E returns owned bytes and table facts only. It does not write files, rename
  temp files, fsync directories, construct object names, or publish objects.
- The builder is table-runtime local and crate-private. L4 owns durable table
  publication, and L6-L8 own branch placement and scheduling.
- M3G currently partitions data blocks by rows-per-block and records
  target-data-block size as a header fact. L5E does not add a separate
  byte-packing algorithm.
- Old durable bloom/filter blocks are not ported. Accelerators remain deferred
  to L5G.

### Deferred

- Immutable table readers move to L5F.
- Block cache and optional accelerators move to L5G.
- Generic compaction output splitting moves to L5H.
- Object-backed table access moves to L5I.
- Branch table installation, manifest mutation, flush scheduling, and recovery
  remain outside L5.

### Tests Ported Or Added

- Add module-local builder tests for construction, empty input rejection,
  unsorted input rejection, duplicate internal-key rejection, one-block output,
  multi-block output, row preservation, deterministic bytes, frozen-table input
  parity, compression paths, and decoded fact alignment.
- Extend the hidden testkit table-runtime property route with generated
  immutable-builder model checks.
- Extend `table_runtime_properties` to require L5E generated coverage under the
  `testkit` feature.
- Extend `table_runtime_source_guard` with old table-builder vocabulary checks
  for old segment builder and old table magic leakage.

### Sensitivity Probes

- Add tests that fail if the builder accepts empty rows, out-of-order rows, or
  duplicate encoded keys.
- Add tests that fail if row output differs after L3 decode.
- Add tests that fail if decoded header/properties facts drift from returned
  `TableRuntimeFacts`.
- Add tests that fail if changing rows-per-block no longer changes data-block
  count.
- Add tests that fail if repeated builds with the same input produce different
  bytes.
- Add source-guard probes proving old segment-builder names and old table magic
  cannot enter production table-runtime code.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/segment_builder.rs` remains in use by
  current storage consumers.
- Follow-up: L5F should read the M3G bytes produced by this builder through the
  immutable reader surface.

## M4-L5F: Immutable Table Reader

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/spec/strata-storage-format-v1.md`
- `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-test-plan.md`
- `crates/storage-next/src/format/table/artifact.rs`
- `crates/storage-next/src/table/reader.rs`
- `crates/storage-next/src/table/builder.rs`
- `crates/storage-next/src/table/cursor.rs`
- `crates/storage-next/src/table/facts.rs`
- `crates/storage-next/src/table/key.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/seekable.rs`

### Behavior Preserved

- Immutable table bytes can be opened and validated before reads are exposed.
- Exact encoded-internal-key lookup returns the matching raw row or `None`.
- Full-table, range, and physical-prefix cursors emit rows in encoded-key order.
- Cursor seek positions at the first row whose encoded key is greater than or
  equal to the target.
- Tombstones, expired-looking rows, empty values, duplicate physical-key
  versions, branch bytes, storage-space ids, timestamps, and values are
  preserved as row facts.
- Corrupt or truncated table bytes route through typed table-runtime decode
  errors with the underlying format error preserved as the source.
- Byte-backed and range-readable source-backed opens produce identical facts and
  rows for the same artifact.

### Intentional V1 Changes

- L5F reads only M3G table bytes through `decode_immutable_table`. It does not
  port old `STRAKV`/segment-v7 readers.
- The first reader implementation validates the full artifact and materializes
  rows at open. Lazy candidate-block reads remain API-compatible but deferred.
- Range-readable sources are L5-local byte abstractions. L5F does not import
  filesystem paths, file handles, backend traits, object names, or services.
- Reader lookup is exact and raw. It does not perform MVCC latest selection,
  inherited-layer lookup, branch rewriting, tombstone hiding, or TTL filtering.
- Block caches, bloom/filter accelerators, and shared reader-local memoization
  are deferred to L5G or a later reader optimization slice.

### Deferred

- Lazy candidate-block decode and source-read failures after open.
- Object-backed table access through L4/L5 handoff; L5I owns that adapter.
- Shared block-cache policy and optional accelerators; L5G owns cache behavior.
- Generic compaction over immutable-reader cursors; L5H owns compaction.
- Branch placement, level ownership, manifest mutation, recovery, and user read
  policy remain outside L5.

### Tests Ported Or Added

- Add module-local reader tests for byte-backed open, source-backed open,
  exact lookup, missing lookup, full/range/prefix cursors, zstd artifacts, row
  shape preservation, corrupt/truncated byte rejection, source read failure, and
  exact byte-source range behavior.
- Extend the hidden testkit table-runtime property route with generated
  immutable-reader model checks.
- Extend `table_runtime_properties` to require L5F generated coverage under the
  `testkit` feature.
- Reuse `table_runtime_source_guard` to prove the reader stays crate-private and
  does not import upper layers, backend/service APIs, filesystem APIs, process
  globals, old table-builder vocabulary, or product payload vocabulary.

### Sensitivity Probes

- Add tests that fail if reader facts drift from L5E-built artifact facts.
- Add tests that fail if byte-backed and source-backed reads diverge.
- Add tests that fail if exact lookup collapses duplicate physical-key versions
  or hides tombstones/expired-looking rows.
- Add tests that fail if cursor seek, range, or prefix filtering diverges from a
  sorted-vector model.
- Add tests that fail if corrupt or truncated bytes are accepted.
- Add source-guard probes that fail if L5 reader code reaches into paths,
  backend/service layers, old segment readers, or user-payload vocabulary.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/segment.rs` and
  `crates/storage/src/seekable.rs` remain in use by current storage consumers.
- Follow-up: L5G can add cache/accelerator mechanics over the reader surface;
  L5I can add the object-backed reader adapter.

## M4-L5G: Block Cache And Accelerators

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-test-plan.md`
- `crates/storage-next/src/table/cache.rs`
- `crates/storage-next/src/table/config.rs`
- `crates/storage-next/src/table/reader.rs`
- `crates/storage-next/src/table/mutable.rs`
- `crates/storage/src/block_cache.rs`
- `crates/storage/src/bloom.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/segment_builder.rs`

### Behavior Preserved

- Table cache instances support insert, lookup, duplicate insert handling,
  removal, table-wide removal, clear, resize, capacity accounting, and stats.
- Cache stats expose hit, miss, insert, duplicate-insert, eviction, removal,
  clear, skipped-disabled, skipped-oversized, entry, byte, and capacity facts.
- Cache identity includes table identity plus block address facts so blocks from
  different tables or different block kinds do not alias.
- Cache pressure evicts deterministic least-recent entries in the storage-next
  implementation.
- Bloom-style accelerators preserve the no-false-negative property for inserted
  L5 byte keys.
- Accelerators operate over L5 encoded key bytes and remain optional from a
  correctness perspective.

### Intentional V1 Changes

- The old lock-free/raw-pointer cache implementation is not ported. Storage-next
  keeps `#![deny(unsafe_code)]`, so L5G uses a safe database-owned cache.
- The old process-global cache is retired. Every cache is explicitly
  constructed and isolated.
- The old path-hash/cache-file-id identity is retired. L5G uses opaque
  table-cache ids and M3G block address facts.
- Old durable SST bloom/filter blocks are not ported. L5G adds only in-memory
  accelerators and leaves M3G footer filter fields unchanged.
- L5G does not force a lazy-reader rewrite. L5F remains correct with eager
  materialized rows; later slices can use the cache for candidate-block reads.

### Deferred

- Lazy data-block decode and cache-backed object range reads are deferred to the
  reader optimization or L5I object-backed handoff.
- Durable bloom/filter table bytes require an M3 format amendment and new
  golden vectors.
- Priority and pinned cache tiers are deferred until a caller needs them.
- Branch-local cache ownership, table lifecycle, and maintenance policy remain
  outside L5.

### Tests Ported Or Added

- Add module-local cache tests for key/address validation, disabled cache,
  insert/get, duplicate insert, remove, clear, capacity pressure, eviction,
  resize, table-wide removal, instance isolation, and stats.
- Add module-local bloom tests for empty filters, no false negatives,
  embedded-zero keys, duplicate keys, invalid configuration, and physical-key
  boundary probes.
- Extend the hidden testkit table-runtime property route with generated cache
  and bloom/filter cases.
- Extend `table_runtime_properties` to require L5G generated coverage under the
  `testkit` feature.
- Extend `table_runtime_source_guard` with checks for unsafe code and old cache
  identity vocabulary.

### Sensitivity Probes

- Add tests that fail if duplicate cache insert overwrites existing bytes.
- Add tests that fail if cache pressure exceeds capacity.
- Add tests that fail if removing one table removes another table's entry.
- Add tests that fail if two cache instances share state.
- Add tests that fail if an inserted bloom key returns definite absence.
- Add source-guard probes that fail if L5 cache code uses unsafe code,
  process-global cache ownership, or old path-hash/cache-file-id vocabulary.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/block_cache.rs` and
  `crates/storage/src/bloom.rs` remain in use by current storage consumers.
- Follow-up: L5H owns generic compaction; L5I can use the cache from an
  object-backed reader adapter.

## M4-L5H: Generic Table Compaction

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/M4/l5h-generic-compaction-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5h-generic-compaction-test-plan.md`
- `crates/storage-next/src/table/compaction.rs`
- `crates/storage-next/src/table/cursor.rs`
- `crates/storage-next/src/table/builder.rs`
- `crates/storage-next/src/table/key.rs`
- `crates/storage/src/compaction.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/merge_iter.rs`

### Behavior Preserved

- Compaction consumes sorted table rows and produces sorted immutable table
  artifacts.
- Keep-all compaction preserves all row facts byte-for-byte, including
  tombstones, expiry-looking timestamps, multiple versions, storage-space ids,
  branch bytes, commit timestamps, and values.
- Output is deterministic for the same rows, policy, and configuration.
- Output splitting occurs only between rows, never before the first row, and
  never creates empty artifacts.
- Output artifacts are built through the L5 immutable table builder and remain
  M3G table bytes.
- Compaction reports input rows, kept rows, dropped rows, output table count,
  output bytes, split count, and drop-reason summaries.

### Intentional V1 Changes

- Old pruning decisions are not embedded in L5. Version retention, tombstone
  elision, expiry dropping, and special row-family exemptions must be supplied
  by caller policy.
- Exact duplicate internal keys across input sources are rejected. L5 does not
  resolve duplicate priority by source order because priority is a higher-layer
  fact.
- The old filesystem-writing `SplittingSegmentBuilder` path is not ported. L5H
  buffers rows and returns in-memory `BuiltTableArtifact` values.
- Branch level selection, manifest install, old-table deletion, and reclaim
  handoff are not part of L5H.

### Deferred

- Streaming compaction output without buffering rows.
- Rate limiting and backpressure.
- Grandparent-overlap splitting and branch-level scoring.
- Caller-provided split boundaries. L5H currently splits only by the configured
  approximate row-size target while preserving physical-key groups; an explicit
  split-boundary API is deferred until L6/L7 can supply branch-level placement
  facts.
- Manifest publication and old-table retirement.
- Caller implementations for retention, tombstone, expiry, and row-family
  policy.

### Tests Ported Or Added

- Add module-local compaction tests for config validation, empty no-op,
  keep-all merge, policy-selected drops, source ordering, local and global
  duplicate rejection, output splitting, output limit errors, and cursor-backed
  source collection.
- Extend the hidden testkit table-runtime property route with generated
  compaction model checks.
- Extend `table_runtime_properties` to require L5H generated coverage under the
  `testkit` feature.
- Extend `table_runtime_source_guard` with checks that `table/compaction.rs`
  does not embed higher-layer retention-policy vocabulary.

### Sensitivity Probes

- Add tests that fail if keep-all compaction drops tombstones or expiry-looking
  rows.
- Add tests that fail if policy-selected drops affect unselected rows.
- Add tests that fail if duplicate exact internal keys are silently resolved.
- Add tests that fail if output table count exceeds the configured maximum.
- Add tests that fail if output artifacts do not decode through the M3G reader
  path.
- Add source-guard probes that fail if L5 compaction grows built-in pruning
  floors or level-position policy.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/compaction.rs`,
  `crates/storage/src/segmented/compaction.rs`, and
  `crates/storage/src/segment_builder.rs` remain in use by current storage
  consumers.
- Follow-up: L5I owns object-backed table access; L6/L8 own table selection,
  retention policy, manifest install, and lifecycle scheduling.

## M4-L5I: Object-Backed Table Access

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-test-plan.md`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/table/reader.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/block_cache.rs`

### Behavior Preserved

- Immutable table bytes can be read through positional/range reads from an
  external storage source.
- Opened object-backed table readers expose the same raw rows, exact lookup,
  cursor, and table facts as byte-backed readers.
- Corrupt table bytes remain table decode errors, while missing objects and
  backend range failures remain object-read errors.
- Table reads can use memory and local filesystem backends without production
  table code importing backend or filesystem APIs.

### Intentional V1 Changes

- Object names, table layout, and backend range reads live at the L4/service
  boundary. Production `src/table/` remains object-neutral.
- The old file/path-backed `KVSegment` reader is not ported. L5I adapts
  backend range reads into the existing L5 `TableByteSource` abstraction.
- The reader helper performs the boundary range read before opening from bytes
  so missing-object and interrupted-read causes remain object-read errors.
- The adapter reads a known table object from caller-supplied `TableObjectFacts`;
  it does not list prefixes, discover reachable tables, or infer branch state.
- Reading an already-published object requires `ReadRange`, not durable publish
  or durable sync capabilities.
- Backends that advertise `ObjectMetadata` validate the expected table-object
  byte count before the range read; weaker cache-mode backends may skip that
  preflight and rely on exact range-read plus M3G decode validation.
- Path-hash cache identity and process-global cache behavior remain retired.

### Deferred

- Lazy block reads after whole-object checksum validation.
- Cache-backed candidate-block reads.
- Durable object-store fences and conditional read validation.
- Branch table manifest integration and reachable table selection.
- Table object listing, deletion, retention, quarantine, and garbage
  collection.

### Tests Ported Or Added

- Add service-local tests for publish-then-read through the object-backed
  reader helper.
- Add memory-backend object-backed reader coverage without durable publication
  capabilities.
- Add object-backed byte-source tests for missing `ReadRange`, zero-length
  reads, exact range reads, long/short reads, overflow, and past-end rejection.
- Add fault tests for missing objects, interrupted range reads, short reads,
  corrupt table bytes, and stale table-object facts.
- Add stale byte-count tests for both actual-object-larger and
  actual-object-smaller metadata mismatches.
- Add object-backed versus byte-backed query parity tests for exact lookup,
  full cursor scans, closed bounds, physical-prefix bounds, zstd, multiblock
  tables, one-row tables, cache-enabled/cache-disabled reader configs, and
  mixed row shapes.
- Extend the generated table-runtime property harness with memory-backend
  object-backed reader parity cases over generated row shapes, compression
  modes, and reader configs.
- Add tests proving caller-supplied `TableIdentity` is preserved and not
  derived from object names.
- Add same-length corruption tests proving bad magic, bad footer CRC, and
  legacy table magic route through wrapped table decode errors.
- Extend localfs table-object tests to read the published object through the
  L5 reader helper.

### Sensitivity Probes

- Add tests that fail if object-backed reads list, write, delete, or publish
  during reader open.
- Add tests that prove `ObjectMetadata` is optional for weaker read-capable
  backends and used when available for stale byte-count detection.
- Add tests that fail if stale L4 table-object row-count, block-count, or
  commit-range facts are accepted.
- Add tests that fail if short backend reads are accepted.
- Add source-guard coverage for object layout literals including `tables/`,
  `wal/`, `snapshots/`, and `manifest`.
- Add tests that fail if memory backend reads require durable publish/sync.
- Keep source guards proving production `src/table/` does not import backend,
  service, layout, object, path, filesystem, or old segment-reader surfaces.

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/segment.rs` and
  `crates/storage/src/block_cache.rs` remain in use by current storage
  consumers.
- Follow-up: L5J should close out L5 conformance; L6 can consume the
  object-backed reader helper once branch table manifests exist.

## M4-L5J: L5 Conformance Closeout

### Current Files Read

- `docs/architecture/storage/l5-table-runtime.md`
- `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l5-table-runtime-test-plan.md`
- `docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-test-plan.md`
- `crates/storage-next/src/table/`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/testkit/table_runtime.rs`
- `crates/storage-next/tests/table_runtime_properties.rs`
- `crates/storage-next/tests/table_runtime_source_guard.rs`
- `crates/storage-next/fuzz/README.md`
- `crates/storage-next/fuzz/fuzz_targets/README.md`
- `crates/storage/src/{memtable,key_encoding,segment_builder,segment,index,bloom,block_cache,merge_iter,seekable,compaction}.rs`
- `crates/storage/src/segmented/{mod,compaction}.rs`

### Coverage Inventory

| Slice | Direct tests | Generated/property | Source guard | Fuzz/fuzz-adjacent | Status |
|---|---|---|---|---|---|
| L5A scaffold/config/facts/stats/errors | `src/table/tests/mod.rs` | `valid_config`, `invalid_config`, `valid_facts`, `invalid_facts`, `error_sources`, `stats` counters | public-surface and product-vocabulary guards | generated testkit route | Closed |
| L5B row/key adapters | `src/table/tests/key.rs` | `row_key_adapters`, `invalid_row_key_sequences`, `key_bounds`, `size_accounting` counters | product/cursor-policy guards | generated row/key scripts | Closed |
| L5C mutable/frozen tables | `src/table/tests/mutable.rs` | `mutable_frozen_tables` counter | product/cursor-policy guards | generated mutable/frozen scripts | Closed |
| L5D raw cursors and merge | `src/table/tests/cursor.rs` | `raw_cursors` counter | cursor-policy guard | `table_runtime_cursor` plus generated cursor and merge scripts | Closed |
| L5E immutable builder | `src/table/tests/builder.rs` | `immutable_builder_artifacts` counter | old-table-vocabulary guard | `format_table_artifact` fuzz target plus generated builder scripts | Closed |
| L5F immutable reader | `src/table/tests/reader.rs` | `immutable_table_readers` counter | filesystem/backend guard | `table_runtime_reader`, `format_table_artifact`, `format_table_block`, and generated reader scripts | Closed |
| L5G cache/accelerators | `src/table/tests/cache.rs` | `table_block_caches`, `table_bloom_filters` counters | old cache identity and global-state guards | generated cache/filter scripts | Closed |
| L5H generic compaction | `src/table/tests/compaction.rs` | `table_compactions` counter | compaction-policy guard | `table_runtime_compaction` plus generated compaction scripts | Closed |
| L5I object-backed access | `src/service/table.rs` tests | `object_backed_table_readers` counter | object-layout and backend-boundary guards | generated memory-backend object-backed scripts | Closed |

### Behavior Preserved

- Ordered mutable table mechanics from `memtable.rs`.
- Frozen read-only table views and raw row iteration.
- Encoded internal-key ordering from `key_encoding.rs`, retargeted to
  storage-next `StorageRow` and M3 key bytes.
- Raw point/range/prefix cursor movement from seekable table sources.
- K-way merge over sorted raw sources from `merge_iter.rs`.
- Immutable table builder and reader mechanics from `segment_builder.rs` and
  `segment.rs`, retargeted to M3G bytes.
- Range-readable external source behavior, retargeted from path/file reads to
  `TableByteSource` and L4 object-backed adapters.
- Cache eviction/stats mechanics from `block_cache.rs`, retargeted to
  database-owned table/block identities.
- Bloom/filter acceleration from `bloom.rs` as optional in-memory
  non-authoritative state.
- Generic sorted compaction mechanics from `compaction.rs`, retargeted to
  caller-supplied row-retention policy.

### Intentional V1 Changes

- L5 stores and compares storage-next `StorageRow` facts; it does not interpret
  product payloads.
- L5 emits and reads only M3G immutable table bytes. Old `STRAKV` bytes are
  rejection inputs only.
- L5 production code is object-neutral and backend-neutral. Object names and
  backend range reads live in L4/L5 boundary services.
- L5 caches use explicit table/block cache keys, not paths, file ids, or
  process-global cache identity.
- L5 compaction executes caller policy. It does not decide snapshot floors,
  branch retention, bottommost-level behavior, or expiry/tombstone safety.
- Runtime generated coverage runs through the `table_runtime_properties`
  route and the `table_runtime_reader`, `table_runtime_cursor`, and
  `table_runtime_compaction` fuzz targets; byte-level table fuzzing remains in
  `format_table_artifact` and `format_table_block`.

### Tests And Guards Strengthened

- Added detailed L5J implementation and test plans.
- Linked L5J from the main M4-L5 implementation plan.
- Added `table_runtime_closeout` integration tests for generated harness
  counters, source-guard category coverage, and registered table fuzz targets.
- Strengthened `table_runtime_source_guard` for additional filesystem/path
  APIs (`PathBuf`, `std::fs::File`, `pread`, `rename`, `remove_file`,
  `mmap`, `memmap`), direct object imports, testkit leakage, and higher-layer
  compaction-policy vocabulary.
- Documented the L5 fuzz/fuzz-adjacent split in the fuzz README files.
- Confirmed the generated table-runtime harness exposes nonzero counters for
  every L5 category, including object-backed table readers.

### Deferred Ledger

| Deferred behavior | Owner | Why not L5 | Current guard or test | First expected consumer |
|---|---|---|---|---|
| Branch table manifests and reachable table selection | L6 | Requires branch state and manifest semantics | L5 source guards reject branch/object ownership in `src/table/` | Branch LSM runtime |
| MVCC/latest-visible selection | L6 | Requires read timestamp and visibility policy | Cursor-policy guard rejects MVCC/latest vocabulary; raw cursor tests preserve all rows | Branch readers |
| Inherited table lookup and fork gates | L6 | Requires branch topology and fork metadata | Cursor-policy guard rejects fork/inherit/rewrite vocabulary | Branch inheritance |
| Flush scheduling and table install | L8 | Requires WAL durability, manifest publish, and lifecycle orchestration | Compaction/output tests produce artifacts but do not install them | Lifecycle runtime |
| Table retention and garbage collection | L8 | Requires durable reachability proofs and manifest coordination | Compaction-policy guard rejects retention/manifest/lifecycle vocabulary | Lifecycle retention |
| Table quarantine policy | L8 | Requires object mutation and operator recovery policy | Object-backed reads do not delete/list/quarantine | Lifecycle recovery |
| Checkpoint/table/WAL coordination | L8 | Requires cross-service scheduling and crash recovery | L5 has no WAL/checkpoint imports | Lifecycle checkpointing |
| Lazy block reads after whole-object validation | post-V1 | M3G footer checksum currently validates whole-object bytes | Reader tests validate full bytes/source parity | Reader optimization |
| Durable object-store fences and conditional reads | post-V1 | Requires object-store generation/fence design | L4 facts and metadata checks remain explicit; `PublishOutcome` fence is deferred | Object backend evolution |
| Durable filter blocks | post-V1/spec update | Requires M3G format extension | Cache/filter tests prove accelerators are optional and non-authoritative | Format V2 or M3G extension |

### Verification Command Set

Mandatory L5J closeout commands:

```sh
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --locked --lib testkit::table_runtime
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional/manual commands:

```sh
cargo test -p strata-storage-next --features testkit,localfs --locked
cd crates/storage-next/fuzz && cargo +nightly fuzz run format_table_artifact -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run format_table_block -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_reader -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_cursor -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_compaction -- -max_total_time=60
```

### Retirement

- Deleted: none.
- Retired from storage-next L5: old `STRAKV` table bytes, path-backed
  `KVSegment`, `pread`/file-handle table access, path-hash cache identity,
  process-global table cache, product `Value`/`Key`/DTO payload semantics, and
  MessagePack table payloads.
- Legacy-retained: old `crates/storage` table modules remain in use by current
  storage consumers until the storage-next stack replaces them.
- Follow-up: M4-L6 can start branch table state on top of L5 mechanics without
  adding new L5 conformance work.

## M4P-L5: Table Runtime Parity Closeout

### Current Files Read

- `docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-implementation-plan.md`
- `docs/architecture/implementation-plans/M4P/m4p-l5-table-runtime-parity-test-plan.md`
- `docs/architecture/storage/l5-table-runtime.md`
- `crates/storage-next/src/table/`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/testkit/table_runtime.rs`
- `crates/storage-next/src/testkit/table_runtime/{builder_reader,cache,bloom_compaction,helpers,outcome_contracts}.rs`
- `crates/storage-next/tests/table_runtime_properties.rs`
- `crates/storage-next/tests/table_runtime_closeout.rs`
- `crates/storage-next/tests/table_runtime_source_guard.rs`
- `crates/storage/src/{segment,index,bloom,block_cache,merge_iter,seekable,compaction}.rs`

### Behavior Preserved

- Lazy table open remains a table-local reader mode with metadata and index
  loaded while data blocks and rows stay unloaded until point/range access.
- Point lookup uses L5 table facts, index entries, optional filter probes, and
  candidate data-block reads without requiring branch or MVCC policy.
- Range and prefix readers use the normal bounded cursor machinery and compare
  to the eager sorted-row model for correctness.
- Cache checks preserve hit, miss, eviction, duplicate-insert, table-removal,
  disabled, and oversized-entry behavior from the old block-cache mechanics.
- Bloom/filter checks preserve non-authoritative acceleration: no false
  negatives for generated keys, definitely-absent probes may skip data blocks,
  and maybe-present/false-positive paths still require table validation.
- Table compaction remains streaming/cursor-oriented and policy-injected, with
  output artifacts validated as normal M3G tables.
- Object-backed reader tests continue to prove L4 service handoff into the lazy
  L5 reader without moving object names, layout, or backend reads into
  production `src/table/`.

### Intentional V1 Changes

- Durable filter blocks remain deferred. Runtime filters may be supplied only
  when they match canonical table bytes and exact content proof; L5 does not
  extend M3G bytes in this parity slice.
- The generated closeout coverage is expressed as table-runtime outcome
  counters rather than benchmark-specific fast paths.
- The M4P closeout entry coexists with the older M4-L5 slice history because
  M4P restored performance-shape parity after the original architecture audit.

### Deferred

- L6 still owns branch table manifests, source pruning, MVCC/latest-visible
  source selection, inherited table lookup, and branch-level scan planning.
- L8 still owns table install, compaction scheduling, retention, quarantine,
  checkpoint coordination, and maintenance policy.
- Durable object-backed filter blocks still require an L3 table-format
  amendment before L5 can read filter bytes from table objects.
- Any remaining L9 benchmark gap after L5 counters pass should be diagnosed in
  L6 source fanout or L8 maintenance/compaction, not by adding L5 fast paths.

### Tests Ported Or Added

- Extended `TableRuntimeScaffoldOutcome` with generated closeout counters for
  lazy reader opens, lazy point hits, lazy point misses, lazy range cursors,
  cache hits, cache misses, filter available/absent/negative/false-positive
  paths, streaming compaction outputs, and object-backed reader parity.
- Added generated lazy-reader checks that assert lazy open runtime facts before
  materialization, then compare point-hit, point-miss, and bounded range-cursor
  results to the generated sorted model.
- Added generated unavailable-filter proof alongside the available supplied
  filter path.
- Added deterministic generated bloom false-positive coverage without treating
  the filter as authoritative.
- Extended `table_runtime_properties` to require every closeout counter to be
  nonzero in generated runs.
- Extended `table_runtime_closeout` to inventory the closeout counters and
  existing source-guard categories.
- Extended `table_runtime_closeout` to require the perf closeout report,
  referenced benchmark result files, and this M4P-L5 porting-log entry.
- Re-ran the source guard suite confirming production `src/table/` still
  rejects backend/object/layout/service imports, old `KVSegment`/`STRAKV`/
  path-hash/global-cache vocabulary, and branch/MVCC/retention policy terms in
  table compaction.

### Benchmark And Perf Evidence

- Closeout report:
  `docs/architecture/perf-tuning/m4p-l5-table-runtime-parity-closeout.md`.
- Required benchmark commands cover L9 100K and 1M point-throughput and
  range/prefix scans for `cache` and `standard` engines.
- Benchmark output is recorded in the closeout report, including any remaining
  stop-condition diagnosis when table-local counters pass but L9 throughput is
  still bottlenecked above L5.

### Retirement

- Deleted: none.
- Legacy-retained: old `crates/storage` table files remain until storage-next
  replaces current storage consumers.
- Follow-up: start L6 source-shape work only after the L5 closeout report shows
  table-local lazy point/range/cache/filter counters are passing and benchmark
  gaps are attributable above L5.
