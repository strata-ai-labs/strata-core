# Strata V1 Versioning And Time Travel

Status: Draft product and architecture direction

This document defines the V1 direction for Strata's versioning and time-travel
capabilities. It is based on the current executor, engine, storage, search,
graph, vector, and branching implementations, but it is not limited to the
current command surface.

The product goal is simple: users should be able to understand, inspect,
compare, restore, and branch database state across time without learning storage
internals.

## Thesis

Versioning and time travel are core Strata capabilities. They are not debug
tools and they should not be treated as incidental metadata.

The V1 product should support these user outcomes:

1. Read current data.
2. Read data as it existed at a point in time.
3. Inspect how a record changed.
4. Compare database state across branches and points in time.
5. Undo a bad range by writing a compensating change.
6. Create a new branch from an earlier point in a branch's history.
7. Run retrieval and generation against a historical view where the supporting
   indexes can provide correct results.

The strongest version of this product is a timeline experience: scrub to a
point, inspect the database as it existed there, search it, compare it, and
create a new branch from that point.

That feature is feasible, but only if Strata makes commit versions, timestamps,
retention, and derived indexes explicit contracts.

## Non-Negotiables

1. Commit versions are the authoritative internal ordering.
   Storage snapshot isolation, branch points, merge bases, revert ranges, and
   recovery should be based on monotonically assigned commit versions.

2. Timestamps are user-facing selectors, not the only source of truth.
   Users think in times, but storage correctness should resolve time to a
   commit version before performing operations that need a consistent database
   frontier.

3. Branch-from-time must not guess.
   Creating a branch from `2026-05-09T12:00:00Z` must resolve to a specific,
   retained commit point. If Strata cannot prove that point, it must fail
   clearly.

4. Historical reads must explain retained history.
   If retention or compaction has removed the requested history, the user should
   get a history-unavailable error, not a silent empty result that looks like
   the record never existed.

5. Derived state must be part of the temporal contract.
   Search indexes, auto embeddings, vector indexes, graph relationship indexes,
   and recipe outputs cannot pretend to be timeless. They must either support
   the requested point or clearly fall back, rebuild, or refuse.

6. Storage remains primitive-agnostic.
   Storage may own commit versions, timestamps, branch identifiers, retained
   rows, and generic indexes needed for time resolution. It should not learn KV,
   JSON, event, graph, vector, search, or recipe semantics.

7. Product APIs should expose concepts, not maintenance mechanics.
   Users should ask for a branch, version, timestamp, history, comparison, or
   restore. They should not manually drive checkpoint, compact, flush, or
   low-level retention behavior as part of ordinary time travel.

## Product Vocabulary

### Version

A version is a committed point in database history. It is the stable ordering
unit used by storage and branch operations.

V1 should treat versions as observable product facts. A user may see versions in
history, branch metadata, comparison output, restore output, and diagnostics.

### Timestamp

A timestamp is a wall-clock time attached to a commit or record version. Users
should be able to select historical state by timestamp because that is the
natural product interaction.

Timestamps are not sufficient by themselves for every operation. A timestamp may
need to be resolved to the latest retained commit version at or before that time.

### Temporal Point

A temporal point identifies the state the user wants to read or branch from.

V1 should converge on a model like:

```text
current
version:<commit-version>
time:<timestamp>
```

The exact syntax can vary by SDK, CLI, or command protocol. The concept should
be shared.

### Temporal View

A temporal view is a read-only view of a branch at a temporal point. It should
behave like normal reads against that branch, except the visible data is bounded
by the selected point.

V1 does not need a long-lived temporal transaction object if the command surface
can remain simple, but the architecture should have an internal temporal context
that all data capabilities can use consistently.

### Commit Timeline

A commit timeline is the per-branch mapping from commit version to commit
timestamp and related generic commit metadata.

This is the missing contract for branch-from-timestamp. Without it, Strata can
answer many point reads by scanning row timestamps, but it cannot cheaply and
reliably turn a user time into the single commit frontier needed for a COW
branch point.

## What Strata Supports Today

The current implementation already has substantial support.

### Storage Substrate

Storage supports:

1. Commit-version MVCC.
2. Version-bounded point reads.
3. Version-bounded prefix scans.
4. Per-key history reads, newest first.
5. Timestamp-bounded point reads.
6. Timestamp-bounded prefix scans.
7. Branch time-range reporting.
8. Copy-on-write branch forks with an inherited-layer fork version.

Important current files:

1. `crates/core/src/id.rs`
2. `crates/core/src/contract/version.rs`
3. `crates/core/src/contract/timestamp.rs`
4. `crates/storage/src/segmented/mod.rs`
5. `crates/storage/src/txn/manager.rs`
6. `crates/storage/src/txn/context.rs`

This is a good substrate. The missing pieces are product-level consistency and
a timeline resolver.

### Public Reads

The executor command surface supports `as_of` timestamp reads for several data
capabilities:

1. KV get and list.
2. JSON get and list.
3. Event get, get-by-type, length, type list, and list.
4. Graph node get, node list, and neighbors.
5. Vector get and query.
6. Search.
7. Branch diff.

KV, JSON, and vector also expose history commands through `getv`-style flows.

Current evidence:

1. `crates/executor/src/command.rs`
2. `crates/executor/src/handlers/kv.rs`
3. `crates/executor/src/handlers/json.rs`
4. `crates/executor/src/handlers/event.rs`
5. `crates/executor/src/handlers/graph.rs`
6. `crates/executor/src/handlers/vector.rs`
7. `crates/executor/src/handlers/search.rs`

### Branch Operations

Branching already uses versions heavily:

1. Fork records a fork version.
2. Merge-base calculation is version-based.
3. Branch diff can run at a timestamp.
4. Revert restores a version range by writing compensating changes.
5. Cherry-pick copies current selected state from one branch to another.

Current evidence:

1. `crates/engine/src/branch_ops/mod.rs`
2. `crates/engine/src/database/branch_service.rs`

The strongest future capability, "create a branch from this moment in history,"
should build on this branch substrate rather than materializing a full copy of
the database.

### Search And Retrieval

Search and retrieval already accept temporal context:

1. Retrieval requests carry `as_of`.
2. Retrieval requests carry `time_range`.
3. Search supports temporal diff by running retrieval at two points.
4. Search can include version-history enrichment.
5. BM25 results are post-filtered against storage timestamp visibility.
6. Vector search has backend-level liveness filtering and historical metadata
   resolution.

Current evidence:

1. `crates/engine/src/search/substrate.rs`
2. `crates/engine/src/vector/store/search.rs`
3. `crates/executor/src/handlers/search.rs`

This is useful today, but not yet enough to claim reference-grade historical
search. BM25 post-filtering can prevent obvious leakage, but it is not the same
as querying a fully versioned historical index.

## Current Gaps

### No Unified Temporal Context

`as_of` is currently command-specific. There is no shared temporal context that
all data capabilities receive.

This creates uneven behavior:

1. Some commands support `as_of`; others do not.
2. Some transaction-session paths bypass active transaction state for historical
   reads; others are missing from that bypass list.
3. Output metadata is inconsistent between current reads, `as_of` reads, and
   history reads.

V1 direction:

1. Introduce an internal temporal context used by all read paths.
2. Define which commands are temporal and which are always current.
3. Preserve actual selected version and timestamp in output where possible.

### Timestamp And Version Are Not Unified

Storage assigns commit versions monotonically. Timestamps come from system time.
System time can move backward or collide at microsecond resolution.

Today, timestamp-bounded row reads work because they inspect row timestamps. But
branch-from-time needs a single consistent commit frontier. That requires a
timeline mapping from timestamp to commit version.

V1 direction:

1. Commit versions remain authoritative.
2. Timestamps are resolved to commit versions for whole-branch operations.
3. The commit timeline should be monotonic per branch even if wall-clock time
   moves backward.
4. Resolution should be explicit: latest commit at or before timestamp.

### Branch From Historical Point Is Not Exposed

Current branch fork creates a branch from the current source state. The storage
fork model already carries a fork version, but callers cannot request an older
fork version or timestamp.

V1 direction:

1. Add branch-from-version first.
2. Add branch-from-timestamp only after the commit timeline exists.
3. Store the resolved branch point in branch metadata and lineage.
4. Make derived-state behavior explicit.

