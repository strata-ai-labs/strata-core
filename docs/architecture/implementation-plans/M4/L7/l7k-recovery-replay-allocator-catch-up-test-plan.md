# L7K Test Plan: Recovery Replay And Allocator Catch-Up

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that L7 can replay one already-durable commit record into L6 without
running the normal commit protocol, while preserving the WAL record's original
commit facts and leaving L8 in control of recovery orchestration.

The suite must fail if L7K:

1. allocates a new commit version during replay;
2. generates a new timestamp during replay;
3. restamps any replayed row;
4. runs read-set or CAS conflict validation;
5. publishes visible version before L6 install or exact duplicate
   confirmation;
6. treats partial replay state as success;
7. treats mismatched duplicate rows as idempotent;
8. omits or rewrites commit-timeline rows;
9. fails to catch up version or timestamp allocators;
10. clears an unresolved durable gate before replay visibility is safe;
11. clears a different unresolved durable gate fact;
12. stores or prints user value bytes in replay errors or reports.

Do not add tests that only prove planning documents exist or link to each
other. L7K automated tests should exercise replay behavior, allocator catch-up,
gate reconciliation, direct fault windows, or source boundaries. Generated
model parity belongs to `L7M`, using the replay surface and cases recorded by
this plan.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/replay.rs` for direct replay protocol
   tests.
2. `crates/storage-next/src/commit/tests/allocator.rs` for focused allocator
   catch-up tests if new boundary cases are added.
3. `crates/storage-next/src/commit/tests/durable_gate.rs` for exact gate clear
   behavior that is independent of replay.
4. `crates/storage-next/tests/commit_runtime_faults.rs` for behavioral replay
   fault windows that are awkward as module-local tests.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary checks.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. deterministic manual timestamp source;
3. real `CommitFactAllocator`;
4. real `VisibleVersionTracker`;
5. real `CommitUnresolvedDurableGate`;
6. real `BranchLocalState` for success/idempotency parity tests;
7. fake L6 apply and visible-publish adapters for fault ordering tests;
8. `WalRecord::new` or an equivalent format-layer constructor for replay
   records;
9. opaque value bytes only;
10. no engine DTOs, JSON, graph, vector, search, public transaction-session,
    product `as_of`, remote, hub, or dataset vocabulary.

Replay fakes should record call order:

```text
validate_replay -> classify_existing_rows -> l6_apply_or_confirm_duplicate
-> catch_up_allocators -> publish_visible -> clear_gate
```

Tests may assert a stricter order if the implementation chooses to catch up
allocators after visibility, but no test may allow visible publication before
L6 install or exact duplicate confirmation.

## Direct Test Matrix

### 1. Replay Request Validation

Required cases:

1. `Standard` durable replay request is accepted.
2. `Always` durable replay request is accepted.
3. `NotDurable` replay request rejects before reading L6.
4. `Uncertain` replay request rejects before reading L6.
5. target branch mismatch rejects before mutation.
6. row branch mismatch rejects through existing format/runtime validation.
7. row commit-version mismatch rejects through existing format/runtime
   validation.
8. row timestamp mismatch rejects through existing format/runtime validation.
9. empty replay payload rejects through the WAL payload layer.
10. missing timeline row pair rejects before mutation.
11. timeline row pair with wrong branch/version/timestamp rejects before
    mutation.
12. user mutation into the commit-timeline namespace rejects through existing
    L7B/L7G validation rather than replay special cases.
13. a valid durable record whose row count exceeds the current admission config
    still replays, because recovery validates durable facts rather than
    re-applying live batch limits.

Assertions:

1. invalid request leaves L6 unchanged;
2. invalid request leaves visible version unchanged;
3. invalid request leaves allocator state unchanged;
4. invalid request leaves unresolved durable gate unchanged;
5. error display/debug includes branch/version facts but not value bytes.

### 2. Successful Replay

Required cases:

1. replayed put row becomes readable after visible publication;
2. replayed delete row installs a tombstone and hides latest value;
3. mixed replayed put/delete rows install atomically;
4. replayed user rows keep the WAL commit version;
5. replayed user rows keep the WAL commit timestamp;
6. replayed timeline rows keep the WAL commit version and timestamp;
7. timeline timestamp lookup resolves the replayed version after replay;
8. timeline version lookup resolves the replayed timestamp after replay;
9. replay outcome kind is `Replay`;
10. replay outcome phase is replay-specific or visible-after-replay according
    to the shipped outcome model;
11. replay outcome durability reflects the durable WAL record, not cache mode;
12. replay report counts checked rows and applied rows.

Assertions:

1. replay never calls normal commit batch stamping helpers;
2. replay never invokes read-set/CAS validation;
3. replay never appends a new WAL record;
4. visible version advances only after L6 install succeeds.

### 3. Conflict Bypass

Required cases:

1. replay of a newer durable row succeeds even if an ordinary read-set check
   would have seen a stale observed version;
2. replay of a durable delete succeeds without CAS facts;
3. replay does not inspect caller-provided validation facts if the request
   type carries any diagnostic validation metadata;
4. replay fails for same-internal-key conflicting facts through duplicate
   classification, not through normal conflict validation.

Assertions:

1. no conflict-error variant is returned for valid replay records;
2. branch guard or replay guard serialization does not call the normal
   unresolved durable admission gate.

### 4. Exact Duplicate Idempotency

Required cases:

1. replaying the same durable record twice succeeds.
2. second replay installs no new duplicate rows.
3. second replay still catches allocator version up if allocator is behind.
4. second replay still catches timestamp guard up if timestamp guard is behind.
5. second replay can publish visible version if rows are present but visible
   tracker is behind.
6. second replay clears an exact matching unresolved durable gate after visible
   publication.
7. idempotent replay reports `AlreadyApplied` or the equivalent shipped action.
8. idempotent replay preserves timeline lookup results.

Assertions:

1. exact duplicate comparison includes physical key, version, timestamp,
   tombstone flag, expiry, and value bytes;
2. exact duplicate comparison does not leak value bytes in failure output.

### 5. Mismatch And Partial State

Required cases:

1. existing row with same internal key but different value rejects;
2. existing row with same internal key but different timestamp rejects;
3. existing row with same internal key but different expiry rejects;
4. existing row with same internal key but different tombstone flag rejects;
5. user rows present but timeline rows missing rejects;
6. timeline rows present but user rows missing rejects;
7. subset of replay rows present rejects;
8. duplicate mismatch leaves allocator unchanged;
9. duplicate mismatch leaves visible version unchanged;
10. duplicate mismatch leaves unresolved durable gate unchanged.

Assertions:

1. partial state is not auto-repaired by L7K;
2. mismatch errors preserve lower-layer sources when L6 read fails.

### 6. Allocator And Timestamp Catch-Up

Required cases:

1. replay of version `N` makes the next normal allocated version greater than
   `N`.
2. replay of lower version after a higher catch-up does not regress allocator
   state.
3. replay of timestamp `T` makes generated timestamps stay at or above the
   allocator's monotonic floor.
4. replay of older timestamp after a higher catch-up does not regress timestamp
   guard state.
5. allocator catch-up runs for exact duplicate replay.
6. allocator catch-up does not run for invalid request, mismatch, or partial
   replay state.
7. no transaction-id catch-up API exists or is exercised in V1.

Assertions:

1. version catch-up uses `CommitFactAllocator::catch_up_to_recovered_version`;
2. timestamp catch-up uses `CommitFactAllocator::catch_up_to_recovered_timestamp`;
3. catch-up errors, if any, are classified as replay catch-up failures.

### 7. Visibility And Gate Reconciliation

Required cases:

1. empty unresolved gate allows replay.
2. exact matching unresolved gate allows replay.
3. different unresolved gate fact rejects before mutation.
4. exact gate clears after replay apply and visible publish.
5. exact gate clears after idempotent duplicate confirmation and visible
   publish.
6. L6 apply failure with a matching durable-not-applied gate leaves that gate
   unchanged.
7. L6 apply failure with an empty gate records a durable-not-applied fact for
   L8 health reporting.
8. visible publication failure does not clear the gate.
9. visible publication failure after replay apply returns an applied-not-visible
   replay error that L8 can turn into recovery health state.
10. visible publication failure with an empty gate records an applied-not-visible
   fact.
11. visible publication failure after applying rows with a matching
   durable-not-applied gate advances that gate to applied-not-visible.
12. visible version never advances past a failed replay.
13. visible version catch-up remains monotonic across repeated replay.

Assertions:

1. clear uses exact branch/version/timestamp/durability facts;
2. clear does not depend on user value bytes;
3. normal cache and durable commits remain blocked while the gate is set.

### 8. Error Source Chains

Required cases:

1. L6 read failure during duplicate classification is preserved as source.
2. L6 apply failure during replay is preserved as source.
3. visible publisher failure is preserved as source.
4. allocator catch-up failure, if represented by a typed error, preserves its
   source.
5. replay validation failures have no misleading lower-layer source.

Assertions:

1. `std::error::Error::source()` exposes the first lower-layer cause;
2. displays stay storage-shaped and value-free.

## L7M Generated Property Handoff

L7M should add a generated replay contract that drives bounded scripts with an
independent model that tracks:

1. branch rows by internal key;
2. commit timeline facts;
3. visible version;
4. version allocator floor;
5. timestamp floor;
6. unresolved durable gate state;
7. replay action counters.

Generated operations for L7M:

1. seed an ordinary applied commit;
2. seed an unresolved durable fact with no rows;
3. replay absent durable record;
4. replay exact duplicate durable record;
5. replay duplicate mismatch;
6. replay partial user rows;
7. replay partial timeline rows;
8. replay lower version after higher catch-up;
9. replay higher version after lower catch-up;
10. replay with empty, matching, and different gates.

Property assertions for L7M:

1. production visible reads match model reads after successful replay;
2. allocator floors match the model after successful or idempotent replay;
3. failed replay leaves production/model unchanged;
4. gate state matches the model after each replay;
5. replay counters cover every action class across the generated suite.

## Fault Windows

Fault tests should exercise protocol boundaries:

1. failure while reading existing rows for duplicate classification;
2. failure during L6 replay apply;
3. failure during visible publication after L6 apply;
4. failure while clearing a matching gate after visible publication;
5. allocator catch-up failure if the allocator exposes an injectable failure;
6. process-open mode with no gate object seeded;
7. in-process repair mode with gate seeded.

Required invariants:

1. no visible publication before L6 install/confirmation;
2. no gate clear before visible publication succeeds;
3. no allocator catch-up when L6 apply fails, mismatches, or partial state is
   detected;
4. applied-not-visible replay failure is explicit.

## Source Guards

`commit/replay.rs` and replay test helpers must not introduce imports from:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::object`;
4. `crate::service::wal` for scanning or listing WAL;
5. direct filesystem/path/environment APIs;
6. engine/product modules;
7. public transaction-session vocabulary;
8. JSON, graph, vector, search, event, embedding, remote, hub, or dataset
   terms.

