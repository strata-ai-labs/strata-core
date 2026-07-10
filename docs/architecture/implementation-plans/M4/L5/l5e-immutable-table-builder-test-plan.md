# L5E Test Plan: Immutable Table Builder

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/l5e-immutable-table-builder-implementation-plan.md`

## Goal

Prove that L5E builds valid M3G immutable table artifacts from sorted
storage-next table rows, derives facts from the decoded bytes, and stays inside
the L5 table-runtime boundary.

The suite must fail if L5E:

1. accepts empty input;
2. accepts unsorted input;
3. accepts duplicate encoded internal keys;
4. drops tombstones;
5. drops expired-looking rows;
6. collapses duplicate physical keys at different commit versions;
7. emits old `STRAKV` bytes or any non-M3G table bytes;
8. constructs object names or publishes table objects;
9. writes files or calls backend APIs;
10. derives facts that disagree with the decoded table bytes;
11. produces nondeterministic bytes for identical input and config;
12. hides branch ids, rewrites keys, applies visibility filters, or evaluates
    TTL/tombstone policy.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/builder.rs` for module-local L5E unit
   tests.
2. `crates/storage-next/src/testkit/table_runtime.rs` for generated builder
   model checks.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5E
   property tests behind the `testkit` feature.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `docs/architecture/implementation-plans/M4/m4-l5-porting-log.md` for the
   old segment-builder porting record.

Do not add reader point/range/prefix lookup tests here. L5F owns immutable
reader behavior. L5E may decode complete artifacts through L3 only to validate
builder output.

## Reference Model

The builder model starts from a sorted vector of `TableRow` values.

For each input:

```text
expected_keys = input encoded internal keys in order
expected_rows = input StorageRow values in order
expected_row_count = input length
expected_data_block_count = ceil(input length / rows_per_block)
expected_first_key = first encoded internal key
expected_last_key = last encoded internal key
expected_commit_min = min(input commit versions)
expected_commit_max = max(input commit versions)
```

The built bytes are then decoded through L3:

```text
decoded = decode_immutable_table(artifact.bytes)
assert decoded.rows == expected_rows
assert decoded.header.row_count == expected_row_count
assert decoded.header.data_block_count == expected_data_block_count
assert decoded.properties match expected facts
assert artifact.facts match decoded facts
```

The model is independent of object IO and branch visibility. It preserves every
input row.

## Required Unit Tests

### 1. Builder Construction And Config

1. Default `TableBuilderConfig` constructs an immutable builder.
2. Explicit uncompressed config constructs.
3. Explicit zstd config constructs where supported.
4. `target_data_block_size == 0` is rejected before build.
5. `rows_per_block == 0` is rejected before build.
6. Builder exposes its config or otherwise proves the configured values are
   applied to output.
7. Builder construction does not allocate table bytes.
8. Builder construction does not import or initialize cache state.

### 2. Input Validation

1. Empty input is rejected.
2. One sorted row builds successfully.
3. Many sorted rows build successfully.
4. Unsorted adjacent rows are rejected with `InvalidRowOrder`.
5. Duplicate encoded internal keys are rejected with `DuplicateInternalKey`.
6. Duplicate physical keys at different commit versions are accepted.
7. Same physical key versions preserve V1 commit-version descending order.
8. User keys with embedded zero bytes are accepted.
9. Rows from different branch ids are accepted.
10. Rows from different storage space ids are accepted.
11. Put rows with empty values are accepted.
12. Tombstone rows are accepted.
13. Expired-looking rows are accepted.
14. Commit timestamp does not affect sorted-order validation.
15. Expiry timestamp does not affect sorted-order validation.

### 3. M3G Byte Shape

1. Built bytes start with `STTB`.
2. Built bytes end with a valid `STTF` footer region and table CRC.
3. Built bytes decode with `decode_immutable_table`.
4. Built bytes never use old `STRAKV` as their table envelope magic.
5. Header format version is 1.
6. Header size is 64 bytes.
7. Footer size is 64 bytes.
8. Header flags are zero.
9. Footer filter offset and length remain zero until a V1 filter format exists.
10. Block frames are length-delimited.
11. Data block, index block, and properties block are all present.
12. Hidden bytes between block regions are absent, as proven by L3 decode.

