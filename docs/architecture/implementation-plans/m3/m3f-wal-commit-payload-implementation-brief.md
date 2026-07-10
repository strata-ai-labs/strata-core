# M3F Implementation Brief: WAL Commit Payload Format

Status: implementation brief

Parent plan: `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`

## Goal

Replace the current opaque WAL commit payload with a storage-row-native V1
commit payload format.

This is an L3 durable format correction. It closes the known gap where
`WalRecord` carries `commit_payload: Vec<u8>` and therefore permits future L7
work to smuggle engine-shaped or ad hoc payload bytes into the storage WAL.

After this slice, a WAL record still carries its existing outer facts:

1. `CommitVersion`
2. `BranchId`
3. commit `Timestamp`
4. row-native commit payload bytes

But the payload bytes are no longer arbitrary. They decode as a bounded,
versioned batch of `StorageRow` records whose commit facts match the outer WAL
record.

## Inputs Read

Architecture and format inputs:

1. `docs/architecture/storage/l3-durable-format-codec.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/storage/l7-commit-runtime.md`
4. `docs/spec/strata-storage-format-v1.md`
5. `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md`
6. `docs/architecture/implementation-plans/m3e2-wal-service-implementation-brief.md`
7. `docs/architecture/implementation-plans/m3e2-wal-test-suite-plan.md`

Current implementation inputs:

1. `crates/storage-next/src/format/wal.rs`
2. `crates/storage-next/src/format/storage_row.rs`
3. `crates/storage-next/src/format/key.rs`
4. `crates/storage-next/src/row/mod.rs`
5. `crates/storage-next/src/service/wal.rs`
6. `crates/storage-next/src/service/wal/tests.rs`
7. `crates/storage-next/testdata/goldens/storage-format-v1/`
8. `crates/storage-next/fuzz/fuzz_targets/format_wal_record.rs`

Current old-code evidence:

1. `crates/storage/src/durability/payload.rs`
2. `crates/storage/src/durability/commit_adapter.rs`
3. `crates/storage/src/durability/format/wal_record.rs`
4. `crates/storage/src/txn/context.rs`
5. `crates/storage/src/stored_value.rs`
6. `crates/storage/src/key_encoding.rs`

## Scope

In scope:

1. L3 WAL commit payload byte format.
2. Storage-row batch encoder and decoder.
3. WAL record encode/decode integration with the row-native payload.
4. Golden vectors for the commit payload and updated WAL records.
5. Format and WAL service tests proving rows survive append/read/reopen.
6. Fuzz/testkit routing updates for the new decoder surface.
7. Spec updates to `docs/spec/strata-storage-format-v1.md`.
8. Porting-log entry explaining which old payload behavior is preserved,
   rewritten, or retired.

Out of scope:

1. L7 commit runtime implementation.
2. Public storage API design.
3. Engine primitive encoding.
4. Table format implementation.
5. Snapshot row-section implementation beyond ensuring the row format remains
   reusable.
6. Object-store WAL chunking or OpenDAL durability.
7. Compatibility readers for pre-V1 development payloads.

## Existing Behavior To Preserve

1. WAL segment headers remain unchanged.
2. WAL record outer envelopes remain unchanged.
3. WAL inner records remain self-delimiting and protected by length CRC plus
   payload CRC.
4. WAL record outer facts remain `commit_version`, `branch_id`, and
   `commit_timestamp`.
5. WAL record decode still returns the exact byte count consumed.
6. WAL service append, rotation, durability-policy, partial-tail, and retention
   mechanics remain service behavior rather than payload behavior.
7. `StorageRow` remains the canonical row byte format for storage rows.
8. L3 errors remain storage-mechanical and avoid product terms.

## Intentional V1 Changes

1. `WalRecord` must stop exposing the commit payload as arbitrary engine bytes.
2. The stable WAL record payload is a V1 storage-row commit batch.
3. A decoded WAL record must expose decoded rows or a strongly typed
   `WalCommitPayload`, not only raw payload bytes.
4. WAL replay consumers must be able to recover committed storage rows without
   engine primitive decoders.
5. Opaque byte payload tests from M3E2 remain useful only for envelope framing;
   they must be rewritten or narrowed so they do not bless arbitrary payloads as
   valid commit payloads.
6. Golden vectors for `wal_record_empty_payload` and arbitrary non-empty WAL
   payloads must be retired or renamed if the new payload rejects empty or
   non-row bytes.

## Proposed Commit Payload Byte Shape

The WAL commit payload is nested inside the existing inner WAL record
`commit_payload` field.

```text
payload_magic          4 bytes   "STCP"
format_version         u32 LE, MUST be 1
row_count              u32 LE, MUST be nonzero
rows                   repeated row frame
```

Each row frame is:

```text
row_len                u32 LE, MUST be nonzero
row_bytes              row_len bytes, V1 StorageRow encoding
```

The payload has no separate checksum. The enclosing WAL inner-record
`payload_crc32` protects the commit payload bytes. If this payload is ever
stored outside a WAL record, that future object family must define its own
integrity boundary instead of reusing this nested shape silently.

Constants:

```text
WAL_COMMIT_PAYLOAD_MAGIC           "STCP"
WAL_COMMIT_PAYLOAD_FORMAT_VERSION  1
```

Implementation should define explicit allocation guards:

1. maximum rows per payload
2. maximum encoded payload bytes accepted by the decoder
3. maximum encoded row length accepted before slicing/allocating

