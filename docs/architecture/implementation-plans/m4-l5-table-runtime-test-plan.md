# M4-L5 Test Plan: Table Runtime

Status: test-suite plan

Parent plan:
`docs/architecture/implementation-plans/m4-l5-table-runtime-implementation-plan.md`

Closeout plan:
`docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-test-plan.md`

## Goal

Prove that the storage-next L5 table runtime is a reusable, policy-free table
mechanics layer.

The suite must fail if L5:

1. accepts unsorted or duplicate table keys where the contract forbids them;
2. returns rows out of encoded internal-key order;
3. drops, hides, or rewrites rows without an explicit caller policy;
4. depends on branch, commit, lifecycle, recovery, product, or engine modules;
5. calls filesystem or backend APIs directly;
6. treats cache or accelerator state as authoritative;
7. emits non-M3G immutable table bytes;
8. allows corrupt table bytes to produce trusted table facts;
9. has different table behavior over in-memory bytes and object-backed reads;
10. panics on generated cursor, compaction, or corrupt-reader inputs.

This plan is intentionally stricter than the existing `crates/storage` tests.
Current tests are evidence and regression input, not proof of storage-next L5
coverage. M4-L5 must build a new reference-grade suite around the L5 contract.

## Testing Principles

1. Test table mechanics, not product semantics.
2. Valid rows are storage-next `StorageRow` values, never product `Value`
   payloads.
3. L5 compares encoded internal-key bytes and treats storage space, branch id,
   commit version, timestamp, TTL, and tombstone facts as row metadata unless a
   caller-supplied policy is explicitly being tested.
4. Every accepted cursor output is compared against an independent sorted-vector
   model.
5. Every compaction drop is explained by an explicit generated policy decision.
6. Object-backed tests go through lower-layer object abstractions; L5 production
   code must not construct paths or call backend methods directly.
7. Cache and accelerator tests must prove correctness with the cache disabled,
   cold, warm, evicted, and polluted.
8. Corruption tests mutate valid table artifacts after construction and assert
   typed failure before facts are trusted.
9. Fuzz targets must cover byte input and cursor-state movement.
10. Sensitivity probes are required for ordering, cache correctness, row-drop
    policy, and source-boundary guards.

## Test Harness Layout

Recommended test locations:

1. `crates/storage-next/src/table/` for unit tests close to each table module.
2. `crates/storage-next/src/table/tests/` for larger module-local suites once a
   file approaches the engineering threshold.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5
   conformance properties.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans.
5. `crates/storage-next/fuzz/fuzz_targets/table_runtime_reader.rs` for reader
   byte fuzzing.
6. `crates/storage-next/fuzz/fuzz_targets/table_runtime_cursor.rs` for cursor
   operation fuzzing over generated valid table sources.
7. `crates/storage-next/fuzz/fuzz_targets/table_runtime_compaction.rs` for
   generated compaction model fuzzing.

Slice-level test plans live beside their implementation plans under
`docs/architecture/implementation-plans/M4/`. L5A is the one structural
exception: because it is only a scaffold slice, its required tests are recorded
inside `l5a-table-runtime-scaffold-implementation-plan.md` rather than in a
separate paired test-plan file.

Required regression file:

1. `crates/storage-next/proptest-regressions/table_runtime.txt`, created only
   when a failing seed is captured.

## Generators

The property suite should share deterministic generators instead of each module
inventing fixtures.

### Row Generator

Generate 1 to 256 `StorageRow` values by default, with a separate stress budget
for 257 to 4096 rows.

Each generated row should vary:

1. branch id bytes, treated as opaque physical-key bytes by L5;
2. physical key space, including short names and multiple spaces;
3. storage space id, including storage-owned nonzero ids and engine-owned ids;
4. user key bytes, including empty, prefix-related, embedded zero bytes, long
   shared prefixes, and high-bit bytes;
5. commit version, including repeated physical keys at distinct versions;
6. commit timestamp, including zero, equal timestamps, and non-monotonic
   timestamps relative to encoded key order;
7. put rows with value lengths 0 to 4096 bytes by default;
8. tombstone rows;
9. put rows with `expires_at = EPOCH`, future-ish timestamps, and generated
   expiry values.

