# L7K Implementation Plan: Recovery Replay And Allocator Catch-Up

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-test-plan.md`

## Objective

Implement the L7 replay hooks that let L8 install already-durable commit
records into L6 without running the normal mutating commit protocol.

L7I/L7J established the forward durable path:

```text
validate/admit/conflict -> allocate/stamp -> WAL append -> L6 apply -> visible publish
```

L7K handles the recovery side of that protocol:

```text
decoded durable WalRecord -> validate replay rows -> install or confirm rows
-> catch up clocks -> publish visible -> clear matching unresolved gate
```

L7K is not process-open recovery orchestration. L8 still owns WAL scanning,
checkpoint selection, recovery health, truncation, repair policy, and deciding
which records to replay. L7K owns the storage-local replay rules once L8 has a
decoded durable commit record and the target branch state.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
3. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
4. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
5. `docs/architecture/storage/commit-timeline-substrate.md`
6. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
7. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
9. `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md`
10. `docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md`
11. `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md`
12. `crates/storage-next/src/commit/allocator.rs`
13. `crates/storage-next/src/commit/durable.rs`
14. `crates/storage-next/src/commit/durable_gate.rs`
15. `crates/storage-next/src/commit/outcome.rs`
16. `crates/storage-next/src/commit/visibility.rs`
17. `crates/storage-next/src/format/wal.rs`
18. `crates/storage-next/src/branch/state.rs`
19. `crates/storage/src/segmented/mod.rs`
20. `crates/storage/src/durability/recovery.rs`

## Existing-Code Source Map

| Current file | L7K evidence | L7K action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | Old storage had recovery apply paths that preserved version and timestamp. | Port the storage rule: replay uses durable facts, not newly allocated facts. Keep segmented recovery scheduling out of L7. |
| `crates/storage/src/durability/recovery.rs` | Old recovery read WAL and replayed durable records. | Use as sequencing evidence only. L8 owns reading and selecting WAL records; L7K owns applying one decoded durable commit safely. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Old bootstrap caught up version state from recovered data. | Port allocator catch-up expectations for commit versions and timestamps. Do not reintroduce transaction-id catch-up in V1. |
| `crates/storage-next/src/format/wal.rs` | `WalRecord` already validates outer branch/version/timestamp against payload rows. | L7K should accept decoded `WalRecord` or an equivalent validated replay request, not raw WAL bytes. |
| `crates/storage-next/src/commit/allocator.rs` | `CommitFactAllocator` already has version and timestamp catch-up helpers. | Use catch-up after replay rows are installed or exact duplicates are confirmed, before normal writes resume. |
| `crates/storage-next/src/commit/durable_gate.rs` | L7J records unresolved durable facts and exposes exact clear. | Clear matching gate facts only after replay rows are installed/confirmed and visibility is published. |
| `crates/storage-next/src/commit/durable.rs` | L7J introduced L6 apply and visible publisher adapter traits. | Reuse the narrow adapters so replay tests can inject L6/visibility faults without backend IO. |
| `crates/storage-next/src/commit/outcome.rs` | `CommitOutcomeKind::Replay` already exists. | Use it for successful replay/catch-up facts; add a separate internal replay report only if idempotent-vs-applied needs to be visible to L8. |

## Scope

L7K implements:

1. a crate-private replay entrypoint for one already-decoded durable commit;
2. replay request validation over `WalRecord` rows;
3. replay row install into L6 with the original commit version and timestamp;
4. replay conflict bypass: no read-set/CAS validation and no new row stamping;
5. exact duplicate replay idempotency;
6. duplicate mismatch rejection;
7. partial replay-state rejection;
8. commit-version allocator catch-up after successful or idempotent replay;
9. commit-timestamp guard catch-up after successful or idempotent replay;
10. visible-version publication after replay install or exact duplicate
    confirmation;
11. exact unresolved durable gate clear after successful matching replay;
12. replay outcomes/reports that preserve branch, version, timestamp,
    durability, and row counts;
13. source guard updates if a new replay module is added.

L7K does not implement:

1. WAL segment listing or scanning;
2. WAL tail repair or truncation;
3. choosing checkpoint vs WAL replay order;
4. database open sequencing;
5. lifecycle recovery health classification;
6. lossy recovery policy;
7. manifest publication;
8. table checkpoint restore;
9. retention or compaction scheduling;
10. branch deletion/clear recovery policy;
11. public repair commands;
12. product `as_of` APIs;
13. durable storage transaction ids.
14. generated property/fuzz harness expansion, which belongs to `L7M`.

## Replay Contract

L8 supplies one decoded durable record in commit order. L7K validates and
applies that record to the target branch.

Required input facts:

```text
CommitReplayRequest
  WalRecord or validated replay rows
  durability class: Standard | Always
  optional matching unresolved durable fact
