# L9. Storage API Boundary

Status: V1 boundary draft after L1-L8 alignment

Depends on:

- [L1. Backend IO](./l1-backend-io.md)
- [L2. Object Layout](./l2-object-layout.md)
- [L3. Durable Format / Codec](./l3-durable-format-codec.md)
- [L4. Log / Manifest / Snapshot Services](./l4-log-manifest-snapshot-services.md)
- [L5. Table Runtime](./l5-table-runtime.md)
- [L6. Branch-Isolated LSM Runtime](./l6-branch-isolated-lsm-runtime.md)
- [L7. Commit Runtime](./l7-commit-runtime.md)
- [L8. Lifecycle / Recovery / Maintenance](./l8-lifecycle-recovery-maintenance.md)

Consumed by:

- engine-next

## Purpose

L9 is the only normal production boundary engine-next consumes from
storage-next.

Its job is to keep engine-next out of backend IO, object naming, durable byte
formats, WAL internals, manifest internals, table internals, branch-LSM
internals, commit internals, and lifecycle internals.

Storage exposes mechanics. Engine owns meaning.

This document is still not a Rust trait design. It is the aligned API contract
after L1-L8. The exact traits, structs, names, and module layout should be
designed during implementation planning, but they should preserve the ownership
rules here.

The storage-next reading order intentionally mentions L9 twice: first as an
early sketch so lower layers know their consumer, and last as this aligned
boundary after L1-L8 have defined their contracts.

## Boundary Rule

Engine-next consumes storage-next. Product crates above engine-next do not.

Allowed normal production consumer:

- engine-next

Allowed exceptions:

- storage-next internals
- storage tests
- storage benches
- storage fuzz targets
- storage diagnostic tools
- migration or verification tools
- engine-next tests that intentionally characterize storage behavior

Not allowed as normal production storage consumers:

- executor
- CLI
- SDK surfaces
- intelligence
- inference
- Strata AI
- StrataHub

If an upper layer needs storage-backed behavior, engine-next exposes an
engine-owned semantic API.

## Layer Alignment

The L9 boundary should expose only the stable mechanics produced by the layers
below it.

| Layer | What L9 may expose upward | What L9 must hide |
| --- | --- | --- |
| L1 Backend IO | backend capability facts, selected backend mode | raw backend IO, provider handles, local filesystem details |
| L2 Object Layout | diagnostic object roles when useful | object-name constructors, path/key formatting |
| L3 Format / Codec | codec id, typed decode/encode errors, format compatibility facts | byte envelopes, primitive DTOs, low-level decoders |
| L4 Durable Services | durability outcomes, WAL/snapshot/manifest/table service facts | WAL reader/writer internals, manifest mutation APIs, table publish mechanics |
| L5 Table Runtime | raw storage rows, cursors, table health facts through higher APIs | table readers/builders, bloom/index/cache internals |
| L6 Branch LSM | branch-local reads, history, scans, COW/fork/materialization mechanics | product branch workflows and primitive-aware diff |
| L7 Commit Runtime | `CommitBatch`, commit outcome, conflict/stall facts | public transaction sessions, WAL records, lock internals |
| L8 Lifecycle | open outcome, recovery health, maintenance facts, close outcome | product open policy, IPC, primitive snapshot meaning |

## Core Boundary Principle

Storage-next may expose:

- storage modes and backend capability facts
- physical or storage-shaped keys
- opaque values or storage row values
- commit batches
- branch-local storage mechanics
- versioned reads
- timestamp-bounded reads over storage commit timestamps
- history reads
- storage checkpoint/recovery hooks with primitive-neutral payloads
- raw recovery outcomes
- raw maintenance outcomes
- raw health and metrics facts
- test/fault hooks behind test or feature gates

Storage-next must not expose product semantics:

- JSON path behavior
- event-chain meaning
- vector collection behavior
- embedding policy
- graph ontology, traversal, analytics, or relationship semantics
- search ranking
- retrieval recipes
- public branch workflow behavior
- IPC behavior
- Strata AI behavior
- StrataHub dataset or fleet behavior

