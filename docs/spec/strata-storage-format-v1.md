# Strata Storage Format V1

Status: Draft / Unstable / Not Yet A Compatibility Promise

Audience: Strata implementers, storage-tool authors, dataset publishers,
backup/repair tooling authors, and reviewers of the storage-next rewrite.

Related architecture draft:
[Storage-Next L3. Durable Format / Codec](../architecture/storage-next/l3-durable-format-codec.md)

## 1. Purpose

This document is the draft public specification for Strata's durable storage
format.

The storage format must eventually be a published contract, not merely behavior
encoded in Rust source files. Users should be able to back up, clone, inspect,
repair, validate, and distribute Strata databases without reverse engineering a
specific implementation commit.

This draft is intentionally written during the L3 architecture pass. It records
what is already clear, marks provisional areas explicitly, and prevents
storage-next from accidentally burying format decisions in implementation code.

## 2. Status And Compatibility

This document is not yet a compatibility promise.

While the status is `Draft / Unstable`:

1. Field layouts may change.
2. Object names may change.
3. Codec behavior may change.
4. Pre-v1 compatibility may be broken deliberately.
5. Golden vectors are not yet authoritative.

Strata is pre-launch. Storage Format V1 should not carry compatibility
machinery for development-era databases. Stable V1 open should reject pre-v1
layouts clearly unless an explicit developer conversion tool is being run.

After this document is promoted to `Format V1 Stable`, Strata implementations
that claim support for storage format V1 must follow the stable version of this
spec.

This draft uses `MUST`, `SHOULD`, and `MAY` to describe the intended eventual
contract. While unstable, those words guide implementation but do not create a
release compatibility guarantee.

## 3. Scope

This specification covers:

- database object namespace
- durable byte order and scalar encodings
- storage codec registry behavior
- manifest format
- WAL segment format
- WAL record format
- commit payload format
- snapshot/checkpoint container format
- snapshot/checkpoint section framing
- storage row encoding
- internal key ordering
- immutable table format
- checksums and authenticated integrity
- format versioning and strict decode rules
- golden vectors and conformance tests

This specification does not cover:

- public engine API
- IPC protocol
- StrataHub protocol
- query language
- graph traversal semantics
- vector search semantics
- BM25/search ranking semantics
- inference or model execution
- user-facing branch workflows
- product error messages

Storage format V1 is a storage mechanics contract. Engine defines the product
meaning of bytes stored in rows.

## 4. Terminology

`Database root`: The root object namespace for one Strata database.

`Object`: A named byte string addressed through the storage backend. A local
filesystem backend maps objects to files. A browser/cache backend maps objects
to in-memory keys. A future object-store backend maps objects to object keys.

`Manifest`: Durable database metadata used to identify the database, codec, WAL
state, snapshot state, and flush state.

`WAL`: Write-ahead log.

`Snapshot`: A durable checkpoint object containing row-native storage state at a
specific recovery watermark. A snapshot may also carry opaque engine-owned
derived sections, but those sections are not required to recover committed
storage rows.

`Table`: An immutable sorted storage object used by the table/LSM runtime.

`Storage row`: A generic row persisted by storage. A row is not a JSON
document, graph edge, vector, event, or search posting. Engine maps those
concepts into physical keys and values.

`Physical key`: The byte sequence used by storage to order and group rows.

`Internal key`: `physical key || descending commit version`, used inside sorted
tables and memtables.

`Codec`: A byte transformation applied at documented durability boundaries.
Codecs may be identity, encryption, compression, or a future composition, but
each boundary must say exactly what is transformed.

## 5. General Encoding Rules

Unless a section says otherwise:

1. Integers are little-endian.
2. Fixed byte arrays are copied as raw bytes.
3. Variable byte strings are length-prefixed with an unsigned integer whose
   size is specified by the enclosing format.
4. UTF-8 strings MUST be valid UTF-8.
5. Decoders MUST reject insufficient data.
6. Decoders MUST reject future format versions unless the format explicitly
   defines forward-compatible extension handling.
7. Decoders MUST reject trailing data unless the format explicitly defines an
   extension area.
8. Decoders MUST NOT allocate unbounded memory solely because a length or count
   field says to do so.
9. Checksums and authentication tags MUST be verified before decoded content is
   trusted.

## 6. Object Namespace