### 4. Roundtrip Row Preservation

1. One put row decodes exactly.
2. Many put rows decode exactly.
3. Put plus tombstone rows decode exactly.
4. Expired-looking rows decode exactly.
5. Empty-value rows decode exactly.
6. Large value rows within L3 limits decode exactly.
7. Multiple versions of the same physical key decode exactly.
8. Branch id bytes are preserved.
9. Storage-space id bytes are preserved.
10. User keys with embedded zeros are preserved.
11. Commit version is preserved.
12. Commit timestamp is preserved.
13. Expiry timestamp is preserved.
14. Tombstone marker is preserved.
15. Value bytes are preserved.

### 5. Block Partitioning

1. One row with `rows_per_block = 1` produces one data block.
2. Two rows with `rows_per_block = 2` produce one data block.
3. Two rows with `rows_per_block = 1` produce two data blocks.
4. `N` rows with `rows_per_block = K` produces `ceil(N / K)` data blocks.
5. First and last key of every decoded index entry match the corresponding
   data block.
6. Index entries are sorted and non-overlapping.
7. Properties data-block count matches header data-block count.
8. Header target data-block size equals the builder config value.
9. Changing `rows_per_block` can change bytes and block count.
10. Changing only target data-block size changes the header fact and keeps row
    payloads intact.

### 6. Compression

1. Uncompressed build decodes successfully.
2. Zstd build decodes successfully where zstd is enabled.
3. Zstd build preserves rows exactly.
4. If L3 exposes block-frame compression facts, the data-block codec matches
   the builder config.
5. Index and properties blocks remain uncompressed unless the M3G format
   deliberately changes.
6. The same rows with different compression configs may produce different
   bytes but must decode to the same rows.
7. Compression config is never read from environment variables.

### 7. Artifact Facts

1. Artifact byte count equals `artifact.bytes().len()`.
2. Artifact row count equals decoded row count.
3. Artifact data-block count equals decoded data-block count.
4. Artifact first key equals decoded properties min key.
5. Artifact last key equals decoded properties max key.
6. Artifact commit min equals decoded properties commit min.
7. Artifact commit max equals decoded properties commit max.
8. Artifact identity equals the caller-supplied identity.
9. Invalid table identity construction is rejected before a builder call, or
   by the builder if it accepts raw identity text as a convenience.
10. Fact derivation never constructs object names.
11. Facts remain valid for one-row tables where first key equals last key.
12. Facts remain valid for one-row tables where commit min equals commit max.

### 8. Determinism

1. Building the same rows with the same config twice yields identical bytes.
2. Building from `FrozenTable` and from its sorted row slice yields identical
   bytes.
3. Building from equivalent generated input across repeated property runs is
   deterministic.
4. Output does not depend on map iteration randomness.
5. Output does not depend on system time.
6. Output does not depend on environment variables.
7. Output does not depend on filesystem paths.

### 9. Error Routing

1. Empty input reports an L5 table-runtime error, not a panic.
2. Unsorted input reports L5 row-order error with bounded key display.
3. Duplicate internal key reports L5 duplicate-key error with bounded key
   display.
4. L3 encoder failures are wrapped as `BuildFormat`.
5. Decode-after-build failures are wrapped as `DecodeFormat`.
6. Error `source()` returns the underlying `FormatError` for wrapped format
   failures.
7. Errors do not expose full oversized keys in display strings.
8. No invalid input mutates or returns a partial artifact.

### 10. Boundary And Vocabulary Guards

1. Builder production code does not import `std::fs`.
2. Builder production code does not import `std::path`.
3. Builder production code does not import `crate::backend`.
4. Builder production code does not import `crate::layout`.
5. Builder production code does not import `crate::service`.
6. Builder production code does not import `crate::branch`, `crate::commit`,
   or `crate::lifecycle`.
7. Builder production code does not import engine crates.
8. Builder production code does not mention old `STRAKV` table bytes except in
   tests or docs.
9. Builder production code does not mention `SegmentBuilder` or old storage
   segment modules.
