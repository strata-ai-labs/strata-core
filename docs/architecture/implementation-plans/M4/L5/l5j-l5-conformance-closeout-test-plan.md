# L5J Test Plan: L5 Conformance Closeout

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5j-l5-conformance-closeout-implementation-plan.md`

## Goal

Prove that M4-L5 is complete as a reusable table mechanics layer and that L6
can build branch state on top without first repairing L5 tests.

The suite must fail if:

1. any L5 slice lacks direct or generated coverage;
2. production `src/table/` imports upper layers, backend APIs, object layout, or
   filesystem/path APIs;
3. old product or table-format vocabulary re-enters storage-next L5;
4. cache or bloom/filter accelerators become authoritative;
5. compaction drops rows without caller policy;
6. object-backed access changes byte-backed reader behavior;
7. generated table-runtime harnesses stop exercising a category;
8. no-default or wasm-compatible builds accidentally depend on localfs;
9. source guards miss known forbidden patterns;
10. the closeout documentation claims a gap is closed without a test,
    command, or explicit deferral.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/` for direct table-module behavior;
2. `crates/storage-next/src/table/*.rs` for small module-local unit tests;
3. `crates/storage-next/src/service/table.rs` for L4/L5 object-backed adapter
   tests;
4. `crates/storage-next/src/testkit/table_runtime.rs` for generated closeout
   route checks;
5. `crates/storage-next/tests/table_runtime_properties.rs` for external
   property-harness assertions;
6. `crates/storage-next/tests/table_runtime_source_guard.rs` for production L5
   boundary scans;
7. `crates/storage-next/tests/table_runtime_closeout.rs` for closeout inventory
   and fuzz-target structural checks;
8. `crates/storage-next/fuzz/fuzz_targets/` for runtime and format fuzz
   targets;
9. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   final closeout ledger.

Do not add backend/object imports to production `src/table/`. Boundary tests
that need backend/object behavior belong in `service/table.rs`, testkit, or
external tests.

## Coverage Matrix

L5J must produce or update a matrix with one row per L5 slice.

Required columns:

1. direct unit tests;
2. generated/property tests;
3. source guards;
4. fuzz or fuzz-adjacent coverage;
5. cross-feature coverage;
6. old-code behavior mapped;
7. deferred behavior and owner;
8. mandatory commands that exercise the row.

Required rows:

1. `L5A` scaffold/config/facts/stats/errors;
2. `L5B` row/key adapters;
3. `L5C` mutable/frozen tables;
4. `L5D` raw cursors and merge cursor;
5. `L5E` immutable table builder;
6. `L5F` immutable table reader;
7. `L5G` cache and accelerators;
8. `L5H` generic compaction;
9. `L5I` object-backed table access.

A blank cell is a test gap unless it has a named owner-layer deferral.

## Required Closeout Tests

### 1. Source Guard Completeness

The source guard suite must assert:

1. production `src/table/` does not import `crate::backend`;
2. production `src/table/` does not import `crate::layout`;
3. production `src/table/` does not import `crate::object`;
4. production `src/table/` does not import `crate::service`;
5. production `src/table/` does not import `crate::branch`,
   `crate::commit`, or `crate::lifecycle`;
6. production `src/table/` does not import engine crates;
7. production `src/table/` does not call backend methods directly;
8. production `src/table/` does not use `std::fs`, `Path`, `PathBuf`, `File`,
   `pread`, `rename`, `remove_file`, or `mmap`;
9. production `src/table/` does not contain object layout literals such as
   `tables/`, `wal/`, `snapshots/`, or `manifest/current`;
10. production `src/table/` does not contain old table vocabulary such as
    `KVSegment`, `SegmentId`, `SegmentBuilder`, `Sst`, or `STRAKV`;
11. production `src/table/` does not contain old cache identity vocabulary such
    as path hash, file id, or global cache state;
12. production `src/table/compaction.rs` does not contain branch retention,
    MVCC, inherited, fork, or lifecycle policy terms;
13. production `src/table/` does not contain product payload vocabulary such as
    `EntityRef`, MessagePack, JSON, graph, vector, search, event, or
    transaction DTOs;
14. no bare public API leaks from storage-next L5;
15. the guard itself has regression probes proving each forbidden category is
    detected.

### 2. Generated Harness Category Counters

`TableRuntimeScaffoldOutcome` must expose a counter for every category:

1. valid config;
2. invalid config;
3. valid facts;
4. invalid facts;
5. row/key adapters;
6. invalid row/key sequences;
7. key bounds;
8. size accounting;
9. mutable/frozen tables;
10. raw cursors;
11. immutable builder artifacts;
12. immutable table readers;
13. object-backed table readers;
14. table block caches;
15. bloom/filter accelerators;
16. generic table compactions;
17. error source chains;
18. stats.

Tests must assert each counter is nonzero in:

1. `tests/table_runtime_properties.rs`;
2. `src/testkit/table_runtime.rs` unit tests.

