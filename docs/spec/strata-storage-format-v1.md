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

Canonical V1 layout:

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

The V1 object namespace uses ASCII-only names. Segment and snapshot IDs are
encoded as 16-character fixed-width lowercase hex. Table levels are encoded as
`l` plus four fixed-width decimal digits, such as `l0000`, and must be in the
range `0..=9999`. Branch and table IDs must use validated database-relative
object-name components; the final durable ID source is owned by the
storage/branch implementation work that allocates those IDs.

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

V1 manifest format:

```text
magic                         4 bytes   "STRM"
format_version                u32 LE, MUST be 1
database_id                   16 bytes
codec_id_len                  u32 LE
codec_id                      codec_id_len bytes, UTF-8
active_wal_segment            u64 LE, MUST be nonzero
snapshot_watermark            u64 LE, 0 means none
snapshot_id                   u64 LE, 0 means none
flushed_through_commit_id     u64 LE, 0 means none
crc32                         u32 LE over all preceding bytes
```

V1 manifest constants:

```text
MANIFEST_MAGIC                  "STRM"
MANIFEST_FORMAT_VERSION         1
MAX_CODEC_ID_LEN                256 bytes
```

Requirements:

1. The stable V1 manifest format version is `1`.
2. The manifest MUST identify the database.
3. The manifest MUST record the configured codec identity.
4. The manifest MUST record enough WAL and snapshot facts to run recovery.
5. The manifest MUST be protected by a checksum or authenticated integrity
   mechanism.
6. The manifest decoder MUST reject invalid magic, pre-v1 development formats,
   future formats, invalid codec strings, insufficient data, and checksum
   mismatch.
7. `active_wal_segment` MUST be nonzero. WAL segment ids are one-based in
   durable manifests.
8. `snapshot_watermark` and `snapshot_id` MUST either both be zero or both be
   nonzero. A manifest with only one snapshot recovery field present is invalid.
9. The manifest database identity is a storage-local physical database
   identity and recovery fact. It is not a StrataHub fleet, instance, dataset,
   or bundle identity. StrataHub must compose its own identifiers and
   provenance above the storage format.
10. Pre-V1 development manifest version `2` is rejected by the normal V1
   decoder. Strata is pre-launch; old development databases are not a stable
   migration target.

## 9. WAL Segment Format

The WAL segment format stores committed records.

V1 WAL segment header format:

```text
segment_magic          4 bytes   "STRA"
format_version         u32 LE, MUST be 1
segment_number         u64 LE
database_id            16 bytes
header_crc             u32 LE over first 32 bytes
```

V1 WAL segment constants:

```text
SEGMENT_MAGIC                    "STRA"
SEGMENT_FORMAT_VERSION           1
SEGMENT_HEADER_SIZE              36 bytes
SEGMENT_BASE_HEADER_SIZE         32 bytes
```

Pre-launch development version history from the current implementation:

1. v1: original 32-byte header, no CRC.
2. v2: 36-byte header with CRC32 over the first 32 bytes.
3. v3: per-record outer envelope for codec-aware reads.

Requirements:

1. The stable V1 WAL segment format version is `1`.
2. Each WAL segment MUST have a self-identifying header.
3. The header MUST bind the segment to a database identity.
4. The header MUST record the segment number.
5. The header MUST have an integrity check.
6. Segment number mismatch between object name and header MUST be rejected
   when the object name provides an expected segment number.
7. Segment header versions `0`, `2`, and `3` are pre-V1 development formats
   rejected by the normal V1 decoder. Versions greater than `1` other than
   known pre-V1 development versions are future formats.
8. Pre-V1 development segment versions MUST produce a typed unsupported-format
   failure.
9. The V1 header decoder returns the exact bytes consumed so L4 can parse
   repeated record envelopes after the header. Trailing segment bytes are not
   header-level trailing data.
