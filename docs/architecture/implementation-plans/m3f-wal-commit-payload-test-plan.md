# M3F Test Plan: WAL Commit Payload Format

Status: test-suite plan

Parent brief:
`docs/architecture/implementation-plans/m3f-wal-commit-payload-implementation-brief.md`

## Goal

Prove that WAL commit payloads are storage-row-native durable bytes, not opaque
engine payloads.

The suite must fail if a valid WAL record can be constructed from arbitrary
bytes, if decoded rows can disagree with the WAL record's outer commit facts,
or if malformed payload bytes can produce partial replay state.

## Testing Principles

1. Test storage mechanics, not product semantics.
2. Valid construction goes through `StorageRow` values.
3. Invalid construction mutates encoded bytes after a valid encode or uses
   explicit malformed byte fixtures.
4. Every accepted payload preserves row order and exact row bytes.
5. Every rejected payload fails before any caller can install a partial row
   batch.
6. Allocation guards are tested with deterministic small limits or fixtures.
7. Golden vectors are part of the contract and must change only with an
   intentional format update.
8. Sensitivity probes must prove tests fail if arbitrary payloads are accepted
   or if row/outer-fact validation is bypassed.

## Required Cases

### 1. Commit Payload Codec

1. One put row round-trips.
2. Put row plus tombstone row round-trips in encoded order.
3. Empty row list is rejected before bytes are emitted or accepted.
4. Payload magic mismatch returns `InvalidMagic`.
5. Payload version `0` returns pre-V1 format.
6. Payload future version returns future format with max-supported version `1`.
7. Zero row count in durable bytes is rejected.
8. Row count above the implementation limit is rejected before allocating row
   storage.
9. Zero row length is rejected.
10. Row length that exceeds remaining payload returns insufficient bytes.
11. Row length that exceeds the implementation limit is rejected before slicing
    or allocating.
12. Truncated row bytes propagate a storage-row decode error or insufficient
    bytes with the correct format family.
13. Corrupt nested storage-row version is rejected.
14. Corrupt nested storage-row flags are rejected.
15. Tombstone row with value bytes is rejected through the nested row decoder.
16. Trailing bytes after the declared rows are rejected.
17. Encoding is deterministic for the same row sequence.
18. Decoding does not reorder rows or deduplicate duplicate physical keys.

### 2. WAL Record Integration

1. `WalRecord::new` or its replacement constructor accepts a typed
   `WalCommitPayload`, not arbitrary `Vec<u8>`, for normal valid construction.
2. Encoding a WAL record with one put row and decoding it returns the same
   commit version, branch id, timestamp, and row payload.
3. Encoding a WAL record with put plus tombstone rows and decoding it preserves
   both rows in order.
4. Payload rows whose commit version differs from the outer WAL commit version
   are rejected.
5. Payload rows whose physical-key branch differs from the outer WAL branch id
   are rejected.
6. Payload rows whose commit timestamp differs from the outer WAL timestamp are
   rejected.
7. Multiple rows for different user keys in the same branch/version are
   accepted.
8. Duplicate physical keys in one payload are preserved in order unless L7
   later introduces a coalescing rule; L3 must not silently drop them.
9. Payload CRC mutation still fails before payload decode.
10. Length CRC mutation still fails before trusting record length.
11. Payload byte mutation with refreshed payload CRC reaches payload validation
    and fails as a commit-payload or storage-row format error.
12. WAL record version pre-V1 and future-version routing is unchanged.
13. `decode_wal_record` still returns exact bytes consumed when a valid record
    is followed by another record.

### 3. WAL Envelope And Segment Compatibility

1. Existing outer envelope length and length-CRC behavior remains unchanged.
2. Envelope decode still returns encoded inner-record bytes without interpreting
   rows.
3. WAL service recovery still classifies latest-segment partial tails using the
   envelope/record boundaries, not row payload internals.
4. Segment header behavior is unchanged.
5. A non-latest segment with a malformed row-native payload is hard format
   corruption.
6. A latest segment ending inside the commit payload row bytes is a latest-tail
   truncation fact only when the truncation is physically at the end of the
   latest segment.

### 4. Golden Vectors

1. Commit payload with one put row matches a checked-in golden.
2. Commit payload with put plus tombstone rows matches a checked-in golden.
3. WAL record containing one row-native commit payload matches a checked-in
   golden.
4. WAL record envelope containing that WAL record matches a checked-in golden.
5. Existing WAL empty-payload golden is removed, renamed as a malformed fixture,
   or explicitly marked non-V1-valid.
6. Golden tests fail if row order changes.
7. Golden tests fail if nested storage-row bytes change without an intentional
   storage-row golden update.

### 5. WAL Service Roundtrip

1. Appending a single-row WAL record and reading it back returns the same row.
2. Appending a multi-row WAL record and reading it back preserves row order.
3. Reopen after standard append reads row-native payloads unchanged.
4. Reopen after rotation reads row-native payloads across segments unchanged.
5. `read_after_commit_version(0)` over row-native records still returns all
   records.
