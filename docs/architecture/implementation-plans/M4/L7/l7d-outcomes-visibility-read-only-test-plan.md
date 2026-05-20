# L7D Test Plan: Outcomes, Visibility, And Read-Only Path

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`

## Goal

Prove that L7D can report storage-shaped commit/read-only facts and maintain a
monotonic visible-version tracker without performing a mutating commit.

The suite must fail if L7D:

1. allocates a commit version for a read-only diagnostic batch;
2. reads a timestamp source for a read-only diagnostic batch;
3. mutates L6, appends WAL, or writes timeline rows;
4. reports read-only diagnostics as durable;
5. allows disabled read-only diagnostics to execute;
6. publishes visible versions backward;
7. publishes visibility from impossible allocated/durable/applied/timeline
   facts;
8. reports a visible mutating outcome before visibility facts support it;
9. hides durable-but-not-visible facts;
10. imports L6, WAL, backend/layout/filesystem, table internals, or
    engine/product transaction APIs.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/outcome.rs` for direct outcome and
   read-only diagnostic tests.
2. `crates/storage-next/src/commit/tests/visibility.rs` for direct
   visible-version tracker tests.
3. `crates/storage-next/src/commit/tests/scaffold.rs` only for shared shell
   assertions that remain relevant.
4. `crates/storage-next/src/testkit/commit_runtime.rs` or
   `crates/storage-next/src/testkit/commit_runtime_outcome.rs` for generated
   L7D contracts.
5. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7D
   counter assertions.
6. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary and forbidden-vocabulary checks.

Do not add tests that only prove planning documents exist or link to each
other. L7D automated tests should exercise outcome behavior, visibility
behavior, generated model coverage, or source boundaries.

## Direct Test Matrix

### 1. Read Snapshot Facts

Required cases:

1. default snapshot uses `CommitVersion::ZERO`;
2. snapshot preserves branch id;
3. snapshot preserves the visible version supplied by the tracker;
4. snapshot debug output does not mention product transactions or dump values;
5. snapshot construction does not require L6 state.

Assertions:

1. snapshot is a read fact, not a commit stamp;
2. snapshot does not claim durability;
3. snapshot does not claim timestamp-history completeness.

### 2. Visible Version Tracker Construction

Required cases:

1. default tracker starts at `CommitVersion::ZERO`;
2. tracker can be initialized from a recovered visible version;
3. initialization at `CommitVersion::MAX` is allowed for recovered state;
4. initialization never allocates a version;
5. initialization never reads a timestamp source.

Assertions:

1. no branch state is required;
2. no WAL service is required;
3. no backend/layout/table type is required.

### 3. Visible Version Monotonicity

Required cases:

1. publishing a greater visible version advances the tracker;
2. publishing the same version is idempotent;
3. publishing a lower version follows the chosen contract: no-op or typed
   invalid state;
4. repeated greater publishes remain monotonic;
5. publishing `CommitVersion::ZERO` over a nonzero visible version cannot
   regress the tracker.

Assertions:

1. visible version never decreases;
2. visible publication does not allocate;
3. visible publication does not mutate L6.

### 4. Visibility Fact Validation

Required cases:

1. allocated-only facts are valid but not visible;
2. durable can equal allocated;
3. durable cannot exceed allocated;
4. applied can equal allocated;
5. applied cannot exceed allocated;
6. visible can equal applied;
7. visible cannot exceed applied;
8. timeline can equal applied;
9. timeline cannot exceed applied;
10. visible cannot exceed timeline when timeline is present;
11. absent allocated with present durable/applied facts is rejected.

Assertions:

1. errors are typed commit-runtime errors;
2. errors do not leak lower-layer implementation details;
3. validation is independent of the visible tracker.

### 5. Commit Mutation Counts

Required cases:

1. read-only counts are all zero;
2. mixed put/delete batch counts puts and deletes separately;
3. delete-only batch reports zero puts and nonzero deletes;
4. timeline count remains zero before L7G;
5. count construction rejects or cannot represent values above L7B limits.

Assertions:

1. counts do not inspect value bytes;
2. counts are derived from validated batch shape;
3. counts do not stamp rows.

### 6. Read-Only Diagnostic Execution

Required cases:

1. enabled read-only diagnostics return a read-only outcome;
2. outcome branch equals the batch branch;
3. outcome read snapshot equals current tracker visible version;
4. no commit version is present;
5. no commit timestamp is present;
6. mutation counts are zero;
7. durability class is `NotDurable`;
8. visibility facts do not report a newly visible commit;
9. explicit timestamp policy is ignored because no timestamp is allocated;
10. durable option is ignored or normalized to non-durable read-only facts;
11. mutating batches are rejected by the read-only executor.

Assertions:

1. read-only execution does not call `CommitFactAllocator`;
2. read-only execution does not call a timestamp source;
3. read-only execution does not mutate L6;
4. read-only execution does not append WAL.

### 7. Read-Only Diagnostics Disabled

Required cases:

1. disabled config rejects read-only execution;
2. rejection happens before any visible tracker mutation;
3. rejection does not allocate a version;
4. rejection does not read timestamp source;
5. rejection error uses commit-runtime vocabulary.