Generators must be able to emit:

1. sorted unique internal keys;
2. unsorted rows;
3. duplicate internal keys;
4. duplicate physical keys at different commit versions;
5. rows sharing physical-key prefixes;
6. rows spanning multiple target data blocks;
7. rows near configured size limits.

### Source Generator

Generate 0 to 16 table sources for cursor and merge tests:

1. mutable table source;
2. frozen table source;
3. immutable bytes source;
4. object-backed immutable source;
5. empty source;
6. source with one row;
7. source with repeated physical keys at different commit versions;
8. source with disjoint ranges;
9. source with overlapping ranges.

Every generated source must be independently sorted by encoded internal-key
bytes unless the test is explicitly an invalid-input test.

### Operation Generator

Generate cursor operations:

1. open cursor;
2. seek to exact key;
3. seek to key prefix lower bound;
4. seek before first key;
5. seek after last key;
6. seek into a gap;
7. next;
8. collect remaining;
9. reset or reopen cursor;
10. drop and recreate cursor with cache state preserved.

### Compaction Policy Generator

Generate a policy table keyed by encoded internal-key bytes or physical-key
facts:

1. keep row;
2. drop row;
3. drop row only if tombstone;
4. drop row only if put;
5. drop row only if expired relative to a generated timestamp;
6. force output split before row;
7. forbid output split before row.

The model must compute expected output by applying this policy to sorted input
vectors. L5 must not independently infer retention safety.

## Required Cases

### 1. Module And Boundary Guards

1. `table` module compiles with no behavior.
2. L5 production code does not import `crate::branch`, `crate::commit`,
   `crate::lifecycle`, upper runtime modules, or engine crates.
3. L5 production code does not import product primitive types such as current
   `Value`, `Key`, `Namespace`, `TypeTag`, `EntityRef`, JSON, graph, vector,
   search, or transaction DTOs.
4. L5 production code does not call `std::fs`, `std::os::unix::fs::FileExt`,
   `File`, `Path`, or backend methods directly.
5. L5 tests may use lower-layer testkit helpers, but production L5 code may not
   depend on testkit.
6. Public crate surface remains unchanged unless a later L9 plan approves it.
7. All L5 errors are storage-mechanical table errors and preserve source
   chains.
8. Table module file sizes stay within engineering thresholds or split into
   submodules before they exceed them.

### 2. Row And Key Adapter

1. Encoded internal-key bytes sort by physical key ascending and commit version
   descending for the same physical key.
2. Duplicate physical keys at different commit versions are accepted.
3. Duplicate internal keys are rejected by constructors that require unique
   rows.
4. Storage space ids are preserved as opaque row facts.
5. Branch ids inside physical keys are preserved as opaque bytes.
6. Empty value put rows are accepted.
7. Tombstone rows carry no value and no expiry.
8. Expiry metadata is preserved but not interpreted as visibility.
9. Size accounting includes key bytes, row bytes, and implementation overhead
   according to a documented approximation.
10. Key-range bounds handle empty ranges, exact singleton ranges, prefix ranges,
    lower-unbounded ranges, upper-unbounded ranges, and gaps.
11. Invalid range construction is rejected before cursor movement.
12. Adapter tests fail if old product key or value types enter valid L5 rows.

### 3. Mutable Table

1. Empty mutable table reports zero rows, zero bytes, and empty iteration.
2. One put row inserts and reads back by raw key.
3. One tombstone row inserts and reads back as a row.
4. Put plus tombstone rows iterate in encoded internal-key order.
5. Duplicate physical keys at descending commit versions iterate newest first.
6. Insert rejects duplicate internal keys or reports the chosen replacement
   behavior explicitly; silent replacement is not allowed unless documented.
7. Insert after freeze is rejected with a typed error, not a panic.
8. Freeze is idempotent or explicitly rejected after first freeze.
9. Sorted iteration over generated rows matches the independent model.
10. Point seek before first, at first, in middle, at last, and after last
    matches the model.
11. Range seek with inclusive/exclusive bounds matches the model.
12. Prefix seek returns only rows whose encoded physical key starts with the
    generated prefix.
