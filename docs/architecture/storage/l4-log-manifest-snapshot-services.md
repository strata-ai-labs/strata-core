# L4. Log / Manifest / Snapshot Services

Status: V1 architecture draft

Depends on:

- [L1. Backend IO](./l1-backend-io.md)
- [L2. Object Layout](./l2-object-layout.md)
- [L3. Durable Format / Codec](./l3-durable-format-codec.md)

Related draft spec:
[../../spec/strata-storage-format-v1.md](../../spec/strata-storage-format-v1.md)

## Purpose

L4 turns object IO, object names, and durable byte formats into durable storage
services.

L1 moves bytes. L2 names objects. L3 defines bytes. L4 performs durable service
operations over those pieces:

- append or publish WAL data
- read WAL data
- publish manifests
- read manifests
- write snapshots
- read snapshots
- publish table reachability manifests
- prune or delete durable objects when a higher layer proves they are safe
- clean up temporary service artifacts

L4 should be the last layer that cares about durable service publication
outcomes. L1 owns the backend-specific publish primitive. L4 turns that
primitive into storage services such as MANIFEST publish, snapshot publish, WAL
chunk publish, table manifest publish, and quarantine inventory publish.
Higher layers should ask for durable service operations, not hand-roll local
filesystem sequences.

This boundary is especially important for checkpointing. L8 may decide when a
checkpoint is needed and which lower-layer state must be advanced, but L4 owns
the durable mechanics of publishing snapshot/checkpoint objects and manifest
watermarks. L5 table code and L6 branch-LSM code should never need to know
whether the backend uses POSIX rename, browser object writes, or future
conditional object-store publication.

## Core Decision

Storage-next needs a durable service layer, not scattered helper functions.

The current code repeats the same durable-publish pattern in several places:

```text
write temp
fsync temp
rename temp to final
fsync parent directory
```

That pattern appears in database MANIFEST writes, snapshot writes, branch/table
manifest writes, quarantine manifest writes, and table file construction.
Storage-next should make this a first-class L4 service over L1 backend
capabilities.

The local filesystem backend can still implement L1 publish with temp files,
fsync, rename, and directory fsync. Browser/cache mode can use non-durable
memory publication because it does not claim crash durability. A future
object-store backend must implement publish through conditional object
operations, generations, etags, or another proven fencing protocol because
S3-like stores do not provide POSIX rename or append.

## Responsibilities

L4 owns:

- WAL append or WAL chunk publish mechanics
- WAL segment/chunk listing
- WAL record streaming through L3 decoders/codecs
- WAL truncation or safe deletion mechanics
- WAL sidecar publication, if sidecars are retained
- database manifest load/create/update/publish service
- table/branch manifest load/update/publish service
- snapshot/checkpoint object write/read service
- snapshot temp cleanup
- snapshot retention deletion mechanics once L8 supplies retention policy
- table object durable publication once L5 supplies table bytes
- durable object publish primitive
- service-level durability barriers
- service-level optional sidecar policy
- service-level metrics and facts
- service-specific fault injection hooks

L4 does not own:

- backend IO implementation
- object name construction
- byte format layout
- primitive snapshot DTOs
- engine data capability semantics
- commit validation
- commit version allocation
- MVCC visibility rules
- branch product semantics
- table compaction decisions
- recovery orchestration
- checkpoint scheduling
- retention policy
- user-facing maintenance commands
- IPC or StrataHub behavior

## Current Code Reference Map

These are the current files to reference while designing and implementing L4.

### WAL Service

- `crates/storage/src/durability/wal/mod.rs`
- `crates/storage/src/durability/wal/writer.rs`
- `crates/storage/src/durability/wal/reader.rs`
- `crates/storage/src/durability/wal/config.rs`
- `crates/storage/src/durability/wal/mode.rs`
- `crates/storage/src/durability/format/wal_record.rs`
- `crates/storage/src/durability/format/segment_meta.rs`
- `crates/storage/src/durability/payload.rs`

Current WAL service facts:

- `WalWriter` owns segment creation, append, rotation, sync, active segment
  metadata, disk usage counters, background-sync hooks, and close/drop sync.
- `WalReader` owns strict and lossy record scanning, codec decode, contiguous
  recovery reads, watermark-filtered reads, segment listing, and truncation
  facts.
- WAL segments currently are local files under `wal/`.
- WAL records are v3 outer-envelope payloads containing codec-encoded v2 record
  bytes.
- `.meta` sidecars are optional accelerators for segment coverage checks.

### Database Manifest Service

- `crates/storage/src/durability/format/manifest.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/layout.rs`