### Derived State Is Not Fully Temporal

Historical raw data is further along than historical derived state.

Current concerns:

1. BM25 uses temporal post-filtering, not a fully historical index.
2. Vector `time_range` currently uses original creation time for updated
   vectors, not the commit timestamp of the visible version.
3. Auto embeddings and system-space records need explicit branch and temporal
   semantics.
4. Graph relationship indexes and analytics need temporal variants.
5. RAG inherits the correctness of retrieval.

V1 direction:

1. Define derived-state contracts per capability.
2. Refuse, rebuild, or mark stale when derived state cannot support a temporal
   point.
3. Do not silently mix current derived indexes with historical authored data
   unless the behavior is proven correct.

### Retention Semantics Are Product-Incomplete

Storage can retain or prune historical versions. Errors for trimmed or
unavailable history exist, but the product contract is not yet consistent.

V1 direction:

1. Document default history retention.
2. Report available history range accurately enough for user decisions.
3. Return clear errors when a requested version or timestamp is unavailable.
4. Make branch-from-history validate retained data before creating visible
   branch metadata.

### CLI Time Input Is Too Low-Level

The current CLI parses `--as-of` as a raw microsecond integer. That is useful
for tests and internal tooling, but not enough for users.

V1 direction:

1. Accept ISO-8601 timestamps.
2. Accept raw microseconds for machine usage.
3. Consider relative times only if they can be documented precisely.
4. Print timestamps in human-readable and machine-readable forms.

## V1 Required Capabilities

### Point-In-Time Reads

Required user outcome:

1. Read a value as of a timestamp.
2. List records as of a timestamp.
3. Search as of a timestamp where the selected search mode supports it.
4. See the actual version and timestamp returned when metadata is available.

Required semantics:

1. `as_of` means latest visible committed value at or before the selected point.
2. Tombstones hide older values when the tombstone is visible at that point.
3. TTL expiration is evaluated at the query point.
4. Missing history due to retention should be distinguishable from "record did
   not exist."

### History Inspection

Required user outcome:

1. Ask how a record changed over time.
2. See newest-to-oldest versions.
3. Bound history by depth or before-version.
4. Understand deletions and unavailable trimmed history.

Required semantics:

1. History output should include version, timestamp, value, and deletion marker.
2. Tombstones should not be presented as ordinary `null` values without context.
3. History should be available for KV, JSON documents, vectors, and graph
   relationship records where the underlying data model supports it.
4. Events already have sequence history; they do not need a separate `getv`
   model unless event mutation is introduced.

### Branch Comparison Over Time

Required user outcome:

1. Compare current branch state.
2. Compare branch state at a timestamp.
3. Filter comparison by space and data capability.
4. Use comparison output to decide what to copy, promote, or revert.

Required semantics:

1. Comparison should clearly distinguish added, removed, and modified records.
2. Large comparisons need bounded output or pagination before V1 scale claims.
3. Derived state should not pollute comparison unless explicitly requested.

### Restore By Writing A Compensating Change

Required user outcome:

1. Restore a branch to undo a bad version range.
2. Preserve later work where possible.
3. Receive a new version for the restore operation.

Required semantics:

1. Restore is not time-machine mutation. It writes a new change that restores
   selected older values.
2. Existing version-range revert is the right model.
3. Timestamp-based restore should resolve timestamps to versions first.

### Branch From Historical Point

Required user outcome:

1. Create a branch from current state.
2. Create a branch from a retained commit version.
3. Create a branch from a timestamp once timestamp resolution is correct.
4. Open the new branch and work normally from that point.

Required semantics:

1. The new branch point is recorded as a version.
2. If the requested point has been trimmed, the operation fails before creating
   visible branch metadata.
3. The source branch remains unchanged.
4. The new branch should use COW inheritance where possible.
5. Derived state must be copied, rebuilt, invalidated, or explicitly marked as
   needing rebuild.

## Feature Direction

### Timeline Scrub

Timeline scrub is the user-facing experience that makes time travel feel
powerful:

1. Show the branch's available time range.
2. Let the user select a point in time.
3. Resolve the selected point to a concrete version.
4. Read, list, inspect, compare, search, and explain state at that point.
5. Create a new branch from that point.

This should be considered a V1 direction even if the first V1 interface is API
and CLI rather than a visual timeline.

### Branch From Here

The most important high-level operation is:

```text
create branch experiment from main at <temporal-point>
```

The implementation should prefer COW fork by commit version. A materialized copy
should be a fallback only if storage architecture cannot support a retained COW
frontier for the requested point.

### Explain Changes

Users should be able to ask:

1. What changed between two points?
2. What changed in this branch since it was created?
3. What changed in this record?
4. What changed between these branches at a specific time?

This is a product layer over history, diff, and branch lineage.

### Historical Retrieval And RAG

Retrieval should be able to answer:

1. Search the current branch.
2. Search this branch as it existed at a timestamp.
3. Search the records created or modified in a time range.
4. Generate an answer using only context visible at that point.
5. Show which versions of records were used.

This is high-value, but only if the derived search/index contracts are honest.

## Architecture Requirements For Next-Generation Crates

### Core

Core should define stable temporal concepts:

1. Commit version.
2. Timestamp.
3. Temporal point.
4. Temporal bounds.
5. Versioned output shape.
6. History availability errors.

Core should not decide storage layout, branch policy, or search semantics.

### Storage

Storage-next should own generic time/version mechanics:

1. Commit-version MVCC.
2. Timestamp preservation.
3. Monotonic commit timeline persistence.
4. Timestamp-to-version resolution.
5. Version-bounded and timestamp-bounded generic reads.
6. COW fork at explicit commit version.
7. Retention validation for requested historical points.

Storage should not know why the engine wants a version. It should only prove
whether the generic data needed for that version is available.

### Engine

Engine-next should own product semantics:

1. Branch creation from current, version, and timestamp.
2. Branch metadata and lineage.
3. Data capability semantics for KV, JSON, events, graph, vectors, and search.
4. Derived-state validity and rebuild policy.
5. User-facing errors and output metadata.
6. Search/RAG temporal behavior.

Engine should consume storage through a documented storage contract, not by
reaching around it.

### Executor And CLI

Executor and CLI should expose simple product operations:

1. `get --as-of <time>`
2. `history <record>`
3. `compare <branch-a> <branch-b> --as-of <time>`
4. `restore <branch> --from-version <v1> --to-version <v2>`
5. `branch create <name> --from <branch> --at <time-or-version>`
6. `search --as-of <time>`

The exact command names can be decided later. The important point is that users
should not have to understand storage snapshots, WAL, checkpoints, compaction,
or internal transaction state.

## Feasibility Assessment

### Branch From Commit Version

Feasibility: high.

Why:

1. Storage already has commit-version MVCC.
2. Storage already has COW inherited layers with fork-version clamps.
3. Branch metadata already records fork anchors.
4. Branch operations already reason about merge bases and version ranges.

Required work:

1. Add storage support for fork at an explicit retained commit version.
2. Validate the requested version is available before publishing branch
   metadata.
3. Copy/register branch spaces as they existed at that version.
4. Record the exact branch point in control metadata and lineage.
5. Define derived-state handling.

### Branch From Timestamp

Feasibility: medium, high after commit timeline exists.

Why:

1. Row-level timestamp reads already exist.
2. Branch time range already exists.
3. The missing primitive is timestamp-to-version resolution.

Required work:

1. Persist a per-branch commit timeline.
2. Make commit timestamps monotonic per branch.
3. Resolve timestamp to latest retained commit version at or before the point.
4. Use branch-from-version after resolution.
5. Return clear history-unavailable errors when no retained commit satisfies the
   request.

### Historical Reads

Feasibility: high.

Why:

1. Most storage mechanics already exist.
2. KV, JSON, event, graph, vector, and search already have partial support.

Required work:

1. Normalize temporal context.
2. Normalize output metadata.
3. Fill command gaps.
4. Add retention-aware errors.
5. Add cross-capability tests.

### Historical Search And RAG

Feasibility: medium.

Why:

1. Current retrieval already accepts temporal filters.
2. Vector search has meaningful historical support.
3. BM25 and derived indexes need stronger contracts.

Required work:

1. Define whether indexes are versioned, rebuildable, or post-filtered.
2. Make recall guarantees explicit.
3. Include selected record versions in RAG provenance.
4. Refuse temporal retrieval modes that cannot be made correct.

### Temporal Graph Analytics

Feasibility: medium.

Why:

1. Graph node and neighbor point reads exist.
2. Traversal, ontology, and analytics need temporal propagation.

Required work:

1. Add temporal context to traversal.
2. Add temporal context to ontology reads.
3. Decide whether analytics run over current graph, temporal graph, or both.
4. Test graph relationship-layer behavior across branches and time.

## Recommended Implementation Order

This is not a cleanup milestone plan. It is the product-driven order that keeps
the architecture honest.

1. Define temporal types and output contracts.
   Add a shared product model for current, version, and timestamp points.

2. Normalize existing historical reads.
   Make `as_of`, history output, event behavior, vector metadata, and
   transaction-session bypass behavior consistent.

3. Add a commit timeline.
   Persist per-branch commit version to timestamp mapping and define monotonic
   timestamp rules.

4. Implement branch from commit version.
   Use existing COW architecture, validate retention, and record branch
   lineage.

5. Implement branch from timestamp.
   Resolve timestamp to version through the commit timeline, then use branch
   from version.

6. Define derived-state temporal contracts.
   Search, vectors, graph relationships, auto embeddings, and recipes need
   explicit current-vs-historical behavior.

7. Build the timeline product pathway.
   Add the user-facing flow for inspect, compare, search, restore, and branch
   from selected points.

8. Expand tests to reference-grade coverage.
   Time travel needs deterministic unit tests, integration tests, recovery
   tests, retention tests, fuzz tests, and crash tests.

## Testing Requirements

Time travel should become one of Strata's highest-assurance surfaces.

Required test classes:

1. Per-key history order and tombstone representation.
2. Timestamp-bounded point reads.
3. Timestamp-bounded scans.
4. TTL behavior at historical query points.
5. Retention and history-unavailable errors.
6. Branch-from-version crash recovery.
7. Branch-from-timestamp resolution.
8. Derived-state rebuild or refusal behavior.
9. Search/RAG provenance under `as_of`.
10. Graph traversal and relationship-layer behavior under `as_of`.
11. Concurrent commits while resolving temporal points.
12. Clock skew and duplicate timestamp scenarios.
13. Checkpoint, snapshot install, WAL replay, and reopen parity.
14. Portable backend conformance where backend semantics affect timeline or
    durability.

The product promise is only credible if tests cover normal, degraded, lossy,
and recovery paths.

## Open Questions

1. Should V1 expose both `version` and `timestamp` selectors everywhere, or
   should timestamp be the default user selector with version as an advanced
   selector?

2. Should history be retained forever by default for V1, or should Strata define
   a bounded default with explicit user configuration?

3. Should branch-from-time include derived state immediately, or should the new
   branch open with derived state marked stale and rebuilt lazily?

4. Should search temporal correctness be strict in V1, or can some retrieval
   modes be labeled approximate while exact modes are added later?

5. Should event timestamps be treated as event occurrence time, commit time, or
   both? The answer affects temporal event queries and search filtering.

6. Should graph relationship edges be allowed to pin entity references to a
   version or timestamp, or should relationships always resolve against the
   active temporal view?

7. How should clone/import/export preserve commit timelines and branch points?

8. How should object-storage and browser/WASM backends prove commit timeline
   atomicity and recovery semantics?

## V1 Position

Strata should treat versioning and time travel as a signature product feature.

The current codebase already proves the core storage idea. The next step is not
to invent time travel from scratch. The next step is to make the existing
substrate coherent:

1. Commit versions provide correctness.
2. Timestamps provide user selection.
3. A commit timeline connects them.
4. Temporal views make reads consistent.
5. Branch-from-history turns inspection into action.
6. Derived-state contracts make retrieval and graph behavior trustworthy.

Once those pieces exist, "scrub back to a moment and create a branch from
there" becomes a natural Strata capability rather than a special-case trick.
