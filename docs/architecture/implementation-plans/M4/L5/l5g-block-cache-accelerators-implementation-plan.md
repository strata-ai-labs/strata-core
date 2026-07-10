# L5G Implementation Plan: Block Cache And Accelerators

Status: draft implementation plan

Parent plans:

1. `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
4. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-test-plan.md`

## Goal

Port table read acceleration into storage-next L5 without reintroducing the old
storage engine boundary violations.

L5G must provide:

1. a database-owned table block cache;
2. stable cache keys that do not depend on filesystem paths;
3. deterministic cache admission, lookup, eviction, and stats;
4. optional in-memory read accelerators such as bloom filters;
5. correctness-preserving integration points for mutable, frozen, and immutable
   table readers;
6. source guards proving that no process-global cache, backend, object-layout,
   branch, visibility, or product-payload behavior enters L5.

L5G is an acceleration slice. It must not change the meaning of any table read.
Disabling the cache and accelerators must leave point, range, prefix, cursor,
and merge behavior unchanged.

## Inputs

1. `docs/architecture/storage/l5-table-runtime.md`
2. `docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md`
4. `docs/architecture/implementation-plans/M4/l5c-mutable-frozen-tables-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/l5f-immutable-table-reader-implementation-plan.md`
6. `crates/storage/src/block_cache.rs`
7. `crates/storage/src/bloom.rs`
8. `crates/storage/src/memtable.rs`
9. `crates/storage/src/segment.rs`
10. `crates/storage/src/segment_builder.rs`
11. `crates/storage-next/src/table/cache.rs`
12. `crates/storage-next/src/table/config.rs`
13. `crates/storage-next/src/table/reader.rs`
14. `crates/storage-next/src/table/mutable.rs`
15. `crates/storage-next/src/table/key.rs`
16. `crates/storage-next/src/format/table/`

## Existing-Code Source Map

| Current file | Relevant evidence | L5G porting rule |
|---|---|---|
| `crates/storage/src/block_cache.rs` | Cache lookup/insert/duplicate handling, capacity accounting, priority tiers, pinned entries, stats, and CLOCK-style eviction tests. | Reuse behavioral cases and vocabulary only where it remains L5-local. Do not port `unsafe`, process globals, file-path hashes, or file-id-only cache keys. |
| `crates/storage/src/bloom.rs` | Cache-local blocked bloom filter with serialization and no-false-negative tests. | Reuse the in-memory algorithm if useful. Do not add durable filter bytes unless the M3G spec is explicitly extended. |
| `crates/storage/src/memtable.rs` | Frozen memtables lazily build a bloom filter for absent-key probes. | Port only optional absent-key acceleration. Do not port MVCC snapshot filtering or product `Value` behavior. |
| `crates/storage/src/segment.rs` | Table reader consults bloom partitions and a block cache before loading data blocks. | Preserve the correctness rule: accelerators may skip only when they prove absence, and corrupt/missing optional accelerators must not make present rows disappear. Replace `pread`, file handles, global cache, and path-hash identity. |
| `crates/storage/src/segment_builder.rs` | Old SST builder wrote durable bloom/filter index blocks. | Do not port durable filter blocks for M3G. V1 table footer filter fields remain zero until the format spec is amended. |
| `crates/storage-next/src/table/config.rs` | `TableCacheConfig` already carries enabled/capacity fields. | Reuse as the owner-provided cache budget and extend only if tests prove additional policy knobs are needed. |
| `crates/storage-next/src/table/reader.rs` | L5F currently validates full M3G bytes and materializes rows. | Add cache-compatible APIs without requiring lazy block decode in this slice. Lazy candidate-block reads remain a follow-up unless L5G explicitly implements them. |

## Scope

L5G implements:

1. `table/cache.rs` as a real L5 module;
2. `TableBlockCache` with explicit construction from `TableCacheConfig`;
3. `TableBlockCacheKey` using opaque table identity plus block address facts;
4. `TableBlockAddress` for data, index, properties, and accelerator blocks;
5. `TableBlockPriority` for data versus metadata/accelerator entries, if
   needed by the eviction policy;
6. `TableBlockCacheStats` with hits, misses, inserts, duplicate inserts,
   per-entry removes, table invalidations, clears, evictions, entries, bytes,
   capacity, and skipped oversized/disabled entries;
7. a safe deterministic eviction policy suitable for M4 tests;
8. optional `TableBloomFilter` or equivalent non-authoritative accelerator over
   L5 byte keys;