Current database manifest facts:

- The database MANIFEST stores database UUID, codec id, active WAL segment,
  snapshot watermark, snapshot id, and flush watermark.
- `ManifestManager` owns load/create/persist and uses temp-write, file sync,
  rename, and parent directory sync.
- `checkpoint_runtime.rs` wraps manifest load/create/update for checkpoint,
  WAL truncation/deletion, shutdown manifest sync, and flush-watermark
  truncation.

### Snapshot / Checkpoint Service

- `crates/storage/src/durability/disk_snapshot/mod.rs`
- `crates/storage/src/durability/disk_snapshot/writer.rs`
- `crates/storage/src/durability/disk_snapshot/reader.rs`
- `crates/storage/src/durability/disk_snapshot/checkpoint.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`
- `crates/storage/src/durability/format/snapshot.rs`
- `crates/storage/src/durability/format/watermark.rs`

Current snapshot service facts:

- `SnapshotWriter` writes a snapshot temp file, fsyncs it, renames to
  `snap-NNNNNN.chk`, and fsyncs the snapshots directory.
- `SnapshotReader` validates header, codec id, CRC, and section framing.
- `CheckpointCoordinator` currently serializes primitive checkpoint data and
  then uses `SnapshotWriter`.
- `run_storage_checkpoint` loads or creates MANIFEST, persists active WAL
  segment, writes snapshot, then persists snapshot watermark.

### WAL Truncation / Deletion Service

- `crates/storage/src/durability/compaction/mod.rs`
- `crates/storage/src/durability/compaction/wal_only.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs`

Current WAL truncation facts:

- `WalOnlyCompactor` deletes WAL segments below the safe active segment when
  covered by the effective retention watermark.
- Effective watermark is `max(snapshot_watermark, flushed_through_commit_id)`.
- Segment coverage uses `.meta` sidecars when valid and falls back to a
  codec-aware full segment scan.
- Delete-not-found is idempotent and treated as already pruned. Other deletion
  failures are logged and skipped for individual segments.

### Table / Branch Manifest Publish

- `crates/storage/src/manifest.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segment_builder.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`

Current table manifest facts:

- `segments.manifest` records branch-local immutable table files and inherited
  layers.
- It uses a separate binary format from the database MANIFEST.
- `write_manifest` uses temp-write, file sync, rename, and directory sync.
- `SegmentedStore` computes manifest payloads from branch state and records
  publication health when directory fsync fails after rename.
- Table file building also uses temp file, fsync, rename, and parent directory
  fsync.

### Quarantine Publish Evidence

- `crates/storage/src/quarantine.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`

Quarantine policy belongs mostly to L8, but the current code is important L4
evidence because it repeats the durable-publish idiom. Storage-next should not
copy this pattern again. Quarantine should consume the same L4 publish service
as manifests and table objects.

### Commit Adapter Boundary Evidence

- `crates/storage/src/durability/commit_adapter.rs`

The commit adapter belongs mainly to L7, but it defines a critical L4 boundary:
L7 must be able to write a WAL record durably before making a commit visible.
L4 should expose that operation without owning validation, version allocation,
or storage-row visibility.

### Test / Fault Evidence

- `crates/storage/src/test_hooks.rs`
- `crates/storage/src/segmented/tests/publish_failures.rs`
- `crates/storage/src/durability/checkpoint_runtime.rs` tests
- `crates/storage/src/durability/wal/writer.rs` tests
- `crates/storage/src/durability/wal/reader.rs` tests
- `crates/storage/src/durability/disk_snapshot/*` tests

Current tests already encode many L4 invariants:

- manifest publish failure before rename rolls back or preserves old state
- directory fsync failure after rename is forward-only and latched as health
- checkpoint fails before writing snapshot if MANIFEST load fails
- snapshot temp files do not remain after success
- WAL readers distinguish partial tails from corruption
- codec-aware WAL paths are required for encrypted WALs
- WAL truncation does not remove active or uncovered segments

## Storage-Next Implementation Map

M4P-L4A reconciles this architecture document with the storage-next service
layer that exists today. The old-storage references above remain parity
evidence. The current storage-next implementation lives under
`crates/storage-next/src/service` and is consumed by lifecycle/recovery code
under `crates/storage-next/src/lifecycle`.