The V1 object namespace should be small and direct. It does not use a `v1/`
prefix by default; the manifest and object byte formats declare the storage
format. A future incompatible storage format can introduce a new namespace only
if it has to coexist with V1 data.

The current target namespace reserves these object families:

```text
manifest/
wal/
tables/
snapshots/
tmp/
quarantine/
locks/
meta/
```

Provisional canonical layout:

```text
manifest/current

wal/<segment-id>

tables/<branch-id>/<level>/<table-id>
tables/<branch-id>/manifest

snapshots/<snapshot-id>

tmp/<operation-id>/<object-id>

quarantine/<branch-id>/<object-id>
quarantine/<branch-id>/manifest

locks/writer

meta/database
```

Object names MUST be database-relative. They MUST NOT contain absolute paths,
empty path components, `.` components, `..` components, backend URL syntax, or
platform path separators other than `/`.

The V1 object namespace SHOULD use ASCII-only names. The stable spec will
define exact ID encodings for `branch-id`, `segment-id`, `snapshot-id`, and
`table-id`.

## 7. Codec Registry

Every durable database has a configured storage codec identity.

Stable V1 requires only the identity codec:

| Codec ID | Meaning | Status |
| --- | --- | --- |
| `identity` | No byte transformation | Required V1 default |

The manifest or static database metadata MUST record the codec identity.
Opening a database with a mismatched codec identity MUST fail before replay or
mutation.

Codec identity comparison is exact and case-sensitive.

### 7.1 Identity Codec

The `identity` codec returns input bytes unchanged.

### 7.2 AES-GCM-256 Codec

`aes-gcm-256` exists in the current implementation and remains useful evidence,
but it is not required for stable V1 until encryption configuration and key
management are productized.

Current AES-GCM evidence:

```text
nonce              12 bytes
ciphertext_and_tag variable bytes, includes 16-byte GCM tag
```

The current implementation obtains the AES key from
`STRATA_ENCRYPTION_KEY`. That is not yet the target V1 contract. The stable V1
spec should define how codec configuration is passed at open time without
hidden process-global environment coupling.

If AES-GCM is exposed by a build, decode failure MUST be treated as an
integrity failure.

### 7.3 Codec Boundaries

Codec boundaries are format-specific.

Current evidence:

1. WAL record payloads are encoded through the configured codec before they are
   written into codec-aware WAL segment envelopes.
2. Snapshot containers record and validate the database codec ID.
3. Current primitive snapshot section payloads use a canonical section codec
   independent from the database codec.

The stable V1 spec must make every codec boundary explicit. A codec MUST NOT be
implicitly applied to an object family without this specification saying so.

## 8. Manifest Format

The manifest stores physical database metadata.

Current manifest evidence:

```text
magic                         4 bytes   "STRM"
format_version                u32 LE
database_uuid                 16 bytes
codec_id_len                  u32 LE
codec_id                      codec_id_len bytes, UTF-8
active_wal_segment            u64 LE
snapshot_watermark            u64 LE, 0 means none
snapshot_id                   u64 LE, 0 means none
flushed_through_commit_id     u64 LE, 0 means none
crc32                         u32 LE over all preceding bytes
```

Current manifest constants:

```text
MANIFEST_MAGIC                  "STRM"
MANIFEST_FORMAT_VERSION         2
MIN_SUPPORTED_MANIFEST_VERSION  2
```

Draft V1 requirements:

1. The stable V1 manifest format version is `1`.
2. The manifest MUST identify the database.
3. The manifest MUST record the configured codec identity.
4. The manifest MUST record enough WAL and snapshot facts to run recovery.
5. The manifest MUST be protected by a checksum or authenticated integrity
   mechanism.
6. The manifest decoder MUST reject invalid magic, pre-v1 development formats,
   future formats, invalid codec strings, insufficient data, and checksum
   mismatch.
7. The manifest database identity is a storage-local physical database
   identity and recovery fact. It is not a StrataHub fleet, instance, dataset,
   or bundle identity. StrataHub must compose its own identifiers and
   provenance above the storage format.

## 9. WAL Segment Format

The WAL segment format stores committed records.

Current WAL segment evidence:

```text
segment_magic          4 bytes   "STRA"
format_version         u32 LE
segment_number         u64 LE
database_uuid          16 bytes
header_crc             u32 LE over first 32 bytes
records                repeated WAL segment records
```