```

Rules:

1. replay never allocates a new commit version;
2. replay never requests a new timestamp;
3. replay never restamps rows;
4. replay never runs read-set or CAS validation;
5. replay rows must all belong to the target branch;
6. replay rows must all carry the record commit version;
7. replay rows must all carry the record commit timestamp;
8. replay rows must include the storage-owned timeline rows for that commit;
9. replay applies the full row set atomically or not at all;
10. replay publishes visible version only after rows are installed or exact
    duplicate rows are confirmed;
11. replay catches up version and timestamp clocks after rows are installed or
    exact duplicate rows are confirmed, and before allowing normal writes;
12. replay can clear an unresolved durable gate only after visibility is safe.

## Proposed Type Surface

Names may change if responsibilities stay intact. Keep everything
`pub(crate)`.

```text
CommitReplayRuntime<'a, S, B = BranchLocalState, V = VisibleVersionTracker> {
  config: &'a CommitRuntimeConfig,
  allocator: &'a mut CommitFactAllocator<S>,
  branch: &'a mut B,
  visible: &'a mut V,
  durable_gate: &'a CommitUnresolvedDurableGate,
}

CommitReplayRequest {
  record: WalRecord,
  durability: CommitDurabilityClass,
}

CommitReplayAction {
  Applied,
  AlreadyApplied,
}