| Storage-next service | Current role | L4A inventory status |
| --- | --- | --- |
| `service/publish.rs` | Shared object publication wrapper over L1 backend publish modes. It enforces durable publish/sync capabilities before durable create/replace and validates visible durable outcomes. | Covered by the durable publisher transition rows below. |
| `service/wal.rs` | WAL segment creation/open, append, force, read, latest-tail repair, retention delete, growth facts, and optional sidecar cleanup. | Covered by WAL create, append, repair, and retention rows. |
| `service/manifest.rs` | Database manifest, table manifest, branch catalog manifest, and pending releases manifest services over L3 codecs and L2 names. | Covered by four manifest-role rows. |
| `service/snapshot.rs` and `service/snapshot/listing.rs` | Snapshot publish/load/list/latest/prune mechanics, including live/newest protection and malformed-object classification. | Covered by snapshot publish/load/list/prune rows. |
| `service/checkpoint.rs` | Mechanical checkpoint publication sequencing for active-WAL facts, snapshots, final manifest facts, and optional table-manifest records. | Covered by checkpoint rows, with restart proof owned by L8 lifecycle tests. |
| `service/table.rs` | Immutable table object publication and object-backed reader handoff. It validates object facts before exposing a readable table object. | Covered by table object publish/open rows. |
| `service/sidecar.rs` | Optional WAL segment metadata sidecar publish/load/delete. Missing or corrupt sidecars are recoverable because the WAL segment is authoritative. | Covered by sidecar and WAL retention rows. |
| `service/quarantine.rs` and submodules | Quarantine inventory load/publish, object copy/delete mutation, family reconciliation, and purge mechanics. | Covered by quarantine inventory, source-delete, reconcile, and purge rows. |
| `service/cache_mode_absence_tests.rs` | Durable service absence checks for cache/non-durable storage mode. | Covered by cache-mode absence row. |

The current topology is intentionally different from old storage. Old storage
used branch-local `segments.manifest` files as the dominant durable table
source of truth. Storage-next splits durable state across database manifest,
branch catalog, per-branch table manifests, table-object facts, WAL segments,
checkpoint facts, snapshot objects, optional sidecars, and quarantine
inventories. That split is acceptable only if L4 returns enough mechanical facts
for L8 recovery to classify every partial publication window.

## Publication And Recovery Transition Inventory

Each row names the authoritative object family, optional object family where one
exists, required lower-layer capabilities, the visible-but-not-durable window,
the already-missing behavior, the recovery classification, and the owning test
surface.