13. Approximate memory use is monotonic for inserts.
14. Approximate memory use does not decrease on duplicate rejection.
15. Mutating one table does not affect another table.
16. Mutable table does not perform MVCC latest selection.
17. Mutable table does not filter tombstones.
18. Mutable table does not filter expired rows.
19. Generated operations never panic.

### 4. Frozen Table

1. Freezing preserves all rows and table facts.
2. Frozen iteration matches mutable iteration at freeze time.
3. Mutating the original mutable table after freeze is impossible or does not
   affect the frozen view.
4. Frozen point/range/prefix cursors match the mutable model.
5. Frozen table can be converted to immutable-builder input without reordering.
6. Frozen table preserves tombstones, expiry metadata, timestamps, and value
   bytes.
7. Frozen table is safe to share by reference across readers according to the
   chosen ownership model.
8. Generated freeze/read sequences never panic.

### 5. Raw Cursors

1. Empty cursor starts invalid and `next` remains empty.
2. Cursor over one row returns that row once.
3. Cursor over many rows returns exact encoded-key order.
4. `seek` to exact row positions the cursor at that row.
5. `seek` into a gap positions at the first greater row.
6. `seek` before first positions at first.
7. `seek` after last returns empty.
8. Repeated `next` after exhaustion remains empty.
9. Prefix cursor stops at prefix boundary.
10. Range cursor respects lower and upper bounds.
11. Range cursor does not skip tombstones.
12. Range cursor does not skip expired rows.
13. Cursor item references or owned rows obey the documented lifetime rules.
14. Cursor output is identical for mutable, frozen, and immutable sources built
    from the same rows.
15. Generated cursor scripts never panic.

### 6. Merge Cursor

1. Merging zero sources returns empty.
2. Merging one source is identity.
3. Merging two disjoint sources returns global sorted order.
4. Merging many disjoint sources returns global sorted order.
5. Merging overlapping sources returns all rows in encoded-key order unless the
   merge contract explicitly deduplicates exact duplicate internal keys.
6. If exact duplicate internal keys appear across sources, the tie-break rule is
   deterministic and tested.
7. Linear merge path and heap merge path both execute under tests.
8. Source order tie-breaks are stable.
9. Tombstones are preserved.
10. Expired rows are preserved.
11. Merge cursor does not perform MVCC latest selection.
12. Merge cursor does not rewrite branch ids.
13. Merge model tests cover 0 to 16 sources.
14. Generated merge scripts never panic.

### 7. Immutable Table Builder

1. Empty input is rejected.
2. Unsorted input is rejected.
3. Duplicate internal keys are rejected.
4. Sorted one-row input builds valid M3G bytes.
5. Sorted multi-row input builds valid M3G bytes.
6. Put plus tombstone rows build and decode in order.
7. Duplicate physical keys at different commit versions build and decode in
   order.
8. Target data block size forces one-block and multi-block outputs.
9. Compression config covers uncompressed and zstd paths where features permit.
10. Builder facts match L3 decoded table facts.
11. Builder byte output is deterministic for the same input and config.
12. Builder never writes old `STRAKV` bytes.
13. Builder does not publish objects or construct object names.
14. Builder rejects row counts, key lengths, row lengths, and block sizes above
    configured L5/L3 limits before allocation.
15. Generated builder model tests pass for 1 to 256 rows by default.

### 8. Immutable Table Reader

1. Reader opens one-block M3G table bytes.
2. Reader opens multi-block M3G table bytes.
3. Reader rejects old `STRAKV` bytes.
4. Reader rejects bad header, footer, block, index, and properties bytes using
   typed errors.
5. Reader validates table facts before exposing trusted facts.
6. Reader point lookup finds first, middle, and last rows.
7. Reader point lookup returns missing for keys before first, after last, and in
   gaps.
8. Reader range cursor over one block matches the model.
9. Reader range cursor over many blocks matches the model.
10. Reader prefix cursor matches the model.
11. Reader handles tombstones as rows.
12. Reader handles expired rows as rows.
13. Reader does not perform branch visibility filtering.
14. Reader validates the full V1 table before exposing facts. Lazy data-block
    reads are deferred until the table format has an authoritative metadata
    proof that does not require the footer CRC over all preceding bytes.