CommitReplayReport {
  action,
  outcome: CommitOutcome,
  rows_checked,
  rows_applied,
  gate_cleared: bool,
}
```

Entry point:

```text
CommitReplayRuntime::replay(request) -> CommitRuntimeResult<CommitReplayReport>
```

The runtime may instead expose a smaller function if the type above is too
heavy:

```text
replay_durable_record(config, allocator, branch, visible, gate, request)
```

The important contract is that replay is a separate path from
`CommitCacheRuntime::execute` and `CommitDurableRuntime::execute`.

## Validation Rules

### Replay Request Shape

1. `CommitDurabilityClass::Standard` and `Always` are accepted.
2. `NotDurable` and `Uncertain` are rejected.
3. `CommitVersion::ZERO` is rejected through `WalRecord`/stamp validation.
4. Empty payload rows are rejected by the WAL payload layer.
5. Every payload row must match record branch/version/timestamp.
6. The target branch id must match the record branch id.
7. The payload must contain exactly one valid timeline pair for the record
   branch/version/timestamp.
8. User rows and timeline rows are installed together.
9. Replay must validate that the local commit-runtime config itself is valid,
   but it must not re-admit already-durable rows through the current batch size
   limits. A durable WAL record written under an older or wider config remains
   recoverable.

### Duplicate Replay

Replay must inspect existing branch state before inserting rows.

Cases:

1. no replay rows are present: apply the full row set;
2. all replay rows are already present with exact row facts: treat as
   idempotent and publish/catch up as needed;
3. any replay row is present with different facts: fail closed;
4. some but not all replay rows are present: fail closed;
5. timeline rows present but user rows missing: fail closed;
6. user rows present but timeline rows missing: fail closed.

Exact means the row's physical key, commit version, commit timestamp, expiry,
tombstone flag, and value bytes match the durable replay row.

The partial cases are deliberately not repaired by L7K. They indicate a broken
atomicity invariant and belong to L8 recovery policy.

## Protocols

### New Replay

```text
validate replay request
check matching/different unresolved durable gate state
capture target branch read view
classify duplicate state as absent/exact/mismatch/partial
apply replay rows atomically into L6
catch up version allocator to record version
catch up timestamp guard to record timestamp
publish visible version from replay visibility facts
clear matching unresolved gate, if present
return CommitOutcomeKind::Replay with action Applied
```

### Idempotent Replay

```text
validate replay request
check matching/different unresolved durable gate state
capture target branch read view
confirm all durable rows are already present exactly
catch up version allocator to record version
catch up timestamp guard to record timestamp
publish visible version from replay visibility facts
clear matching unresolved gate, if present
return CommitOutcomeKind::Replay with action AlreadyApplied
```

### Mismatch Or Partial Replay

```text
validate replay request
capture target branch read view
detect mismatch or partial row presence
do not catch up allocator
do not mutate L6
do not publish visible
do not clear unresolved gate
return typed replay mismatch error
```

If replay validation fails before reading L6, it should also leave allocator,
visible tracker, and gate unchanged.

## Unresolved Durable Gate Interaction

L7K must be safe in two modes:

1. process-open replay where no in-process L7J gate exists yet;
2. in-process repair after L7J recorded an unresolved durable fact.

Rules:

1. if the gate is empty, replay may proceed;
2. if the gate contains the same branch/version/timestamp/durability facts,
   replay may proceed and must clear the gate after visibility is published;
3. if the gate contains a different unresolved durable fact, replay must fail
   closed before mutation;
4. replay must not call the normal mutating admission gate, because that gate is
   intentionally closed while unresolved durable state exists;
5. replay may use the per-branch commit guard to serialize with any accidental
   normal writer, but the normal unresolved durable gate is the primary write
   blocker.

## Allocator And Visibility Catch-Up

Replay must update local facts so later normal commits are ordered after
recovered data.

Rules:

1. `CommitFactAllocator::catch_up_to_recovered_version(record.version)` runs
   only after replay rows are installed or exact duplicate rows are confirmed;
2. `CommitFactAllocator::catch_up_to_recovered_timestamp(record.timestamp)`
   runs only after replay rows are installed or exact duplicate rows are
   confirmed;
3. catch-up with lower facts is idempotent;
4. the next allocated mutating commit version must be greater than every
   replayed version;
5. generated timestamps after replay must not move below recovered timestamp
   floors;
6. visible publication uses the replayed commit version only after install or
   exact duplicate confirmation;
7. visible catch-up must use the same monotonic `VisibleVersionTracker`
   semantics as normal commits.

V1 has no transaction-id allocator. Do not add one in L7K.

## Failure Classification

| Boundary | Mutated L6? | Visible? | Expected classification |
|---|---:|---:|---|
| malformed replay request | no | no | invalid replay request |
| branch mismatch | no | no | branch mismatch |
| gate has different unresolved fact | no | no | unresolved durable conflict |
| duplicate exact | no new rows | yes after publish | replay already applied |
| duplicate mismatch | no | no | replay mismatch |
| partial existing rows | no | no | replay partial-state failure |
| L6 apply failure | no partial rows by L6 contract | no | lower-layer branch runtime failure; no allocator catch-up |
| visible publish failure after replay apply | rows may be present | no | applied-not-visible replay failure; gate remains or is recorded |
| replay success | yes or exact duplicate | yes | replay outcome |

If visible publication fails after applying replay rows, L7K must not clear the
unresolved durable gate. If no gate exists because this is process-open replay,
L7K should return a typed applied-not-visible error that L8 can convert into
recovery health state.

If L6 rejects the replay rows after the WAL record has already been accepted as
durable, L7K must not catch up allocators or visibility. With an empty gate it
records a durable-not-applied fact so L8 can surface and reconcile the
durable-but-not-installed state; with an exact matching durable-not-applied gate
it leaves the gate in place.

## Module Layout

Expected production layout:

```text
crates/storage-next/src/commit/
  replay.rs
  tests/
    replay.rs
