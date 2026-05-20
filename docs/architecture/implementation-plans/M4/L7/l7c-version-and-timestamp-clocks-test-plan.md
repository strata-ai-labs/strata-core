# L7C Test Plan: Version And Timestamp Clocks

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`

## Goal

Prove that L7C allocates storage-owned commit facts correctly and narrowly:
one nonzero commit version and one commit timestamp for each mutating validated
batch, no allocation for read-only diagnostic batches, no transaction IDs, and
no contact with L6, WAL, timeline rows, backend, layout, filesystem, or engine
code.

The suite must fail if L7C:

1. allocates `CommitVersion::ZERO`;
2. wraps `CommitVersion::MAX`;
3. ignores recovered-version catch-up;
4. consumes a version when timestamp resolution fails before allocation;
5. moves generated timestamps backward within one runtime;
6. rewrites invalid explicit timestamps instead of rejecting them;
7. rejects equal timestamps even though timeline uses commit-version
   tiebreaking;
8. allocates any fact for read-only diagnostic batches;
9. adds durable storage transaction-id allocation;
10. imports L6, WAL services, backend/layout/filesystem, table internals, or
    engine/product vocabulary into the allocator.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/allocator.rs` for direct allocator and
   timestamp-source tests.
2. `crates/storage-next/src/commit/tests/scaffold.rs` for any existing fact
   shell assertions that remain shared.
3. `crates/storage-next/src/testkit/commit_runtime.rs` or
   `crates/storage-next/src/testkit/commit_runtime/` for generated allocator
   contracts.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7C
   counter assertions.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary and forbidden-vocabulary checks.

Do not add tests that only prove planning documents exist or link to each
other. L7C automated tests should exercise allocator behavior, generated model
coverage, or source boundaries.

## Direct Test Matrix

### 1. Version Allocator Construction

Required cases:

1. default allocator starts at `CommitVersion::ZERO`;
2. allocator initialized from `CommitVersion::ZERO` allocates version 1 first;
3. allocator initialized from a recovered floor `N` allocates `N + 1` first;
4. allocator initialized from `CommitVersion::MAX` is constructible for
   recovery state but cannot allocate another version;
5. debug output does not expose product or transaction-session vocabulary.

Assertions:

1. no branch state is required to construct the allocator;
2. no WAL service is required to construct the allocator;
3. no timestamp source is read during version allocator construction.

### 2. Monotonic Version Allocation

Required cases:

1. first allocation after zero returns version 1;
2. repeated allocations are strictly increasing;
3. allocated versions are not required to be dense after simulated later-slice
   failures;
4. allocator state reports the last allocated version;
5. `CommitVersion::ZERO` is never returned.

Assertions:

1. every allocation returns a typed `CommitVersion`;
2. no row stamping happens inside the version allocator;
3. no visible-version fact is published by L7C.

### 3. Version Overflow

Required cases:

1. allocator at `CommitVersion::MAX` returns a typed overflow error;
2. overflow does not wrap to zero;
3. overflow does not move allocator state;
4. repeated allocation attempts after overflow keep returning overflow;
5. display text is bounded and does not mention public transactions.

### 4. Version Catch-Up

Required cases:

1. catch-up to a greater recovered version advances the floor;
2. next allocation after catch-up returns recovered + 1;
3. catch-up to an equal version is a no-op;
4. catch-up to a lower version is a no-op;
5. catch-up to `CommitVersion::ZERO` is a no-op;
6. catch-up to `CommitVersion::MAX` succeeds, and the next allocation
   overflows;
7. catch-up does not set durable/applied/visible/timeline facts.

Assertions:

1. catch-up is idempotent;
2. catch-up does not allocate a transaction id;
3. catch-up does not imply replay has installed rows.

### 5. Timestamp Source

Required cases:

1. deterministic/manual timestamp source returns the configured timestamp;
2. sequence source returns timestamps in script order;
3. source failure returns a typed commit-runtime error;
4. source failure preserves lower-layer source where applicable;
5. optional system-clock source is isolated behind the source abstraction and
   is not required for no-default-features wasm checks.

Assertions:

1. `Timestamp` itself is not modified to read clocks;
2. timestamp source tests do not rely on wall-clock sleep;
3. source errors do not dump product values or row bytes.

### 6. Monotonic Timestamp Guard

Required cases:

1. first generated timestamp is accepted as-is;
2. generated timestamp greater than the last allocated timestamp is accepted
   as-is;
3. generated timestamp equal to the last allocated timestamp is accepted;
4. generated timestamp less than the last allocated timestamp is clamped to the
   last allocated timestamp;
5. timestamp guard state advances to the guarded value;
6. timestamp catch-up advances the guard floor;
7. lower timestamp catch-up is a no-op.

Assertions:

1. equal timestamps are valid because L7G timeline keys include commit version;
2. generated timestamps never move backward in one runtime;
3. the guard does not claim timestamp-history completeness or retention
   coverage.

### 7. Explicit Timestamp Policy

Required cases:

1. explicit timestamp greater than the guard floor is accepted unchanged;
2. explicit timestamp equal to the guard floor is accepted unchanged;
3. explicit timestamp less than the guard floor is rejected;
4. rejected explicit timestamp does not allocate a version;
5. explicit `Timestamp::EPOCH` is accepted as a commit timestamp unless a
   future policy explicitly changes this;
6. accepted explicit timestamp updates the guard floor when it is greater.

Assertions:

1. explicit timestamps are not silently clamped;
2. explicit timestamps are not replaced with wall-clock time;
3. explicit policy errors are typed.

### 8. Commit Fact Allocation For Mutating Batches

Required cases:

1. valid single-put batch allocates one stamp;
2. valid mixed put/delete batch allocates one stamp;
3. every stamp branch equals the validated batch branch;
4. every stamp version is nonzero;
5. every stamp timestamp is the guarded timestamp;
6. two mutating allocations get increasing versions;
7. two mutating allocations may get equal timestamps;
8. allocation does not stamp user rows; L7B stamping remains a separate call.

Assertions:

1. timestamp resolution happens before version consumption;
2. invalid timestamp policy leaves allocator version unchanged;
3. source failure leaves allocator version unchanged;
4. no L6/WAL/timeline behavior is triggered.

### 9. Read-Only No-Allocation

Required cases:

1. read-only diagnostic batch returns a no-allocation outcome;
2. read-only diagnostic batch does not read timestamp source;
3. read-only diagnostic batch does not advance version allocator;
4. read-only diagnostic batch does not advance timestamp guard;
5. read-only diagnostic batch with explicit timestamp policy still allocates
   nothing;
6. read-only diagnostic batch with durable option still allocates nothing in
   L7C; L7D/L7E own option-level execution rejection.

Assertions:

1. read-only path does not create a `CommitStamp`;
2. read-only path does not mutate L6;
3. read-only path does not append WAL.

### 10. Invalid Input Boundaries

Required cases:

1. allocation rejects or cannot accept an unvalidated mutating batch;
2. allocation cannot construct a stamp with version zero;
3. branch mismatch between allocation request and stamp is impossible through
   the allocator API;
4. allocation failure does not create partial allocation output;
5. invalid L7B batch shapes remain L7B-owned and are not revalidated by
   duplicating all batch validation in L7C.

### 11. No Transaction IDs

Required checks:

1. production `src/commit/` has no `TxnId`;
2. production `src/commit/` has no `TransactionId`;
3. production `src/commit/` has no transaction-id allocator;
4. production `src/commit/` has no transaction-id catch-up hook;
5. error variants do not mention transaction-id overflow.

These checks belong in `commit_runtime_source_guard.rs`, not in documentation
closeout tests.

## Generated Test Matrix

Extend the generated commit runtime contract so script bytes exercise:

1. initial version floor in `[0, 255]`;
2. recovered catch-up floor lower/equal/greater than current floor;
3. overflow floor near `CommitVersion::MAX`;
4. runtime-generated timestamp sequences that increase, repeat, and decrease;
5. explicit timestamp policy above/equal/below the guard floor;
6. timestamp source failure before version allocation;
7. read-only diagnostic allocation route;
8. mutating batch allocation route;
9. no transaction-id surface checks.

Generated assertions must include nonzero counters for:

1. version allocation cases;
2. version catch-up cases;
3. version overflow cases;
4. generated timestamp cases;
5. clamped timestamp cases;
6. explicit timestamp cases;
7. invalid explicit timestamp cases;
8. timestamp source failure cases;
9. read-only no-allocation cases;
10. no transaction-id checks.

## Source Guard Matrix

Extend `commit_runtime_source_guard.rs` to fail on:

1. `TxnId`;
2. `TransactionId`;
3. `next_txn_id`;
4. `transaction_id`;
5. `crate::branch` imports from allocator code;
6. `crate::service::wal` imports from allocator code;
7. `crate::format::wal` imports from allocator code;
8. `crate::backend` imports;
9. `crate::layout` imports;
10. `crate::table` imports;
11. engine crate imports;
12. public `pub` allocator surface;
13. process-global mutable clock state.

Allowed terms include:

1. `CommitVersion`;
2. `Timestamp`;
3. `CommitStamp`;
4. `CommitTimestampPolicy`;
5. `pub(crate)`;
6. deterministic/manual timestamp source names in test or testkit code.

## Sensitivity Probes

Record these in the L7 porting log when implementation lands:

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Return `CommitVersion::ZERO` from first allocation. | Direct version allocation and generated allocation tests fail. |
| S2 | Use wrapping add at `CommitVersion::MAX`. | Overflow tests fail. |
| S3 | Ignore catch-up to recovered max version. | Catch-up direct and generated tests fail. |
| S4 | Consume version before timestamp source failure. | No-gap-on-source-failure test fails. |
| S5 | Clamp explicit timestamp below floor instead of rejecting. | Explicit timestamp rejection test fails. |
| S6 | Allow generated timestamp to move backward. | Monotonic guard test fails. |
| S7 | Reject equal timestamps. | Equal timestamp direct/generated tests fail. |
| S8 | Allocate a stamp for read-only diagnostic batch. | Read-only no-allocation tests fail. |
| S9 | Add transaction-id allocator or catch-up. | Source guard fails. |
| S10 | Import L6, WAL, backend, layout, or engine code into allocator. | Source guard fails. |

## Verification Commands

Run after implementation:

```bash
cargo test -p strata-storage-next --locked --lib commit
cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## Exit Gate

L7C test work is complete when:

1. direct tests pin version allocation, overflow, and catch-up;
2. direct tests pin timestamp source, guard, explicit policy, and source
   failure;
3. direct tests prove read-only no-allocation;
4. generated tests exercise every L7C counter category;
5. source guards reject transaction-id and boundary regressions;
6. no test asserts that docs exist or link to each other;
7. no test requires localfs, WAL services, L6 branch mutation, or engine code;
8. the verification commands pass.