9. frozen-table absent-key accelerator hooks, if they stay purely optional;
10. immutable-reader cache hooks that are API-compatible with later lazy
    data-block decode;
11. generated testkit coverage and source guards;
12. a porting-log entry for old block-cache and bloom behavior.

L5G does not implement:

1. process-global cache state;
2. `unsafe` lock-free cache internals;
3. path hashing, file handles, `pread`, mmap, or backend calls;
4. object names or durable publication;
5. durable bloom/filter blocks in M3G;
6. branch-local table placement or level ownership;
7. snapshot/as-of/latest-visible filtering;
8. tombstone hiding, TTL expiry filtering, or retention policy;
9. object-backed table source integration; L5I owns the L4/L5 object handoff;
10. user-tunable public API; L9 owns public storage API exposure.

## Design Decisions

### Database-Owned Cache

The V1 cache owner is the database runtime, not the process. The cache must be
constructed explicitly and passed to the L5 components that may use it.

Rules:

1. `TableBlockCache::new(config: TableCacheConfig)` creates an independent
   cache instance.
2. `TableBlockCache::disabled()` or `TableCacheConfig::new(false, 0)` produces
   a cache that records misses/skips but stores nothing.
3. Two cache instances with the same table and block keys do not share state.
4. No `static`, `lazy_static`, `OnceLock`, `OnceCell`, or mutable global cache
   appears in L5 production code.
5. Cache capacity comes only from resolved configuration. L5 does not inspect
   host memory, device class, environment variables, or process-wide settings.

### No Unsafe Port

The old cache is lock-free and uses raw pointers plus manual atomic lifetime
management. Storage-next has `#![deny(unsafe_code)]`, so L5G must not port that
implementation literally.

The M4 implementation should prefer a safe baseline:

1. `std::sync::Mutex` or another safe synchronization primitive around cache
   state;
2. deterministic map/deque bookkeeping;
3. simple LRU or CLOCK-like behavior implemented without raw pointers;
4. tests that lock down externally visible behavior rather than old internal
   atomics.

If a later performance pass wants a lock-free cache, it must first change the
crate safety policy deliberately. L5G should not be that change.

### Cache Keys

Old storage keys cache blocks by `(file_path_hash, block_offset)`. Storage-next
must use table-runtime facts instead.

Suggested shape:

```text
TableBlockCacheKey {
    table: TableCacheTableId,
    address: TableBlockAddress,
}

TableCacheTableId {
    bytes: Vec<u8>,
}

TableBlockAddress {
    kind: TableBlockKindForCache,
    offset: u64,
    length: u32,
    ordinal: Option<u32>,
}
```

Rules:

1. `TableCacheTableId` is opaque to L5. It may be derived from
   `TableIdentity` in tests and from a future L4/L6 table-object fence in L5I.
2. Cache identity must include enough table-generation information to prevent
   two different table objects with the same display identity from aliasing.
   Until L5I provides object fences, tests should use unique `TableIdentity`
   values or explicit synthetic cache ids.
3. Cache keys must not contain filesystem paths, backend object names, branch
   ids as policy facts, or product-level identifiers.
4. `TableBlockAddress` may use block offsets and frame lengths because those
   are M3G table facts, not filesystem facts.
5. Data blocks, index blocks, properties blocks, and in-memory accelerator
   entries must not collide.

### Cache Values

The cache should initially store validated table-local payloads without owning
format parsing:

```text
TableCachedBlock {
    bytes: Arc<[u8]>,
    charge_bytes: usize,
}
```

This keeps the cache generic and avoids storing product rows or visibility
state. Reader code may later choose to cache decompressed data-block payloads or
decoded row slices, but the cache key and eviction policy must not depend on
that choice.

Rules:

1. Cache values are immutable after insertion.
2. Duplicate insert for an existing key returns or preserves the existing
   value deterministically.
3. Oversized values larger than capacity are returned uncached and counted as
   skipped.
4. Zero-length values are rejected or skipped consistently. The plan prefers
   rejecting them with `InvalidRange { field: "cache_charge" }` because M3G
   blocks are nonempty.
5. Cache charge is explicit and must not undercount the stored bytes.

### Eviction Policy

Use a deterministic safe policy for M4. A straightforward LRU is acceptable:

1. cache hit moves the key to most-recent;
2. insert places the key at most-recent;
3. eviction removes least-recent entries until the new entry fits;
4. disabled cache never stores;
5. if every remaining entry is pinned or protected, the incoming entry is
   skipped rather than exceeding capacity.

Priority tiers are optional in M4. If implemented, they must remain mechanical:

1. data blocks are low priority;
2. index/properties/accelerator entries may be high priority;
3. pinned entries may survive ordinary pressure but must still be removable by
   `clear`, `remove_table`, or `resize(0)`;
4. priority must not encode branch, visibility, or product meaning.

### Statistics

Stats are part of the cache contract because later L6/L8 need observability and
tests need a way to prove cache behavior.

Suggested stats:

```text
TableBlockCacheStats {
    hits: u64,
    misses: u64,
    inserts: u64,
    duplicate_inserts: u64,
    evictions: u64,
    removes: u64,
    table_invalidations: u64,
    clears: u64,
    skipped_oversized: u64,
    skipped_disabled: u64,
    entries: usize,
    bytes: usize,
    capacity_bytes: usize,
}
```

Stats should be monotonic except for current gauges (`entries`, `bytes`,
`capacity_bytes`). Reads of stats must not mutate cache state.

### Accelerators

L5G may add an in-memory bloom/filter helper, but it must be optional and
non-authoritative.

Suggested shape:

```text
TableBloomFilter::build(keys: impl Iterator<Item = &[u8]>, bits_per_key: usize)
TableBloomFilter::might_contain(key: &[u8]) -> TableBloomProbe

TableBloomProbe::DefinitelyAbsent
TableBloomProbe::MaybePresent
TableBloomProbe::Unavailable
```

Rules:

1. The accelerator may produce false positives.
2. The accelerator must not produce false negatives for keys it was built with.
3. Missing, disabled, corrupt, or unsupported accelerator state must route to
   `Unavailable` or `MaybePresent`, never to `DefinitelyAbsent`.
4. Accelerators operate over encoded L5 key bytes, usually
   `TablePhysicalKeyBytes` for absent-key probes.
5. Accelerators do not hide tombstones, expired rows, or older versions.
6. Accelerators do not become durable M3G bytes in L5G.

### Integration Points

L5G should integrate conservatively.

Initial integration should be one or more of:

1. `FrozenTable` can lazily build an optional bloom filter for exact physical
   key absence checks, while keeping exact lookup authoritative in the ordered
   rows.
2. `ImmutableTableReader` can accept an optional cache handle and cache id, but
   current eager/materialized row reads remain correct if the cache is absent.
3. Reader stats can report cache-independent row reads now and leave candidate
   block read savings to the later lazy-reader follow-up.

Do not force L5G to implement lazy candidate-block decoding if it would expand
the slice into a second reader rewrite. It is acceptable for L5G to provide the
cache and accelerator primitives plus API-compatible hooks, then let L5I or a
later reader optimization use them for object-backed range reads.

## Proposed Type Surface

Names may change if the implementation discovers better local conventions, but
the responsibilities should remain stable.

### Cache Config

Reuse:

```text
TableCacheConfig::new(enabled: bool, capacity_bytes: usize)
```

Add only if needed:

```text
TableCacheConfig::with_policy(...)
```

Avoid adding policy knobs until tests prove they are necessary.

### Cache Key Types

```text
TableCacheTableId::new(bytes: impl Into<Vec<u8>>) -> TableRuntimeResult<Self>
TableBlockAddress::new(kind, offset, length, ordinal) -> TableRuntimeResult<Self>
TableBlockCacheKey::new(table, address) -> Self
```

Validation:

1. table id must be nonempty and bounded;
2. offset + length must not overflow;
3. length must be nonzero;
4. ordinal, if present, is descriptive and must not replace offset/length for
   identity unless the address kind is explicitly ordinal-only.

### Cache

```text
TableBlockCache::new(config: TableCacheConfig) -> TableRuntimeResult<Self>
TableBlockCache::disabled() -> Self
TableBlockCache::get(&self, key: &TableBlockCacheKey) -> Option<Arc<[u8]>>
TableBlockCache::insert(&self, key: TableBlockCacheKey, bytes: Arc<[u8]>) -> CacheInsert
TableBlockCache::remove(&self, key: &TableBlockCacheKey) -> bool
TableBlockCache::remove_table(&self, table: &TableCacheTableId) -> usize
TableBlockCache::clear(&self)
TableBlockCache::resize(&self, capacity_bytes: usize)
TableBlockCache::stats(&self) -> TableBlockCacheStats
```

