# Branching, Versioning, And Time-Travel Pathways

Status: Draft pathway group

This document expands the V1 pathways for branch workspaces, record history,
time-travel reads, timeline scrub, branch-from-history, and branch change
management.

## Pathway 25: Create And Manage Branch Workspaces

### Goal

A user creates a branch from existing data, selects branch context, lists
branches, inspects branch state, and deletes branches safely.

### Flow

1. Inspect available branches.
2. Create an empty branch or create a branch from an existing branch.
3. Select the branch as working context.
4. Read, write, search, or relate data inside that branch.
5. Inspect branch metadata and lineage.
6. Delete the branch only when intended.

### Surface

Branch SDK APIs, CLI branch commands, branch context commands, branch metadata,
branch lifecycle status, branch docs.

### Guarantees

Branches must isolate user data, record branch points, preserve source branch
state, protect against same-name lifecycle races, and clean up branch-local
derived state safely.

### Failures

Missing source branch, duplicate destination, invalid branch name, read-only
access, branch deletion race, unsupported backend, and derived cleanup failure
should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep branch workspaces as a core product concept. Present "create branch from"
as the product language and keep low-level fork terminology secondary.

## Pathway 26: Inspect Record History

### Goal

A user asks how a KV, JSON, vector, graph relationship, or other supported
record changed and sees versions, timestamps, values, deletions, and
retained-history limits.

### Flow

1. Select a record identity, branch, and space.
2. Request history.
3. Strata reads retained versions newest first.
4. Strata returns version, timestamp, value or deletion marker, and retention
   metadata where available.
5. The user selects a version or timestamp for inspection, restore, or branch
   workflows.

### Surface

History APIs, `getv`-style commands, versioned output, entity references,
retention errors, time-travel docs.

### Guarantees

History must distinguish deleted values from ordinary null values, preserve
actual version and timestamp metadata, and explain when history has been
trimmed or is unavailable.

### Failures

Missing record, unsupported history for capability, retained history trimmed,
corrupt historical value, invalid entity reference, and backend limitation
should surface clearly.

### V1 Decision

Required.

### Cleanup

Normalize KV, JSON, vector, and graph history output. Do not expose tombstones
as ordinary user values without deletion context.

## Pathway 27: Read Data As Of A Point In Time

### Goal

A user selects a timestamp or version and runs normal reads, lists, graph
lookups, vector queries, or supported searches against that historical view.

### Flow

1. Select branch, space, and temporal point.
2. Run a read, list, graph query, vector query, or supported search.
3. Strata resolves the temporal point to visible data.
4. Strata returns values and actual selected version/timestamp metadata where
   supported.
5. The user continues inspecting current or historical state intentionally.

### Surface

`as_of` APIs, version selectors, CLI timestamp parsing, temporal context,
history-unavailable errors, search temporal filters.

### Guarantees

Point-in-time reads must mean latest visible committed state at or before the
selected point. Tombstones and TTL expiration must be evaluated at that point.

### Failures

Invalid timestamp, requested point before retained history, unsupported temporal
mode, stale derived index, missing branch, and backend limitation should
surface clearly.

### V1 Decision

Required.

### Cleanup

Replace ad hoc `as_of` handling with a shared temporal context. Add human time
parsing and normalize output metadata.

## Pathway 28: Scrub And Explain A Branch Timeline

### Goal

A user inspects the available time range, picks a point, resolves it to
concrete state, and understands what changed before or after that point.

### Flow

1. Ask for the branch's available time range.
2. Select a timestamp or version.
3. Strata resolves the selection to a retained commit point.
4. The user inspects state, search results, or changes around the point.
5. The user may branch from that point or restore selected changes.

### Surface

Time range command, temporal resolver, history APIs, branch diff, search,
timeline UI or CLI output, commit timeline metadata.

### Guarantees

Timeline scrub must never guess. The selected point must resolve to concrete
retained state, and unavailable history must be explicit.

### Failures

No branch history, timestamp before retained range, timestamp after current
state, missing commit timeline, trimmed history, and unsupported backend should
surface clearly.

### V1 Decision

Required.

### Cleanup

Introduce a commit timeline that connects user timestamps to commit versions.
Do not rely on row timestamp scans for whole-branch timeline semantics.

## Pathway 29: Create A Branch From Historical State

### Goal

A user creates a new branch from current state, a retained commit version, or a
timestamp that resolves to a retained branch point.

### Flow

1. Choose source branch and destination branch name.
2. Choose current, version, or timestamp as the branch point.
3. Strata validates source branch, destination name, backend support, and
   retained history.
4. Strata creates the branch using copy-on-write where possible.
5. Strata records branch metadata, lineage, and derived-state status.
6. The user opens the new branch and works normally.

### Surface

Branch create/fork APIs, temporal point selector, commit timeline resolver,
branch metadata, DAG/lineage output, derived-state rebuild status.

### Guarantees

The new branch must represent the selected retained state, not a nearby guess.
Failure must occur before visible branch metadata is published. Derived state
must be copied, rebuilt, invalidated, or marked stale explicitly.

### Failures

Missing source, duplicate destination, unsupported backend, requested version
trimmed, timestamp cannot resolve, storage fork failure, metadata publish
failure, and derived-state failure should surface clearly.

### V1 Decision

Required.

### Cleanup

Implement branch-from-version first, then branch-from-timestamp through the
commit timeline. Avoid materialized full copies unless COW cannot support the
requested point.

## Pathway 30: Compare, Promote, Copy, And Restore Branch Changes

### Goal

A user compares current or historical branch state, previews conflicts,
promotes completed work, copies selected records or changes, and restores a bad
version range by writing a compensating change.

### Flow

1. Select source, target, and optional temporal context.
2. Compare branches or selected records.
3. Preview conflicts or selected changes.
4. Promote a branch, copy selected changes, or restore a version range.
5. Strata writes a new committed change and reports the resulting version.

### Surface

Branch compare/diff, merge/promote, cherry-pick/copy, restore/revert, temporal
selectors, conflict output, branch lineage.

### Guarantees

Comparison must be explainable. Promotion must preserve conflict semantics.
Restore must write compensating changes rather than mutating history. Later
work should be preserved where the restore model promises it.

### Failures

Unrelated branches, conflict, missing branch, unsupported capability merge,
trimmed history, invalid version range, read-only access, and derived-state
incompatibility should surface clearly.

### V1 Decision

Required.

### Cleanup

Use product language: compare, promote, copy, restore. Keep Git-like command
names only where useful for compatibility, not as the primary mental model.
