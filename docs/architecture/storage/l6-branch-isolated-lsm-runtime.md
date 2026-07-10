# L6. Branch-Isolated LSM Runtime

Status: V1 architecture draft

Depends on:

- [L5. Table Runtime](./l5-table-runtime.md)
- [L4. Log / Manifest / Snapshot Services](./l4-log-manifest-snapshot-services.md)

Consumed by:

- L7. Commit Runtime
- L8. Lifecycle / Recovery / Maintenance
- L9. Storage API Boundary

## Purpose

L6 defines Strata's storage identity: a branch-isolated MVCC LSM forest.

L5 provides table primitives. L6 assembles those primitives into branch-local
mutable state, branch-local immutable levels, versioned row visibility, and
copy-on-write inherited layers.

This is the layer that makes Strata unusual. Branching is not implemented as an
engine-only feature above a flat key-value store. Branch identity, versioned
visibility, inherited immutable table layers, fork-version gates, and lazy
materialization are storage mechanics.

## Core Decision

Storage-next should preserve the current branch-aware LSM architecture, but
make its ownership explicit.

The target shape is:

```text
BranchLsm
  branches: BranchId -> BranchState

BranchState
  active mutable table
  frozen mutable tables
  own immutable table levels
    L0: overlapping, newest first
    L1-LN: non-overlapping, sorted by key range
  inherited layers
    source branch id
    fork version
    immutable table levels from source snapshot
    materialization status
```

Reads search child-local state first, then inherited layers nearest ancestor
first. Inherited layer keys are rewritten into the child branch namespace so the
same MVCC selection logic can reason over child-local and inherited rows.

Storage owns that mechanism. Engine owns the product meaning of branch
operations such as merge, cherry-pick, revert, restore, compare, and publish.

## Responsibilities

L6 owns:

- branch IDs as storage-local physical isolation boundaries
- branch-local active mutable table ownership
- branch-local frozen table ownership
- branch-local immutable table level ownership
- branch-aware row-key construction
- versioned row-chain visibility
- latest reads
- version-bounded reads for product `getv`
- timestamp-bounded reads for product `as_of` over storage commit timestamps
- per-key retained history reads
- prefix/range scans over versioned rows
- child-local shadowing of inherited rows
- copy-on-write inherited layers
- fork-version visibility gates
- inherited key rewriting
- inherited layer materialization
- branch-safe tombstone behavior
- branch-safe TTL visibility and retention facts
- branch/table reachability facts
- shared immutable table reference facts
- snapshot-row install into branch storage state
- storage facts needed by L8 recovery, retention, quarantine, and repair

L6 does not own:

- engine data capability semantics
- JSON path behavior
- event-chain product meaning
- vector collection behavior
- embedding generation policy
- graph ontology, traversal, analytics, or relationship semantics
- search ranking or retrieval recipes
- product branch workflows
- merge, cherry-pick, revert, restore, or branch comparison semantics
- public branch naming UX
- commit validation
- WAL-before-visible discipline
- version allocation and commit ordering
- checkpoint scheduling
- recovery orchestration
- durable publication mechanics
- table byte format
- backend IO
- IPC
- StrataHub behavior

## Layer Boundary

L6 sits between table mechanics and commit/lifecycle orchestration.

```text
L7 Commit Runtime
  commits batches into branch LSM state
        |
        v
L6 Branch-Isolated LSM Runtime
  owns branch-local versioned row visibility
        |
        v
L5 Table Runtime
  builds, reads, merges, and compacts tables
        |
        v
L4 Durable Services
  publishes table objects and reachability manifests
```

L6 may ask L5 to build, read, merge, and compact tables. L6 may ask L4 to
publish branch/table reachability manifests. L6 should not know how L4 performs
durable publication, and L6 should not write files or backend objects directly.

L7 decides whether a commit is valid and when it becomes visible. L6 provides
the branch-local structures into which committed rows are installed.

L8 coordinates recovery, checkpoint, compaction scheduling, retention, and
quarantine. L6 provides the raw branch/table facts and safe mutation operations
that L8 needs.

Version ownership rule:

> L7 owns commit-version allocation and commit ordering. L6 records applied
> versions, maintains branch-local max-version facts, and enforces version
> visibility during reads.

## Current Code Reference Map

The current L6 implementation is mostly inside `SegmentedStore`. It is powerful
but mixed with L5, L7, and L8 responsibilities.