## Normal Engine-Facing Surface

This section describes the expected V1 boundary categories. It intentionally
uses conceptual names, not final Rust names.

### Open / Create

Storage-next should expose a storage open/create operation shaped around L8
`StorageOpenPlan`.

Inputs:

- backend descriptor, backend configuration, or storage-owned backend handle
- layout root or database-relative root
- storage mode: cache or durable local for V1
- durability policy: none for cache, `standard` or `always` for durable modes
- resolved storage runtime budget/config
- codec config
- recovery config for storage-mechanical choices
- optional test/fault hooks in test builds

The open plan must not include:

- product access mode
- IPC fallback behavior
- engine primitive registries
- engine subsystem wiring
- StrataHub fleet behavior
- user-facing recovery text
- host hardware facts or product resource-profile classification

Storage open must validate backend capabilities and codec configuration before
creating durable objects.

### Open Outcome

Storage-next should return an L8-shaped `StorageOpenOutcome`.

The outcome should report raw storage facts:

- created vs opened existing
- selected storage mode
- backend capabilities used
- database UUID when durable mode owns one
- codec id
- recovered visible version
- recovered maximum transaction id if retained
- snapshot recovery facts
- WAL replay facts
- segment/table recovery facts
- recovery health
- lossy fallback facts if such mode exists
- maintenance state
- raw warnings

Engine-next decides whether those facts become public diagnostics, open
failures, warnings, or Strata AI explanations.

### Capability Validation

Capability validation is required at the boundary.

Examples:

- cache mode may run without durable sync
- durable local `standard` requires durable publish/sync, a writer guard, and
  background or periodic WAL force behavior
- durable local `always` requires durable publish/sync, a writer guard, and a
  per-commit durability barrier
- object-store durable mode is not V1 and must fail if requested without a
  proven capability contract
- browser/cache mode must not claim crash recovery unless a durable browser
  backend is explicitly designed later

Capability mismatch should fail before WAL, manifest, table, snapshot, or
recovery work begins.

### Commit Batch

Storage-next should expose an internal `CommitBatch` from L7.

The commit unit should support:

- single target branch for V1
- puts
- deletes/tombstones
- expiry metadata, with zero meaning no expiry
- optional write-mode or retention hints if retained
- optional read-set facts
- optional CAS facts
- optional operation origin for diagnostics
- storage-owned commit version assignment
- one storage-owned commit timestamp

The M4P L7-C boundary starts with explicit per-key CAS conditions. An absent
condition maps to L7's explicit missing observed-version state. A present
condition must carry a nonzero commit version. These conditions are not captured
read sets: reads, scans, and history calls performed before a commit are not
remembered unless the caller supplies an explicit storage fact for each key that
must be checked.

M4P-L9B may add explicit storage-shaped read facts for engine-next. That surface
must remain branch-local and storage-key based, and it must map directly to L7
`CommitReadFact` / `CommitCasFact` without exposing product transaction sessions
or primitive DTOs. Product transaction/session policy, operation-level write-skew
claims, and engine-specific condition builders stay above L9.

The commit unit must not contain:

- primitive DTOs
- JSON path operations
- graph edge/node semantics
- vector embedding semantics
- search indexing semantics
- user-facing transaction state
- WAL record bytes
- table object names

Storage-next should not expose public begin/commit/rollback sessions. Engine
may group product operations into a storage commit batch, but users should not
manage storage transactions directly.

V1 commit batches are single-branch. Cross-branch atomic batches are deferred.

### Commit Outcome

Commit should return storage-shaped facts:

- committed version
- commit timestamp
- write count
- delete count
- durability status: non-durable cache, standard accepted, always forced, or
  durability uncertain where a failure window requires it
- conflict outcome if rejected by validation
- write-stall or backpressure facts if applicable
- durable-but-not-visible classification if WAL succeeded but apply failed

Engine-next owns product error mapping and public guarantees.

### Read Latest

Storage-next should expose latest visible read by physical or storage-shaped
key. This is the storage mechanic behind product `get`.

Inputs:

- branch
- key

Result:

- value bytes or storage value
- commit version
- commit timestamp
- tombstone/absence semantics

Engine-next interprets the value.

### Read By Version

Storage-next should expose version-bounded reads by physical or storage-shaped
key. This is the storage mechanic behind product `getv`.

Inputs:

- branch
- key
- maximum visible commit version

Version-bounded reads are native to the storage row ordering: versions of a
logical key are adjacent and sorted newest-first by commit version.

### Read By Timestamp

Storage-next should expose timestamp-bounded reads by physical or
storage-shaped key. This is the storage mechanic behind product `as_of`.

Inputs:

- branch
- key
- maximum visible timestamp

Timestamp-bounded reads are not the same thing as version-bounded reads. The
API should name this directly and should not use `as_of` to mean commit-version
visibility.

Engine-next owns product time-travel commands, branch-from-history behavior,
timestamp resolution rules, and user-facing diagnostics.

Storage-next also owns the generic per-branch commit timeline that resolves
timestamps to retained commit versions. Engine-next owns product commands such
as `as_of`, timeline scrub, and branch-from-time. The physical timeline is
stored as storage-owned system rows under `storage_space_id = 0x01`; L9 exposes
resolution methods and retained-history facts rather than raw timeline rows.

### Scan Physical Key Range / Prefix

Storage-next should expose bounded scans by physical key range or prefix.

Inputs:

- branch
- prefix/range
- visibility bound
- limit
- direction if reverse iteration is retained
- pagination token if pagination is retained

The result should be storage-shaped rows in deterministic order.

### Read History

Storage-next should expose per-key retained version history. This is the
storage mechanic behind product `history`.

Inputs:

- branch
- key
- limit
- before version or timestamp where supported

Storage history is physical value history over the same versioned row chain
used by latest, version-bounded, and timestamp-bounded reads. Engine-next owns
semantic history presentation.

### Unified Versioned Row Model

Storage-next should not maintain separate physical stores for latest, `getv`,
`as_of`, and history.

The target model is one ordered version chain per physical key:

```text
physical key + descending commit version -> stored row
```

Then:

- latest is the first live row in that chain
- `getv` is the first live row with commit version <= requested version
- `history` scans retained rows in that chain
- `as_of` is timestamp-bounded selection over retained rows if storage owns the
  timestamp read

This preserves the current strong design: versioned reads are derived from
ordering, not from side pointers, latest tables, or separate history stores.

### Branch Storage Mechanics

Storage-next should expose generic branch mechanics backed by the
branch-isolated LSM runtime:

- create empty branch storage state
- fork branch storage state using inherited COW layers
- fork branch storage state at an explicit retained commit version
- expose inherited COW state through normal reads without eager row copy
- materialize inherited storage layers when requested or scheduled
- list branch IDs known to storage
- clear/delete branch storage state with appropriate reachability safety
- return raw branch-storage facts

Storage-next must not expose product branch workflows:

- merge
- cherry-pick
- revert
- restore UX
- branch naming UX
- branch comparison presentation
- primitive-aware diff
- graph-aware diff

Engine-next implements product branch behavior by reading and writing
storage-shaped rows through storage mechanics.

### Checkpoint Boundary

Storage-next should expose a checkpoint operation, but the content boundary
must remain primitive-neutral.

Storage owns:

- rejecting checkpoint while storage is closing
- commit quiesce through L7
- checkpoint watermark selection
- row-native committed storage-state collection
- snapshot object publication through L4
- database manifest snapshot watermark update
- WAL truncation eligibility after checkpoint
- snapshot retention/pruning trigger
- raw checkpoint outcome

Engine-next owns:

- optional derived-state checkpoint sections
- optional derived-state install or rebuild policy during recovery
- public checkpoint diagnostics

The committed recovery payload is row-native storage state. Engine may provide
opaque derived sections for search/vector/graph indexes or other rebuildable
state, but those sections must not be required to recover committed rows. Any
engine-supplied checkpoint payload must be storage-shaped: opaque sections,
storage rows, or decoded storage entries. It must not be graph/vector/JSON/
event/search DTOs.

### Recovery Boundary

