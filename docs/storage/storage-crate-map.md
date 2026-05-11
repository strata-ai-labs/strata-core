# Storage Crate Map

## Purpose

This document maps `crates/storage` as it exists today. It is descriptive, not
aspirational: the goal is to make the current crate understandable before we
design the next storage architecture.

For the target boundary of the clean storage layer, see
[storage-charter.md](./storage-charter.md).

For the explicit allowlist of storage operations the consolidated engine may
consume, see
[v1-storage-consumption-contract.md](./v1-storage-consumption-contract.md).

For the cross-boundary ownership question with engine, see
[storage-engine-ownership-audit.md](./storage-engine-ownership-audit.md).

The important takeaway is that `strata-storage` is not just a collection of
data structures. It currently owns the lower persistence runtime: storage
addressing, MVCC state, transaction coordination, segment files, WAL,
checkpoint, snapshot, recovery, runtime configuration, and operational
mechanics.

## High-Level Shape

```text
                 strata-engine
                      |
                      v
        +-----------------------------+
        |        strata-storage       |
        +-----------------------------+
        |  Public storage contract    |
        |  Transaction coordination   |
        |  MVCC / branching runtime   |
        |  Segment / index / cache    |
        |  Durability / recovery      |
        |  Operational policy knobs   |
        +-----------------------------+
                      |
                      v
              Local filesystem today
```

The center of the crate is
[SegmentedStore](../../crates/storage/src/segmented/mod.rs). It is the actual
storage runtime: branch state, active memtables, frozen memtables, immutable
segments, MVCC reads, writes, flush, compaction, copy-on-write branch layers,
snapshot install, and recovery application.

## Source Tree

```text
crates/storage/src
+-- lib.rs                      public exports
+-- traits.rs                   Storage trait and WriteMode
+-- layout.rs                   Namespace, TypeTag, Key
+-- error.rs                    StorageError and StorageResult
+-- stored_value.rs             value/deletion/timestamp representation
+-- key_encoding.rs             InternalKey encoding for segment ordering
+-- memtable.rs                 mutable in-memory writes
+-- segment_builder.rs          writes immutable segment files
+-- segment.rs                  reads immutable segment files
+-- bloom.rs                    bloom filters
+-- block_cache.rs              process-global block cache
+-- index.rs                    index helpers
+-- merge_iter.rs               merge iterator stack
+-- seekable.rs                 seekable iterator abstractions
+-- compaction.rs               generic compaction helpers
+-- segmented/
|   +-- mod.rs                  main SegmentedStore runtime
|   +-- compaction.rs           segmented-store compaction execution
|   +-- recovery.rs             recovered state and degradation health
|   +-- quarantine_protocol.rs  orphan/quarantine/purge handling
|   +-- ref_registry.rs         segment reference tracking
+-- txn/
|   +-- context.rs              TransactionContext and pending ops
|   +-- manager.rs              version allocation and commit ordering
|   +-- validation.rs           transaction validation
|   +-- lock_ordering.rs        lock discipline checks
+-- durability/
|   +-- mod.rs                  durability facade
|   +-- commit_adapter.rs       txn manager + WAL + store apply bridge
|   +-- checkpoint_runtime.rs   checkpoint/compact/prune helpers
|   +-- decoded_snapshot_install.rs
|   +-- recovery.rs             recovery planner/coordinator
|   +-- recovery_bootstrap.rs   storage recovery entrypoint
|   +-- layout.rs               database directory layout
|   +-- payload.rs              transaction payload encoding surface
|   +-- codec/                  identity/AES/storage codec abstraction
|   +-- wal/                    WAL writer/reader/config/mode
|   +-- format/                 manifest/WAL/snapshot/writeset formats
|   +-- disk_snapshot/          snapshot writer/reader/checkpoint
|   +-- compaction/             WAL-only and tombstone compaction
+-- runtime_config.rs           storage runtime config normalization
+-- pressure.rs                 memory/flush pressure
+-- quarantine.rs               quarantine support
+-- rate_limiter.rs             compaction rate limiter
+-- ttl.rs                      TTL index
+-- manifest.rs                 segment manifest support
+-- memory_stats.rs             memory accounting
+-- test_hooks.rs               fault/test hooks
```

