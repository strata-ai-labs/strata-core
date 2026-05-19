# L5B Test Plan: Row And Key Adapters

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/l5b-row-key-adapters-implementation-plan.md`

## Goal

Prove that L5B provides a correct, storage-native row/key adapter layer without
pulling in mutable-table behavior or product semantics.

The suite must fail if L5B:

1. sorts rows by anything other than V1 encoded internal-key bytes;
2. orders commit versions ascending for the same physical key;
3. rejects duplicate physical keys at distinct commit versions;
4. accepts duplicate encoded internal keys where uniqueness is required;
5. drops, rewrites, hides, or interprets row metadata;
6. treats branch ids, storage space ids, user keys, tombstones, timestamps, or
   expiry as product semantics;
7. accepts malformed encoded key bytes as canonical keys;
8. constructs malformed key ranges;
9. reports nondeterministic or zero size estimates;
10. imports old product key/value code or upper storage layers.

## Test Locations

Use these locations:

1. `crates/storage-next/src/table/tests/key.rs` for module-local L5B unit
   tests, with `mod key;` from `src/table/tests/mod.rs`.
2. `crates/storage-next/src/testkit/table_runtime.rs` for generated L5B
   scaffold-contract checks.
3. `crates/storage-next/tests/table_runtime_properties.rs` for generated L5B
   property tests behind the `testkit` feature.
4. `crates/storage-next/tests/table_runtime_source_guard.rs` for source-boundary
   scans and executable guard probes.
5. `crates/storage-next/proptest-regressions/table_runtime.txt` only when a
   failing seed is captured.

Do not add a fuzz target in L5B unless the adapter begins accepting arbitrary
encoded bytes from external callers. Byte fuzzing belongs primarily to L3
format decoders and L5F reader open paths.

## Generators

### Branch Bytes

Generate branch ids as opaque 16-byte values:

1. all zero bytes;
2. all `0xff` bytes;
3. incrementing bytes;
4. random bytes;
5. pairs that differ only in the first byte;
6. pairs that differ only in the last byte.

L5B tests must not name branch lifecycle concepts. These bytes are only part of
the physical key sort order.

### Space Names

Generate valid storage spaces:

1. one-byte ASCII names;
2. multi-byte ASCII names;
3. high Unicode names if existing `PhysicalKey` validation accepts them;
4. names with shared prefixes;
5. names that differ only after a long common prefix.

Invalid spaces with empty strings or NUL bytes are already row-constructor
tests. L5B may include smoke coverage but should not duplicate every row-layer
test.

### Storage Space Ids

Generate opaque nonzero storage space ids:

1. `0x01` commit timeline;
2. storage-reserved ids `0x02..=0x1f`;
3. engine-owned ids `0x20..=0xff`;
4. pairs that differ only by storage space id.

L5B must preserve and order these ids as bytes. It must not interpret engine
capabilities from them.

### User Keys

Generate user keys:

1. empty bytes;
2. single byte;
3. bytes containing `0x00`;
4. bytes containing `0x00 0x00`;
5. high-bit bytes;
6. long shared prefixes;
7. one key that is a prefix of another;
8. keys near the default property-test size budget.

### Commit Versions

Generate commit versions:

1. `0`;
2. `1`;
3. adjacent values;
4. `u64::MAX`;
5. repeated physical key with distinct versions;
6. repeated physical key with the same version for duplicate-key tests.

### Rows

Generate rows with:

1. put values of length 0, 1, small random, and large bounded random;
2. tombstones;
3. `expires_at = Timestamp::EPOCH`;
4. nonzero expiry on put rows;
5. commit timestamps that do not correlate with commit version;
6. equal timestamps across different rows.

The model must treat timestamp and expiry as preserved metadata only.

## Required Unit Tests

### 1. Internal-Key Bytes

1. `TableInternalKeyBytes::from_row` equals V1 `encode_internal_key` for a put
   row.
2. `TableInternalKeyBytes::from_row` equals V1 `encode_internal_key` for a
   tombstone row.
3. Encoded bytes round trip through the V1 internal-key decoder.
4. Physical keys sort ascending by branch id bytes.
5. Physical keys sort ascending by space bytes.
6. Physical keys sort ascending by storage space id byte.
7. Physical keys sort ascending by escaped user-key bytes.
8. User keys containing zero bytes preserve ordering and round trip.
9. For the same physical key, higher commit version sorts before lower commit
   version.
10. `CommitVersion::MAX` sorts before `CommitVersion::new(0)` for the same
    physical key.
11. Invalid raw encoded internal-key bytes are rejected if a raw constructor is
    exposed.
12. Canonical raw constructor rejects trailing or noncanonical key bytes by
    decode and re-encode comparison.

### 2. Physical-Key Bytes And Prefixes

1. `TablePhysicalKeyBytes::from_physical_key` equals V1 `encode_physical_key`.
2. Every internal key for the same physical key starts with that physical-key
   byte prefix.
3. Different commit versions share the same physical-key prefix.
4. Different storage space ids do not share an exact physical-key prefix.
5. Different branch ids do not share an exact physical-key prefix.
6. Prefix helper treats prefix bytes mechanically and does not decode product
   meaning.

### 3. Row Adapter

1. Put row adapter preserves physical key, commit version, timestamp, expiry,
   value bytes, and tombstone flag.
2. Empty-value put row is accepted and preserved.
3. Tombstone row is accepted, has no value bytes, and preserves commit facts.
4. Nonzero expiry on a put row is preserved and not interpreted.
5. Commit timestamp ordering does not affect key ordering.
6. Storage-owned ids and engine-owned ids are both preserved.
7. Two rows with identical physical key and different commit versions produce
   different encoded internal keys.
8. Two rows with identical physical key and identical commit version produce
   identical encoded internal keys.
9. Adapter construction does not allocate unbounded diagnostic strings.
10. Adapter debug output, if implemented, does not include full value payloads.

### 4. Sorted-Unique Validation

1. Empty row slice is accepted by generic sorted validation, if that helper is
   intended to be generic.
2. Empty row slice is rejected by nonempty table-input validation, if such a
   helper is added.
3. One row is sorted and unique.
4. Many sorted unique rows are accepted.
5. Unsorted adjacent rows are rejected with `InvalidRowOrder`.
6. Unsorted non-adjacent rows are rejected after sorting is not silently
   applied.
7. Duplicate encoded internal keys are rejected with `DuplicateInternalKey`.
8. Duplicate physical keys at different commit versions are accepted.
9. Duplicate physical keys with versions generated out of descending order are
   rejected until sorted.
10. Rejection reports the offending encoded key or enough bounded context for
    diagnostics.

### 5. Key Bounds

1. Unbounded bounds contain every generated key.
2. Exact-key bounds contain only the exact encoded key.
3. Inclusive lower bound includes the lower key.
4. Exclusive lower bound excludes the lower key.
5. Inclusive upper bound includes the upper key.
6. Exclusive upper bound excludes the upper key.
7. Closed range contains all generated model keys between lower and upper.
8. Open range contains only keys strictly between lower and upper.
9. Lower greater than upper is rejected.
10. Equal lower and upper inclusive bounds produce a singleton range.
11. Equal lower and upper exclusive bounds are well-formed but empty, if the
    chosen API supports empty result ranges.
12. Prefix bounds include all keys with the generated encoded prefix.
13. Prefix bounds exclude adjacent keys that only share decoded product
    meaning, not encoded bytes.
14. Bounds never skip tombstones or expired rows because those are row metadata,
    not key-bound facts.

### 6. Size Accounting

1. Every row estimate is nonzero.
2. Estimate is deterministic across repeated calls.
3. Estimate grows when user-key bytes grow and all other fields are equal.
4. Estimate grows when value bytes grow and all other fields are equal.
5. Tombstone estimate includes key and metadata overhead.
6. Empty-value put estimate is at least key plus metadata overhead.
7. Size estimate does not depend on commit timestamp value except through fixed
   metadata overhead.
8. Size estimate does not depend on expiry value except through fixed metadata
   overhead.
9. Generated rows never produce arithmetic overflow under the property-test
   size budget.

### 7. Error Surface

1. Invalid order errors display a concise storage-mechanical message.
2. Duplicate internal-key errors display a concise storage-mechanical message.
3. Invalid range errors display a concise storage-mechanical message.
4. Invalid raw key errors preserve the source `FormatError`, if raw key
   construction wraps decode failures.
5. Error messages do not mention product capabilities.

## Required Property Tests

Add generated L5B checks to the existing table-runtime property harness.

For each generated script:

1. build a bounded set of `StorageRow` values;
2. adapt them into `TableRow` values;
3. compute expected encoded internal-key bytes using an independent model or the
   V1 key codec as an explicit oracle;
4. sort the model by encoded internal-key bytes;
5. assert adapter ordering equals model ordering;
6. assert duplicate internal keys are rejected;
7. assert duplicate physical keys at distinct versions are accepted;
8. assert generated bounds match independent vector filtering;
9. assert size estimates are nonzero and monotonic for controlled row pairs;
10. assert metadata fields survive adaptation unchanged.

Default property budget:

1. 1 to 64 rows per generated case for normal runs;
2. 65 to 512 rows in one ignored or stress-labelled test if runtime stays
   reasonable;
3. value bytes capped at 1024 by default;
4. user key bytes capped at 256 by default.

The generated test must terminate under a fixed operation and row budget.

## Source Guards

Extend `table_runtime_source_guard` so production files under
`crates/storage-next/src/table/` reject:

1. `crates/storage`;
2. `crate::key_encoding`;
3. `key_encoding::`;
4. old product key imports;
5. old product value imports;
6. `TypeTag`;
7. `Namespace`;
8. `EntityRef`;
9. graph, vector, JSON, search, event, or transaction DTO vocabulary;
10. upper-layer storage modules;
11. direct backend and filesystem APIs;
12. `std::env` and environment reads.

Because storage rows legitimately have value bytes, do not ban the ordinary
word `value` globally. Ban old value types and product payload vocabulary
instead.

Add executable source-guard probes proving the new forbidden terms are caught.

## Sensitivity Probes

Before marking L5B complete, temporarily introduce each mutation and confirm a
targeted test fails:

1. change internal-key version suffix comparison so older versions sort first;
2. omit storage space id from encoded key construction;
3. omit branch id from encoded key construction;
4. allow duplicate encoded internal keys in sorted validation;
5. reject duplicate physical keys at different commit versions;
6. make key bounds treat exclusive bounds as inclusive;
7. make prefix bounds operate on decoded user-key bytes instead of encoded
   prefix bytes;
8. make size estimate return zero for tombstones;
9. import `crates/storage/src/key_encoding.rs` concepts into production L5
   code;
10. add an upper-layer import to production L5 code.

Record the probes in the porting log.

## Verification Commands

Minimum L5B closeout commands:

1. `cargo test -p strata-storage-next --locked table::tests::key`
2. `cargo test -p strata-storage-next --locked --test table_runtime_source_guard`
3. `cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test table_runtime_properties`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## Exit Gate

L5B test coverage is complete when:

1. all key ordering rules from the V1 spec have direct unit tests;
2. generated tests cover branch bytes, storage space ids, user-key bytes,
   commit versions, puts, tombstones, timestamps, and expiry;
3. sorted-unique validation catches unsorted and duplicate internal-key inputs;
4. duplicate physical keys at different versions are proven valid;
5. key bounds are model-tested;
6. size accounting invariants are tested;
7. source guards prove old product key/value code did not leak into L5;
8. no test requires mutable tables, cursors, immutable builders, readers,
   backend objects, or branch runtime behavior.
