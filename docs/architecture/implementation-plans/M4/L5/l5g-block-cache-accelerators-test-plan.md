# L5G Test Plan: Block Cache And Accelerators

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5g-block-cache-accelerators-implementation-plan.md`

## Goal

Prove that L5G accelerates table reads without changing table semantics or
leaking old storage engine boundaries.

The suite must fail if L5G:

1. creates process-global cache state;
2. uses filesystem paths, path hashes, file descriptors, backend APIs, or
   object names as L5 cache identity;
3. uses `unsafe`;
4. aliases blocks from different tables;
5. returns stale bytes after replacement, removal, resize, or clear;
6. exceeds capacity without a documented pinned/protected reason;
7. misreports hit/miss/eviction/byte stats;
8. makes table reads depend on cache being enabled;
9. makes a bloom/filter false negative for an inserted key;
10. treats corrupt/missing accelerators as authoritative absence;
11. filters tombstones, expired-looking rows, or old versions;
12. changes M3G durable table bytes.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/cache.rs` for module-local cache tests.
2. `crates/storage-next/src/table/tests/accelerator.rs` or
   `crates/storage-next/src/table/tests/cache.rs` for bloom/filter tests.
3. `crates/storage-next/src/testkit/table_runtime.rs` for generated cache and
   accelerator model checks.
4. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5G
   property tests behind the `testkit` feature.
5. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
6. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old block-cache and bloom/filter porting record.

Tests should use storage-next L5 keys, rows, and table identities. Old
`crates/storage` code is evidence only and must not be used as a runtime oracle.

## Reference Model

### Cache Model

Use an independent model for cache behavior:

```text
model_capacity = configured capacity bytes
model_entries = ordered map from cache key to bytes and charge
model_recency = least-recent to most-recent keys

get(key):
  if key exists:
    hit, return bytes, move key to most-recent
  else:
    miss, return None

insert(key, bytes):
  if disabled: skipped_disabled
  else if bytes.len > capacity: skipped_oversized
  else if key exists: duplicate_existing
  else evict least-recent until bytes fit, insert most-recent
```

If the implementation chooses CLOCK rather than LRU, generated tests should
model only stable contractual facts:

1. no stale bytes;
2. capacity not exceeded;
3. hot entries are not guaranteed unless the contract says so;
4. eviction count and entry/byte gauges remain coherent.

### Accelerator Model

For bloom/filter accelerators:

```text
inserted_keys = exact set of byte keys used to build the filter

for key in inserted_keys:
  filter.might_contain(key) != DefinitelyAbsent

for key not in inserted_keys:
  MaybePresent or DefinitelyAbsent are both allowed

for missing/corrupt/unavailable filter:
  result is MaybePresent or Unavailable
```

Accelerators must never be used as the only proof that a present row is absent.

## Required Unit Tests

### 1. Cache Key And Address Validation

1. Empty table cache id is rejected.
2. Oversized table cache id is rejected or display-bounded.
3. Table ids with equal bytes compare equal.
4. Different table ids never compare equal.
5. Data, index, properties, and accelerator address kinds do not collide.
6. Same offset with different lengths does not collide.
7. Same ordinal with different offset does not collide unless the address kind
   is explicitly ordinal-only.
8. Offset plus length overflow is rejected.
9. Zero block length is rejected.
10. Debug/display output is bounded for long table ids.
11. Cache keys contain no path separator assumptions.
12. Cache keys can be constructed from synthetic `TableIdentity` values for
    tests without importing L4 layout.

### 2. Disabled Cache

1. Disabled cache stores no entries.
2. Disabled cache `get` returns `None`.
3. Disabled cache insert returns `SkippedDisabled`.
4. Disabled cache increments skipped or miss stats as specified.
5. Disabled cache stats report zero entries and zero bytes.
6. Disabled cache clear and resize do not panic.
7. A disabled cache and no cache produce identical table read outputs.

### 3. Basic Cache Operations

1. Insert then get returns the inserted bytes.
2. Miss returns `None`.
3. Hit increments hit count.
4. Miss increments miss count.
5. Insert increments insert count only for stored entries.
6. Duplicate insert for the same key returns or preserves the existing bytes.
7. Duplicate insert does not double-count bytes.
8. Two keys in the same table with different block addresses are independent.
9. Two tables with the same block address are independent.
10. Remove returns true when an entry existed.
11. Remove returns false when an entry did not exist.
12. Removed entry cannot be read.
13. Clear removes all entries.
14. Clear resets entry and byte gauges without resetting monotonic counters
    unless the implementation explicitly documents otherwise.
15. Stats reads do not mutate recency or counters.

### 4. Capacity And Eviction

1. Cache with capacity zero stores nothing.
2. Entry larger than capacity is skipped and returned uncached.
3. Entry exactly equal to capacity is admitted.
4. Inserting entries past capacity evicts enough entries to fit.
5. Current byte gauge never exceeds capacity, except for explicitly pinned
   behavior if implemented.
