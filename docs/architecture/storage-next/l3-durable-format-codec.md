# L3. Durable Format / Codec

Status: V1 architecture draft

Draft public spec:
[../../spec/strata-storage-format-v1.md](../../spec/strata-storage-format-v1.md)

## Purpose

L3 owns durable bytes.

L1 moves objects. L2 names objects. L3 defines how storage data is encoded
inside those objects: headers, records, frames, row bytes, checksums, format
versions, and codec boundaries.

This layer exists so durable bytes are explicit, testable, fuzzable, and
separate from runtime policy. Storage-next should not scatter byte formats
across WAL code, table code, checkpoint code, and recovery code the way the
current crate does.

## Core Decision

Storage-next should have a small, deliberate durable-format layer.

L3 should own the byte-level formats for storage mechanics. It should not own
database lifecycle, recovery policy, compaction policy, checkpoint policy,
branch product semantics, or engine data capability semantics.

The first storage rewrite must not hide physical-format changes. Current
formats are evidence, not compatibility constraints. Strata is pre-launch, so
storage-next should define the clean V1 stable format rather than preserve
development-era version numbers or compatibility branches.

## Format Revision Policy

Storage-next is allowed to introduce a V1 storage-row-native format where the
architecture requires it. In particular, a commit payload encoded as storage
rows instead of `EntityRef` plus primitive tags is a deliberate format revision,
not a hidden refactor.

Format changes must be specified before implementation:

1. Assign stable V1 format versions starting at version 1 for each object
   family.
2. Reject pre-v1 development formats during normal open.
3. Add golden vectors for every stable object family.
4. Keep current durability rules explicit: WAL before visible, manifest
   publish, checkpoint watermarks, and recovery ordering.
5. Do not claim backward compatibility by accident.

## Responsibilities

L3 owns:

- format identifiers, magic bytes, and version checks
- WAL segment and WAL record byte framing
- commit payload byte encoding
- manifest byte encoding
- snapshot/checkpoint container byte encoding
- snapshot/checkpoint section envelope encoding
- immutable table file byte encoding
- table block framing
- table entry encoding
- storage row value encoding
- segment/table metadata encoding
- watermark byte encoding, if retained as a durable object
- checksums and authenticated-integrity boundaries
- compression frame hooks
- encryption codec hooks
- strict decode behavior
- golden vectors, corruption tests, and fuzz targets for durable bytes

L3 does not own:

- object names or paths
- backend IO
- append, publish, rename, fsync, or object-store commit protocols
- WAL rotation policy
- manifest update policy
- recovery ordering
- checkpoint scheduling
- snapshot retention policy
- table compaction scheduling
- MVCC visibility rules
- public transaction sessions
- engine primitive types
- JSON, event, vector, graph, search, or inference semantics

## Layer Boundary

L3 is below durable services and table runtime.

```text
L5/L6/L7/L8 ask for encoded bytes or decoded records
        |
        v
L3 durable format and codec
        |
        v
L2 object names + L1 backend IO
```

L3 can say "these bytes are not a valid WAL record" or "this table block has a
bad checksum." It must not decide whether to quarantine the object, replay a
different segment, rebuild an index, compact, or surface a product error. Those
decisions belong above L3.

## Current Code Evidence

The current format surface is spread across several modules:

- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/durability/format/snapshot.rs`
- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/durability/format/watermark.rs`
- `crates/storage/src/durability/format/writeset.rs`
- `crates/storage/src/durability/format/primitives.rs`
- `crates/storage/src/durability/format/primitive_tags.rs`
- `crates/storage/src/durability/payload.rs`
- `crates/storage/src/durability/codec/`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segment.rs`

This is the problem L3 should fix. Some bytes live under `durability/format`.
Some live in table runtime files. Some commit payload bytes live next to
transaction code. Some primitive-shaped snapshot bytes live in storage even
though primitive meaning belongs above storage.

## Current Durable Formats

### WAL Segment And Record Format

Current WAL segments are named by L2-like path helpers today, but the bytes are
L3 material.

Current facts:

- segment magic: `STRA`
- current segment format version: `3`
- minimum supported segment format version: `3`
- segment header size: 36 bytes for v2/v3 headers
- current WAL record format version: `2`
- v3 adds a per-record outer envelope around codec-encoded inner records
- WAL record payload carries transaction id, branch id, timestamp, and commit
  payload bytes
- CRCs are used for segment header and record integrity

Storage-next should preserve the idea of explicit segment headers and framed
records. Whether the exact bytes remain identical is an implementation
decision, but any change must be a deliberate format-version decision.

### Manifest Format

Current manifest bytes are storage-physical metadata:

- magic: `STRM`
- current manifest format version: `2`
- minimum supported manifest format version: `2`
- database UUID
- codec ID
- active WAL segment
- snapshot watermark
- snapshot ID
- flushed-through commit ID
- CRC32 over preceding bytes

This is correctly storage-owned in spirit. L3 owns encoding and strict decode.
L4 owns manifest load/update/publish mechanics. L8 owns how manifest facts are
used during lifecycle and recovery.

### Snapshot Container Format

Current snapshot files have:

- magic: `SNAP`
- current snapshot format version: `2`
- minimum supported snapshot format version: `2`
- 64-byte snapshot header
- snapshot id
- watermark transaction
- created-at timestamp
- database UUID
- codec ID length and codec ID string
- section headers
- footer CRC

Storage-next should keep the distinction between a storage-owned snapshot
container and the payload meaning inside sections.

L3 may own:

- snapshot header bytes
- section envelope bytes
- section length validation
- footer/integrity validation
- codec-id validation

L3 must not infer product meaning from section types.

### Primitive Snapshot Sections

Current storage contains primitive snapshot DTOs for KV, event, branch, JSON,
vector, and graph, plus primitive tag bytes:

```text
KV     = 0x01
Event  = 0x02
Branch = 0x03
JSON   = 0x04
Vector = 0x05
Graph  = 0x06
```

These are current primitive snapshot-section tags, not the V1
`storage_space_id` registry. V1 storage-space IDs are assigned separately in
`docs/architecture/storage-next/storage-space-id-registry.md`.

These are current-format evidence, not target storage ownership.

Storage-next should not conclude that storage owns JSON documents, event
chains, vector collections, graph records, or branch product behavior because
old snapshot sections were shaped that way.

V1 target direction:

1. Storage owns generic snapshot containers and generic storage-row payload
   encoding.
2. Committed storage state snapshots are row-native. Section identifiers
   describe storage row groups, storage metadata, or opaque engine-supplied
   extension sections, not primitive product semantics.
3. Optional opaque engine-owned sections may exist for derived or rebuildable
   state, but they are not required to recover committed storage rows.
4. Engine owns any primitive or derived-state decode for opaque sections.

This decision is load-bearing for storage-next. Pulling primitive DTOs into the
new storage core would recreate the boundary problem we just removed.

### Commit Payload And Writeset Encoding

The current code has two related paths:

- `durability/format/writeset.rs` encodes mutations using `EntityRef` and
  primitive tags.
- `durability/payload.rs` encodes `TransactionPayload` as MessagePack over
  storage `Key` and `Value`.

Both are useful evidence. Neither is the ideal storage-next contract.

Storage-next should encode an internal commit unit as storage mechanics:

- branch id
- physical key bytes or storage-key components
- operation kind
- row value bytes
- commit version
- commit timestamp
- expiry timestamp, with zero meaning no expiry
- tombstone marker

The stable V1 commit payload should use a storage-native binary format rather
than MessagePack. MessagePack can remain an engine-owned value encoding where
appropriate, but storage mechanics should be deterministic, compact, and
directly fuzzable from the published spec.

The commit payload must not require storage to understand engine primitive
entities. Engine can map product operations to storage rows before calling the
storage commit boundary.

### Immutable Table Format

Today the immutable KV/table format lives mostly in `segment_builder.rs` and
`segment.rs`, not under `durability/format`.

Current facts:

- table header magic: `STRAKV\0\0`
- table footer magic: `STRAKEND`
- current table format version: `7`
- reader accepts versions `4..=7`
- fixed header size: 64 bytes
- fixed footer size: 56 bytes
- block frame overhead: 12 bytes
- block types include data, index, filter, properties, filter index, and
  sub-index
- block frames include type, codec byte, reserved bits, data length, data, and
  CRC32
- data entries use prefix-compressed internal keys
- values use bincode today
- data blocks may use zstd compression
- internal keys sort by physical key ascending and commit version descending

Storage-next should move table byte definitions into the L3 ownership model,
even if implementation keeps builder/reader code nearby for performance.

L5 should own table runtime behavior: building, reading, caching, seeking,
merging, and compaction mechanics. L6 owns branch-local flush state transitions
and table installation. L8 owns lifecycle scheduling. L3 should own the durable
bytes those operations produce and consume.

### Internal Key Encoding

Current internal keys are encoded as:

```text
TypedKeyBytes || EncodeDesc(commit_id)
```

The current typed-key layout is:

```text
branch_id        16 bytes
space            NUL-terminated
storage_space_id 1 byte
user_key         byte-stuffed, terminated by 0x00 0x00
!commit_id       8-byte big-endian bitwise-NOT
```

This is a strong design choice and should be preserved unless a later format
plan proves a better one. It gives Strata latest, version-bounded reads,
history, and scans without maintaining separate physical stores for each view.

The target architecture should express this as a storage key/row encoding
contract, not as a primitive API contract. The current `TypeTag` byte becomes
an opaque `storage_space_id` in storage-next. Engine assigns stable space ids
for its data capabilities and future extensions; storage may route and order by
the byte but must not know what product capability it represents.

The storage-owned and engine-owned byte ranges are defined in
`docs/architecture/storage-next/storage-space-id-registry.md`. L3 durable key
encoding must reject invalid sentinel IDs and preserve storage-owned system row
families such as the commit timeline.

## Codec Boundary

The codec layer transforms bytes at explicit durability boundaries.

Current codecs:

- `identity`
- `aes-gcm-256`

Current AES-GCM wire format:

```text
nonce (12 bytes) || ciphertext + tag
```

Current code resolves AES-GCM key material from `STRATA_ENCRYPTION_KEY`. That
is acceptable evidence for the old implementation, but storage-next should not
hide security configuration inside L3. The target design should receive
resolved codec configuration from open-time configuration. L3 can validate and
use key material; it should not silently read global process environment as the
core contract.

Stable V1 requires the identity codec. AES-GCM is deferred from required V1
until encryption configuration and key management are productized.

Codec rules:

1. Every durable database has one configured storage codec identity unless a
   later design explicitly supports per-object codecs.
2. The durable manifest or static database metadata records the codec identity.
3. Opening with a mismatched codec identity fails before replay or mutation.
4. Codecs transform payload bytes at documented boundaries only.
5. Compression and encryption are separate concerns even if they share frame
   plumbing.
6. Authenticated encryption failures are corruption/integrity failures, not
   ordinary decode misses.
7. Identity remains the default V1 codec.

Snapshots need special care. The current snapshot container records and
validates the configured codec id, but primitive section payloads use a
canonical section codec. Storage-next should make this explicit rather than
letting snapshot and WAL codec behavior drift.

## Format Versioning

Every durable format should have an explicit compatibility story.

L3 should require:

- magic bytes or another unambiguous format identifier
- current format version
- minimum readable format version when a stable format later supports old
  stable versions
- future-version rejection
- typed unsupported-format error when a pre-v1 development format or obsolete
  stable format is rejected
- strict length validation
- strict checksum or authenticated-integrity validation
- trailing-byte policy

Trailing bytes should be rejected unless the format has an explicit extension
section. "Ignore what is left" makes fuzzing weaker and allows broken writers
to become de facto supported.

V1 may choose clean breaks for pre-v1 formats. Strata is still pre-v1. That
does not mean format changes should be casual. It means format changes should
be explicit, documented, and easy to test.

## Checksums And Integrity

L3 owns byte-level integrity checks.

Expected integrity tools:

- CRC32 for accidental corruption and torn-write detection
- authenticated encryption tags when encryption is enabled
- object length validation
- magic/version validation
- checksum mismatch errors that identify the format and object role

L3 does not decide recovery actions. It reports that bytes failed validation.
L4/L8 decide whether to stop, replay, rebuild, quarantine, or ignore optional
sidecars.

Optional sidecars are allowed to have softer failure behavior, but the softness
belongs to the owning service. For example, current WAL segment metadata
sidecars can be missing or corrupt and regenerated from WAL segments. L3 only
defines how to parse or reject the sidecar bytes.

## Storage Row Encoding

Storage-next needs one generic row model.

The row model should support:

- physical key bytes
- commit version
- commit timestamp
- value bytes
- tombstone marker
- expiry timestamp, with zero meaning no expiry
- optional row flags reserved for storage mechanics

The row model should not contain:

- JSON document ids as a storage concept
- event sequences as a product concept
- vector collection config as a storage concept
- graph relationship semantics
- search index semantics
- inference or embedding semantics

Engine can encode those meanings into physical key bytes and value bytes.
Storage persists and orders rows.

## Exposed Upward

L3 should expose narrow byte-format APIs to L4-L8 and L5-L7.

Examples of acceptable upward surfaces:

- encode/decode WAL segment header
- encode/decode WAL record frame
- encode/decode manifest bytes
- encode/decode snapshot container header
- encode/decode snapshot section envelope
- encode/decode table header/footer/block frame
- encode/decode storage row
- encode/decode commit payload
- validate codec identity
- encode/decode through configured codec

These should be format operations, not services. They should not open files,
list objects, publish manifests, replay logs, compact tables, or install
snapshots.

## Required Downward

L3 should require almost nothing from lower layers.

It can depend on:

- byte buffers
- object names for diagnostics if passed in by callers
- core/storage identifier types that are stable enough to be durable
- codec configuration supplied at open

It should not depend on:

- local filesystem paths
- file handles
- object-store clients
- engine APIs
- IPC
- global process environment as the primary codec configuration mechanism

## Failure Model

L3 failures should be typed and precise.

Expected categories:

- insufficient bytes
- invalid magic
- pre-v1 development format
- future format
- unsupported version
- checksum mismatch
- codec mismatch
- codec decode failure
- invalid length
- invalid UTF-8 where a format requires UTF-8
- invalid tag where a tag belongs to the storage format
- trailing data
- decompression failure
- deserialization failure

L3 errors should avoid product language. For example, "invalid JSON document
snapshot" is engine language. "invalid snapshot section payload" is storage
format language.

## Testing

L3 is the easiest layer to test hard, so it should become the first place where
storage-next approaches reference-grade coverage.

Required test categories:

- golden encode vectors for every durable format
- roundtrip encode/decode tests
- strict decode tests for truncation at every boundary
- magic/version rejection tests
- pre-v1 development-version and future-version tests
- checksum mismatch tests
- trailing-byte rejection tests
- unknown tag tests for storage-owned tags
- oversized count/length tests to prevent allocation bombs
- codec mismatch tests
- authenticated encryption corruption tests
- compression frame corruption tests
- table block frame corruption tests
- internal-key ordering tests
- commit payload compatibility tests
- snapshot section envelope tests independent of primitive payloads
- fuzz targets for every public decoder

For fuzzing, every decoder should obey:

1. No panic.
2. No unbounded allocation based only on attacker-controlled length fields.
3. No successful decode with unconsumed bytes unless the format explicitly
   supports extension data.
4. No success after checksum/authentication failure.

Golden vectors should live with the format they validate. Regenerating them
should require an explicit command or test fixture update, not incidental test
execution.

## V1 Minimum

V1 storage-next needs:

- one documented database codec configuration path
- identity codec
- WAL segment and record byte format
- commit payload byte format based on storage rows, not engine primitives
- manifest byte format
- snapshot container and section envelope format
- storage row encoding
- internal key encoding
- immutable table header/footer/block/entry format
- table block compression frame support for uncompressed and zstd blocks
- checksum strategy for every durable object family
- strict decode errors
- golden vectors for all stable formats
- fuzz targets for all public decoders

V1 does not need:

- object-store-optimized physical formats
- migration or compatibility readers for pre-v1 development formats
- per-object codec selection
- user-facing encryption product polish
- primitive snapshot DTOs as storage-owned concepts

## Resolved First-Pass Format Decisions

These decisions should be reflected in the stable format spec:

1. Stable manifest, WAL segment, snapshot, and table formats start at version
   1 even where current code has higher development-era versions.
2. Stable V1 open rejects pre-v1 development databases by default.
3. Primitive snapshot DTOs are evidence only, not V1 storage-owned payloads or
   a required migration format.
4. Commit payloads use storage-native binary encoding.
5. Expiry metadata is carried in every storage row.
6. Table readers support uncompressed and zstd-compressed blocks.
7. Required stable V1 codec support is identity only; AES-GCM is deferred from
   required V1.
8. Durable identifier encodings are raw bytes or little-endian integers:
   `BranchId` as 16 UUID bytes, `CommitVersion` as `u64`, `TxnId` as `u64`,
   and timestamp as `u64` microseconds since Unix epoch.

## Next Layer Dependency

L4 should be designed on top of this layer.

L4 will decide how WAL objects, manifest objects, snapshot objects, table
objects, and temporary objects are published, synchronized, replayed, and
cleaned up. L4 should call L3 for bytes. It should not define new hidden byte
formats.