Storage-next should expose recovery facts primarily through open outcome and
health APIs.

Recovery facts should include:

- clean open
- recovered from WAL
- recovered from snapshot/checkpoint
- ignored, quarantined, or retained orphan objects
- degraded recovery
- lossy recovery if such mode exists
- fatal corruption
- recovered version/transaction allocator facts

Storage classifies recovery health. Engine-next decides whether a degraded
outcome is accepted, rejected, or rendered as a diagnostic.

### Maintenance Boundary

Storage-next should expose storage maintenance controls and facts without
making maintenance a normal user workflow.

Expected controls:

- drain maintenance
- close storage
- checkpoint now
- run storage maintenance now for tests/diagnostics
- get maintenance status
- get health/metrics

Expected automatic internals:

- flush frozen mutable tables
- update flush watermark
- truncate covered WAL objects
- compact branch table levels
- materialize inherited layers
- prune snapshots
- quarantine unreferenced table objects
- purge safe quarantine inventory
- clean temporary objects

Engine-next may trigger lifecycle hooks, but product users should not need to
run flush, compact, checkpoint, prune, or repair during normal use.

### Shutdown / Close

Storage-next should expose a safe close operation.

Close may:

- stop accepting new storage commits
- drain storage maintenance work
- wait for commit quiescence or return a typed timeout
- stop storage writer/background sync loops
- flush required durable WAL state
- publish required storage manifests
- release writer guard or backend lease
- return raw close facts

Engine-next wraps close with product handle shutdown, IPC shutdown, primitive
freeze hooks, registry release, and public error mapping.

Close should be idempotent after successful close and retryable after timeout
or hook-independent storage failure where safe.

### Health / Metrics

Storage-next should expose raw facts such as:

- backend capability facts
- selected storage mode
- object counts by family
- WAL object facts
- snapshot facts
- table counts and bytes by branch/level
- cache stats if retained
- frozen mutable-table counts
- pending flush/compaction/materialization facts
- recovery health
- quarantine facts
- retention debt
- writer/sync health
- approximate memory use
- resolved storage runtime budget facts

Engine-next decides which facts become public diagnostics, CLI output, Strata
AI explanations, or future StrataHub telemetry.

### Fault / Test Hooks

Storage-next should provide test-only or feature-gated hooks for:

- backend failures
- publish failures
- WAL failures
- manifest failures
- snapshot failures
- table corruption
- recovery interruption
- maintenance scheduling failure
- close timeout
- crash simulation

These hooks are not product APIs.

## Do Not Expose

Storage-next should not expose these details through the normal engine
boundary:

- provider-specific handles after open
- raw backend object IO
- local filesystem paths
- object-name constructors
- object names except where diagnostics explicitly need them
- WAL record structs as engine-facing API
- WAL writer/reader internals
- manifest mutation internals
- snapshot file internals
- table file readers/builders
- block cache mutation APIs
- bloom/index internals
- branch LSM mutable-table internals
- compaction algorithm internals
- quarantine publish internals
- primitive snapshot DTOs as storage-owned concepts
- public transaction sessions
- follower refresh
- IPC server/client behavior
- StrataHub behavior

Some of these may exist inside storage-next or in test/diagnostic tooling. They
should not be normal engine-next production dependencies.

## Failure Model

The boundary should expose storage-local error categories without product text.

Expected categories:

- invalid storage config
- unsupported storage mode
- capability mismatch
- backend unavailable
- permission denied
- writer lock or lease conflict
- object not found
- object already exists
- publish precondition failed
- publish failed before visibility
- publish visibility unknown
- publish visible but durability unconfirmed
- codec init failed
- codec mismatch
- corrupt storage metadata
- corrupt WAL
- WAL partial tail
- corrupt table
- corrupt snapshot
- recovery failed
- recovery degraded
- lossy recovery used
- commit conflict
- branch storage conflict
- branch deleting
- durable-but-not-visible commit
- write stall timeout
- maintenance failed
- retention proof incomplete
- reclaim blocked by degraded recovery
- close timeout
- internal invariant violation