`CacheInsert` should distinguish:

1. inserted;
2. duplicate existing;
3. skipped because disabled;
4. skipped because oversized;
5. skipped because capacity is pinned/protected.

### Accelerator

```text
TableBloomFilter::build(keys, bits_per_key) -> TableRuntimeResult<Self>
TableBloomFilter::might_contain(key: &[u8]) -> TableBloomProbe
TableBloomFilter::is_empty(&self) -> bool
TableBloomFilter::approximate_size_bytes(&self) -> usize
```

No durable serialization should be exposed in L5G unless M3G is amended.
Serialization helpers from old storage are historical evidence only.

## Implementation Steps

### L5G-A: Source Audit And Porting Log

1. Read `block_cache.rs`, `bloom.rs`, and their call sites in old storage.
2. Record which behaviors are retained, rewritten, deferred, or retired in
   `m4-l5-porting-log.md`.
3. Explicitly record that the old process-global cache, path-hash key, and
   `unsafe` internals are retired.

### L5G-B: Cache Key And Stats Types

1. Implement key/address/table-id types in `table/cache.rs`.
2. Add bounded display/debug output for keys and stats.
3. Add validation for empty ids, zero lengths, and range overflow.
4. Re-export crate-private cache types from `table/mod.rs`.

### L5G-C: Safe Cache Core

1. Implement disabled and enabled cache construction.
2. Implement get/insert/remove/clear/resize/stats.
3. Implement deterministic eviction.
4. Ensure duplicate inserts are non-mutating or explicitly documented.
5. Ensure all operations are safe Rust.

### L5G-D: Non-Authoritative Bloom Filter

1. Implement an in-memory bloom/filter helper over byte keys.
2. Clamp probe counts and allocation sizes.
3. Guarantee no false negatives for inserted keys.
4. Return conservative results for empty or unavailable filter state.
5. Keep M3G footer filter fields unchanged.

### L5G-E: Optional Table Hooks

1. Add optional frozen-table bloom construction if it remains purely
   mechanical.
2. Add optional immutable-reader cache handle types only if the current reader
   API can do so without lazy-block churn.
3. Do not make cache state required for any read path.
4. Update stats only for behavior that actually exists in L5G.

### L5G-F: Testkit And Guards

1. Extend `table_runtime_properties` or a neighboring testkit route with cache
   and accelerator counters.
2. Extend source guards for process globals, unsafe code, filesystem paths,
   backend imports, old file-id/path-hash vocabulary, and product payload
   vocabulary.
3. Run default, no-default, and wasm test lanes.

## Format Policy

L5G must not change durable M3G bytes.

Rules:

1. Do not add durable bloom/filter blocks.
2. Do not set footer filter offset/length.
3. Do not accept old `STRAKV` filter/index bytes.
4. Do not duplicate L3 table parsing.
5. Any future durable filter format requires a separate M3 spec amendment and
   new golden vectors.

## Safety And Boundary Checklist

Before implementation is complete:

1. No `unsafe` appears in `crates/storage-next/src/table/`.
2. No static cache owner appears in production code.
3. No `std::fs`, `std::path`, platform filesystem traits, backend imports, or
   service imports appear in L5 cache/accelerator code.
4. No branch, lifecycle, commit runtime, engine, product payload, MessagePack,
   bincode, snapshot, as-of, latest, TTL filtering, or tombstone hiding
   vocabulary appears in production cache/accelerator code.
5. Cache disabled behavior is tested and correct.
6. Cache enabled behavior is deterministic and never changes row results.
7. Accelerator absent result is used only to skip authoritative work when
   absence is proven by a no-false-negative structure.
8. Missing/corrupt/unavailable accelerators fall back conservatively.

## Exit Criteria

L5G is complete when:

1. L5 has a database-owned block cache with stable, non-path cache keys;
2. cache get/insert/eviction/clear/resize/stats are covered by direct tests;
3. cache instances are isolated from each other;
4. disabling cache produces identical table read results;
5. enabling cache produces identical table read results and deterministic
   stats;
6. optional bloom/filter accelerators have no false negatives and are
   non-authoritative;
7. M3G durable bytes remain unchanged;
8. generated tests include cache and accelerator cases;
9. source guards reject process globals, old cache identity, unsafe code, upper
   layer imports, and product vocabulary;
10. the porting log records retained, rewritten, retired, and deferred old
    cache/filter behavior.