15. Reader reports range-read failures with object/source context.
16. Reader handles short reads, overlong reads, and inconsistent metadata as
    typed errors.
17. Reader behavior over full in-memory bytes and range-backed source is
    identical.
18. Generated reader scripts never panic.

### 9. Block Cache

1. Cache disabled path stores no bytes and returns no hits.
2. Cold insert stores the requested block bytes.
3. Warm cache hit returns the stored block bytes without another insert.
4. Cache key includes stable table identity and block address.
5. Same block offset in different table identities does not collide.
6. Eviction removes entries when capacity is exceeded.
7. Evicted block is reread and returns correct rows.
8. Cache stats report hits, misses, inserts, evictions, entries, and bytes.
9. Cache stats are deterministic enough for tests under single-threaded access.
10. Cache does not use process-global state.
11. Separate database-owned caches do not see each other's entries.
12. Corrupt cached bytes cannot be installed through the public cache API.
13. Cache pollution cannot change standalone cache lookup output.
14. Cache capacity zero behaves like disabled cache or is rejected explicitly.
15. Generated cache scripts with random evictions match the in-memory cache
    model.

### 10. Optional Accelerators

These tests apply only if L5 implements bloom/filter/index accelerators beyond
the M3G authoritative index.

1. Missing accelerator falls back or disables itself without data loss.
2. Corrupt optional accelerator falls back or returns a typed non-authoritative
   accelerator error according to the contract.
3. Bloom/filter never produces a false negative for generated present keys.
4. Bloom/filter false positives still read authoritative rows.
5. Accelerator cache entries are scoped by table identity.
6. Accelerator absence does not change point/range/prefix output.
7. Accelerator tests do not require durable filter bytes unless the M3G format
   spec is amended.

### 11. Generic Compaction

1. Empty compaction input is rejected or returns no output according to the
   documented contract.
2. One input source with keep-all policy is identity modulo output splitting.
3. Multiple disjoint sources with keep-all policy merge in encoded-key order.
4. Multiple overlapping sources with keep-all policy preserve deterministic
   source/index tie behavior for distinct internal keys; exact duplicate
   internal keys across sources are rejected until a caller-supplied duplicate
   policy exists.
5. Tombstones are preserved under keep-all policy.
6. Expired rows are preserved under keep-all policy.
7. A drop-exact-row policy drops exactly the selected rows.
8. A tombstone-drop policy drops only selected tombstones.
9. A TTL-drop policy drops only selected expired rows.
10. A put-drop policy drops only selected put rows.
11. Compaction does not infer branch-safe tombstone elision.
12. Compaction does not infer version-retention floors.
13. Compaction does not infer snapshot floors.
14. Compaction does not special-case product data families.
15. Output tables are sorted and duplicate-free where required.
16. Output splitting respects the configured approximate row-size target.
17. Every produced output artifact decodes through M3G whole-table validation.
18. Compaction reports input row count, output row count, dropped row count,
    output table count, and byte estimates.
19. Generated compaction model tests cover 1 to 16 sources and 0 to 4096 rows.

### 12. Object-Backed Reader Handoff

1. L4 publishes a table object and L5 reads it through the approved source
   abstraction.
2. Memory backend object source and local filesystem object source produce the
   same reader output.
3. Object names are constructed outside L5.
4. Durable publication remains in L4.
5. L5 reader does not mutate table objects.
6. Full-source read failure while loading table bytes returns typed source
   error with object/source context.
7. Short range-read is rejected.
8. Object-backed reader may read the full V1 table object on open because the
   current footer CRC validates all preceding bytes.
9. Object-backed reader tests run against memory and local filesystem paths
   through lower-layer APIs, not by opening files from L5.

### 13. Error And Diagnostic Coverage

1. Every L5 public/internal result type has typed error variants.
2. Error variants preserve lower-layer `FormatError`, object-source read error,
   and cache/decode source chains.
3. Error display text is stable enough for debugging but tests assert variants
   and facts, not prose.
4. Errors include table identity or table facts where available.
5. Corruption errors distinguish invalid table bytes from object read failures.
6. Policy errors distinguish invalid caller policy from invalid input ordering.
7. No error variant exposes product-level vocabulary.