10. The byte header is stable in V1. Whether L4 implements segments as append
    logs or object-published immutable chunks remains a service-level decision.

## 10. WAL Record Format

V1 WAL records are two-layer frames:

1. Outer codec-aware envelope: stored in WAL segments.
2. Inner logical record: decoded from the outer envelope payload after the WAL
   service applies the configured storage codec.

V1 outer WAL record envelope:

```text
encoded_record_len     u32 LE
encoded_len_crc32      u32 LE, CRC32 over encoded_record_len bytes
encoded_record         encoded_record_len bytes
```

V1 inner WAL record format:

```text
record_len             u32 LE, number of bytes after this field
format_version         u8, MUST be 1
record_len_crc32       u32 LE, CRC32 over record_len bytes
commit_version         u64 LE
branch_id              16 bytes
timestamp_micros       u64 LE
commit_payload         V1 WAL commit payload bytes
payload_crc32          u32 LE
```

`payload_crc32` covers:

```text
format_version || record_len_crc32 || commit_version || branch_id ||
timestamp_micros || commit_payload
```

V1 WAL record constants:

```text
WAL_RECORD_FORMAT_VERSION          1
WAL_RECORD_MIN_LEN_AFTER_PREFIX    116 bytes
WAL_RECORD_ENVELOPE_HEADER_SIZE    8 bytes
```

Requirements:

1. WAL records MUST be self-delimiting.
2. WAL records MUST detect torn writes to the length field.
3. WAL records MUST detect payload corruption.
4. WAL records MUST carry commit version, branch identity, commit timestamp,
   and a row-native WAL commit payload.
5. WAL record decode MUST return the exact byte count consumed.
6. The outer envelope length MUST be nonzero and protected by CRC before the
   WAL service trusts the encoded payload length.
7. The inner record decoder MUST verify `record_len_crc32` before trusting
   `record_len`.
8. The inner record decoder MUST verify `payload_crc32` before decoding the
   commit payload.
9. Inner record version `0` and pre-launch development version `2` are pre-V1
   formats rejected by the normal V1 decoder. Versions greater than `1` other
   than known pre-V1 development versions are future formats.
10. The codec-aware outer envelope is WAL segment framing. The logical WAL
    record begins after the segment frame payload has been decoded.
11. Every decoded payload row MUST carry the same commit version, branch id,
    and commit timestamp as the outer WAL record.

## 11. Commit Payload Format

V1 WAL commit payloads are storage-row-native batches. They are not legacy
primitive writesets, transaction MessagePack, JSON, or engine operation bytes.

```text
magic                  4 bytes, ASCII "STCP"
format_version         u32 LE, MUST be 1
row_count              u32 LE, MUST be nonzero
rows                   repeated row_count times:
  row_len              u32 LE
  storage_row          row_len bytes, storage row format V1
```

V1 commit payload constants:

```text
WAL_COMMIT_PAYLOAD_MAGIC            "STCP"
WAL_COMMIT_PAYLOAD_FORMAT_VERSION   1
MAX_WAL_COMMIT_PAYLOAD_ROWS         4096
MAX_WAL_COMMIT_PAYLOAD_BYTES        64 MiB
MAX_WAL_COMMIT_PAYLOAD_ROW_BYTES    16 MiB
```

The nested `storage_row` bytes use the storage-row format from section 4.

Requirements:

1. Commit payloads MUST be storage-mechanical, not engine-primitive-shaped.
2. Commit payloads MUST be deterministic for the same mutation sequence.
3. Commit payloads MUST preserve all data required for WAL replay.
4. Commit payloads MUST support tombstones.
5. Commit payloads MUST support retained row metadata needed by history,
   `getv`, and timestamp-bounded `as_of`.
6. Commit payloads MUST be easy to fuzz and specify without relying on a
   serde data model.
7. Row count MUST be validated before allocating the decoded row vector.
8. Row length MUST be nonzero and within bounds before slicing row bytes.
9. Trailing bytes after the declared rows MUST be rejected.
10. Version `0` is a pre-V1 format and versions greater than `1` are future
    formats.