6. `read_after_commit_version(MAX)` over row-native records still returns none.
7. Duplicate commit-version filtering is based on outer WAL commit version, not
   row count or nested row keys.
8. Records for different branches are not interpreted as product branch
   behavior; they are returned with their decoded storage rows.
9. Backend append failure leaves no partially accepted row payload in service
   facts.
10. Record-too-large preflight accounts for row-native payload byte size.

### 6. Allocation And Size Bounds

1. Payload with `row_count = u32::MAX` is rejected before row vector allocation.
2. Payload with a row length that would overflow total-frame arithmetic is
   rejected.
3. Payload whose declared row lengths sum past the payload end is rejected.
4. Payload at the maximum allowed test row count decodes when total bytes are
   within the configured limit.
5. Payload one row above the allowed test row count is rejected.
6. A WAL record whose row-native payload exactly fits the remaining segment
   space appends without rotation.
7. A WAL record whose row-native payload exceeds the remaining segment space by
   one byte rotates before append.
8. A WAL record whose row-native payload exceeds segment size is rejected
   before backend mutation.

### 7. Fuzz And Property Coverage

1. The WAL record fuzz target routes decoded inner records through commit
   payload validation.
2. The commit payload decoder has a direct fuzz route if the WAL fuzz route is
   too indirect to cover row-count and row-length guardrails.
3. Fuzz invariant: arbitrary bytes never panic.
4. Fuzz invariant: successful decode consumes all commit payload bytes.
5. Fuzz invariant: successful payload decode has nonzero row count.
6. Fuzz invariant: successful WAL record decode has every row matching outer
   commit version, branch id, and timestamp.
7. Property test generates 1 to 64 rows, value sizes 0 to 512 bytes, put and
   tombstone rows, and optional duplicate physical keys.
8. Property model asserts encode/decode identity and exact row order.
9. Regression seeds go under
   `crates/storage-next/proptest-regressions/wal_commit_payload.txt` only if a
   failing seed is captured.

### 8. No Engine Payload Leakage

1. No valid WAL record test constructs a payload from arbitrary string or
   MessagePack bytes.
2. No storage-next WAL format code imports engine crates or product primitive
   types.
3. The old `EntityRef` or primitive-tag payload shape is not accepted as a
   valid V1 commit payload.
4. The spec marks old primitive/MessagePack payloads as historical evidence
   only.
5. Source scans should reject test names or comments that describe valid V1 WAL
   payloads as primitive, transaction, entity, JSON, graph, vector, or search
   payloads.

## Sensitivity Probes

Each implementation closeout should record at least three probes:

1. Temporarily allow `WalRecord` valid construction from arbitrary bytes.
   Expected failure: a no-engine-payload or arbitrary-payload rejection test.
2. Temporarily skip branch-id validation between row physical keys and the
   outer WAL record. Expected failure: branch mismatch test.
3. Temporarily skip commit-version validation between rows and the outer WAL
   record. Expected failure: commit-version mismatch test.
4. Optional: temporarily ignore trailing commit-payload bytes. Expected failure:
   trailing-byte test.
5. Optional: temporarily allocate row vector directly from unchecked row count.
   Expected failure: allocation-guard test or clippy/review rejection.

The mutation must be reverted before closeout. The progress tracker must name
the failing test and the verification command that passed after revert.

## Suggested Test Layout

Prefer splitting tests before files become large:

1. `crates/storage-next/src/format/wal/payload_tests.rs` or an equivalent
   module-local split for payload codec tests.
2. Existing `format::wal` tests for segment, envelope, and inner-record framing.
3. Existing `service::wal::tests::append/read/corruption` modules for service
   behavior.
4. `crates/storage-next/tests/format_golden.rs` for golden vector registration.
5. `crates/storage-next/src/testkit/format_fuzz.rs` for hidden fuzz routing.

Do not create production module names with roadmap labels.

## Verification Commands

Narrow commands:

```sh
cargo test -p strata-storage-next --locked wal_commit_payload
cargo test -p strata-storage-next --locked format::wal
cargo test -p strata-storage-next --locked service::wal
cargo test -p strata-storage-next --no-default-features --locked service::wal
cargo test -p strata-storage-next --locked format_golden
```

Property/fuzz commands:

```sh
PROPTEST_CASES=2048 cargo test -p strata-storage-next --locked wal_commit_payload_model
cd crates/storage-next && cargo +nightly fuzz run format_wal_record -- -runs=4096
```

Broad commands:

```sh
cargo test -p strata-storage-next --locked
cargo test -p strata-storage-next --no-default-features --locked
cargo test -p strata-storage-next --all-features --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo doc -p strata-storage-next --no-deps --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

## Exit Gate

The suite is complete when:

1. Every required case above is covered by executable tests or explicitly
   classified as unreachable with a code reference.
2. Golden vectors cover the stable row-native payload and WAL record shape.
3. WAL service tests no longer bless arbitrary payload bytes as valid commit
   payloads.
4. Fuzz/property coverage exercises row-count and row-length guardrails.
5. Sensitivity probes are recorded and reverted.
6. The spec, implementation brief, test plan, porting log, and progress tracker
   agree on the final payload format.
