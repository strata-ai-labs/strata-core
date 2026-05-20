# M4-L7 Porting Log

Status: active

Parent implementation plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## L7A: Commit Runtime Scaffold

### Source Evidence Read

1. `docs/architecture/storage-next/l7-commit-runtime.md`
2. `docs/architecture/storage-next/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage-next/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage-next/commit-timeline-substrate.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-implementation-plan.md`
8. `docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-test-plan.md`
9. `crates/storage-next/src/commit/mod.rs`
10. `crates/storage-next/src/branch/mod.rs`
11. `crates/storage-next/src/format/wal.rs`
12. `crates/storage-next/src/service/wal.rs`
13. `crates/storage/src/txn/context.rs`
14. `crates/storage/src/txn/manager.rs`
15. `crates/storage/src/txn/validation.rs`
16. `crates/storage/src/txn/lock_ordering.rs`
17. `crates/storage/src/durability/commit_adapter.rs`
18. `crates/storage/src/durability/payload.rs`

### Preserved As Vocabulary

1. Commit runtime configuration is separate from branch, table, WAL, and
   lifecycle configuration.
2. Commit errors are typed, phase-oriented, and preserve lower-layer source
   chains.
3. Commit visibility facts keep allocated, durable, applied, visible, and
   timeline versions distinct.
4. Durability facts distinguish non-durable, standard, always, and uncertain
   outcomes.
5. Read-only diagnostics are internal helpers and allocate no commit version
   in later slices.

### Intentionally Changed Or Retired

1. Public storage transaction sessions remain retired from storage-next V1.
2. Durable storage transaction ids remain retired from storage-next V1.
3. L7A uses `CommitReadOnlyDiagnostics` instead of a boolean control field.
4. L7A does not port old WAL bytes or old payload builders.
5. L7A keeps all production commit runtime items crate-private.

### Deferred By Owner Slice

1. `L7B`: `CommitBatch`, `CommitMutation`, duplicate-key policy, and row
   stamping.
2. `L7C`: commit-version allocator and timestamp source.
3. `L7D`: outcomes, visible-version tracker, and read-only diagnostic path
   behavior.
4. `L7E`: branch registry, branch generation guard, per-branch commit guard,
   and quiesce skeleton.
5. `L7F`: read-set and CAS conflict validation over L6 read views.
6. `L7G`: timeline row construction and lookup substrate.
7. `L7H`: cache/no-WAL commit apply into L6.
8. `L7I`: `WalRecord` construction and `WalRecordEnvelope` append through L4.
9. `L7J`: durable-but-not-visible classification and write gate.
10. `L7K`: replay and commit-version allocator catch-up hooks.
11. `L7L`: concurrency, quiesce, lock-order, and scheduler/loom hardening.
12. `L7M`: generated/fuzz/fault depth beyond the scaffold route.
13. `L7N`: closeout inventory, full command evidence, and sensitivity ledger.

### Tests And Guards Added