11. A successful WAL record decode MUST reject payload rows whose commit
    version, branch id, or commit timestamp differs from the outer WAL record.

## 12. Snapshot Container Format

The snapshot container stores checkpoint data at a recovery watermark.

V1 snapshot byte layout:

```text
snapshot_magic         4 bytes   "SNAP"
format_version         u32 LE
snapshot_id            u64 LE
watermark_commit_version u64 LE
created_at_micros      u64 LE
database_id            16 bytes
codec_id_len           u8
reserved               15 bytes
codec_id               codec_id_len bytes, UTF-8
sections               repeated snapshot sections
footer_crc32           u32 LE
```

V1 snapshot constants:

```text
SNAPSHOT_MAGIC                    "SNAP"
SNAPSHOT_FORMAT_VERSION           1
SNAPSHOT_HEADER_SIZE              64 bytes
SNAPSHOT_FOOTER_SIZE              4 bytes
```

V1 requirements:

1. The stable V1 snapshot container format version is `1`.
2. Snapshot id MUST be nonzero.
3. A snapshot MUST identify the database.
4. A snapshot MUST identify the recovery watermark it covers.
5. The recovery watermark is a storage commit version encoded as `u64 LE`.
6. A snapshot MUST record or validate the database codec identity.
7. A snapshot MUST consist of zero or more length-delimited sections.
8. `codec_id_len` MUST be in `1..=255`; the codec id bytes MUST be valid UTF-8
   and MUST NOT contain NUL.
9. Reserved header bytes MUST be zero.
10. `footer_crc32` is CRC32 over every byte before the footer, including header,
    codec id, and all section envelope/payload bytes.
11. Snapshot decode MUST fail before install if the header, footer checksum, or
    any section envelope is corrupt.
12. Pre-V1 development snapshot format version `2` is rejected by the V1
    decoder rather than migrated.
13. Implementations MUST validate section lengths before allocating payload
    buffers. A materializing decoder MAY enforce implementation-defined
    section-count and total-payload limits; large snapshot install paths SHOULD
    use borrowed or streaming section iteration.

## 13. Snapshot Section Format

V1 section envelope layout:

```text
section_kind           u8
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

V1 direction:

1. Storage owns the section envelope.
2. `section_kind = 0x00` is invalid and reserved.
3. L3 validates only mechanical envelope shape. It does not map section kinds
   to KV, JSON, event, vector, graph, search, or any product primitive.
4. Committed storage state uses row-native storage snapshot sections.
5. Engine owns opaque derived-state section payload semantics if such sections
   remain.
6. Unknown storage-owned section types MUST be rejected by the owning snapshot
   install service unless the section is explicitly marked skippable by that
   service contract.
7. Opaque engine-owned sections MAY exist only if their ownership and
   install path are explicit.
8. Opaque engine-owned sections MUST NOT be required to recover committed
   storage rows.
9. Section decoders MUST reject length fields that cannot fit the host address
   space or would overflow envelope-size arithmetic before allocating or
   copying payload bytes.

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

Storage row payloads use format version `1`.

V1 row bytes:

```text
format_version         u8, currently 1
physical_key_len       u32 LE
physical_key           physical_key_len bytes
commit_version         u64 LE
timestamp_micros       u64 LE
expires_at_micros      u64 LE, 0 means no expiry
row_flags              u32 LE, must be 0 in V1
tombstone              u8, 0=false, 1=true
value_len              u32 LE
value                  value_len bytes
```

V1 row requirements:

1. Rows MUST be generic.
2. Rows MUST support deletion tombstones.
3. Rows MUST preserve enough metadata for latest reads, version-bounded reads,
   history reads, and timestamp-bounded reads.
4. Rows MUST be encodable in WAL payloads, snapshots, and immutable tables
   without changing product meaning.
5. Rows MUST carry expiry metadata. A zero expiry means no expiry.
6. Decoders MUST reject unsupported row versions, nonzero row flags, invalid
   tombstone bytes, tombstones with nonzero expiry, tombstones with non-empty
   values, invalid nested physical keys, insufficient bytes, and trailing data.

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

The `descending_commit` suffix intentionally uses big-endian order so natural
byte ordering sorts newer versions first for a physical key. Other standalone
core atom encodings remain little-endian unless a format section explicitly
states otherwise.

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

The immutable table format stores L5-built table bytes as a self-identifying L3
artifact. V1 table bytes are storage-row-native; old development table files are
historical evidence only and are not valid V1 input.

Stable V1 table object envelope:

```text
table_header           64 bytes
data_block_frames      repeated, at least one
filter_block_frame     optional, absent until a V1 filter subformat is assigned
index_block_frame      one, required
properties_block_frame one, required
table_footer           64 bytes
```

Stable V1 table constants:

```text
table_header_magic       "STTB"
table_footer_magic       "STTF"
table_format_version     1
header_size              64 bytes
footer_size              64 bytes
block_frame_overhead     16 bytes
```

Stable V1 table header:

```text
table_magic            4 bytes   "STTB"
format_version         u32 LE, MUST be 1
header_size            u32 LE, MUST be 64
header_flags           u32 LE, MUST be 0
target_data_block_size u32 LE, MUST be nonzero
data_block_count       u32 LE, MUST be nonzero
row_count              u64 LE, MUST be nonzero
commit_min             u64 LE
commit_max             u64 LE
reserved               16 bytes, MUST be zero
```

Stable V1 table footer:

```text
index_block_offset     u64 LE
index_block_frame_len  u32 LE
filter_block_offset    u64 LE, MUST be 0 until filter subformat assignment
filter_block_frame_len u32 LE, MUST be 0 until filter subformat assignment
props_block_offset     u64 LE
props_block_frame_len  u32 LE
footer_magic           4 bytes   "STTF"
reserved               20 bytes, MUST be zero
table_crc32            u32 LE
```

The table CRC32 covers every byte in the table object before `table_crc32`.
Readers MUST validate this checksum before trusting footer offsets.

Stable V1 table block frame:

```text
block_type             u8
compression_codec      u8
block_flags            u16 LE, MUST be 0
encoded_len            u32 LE
decoded_len            u32 LE
encoded_payload        encoded_len bytes
crc32                  u32 LE
```

The block CRC32 covers every byte in the block frame before the `crc32` field.

Stable V1 table block codec values:

```text
0                      uncompressed
1                      zstd compressed
```

Stable V1 table block types:

```text
1                      data
2                      index
3                      filter, reserved until assigned by a later V1 subformat
4                      properties
```

Readers MUST reject unknown block types, unknown compression codecs, nonzero
block flags, impossible offsets, checksum mismatch, future format versions, and
pre-V1 development bytes. The old `STRAKV`/version-7 table format and its
prefix-compressed entry encoding are not V1 compatibility formats.

Stable V1 data block payload:

```text
entry_count            u32 LE, MUST be nonzero

entries repeated entry_count times:
  internal_key_len     u32 LE, MUST be nonzero
  internal_key_bytes   V1 InternalKey bytes
  row_len              u32 LE, MUST be nonzero
  row_bytes            V1 StorageRow bytes
```

Data block payload decoders MUST reject zero or oversized entry counts, zero or
oversized key and row lengths, invalid nested internal-key bytes, invalid
nested storage-row bytes, duplicate internal keys, entries not strictly sorted
by encoded internal-key bytes, trailing payload bytes, and any mismatch between
the internal key's physical key or commit version and the nested row facts.
Duplicate physical keys at different commit versions are valid when their
encoded internal-key bytes remain strictly ordered.

Stable V1 monolithic index block payload:

```text
index_format_version   u32 LE, MUST be 1
entry_count            u32 LE, MUST be nonzero

