# Storage-Next Commit Timeline Substrate

Status: V1 architecture draft

## Purpose

V1 product time travel depends on resolving a branch timestamp to a retained
commit version. Per-row commit timestamps are necessary, but not sufficient as
the only lookup structure: resolving "latest commit at or before time T on
branch B" by scanning row chains would be too expensive and would depend on
which keys happen to exist.

Storage-next therefore owns a generic per-branch commit timeline substrate.
Engine-next owns the product commands and explanations built on top of it:
`as_of`, timeline scrub, branch-from-time, and retained-history diagnostics.

## Binding Decision

The commit timeline is stored as storage-owned system rows, not as a separate
L4 object service.

Consequences:

1. L2 does not need a `timeline/` object family.
2. L4 does not need a timeline service.
3. L3 must define row/key encodings for storage-owned timeline rows.
4. L6 must preserve timeline rows through normal table, compaction, retention,
   and recovery mechanics.
5. L7 must write timeline rows as part of commit publication.
6. L8 must recover, validate, and repair timeline facts from WAL/checkpoint
   state.
7. L9 must expose timeline resolution through storage APIs, not by exposing the
   physical timeline row family.

## Storage Space

Timeline rows use the storage-reserved `storage_space_id = 0x01`, defined in
`docs/architecture/storage-next/storage-space-id-registry.md`.

Engine-owned rows must not use this ID.

## Logical Indexes

The timeline stores two logical indexes per branch:

```text
branch id + commit timestamp + commit version -> commit version
branch id + commit version                    -> commit timestamp
```

The timestamp index includes commit version as a tiebreaker so multiple commits
with the same timestamp remain ordered deterministically.

The exact byte layout belongs in the storage format spec. The required behavior
is:

1. Resolve latest retained commit version at or before timestamp `T`.
2. Resolve commit timestamp for retained commit version `V`.
3. Detect retained-history gaps.
4. Preserve deterministic ordering when timestamps repeat.
5. Rebuild or validate timeline state during recovery.

## Commit Path

For each successful commit:

1. L7 allocates a commit version.
2. L7 assigns one commit timestamp.
3. L7 writes user rows with that version and timestamp.
4. L7 writes timeline rows in the same internal commit unit.
5. Durable local mode records the same facts in the WAL before visible publish.
6. Cache mode records the same facts in memory, with no crash durability claim.

Timeline rows are part of storage atomicity. A visible commit without matching
timeline rows is a storage invariant failure.

## Recovery

During recovery:

1. WAL replay restores user rows and timeline rows together.
2. Checkpoint/snapshot install restores row-native timeline rows.
3. L8 catches commit-version and timestamp allocators up to recovered facts.
4. L8 validates that visible commits have timeline entries.
5. Missing or corrupt timeline rows are storage corruption unless L8 has enough
   durable facts to rebuild them deterministically.

Recovery facts should distinguish:

1. Timeline rebuilt from WAL/checkpoint.
2. Timeline recovered without repair.
3. Timeline corrupt but repairable.
4. Timeline corrupt and unrecoverable.

## Retention And Compaction

Retention may remove timeline entries only when the corresponding commit version
is outside retained history for that branch.

Compaction must preserve:

1. Timeline entries needed for retained `as_of` queries.
2. Timeline entries needed by branch-from-time.
3. Timeline entries needed to explain retained-history gaps.
4. Timeline entries pinned by snapshots or read views.

If user rows for a commit are retained, the timeline entry for that commit must
also be retained.

## L9 API Shape

L9 should expose timeline behavior through semantic storage methods, not raw
timeline-row scans.

Required storage-facing operations:

1. Resolve timestamp to retained commit version.
2. Resolve commit version to commit timestamp.
3. Report retained timeline bounds for a branch.
4. Report whether a timestamp miss is before history, after latest, or inside a
   pruned gap.

Engine-next uses those facts to implement product `as_of`, scrub, and
branch-from-time behavior.

## Testing Requirements

1. Commit writes user rows and timeline rows atomically.
2. Multiple commits with the same timestamp resolve deterministically.
3. Timestamp-to-version lookup returns the greatest retained version at or
   before the timestamp.
4. Version-to-timestamp lookup returns the original commit timestamp.
5. Retention removes timeline entries only with the corresponding history.
6. Compaction preserves retained timeline answers.
7. WAL replay rebuilds the timeline.
8. Snapshot install restores the timeline.
9. Missing timeline rows are detected during recovery.
10. Fuzz recovery ordering cannot produce inconsistent timeline state.

## Open Questions

1. What exact byte prefixes inside storage space `0x01` distinguish
   timestamp-index rows from version-index rows?
2. Should cache mode expose ephemeral timeline bounds through the same L9
   methods as durable mode?