| Transition | Authoritative object | Optional objects | Required capabilities | Visible-but-not-durable window | Already-missing behavior | Recovery classification | Owning tests |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Durable publisher create/replace | L2 object supplied by caller | None | L1 durable publish and durable sync | Publish reached namespace but durability confirmation failed | Not applicable for publish; create may reject already-existing object | `PublishFailureKind::VisibilityUnknown` or `VisibleDurabilityUnconfirmed` | `service/publish.rs` unit tests; L4C service conformance |
| WAL segment create/open | WAL segment object | WAL metadata sidecar | L1 append, read, list, metadata, durable sync | Segment object visible before close/force confirms durability | Missing segment during open/list is skipped unless it is required by recovery facts | Empty/new segment, dirty segment, or missing required segment | `service/wal/tests/append.rs`, `localfs.rs`, `read.rs` |
| WAL append with `standard` policy | Active WAL segment | WAL metadata sidecar | L1 append and read; durable sync may be deferred | Appended bytes visible but not yet forced durable | Not applicable | Dirty WAL state; strict replay may report latest partial tail | `service/wal/tests/append.rs`, `durability.rs`, `read.rs`, `corruption.rs` |
| WAL append with `always` policy | Active WAL segment | WAL metadata sidecar | L1 append and durable sync | Append visible but force failed before durable acknowledgement | Not applicable | Commit must not be acknowledged durable; replay still classifies bytes mechanically | `service/wal/tests/durability.rs`, `fault_windows.rs` |
| WAL latest-tail repair | Latest WAL segment | WAL metadata sidecar | L1 durable replace/publish, read, metadata | Repaired prefix visible but durability unconfirmed | Not applicable | Append is blocked until reopen/recovery confirms repaired state | `service/wal/tests/retention_reopen.rs` |
| WAL retention delete | Closed WAL segments below durable proof | Matching optional sidecars | L1 list, metadata, delete | Delete returned removed but durability is not confirmed | Missing segment or sidecar is idempotent cleanup | Deleted, already missing, failed, or durability-unconfirmed cleanup fact | `service/wal/tests/retention_reopen.rs`; L4B cleanup tests |
| Database manifest create/replace | `manifest/current` database manifest | None | L1 durable publish/sync, L2 database manifest name, L3 database manifest codec | New manifest visible but durability unconfirmed | Missing current manifest is allowed only on create/bootstrap paths | Load missing, decode corrupt, codec mismatch, or publish uncertainty | `service/manifest.rs` tests; `lifecycle/tests/durable.rs`; L4C conformance |
| Table manifest replace | Per-branch table manifest | Referenced table objects | L1 durable publish/sync, L2 table manifest name, L3 table manifest codec | New reachability manifest visible but durability unconfirmed | Missing manifest is recoverable as absent reachability for the owning branch | Missing, corrupt, invalid table facts, or manifest debt for L8 | `service/manifest.rs` tests; `lifecycle/tests/table_manifest_recovery.rs`; L4D restart tests |
| Branch catalog manifest replace | Branch catalog manifest | Per-branch manifests referenced by catalog | L1 durable publish/sync, L2 branch catalog name, L3 branch catalog codec | New catalog visible but durability unconfirmed | Missing catalog is bootstrap/recovery input, not a silent empty durable database | Missing, corrupt, or catalog/branch topology mismatch | `service/manifest.rs` tests; `lifecycle/tests/branch_lifecycle/catalog.rs` |
| Pending releases manifest replace | Pending releases manifest | Table objects and table manifests named by release facts | L1 durable publish/sync, L2 pending releases name, L3 pending releases codec | Pending-release fact visible but durability unconfirmed | Missing pending releases means no pending durable release facts | Missing, corrupt, or invalid recovery fact | `service/manifest.rs` tests; lifecycle release tests as added by L4D |
| Snapshot publish | Snapshot object | None | L1 durable publish/sync, L2 snapshot name, L3 snapshot codec | Snapshot visible but manifest does not yet point at it, or durability is unconfirmed | Missing snapshot is an orphan/missing-reference fact depending on manifest state | Orphan snapshot, missing referenced snapshot, corrupt snapshot, or codec mismatch | `service/snapshot/publish_load_tests.rs`, `publish_fault_tests.rs`; L4D restart tests |
| Snapshot load/list/latest | Snapshot objects under snapshot prefix | None | L1 read/list/metadata, L2 snapshot classifier, L3 snapshot codec | Not a publish transition | Missing listed object becomes malformed/missing classification | Latest valid snapshot, malformed snapshot, missing referenced snapshot | `service/snapshot/listing_tests.rs`, `listing_property_tests.rs` |
| Snapshot prune | Snapshot objects proven unreachable | None | L1 list/delete/metadata | Delete returned removed but durability is not confirmed | Missing snapshot is idempotent cleanup | Deleted, already missing, failed, protected live/newest, or malformed-object fact | `service/snapshot/listing_tests.rs`; L4B cleanup tests |
| Checkpoint active-WAL fact | Database manifest checkpoint fields | Snapshot object and optional table-manifest record | L1 durable publish/sync through manifest/snapshot services | Active-WAL fact visible before snapshot/final manifest completes | Missing manifest on bootstrap may create initial manifest | Incomplete checkpoint phase that L8 must retry or ignore safely | `service/checkpoint/tests/sequencing.rs`; `lifecycle/tests/checkpoint.rs` |
| Checkpoint snapshot/final manifest | Snapshot object plus database manifest checkpoint facts | Optional table-manifest record | L1 durable publish/sync, L3 snapshot and manifest codecs | Snapshot visible before final manifest, or final manifest visible before all referenced facts are confirmed | Missing referenced object is hard recovery evidence, not ignored success | Orphan snapshot, missing/corrupt referenced snapshot, incomplete table flush proof | `service/checkpoint/tests/sequencing.rs`; `lifecycle/tests/checkpoint/*`; L4D restart tests |
| Table object publish/open | Immutable table object | Future table sidecars, if any | L1 durable publish/read/range/metadata, L2 table object name, L5 table parser | Table object visible before table manifest names it, or durability unconfirmed | Missing object is valid only if no manifest references it | Orphan table object, missing referenced object, corrupt object, or object fact mismatch | `service/table.rs` tests; `lifecycle/tests/table_manifest_recovery.rs`; L4D restart tests |
| Rewrite publication and table-manifest debt | Newly published table objects and later table manifest | Old table objects until retention proof | L4 table publish and table manifest publish; L8 owns branch install | Output table visible before catalog update, branch install, or manifest publication | Missing prior output during retry is debt/recovery evidence | Manifest debt, orphaned output, invalid reopened table, or retryable publish failure | `lifecycle/rewrite_publication.rs` tests; `lifecycle/tests/compaction/*`; L4D restart tests |
| WAL sidecar publish/load/delete | None; sidecar is optional | WAL metadata sidecar | L1 durable publish/read/delete where used | Sidecar visible but durability unconfirmed | Missing sidecar falls back to authoritative WAL scan | Optional accelerator missing/corrupt/delete-failed fact | `service/sidecar/tests/*`; WAL retention tests |
| Quarantine inventory publish | Quarantine inventory manifest | Quarantine object copy | L1 durable publish/sync, L2 quarantine names, L3 quarantine inventory codec | Inventory visible before quarantine copy or source delete completes | Missing inventory can synthesize an empty optional inventory but must be reconciled with listed quarantine objects | Empty inventory, corrupt inventory, publish uncertainty, or inventory mismatch | `service/quarantine/tests/inventory.rs`, `mutation.rs`, `reconcile.rs` |
| Quarantine source copy/delete | Quarantine copy object and source object delete outcome | Inventory update | L1 read, metadata, durable publish, delete | Copy visible but source delete failed or durability unconfirmed | Missing source before copy is a pre-mutation failure; missing source after copy is reconciliation input | Quarantined, copy uncertain, source delete failed, or transient failure | `service/quarantine/tests/mutation.rs`; `lifecycle/tests/quarantine.rs` |
| Quarantine reconcile/purge | Quarantine inventory and quarantine objects | Source objects named by entries | L1 list/read/delete, L2 quarantine classifier, L3 inventory codec | Purge delete removed object but durability is not confirmed; inventory rewrite may fail after deletions | Missing quarantine object is idempotent purge or missing-object reconciliation fact | Listed, unlisted, missing, deleted, already missing, failed, or inventory rewrite failed | `service/quarantine/tests/reconcile.rs`; `lifecycle/tests/quarantine.rs`; L4B cleanup tests |
| Cache-mode durable service absence | No durable objects | None | Cache backend read/write/list/delete only; no durable publish/sync/WAL | Not applicable because durable services reject before mutation | Not applicable | Explicit unsupported durable service path | `service/cache_mode_absence_tests.rs`; `lifecycle/tests/cache.rs` |