### Core L6 Evidence

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/stored_value.rs`

Current roles:

- `SegmentVersion`: branch-local immutable table levels.
- `BranchState`: branch-local active table, frozen tables, immutable table
  levels, compact pointers, level targets, inherited layers, and counters.
- `BranchSnapshot`: pinned read view over active, frozen, own table levels, and
  inherited layers.
- `InheritedLayer`: copy-on-write parent table snapshot with source branch,
  fork version, and materialization status.
- `key_encoding.rs`: physical ordering for branch, space, type, user key, and
  descending commit version.
- `merge_iter.rs`: sorted merge plus MVCC and inherited-layer rewriting.
- `seekable.rs`: seekable iterator stack plus inherited-layer rewriting.
- `stored_value.rs`: row value metadata used by version visibility.

### COW And Materialization Evidence

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage/src/manifest.rs`

Current roles:

- `fork_branch`: creates child inherited layers instead of copying all rows.
- `materialize_layer`: rewrites inherited rows into child-owned immutable
  tables and removes the inherited layer.
- `collect_unshadowed_entries`: determines which inherited rows are still
  visible after child-local shadowing.
- `SegmentRefRegistry`: protects shared immutable tables while inherited
  layers reference them.
- `segments.manifest`: persists branch-owned tables and inherited layers.

### Mixed L6/L7/L8 Evidence

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/recovery.rs`
- `crates/storage/src/segmented/quarantine_protocol.rs`
- `crates/storage/src/durability/decoded_snapshot_install.rs`

Current roles:

- `apply_writes_atomic`: currently installs committed rows into branch state;
  this is L6 mutation called by L7 commit runtime.
- `flush_oldest_frozen`: currently builds and installs branch-local L0 tables;
  the table build is L5, branch installation is L6, publish mechanics are L4,
  and scheduling is L8.
- `segmented/compaction.rs`: table algorithms, branch level selection, branch
  state mutation, and maintenance policy are currently mixed.
- `recover_segments`: rebuilds branch table state and inherited layers during
  recovery; orchestration is L8, branch state reconstruction is L6.
- `decoded_snapshot_install.rs`: installs generic decoded rows into storage
  branch state; the generic row install shape belongs at the L6 boundary.

Storage-next should preserve the mechanics but split ownership cleanly.

## Branch State Model

The branch is the unit of storage isolation.

A target `BranchState` should contain:

- branch id
- active mutable table
- frozen mutable tables, newest first
- immutable table levels owned by the branch
- inherited COW layers, nearest ancestor first
- branch-local max committed version
- branch-local min/max timestamp facts if timestamp reads are storage-native
- branch-local table reachability facts
- branch-local compaction cursor/facts if retained
- branch-local approximate entry/deletion counters

It should not contain:

- product branch name UX
- branch DAG semantics
- merge policy
- graph-aware branch facts
- IPC state
- backend paths
- public diagnostics text

Branch names, branch DAG, status, review workflows, merge policies, and
branch-product commands belong in engine.

## Pinned Read Views

L6 must provide pinned branch read views.

A read view captures:

- active mutable table reference
- frozen mutable table references
- branch-owned immutable table levels
- inherited layer list
- materialization status for inherited layers
- visibility bound supplied by the caller

The read view must remain valid while concurrent branch mutations happen:

- active table rotation
- frozen table flush
- branch-level compaction
- inherited layer materialization
- fork
- branch clear/delete
- recovery-time state rebuild

Readers must never see a partial branch state. A reader may see either the old
view or the new view, but not a mix where a frozen table has disappeared before
its immutable table replacement is visible, or where an inherited layer is
removed before its materialized tables are visible.

The current `BranchSnapshot`/`SegmentVersion` shape is the right evidence:
immutable references and atomic branch-state publication are the target pattern.

## Immutable Level Model

L6 owns branch-local table levels.

Current shape:

```text
L0: overlapping tables, newest first
L1-L6: non-overlapping tables, sorted by key range
```

Storage-next can keep that shape unless the table-format spec or benchmark data
proves a better one. The important boundary is ownership:

- L5 can compact table inputs into table outputs.
- L6 owns which tables are in which branch level.
- L6 owns installing new tables into branch state.
- L6 owns preserving branch read semantics while state changes.
- L8 owns when maintenance work should run.

## Flush And Table Install

L6 owns branch-local flush state transitions.

Target active-to-immutable flow:

```text
active mutable table
  -> frozen mutable table
  -> L5 builds immutable table artifact
  -> L4 publishes table object
  -> L6 installs table into branch L0
  -> L4 publishes branch/table reachability manifest
  -> frozen table can leave the read path