### 14. Source Guards

Add source scans that fail if L5 production code contains:

1. imports from `crate::branch`, `crate::commit`, `crate::lifecycle`, or future
   upper-layer modules;
2. imports from engine crates;
3. product primitive vocabulary in valid L5 table payloads;
4. old `STRAKV` valid-byte claims;
5. direct filesystem/path/file APIs;
6. direct backend calls outside an approved object-source adapter;
7. environment-variable reads;
8. process-global cache singletons.

Tests may mention old code paths only as historical evidence or invalid input.

### 15. Fuzz Coverage

Required fuzz targets:

1. `table_runtime_reader`: arbitrary bytes through the L5 reader open path.
2. `table_runtime_cursor`: arbitrary operation scripts over generated valid
   table sources.
3. `table_runtime_compaction`: arbitrary operation/policy scripts over bounded
   generated sorted sources if the compaction API shape is stable enough.

Fuzz invariants:

1. no panic;
2. no unbounded allocation;
3. successful reader open exposes table facts that match L3 decode;
4. successful cursor movement returns sorted rows within requested bounds;
5. successful merge cursor output is sorted;
6. successful compaction output decodes as M3G tables;
7. corrupt bytes never become trusted table facts;
8. cursor scripts terminate under a fixed step budget.

Seed corpora:

1. M3G one-block table golden;
2. M3G two-block table golden;
3. table with put plus tombstone;
4. table with duplicate physical keys at different commit versions;
5. table with long shared key prefixes;
6. table with empty values;
7. table with zstd data blocks.

### 16. Build And Feature Matrix

Required verification for M4-L5 closeout:

1. `cargo test -p strata-storage-next --locked table`
2. `cargo test -p strata-storage-next --no-default-features --locked table`
3. `cargo test -p strata-storage-next --all-features --locked table`
4. `cargo test -p strata-storage-next --locked --test table_runtime_properties`
5. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
8. `cargo doc -p strata-storage-next --no-deps --locked`
9. `cargo fmt --package strata-storage-next --check`
10. `git diff --check`

If `cargo hack` is available, run the storage-next feature powerset check at the
same depth used by M3 closure.

## Sensitivity Probes

Before marking M4-L5 complete, run temporary local mutations and confirm the
targeted tests fail:

1. reverse internal-key comparator ordering;
2. allow duplicate internal keys into mutable table;
3. allow duplicate internal keys into builder;
4. make cursor `seek` choose the previous row instead of first greater-or-equal;
5. remove merge tie-break by source order;
6. make reader trust header facts without validating footer/index/properties;
7. make reader ignore block checksum errors;
8. make cache key omit table identity;
9. make cache return bytes for the wrong block offset;
10. make compaction drop tombstones without policy approval;
11. make compaction drop expired rows without policy approval;
12. let L5 import an upper-layer module and verify source guard failure;
13. let L5 call `std::fs` and verify source guard failure.

Record sensitivity probe results in the M4-L5 closeout notes.

## Deferred To Later Layers

The following are explicitly not L5 test gaps:

1. branch latest selection;
2. branch inheritance and copy-on-write key rewriting;
3. fork-version filtering;
4. commit version allocation;
5. WAL-before-visible behavior;
6. visible-version publication;
7. crash recovery and WAL replay;
8. checkpoint scheduling;
9. retention reachability;
10. table quarantine policy;
11. public storage API conformance;
12. lazy object-backed table reads and reader/cache integration;
13. caller-provided compaction split boundaries.

Those belong to M4-L6, M4-L7, M4-L8, and M4-L9.

## Exit Gate

M4-L5 test coverage is complete when:

1. every required case above has a passing test or a documented deferment to a
   later layer;
2. generated model tests cover mutable, cursor, merge, builder/reader, and
   compaction behavior;
3. fuzz targets exist with checked-in seed corpora;
4. source guards prevent upper-layer and product leakage;
5. memory and local filesystem object-backed reader tests pass;
6. standalone cache-disabled and cache-enabled paths are covered by cache model
   tests;
7. sensitivity probes have been run and recorded;
8. the implementation plan and porting log identify old-code mechanics that
   were ported, rewritten, retired, or deferred.