The inventory intentionally records L8-owned recovery classifications beside L4
service operations. L4 owns the mechanical facts; L8 owns whether those facts
become retry, repair, quarantine, lifecycle health debt, or hard open failure.

## Current Pressure Points

### Local Filesystem Assumptions

Current L4-like code directly uses:

- `PathBuf`
- `std::fs::read_dir`
- `std::fs::File`
- `std::fs::rename`
- `File::sync_all`
- parent directory fsync
- local temp filenames

That is correct for the current local filesystem implementation, but it cannot
be the storage-next service contract. L4 must express durable publication in
backend terms and let the local filesystem backend implement it with POSIX-like
steps.

### Missing Durable Publish Primitive

The same publish protocol is duplicated across:

- database MANIFEST
- branch `segments.manifest`
- snapshot files
- quarantine manifests
- table file builds
- WAL segment creation
- segment metadata sidecars

Storage-next should factor this into one durable publish service with explicit
failure semantics.

### Mixed Service And Policy

`checkpoint_runtime.rs` is a useful transition module, but it mixes several
layers:

- L4 manifest/snapshot/WAL mechanics
- L8 lifecycle sequencing
- engine-supplied primitive checkpoint data
- retention/truncation decisions

Storage-next should keep the mechanics in L4 and move lifecycle policy to L8.

### Primitive Leakage In Snapshot Service

`SnapshotReader` validates primitive tags, and `CheckpointCoordinator`
serializes primitive DTOs. That is current-format evidence, not target L4
ownership.

L4 should write and read snapshot containers and sections. It should not decide
what a JSON, graph, vector, or event section means.

### Multiple Manifest Concepts

The current code has at least three manifest families:

1. Database MANIFEST.
2. Branch/table `segments.manifest`.
3. Quarantine `quarantine.manifest`.

They have different semantics, but they share service needs:

- encode/decode via L3
- publish atomically/durably
- read current version
- handle missing manifest where allowed
- report corruption precisely

Storage-next should distinguish manifest roles while sharing the durable
manifest service machinery.

### WAL Append Does Not Generalize To Object Stores

The current WAL writer is file-append oriented. Local filesystem can support
that well. Browser/cache can fake it. Object stores usually cannot.

Storage-next should avoid exposing "open appendable file" as the L4 contract.
It should define WAL as a service that can be implemented as:

- local appendable segment files for local FS
- in-memory append buffers for cache/browser mode
- future immutable WAL chunks plus manifest/fence publication for object-store
  mode

The first implementation only needs cache/browser and local filesystem, but the
contract should not prevent the object-store path.