1. `crates/storage-next/src/commit/config.rs`
2. `crates/storage-next/src/commit/error.rs`
3. `crates/storage-next/src/commit/facts.rs`
4. `crates/storage-next/src/commit/result.rs`
5. `crates/storage-next/src/commit/tests.rs`
6. `crates/storage-next/src/testkit/commit_runtime.rs`
7. `crates/storage-next/tests/commit_runtime_properties.rs`
8. `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probes

Planned L7A probes:

1. Add an engine import to production commit code; `commit_runtime_source_guard`
   must fail.
2. Add a table-internal import to production commit code;
   `commit_runtime_source_guard` must fail.
3. Add a backend/layout import to production commit code;
   `commit_runtime_source_guard` must fail.
4. Add public transaction-session vocabulary to production commit code;
   `commit_runtime_source_guard` must fail.
5. Add durable transaction-id vocabulary to production commit code;
   `commit_runtime_source_guard` must fail.
6. Add filesystem/path usage to production commit code;
   `commit_runtime_source_guard` must fail.
7. Change a `pub(crate)` production item to bare `pub`;
   `commit_runtime_source_guard` must fail.
8. Allow zero `max_mutations_per_batch`; commit module tests and generated
   scaffold route must fail.
9. Allow visible version greater than applied version; commit module tests and
   generated scaffold route must fail.
10. Remove one scaffold outcome counter; `commit_runtime_properties` must fail.

### Command Evidence

Verified for L7A:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## L7B: Commit Batch And Mutation Model

### Source Evidence Read

1. `docs/architecture/storage-next/l7-commit-runtime.md`
2. `docs/architecture/storage-next/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage-next/commit-timeline-substrate.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-test-plan.md`
8. `crates/storage-next/src/commit/`
9. `crates/storage-next/src/row/mod.rs`
10. `crates/storage/src/txn/context.rs`
11. `crates/storage/src/txn/validation.rs`
12. `crates/storage/src/traits.rs`

### Preserved As Behavior

1. One internal commit batch targets one branch by default.
2. Put and delete intents remain buffered until a later commit path stamps and
   applies them.
3. Delete intent becomes a tombstone row at commit time.
4. Put value bytes are opaque storage payloads and are not interpreted by L7.
5. Read-set and CAS facts remain separate validation inputs.
6. Missing observed versions use an explicit `Missing` fact instead of the old
   version-zero sentinel at the L7 boundary.
7. Append/keep-last write behavior is retained as a storage retention hint, not
   as immediate pruning.

### Intentionally Changed Or Retired

1. L7B does not expose public transaction contexts or public begin/commit/
   rollback sessions.
2. L7B does not store product `Key`, `Value`, `VersionedValue`, JSON, graph,
   vector, search, or entity concepts.
3. L7B rejects duplicate physical keys in one validated mutation batch instead
   of using last-write-wins.
4. L7B rejects caller-supplied storage-owned spaces, including the commit
   timeline space. Timeline rows are generated by L7G.
5. L7B rejects `CommitObservedVersion::Present(CommitVersion::ZERO)` and uses
   `Missing` for absent rows.
6. L7B rejects expiry-at-epoch because `StorageRow` uses `Timestamp::EPOCH` as
   the no-expiry sentinel.

### Deferred By Owner Slice

1. `L7C`: commit-version allocation, timestamp allocation, monotonic timestamp
   guard, and allocator catch-up shape.
2. `L7D`: read-only diagnostic execution and visible-version facts.
3. `L7E`: branch registry, branch-generation guard, per-branch write guard,
   and quiesce skeleton.
4. `L7F`: live read-set and CAS validation over L6 read views.
5. `L7G`: timeline row generation and timestamp/version lookup.
6. `L7H`: cache/no-WAL apply into L6 and visibility publication.
7. `L7I`: V1 WAL payload row/byte cap checks, `WalRecord` construction, and
   `WalRecordEnvelope` append.
8. `L7J`: durable-but-not-visible write gate.
9. `L7K`: replay-specific stamping and conflict bypass.
10. `L7M`: full generated/fuzz/fault depth beyond the batch route.
11. `L7N`: closeout inventory and full sensitivity ledger.

### Tests And Guards Added

1. `crates/storage-next/src/commit/batch.rs`
2. Batch direct tests in `crates/storage-next/src/commit/tests/`
3. Batch generated counters in `crates/storage-next/src/testkit/commit_runtime.rs`
4. Batch counter assertions in `crates/storage-next/tests/commit_runtime_properties.rs`
5. L7B source-boundary additions in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probes

Planned L7B probes:

1. Accept an empty mutating batch; direct invalid-batch and generated route
   must fail.
2. Skip branch validation for mutation keys; branch-mismatch tests must fail.
3. Skip branch validation for validation facts; validation-fact mismatch tests
   must fail.
4. Allow caller-supplied timeline keys; storage-owned-space tests must fail.
5. Allow duplicate put/delete keys; duplicate mutation tests must fail.
6. Stamp deletes as non-tombstones; stamping invariant tests must fail.
7. Stamp rows with the wrong commit timestamp; stamping invariant tests must
   fail.
8. Reorder mutation rows during stamping; stamping order tests must fail.
9. Dump value bytes in debug/error output; vocabulary tests must fail.
10. Import `crate::branch`, `crate::format::wal`, or `crate::service::wal` from
    production commit code; source guard must fail.

### Command Evidence

Verified for L7B:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## L7C: Version And Timestamp Clocks

### Source Evidence Read

1. `docs/architecture/storage-next/l7-commit-runtime.md`
2. `docs/architecture/storage-next/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-test-plan.md`
7. `crates/storage-next/src/commit/`
8. `crates/core-next/src/version.rs`
9. `crates/core-next/src/time.rs`
10. `crates/storage/src/txn/manager.rs`
11. `crates/storage/src/segmented/mod.rs`

### Preserved As Behavior

1. Commit versions are storage-owned, global, nonzero, and monotonically
   increasing.
2. Version allocation uses typed overflow behavior at `CommitVersion::MAX`
   instead of wrapping.
3. Recovery catch-up advances the allocator floor above recovered versions.
4. Version gaps are allowed after later post-allocation failures.
5. Mutating commit fact allocation produces one `CommitStamp` carrying one
   version and one timestamp.
6. Runtime-generated timestamps are monotonic nondecreasing within one runtime.
7. Equal timestamps are allowed; L7G timeline ordering uses commit version as
   the deterministic tiebreaker.

### Intentionally Changed Or Retired

1. L7C does not port durable storage transaction ids from the old transaction
   manager.
2. L7C has no transaction-id allocator and no transaction-id catch-up hook.
3. Version availability is preflighted before timestamp resolution, then
   timestamp resolution happens before version consumption. This avoids both
   reading a timestamp source when the version allocator is exhausted and
   creating avoidable version gaps on timestamp-source failure.
4. Explicit timestamps below the monotonic floor are rejected instead of being
   silently clamped.
5. The production timestamp source is an abstraction; no direct clock read is
   embedded in `Timestamp`.
6. L7C still does not stamp rows, apply L6 state, append WAL, publish
   visibility, or write timeline rows.

### Deferred By Owner Slice

1. `L7D`: read-only diagnostic execution and visible-version facts.
2. `L7E`: branch registry, branch-generation guard, per-branch write guard,
   and quiesce skeleton.
3. `L7F`: no-allocation-on-conflict validation over L6 read views.
4. `L7G`: timeline row generation and duplicate-timestamp lookup tiebreaking.
5. `L7H`: cache/no-WAL apply into L6 and version-gap behavior after apply
   failure.
6. `L7I`: WAL record construction from stamped rows.
7. `L7K`: replay entrypoints that call version and timestamp catch-up after
   durable facts are recovered.
8. `L7M`: broader generated/fuzz/fault scripts over the full commit protocol.
9. `L7N`: closeout inventory and full sensitivity ledger.

### Tests And Guards Added

1. `crates/storage-next/src/commit/allocator.rs`
2. Allocator direct tests in `crates/storage-next/src/commit/tests/allocator.rs`
3. Allocator generated checks in
   `crates/storage-next/src/testkit/commit_runtime_allocator.rs`
4. Allocator counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
5. Transaction-id vocabulary additions in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probes

Planned L7C probes:

1. Return `CommitVersion::ZERO` from first allocation; direct and generated
   allocation tests must fail.
2. Wrap from `CommitVersion::MAX` to zero or read the timestamp source after
   overflow is already known; overflow tests must fail.
3. Ignore recovered-version catch-up; catch-up tests must fail.
4. Consume a version before timestamp source failure; source-failure tests must
   fail.
5. Clamp explicit timestamps below the floor instead of rejecting; explicit
   timestamp tests must fail.
6. Allow generated timestamps to move backward; monotonic guard tests must
   fail.
7. Reject equal timestamps; equal timestamp tests must fail.
8. Allocate a stamp for a read-only diagnostic batch; read-only no-allocation
   tests must fail.
9. Add transaction-id allocator or catch-up vocabulary; source guard must fail.
10. Import L6, WAL, backend, layout, or engine code into the allocator; source
    guard must fail.

### Command Evidence

Verified for L7C:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`