## Major Components

### Public Contract

The public surface is rooted in [lib.rs](../../crates/storage/src/lib.rs).
Storage exposes:

- layout/addressing types from [layout.rs](../../crates/storage/src/layout.rs):
  `Key`, `Namespace`, `TypeTag`, and `validate_space_name`
- the lower storage API from [traits.rs](../../crates/storage/src/traits.rs):
  `Storage` and `WriteMode`
- storage errors from [error.rs](../../crates/storage/src/error.rs):
  `StorageError` and `StorageResult`
- the primary runtime type: `SegmentedStore`
- transaction manager/context/validation types
- durability helpers for WAL, checkpoint, snapshot install, recovery, codecs,
  manifest sync, snapshot pruning, and WAL compaction

### MVCC Runtime

[segmented/mod.rs](../../crates/storage/src/segmented/mod.rs) owns the live
storage runtime.

Important concepts:

- `SegmentedStore`: process-local storage engine instance
- `BranchState`: active mutable state for a branch
- `BranchSnapshot`: pinned read view over active, frozen, segment, and inherited
  layers
- `SegmentVersion`: immutable per-level segment lists
- `InheritedLayer`: copy-on-write branch inheritance
- `StorageIterator`: merged seekable iterator over the branch snapshot
- `DecodedSnapshotEntry`: generic decoded row used during snapshot install

The branch read order is:

```text
active memtable
  -> frozen memtables, newest first
  -> L0 segments, newest first
  -> L1-L6 segments, sorted/non-overlapping
  -> inherited branch layers
  -> MVCC filter
```

### Segment Layer

The immutable segment path is spread across:

- [memtable.rs](../../crates/storage/src/memtable.rs)
- [segment_builder.rs](../../crates/storage/src/segment_builder.rs)
- [segment.rs](../../crates/storage/src/segment.rs)
- [bloom.rs](../../crates/storage/src/bloom.rs)
- [block_cache.rs](../../crates/storage/src/block_cache.rs)
- [key_encoding.rs](../../crates/storage/src/key_encoding.rs)
- [merge_iter.rs](../../crates/storage/src/merge_iter.rs)
- [seekable.rs](../../crates/storage/src/seekable.rs)

This layer owns the mechanics of turning ordered internal keys into immutable
files and reading those files back efficiently through indexes, bloom filters,
seekable iterators, and the process-global block cache.

### Transaction Layer

The transaction layer lives under [txn/](../../crates/storage/src/txn).

It owns:

- transaction IDs
- version allocation
- branch commit locks
- commit quiescing
- visible-version tracking
- validation
- pending operation grouping

The important split is that `TransactionManager` owns generic commit ordering,
while durability plugs into the commit path through a hook. WAL persistence is
not embedded directly in the transaction manager.

### Durability Layer

The durability subtree is rooted at
[durability/mod.rs](../../crates/storage/src/durability/mod.rs).

It owns:

- WAL read/write/configuration under `durability/wal/`
- durable byte formats under `durability/format/`
- storage codecs under `durability/codec/`
- database directory layout in `durability/layout.rs`
- checkpoint and snapshot file mechanics under `durability/disk_snapshot/`
- checkpoint, manifest sync, WAL compaction, and snapshot pruning in
  `durability/checkpoint_runtime.rs`
- decoded-row snapshot install in `durability/decoded_snapshot_install.rs`
- recovery planning and bootstrap in `durability/recovery.rs` and
  `durability/recovery_bootstrap.rs`
- transaction-to-WAL commit bridging in `durability/commit_adapter.rs`

This layer is the lower durable runtime. Engine is supposed to provide product
meaning and primitive decoding; storage owns raw persistence mechanics.

### Operational Support

Storage also owns several operational support surfaces:

- [runtime_config.rs](../../crates/storage/src/runtime_config.rs): normalized
  storage runtime config
- [pressure.rs](../../crates/storage/src/pressure.rs): flush/memory pressure
- [quarantine.rs](../../crates/storage/src/quarantine.rs): quarantine support
- [rate_limiter.rs](../../crates/storage/src/rate_limiter.rs): compaction rate
  limiting