M3E2 implementation decision: V1 local filesystem WAL uses an object-name based
backend append/sync primitive for existing WAL segment objects. The primitive
does not expose paths, file descriptors, or append streams above L1. L4 exposes
WAL append as a service operation; future object-store WAL can satisfy that
service with immutable chunks plus fencing instead of POSIX append.

## Target Service Set

### Durable Publisher

The durable publisher is the core L4 building block.

It should provide service operations such as:

- publish new object
- replace current object
- conditionally replace current object when a fence matches
- publish temporary object and promote it
- delete object durably where the backend supports durable deletion
- cleanup temporary objects

Local filesystem implementation:

```text
write temp file
sync temp file
rename temp to final
sync parent directory
```

Cache/browser implementation:

```text
write key
record non-durable success
```

Future object-store implementation:

```text
write immutable object
conditionally publish pointer/manifest/fence
verify published generation where possible
```

The service result should distinguish:

- not visible
- visible and durable
- visible but durability unconfirmed
- failed before publication
- failed after publication
- unsupported by backend capability

That distinction matters because the current code already treats directory
fsync failure after rename as forward-only: the new state may be visible, but
durability is unconfirmed.

### WAL Service

The WAL service owns durable log mechanics.

It should expose operations equivalent to:

- append committed record bytes
- force durability barrier
- begin/commit/abort background sync where supported
- close log writer
- list log chunks/segments
- read all records
- read records after a watermark
- read contiguous records after a watermark
- return truncation facts for partial tails
- repair the latest partial tail by durably replacing it with the validated
  prefix before appends resume
- delete/truncate safe log chunks after a retention proof
- report counters and disk/object usage

The WAL service must use L3 for record bytes and codec boundaries. It should
not parse commit payload meaning.

V1 durable commit policy must remain explicit:

1. `standard`
   WAL-backed durable mode where force-durability is handled by background or
   periodic sync according to the configured interval and backend capability.

2. `always`
   WAL-backed durable mode where the WAL service forces durability before the
   commit can be acknowledged.

`cache` is not a WAL policy. Cache/browser mode has no WAL service; L7 uses the
WAL-free commit path and reports non-durable commit facts.

Open design point: L4 should decide whether background sync is a WAL-service
API or an L8 lifecycle API that calls a simpler WAL force operation. The current
engine-internal background sync extension suggests the split is not clean yet.

### Database Manifest Service

The database manifest service owns durable publication of database-level
physical metadata.

It should expose operations equivalent to:

- load current manifest
- create initial manifest
- persist active WAL segment/chunk facts
- persist snapshot watermark
- persist flush watermark
- validate codec id
- return current durable facts
- conditional update when backend supports or requires fencing

The service must not define what a branch means or what engine features exist.

The manifest should remain a storage-local physical identity and recovery fact,
not a StrataHub fleet identity.

### Table Manifest Service

The table manifest service owns durable publication of table reachability
facts.

L5 builds immutable table objects. L6 owns branch-local table reachability,
inherited-layer meaning, fork-version frontiers, and materialization state. L4
should provide the manifest publication service used to make the L6-supplied
reachability payload durable.

The table manifest service may expose:

- publish table manifest for a branch/table namespace
- read table manifest
- report missing manifest
- report corrupt manifest
- conditionally publish manifest when fencing is required

Branch and inherited-layer semantics should be represented as payload provided
by L6/L8 or as an L3 format type owned by storage mechanics. L4 should not
become the branch model.

### Snapshot Service

The snapshot service owns snapshot object publication and loading.

It should expose operations equivalent to:

- write snapshot container from sections/row groups
- load snapshot container and raw section bytes
- validate codec id and container integrity
- list snapshots
- find latest snapshot
- cleanup temporary snapshots
- delete/prune snapshot objects when a caller supplies retention intent

L4 should not collect primitive data or install decoded rows into branch state.
For V1, committed storage state snapshots should be row-native storage
snapshots. L4 writes and loads snapshot containers from raw storage-owned row
groups. Optional opaque engine-owned sections are allowed for derived or
rebuildable state, but they must not be required to recover committed storage
rows. L6 owns decoded row installation. Engine owns any opaque section payload
meaning.

### Sidecar Service

Sidecars are optional accelerators.

Examples from current code:

- WAL segment metadata `.meta`
- future table filter/index sidecars if they become separate objects

L4 should classify sidecars as either:

- authoritative: required for correctness
- optional: can be rebuilt from authoritative objects