10. Builder production code does not use product payload vocabulary such as
    `Value`, primitive names, or old table value types.
11. Builder production code does not use read-policy vocabulary such as
    `snapshot`, `as_of`, `visible_at`, `latest`, `ttl_filter`, or
    `live_only`.
12. Builder production code remains crate-private.

## Required Generated Tests

Extend `check_table_runtime_scaffold_contract` or a neighboring hidden
testkit route with a builder case counter.

For each generated script:

1. generate 1 to 256 sorted rows by default;
2. force at least one one-row table;
3. force at least one multi-row one-block table;
4. force at least one multi-block table;
5. include tombstones;
6. include expired-looking rows;
7. include empty values;
8. include nonempty values;
9. include user keys with embedded zero bytes;
10. include duplicate physical keys at different commit versions;
11. include different branch ids;
12. include different storage space ids;
13. vary `target_data_block_size`;
14. vary `rows_per_block`;
15. vary compression between uncompressed and zstd where supported;
16. build from sorted rows;
17. build from frozen table rows;
18. decode through L3 and compare rows to the model;
19. compare artifact facts to decoded facts;
20. build twice and assert deterministic bytes;
21. inject unsorted input and assert rejection;
22. inject duplicate internal-key input and assert rejection;
23. enforce a fixed operation and allocation budget.

Generated tests should keep row sizes bounded so the regular property suite is
fast. Oversized length-limit tests belong in focused unit tests or L3 format
tests, not in every generated script.

## Old Segment Builder Regression Map

Review the old `crates/storage/src/segment_builder.rs` tests and port only the
cases that still match V1.

Port or rewrite:

1. build from sorted memtable input;
2. build with tombstones;
3. timestamp preservation;
4. commit-version ordering;
5. one-block output;
6. multi-block output;
7. compression on/off coverage;
8. deterministic metadata;
9. roundtrip through reader-equivalent decode.

Do not port:

1. old `STRAKV` format-version tests;
2. path creation tests;
3. temp-file cleanup tests;
4. directory fsync tests;
5. old bloom/filter durable block tests;
6. filesystem crash-safety tests;
7. branch-level install tests;
8. segmented flush/read helpers that mix L5 with L6-L8 behavior.

## Review Checklist

Before calling L5E complete, review for these edge cases:

1. Does the builder reject empty input before calling L3?
2. Does the builder reject unsorted rows with an L5 error?
3. Does the builder reject duplicate encoded keys with an L5 error?
4. Does `rows_per_block = 1` produce the expected number of blocks?
5. Does one-row fact derivation handle equal min/max keys and commits?
6. Do tombstones and expired rows survive decode?
7. Do duplicate physical-key versions survive decode?
8. Is byte output deterministic?
9. Are facts derived from decoded bytes, not from a parallel unchecked model?
10. Is there any path, backend, object layout, service, branch, commit,
    lifecycle, or engine import?
11. Is there any old `STRAKV` byte construction in production code?
12. Did source guards gain executable probes for the new forbidden terms?
13. Does the generated property route fail if builder coverage is removed?
14. Do default and no-default-feature testkit lanes still pass?
15. Does wasm check still pass?

## Verification Commands

Run at minimum:

```text
cargo test -p strata-storage-next --locked --lib table::tests::builder
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --locked --test table_runtime_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

If L5E changes any L3 format re-export, also run the focused M3G table-format
tests:

```text
cargo test -p strata-storage-next --locked --lib format::table
```

## Exit Criteria

L5E test coverage is complete when:

1. valid sorted inputs build M3G bytes and decode through L3;
2. invalid inputs reject with typed L5 errors;
3. row preservation is proven for puts, tombstones, expired rows, duplicate
   physical-key versions, branch ids, storage spaces, timestamps, and values;
4. one-block and multi-block artifacts are covered;
5. uncompressed and zstd paths are covered where available;
6. artifact facts match decoded table facts;
7. deterministic-byte tests pass;
8. generated property tests include builder coverage;
9. source guards enforce the L5 boundary;
10. no test relies on old `crates/storage` code as an oracle at runtime.