```

Required invariants:

1. The frozen table remains visible until its replacement immutable table is
   installed into branch state.
2. If table build fails, branch state remains unchanged.
3. If table publication fails before visibility, branch state remains
   unchanged.
4. If table publication succeeds but branch-state publication fails, L6 must
   either roll back the in-memory install or report a typed visible-but-not-
   durable state that L8 can reconcile.
5. A branch clear/delete racing with flush must not resurrect a deleted branch.
6. Flush output becomes branch-owned table reachability only through an L6 state
   transition and an L4-published reachability record.

L8 may decide when to flush. L5 builds the table. L4 publishes the table object
and reachability record. L6 owns the branch-local install/removal semantics.

## Row Key Model

L6 owns the meaning of physical row keys.

The current ordered key shape is:

```text
branch id | space | type tag | user key | descending commit version
```

This is a strong design because latest, `getv`, history, and scans all operate
over one ordered row chain rather than separate latest/history stores.

Target rules:

1. Branch identity remains part of the physical storage key.
2. Versions of one logical row remain adjacent.
3. Newer commit versions sort before older commit versions for the same row.
4. Prefix/range scans can be implemented over ordered row-key bytes.
5. L5 treats the key as ordered bytes; L6 owns what the bytes mean.
6. Engine owns primitive-specific interpretation of spaces, storage space ids,
   and values.

The current `TypeTag` byte is a storage-family routing fact today. In
storage-next it becomes an opaque engine-supplied `storage_space_id`. Storage
may route, sort, and scan by that byte. It must not know whether the byte means
KV, JSON, events, graph, vectors, search, system rows, or a future engine
capability.

## Versioned Row Model

L6 should keep one versioned row chain per physical row key.

```text
row key + descending commit version -> row value / tombstone / metadata
```

From that one chain:

- latest read selects the newest visible live row
- `getv` selects the newest live row with commit version <= requested version
- `history` scans retained rows in descending commit-version order
- `as_of` selects by storage commit timestamp
- tombstones participate in the same row chain
- TTL metadata participates in the same row chain

This design should not be replaced by separate latest tables, separate history
stores, or pointer chains unless a later design proves a major benefit. The
current adjacency model is simple, efficient, and testable.

## Read Order

A branch read should search sources in semantic shadowing order:

```text
active mutable table
  -> frozen mutable tables, newest first
  -> branch-owned L0 tables, newest first
  -> branch-owned L1+ tables
  -> inherited layers, nearest ancestor first
  -> MVCC / timestamp / tombstone / TTL visibility
```

Point reads can short-circuit when a visible row or visible tombstone is found.
Scans should merge child-local and inherited sources so logical keys appear in
branch namespace order.

The storage invariant is:

> Child-local data shadows inherited data without copying inherited rows.

This invariant is what makes cheap branch operations possible.

Inherited visibility rule:

```text
effective inherited visibility = min(requested version, layer fork version)
```

Point reads may short-circuit only after version, tombstone, TTL, and inherited
fork-version visibility have been evaluated. Scan reads must rewrite inherited
keys into the child branch namespace before MVCC grouping, so own and inherited
versions of the same logical row are compared as one row chain.

## Inherited Layers

An inherited layer is a physical COW reference to immutable table state from
another branch.

It should contain:

- source branch id
- fork version
- immutable table levels from the source snapshot
- layer status
- durable reachability facts sufficient for recovery

Inherited layers are ordered nearest ancestor first. This gives chained forks a
deterministic shadowing order:

```text
child own rows
  -> direct parent layer
  -> grandparent layer
  -> older ancestor layers