Current WAL segment metadata sidecars are optional. Missing or corrupt sidecars
fall back to full WAL segment scans.
When an authoritative WAL segment is deleted, the matching optional sidecar is
best-effort deleted as cleanup; failure to delete the sidecar does not affect the
retention result because the sidecar is not authoritative.

## Service Ordering Invariants

### WAL Before Visibility

L7 commit runtime must be able to call L4 so a commit record is durably written
before L6 makes the commit visible.

L4 does not validate the transaction or assign versions. It only gives L7 a
durable-log operation with clear success/failure semantics.

### Snapshot Publication Before Manifest Watermark

A manifest must not point to a snapshot that has not been durably published.

The current order is correct in shape:

1. Load/create MANIFEST.
2. Persist active WAL segment.
3. Write snapshot object durably.
4. Persist snapshot id and snapshot watermark into MANIFEST.

If a crash happens after step 3 and before step 4, the snapshot is an orphan and
can be ignored or pruned. If a crash happens after step 4, recovery must be
able to load the snapshot.

### Flush Watermark Before WAL Deletion

WAL segments must not be deleted merely because data appears in memory.

The current flush-truncation path persists `flushed_through_commit_id` before
running WAL truncation/deletion. That shape should remain. WAL deletion must be
based on durable reachability facts:

- snapshot watermark, or
- flushed table state watermark, or
- another explicitly durable retention proof.

### Active WAL Segment Is Protected

WAL truncation must not delete the active segment or any segment at or above
the safe active boundary.

The current WAL compactor uses the maximum of manifest active segment and
writer active segment. Storage-next should preserve this "writer fact beats
stale manifest fact" safety rule or replace it with a stronger manifest fencing
protocol. Table compaction remains L5/L8; this L4 service is only about durable
log reachability and safe WAL object deletion.

### Publish Failure Is Classified By Window

L4 must distinguish failure windows:

- failure before object is visible
- failure during byte write
- failure during byte durability barrier
- failure during publication
- failure after publication while confirming namespace durability
- failure during cleanup

The current `DirFsync` handling is important: after rename, the new state may
be visible even if durability is not confirmed. That must not be collapsed into
a generic IO error.

## Backend Capability Model

L4 consumes L1 backend capabilities.

### Cache / Browser Mode

Cache mode requires only:

- object read
- object range read
- object write
- object delete
- object list

It must not claim crash durability. L4 services may return successful
non-durable publication facts in cache mode, but cache mode does not create or
persist database MANIFEST, WAL, snapshot, checkpoint, table, or quarantine
objects.

### Local Filesystem Durable Mode

Local durable mode requires:

- read object
- range read where WAL/table readers need it
- write object
- append object
- delete object
- list prefix
- metadata
- durable publish or equivalent
- durable sync or equivalent
- single-writer lock or equivalent

The local filesystem backend is the reference implementation for durable L4
semantics.

### Future Object Durable Candidate Mode

Object-store mode is not required for the first rewrite, but L4 should avoid
blocking it.

Object durable mode likely requires:

- immutable object writes
- conditional create/update
- manifest/pointer fencing
- explicit list consistency assumptions
- no dependence on appendable files
- no dependence on rename
- no dependence on parent-directory fsync

If object durable mode is added early, unsupported combinations must fail at
open rather than silently weakening durability.

## Failure Model

L4 failures should be typed around service mechanics.

Expected categories:

- object not found
- object already exists
- backend capability unsupported
- publish precondition failed
- publish failed before visibility
- publish visibility unknown
- publish visible but durability unconfirmed
- sync failed
- delete failed
- list failed
- manifest load failed
- manifest publish failed
- WAL append failed
- WAL sync failed
- WAL read failed
- WAL partial tail
- WAL corruption
- WAL codec decode failed
- snapshot write failed
- snapshot read failed
- snapshot codec mismatch
- snapshot prune failed
- optional sidecar corrupt
- optional sidecar publish failed
- temporary cleanup failed

L4 errors should include the service object role and object name where useful.
They should avoid product meaning.

## Exposed Upward

L4 should expose durable services to L5-L8, not public product APIs.

Expected upward surfaces:

- `WalService`
- `ManifestService`
- `SnapshotService`
- `TableManifestService`
- `DurablePublisher`
- `SidecarService`, if retained
- service result structs containing mechanical facts
- service error enums
- service metrics/counters
- fault-injection hooks for crash and publish windows

The exact names can change. The important property is that upper layers do not
directly perform filesystem writes, renames, syncs, object deletes, or ad hoc
manifest publication.

## Required Downward

L4 should depend on:

- L1 backend IO
- L2 object layout constructors
- L3 format/codec APIs
- backend capability facts
- backend-local durability barriers