Allowed replay production imports are limited to the format `WalRecord`, L6
branch boundaries, row facts, commit allocator/visibility/gate modules, and
standard library synchronization primitives already allowed by the parent L7
plan.

## Sensitivity Probes

Record probe results in
`docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md` when the
slice closes.

The suite should fail if a mutation:

1. replaces replay version with a newly allocated version;
2. replaces replay timestamp with a newly generated timestamp;
3. removes timeline-row validation;
4. moves visible publish before L6 apply;
5. treats partial rows as exact duplicate;
6. ignores duplicate value mismatch;
7. clears gate before visible publish;
8. clears a different gate fact;
9. omits allocator version catch-up;
10. omits timestamp catch-up;
11. calls normal conflict validation from replay;
12. logs value bytes in replay errors.

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
cargo test -p strata-storage-next --locked --lib commit::tests::visibility
```

## Exit Criteria

L7K testing is complete when:

1. direct tests cover request validation, success, idempotency, mismatch,
   partial state, allocator catch-up, visibility, and gate clear;
2. replay fault windows preserve the right phase and state invariants;
3. source guards prove replay does not scan WAL or import product layers;
4. no tests exist solely to prove planning-document links;
5. the L7 porting log records replay sensitivity probes and deferred items;
6. L7M has a clear generated-property handoff for replay action classes and
   allocator/gate state.