```

Each inherited layer is visible only up to its fork version. Rows committed to
the source branch after the fork must not appear in the child.

## Key Rewriting

Inherited rows physically belong to the source branch. Reads in the child must
present them in the child branch namespace.

L6 therefore owns key rewriting:

```text
source branch id in inherited row key -> child branch id
```

This is needed so merged own and inherited rows group under the same logical
row key. It is also needed for scans to produce branch-local result keys.

Key rewriting must be mechanical. It must not reinterpret engine primitive
payloads.

## Fork

Storage-level fork creates a new branch state from an existing branch state.

The target behavior:

1. Ensure the source branch's visible mutable state is represented in immutable
   or otherwise inherited-safe state.
2. Capture a source table snapshot.
3. Capture the source branch's max applied commit version as fork version.
4. Create destination branch state with empty own tables and inherited layers.
5. Preserve source inherited layers in destination ancestry order.
6. Publish branch/table reachability through L4.
7. Protect shared immutable tables from deletion.

Fork cost should be proportional to metadata and inherited table references,
not dataset size.

Fork publication invariants:

1. The destination branch must not become visible until its inherited-layer
   reachability is durable enough for the requested storage mode.
2. Shared immutable tables must be protected before any concurrent cleanup can
   observe them as unreferenced.
3. If destination reachability publication fails before visibility, the fork
   must leave no visible destination branch.
4. If publication is visible but durability is unconfirmed, L6 must surface raw
   facts that L8 can classify during recovery.
5. Fork must capture the source branch's applied max version, not a global
   allocated-but-unapplied version.

Storage fork does not mean product fork UX. Engine decides command names,
branch metadata, branch DAG, audit events, and user-facing explanations.

## Fork At History

Product docs identify branch-from-history as a V1 required capability.

L6 is the natural storage layer for the mechanism:

```text
create destination branch inheriting source rows visible at commit version V
```

This is a generalization of current fork-version gates. The storage challenge
is proving that enough retained history exists for the requested fork version
before making a visible destination branch.

V1 must expose the storage mechanism through L9 as a generic fork-at-retained-
commit-version operation. Storage still does not know the product meaning of
"branch from history"; it only proves retained-history availability, creates the
destination branch state with the requested fork-version frontier, protects
shared table reachability, and reports raw storage facts.

The row-chain model and inherited-layer fork version are already the right
foundation.

## Materialization

Materialization converts inherited state into child-owned immutable tables.

Target behavior:

1. Choose a child branch and inherited layer.
2. Freeze/flush child-local mutable state as needed so shadowing is complete.
3. Mark materialization intent durably.
4. Collect inherited rows still visible after child-local and closer-layer
   shadowing.
5. Rewrite row keys into child branch namespace.
6. Build child-owned immutable tables through L5.
7. Publish table objects through L4.
8. Install new tables into child branch levels.
9. Remove the inherited layer from child branch state.
10. Publish updated branch/table reachability through L4.
11. Release shared table references no longer needed.

Materialization must preserve visible results. It changes physical ownership,
not branch meaning.

Crash safety belongs across L4/L6/L8:

- L6 defines materialization state and idempotent state transitions.
- L4 publishes objects and manifests durably.
- L8 resets or completes interrupted work during recovery.

Materialization publication invariants:

1. The inherited layer remains visible until replacement child-owned tables are
   installed.
2. Replacement tables must be built from a view that includes child-local
   shadowing and closer inherited layers.
3. Removing the inherited layer and installing replacement tables must be a
   single branch-state transition from the reader's perspective.
4. Shared table references from the removed layer may be released only after
   the new branch reachability is published or the operation is safely rolled
   back.
5. Recovery must be able to distinguish "materialization intended,"
   "replacement tables published," and "branch reachability updated."

## Tombstones And TTL

Branch inheritance makes tombstone and TTL rules harder than a flat LSM.

Rules:

1. A tombstone is a row version.
2. A child-branch tombstone can hide an inherited parent value.
3. Compaction must not drop a tombstone while any lower or inherited row could
   be resurrected by dropping it.
4. TTL expiry must be evaluated consistently across own and inherited rows.
5. L5 may execute pruning, but L6/L8 must supply the safety policy.

This is a storage correctness issue, not product semantics. L6 must expose the
facts required for safe pruning and materialization.

Example:

1. Parent branch writes key `k` at version 10 with TTL expiring at time 50.
2. Child branch forks at version 20.
3. At time 40, child reads inherited `k` as live unless child has shadowed it.
4. At time 60, child reads `k` as expired unless child has a newer own row.
5. Compaction may not drop the parent row or a child tombstone unless L6 can
   prove no retained child view, inherited layer, snapshot floor, or timestamp
   query can observe a different result.

This example should become a conformance test because TTL, inheritance,
compaction, and retention interact in one place.

## Timestamp Reads

L6 owns timestamp-bounded visibility over storage commit timestamps. It must
define:

1. Whether commit timestamps are monotonic with commit versions per branch.
2. How timestamp selection consumes the storage-owned commit timeline substrate
   in `docs/architecture/storage/commit-timeline-substrate.md`.
3. How tombstones and TTL are evaluated at the requested timestamp.
4. How inherited layers apply both timestamp visibility and fork-version
   visibility.
5. What error is returned when retained history is insufficient to answer the
   timestamp request.

These rules are part of the V1 storage substrate because branch-from-time and
timeline scrub need a reliable timestamp-to-version mapping. Engine owns the
product command and explanation. Storage owns the generic timestamp visibility
mechanics and retained-history failure facts.

## Branch Compaction

L6 owns branch-level compaction state changes.

L5 can compact table inputs into table outputs. L8 can decide that maintenance
should run. L6 must own the branch-specific mechanics:

- selecting candidate tables from branch-owned levels
- supplying inherited/shared reachability constraints to retention policy
- preserving child/inherited tombstone safety
- deciding where output tables install in the branch's level set
- publishing the new branch/table reachability through L4
- removing old branch-owned tables only after replacement reachability is safe
- preserving shared inherited tables until no branch/layer references them
- updating branch-local level facts and compaction cursors

Compaction must preserve pinned read views. Readers may continue using old
table references while the branch publishes a new level view.

## Shared Table Reachability

COW branches share immutable tables. Storage-next needs explicit reachability
facts so shared tables are not deleted early.

L6 owns:

- which branch owns a table
- which inherited layer references a table
- which materialization state still requires a table
- which tables are reachable from branch manifests
- which tables may be released when a branch/layer is removed

L8 may rebuild, validate, and repair these facts during recovery. L4 publishes
the durable manifests. L5 reads table objects but does not decide reachability.

The current `SegmentRefRegistry` is a runtime accelerator. Storage-next should
keep the distinction:

- durable branch/table manifests are the source of truth
- runtime refcount registries are rebuildable acceleration structures

## Snapshot Row Install

Snapshot install should target generic storage rows.

L6 owns installing decoded rows into branch storage state:

- validate row key ordering and branch target
- group rows into branch-local table build plans
- build immutable tables through L5
- stage branch-owned table refs and expose publication facts to L8
- install staged tables into branch-local state
- return generic install facts

L6 does not publish table objects or branch manifests itself. L8 composes L6
snapshot row install with L4 publication, manifest updates, and crash-window
reconciliation.

Snapshot install must be all-or-nothing at the branch-state boundary. L6 should
preflight the full decoded-row install plan before publishing any branch-state
mutation:

- validate every target branch
- validate row-key ordering and branch ownership
- validate version and timestamp metadata
- reject duplicate rows within the same install plan
- build all required table artifacts or stage them before branch visibility
- install only after all validation and required staging succeeds

If any row or table artifact fails validation, no partial branch install should
be visible.

L6 must not know primitive snapshot DTOs. Engine may decode primitive-shaped
sections if the format still uses them. Storage should receive generic rows or
opaque storage-family rows.

## Branch Deletion And Clearing

Clearing a branch is a storage mutation over branch-local state and inherited
references.

L6 owns:

- removing branch-local active/frozen state
- removing branch-owned table reachability
- releasing inherited layer references
- preserving shared tables still referenced elsewhere
- producing raw deletion/release facts

Engine owns:

- whether a branch may be deleted
- branch DAG policy
- user-facing branch state
- audit and product diagnostics

L8 owns:

- cleanup scheduling
- orphan detection
- quarantine or repair if deletion is interrupted

## Interaction With L7 Commit Runtime

L7 commits into L6.

L7 owns:

- commit batch validation
- version allocation if storage-local
- branch commit locks
- WAL-before-visible discipline
- conflict handling
- commit ordering

L6 provides:

- append committed rows into branch active table
- insert tombstones
- update branch-local max version/timestamp facts
- expose visible reads after commit publication
- rotate or expose mutable-table pressure facts to L8

L6 should not expose public transactions. It is a storage runtime used by the
internal commit layer.

## Interaction With L8 Lifecycle And Maintenance

L8 orchestrates lifecycle work using L6 facts and mutation operations.

L8 owns:

- open sequencing
- raw recovery execution
- WAL replay
- checkpoint orchestration
- compaction scheduling
- retention scheduling
- quarantine and repair orchestration
- shutdown sequencing
- health/metrics aggregation

L6 provides:

- recoverable branch state model
- branch table reachability manifests/payloads
- safe table install/remove operations
- branch compaction candidate facts
- branch materialization operations
- branch-safe retention facts
- raw metrics on branch/table state

L8 should not bypass L6 to mutate branch table state directly.

## Failure Model

L6 failures should be storage-local and branch/runtime oriented:

- branch not found
- branch already exists
- invalid branch id
- invalid row key for branch
- commit version out of order for branch
- inherited layer not found
- inherited layer corrupt
- fork source unavailable
- fork version unavailable due to retention
- fork reachability publication failed before visibility
- flush table build or publication failed before branch install
- flush table publication visible but durability unconfirmed
- materialization already in progress
- materialization state conflict
- materialization recovery state conflict
- inherited key rewrite failure
- branch table reachability conflict
- branch compaction install conflict
- shared table still referenced
- timestamp history unavailable
- unsafe tombstone pruning requested
- unsafe TTL pruning requested
- snapshot row install validation failure
- snapshot row install duplicate row
- branch state manifest publish failed through L4
- branch state recovery conflict

L6 should preserve source errors from L4/L5 where useful, but it should not
convert branch invariants into silent defaults.

## Testing Requirements

L6 needs direct tests that do not require engine primitives.

Required test families:

1. Branch creation and empty branch reads.
2. Branch-local latest reads.
3. Version-bounded `getv` reads.
4. Timestamp-bounded reads over storage commit timestamps.
5. Per-key history order and tombstone representation.
6. Prefix/range scans over own branch state.
7. Fork creates inherited state without row copy.
8. Child-local writes shadow inherited values.
9. Child-local tombstones shadow inherited values.
10. Parent writes after fork are invisible to child.
11. Chained fork ancestry order is deterministic.
12. Inherited key rewriting preserves scan order.
13. Materialization preserves all visible reads.
14. Materialization is idempotent across crash/recovery windows.
15. Branch deletion preserves tables still inherited elsewhere.
16. Shared table reachability/refcount rebuild from manifests.
17. Compaction safety for inherited tombstones.
18. TTL behavior across own and inherited rows.
19. Snapshot row install into branch state.
20. Fault tests for branch manifest publish failure.
21. Property tests for latest/getv/history consistency over one row chain.
22. Fuzz tests for branch key decoding/rewriting.
23. Concurrency tests for fork vs writes, materialization vs writes, and clear
    branch vs flush/compaction.
24. Pinned read views during flush, compaction, materialization, and clear.
25. Flush publish failure and visible-but-not-durable reconciliation.
26. Fork publish failure before destination visibility.
27. Branch compaction output install and shared-table reachability safety.
28. Snapshot row install all-or-nothing preflight failures.

These tests should use synthetic storage rows. They should not need JSON,
graph, vector, search, event, or engine branch workflow semantics.

## V1 Minimum

The first storage-next L6 implementation needs:

1. Branch-local active/frozen table ownership.
2. Branch-local immutable table level ownership.
3. Physical row keys with branch and version ordering.
4. Latest reads.
5. Version-bounded reads.
6. Per-key history reads.
7. Prefix/range scans by version visibility.
8. Timestamp-bounded reads if storage owns `as_of`.
9. COW inherited layers.
10. Fork-version gates.
11. Inherited key rewriting.
12. Materialization.
13. Branch-safe tombstone behavior.
14. Shared table reachability facts.
15. Snapshot row install into generic rows.
16. Raw branch/table metrics for L8.
17. Pinned read views.
18. Branch-local flush and table install protocol.
19. Branch-level compaction state transitions.

It does not need:

1. Product merge/cherry-pick/revert semantics.
2. Graph-aware or primitive-aware diff.
3. Public branch command UX.
4. Public transaction sessions.
5. IPC.
6. StrataHub dataset/fleet behavior.
7. Object-store-specific branch behavior.

## Open Questions

1. Does storage-next keep the current physical key shape exactly, or introduce a
   new row-key envelope in the storage format spec?
2. What exact commit-version allocation API should L7 expose to L6 mutation
   paths?
3. How much branch/table reachability should be persisted per branch manifest
   versus reconstructed from table metadata?
4. Should materialization always produce L0 tables, or may it target lower
   levels when safe?
5. What is the maximum inherited-layer depth before automatic materialization
   should run?
6. Should fork-at-history be part of V1 storage API or reserved behind an
   internal capability for post-V1?
9. What exact facts must L6 expose so L8 can safely schedule retention and
   compaction without learning engine semantics?
10. How should branch state metrics be represented without creating another
    set of one-off structs?

## Next Step

After L6 and L7, the next storage-next document should define L8 Lifecycle /
Recovery / Maintenance. That document should explain how storage opens,
recovers, checkpoints, schedules compaction and retention, repairs or
quarantines damaged state, and shuts down while consuming L4-L7 through their
explicit contracts.
