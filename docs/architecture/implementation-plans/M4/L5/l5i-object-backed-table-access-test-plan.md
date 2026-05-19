# L5I Test Plan: Object-Backed Table Access

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5i-object-backed-table-access-implementation-plan.md`

## Goal

Prove that L5I reads published table objects through the L4/L5 boundary without
letting L5 own object names, durable publication, backend calls, paths, or
branch reachability.

The suite must fail if L5I:

1. reads table bytes through `std::fs`, paths, or direct file handles;
2. imports backend, layout, object, or service modules into production
   `src/table/`;
3. constructs or parses table object names inside L5;
4. trusts stale L4 table-object facts without validation;
5. silently accepts short range reads;
6. rewrites backend read errors as table format corruption;
7. requires durable publish capabilities to read an already-published object;
8. lists table prefixes to discover reachability;
9. changes rows or reader facts compared with byte-backed reads;
10. changes cache or accelerator correctness semantics.

## Test Locations

Use these locations:

1. `crates/storage-next/src/service/table.rs` for service-local
   object-backed source and reader tests.
2. `crates/storage-next/src/table/tests/reader.rs` only for object-neutral
   reader behavior that does not import backend/object modules.
3. `crates/storage-next/src/testkit/table_runtime.rs` for generated
   publish-then-read or source-backed parity cases if the adapter is exposed
   through testkit.
4. `crates/storage-next/tests/table_runtime_properties.rs` for generated
   table-runtime coverage behind `testkit`.
5. `crates/storage-next/tests/table_runtime_source_guard.rs` for L5 import and
   vocabulary guards.
6. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old file-backed segment behavior classification.

Do not put backend/object imports in `src/table/tests/reader.rs` unless the
test is explicitly gated as a boundary test outside production L5. Prefer
service-local tests for adapter behavior.

## Reference Model

The reference model is byte-backed reader parity.

For each generated or hand-written table:

```text
rows -> ImmutableTableBuilder -> BuiltTableArtifact
artifact.bytes -> ImmutableTableReader::open_bytes
artifact.bytes -> TableObjectService::publish_create
published object -> TableObjectReaderService::open_reader
```

Assertions:

1. object-backed reader rows equal byte-backed reader rows;
2. object-backed reader facts equal byte-backed reader facts except for the
   caller-supplied table identity if a test intentionally varies identity;
3. object-backed reader facts agree with L4 `TableObjectFacts`;
4. exact, full cursor, range cursor, and physical-prefix cursor output matches
   the byte-backed model;
5. all failures occur before exposing a reader.

The model must not list objects or inspect layout strings. It may compare the
object returned by L4 facts with backend operation logs in service tests.

## Required Unit Tests

### 1. Adapter Construction And Capability Checks

1. A backend with `ReadRange` can construct an object-backed byte source.
2. A backend without `ReadRange` is rejected before any read.
3. Reading one known object does not require `DurablePublish`.
4. Reading one known object does not require `DurableSync`.
5. Reading one known object does not require `ListPrefix`.
6. Reading one known object does not require `WriteObject`.
7. If `ObjectMetadata` is required by implementation, missing metadata
   capability is rejected before read.
8. If `ObjectMetadata` is optional, missing metadata capability still allows a
   successful exact range read and M3G decode.
9. Construction rejects zero byte count.
10. Construction does not parse object name components.
11. Construction does not validate branch id, level, or table id strings; L4
    layout already did that.

### 2. Exact Range-Read Contract

1. `byte_count()` returns the L4-provided byte count.
2. `read_at(0, 0)` returns empty bytes without backend access.
3. `read_at(byte_count, 0)` returns empty bytes without backend access.
4. `read_at(0, byte_count)` reads the full object.
5. `read_at(nonzero_offset, len)` reads exactly the requested slice.
6. `offset + len` overflow is rejected before backend access.
7. ranges past byte count are rejected before backend access.
8. `len` that cannot fit backend range types is rejected before backend access.
9. backend short read becomes a typed source/read error.
10. backend long read, if representable by the backend API, becomes a typed
    source/read error.
11. backend `NotFound` remains distinguishable from table-format corruption.
12. backend `Interrupted` remains distinguishable from table-format corruption.

### 3. Publish-Then-Read Parity

1. Build one M3G table artifact with uncompressed blocks.
2. Publish it through `TableObjectService::publish_create`.
3. Open it through the object-backed reader helper.
4. Open the same bytes through `ImmutableTableReader::open_bytes`.
5. Assert facts match:
   - row count;
   - data block count;
   - key range;
   - commit range;
   - byte count;
   - table identity.
6. Assert rows match byte-for-byte.
7. Assert exact lookup matches byte-backed reader.
8. Assert full cursor output matches byte-backed reader.
9. Assert bounded range cursor output matches byte-backed reader.
10. Assert physical-prefix cursor output matches byte-backed reader.
11. Repeat with zstd table blocks.
12. Repeat with multiple data blocks.
13. Repeat with one-row table.
14. Repeat with tombstones, expired-looking rows, empty values, embedded NUL
    user keys, duplicate physical-key versions, multiple branches, and multiple
    storage-space ids.

### 4. L4 Fact Validation

1. Stale byte count smaller than actual object is rejected.
2. Stale byte count larger than actual object is rejected.
3. Stale row count is rejected after decode.
4. Stale data block count is rejected after decode.
5. Stale commit min is rejected after decode.
6. Stale commit max is rejected after decode.
7. If key range is added to `TableObjectFacts`, stale first key is rejected.
8. If key range is added to `TableObjectFacts`, stale last key is rejected.
9. Fact mismatch returns the field name that failed.
10. Fact mismatch does not expose a partially-open reader.

### 5. Decode And Corruption Routing

1. Missing object returns object-backed read error, not `DecodeFormat`.
2. Truncated object returns source/short-read error when byte count says more
   bytes should exist.
3. Corrupt table magic returns wrapped L5 decode error.
4. Corrupt table footer checksum returns wrapped L5 decode error.
5. Corrupt data block CRC returns wrapped L5 decode error.
6. Unsupported compression returns wrapped L5 decode error.
7. Legacy `STRAKV` bytes are rejected through decode error.
8. Decode errors preserve their source chain for diagnostics.
9. Error displays do not dump table payload bytes.

### 6. Backend Coverage

1. Memory backend publish-then-read succeeds.
2. Local filesystem backend publish-then-read succeeds when `localfs` is
   enabled and durable capabilities are available.
3. Cache-mode or weak backend that supports `ReadRange` but not durable publish
   can read an already-seeded object when construction does not require durable
   capabilities.
4. Backend with only `ReadObject` and no `ReadRange` is rejected unless an
   explicit `ReadObject` fallback is implemented and tested.
5. Backend operation log shows only optional metadata reads plus range reads for
   object-backed reader open.
6. Backend operation log shows no list-prefix calls.
7. Backend operation log shows no list/write/delete/publish calls during read.
8. Localfs test does not inspect filesystem paths directly.

### 7. Reader Identity

1. Caller-supplied `TableIdentity` is used for L5 reader facts.
2. Invalid table identity is rejected before backend read.
3. Full object name with slashes is not accepted as `TableIdentity`.
4. Two different caller-supplied identities over the same object produce
   readers with identical rows and distinct identities, if this is the chosen
   contract.
5. Object name is retained in object-access errors for diagnostics.
6. L5 table facts do not expose object names.

### 8. Cache And Accelerator Neutrality

1. Object-backed reader rows match byte-backed reader rows with cache enabled.
2. Object-backed reader rows match byte-backed reader rows with cache disabled.
3. Opening an object-backed reader does not require `TableBlockCache`.
4. Bloom/filter accelerators are not required for correctness.
5. Missing accelerators do not change output.
6. Corrupt optional accelerators, if any are introduced later, cannot cause
   false absence.
7. Cache stats, if touched by the implementation, are ordinary read stats only
   and do not affect correctness.

### 9. Layer And Source Guards

1. Production `src/table/` does not import `crate::backend`.
2. Production `src/table/` does not import `crate::layout`.
3. Production `src/table/` does not import `crate::object`.
4. Production `src/table/` does not import `crate::service`.
5. Production `src/table/` does not use `std::fs`, `Path`, `PathBuf`, `File`,
   `rename`, `remove_file`, `pread`, or `mmap`.
6. Production `src/table/` does not contain object layout literals such as
   `tables/`, `manifest`, `wal/`, or `snapshots/`.
7. Production `src/table/` does not contain old segment-reader vocabulary such
   as `KVSegment`, `SegmentId`, `Sst`, or `STRAKV`.
8. The adapter module may contain object/backend vocabulary, but tests must
   assert it lives outside production `src/table/`.
9. No bare public API is introduced.
10. No product payload vocabulary enters production table or adapter code.

### 10. Generated Cases

Extend the table-runtime generated harness if the adapter can be exercised
without localfs-only setup.

For each generated case:

1. generate 1 to 256 sorted rows;
2. vary uncompressed and zstd blocks;
3. vary one-block and multi-block artifacts;
4. vary value sizes, including empty values and block-boundary values;
5. include tombstones and non-tombstones;
6. include expired-looking and non-expired-looking rows;
7. include duplicate physical-key versions;
8. include embedded zero bytes in user keys;
9. publish generated bytes to an in-memory backend;
10. open through object-backed source;
11. compare to byte-backed reader model;
12. inject one generated read fault when supported by the fixture;
13. assert object-backed fact validation catches stale facts.

Generated cases must stay bounded enough for normal CI. Larger local stress
commands can be documented but should not be required by default.

## Regression Map From Old Storage

Port these old behaviors as object-backed access tests:

1. table opens from an external byte source;
2. point lookup after open;
3. range scan after open;
4. prefix scan after open;
5. corrupt table bytes fail before rows are exposed;
6. table identity is stable across cache lookups;
7. reads can be performed by offset and length.

Do not port these old behaviors:

1. direct local path opening;
2. `pread` or file-handle ownership;
3. path-hash cache identity;
4. process-global table cache;
5. old `STRAKV` bytes;
6. local file deletion or lifecycle control;
7. segment id allocation;
8. branch-level manifest install;
9. compaction scheduling;
10. MVCC latest-row selection.

## Fault Fixtures

Add or reuse a service-local backend fixture that can inject:

1. missing object;
2. unsupported `ReadRange`;
3. read-range failure before bytes are returned;
4. short range read;
5. stale metadata size, if metadata is checked;
6. corrupt stored bytes;
7. operation logging for read/list/write/delete/publish calls.

Do not use destructive localfs manipulation for fault injection. Keep localfs
tests to publish-then-read and parity checks unless an existing localfs fault
fixture can inject safely.

## Verification Commands

Run at least:

```sh
cargo test -p strata-storage-next --locked --lib service::table
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib table::tests
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If localfs tests are feature-gated, also run the crate's localfs-enabled test
command used by the lower-layer conformance suite.

## Exit Gate

The L5I test suite is complete when:

1. publish-then-read works through the object-backed source on memory backend;
2. publish-then-read works through localfs when enabled;
3. byte-backed and object-backed readers are proven equivalent;
4. stale object facts are rejected;
5. short reads, missing objects, unsupported capabilities, invalid identities,
   and corrupt table bytes are distinguished;
6. no table code imports backend/object/layout/service/path APIs;
7. object-backed reads do not list, write, delete, or publish;
8. cache and accelerator state remains optional for correctness;
9. generated cases cover varied row shapes and compression modes;
10. all verification commands pass.