Current WAL segment constants:

```text
SEGMENT_MAGIC                    "STRA"
SEGMENT_FORMAT_VERSION           3
MIN_SUPPORTED_SEGMENT_VERSION    3
SEGMENT_HEADER_SIZE              32 bytes
SEGMENT_HEADER_SIZE_V2_V3        36 bytes
```

Segment version history from the current implementation:

1. v1: original 32-byte header, no CRC.
2. v2: 36-byte header with CRC32 over the first 32 bytes.
3. v3: per-record outer envelope for codec-aware reads.

Current v3 segments reject pre-v3 segment headers as unsupported development
formats.

Draft V1 requirements:

1. The stable V1 WAL segment format version is `1`.
2. Each WAL segment MUST have a self-identifying header.
3. The header MUST bind the segment to a database identity.
4. The header MUST record the segment number.
5. The header MUST have an integrity check.
6. Segment number mismatch between object name and header MUST be rejected
   when the object name provides an expected segment number.
7. Future segment versions MUST be rejected.
8. Pre-v1 development segment versions MUST produce a typed unsupported-format
   failure.
9. The current v3 outer envelope is retained as the V1 design idea, but the
   stable public version starts at segment format version 1.

Open issue: The stable V1 spec must decide whether WAL segments remain
file-like append logs or become object-published immutable log chunks.

## 10. WAL Record Format

Current WAL record evidence uses a v2 inner record format.

```text
record_len             u32 LE, number of bytes after this field
format_version         u8, currently 2
record_len_crc32       u32 LE, CRC32 over record_len bytes
txn_id                 u64 LE
branch_id              16 bytes
timestamp_micros       u64 LE
commit_payload         variable bytes
payload_crc32          u32 LE
```

`payload_crc32` covers:

```text
format_version || record_len_crc32 || txn_id || branch_id ||
timestamp_micros || commit_payload
```

Current parser behavior:

1. v2 verifies `record_len_crc32` before trusting `record_len`.
2. v2 verifies `payload_crc32` before parsing fields.
3. v1 records lack `record_len_crc32` and remain parseable in the record parser
   even though v3 segments reject pre-v3 segment headers.
4. Unknown record versions are rejected.

Draft V1 requirements:

1. WAL records MUST be self-delimiting.
2. WAL records MUST detect torn writes to the length field.
3. WAL records MUST detect payload corruption.
4. WAL records MUST carry commit identity, branch identity, commit timestamp,
   and commit payload bytes.
5. WAL record decode MUST return the exact byte count consumed.
6. The codec-aware outer envelope is WAL segment framing. The logical WAL
   record begins after the segment frame payload has been decoded.

## 11. Commit Payload Format

This section is intentionally provisional.

Current implementation evidence has two commit payload families:

1. Legacy writesets encode mutations using `EntityRef` and primitive tags.
2. Current transaction payloads encode version, puts, deletes, and TTLs using
   MessagePack over storage `Key` and `Value`.

Storage-next V1 must not use primitive-shaped commit payloads as the storage
contract. It should use a storage-native binary encoding, not MessagePack, for
the stable commit payload.

Draft V1 commit payload should be row-native:

```text
commit_version         u64
mutation_count         u32 or varint, exact size TBD
mutations              repeated storage-row mutation
```

A storage-row mutation should contain:

```text
operation_kind         put | delete
physical_key           bytes
value                  bytes, empty for delete unless explicitly allowed
timestamp_micros       u64
expires_at_micros      u64, 0 means no expiry
row_flags              reserved storage flags
```

Draft V1 requirements:

1. Commit payloads MUST be storage-mechanical, not engine-primitive-shaped.
2. Commit payloads MUST be deterministic for the same mutation sequence.
3. Commit payloads MUST preserve all data required for WAL replay.
4. Commit payloads MUST support tombstones.
5. Commit payloads MUST support retained row metadata needed by history,
   `getv`, and timestamp-bounded `as_of`.
6. Commit payloads MUST be easy to fuzz and specify without relying on a
   serde data model.

## 12. Snapshot Container Format

The snapshot container stores checkpoint data at a recovery watermark.

Current snapshot evidence:

```text
snapshot_magic         4 bytes   "SNAP"
format_version         u32 LE
snapshot_id            u64 LE
watermark_txn          u64 LE
created_at_micros      u64 LE
database_uuid          16 bytes
codec_id_len           u8
reserved               15 bytes
codec_id               codec_id_len bytes, UTF-8
sections               repeated snapshot sections
footer_crc32           u32 LE
```

Current snapshot constants:

```text
SNAPSHOT_MAGIC                    "SNAP"
SNAPSHOT_FORMAT_VERSION           2
MIN_SUPPORTED_SNAPSHOT_VERSION    2
SNAPSHOT_HEADER_SIZE              64 bytes
```

Draft V1 requirements:

1. The stable V1 snapshot container format version is `1`.
2. A snapshot MUST identify the database.
3. A snapshot MUST identify its snapshot id.
4. A snapshot MUST identify the recovery watermark it covers.
5. A snapshot MUST record or validate the database codec identity.
6. A snapshot MUST have an integrity check over the container.
7. A snapshot MUST consist of zero or more length-delimited sections.
8. Snapshot decode MUST fail before install if any section is corrupt.

Open issue: The stable spec must decide whether snapshot `watermark_txn` is a
transaction id, a commit version, or a unified storage recovery watermark.

## 13. Snapshot Section Format

Current section header evidence:

```text
section_type           u8
section_data_len       u64 LE
section_data           section_data_len bytes
```

Current section types are primitive tags:

```text
KV       0x01
Event    0x02
Branch   0x03
JSON     0x04
Vector   0x05
Graph    0x06
```

These primitive tags are current-format evidence, not target storage ownership.

Draft V1 direction:

1. Storage owns the section envelope.
2. Committed storage state uses row-native storage snapshot sections.
3. Engine owns opaque derived-state section payload semantics if such sections
   remain.
4. Unknown storage-owned section types MUST be rejected unless the section is
   explicitly marked skippable by the format.
5. Opaque engine-owned section types MAY exist only if their ownership and
   install path are explicit.
6. Opaque engine-owned sections MUST NOT be required to recover committed
   storage rows.

## 14. Primitive Snapshot Payloads

This section documents current evidence only. It is not a target V1 storage
ownership decision and is not a required migration format for stable V1.

Current primitive snapshot payloads encode:

- KV rows with branch id, space, type tag, user key, value, version, timestamp,
  TTL, and tombstone marker
- event rows with branch id, space, sequence, payload, version, and timestamp
- branch rows with branch id, key, value, version, timestamp, and tombstone
  marker
- JSON rows with branch id, space, document id, content, version, timestamp,
  and tombstone marker
- vector collection rows with branch id, space, collection name, config,
  config version, config timestamp, and vector entries
- vector entries with key, vector id, embedding, metadata, raw value, version,
  timestamp, and tombstone marker
- graph rows through KV-like typed storage

Current primitive payload decoders reject trailing data. That strictness should
carry into any stable V1 payload format.

Target V1 storage format should not require storage to understand this list as
product semantics.

## 15. Storage Row Format

Storage row format is not yet frozen.

Draft V1 row fields:

```text
physical_key           bytes
commit_version         u64
timestamp_micros       u64
value                  bytes
tombstone              bool
expires_at_micros      u64, 0 means no expiry
row_flags              reserved storage flags
```

Draft V1 requirements:

1. Rows MUST be generic.
2. Rows MUST support deletion tombstones.
3. Rows MUST preserve enough metadata for latest reads, version-bounded reads,
   history reads, and timestamp-bounded reads.
4. Rows MUST be encodable in WAL payloads, snapshots, and immutable tables
   without changing product meaning.
5. Rows MUST carry expiry metadata. A zero expiry means no expiry.

## 16. Internal Key Encoding

Current internal key evidence:

```text
InternalKey = TypedKeyBytes || EncodeDesc(commit_version)
```

Current typed key layout:

```text
branch_id              16 bytes
space                  UTF-8 bytes terminated by 0x00
storage_space_id       1 byte
user_key               byte-stuffed bytes terminated by 0x00 0x00
descending_commit      8 bytes, big-endian bitwise-NOT of commit_version
```

User key byte-stuffing:

```text
0x00 in source bytes   encoded as 0x00 0x01
terminator             encoded as 0x00 0x00
```

Ordering property:

1. Physical keys sort ascending.
2. Versions for the same physical key sort newest first.
3. The first live row for a physical key is the latest value.
4. The first live row with `commit_version <= requested_version` is the `getv`
   result.
5. History is the retained row sequence for a physical key.

This ordering strategy is expected to remain central to V1.

`storage_space_id` is an opaque engine-assigned storage family byte. Storage
may order, route, and scan by it, but it must not interpret it as KV, JSON,
events, graph, vectors, search, or any other product data capability.

## 17. Immutable Table Format

The immutable table format is provisional but important.

Current table evidence:

```text
header                 64 bytes
data_blocks            repeated framed blocks
sub_index_blocks       optional, for partitioned indexes
bloom_partitions       repeated framed blocks
filter_index_block     framed block
top_level_index        framed block
properties_block       framed block
footer                 56 bytes
```

Current table constants:

```text
table_header_magic       "STRAKV\0\0"
table_footer_magic       "STRAKEND"
table_format_version     7
minimum_read_version     4
header_size              64 bytes
footer_size              56 bytes
block_frame_overhead     12 bytes
```

Current table header evidence:

```text
magic                  8 bytes
format_version         u16 LE
reserved_a             6 bytes
commit_min             u64 LE
commit_max             u64 LE
entry_count            u64 LE
data_block_size        u32 LE
reserved_b             20 bytes
```

Current table footer evidence:

```text
index_block_offset     u64 LE
index_block_len        u32 LE
filter_block_offset    u64 LE
filter_block_len       u32 LE
props_block_offset     u64 LE
props_block_len        u32 LE
index_type             u8
reserved               11 bytes
footer_magic           8 bytes
```

Current framed block evidence:

```text
block_type             u8
codec                  u8
reserved               u16 LE
data_len               u32 LE
data                   data_len bytes
crc32                  u32 LE
```

Current block codec values:

```text
0                      uncompressed
1                      zstd compressed
```

Current block types:

```text
1                      data
2                      index
3                      filter
4                      properties
5                      filter index
6                      sub-index
```

Current entry encoding uses prefix-compressed internal keys and stores value
kind, timestamp, TTL, value length, and value bytes.

Draft V1 requirements:

1. The stable V1 immutable table format version is `1`.
2. Immutable tables MUST be self-identifying.
3. Immutable tables MUST contain enough metadata for fast rejection by commit
   range where applicable.
4. Blocks MUST be length-delimited and integrity-checked.
5. Table readers MUST reject invalid magic, future format versions, invalid
   block frames, checksum mismatch, and impossible offsets.
6. Table entry encoding MUST preserve internal-key ordering.
7. Required table readers MUST support uncompressed blocks and zstd-compressed
   blocks. Writers may choose compression per storage mode and table level.
8. The current table v7 implementation is evidence, not the public stable V1
   version number.

## 18. Watermark And Sidecar Formats

Current code has a snapshot watermark byte format:

```text
has_data               u8, 0 means empty, 1 means present
snapshot_id            u64 LE, present when has_data = 1
watermark_txn          u64 LE, present when has_data = 1
updated_at_micros      u64 LE, present when has_data = 1
```

Current code also has WAL segment metadata sidecars:

```text
magic                  4 bytes   "STAM"
version                u32 LE
segment_number         u64 LE
min_timestamp          u64 LE
max_timestamp          u64 LE
min_txn_id             u64 LE
max_txn_id             u64 LE
record_count           u64 LE
crc32                  u32 LE
```

Current segment metadata constants:

```text
SEGMENT_META_MAGIC       "STAM"
SEGMENT_META_VERSION     1
SEGMENT_META_SIZE        60 bytes
```

Draft V1 requirements:

1. Optional sidecars MUST be explicitly marked optional by the spec.
2. Missing optional sidecars MAY be regenerated.
3. Corrupt optional sidecars MAY be ignored only if their owning service can
   rebuild them from authoritative objects.
4. Authoritative metadata MUST NOT be hidden in optional sidecars.

First-pass decision: standalone watermark bytes and segment metadata sidecars
are not stable V1 authoritative objects by default. Add sidecars only when the
implementation proves they are needed for performance or diagnostics, and keep
them rebuildable from authoritative manifest, WAL, snapshot, and table objects.

## 19. Checksums And Integrity

V1 durable formats MUST define integrity protection per object family.

Current evidence uses:

- CRC32 for manifest bytes
- CRC32 for WAL segment headers
- CRC32 for WAL record length fields
- CRC32 for WAL record payloads
- CRC32 for snapshot container footer
- CRC32 for table block frames
- CRC32 for segment metadata sidecars
- AES-GCM authentication tags for encrypted codec payloads

Draft V1 requirements:

1. A checksum mismatch MUST fail decode.
2. An authenticated encryption failure MUST fail decode.
3. Integrity failures MUST happen before decoded data is trusted.
4. Recovery policy may treat optional sidecars differently from authoritative
   objects, but the format decoder itself must report corruption precisely.

## 20. Strict Decode And Failure Semantics

Conforming decoders MUST report typed failures for:

- insufficient data
- invalid magic
- pre-v1 development format
- future format
- unsupported version
- checksum mismatch
- codec mismatch
- codec decode failure
- invalid length
- invalid UTF-8 where UTF-8 is required
- invalid storage-owned tag
- trailing data
- decompression failure
- deserialization failure

Decoders MUST NOT:

- panic on malformed input
- trust lengths before validating enough envelope data
- allocate unbounded memory from attacker-controlled counts
- silently ignore trailing bytes unless the format explicitly defines an
  extension area
- reinterpret product semantics while parsing storage bytes

## 21. Golden Vectors

The stable V1 spec must include golden vectors.

Required golden vector categories:

- manifest with identity codec
- WAL segment header
- WAL record with empty commit payload
- WAL record with non-empty commit payload
- snapshot header with identity codec
- snapshot section envelope with empty payload
- internal key with ordinary bytes
- internal key with zero bytes in user key
- storage row put
- storage row tombstone
- table header/footer
- table framed block
- segment metadata sidecar, if retained

Golden vectors must include:

1. Human-readable field values.
2. Hex bytes.
3. Expected decode result.
4. Expected checksum values.
5. Negative mutations that must fail decode.

Golden vector generation must be explicit. Tests must not silently rewrite
golden vectors.

## 22. Conformance

An implementation claiming `strata-storage-format-v1` support must:

1. Pass all golden vector tests.
2. Pass all strict decode negative tests.
3. Pass fuzz no-panic tests for public decoders.
4. Reject unsupported future format versions.
5. Reject pre-v1 development formats unless an explicit developer conversion
   tool is being run.
6. Enforce codec mismatch before replay or mutation.
7. Preserve internal-key ordering.
8. Preserve storage row metadata through WAL, snapshot, and table paths.

## 23. Stabilization Decisions

These decisions close the first-pass stabilization questions:

1. The V1 object namespace has no `v1/` prefix by default.
2. Stable V1 manifest format starts at version 1.
3. Stable V1 WAL segment format starts at version 1.
4. The codec-aware outer envelope is WAL segment framing.
5. Stable V1 commit payloads use storage-native binary encoding, not
   MessagePack.
6. Primitive snapshot DTOs are current-code evidence only, not a V1 migration
   format.
7. Expiry metadata is mandatory in every storage row; zero means no expiry.
8. Stable V1 table format starts at version 1.
9. Table readers support uncompressed and zstd-compressed blocks.
10. AES-GCM is deferred from required stable V1 until encryption
    productization is designed.
11. Pre-v1 development databases are rejected by default. Migration, if ever
    needed before launch, is explicit developer tooling outside normal open.
12. Durable core encodings stable enough for storage bytes are:
    `BranchId = 16 raw UUID bytes`, `CommitVersion = u64 LE`,
    `TxnId = u64 LE`, and `Timestamp = u64 LE microseconds since Unix epoch`.
    `EntityRef`, `PrimitiveType`, `Versioned`, and product DTOs are not stable
    storage-format types.

## 24. Drafting Plan

This document should be updated as the storage-next layer documents are
completed:

1. L3 defines durable bytes and codec boundaries.
2. L4 finalizes manifest, WAL, snapshot publication, and recovery object
   service formats.
3. L5 finalizes immutable table bytes.
4. L6 finalizes row and internal-key encoding.
5. L7 finalizes commit payload encoding.
6. L8 finalizes lifecycle, recovery, sidecar, quarantine, and retention
   format requirements.
7. L9 finalizes the engine-facing storage API contract.

The stable spec should be cut only after these layers agree on the same
storage row model, object namespace, and format-version policy.