L4 should not depend on:

- engine crate
- public executor APIs
- local paths except inside the local filesystem implementation
- primitive DTOs as service-owned concepts
- global process environment
- product lifecycle scheduling

## Testing

L4 needs crash-style and fault-window tests from the beginning.

Required test groups:

- durable publisher success on local filesystem
- durable publisher failure before publish
- durable publisher failure after publish but before durability confirmation
- manifest create/load/update roundtrip
- manifest corrupt bytes
- manifest codec mismatch path where applicable
- WAL append/read roundtrip
- WAL partial tail detection
- WAL mid-segment corruption detection
- WAL lossy scan behavior, if retained
- WAL codec decode failure
- WAL active segment protection during truncation
- WAL sidecar missing/corrupt fallback
- snapshot write/read roundtrip
- snapshot temp cleanup
- snapshot codec mismatch
- snapshot CRC failure
- snapshot prune never deletes manifest-live snapshot
- table manifest publish success
- table manifest publish failure before and after namespace publication
- cache-mode service behavior reports non-durable facts
- local filesystem service behavior reports durable facts
- backend capability mismatch fails before service work starts

Fault injection should be service-level rather than scattered across individual
call sites. The current manifest publish and directory fsync hooks are good
evidence, but storage-next should generalize them into a backend or publisher
fault harness.

## V1 Minimum

The first storage-next implementation needs:

1. A durable publisher abstraction with local filesystem and cache/browser
   implementations.
2. A local filesystem WAL service with explicit `standard` and `always`
   durability-policy behavior, an explicit identity storage-codec boundary, and
   latest-partial-tail repair.
3. No WAL service in cache/browser mode; L7 uses the WAL-free commit path for
   cache/browser storage and reports explicit non-durable facts.
4. A database manifest service.
5. A snapshot service.
6. A table manifest publish service, even if the table runtime initially keeps
   the current table bytes.
7. WAL safe deletion/truncation mechanics based on durable watermarks.
8. Snapshot prune mechanics that protect the live manifest snapshot.
9. Typed service errors for publish windows and durability uncertainty.
10. Fault-window tests for manifest, snapshot, WAL, and table manifest
    publication.

The first implementation does not need:

1. Production object-store durable mode.
2. OpenDAL adapter code.
3. Multi-writer manifest fencing.
4. Distributed WAL.
5. User-facing checkpoint/compact commands.
6. Primitive snapshot materialization inside storage.
7. Quarantine policy redesign.
8. Background sync as a public storage API.

## Resolved V1 Decisions

1. WAL append is a stable L4 service operation backed by object-name based
   backend append/sync. Future object-durable mode may implement the same
   service as immutable WAL chunks, but V1 local durable mode exposes
   `WalService::append` and `WalService::force_durable`.
2. The database manifest is single-current for V1. It lives at
   `manifest/current`; manifest generation history is not part of the V1 local
   durable contract.
3. Database and table/branch manifests stay as separate services over the same
   publisher. `DatabaseManifestService` owns storage recovery facts and
   `TableManifestService` publishes opaque branch/table reachability bytes.
4. L4 does not expose background sync phases as a public storage API. L4 owns
   `force_durable`; lifecycle or commit orchestration decides when to schedule
   background sync work around that operation.
5. A publish where the final object became visible but parent-directory
   durability failed is reported as
   `PublishFailureKind::VisibleDurabilityUnconfirmed`.
6. Optional sidecar corruption is represented as a typed recoverable load fact,
   not as authoritative corruption and not as an L4 health latch. Recovery must
   be able to ignore or rebuild sidecars from authoritative WAL/snapshot state.

## Deferred Questions

1. What object-store fencing model is sufficient for a future durable OpenDAL
   backend: conditional object operations, generations, ETags, leases, or a
   higher-level manifest pointer protocol?
2. Should durable deletion require a backend durability barrier, or can deletion
   remain best-effort unless a higher layer requires proof?
3. What lifecycle health state should be latched after L4 publish uncertainty,
   and which L8 action clears it? L4 reports typed uncertainty
   (`VisibilityUnknown` or `VisibleDurabilityUnconfirmed`); L8 owns whether to
   latch degraded writer health and whether reopen, reconcile, or explicit
   maintenance clears it.

## Next Layer Dependency

L5 table runtime should build on L4 rather than writing files directly.

L5 should produce immutable table bytes and table metadata. L4 should publish
those bytes durably and publish the reachability metadata that makes them live.
This split keeps table algorithms testable without coupling them to local
filesystem publication mechanics.