```

Reuse existing modules:

1. `allocator.rs` for catch-up;
2. `durable.rs` for L6 apply/visible adapter traits if they remain there;
3. `durable_gate.rs` for unresolved durable facts and exact clear;
4. `outcome.rs` for replay outcome facts;
5. `timeline.rs` for validating replayed timeline rows.

If the adapter traits become shared by durable and replay code, move them to a
small `commit/apply.rs` module rather than duplicating them.

## Implementation Steps

### L7K-A: Replay Request And Report

1. Add `CommitReplayRequest`.
2. Validate durability class and target branch.
3. Extract rows from `WalRecord` without re-decoding WAL bytes.
4. Add `CommitReplayAction` and `CommitReplayReport`.
5. Add bounded debug/display tests with no value-byte leakage.

Exit gate: a decoded durable record can be represented as a replay request.

### L7K-B: Replay Row Validation

1. Validate every row matches branch/version/timestamp.
2. Validate timeline row pair exists and matches the commit facts.
3. Reject malformed or missing timeline facts.
4. Reject branch-mismatched rows.
5. Reuse L3/L7G validators instead of hand-parsing row bytes.

Exit gate: replay cannot install rows whose durable facts diverge from the WAL
record.

### L7K-C: Duplicate Classification

1. Capture an L6 read view.
2. Check every replay row by physical key and version.
3. Classify absent, exact duplicate, mismatch, or partial.
4. Add direct tests for all four classes.

Exit gate: replay is idempotent only for exact durable duplicates.

### L7K-D: Apply And Publish

1. Apply absent rows through L6 atomically.
2. Skip apply for exact duplicates.
3. Catch up version and timestamp allocators.
4. Publish visible version only after apply or exact confirmation.
5. Return `CommitOutcomeKind::Replay`.

Exit gate: replayed rows become visible without normal conflict validation or
new version allocation.

### L7K-E: Gate Reconciliation

1. Allow replay with an empty gate.
2. Allow replay with an exact matching gate.
3. Reject replay with a different gate fact.
4. Clear the exact gate only after visibility succeeds.
5. Preserve the gate on replay failure.

Exit gate: L7J unresolved durable state can be repaired by replay.

### L7K-F: L7M Harness Handoff

1. Record the direct replay action classes that `L7M` must generate:
   absent, exact duplicate, mismatch, and partial replay.
2. Record the allocator catch-up scenarios that `L7M` must generate.
3. Record the gate-empty, gate-matching, and gate-different scenarios that
   `L7M` must generate.
4. Keep the L7K direct tests behavior-focused so the generated harness can
   reuse the same public-in-crate replay surface without duplicating fixtures.

Exit gate: L7K exposes a replay surface and direct coverage that L7M can drive
from generated scripts.

## Source Guard Policy

L7K may import:

1. `crate::format::wal::WalRecord`;
2. `crate::branch` read/apply boundaries already used by L7;
3. `crate::row::StorageRow`;
4. `crate::config::mode::DurabilityPolicy` only if needed for durable class
   mapping;
5. existing `crate::commit` modules.

L7K must not import:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::object`;
4. `crate::service::wal` for scanning or reading WAL;
5. direct filesystem/path/environment APIs;
6. engine/product modules;
7. public transaction-session vocabulary;
8. JSON, graph, vector, search, event, embedding, remote, hub, or dataset
   terms.

L8 will call L4 services and then call L7K. L7K should not reach down to L4
except for accepting already-decoded format objects.

## Sensitivity Probes

The L7K suite should fail if:

1. replay allocates a fresh commit version;
2. replay assigns a fresh timestamp;
3. replay runs normal read-set/CAS conflict validation;
4. replay publishes visibility before L6 apply;
5. replay treats partial existing rows as success;
6. replay treats duplicate mismatch as idempotent;
7. replay omits timeline rows;
8. replay fails to catch up the version allocator;
9. replay fails to catch up the timestamp guard;
10. replay clears a different unresolved durable gate fact;
11. replay clears the gate before visible publication succeeds;
12. replay stores or prints user value bytes.

Record any probes that are run in
`docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`.

## Verification Commands

Minimum commands for this slice:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties
cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults
cargo test -p strata-storage-next --no-default-features --locked --lib commit
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Focused commands during development:

```bash
cargo test -p strata-storage-next --locked --lib commit::tests::replay
cargo test -p strata-storage-next --locked --lib commit::tests::allocator
cargo test -p strata-storage-next --locked --lib commit::tests::durable_gate
cargo test -p strata-storage-next --locked --lib commit::tests::timeline
```

## Exit Criteria

L7K is complete when:

1. L8 can submit a decoded durable `WalRecord` for replay;
2. replay applies rows with original WAL version and timestamp;
3. replay installs or validates timeline rows;
4. replay bypasses normal conflict validation and version allocation;
5. exact duplicate replay is idempotent;
6. mismatch and partial replay states fail closed;
7. version and timestamp allocators catch up after replay;
8. visible version publishes only after replay rows are installed or confirmed;
9. matching unresolved durable gate facts clear only after replay visibility;
10. source guards remain green;
11. L7H/L7I/L7J behavior remains unchanged;
12. L7M has a clear generated-harness handoff for replay action classes.