Engine-next maps these into public errors and diagnostics.

## Testing Requirements

The L9 boundary needs contract tests:

1. Engine-facing open cannot bypass capability validation.
2. Unsupported durable object-store mode fails before side effects.
3. Cache mode does not claim crash durability.
4. Durable local mode reports durable capability facts.
5. Commit batch is atomic from the reader's perspective.
6. Failed commit does not become visible.
7. WAL append failure leaves no visible rows.
8. WAL append plus apply failure returns durable-but-not-visible.
9. Latest and version-bounded reads agree on visibility.
10. Timestamp reads either work natively or are explicitly unsupported.
11. History returns storage versions in deterministic order.
12. Prefix/range scans obey visibility bounds.
13. Branch fork exposes inherited state without copying all rows immediately.
14. Branch materialization preserves reads.
15. Branch delete preserves tables still inherited elsewhere.
16. Checkpoint uses primitive-neutral payloads only.
17. Recovery outcomes are raw storage facts and do not contain product
    semantics.
18. Maintenance controls expose facts without requiring user maintenance
    workflows.
19. Shutdown drains maintenance and releases writer guard/lease.
20. Fault hooks are unavailable or inert in normal production builds.
21. Upper crates above engine-next cannot import storage-next in normal
    production code.

## Current Code Evidence

Current evidence lives in:

- `crates/storage/src/traits.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/txn/`
- `crates/storage/src/durability/`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/recovery.rs`
- `crates/engine/src/database/compaction.rs`
- `crates/engine/src/database/lifecycle.rs`
- `crates/engine/src/database/transaction.rs`
- `docs/storage/v1-storage-consumption-contract.md`

The current consumption contract is intentionally large because it documents
the post-consolidation engine/storage boundary. Storage-next should use it as
evidence, not as a surface to preserve.

## V1 Minimum

The first storage-next API boundary needs:

1. Open/create storage runtime from storage plan.
2. Capability validation before durable side effects.
3. Cache mode and local filesystem durable mode.
4. Raw open outcome with recovery, capability, and recovered visible-version
   facts.
5. Internal single-branch `CommitBatch`.
6. Commit outcome with version, timestamp, durability, and conflict/stall
   facts.
7. Read latest.
8. Read by version.
9. Prefix/range scan by visibility bound.
10. Per-key history.
11. Timestamp-bounded read over storage commit timestamps.
12. Basic branch create/fork/fork-at-retained-version/materialize/delete
    mechanics.
13. Primitive-neutral checkpoint boundary.
14. Raw recovery outcome and recovery health.
15. Maintenance drain/status/control hooks for engine/tests.
16. Safe close/shutdown.
17. Raw health and metrics sufficient for tests and engine diagnostics.
18. Test/fault hook boundary behind test or feature gates.

The first boundary does not need:

1. Public transaction sessions.
2. User-facing maintenance commands.
3. Production object-store durable mode.
4. IPC.
5. Primitive DTOs.
6. Follower mode.
7. StrataHub hooks.
8. Full final engine-next diagnostic taxonomy.
9. Cross-branch atomic commit batches.
10. Distributed locks or consensus.

## Open Questions

These should be resolved before implementation planning:

1. Does storage-next expose physical keys as a typed `StorageKey`, opaque byte
   slices, or both behind constructors?
2. What exact storage-shaped row representation does `CommitBatch` carry:
   opaque bytes, current `Value`, or an L3 row envelope?
3. Are maintenance controls exposed as direct methods, a task executor, or a
   typed control plane over L8?
4. Which raw health facts are stable enough to guarantee to engine-next in V1?
5. Does lossy WAL fallback remain a normal open option, or move to diagnostic
   recovery tooling?
6. How strict should the boundary be about exposing object names in diagnostics
   versus opaque object roles?

## Final Alignment Rule

If an engine-next feature needs a storage behavior not listed here, the default
answer is not "reach into a lower storage module." The default answer is:

1. identify the lower-layer mechanic,
2. decide whether it is genuinely storage-owned,
3. add the smallest storage-shaped boundary to L9,
4. keep product meaning in engine-next.