- [memory_stats.rs](../../crates/storage/src/memory_stats.rs): memory accounting
- [test_hooks.rs](../../crates/storage/src/test_hooks.rs): fault/test hooks

## Write Path

```text
engine operation
  |
  v
TransactionContext
  |
  v
TransactionManager
  | allocates version and orders commit
  v
durability::commit_adapter
  | writes WAL first when durability requires WAL
  v
SegmentedStore::apply_writes_atomic
  |
  v
active Memtable
  |
  | rotate
  v
frozen Memtables
  |
  | flush
  v
SegmentBuilder
  |
  v
KVSegment files
  |
  v
SegmentVersion for the branch
```

In durable modes, the commit adapter places WAL persistence before visibility in
the store. In cache/ephemeral paths, WAL is bypassed and the same generic
transaction machinery still supplies versioning and commit ordering.

## Read Path

```text
get / scan / history / as-of read
  |
  v
SegmentedStore
  |
  v
BranchSnapshot
  |
  +--> active memtable
  +--> frozen memtables
  +--> L0 segments
  +--> L1-L6 segments
  +--> inherited COW layers
  |
  v
seekable + merge iterators
  |
  v
MVCC filtering by version/timestamp
  |
  v
Versioned value
```

The read path is branch-aware and version-aware. It must merge data from live
memtables, frozen memtables, immutable segment levels, and inherited branch
layers before applying MVCC visibility.

## Recovery Path

```text
database directory
  |
  v
DatabaseLayout
  |
  +--> ManifestManager
  +--> SnapshotReader
  +--> WalReader
  |
  v
RecoveryCoordinator / run_storage_recovery
  |
  +--> install decoded snapshot rows
  +--> replay WAL records
  +--> recover/reconcile segment files
  +--> quarantine bad or orphaned files
  |
  v
SegmentedStore recovered state
```

Recovery is split between durable-file interpretation and store application.
The snapshot reader and WAL reader produce storage-level facts; decoded snapshot
install and WAL replay apply those facts into `SegmentedStore`.

## Checkpoint And Retention Path

```text
checkpoint request
  |
  v
CheckpointCoordinator
  |
  +--> collect checkpoint data
  +--> SnapshotWriter
  +--> Manifest update
  +--> WAL compaction/truncation
  +--> snapshot retention pruning
```

Checkpointing currently spans both runtime state and durable file mechanics.
Storage owns the raw mechanics; engine remains responsible for product-shaped
checkpoint content and primitive semantics.

## Current Dependency Shape

`strata-storage` has one normal workspace dependency:

- `strata-core`

The normal production incoming dependency is:

- `strata-engine`

Crates above engine should not drive storage directly. The root package may use
storage in dev/test paths for storage-facing integration tests, but production
open/recovery/checkpoint/retention policy flows through engine.

## Current Architectural Read

The crate currently combines these responsibilities:

```text
strata-storage today =
  storage addressing and key layout
+ MVCC branch store
+ transaction/version coordinator
+ segment file format and read path
+ WAL/checkpoint/recovery system
+ runtime policy/configuration
+ fault/test hooks
```

This is workable, but not yet reference-grade. The main architectural pressure
points are:

- `segmented/mod.rs` is too large and owns too many runtime concerns.
- `durability/` is better separated, but still mixes runtime services, byte
  formats, recovery policy, checkpoint policy, and filesystem mechanics.
- Storage still contains some product-shaped vocabulary, especially around
  `TypeTag`, primitive snapshot DTOs, and durable writeset payloads.
- The block cache is process-global, which makes isolation and multi-database
  testing harder.
- Many small grouping structs were introduced during cleanup; some are useful
  boundary objects, while others should eventually be folded into clearer
  repeatable patterns.

## Next Mapping Step

The next architecture pass should not start by moving code. It should first
define the intended storage-next layers:

```text
storage-next candidate layers
+-- data model and keyspace
+-- write path
+-- read path
+-- branch/version runtime
+-- segment/table format
+-- durability services
+-- backend abstraction
+-- runtime configuration
+-- test/fault framework
```

Only after those layers are explicit should we decide which existing modules can
move as-is, which need refactoring, and which should be rebuilt around a cleaner
contract.
