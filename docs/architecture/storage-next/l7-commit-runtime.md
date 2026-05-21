# L7. Commit Runtime

Status: V1 architecture draft

Depends on:

- [L3. Durable Format / Codec](./l3-durable-format-codec.md)
- [L4. Log / Manifest / Snapshot Services](./l4-log-manifest-snapshot-services.md)
- [L6. Branch-Isolated LSM Runtime](./l6-branch-isolated-lsm-runtime.md)

Consumed by:

- L8. Lifecycle / Recovery / Maintenance
- [L9. Storage API Boundary](./l9-storage-api-boundary.md)
- engine-next, through storage's internal commit-batch API

## Purpose

L7 owns Strata's internal commit unit.

Storage-next should not expose public begin/commit/rollback transaction
sessions as a V1 product surface. Users should use product operations such as
get, set, put, delete, append, search, and branch operations. Storage still
needs a precise commit runtime so writes can be ordered, validated, made
durable, and made visible without corrupting branch-local LSM state.

L7 is that runtime.

## Core Decision

The storage write contract is an internal `CommitBatch`, not a public
transaction session.

The target shape is:

```text
engine-next builds semantic operation
  |
  v
storage CommitBatch
  branch target
  rows to put/delete
  expiry/write-mode metadata
  optional conflict facts
  |
  v
L7 validates, assigns version/timestamp, and orders commit
  |
  v
L4 appends durable commit record when durability requires WAL
  |
  v
L6 installs committed rows into branch-local state
  |
  v
L7 publishes commit visibility and returns CommitOutcome
```

This preserves the useful part of the current transaction system: one commit
version for all rows in a commit, read-your-writes when an internal operation
needs a staged view, optimistic conflict detection when requested, branch commit
guards, WAL-before-visible discipline, and visible-version tracking.

It removes the product assumption that users should manage long-lived storage
transactions directly.

## Responsibilities

L7 owns:

- internal commit-batch shape
- transaction/commit IDs where storage-local
- commit-version allocation
- commit timestamp allocation
- commit ordering
- commit visibility publication
- branch commit guards
- commit quiescing for checkpoint/fork/recovery safety
- branch-deletion commit barriers
- optional read-set and CAS conflict validation
- read-only commit fast path
- WAL-before-visible discipline through L4
- durable-but-not-visible classification
- atomic application of a committed batch into L6
- visible-version tracking
- lock-order rules for commit path locks
- storage-local commit metrics and commit facts needed by L8

L7 does not own:

- public user transaction commands
- product ACID claims
- engine data capability semantics
- JSON path semantics
- graph/vector/search/index side effects
- embedding or inference side effects
- product branch workflows such as merge, cherry-pick, revert, restore, or
  publish
- backend object naming
- WAL byte format
- table byte format
- branch-local LSM table layout
- checkpoint, compaction, retention, or repair scheduling
- multi-process or distributed consensus

## Layer Boundary

```text
L8 Lifecycle / Recovery / Maintenance
  schedules shutdown, checkpoint, replay, repair, and write gates
  |
  v
L7 Commit Runtime
  validates and orders commit batches
  assigns versions/timestamps
  writes durable commit records through L4
  applies committed rows through L6
  |
  +--> L4 Log / Manifest / Snapshot Services
  |      append WAL record envelope, apply standard/always durability policy
  |
  +--> L6 Branch-Isolated LSM Runtime
         install rows/tombstones into branch-local mutable state
```

L7 may call L4 and L6. L4 must not call L7. L6 must not call L7.

Engine-next should not import WAL records, `TransactionContext`, or branch-LSM
internals. It should submit semantic writes as storage commit batches.

## Current Code Evidence

The current implementation already contains most L7 mechanics, but they are
spread across storage and engine and are still named around public
transactions.

### Core L7 Evidence

- `crates/storage/src/txn/context.rs`: current staged write/read context,
  read-your-writes behavior, read set, delete set, CAS set, TTL map, write
  modes, transaction lifecycle, and apply summary.
- `crates/storage/src/txn/manager.rs`: storage-owned transaction manager,
  commit version allocation, retired transaction ID allocation evidence, branch
  commit locks, commit quiescing, visible-version tracking, pending-version
  tracking, branch deletion barriers, and no-WAL commit path.