entries repeated entry_count times:
  first_key_len        u32 LE, MUST be nonzero
  first_key_bytes      V1 InternalKey bytes
  last_key_len         u32 LE, MUST be nonzero
  last_key_bytes       V1 InternalKey bytes
  block_offset         u64 LE, absolute table offset of data block frame
  block_frame_len      u32 LE, full encoded data-block frame length
  row_count            u32 LE, rows in that data block, MUST be nonzero
```

Index block payload decoders MUST reject pre-V1 version `0`, future versions,
zero or oversized entry counts, zero or oversized key lengths, invalid nested
key bytes, `first_key_bytes > last_key_bytes`, unsorted index entries,
overlapping adjacent key ranges, zero frame lengths, zero row counts, and
trailing payload bytes. Whole-table validation additionally MUST verify the
entry count against the table header's data-block count and verify each
referenced frame, first key, last key, and row count against the decoded data
block bytes.

Stable V1 properties block payload:

```text
properties_format_version u32 LE, MUST be 1
row_count                 u64 LE, MUST be nonzero
data_block_count          u32 LE, MUST be nonzero
commit_min                u64 LE
commit_max                u64 LE
min_key_len               u32 LE, MUST be nonzero
min_key_bytes             V1 InternalKey bytes
max_key_len               u32 LE, MUST be nonzero
max_key_bytes             V1 InternalKey bytes
```

Properties block payload decoders MUST reject pre-V1 version `0`, future
versions, zero or oversized row and data-block counts, `commit_min >
commit_max`, zero or oversized key lengths, invalid nested key bytes,
`min_key_bytes > max_key_bytes`, and trailing payload bytes. Whole-table
validation additionally MUST verify these properties against the table header
and decoded data blocks.

Whole-table artifact decoders MUST validate the object as one contiguous table:
the data-block frames must fill the byte range between the 64-byte header and
the footer-referenced index frame exactly, the index frame must fill the
footer-referenced index range exactly, the properties frame must fill the
footer-referenced properties range exactly, and the properties frame must end
immediately before the footer. Hidden bytes between data blocks, index,
properties, or footer are invalid. The decoded index entries MUST match the
actual data-block frame offsets, frame lengths, first keys, last keys, and row
counts.

V1 table requirements:

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
8. Stable table data entries MUST be based on `StorageRow` bytes, not bincode
   product values or engine primitive payloads.

## 18. Watermark, Sidecar, And Quarantine Inventory Formats

V1 snapshot watermark byte format:

```text
has_data               u8, 0 means empty, 1 means present
snapshot_id            u64 LE, present when has_data = 1
watermark_commit_version u64 LE, present when has_data = 1
updated_at_micros      u64 LE, present when has_data = 1
```

The empty watermark is exactly one byte: `00`. The present watermark is exactly
25 bytes. In a present watermark, `snapshot_id` MUST be nonzero. The decoder
MUST reject any other `has_data` byte, zero snapshot id, truncation, and
trailing data.

V1 WAL segment metadata sidecar format:

```text
magic                  4 bytes   "STAM"
version                u32 LE, MUST be 1
segment_number         u64 LE
min_timestamp          u64 LE
max_timestamp          u64 LE
min_commit_version     u64 LE
max_commit_version     u64 LE
record_count           u64 LE
crc32                  u32 LE over all preceding bytes
```

V1 segment metadata constants:

```text
SEGMENT_META_MAGIC       "STAM"
SEGMENT_META_VERSION     1
SEGMENT_META_SIZE        60 bytes
```

Requirements:

1. Segment metadata sidecars are optional accelerators. Missing sidecars do not
   make the authoritative WAL segment invalid.
2. If a segment metadata sidecar is present, it MUST be exactly 60 bytes and
   pass magic, version, and CRC checks.
3. Segment metadata version `0` is pre-V1. Versions greater than `1` are future
   formats.
4. Segment metadata decode MUST reject trailing bytes. The format has no
   extension area in V1.

5. Optional sidecars MUST be explicitly marked optional by the spec.
6. Missing optional sidecars MAY be regenerated.
7. Corrupt optional sidecars MAY be ignored only if their owning service can
   rebuild them from authoritative objects.
8. Authoritative metadata MUST NOT be hidden in optional sidecars.

First-pass decision: snapshot watermark bytes are stable when an owning service
persists them. Segment metadata sidecars are stable optional accelerators; add
them only when the implementation proves they are needed for performance or
diagnostics, and keep them rebuildable from authoritative manifest, WAL,
snapshot, and table objects.

V1 quarantine inventory byte format:

```text
magic                  4 bytes   "STQI"
version                u32 LE, MUST be 1
database_id            16 bytes
branch_id              16 raw BranchId bytes
codec_id_len           u32 LE
codec_id               codec_id_len UTF-8 bytes
entry_count            u32 LE