Adding a new L5 category later requires adding a counter and nonzero assertion
in the same change.

### 3. Row/Key Closeout

The direct and generated tests must cover:

1. physical key ascending order;
2. commit version descending order for identical physical keys;
3. duplicate physical keys at distinct versions;
4. duplicate exact internal-key rejection;
5. empty user keys and embedded zero bytes;
6. high-bit user-key bytes where the row type permits them;
7. storage-owned and engine-owned storage-space ids;
8. branch bytes as opaque row facts;
9. put rows with empty values;
10. tombstones with no values and no expiry;
11. expiry-looking put rows preserved without visibility filtering;
12. exact bounds, closed bounds, unbounded bounds, degenerate bounds, and
    physical-prefix bounds;
13. deterministic approximate size accounting.

### 4. Mutable/Frozen Closeout

The direct and generated tests must cover:

1. empty table facts;
2. one-row facts;
3. many-row facts;
4. insertion of puts and tombstones;
5. duplicate exact internal-key rejection without mutation;
6. encoded-key iteration order;
7. exact lookup;
8. range lookup;
9. physical-prefix lookup;
10. freeze preserving all rows and facts;
11. mutation isolation between table instances;
12. no MVCC latest filtering;
13. no tombstone filtering;
14. no expiry filtering.

### 5. Cursor/Merge Closeout

The direct and generated tests must cover:

1. empty cursor state;
2. one-row cursor state;
3. seek before first;
4. seek at first;
5. seek into a gap;
6. seek at last;
7. seek after last;
8. repeated advance after exhaustion;
9. bounded cursor over exact bounds;
10. bounded cursor over closed range;
11. bounded cursor over physical-prefix bounds;
12. mutable/frozen/immutable cursor parity;
13. merge zero sources;
14. merge one source;
15. merge many disjoint sources;
16. merge overlapping sources;
17. merge equal keys with documented tie behavior;
18. linear merge path;
19. heap merge path;
20. reseek after partial consumption.

### 6. Immutable Builder/Reader Closeout

The direct and generated tests must cover:

1. empty input rejection;
2. unsorted input rejection;
3. duplicate input rejection;
4. deterministic output bytes for identical input;
5. one-block tables;
6. multi-block tables;
7. uncompressed tables;
8. zstd tables;
9. row fact preservation;
10. M3G header/footer facts;
11. M3G properties/index/data-block consistency;
12. byte-backed reader open;
13. source-backed reader open;
14. exact lookup;
15. full cursor;
16. bounded range cursor;
17. physical-prefix cursor;
18. corrupt magic rejection;
19. corrupt footer CRC rejection;
20. corrupt block CRC rejection;
21. truncated object rejection before facts are trusted;
22. legacy `STRAKV` rejection.

### 7. Cache/Accelerator Closeout

The direct and generated tests must cover:

1. cache disabled behavior;
2. cache insert;
3. cache duplicate insert;
4. cache hit;
5. cache miss;
6. cache eviction;
7. cache resize down;
8. cache table removal;
9. independent cache instances;
10. concurrent insert/remove/clear paths if the implementation supports them;
11. stats consistency;
12. stable table/block cache key validation;
13. no process-global cache state;
14. bloom/filter no false negatives;
15. bloom/filter bounded probing;
16. bloom/filter absence is conservative;
17. reader correctness unchanged with cache disabled, cold, warm, or evicted.

### 8. Compaction Closeout

The direct and generated tests must cover:

1. empty input as no output;
2. single source keep-all;
3. multiple source keep-all;
4. source ordering validation;
5. duplicate exact internal-key rejection across sources;
6. caller policy keeping all rows;
7. caller policy dropping selected rows;
8. caller policy dropping tombstones only when selected;
9. caller policy dropping expired-looking rows only when selected;
10. dropped rows do not drive output splitting;
11. kept physical-key groups remain ordered;
12. output table size splitting;
13. maximum output table count enforcement;
14. policy errors produce no partial output;
15. output artifacts decode as valid M3G tables;
16. output artifacts read through the L5 reader path;
17. deterministic output across equivalent source groupings;
18. no built-in L6/L8 retention policy vocabulary.

### 9. Object-Backed Access Closeout

The direct and generated tests must cover:

1. memory backend object-backed reads;
2. localfs publish-then-read when the feature is enabled;
3. missing `ReadRange` rejection;
4. no durable publish/sync requirement for reads;
5. optional metadata preflight;
6. stale byte-count rejection when metadata is available;
7. short range read rejection;
8. long range read rejection;
9. missing object stays an object-read error;
10. interrupted read stays an object-read error;
11. corrupt table bytes stay table decode errors;
12. stale row-count, block-count, commit-min, and commit-max fact rejection;
13. byte-backed parity for exact lookup, full cursor, range cursor, and prefix
    cursor;