- `crates/storage/src/txn/validation.rs`: read-set and CAS validation. Current
  semantics are snapshot isolation with first-committer-wins over read-set/CAS
  facts; blind writes do not conflict.
- `crates/storage/src/txn/lock_ordering.rs`: explicit commit-path lock order.
- `crates/storage/src/durability/commit_adapter.rs`: WAL-before-storage bridge.
  It serializes one commit payload, appends it to WAL when required, then lets
  the storage transaction manager apply rows.
- `crates/storage/src/durability/payload.rs`: current commit payload shape.
- `crates/storage/src/durability/format/wal_record.rs`: WAL record envelope and
  durable commit record shape.

### Boundary Evidence

- `crates/storage/src/traits.rs`: current storage trait exposes
  `apply_writes_atomic`, `put_with_version_mode`, `delete_with_version`, and
  version-bounded reads.
- `crates/storage/src/segmented/mod.rs`: current `apply_writes_atomic`,
  `apply_recovery_atomic`, version tracking, branch mutable-table insertion, and
  recovery-specific timestamp preservation. This file mixes L6 apply mechanics
  with L7 commit application and L8 recovery/lifecycle helpers.
- `crates/engine/src/coordinator.rs`: current engine transaction coordinator
  wraps storage's `TransactionManager` with active transaction metrics, timeout
  checks, GC-safe version tracking, and error conversion.
- `crates/engine/src/database/transaction.rs`: current database-level begin and
  commit orchestration, writer-health checks, generation guard validation,
  write backpressure, WAL selection, flush scheduling, and post-commit observer
  notification.
- `crates/engine/src/transaction/owned.rs` and `crates/engine/src/transaction/pool.rs`:
  current public/manual transaction handle and pooled `TransactionContext`.
  These are evidence for optimization and internal staging, not evidence that
  public transaction sessions should remain a product API.

## Target Concepts

L7 should use a small set of repeatable concepts.

### CommitBatch

`CommitBatch` is the internal write unit submitted to storage.

It should contain:

- target branch
- puts
- deletes
- expiry metadata, with zero meaning no expiry
- optional write-mode or retention hints
- optional expected read facts
- optional CAS facts
- optional operation origin for diagnostics

It should not contain:

- primitive DTOs
- JSON path operations
- graph edge/node semantics
- vector embedding semantics
- search indexing semantics
- user-facing transaction state
- WAL record bytes
- table object names

Engine-next may build a `CommitBatch` from one product operation or from a
small engine-internal operation group. The batch is the storage boundary.

### CommitRow

`CommitRow` is a storage-shaped row mutation.

It should contain:

- physical row key or storage key parts
- row value bytes or storage value
- mutation kind: put or tombstone
- expiry metadata, with zero meaning no expiry
- retention/write-mode hint if retained

L7 assigns version and timestamp to the row at commit time. Engine should not
pre-stamp rows with storage commit versions.

### CommitValidator

`CommitValidator` checks storage facts before versioned visibility.

Validation should include:

- commit batch is not malformed
- branch target is allowed to receive writes
- branch is not marked deleting
- row keys belong to the target branch unless the operation is explicitly
  storage-internal and cross-branch safe
- read facts still match current visible versions when optimistic validation is
  requested
- CAS facts still match current visible versions
- write-buffer limits and row-count limits are respected

The current read-set/CAS validation is useful and should be preserved as an
internal capability. It should not be marketed as full serializable
transactions. The current model allows write skew, and that is acceptable unless
the V1 product explicitly chooses a stronger claim.

### CommitGuard

`CommitGuard` represents the ordered locks held while a mutating commit is
being prepared, made durable, and applied.

The current lock order is a good starting point:

```text
1. commit quiesce read guard
2. per-branch commit mutex
3. deletion/reference barrier only where required by branch-table operations
4. WAL mutex
5. branch/table state guards
```

The target should keep this as an explicit storage contract. New locks should
be assigned a level before implementation.

### VersionClock

`VersionClock` owns commit-version allocation.

Rules:

1. Commit versions are monotonically increasing.
2. Commit versions do not need to be dense.
3. A failed pre-visible commit may leave a version gap.
4. A durable WAL record permanently reserves its commit version.
5. Recovery must advance the allocator above every recovered commit version.
6. Fork-version capture must use applied branch max version, not an
   allocated-but-unapplied global version.

The current code already treats version gaps as acceptable. For example, a
pre-apply hook failure can reserve a version and then return no visible rows.
Storage-next should either preserve version gaps deliberately or explicitly
design them away. It must not accidentally rely on dense version sequences.

### VisibleVersionTracker

`VisibleVersionTracker` publishes the highest version that readers may treat as
fully applied.

The current design has both storage's branch/global version facts and the
transaction manager's visible version. Storage-next should define this more
clearly:

- `allocated_version`: highest version reserved by L7
- `durable_version`: highest version known durable in WAL or equivalent
- `applied_version`: highest version applied to L6 branch state
- `visible_version`: highest version safe for new snapshots

The V1 implementation may collapse some of these if a backend does not need the
distinction, but failure handling must preserve the distinction between durable
and visible.

### CommitOutcome

`CommitOutcome` should return:

- commit version
- commit timestamp
- rows written
- rows deleted
- durable status
- visibility status
- branch target
- optional WAL object/segment facts for diagnostics

The outcome should be storage-shaped. Engine can convert it into product
responses and can run best-effort post-commit observers.

## Commit Batch Model

The target batch model should be single-branch by default:

```text
CommitBatch
  branch_id
  mutations
    Put(row_key, value, ttl, write_mode)
    Delete(row_key)
  validation
    ReadVersion(row_key, expected_version)
    CompareVersion(row_key, expected_version, new_value)
  options
    durability requirement
    conflict validation mode
    timestamp mode
```

Single-branch default keeps branch commit locking simple and matches the common
write path.

Cross-branch product operations should usually be represented as:

1. engine reads from source branches using L6/L9 versioned reads,
2. engine computes semantic result,
3. engine submits one target-branch commit batch.

If storage-next later needs a true multi-branch storage commit, it should be a
separate design with deterministic branch-lock ordering or a quiesce guard. It
should not appear accidentally because a caller passed rows from multiple
branches.

## Commit Protocol

### Read-Only Batch

Read-only work should not allocate a commit version.

Flow:

```text
validate batch has no mutations
return current visible version / read snapshot fact
```

Read-only work is usually better expressed as L6/L9 reads rather than L7
commits. The read-only fast path exists only for internal compatibility and
diagnostics.

### Cache / No-WAL Commit

Cache mode has no crash durability claim.

Flow:

```text
validate batch
acquire commit guard
allocate commit version and timestamp
apply rows atomically through L6
publish visible version
release guard
return CommitOutcome { durable: false, visible: true }
```

If L6 apply fails before visibility, the commit is aborted. Since no WAL exists,
there is no recovery obligation.

### Durable Local Commit

Durable local mode must write a commit record before making rows visible.

Flow:

```text
validate batch
acquire commit guard
allocate commit version and timestamp
encode commit payload through L3
append commit record through L4 according to the selected durable policy
apply rows atomically through L6
publish visible version
release guard
return CommitOutcome { durable: true, visible: true }
```

The WAL record is the durability point. L6 visibility comes after L4 reports
that the record is durable enough for the selected durable policy.

For V1, the selected durable policy is one of:

1. `standard`
   L4 has accepted the WAL record and the background or periodic durability
   policy is responsible for forcing it within the configured window.

2. `always`
   L4 has accepted the WAL record and forced the required durability barrier
   before L7 acknowledges the commit.

`cache` bypasses this path entirely. It is a WAL-free storage mode, not a
durable commit policy.

### Object Backend Candidate Commit

Object-backed durable commits are post-V1 unless explicitly scoped otherwise.

If added, L7 must not assume POSIX append, rename, file handles, or directory
fsync. It should depend on L4 capability declarations:

- appendable log or append-equivalent object log
- conditional create/update for commit fencing
- manifest fencing if append cannot be trusted
- documented list/read-after-write assumptions

If those capabilities are absent, durable open must fail before commit runtime
starts.