The exact numeric guards may be implementation constants, but they must be
documented in the format module and tested with small deterministic fixtures.

## Row Validation Rules

The payload decoder should decode row bytes mechanically. The WAL record
decoder or a validation helper must then validate payload rows against the WAL
record outer facts:

1. `row.commit_version == wal_record.commit_version`
2. `row.commit_timestamp == wal_record.commit_timestamp`
3. `row.physical_key.branch_id == wal_record.branch_id`
4. row order is preserved exactly as encoded
5. row count is nonzero
6. tombstone semantics are delegated to `StorageRow` decode
7. duplicate physical keys are not rejected by L3 unless a later L7 contract
   explicitly requires commit-batch coalescing before WAL encode

The validation is storage-mechanical. It does not interpret storage space IDs,
user keys, values, JSON, graph, vector, search, event, or other engine
semantics.

## API Shape

Prefer a small L3 type:

```text
WalCommitPayload {
    rows: Vec<StorageRow>
}
```

The exact Rust names may change, but the ownership should not:

1. L3 owns encode/decode of the payload bytes.
2. `WalRecord` owns outer WAL facts plus a typed commit payload.
3. L4 WAL service appends and reads `WalRecord` values without interpreting row
   product meaning.
4. L7 later builds `WalCommitPayload` from a validated commit batch.

Avoid exposing a public "raw payload escape hatch" that normal tests can use to
construct arbitrary valid WAL records. Test-only helpers may corrupt encoded
bytes after a valid encode, but valid construction should go through rows.

## Error Mapping

Use existing `FormatError` variants where possible:

1. bad payload magic -> `InvalidMagic { format: "wal_commit_payload" }`
2. version `0` -> `PreV1Format`
3. version greater than `1` -> `FutureFormat`
4. zero row count -> `InvalidValue { field: "row_count" }`
5. oversized row count -> `InvalidLength { field: "row_count" }`
6. zero row length -> `InvalidLength { field: "row_len" }`
7. row frame truncation -> `InsufficientBytes`
8. storage row decode failure -> propagate the row `FormatError`
9. trailing payload bytes -> strict decode failure
10. row/outer fact mismatch -> `InvalidValue` with a precise field such as
    `commit_version`, `branch_id`, or `commit_timestamp`

If existing `FormatError` lacks a precise enough variant, add one only if it is
useful across durable formats. Do not introduce product-language errors.

## Spec And Golden Updates

This slice must update `docs/spec/strata-storage-format-v1.md`:

1. WAL record requirement #4 must say row-native commit payload, not opaque
   bytes.
2. Section 11 must become concrete instead of provisional.
3. Golden-vector inventory must include row-native commit payload vectors and
   updated WAL record vectors.
4. Any reference to MessagePack, transaction payloads, or primitive payloads
   must be marked as historical evidence only.

Golden vectors should include at least:

1. commit payload with one put row
2. commit payload with put plus tombstone
3. WAL record containing one row-native commit payload
4. WAL envelope around that WAL record

If old arbitrary-payload WAL goldens remain, their names must make clear that
they are envelope/framing corruption fixtures, not valid V1 commit records.

## Source Map And Retirement Notes

Before implementation, update `m3-porting-log.md` with:

1. old payload files inspected
2. behavior preserved: durable commit carries version, branch, timestamp, TTL,
   tombstones, and row values
3. behavior intentionally changed: no `EntityRef`, primitive tags, MessagePack
   transaction payload, or opaque test payloads in valid V1 WAL records
4. deferred behavior: L7 commit-batch construction and conflict validation
5. old tests that remain only as evidence until L7 consumes the new payload

## Implementation Slices

Suggested slices:

1. `M3F1`: Add `WalCommitPayload` encode/decode and focused tests.
2. `M3F2`: Integrate `WalRecord` with typed payloads and update WAL format
   tests/goldens/spec.
3. `M3F3`: Update WAL service tests so append/read/reopen preserve decoded
   rows, and remove arbitrary-valid-payload assumptions.
4. `M3F4`: Add fuzz routing and closeout docs.

The slices may merge if the patch stays reviewable, but each closeout must
record which old arbitrary-payload behavior was retired.

## Verification Commands

Use narrow commands while developing:

```sh
cargo test -p strata-storage-next --locked format::wal
cargo test -p strata-storage-next --locked wal_record
cargo test -p strata-storage-next --locked wal_commit_payload
cargo test -p strata-storage-next --locked service::wal
cargo test -p strata-storage-next --no-default-features --locked service::wal
```

Close with:

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

Run the WAL fuzz target after the format compiles:

```sh
cd crates/storage-next && cargo +nightly fuzz run format_wal_record -- -runs=4096
```

If the fuzz target name changes, record the final command in the tracker.

## Exit Gate

M3F is complete when:

1. Valid WAL records cannot be constructed with arbitrary opaque payload bytes.
2. WAL commit payload bytes decode as a bounded batch of storage rows.
3. Payload rows are validated against the WAL record outer commit version,
   branch id, and timestamp.
4. Golden vectors and the format spec describe the stable row-native bytes.
5. WAL service append/read/reopen tests prove rows survive unchanged.
6. Malformed payloads fail closed before allocation bombs or partial replay.
7. Fuzz routing covers the commit payload decoder.
8. The progress tracker records sensitivity probes and verification commands.