6. Eviction count increases when entries are evicted.
7. Evicted entries miss after eviction.
8. Non-evicted entries retain exact bytes.
9. Hit updates recency for LRU policy.
10. Re-inserting after eviction works.
11. Resizing downward evicts or marks pressure so subsequent operations bring
    bytes under capacity.
12. Resizing to zero removes or prevents all cached entries.
13. Resizing upward admits later entries.
14. Pinned entries, if implemented, survive ordinary pressure.
15. Pinned entries, if implemented, are removable by `clear`, `remove_table`,
    or resize-to-zero.

### 5. Cache Isolation

1. Two `TableBlockCache` instances do not share entries.
2. Stats are per instance.
3. Clearing one cache does not clear another.
4. Resizing one cache does not resize another.
5. A table id reused in two instances does not alias.
6. No process-global cache survives between tests.

### 6. Cache Concurrency

Run these only where native threads are available.

1. Concurrent gets of an existing key return identical bytes.
2. Concurrent duplicate inserts for one key leave one logical entry.
3. Concurrent inserts for disjoint keys keep stats coherent.
4. Concurrent get/insert/remove does not panic.
5. Concurrent clear plus reads does not expose partial or invalid bytes.
6. Final gauges remain within capacity.

These tests must not assert old lock-free internals. They assert safe
externally visible behavior only.

### 7. Cache Integration With Table Reads

If L5G adds reader or frozen-table cache hooks, test:

1. Cache disabled reader output equals ordinary reader output.
2. Cache enabled reader output equals ordinary reader output.
3. Exact lookup present row returns the same row with and without cache.
4. Exact lookup missing row returns `None` with and without cache.
5. Full cursor output is identical with and without cache.
6. Range cursor output is identical with and without cache.
7. Prefix cursor output is identical with and without cache.
8. Tombstones are returned with and without cache.
9. Expired-looking rows are returned with and without cache.
10. Multiple versions for one physical key are returned with and without cache.
11. Zstd table output is identical with and without cache.
12. Corrupt table bytes still fail even when cache has a key collision-shaped
    entry from another table.
13. Removing a table from cache does not affect reader correctness.
14. Cache stats show the expected hit/miss pattern only for behavior that L5G
    actually implements.

If L5G does not yet connect cache to immutable-reader block reads because L5F
is still eager/materialized, these integration cases should be marked deferred
to the lazy-reader or L5I follow-up. The cache primitives still need full
direct tests.

### 8. Bloom/Filter Construction

1. Empty key set creates an empty or always-conservative filter.
2. Single key creates a filter with no false negative.
3. Many keys create a filter with no false negatives.
4. Duplicate keys do not break construction.
5. Embedded-zero key bytes work.
6. Very long key bytes are bounded or accepted without panic.
7. `bits_per_key = 0` is rejected or clamped explicitly.
8. Probe count is bounded.
9. Allocation size is bounded.
10. Approximate size accounting is deterministic.
11. Build is deterministic for the same keys and config.
12. Different key sets usually produce different filter state, without relying
    on that for correctness.

### 9. Bloom/Filter Probe Semantics

1. Every inserted key returns `MaybePresent`.
2. Absent keys may return either `DefinitelyAbsent` or `MaybePresent`.
3. Empty filter returns `DefinitelyAbsent` only when it represents a proven
   empty key set.
4. Unavailable filter returns `Unavailable` or `MaybePresent`.
5. Corrupt filter state, if a corrupt state can be represented, does not return
   `DefinitelyAbsent`.
6. Probe does not inspect commit timestamp.
7. Probe does not inspect expiry timestamp.
8. Probe does not hide tombstones.
9. Probe does not collapse physical-key versions.
10. Probe over physical-key bytes does not cross branch-id bytes.
11. Probe over physical-key bytes does not cross storage-space-id bytes.
12. Probe over physical-key bytes excludes prefix-like user-key neighbors unless
    the filter returns an allowed false positive.

### 10. Frozen Table Accelerator Hooks

If L5G accelerates frozen tables:

1. Frozen table exact lookup present row returns the same row before and after
   accelerator construction.
2. Frozen table exact lookup absent row may avoid authoritative search only
   when the accelerator says `DefinitelyAbsent`.
3. First lookup may build the accelerator lazily.
4. Subsequent lookups reuse the same frozen-table-local accelerator.
5. Accelerator construction does not mutate row order.
6. Accelerator construction does not change memory facts except documented
   accelerator memory accounting.
7. Frozen table remains correct if accelerator construction is disabled.
8. Frozen table remains correct if accelerator reports `MaybePresent` for all
   keys.

### 11. Durable Format Non-Regression

1. L5E-built table footer filter offset remains zero.
2. L5E-built table footer filter length remains zero.
3. Golden M3G table vectors remain unchanged.
4. L5G does not add table-format golden vectors unless the format spec is
   deliberately amended.
5. L5G does not accept old `STRAKV` filter/index bytes as M3G.
6. L5G does not serialize bloom filters into table artifact bytes.

### 12. Error Routing

1. Invalid cache config returns `InvalidConfig`.
2. Invalid cache key/address returns `InvalidRange` or `InvalidConfig`
   consistently.