14. generated memory-backend object-backed parity;
15. caller-supplied identity preservation;
16. no list/write/delete/publish during reader open.

### 10. Error And Diagnostic Closeout

The suite must verify:

1. table runtime errors are typed;
2. source errors preserve source chains where available;
3. decode errors preserve format sources;
4. backend errors are not collapsed into table corruption at the L4/L5 boundary;
5. displays are bounded and do not dump row payloads or table bytes;
6. invalid config/range/fact errors identify the field involved;
7. unsupported capability errors identify the missing capability.

## Fuzz And Property Requirements

L5J must inventory fuzz or fuzz-adjacent coverage.

Required coverage classes:

1. arbitrary M3G table artifact bytes;
2. arbitrary table block bytes;
3. generated sorted rows through builder/reader;
4. generated cursor movement over mutable/frozen/immutable sources;
5. generated merge source combinations;
6. generated cache operations;
7. generated compaction policy decisions;
8. generated object-backed table reads over memory backend.

Acceptable proof forms:

1. libFuzzer targets under `crates/storage-next/fuzz/fuzz_targets/`;
2. proptest routes under `tests/table_runtime_properties.rs`;
3. testkit model scripts that are callable by fuzz targets;
4. documented deferral with a reason and owner.

The delivered runtime fuzz targets are mandatory closeout inventory items:

1. `table_runtime_reader`, calling
   `check_table_runtime_reader_contract`;
2. `table_runtime_cursor`, calling
   `check_table_runtime_cursor_contract`;
3. `table_runtime_compaction`, calling
   `check_table_runtime_compaction_contract`.

`tests/table_runtime_closeout.rs` must verify both target registration and the
structural rule that each runtime target calls its dedicated contract rather
than only the shared scaffold contract.

## Cross-Feature Matrix

Run and record these mandatory modes:

| Mode | Purpose | Command |
|---|---|---|
| focused L5 unit | fast table mechanics check | `cargo test -p strata-storage-next --locked --lib table::tests` |
| testkit unit | generated route unit check | `cargo test -p strata-storage-next --locked --lib testkit::table_runtime` |
| testkit property | generated external property check | `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties` |
| no-default property | prove no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties` |
| closeout inventory | fuzz inventory and generated-counter enforcement | `cargo test -p strata-storage-next --locked --test table_runtime_closeout` |
| source guards | L5 purity | `cargo test -p strata-storage-next --locked --test table_runtime_source_guard` |
| wasm/no-default | browser-compatible lower surface | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| full package | regression safety net | `cargo test -p strata-storage-next --locked` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Optional modes:

1. localfs explicit feature if not already part of defaults;
2. short fuzz smoke commands for `table_runtime_reader`,
   `table_runtime_cursor`, and `table_runtime_compaction`;
3. longer stress commands for generated table runtime scripts.

Runtime fuzz smoke commands, when cargo-fuzz and nightly are available:

```sh
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_reader -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_cursor -- -max_total_time=60
cd crates/storage-next/fuzz && cargo +nightly fuzz run table_runtime_compaction -- -max_total_time=60
```

## Regression Map From Old Storage

L5J must confirm each old-code behavior is classified.

Ported to L5:

1. ordered mutable table mechanics;
2. frozen read-only table view;
3. raw seekable cursors;
4. k-way merge over sorted raw sources;
5. immutable table builder mechanics;
6. immutable table reader mechanics;
7. read-by-range source abstraction;
8. block cache mechanics;
9. non-authoritative bloom/filter acceleration;
10. generic sorted compaction mechanics.

Retired in storage-next L5:

1. old `STRAKV` table bytes;
2. path-backed `KVSegment` reader;
3. `pread`/file-handle table access;
4. path-hash table cache identity;
5. process-global table cache;
6. product `Value`, `Key`, and DTO payload semantics;
7. MessagePack table payloads.

Deferred to L6:

1. branch table manifests;
2. reachable table selection;
3. level ownership;
4. inherited table lookup;
5. fork gates;
6. MVCC/latest-visible row selection.

Deferred to L8:

1. flush scheduling;
2. manifest install;
3. table retention and garbage collection;
4. table quarantine policy;
5. checkpoint/table/WAL coordination.

Deferred to post-V1:

1. lazy block reads after whole-object validation;
2. caller-provided compaction split boundaries;
3. durable object-store fences;
4. conditional read validation;
5. durable filter blocks if the M3G format is explicitly extended.

## Exit Gate

L5J test closeout is complete when:

1. the coverage matrix has no blank cells without owner-layer deferrals;
2. every mandatory command passes on the final tree;
3. direct tests cover each table surface;
4. generated/property tests cover each table surface;
5. source guards prove L5 purity;
6. old-code behavior is classified in the porting log;
7. no new public API leaks;
8. no new product, path, branch, lifecycle, or backend dependency enters
   production L5;
9. remaining work is explicitly assigned to L6, L8, or post-V1.