Assertions:

1. disabled diagnostics cannot be bypassed through durable options;
2. disabled diagnostics cannot be bypassed through explicit timestamp policy;
3. disabled diagnostics do not return partial outcome facts.

### 8. Outcome Constructors

Required cases:

1. read-only outcome constructor rejects commit version/timestamp fields;
2. visible mutating outcome requires commit version and timestamp;
3. visible mutating outcome requires visible facts that include the commit
   version;
4. not-visible outcome does not report `Visible` phase;
5. durable-but-not-visible outcome preserves durable class;
6. durable-but-not-visible outcome keeps visible version below the commit
   version or absent;
7. replay outcome can preserve original commit facts without allocating;
8. invalid phase/durability/visibility combinations are rejected.

Assertions:

1. outcome constructors are the only path to construct nontrivial outcome
   facts;
2. outcome debug output is bounded and value-free;
3. outcome does not expose public API DTOs.

### 9. Stats Interaction

Required cases:

1. if L7D adds stats recording, read-only execution increments only
   read-only counters;
2. disabled read-only diagnostics increment rejected counters only if a stats
   recorder is explicitly part of L7D;
3. visible tracker publication alone does not increment committed counters;
4. durable-but-not-visible counters are reserved for L7J.

Assertions:

1. stats behavior is explicit;
2. there is no hidden global mutable stats state.

### 10. Cross-Branch Visibility Policy

Required cases:

1. global tracker visible version is branch-neutral;
2. read-only outcome includes target branch id separately from visible
   version;
3. read-only outcome on branch A does not change branch B facts;
4. publishing a visible version for one future branch commit cannot be confused
   with a branch id;
5. policy comments/tests state that L6 remains branch-isolated.

Assertions:

1. branch id is never encoded into `CommitVersion`;
2. visible-version facts remain global in V1;
3. per-branch visible-version support is not added accidentally.

## Generated Testkit Matrix

Extend the commit-runtime property harness with counters for:

1. read-only outcome success;
2. read-only disabled rejection;
3. visible tracker initialization;
4. visible tracker monotonic publish;
5. lower-version publish behavior;
6. invalid visibility facts;
7. outcome constructor rejection;
8. mutation count facts;
9. cross-branch read-only facts;
10. no-allocation proof for read-only diagnostics.

The generated harness should vary:

1. branch id;
2. visible version floor;
3. attempted publish versions below/equal/above floor;
4. read-only diagnostic options;
5. disabled/enabled diagnostics;
6. malformed outcome fact combinations.

The generated harness must not:

1. call L6 branch mutation;
2. append WAL;
3. construct timeline rows;
4. use engine/product DTOs;
5. assert that documentation files exist.

## Source Guards

The existing commit-runtime source guard should continue to reject:

1. `pub mod commit`;
2. public `pub` commit-runtime type/function leaks;
3. product transaction vocabulary;
4. `VersionedValue`/product value/key vocabulary;
5. `crate::branch` imports;
6. `crate::service::wal` imports;
7. `crate::format::wal` imports;
8. `crate::backend`, `crate::layout`, `crate::object`, or `crate::table`
   imports;
9. `std::fs`, `std::path`, `std::env`, `SystemTime`, `Instant::now`, and
   `Timestamp::now`.

Add guard examples only if L7D introduces new vocabulary that could bypass the
existing checks.

## Sensitivity Probes

Record probe results in `m4-l7-porting-log.md` during implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| S1 | Read-only execution calls the allocator. | Read-only no-allocation direct/generated tests. |
| S2 | Read-only execution calls timestamp source. | Failing-source read-only test. |
| S3 | Disabled read-only diagnostics execute. | Disabled diagnostics direct/generated tests. |
| S4 | Visible tracker allows regression. | Monotonic visibility direct/generated tests. |
| S5 | Visible tracker publishes allocated-only facts. | Visibility fact validation tests. |
| S6 | Outcome marks not-visible facts as visible. | Outcome constructor tests. |
| S7 | Read-only outcome reports durability. | Read-only outcome tests. |
| S8 | Mutating batch enters read-only executor. | Mutating-rejection test. |
| S9 | Outcome debug dumps value bytes or product types. | Debug/source vocabulary tests. |
| S10 | Outcome/visibility imports L6, WAL, backend, layout, table, fs, or engine code. | Source guard. |

## Verification Commands

Run at least:

```text
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run broader package tests if L7D edits shared commit fact validation used by
non-commit tests.

## Exit Gate

L7D test work is complete when:

1. direct tests cover outcome construction, read snapshots, and visible tracker
   behavior;
2. direct tests prove read-only diagnostics allocate nothing and touch no
   timestamp source;
3. disabled read-only diagnostics reject cleanly;
4. impossible visibility and outcome facts are rejected;
5. generated tests exercise every L7D counter category;
6. source guards reject boundary regressions;
7. no tests assert that docs exist or link to each other;
8. no tests require localfs, WAL services, L6 branch mutation, timeline rows,
   backend IO, or engine code;
9. the verification commands pass.