entries repeated entry_count times:
  object_id_len        u32 LE
  object_id            object_id_len UTF-8 bytes
  source_object_len    u32 LE
  source_object        source_object_len UTF-8 object-name bytes
  byte_count           u64 LE
  quarantined_at_micros u64 LE

crc32                  u32 LE over all preceding bytes
```

V1 quarantine inventory constants:

```text
QUARANTINE_INVENTORY_MAGIC    "STQI"
QUARANTINE_INVENTORY_VERSION  1
```

Requirements:

1. Quarantine inventory is authoritative for storage-owned quarantine state.
2. Inventory entries are canonicalized by `object_id`, then `source_object`.
3. `object_id` is a branch-local quarantine object id used in
   `quarantine/<branch-id>/<object-id>`.
4. `object_id` MUST be a valid object-name component and MUST NOT be
   `manifest`, which is reserved for `quarantine/<branch-id>/manifest`.
5. `source_object` MUST be a valid database-relative `ObjectName`.
6. `source_object` MUST NOT be in the `quarantine/` family.
7. `source_object` MUST map to a known non-quarantine object family.
8. Source family is derived from `source_object`. It is not stored as a
   redundant durable field.
9. Duplicate `object_id` values are invalid.
10. Duplicate `source_object` values are invalid.
11. Decoders MUST reject invalid magic, pre-V1 version `0`, future versions,
    checksum mismatch, invalid UTF-8, invalid object-name values, noncanonical
    entry ordering, insufficient bytes, and trailing data.
12. Decoders MUST reject entry counts that cannot fit in the remaining bytes
    before allocating an entry vector.
13. The old storage crate's `STRAQRTN` quarantine manifest is pre-V1 evidence
    only. V1 decoders MUST NOT accept it as a compatibility format.

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
- CRC32 for quarantine inventory bytes
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
- snapshot watermark, empty and present
- WAL segment header
- WAL record outer envelope
- WAL record with empty commit payload, historical malformed fixture only
- WAL record with row-native commit payload
- WAL commit payload with one put row
- WAL commit payload with put plus tombstone row
- snapshot header with identity codec
- snapshot section envelope with empty payload
- snapshot container with one section and footer CRC
- internal key with ordinary bytes
- internal key with zero bytes in user key
- storage row put
- storage row tombstone
- table data block frame with one put row
- table data block frame with put plus tombstone rows
- table data block frame with zstd compression
- table monolithic index block payload
- table properties block payload
- complete one-block immutable table
- complete two-block immutable table
- segment metadata sidecar, if retained
- quarantine inventory, empty
- quarantine inventory, multiple entries

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
    and `Timestamp = u64 LE microseconds since Unix epoch`. V1 storage does
    not define a durable transaction-id atom. `EntityRef`, `PrimitiveType`,
    `Versioned`, and product DTOs are not stable storage-format types.

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