## WAL-Before-Visible Contract

L7 must not make a mutating durable commit visible until L4 has accepted the
commit record.

Rules:

1. L7 assigns version before WAL encode.
2. L3 encodes the commit payload.
3. L4 appends the WAL record and applies the selected `standard` or `always`
   durability policy.
4. Only after L4 succeeds may L7 call L6's apply operation.
5. Only after L6 apply succeeds may L7 publish visibility.

Crash outcomes:

- before WAL append succeeds: commit is not durable and must not become visible
- after WAL append succeeds, before visibility: commit is durable and must be
  replayed by L8 recovery
- after visibility, before table flush: commit is visible in memory and still
  recoverable from WAL
- after table flush and flush watermark publication: L8 may later make the WAL
  segment eligible for truncation

## Visibility And Snapshot Reads

New snapshots need a coherent visibility point.

The current implementation waits for `visible_version` to catch up to the
storage version before constructing normal transaction snapshots. That protects
readers from observing a version whose rows are not fully applied.

Storage-next should make the rule explicit:

1. A reader snapshot may target only a visible version.
2. L7 publishes visible version after L6 apply completes.
3. L6 read APIs use the requested visible version as the upper bound.
4. L8 recovery may restore visible version from recovered state, but only after
   replaying durable commits into L6.
5. A durable-but-not-visible commit must not be reported as visible to normal
   reads in the current process.

If storage-next keeps separate per-branch visible versions, L7 must define how
global reads and cross-branch operations choose a safe snapshot. If it keeps one
global visible version, it must preserve the current cross-branch safety
property without causing unnecessary stalls.

## Read-Your-Writes

Public transaction sessions are not a V1 product requirement, but some internal
engine operations still need a staged view while building a commit.

The target should separate two concepts:

1. `CommitBatch`: the final storage write unit submitted to L7.
2. `CommitDraft` or engine-owned semantic draft: an optional builder used while
   evaluating a product operation.

The staged view may support:

- read own put
- read own delete as missing
- scan overlay of staged rows over L6 snapshot reads
- read-set capture for validation when requested
- CAS fact capture

This builder should not become a public storage transaction session by default.
Engine-next can own product-specific staging when it needs semantic reads before
commit. L7 only needs the final batch and validation facts.

## Conflict Model

V1 should be honest about conflict guarantees.

The current model is snapshot isolation with first-committer-wins validation for
read-set and CAS facts:

- reads capture the version observed
- commit validates those versions against current storage
- CAS validates expected versions
- blind writes do not conflict
- write skew is allowed

This is a reasonable internal model. It is not a full serializable transaction
claim.

Storage-next should preserve the model unless product requirements explicitly
choose a different guarantee. If V1 removes public transaction commands, the
conflict model becomes an internal correctness mechanism, not a user-facing
feature.

## Branch Guards

L7 is responsible for preventing commits into branches that are unsafe to
mutate.

Required guards:

- per-branch commit lock for normal target-branch commits
- branch-deleting marker that rejects new commits
- branch generation guard if branch IDs can be reused after deletion/recreation
- commit quiesce guard for checkpoint, fork, recovery, and operations that need
  a stable version point
- explicit cross-branch permission for storage-internal operations that cannot
  be represented as one target-branch batch

Branch product semantics stay above storage. L7 should not know why a branch is
being deleted, merged, restored, or materialized. It only enforces storage
mutation safety.

## Commit Timestamps

Today, WAL commit records carry timestamps and normal storage apply paths stamp
rows at apply time. Recovery-specific paths preserve WAL timestamps.

Storage-next should tighten this:

1. L7 allocates one commit timestamp per commit.
2. L3 encodes that timestamp into the commit payload/WAL record.
3. L6 applies every row in that commit with the same commit timestamp.
4. L7 records the commit in a per-branch commit timeline.
5. L8 recovery replays the original timestamp and catches up the timeline.

This is necessary because product `as_of`, timeline scrub, and
branch-from-time are timestamp features rather than version aliases. Engine
owns the product selector and explanation; storage owns the generic
timestamp-to-version substrate.

The timeline is storage-native V1 substrate:

```text
branch id + commit timestamp + commit version -> commit version
branch id + commit version                    -> commit timestamp
```

Timestamp lookup returns the greatest retained commit version at or before the
requested timestamp. If multiple commits share a timestamp, the greatest commit
version is the deterministic tiebreaker.

The physical representation is a storage-owned system-row family under
`storage_space_id = 0x01`, defined in
`docs/architecture/storage-next/commit-timeline-substrate.md`. It is not a
separate L4 object service. L7 writes timeline rows in the same internal commit
unit as user rows; L8 recovery replays or validates those rows like any other
storage-owned durable row family.

## Recovery Interaction

L8 owns recovery orchestration. L7 owns commit replay rules.

During WAL replay:

1. L8 reads WAL records through L4.
2. L3 decodes commit payloads.
3. L8 submits recovered rows to L6 using the version and timestamp from the WAL.
4. L7's version clock catches up above the maximum recovered commit version.
5. L7 restores visible-version facts only after recovery has installed durable
   rows into L6.

V1 storage-next does not keep durable storage transaction IDs. If a future
private optimization reintroduces them, it must also add allocator catch-up
rules and tests.

Recovery replay should bypass normal conflict validation. The WAL record already
represents a committed durability fact.

Replay must be idempotent. Replaying the same durable commit after a crash must
not duplicate visible rows or corrupt row history.

## Commit Observers And Side Effects

Engine-owned post-commit side effects must stay above L7.

Examples:

- search index refresh
- graph audit hooks
- vector sidecar updates
- embedding queues
- inference-backed generation
- user-facing metrics or events

L7 may return `CommitOutcome` facts that engine observers consume. Observer
failure after durable commit must not turn the storage commit into a failure.

If a side effect must be part of storage atomicity, it is not a post-commit
observer; it must be represented as storage rows inside the same `CommitBatch`.

## Failure Model

L7 failures should be storage-local and phase-specific.

Important failures:

- invalid commit batch
- branch not writable
- branch deleting
- branch generation changed
- commit conflict
- commit timed out or exceeded configured limits
- version counter overflow
- WAL writer halted
- WAL append/sync failed
- commit durable but not visible
- L6 apply failed before visibility
- visibility publish failed
- commit quiesce unavailable or caller-level maintenance deadline
- unsupported durability capability for backend

Failure phases:

### Rejected Before Version Allocation

The commit is neither durable nor visible. Caller may retry after fixing the
input or conflict.

Examples:

- malformed batch
- branch deleting
- validation conflict
- write buffer limit exceeded

### Failed After Version Allocation But Before WAL Durability

The commit is not durable and not visible. A version gap may remain.

Version gaps are acceptable if the version model documents that versions are
monotonic, not dense.

### Failed After WAL Durability But Before Visibility

The commit is durable but not visible in the current process.

This must surface distinctly. L8 recovery must replay the WAL record on restart,
or an in-process repair path must install the durable commit before normal
writes resume.

### Failed After Visibility

The commit is visible. Later side-effect failures or observer failures should
not roll it back. Any required repair belongs to engine or L8 depending on the
side effect.

## Backend Matrix

| Backend mode | WAL required | Durable claim | L7 behavior |
| --- | --- | --- | --- |
| Browser/cache | No | No crash durability | validate, version, apply to L6, publish visible |
| Local filesystem standard | Yes | WAL-backed crash recovery with bounded sync window | validate, version, L4 WAL append, L6 apply, publish visible, background/periodic sync |
| Local filesystem always | Yes | WAL-backed crash recovery with per-commit durability barrier | validate, version, L4 WAL append + force durability, L6 apply, publish visible |
| Future OpenDAL/object | Only if backend declares equivalent capability | Not V1 by default | fail fast unless L4 can provide a commit durability primitive |

L7 must not pretend an object backend is durable just because it can write
objects. It needs a durable commit primitive from L4.

## Testing Requirements

L7 needs direct tests that do not require engine primitives.

Unit tests:

1. Empty/read-only batch does not allocate a new commit version.
2. Mutating batch allocates exactly one commit version.
3. All rows in a batch share the same version and timestamp.
4. Puts and deletes in one batch become visible atomically.
5. Version gaps do not break latest, `getv`, or history reads.
6. Read-set conflicts are detected when validation facts are supplied.
7. CAS conflicts are detected.
8. Blind writes do not conflict if the retained model allows them.
9. Branch-deleting marker rejects commits before version allocation.
10. Branch generation mismatch rejects commits before visibility.
11. Counter overflow returns a typed error.

Concurrency tests:

1. Per-branch commit ordering is deterministic.
2. Commits on different branches do not corrupt global visible-version facts.
3. Commit quiesce drains in-flight commits and blocks new mutating commits.
4. Lock-order tests prove no path acquires locks out of order.
5. Long-running read snapshots do not let retention reclaim visible versions.

Durability tests:

1. WAL append failure leaves no visible rows.
2. Crash after WAL append and before visibility replays the commit.
3. L6 apply failure after WAL append returns durable-but-not-visible.
4. Recovery catches up the commit-version allocator.
5. WAL timestamps are preserved through recovery.
6. WAL-free cache mode does not claim crash durability.

Property/fuzz tests:

1. Random commit batches preserve atomic visibility.
2. Random interleavings of commits and quiesce never expose partial batches.
3. Random version gaps preserve ordered history.
4. Random conflict facts either commit with unchanged facts or reject cleanly.
5. Crash-point fuzzing across every commit phase preserves WAL-before-visible.

Guard tests:

1. Public storage API does not expose begin/commit/rollback transaction
   sessions.
2. Engine-next does not import WAL record structs or transaction internals.
3. L7 tests use storage-shaped rows, not engine primitive DTOs.

## V1 Minimum

The first storage-next L7 implementation needs:

1. Internal `CommitBatch`.
2. Single-branch mutating commit path.
3. Storage-owned monotonic commit-version allocator.
4. One commit timestamp per commit.
5. Optional read-set/CAS validation facts, preserving current conflict behavior.
6. Per-branch commit guard.
7. Commit quiesce guard.
8. Branch-deleting rejection.
9. WAL-before-visible through L4 for durable local `standard` and `always`
   modes.
10. WAL-free path for browser/cache mode with no crash durability claim.
11. Atomic L6 apply for puts and tombstones.
12. Visible-version publication after L6 apply.
13. Durable-but-not-visible error classification.
14. Recovery catch-up hooks for the commit-version allocator.
15. Direct L7 tests for conflict, ordering, WAL failure, version gaps, and
    crash-point behavior.

## Deferred Or Removed

Not required for V1:

1. Public transaction sessions.
2. User-facing ACID transaction commands.
3. Serializable isolation.
4. Distributed commits.
5. Multi-writer object-store durability.
6. Cross-branch atomic commit batches.
7. Savepoints.
8. Nested transactions.
9. Two-phase commit.
10. External transaction IDs as a public API.
11. Durable storage transaction IDs and transaction-ID allocator catch-up.

## Open Questions

1. Does V1 keep snapshot isolation with read-set/CAS validation, or reduce the
   public claim while keeping validation only for internal guards?
2. Should commit versions remain global, or should storage introduce per-branch
   visible versions while preserving global ordering for history and recovery?
3. Are version gaps explicitly part of the storage contract?
4. What exact row value representation should `CommitRow` carry: storage
   `Value`, opaque bytes, or an L3-encoded row payload?
5. Should branch generation guards stay in engine branch-control logic, or move
   into storage branch metadata once storage-next owns branch lifecycle
   mechanics?
6. How much of the current transaction pool optimization is worth preserving
   once public transaction sessions are removed?
7. What metrics are stable L7 facts versus L8 health aggregation?

Question 5 is also an engine-next architecture input. Engine-next must decide
which product branch-generation guarantees it expects storage-next to enforce
mechanically before the L7 implementation plan freezes.

## Next Step

The next storage-next document should define L8 Lifecycle / Recovery /
Maintenance. It should explain storage open sequencing, recovery replay,
checkpoint orchestration, compaction scheduling, retention scheduling,
quarantine/repair, shutdown, and how L8 consumes L4-L7 without taking ownership
of backend IO, table mechanics, branch semantics, or commit ordering.