3. Oversized cache entry is not a table corruption error.
4. Accelerator construction allocation-bound failure is typed and bounded.
5. Cache stats/display for large keys is bounded.
6. No product error type appears in cache/accelerator errors.

## Required Generated Tests

Extend the table-runtime testkit with cache and accelerator counters.

For each generated script:

1. generate 1 to 64 table cache ids;
2. generate 1 to 256 cache operations;
3. vary enabled/disabled cache;
4. vary capacity across zero, exact-fit, under-pressure, and roomy cases;
5. generate duplicate inserts;
6. generate removes, clears, and resizes;
7. generate keys that share offsets but differ by table id;
8. generate keys that share table id but differ by block kind/address;
9. compare cache outputs and stats invariants to an independent model;
10. generate bloom key sets with embedded zeros and duplicate physical keys;
11. assert no false negatives for inserted bloom keys;
12. run cache-enabled and cache-disabled table read paths if L5G wires them;
13. enforce operation, byte, key, and allocation budgets;
14. increment explicit cache and accelerator case counters so source tests fail
    if the generated route is removed.

Generated tests should avoid probabilistic false-positive rate assertions as
hard pass/fail criteria unless they use a very wide margin and deterministic
seed. No-false-negative is the hard correctness property.

## Old Regression Map

Port or rewrite these old tests:

1. insert/get;
2. miss accounting;
3. multiple table identities;
4. entry too large for cache;
5. duplicate insert;
6. capacity zero;
7. eviction under pressure;
8. pinned or high-priority behavior, if retained;
9. resize behavior;
10. clear behavior;
11. concurrent access behavior;
12. bloom no-false-negative;
13. bloom serialization-equivalent construction only if in-memory bytes are
    still exposed;
14. malformed bloom state falls back conservatively;
15. absent-key bloom probe can skip authoritative work only when safe.

Do not port:

1. process-global cache tests except as negative source guards;
2. file-path hash tests except as negative source guards;
3. raw pointer lifetime tests;
4. lock-free atomic metadata tests;
5. local file `pread` integration tests;
6. old durable SST filter block tests;
7. MVCC snapshot/latest lookup tests;
8. branch rewrite/inherited-layer behavior;
9. product `Value` or primitive payload behavior.

## Boundary And Vocabulary Guards

Production L5 cache/accelerator code must not contain:

1. `unsafe`;
2. `static GLOBAL`, `OnceLock`, `lazy_static`, `OnceCell`, or mutable global
   cache state;
3. `std::fs`, `std::path`, platform filesystem traits, `pread`, mmap, or file
   descriptor vocabulary;
4. `crate::backend`, `crate::layout`, or `crate::service`;
5. `crate::branch`, `crate::commit`, `crate::lifecycle`, or engine crates;
6. `file_path_hash`, `file_id`, `KVSegment`, `SegmentEntry`, old `STRAKV`
   vocabulary, except in tests/docs;
7. MessagePack, bincode product payloads, primitive names, or product `Value`;
8. `snapshot`, `as_of`, `visible_at`, `latest`, `ttl_filter`, `live_only`,
   fork rewrite, or inherited-layer vocabulary.

## Review Checklist

Before calling L5G complete:

1. Can two cache instances with identical keys be proven isolated?
2. Does every cache key include table identity and block address facts?
3. Can a path hash or object name accidentally become the canonical L5 key?
4. Does disabled cache preserve all read results?
5. Does enabled cache preserve all read results?
6. Does eviction preserve capacity and avoid stale bytes?
7. Do stats distinguish hits, misses, inserts, evictions, skips, entries, bytes,
   and capacity coherently?
8. Does every inserted bloom key avoid `DefinitelyAbsent`?
9. Are corrupt/missing accelerators conservative?
10. Are tombstones, expired rows, and duplicate physical-key versions preserved?
11. Do M3G footer filter fields remain zero?
12. Do source guards fail on process-global cache vocabulary?
13. Do default, no-default, and wasm lanes pass?
14. Did the porting log record old behavior retained, rewritten, retired, and
    deferred?

## Verification Commands

Run at minimum:

```text
cargo test -p strata-storage-next --locked --lib table::tests::cache
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --locked --lib format::table
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If cache tests add native thread-only cases, also run the default host test lane
that includes them and verify wasm excludes or adapts those cases intentionally.

## Exit Criteria

L5G test coverage is complete when:

1. cache key validation is directly tested;
2. disabled cache behavior is directly tested;
3. insert/get/remove/clear/resize are directly tested;
4. capacity and eviction are directly tested;
5. cache instance isolation is directly tested;
6. cache stats are directly tested;
7. cache-enabled and cache-disabled table reads are identical where integrated;
8. bloom/filter no-false-negative behavior is directly and generatively tested;
9. unavailable/corrupt accelerators are conservative;
10. durable M3G bytes are unchanged;
11. generated tests include cache and accelerator cases;
12. source guards enforce no unsafe, no globals, no path/backend/service imports,
    no old file-id/path-hash identity, and no product/visibility vocabulary.
