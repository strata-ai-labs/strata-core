# M4-L7 Porting Log

Status: active

Parent implementation plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Parent test plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## L7A: Commit Runtime Scaffold

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
4. `docs/architecture/storage/commit-timeline-substrate.md`
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
11. `L7L`: concurrency, quiesce, lock-order, and deterministic guard
    interleaving hardening.
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

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
3. `docs/architecture/storage/commit-timeline-substrate.md`
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

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
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

## L7D: Outcomes, Visibility, And Read-Only Path

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7d-outcomes-visibility-read-only-test-plan.md`
7. `crates/storage-next/src/commit/`
8. `crates/storage/src/txn/manager.rs`
9. `crates/storage/src/txn/context.rs`
10. `crates/storage/src/segmented/tests/batch.rs`

### Preserved As Behavior

1. Read-only diagnostic work allocates no commit version.
2. Read-only diagnostic work reads no timestamp source.
3. Visible version is tracked separately from allocated/durable/applied facts.
4. Visible version starts at `CommitVersion::ZERO` for empty runtime state.
5. Visible version publication is monotonic and idempotent at the same version.
6. Durable-but-not-visible facts are representable without publishing
   visibility.
7. Commit outcomes are storage-shaped and do not contain product transaction
   session vocabulary.

### Intentionally Changed Or Retired

1. L7D does not port public read-only transaction sessions from the old
   transaction context.
2. L7D uses a single global visible-version tracker for V1 because commit
   versions are globally ordered.
3. Regressing visible-version publication returns a typed error instead of
   silently applying a `fetch_max` no-op.
4. Read-only diagnostics disabled by config return a typed phase error.
5. L7D does not mutate stats implicitly; stats remain explicit facts.
6. L7D still does not apply rows into L6, construct timeline rows, append WAL,
   acquire branch guards, or replay durable records.

### Deferred By Owner Slice

1. `L7E`: branch registry, branch generation, branch deletion, and commit
   guards.
2. `L7F`: read-set and CAS validation over L6 read views.
3. `L7G`: timeline row construction and lookup.
4. `L7H`: cache/no-WAL mutating commit path and real visibility publication
   after L6 apply.
5. `L7I`: WAL-backed durable commit outcomes.
6. `L7J`: write gates for durable-but-not-visible failures.
7. `L7K`: replay hooks that publish visible version after recovered rows are
   installed.
8. `L7M`: generated/fuzz/fault scripts over the full commit protocol.
9. `L7N`: closeout inventory and full sensitivity ledger.

### Tests And Guards Added

1. `crates/storage-next/src/commit/outcome.rs`
2. `crates/storage-next/src/commit/visibility.rs`
3. Outcome direct tests in `crates/storage-next/src/commit/tests/outcome.rs`
4. Visible-version direct tests in
   `crates/storage-next/src/commit/tests/visibility.rs`
5. Generated outcome/visibility checks in
   `crates/storage-next/src/testkit/commit_runtime_outcome.rs`
6. Generated counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
7. Existing commit-runtime source guards continue to cover the new modules.

### Sensitivity Probes

Planned L7D probes:

1. Read-only diagnostic allocates a version; read-only outcome and generated
   no-allocation tests must fail.
2. Read-only diagnostic reads timestamp source; function-signature and
   failing-source allocator tests keep this isolated.
3. Disabled read-only diagnostics execute; disabled direct/generated tests
   must fail.
4. Visible tracker allows regression; monotonic direct/generated tests must
   fail.
5. Visible tracker publishes from allocated-only facts; visibility fact tests
   must fail.
6. Outcome marks not-visible facts as visible; constructor tests must fail.
7. Read-only outcome reports durability; read-only outcome tests must fail.
8. Mutating batch enters read-only executor; mutating-rejection test must fail.
9. Outcome accepts a stamp from a different branch; branch-mismatch test must
   fail.
10. Import L6, WAL, backend, layout, table, filesystem, or engine code into
    outcome/visibility modules; source guard must fail.

### Command Evidence

Verified for L7D:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## L7E: Branch Registry And Commit Guards

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-test-plan.md`
6. `crates/storage-next/src/commit/`
7. `crates/storage/src/txn/manager.rs`
8. `crates/storage/src/txn/lock_ordering.rs`
9. `crates/engine/src/database/transaction.rs`

### Preserved As Behavior

1. Branch commit admission is separated from version allocation, timestamp
   allocation, WAL append, and L6 apply.
2. Missing branches reject before allocation.
3. Deleting or deleted branches reject before allocation.
4. Optional branch-generation facts are enforced exactly when supplied.
5. Same-branch mutating guard acquisition is serialized.
6. Different-branch mutating guards can be live at the same time.
7. Quiesce blocks new mutating guard acquisition.
8. Guard tokens release through RAII on clean and error paths.
9. Read-only diagnostics remain outside the mutating guard path.

### Intentionally Changed Or Retired

1. L7E does not port public branch lifecycle commands from engine code.
2. L7E does not port public transaction sessions or transaction ids from the
   old transaction manager.
3. L7E keeps quiesce nonblocking. L7L owns deterministic guard/quiesce
   coverage, while L8 owns retry and caller-level deadline behavior.
4. L7E treats branch generation as an optional storage fact. L9 owns public
   branch-id reuse semantics; L7E only rejects stale exact facts when supplied.
5. L7E stores registry descriptors in explicit runtime state, not process-global
   state.
6. L7E still does not validate read-set/CAS conflicts, construct timeline rows,
   apply L6 rows, append WAL, publish visibility, or replay durable records.

### Deferred By Owner Slice

1. `L7F`: read-set and CAS validation over L6 read views.
2. `L7G`: timeline row construction and lookup.
3. `L7H`: cache/no-WAL apply into L6 after admission.
4. `L7I`: WAL append after admission and before L6 apply.
5. `L7J`: durable-but-not-visible write gate.
6. `L7K`: replay and allocator catch-up.
7. `L7L`: deterministic guard/quiesce interleavings and runtime ordering
   hardening.
8. `L8`: checkpoint/recovery orchestration that uses quiesce.
9. `L9`: public branch lifecycle API and branch-generation ownership.

### Tests And Guards Added

1. `crates/storage-next/src/commit/branch_registry.rs`
2. `crates/storage-next/src/commit/guard.rs`
3. Branch registry direct tests in
   `crates/storage-next/src/commit/tests/branch_registry.rs`
4. Guard/quiesce direct tests in
   `crates/storage-next/src/commit/tests/guard.rs`
5. Generated branch-guard checks in
   `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs`
6. Generated counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
7. Existing commit-runtime source guards continue to cover the new production
   modules.

### Sensitivity Probes

Planned L7E probes:

1. Treat missing branch as active; missing-branch direct/generated tests must
   fail.
2. Ignore deleting/deleted markers; branch-not-writable tests must fail.
3. Ignore supplied generation mismatch; generation-mismatch tests must fail.
4. Treat stale generation after recreate as valid; recreate-generation tests
   must fail.
5. Allow same-branch double guard; guard serialization tests must fail.
6. Leak guard token on drop; reacquire-after-drop tests must fail.
7. Allow mutating guard during quiesce; quiesce-blocking tests must fail.
8. Start quiesce while mutating guards are active; active-guard quiesce tests
   must fail.
9. Route read-only diagnostics through the mutating guard path; read-only
   during quiesce tests must fail.
10. Import L6, WAL, backend, layout, table internals, filesystem, or engine code
    into registry/guard modules; source guard must fail.

### Command Evidence

Verified for L7E:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## L7F: Conflict Validation

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-test-plan.md`
6. `crates/storage/src/txn/validation.rs`
7. `crates/storage/src/txn/context.rs`
8. `crates/storage-next/src/commit/batch.rs`
9. `crates/storage-next/src/branch/read.rs`

### Preserved As Behavior

1. Read-set validation compares captured observed versions against the current
   target branch read view.
2. CAS validation compares expected versions against the current target branch
   read view.
3. Missing current rows are represented as `CommitObservedVersion::Missing`.
4. Visible non-tombstone current rows are represented as
   `CommitObservedVersion::Present(version)`.
5. Tombstone-hidden rows map to missing through the L6 visible-row API.
6. Blind puts and blind deletes do not conflict.
7. Conflict validation mode `Skip` reads nothing.
8. Lower-layer branch read failures are preserved as commit lower-layer errors
   with source chains.

### Intentionally Changed Or Retired

1. L7F does not port public transaction sessions.
2. L7F does not port product transaction ids.
3. L7F does not claim serializable isolation; write skew remains possible.
4. L7F does not implement read-your-writes staging overlays.
5. L7F stores conflict diagnostics as storage facts, including a stable key
   fingerprint for same-length-key disambiguation, not product keys or row
   values.

### Deferred By Owner Slice

1. `L7H`: cache/no-WAL commit path integrates admission, conflict validation,
   allocation, L6 apply, and visibility.
2. `L7I`: durable WAL path runs conflict validation before WAL append.
3. `L7K`: recovery replay bypasses normal read-set/CAS validation.
4. `L7M`: fuzz targets and larger generated conflict scripts.
5. `L9`: public APIs that decide when to supply read-set and CAS facts.

### Tests And Guards Added

1. `crates/storage-next/src/commit/conflict.rs`
2. Direct conflict tests in `crates/storage-next/src/commit/tests/conflict.rs`
3. Generated conflict checks in
   `crates/storage-next/src/testkit/commit_runtime_conflicts.rs`
4. Generated counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
5. Narrow L7-to-L6 source-guard allowance for
   `crate::branch::BranchReadView` only in `commit/conflict.rs`

### Sensitivity Probes

Planned L7F probes:

1. Treat every read-set fact as matched; read-set mismatch tests must fail.
2. Treat every CAS fact as matched; CAS mismatch tests must fail.
3. Compare only present/missing and ignore version; present-version mismatch
   tests must fail.
4. Treat tombstone-hidden rows as present; tombstone-as-missing tests must
   fail.
5. Reject blind writes; blind put/delete tests must fail.
6. Read the source in skip mode; skip no-read tests must fail.
7. Validate CAS before read-set; combined-ordering tests must fail.
8. Drop lower-layer source errors; source-chain tests must fail.
9. Validate against a non-target branch view; branch mismatch source tests must
   fail.
10. Include row value bytes or user-key bytes in conflict display; vocabulary
    and display tests must fail.

### Command Evidence

Verified for L7F:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## L7G: Commit Timeline Substrate

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/storage/storage-space-id-registry.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md`
7. `docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-test-plan.md`
8. `crates/storage/src/txn/manager.rs`
9. `crates/storage/src/segmented/mod.rs`
10. `crates/storage-next/src/row/mod.rs`
11. `crates/storage-next/src/format/key.rs`
12. `crates/storage-next/src/commit/batch.rs`

### Preserved As Behavior

1. Commit versions remain monotonic storage facts allocated outside the
   timeline substrate.
2. Commit timestamps remain commit facts stamped onto rows.
3. Timestamp reads continue to rely on storage commit facts rather than product
   event-time payloads.
4. Caller-supplied mutations into storage-owned row spaces remain rejected.

### Intentionally Changed Or Added

1. L7G adds an explicit storage-owned commit timeline row family under
   `StorageSpaceId::COMMIT_TIMELINE`.
2. One timeline entry creates exactly two rows: timestamp-to-version and
   version-to-timestamp.
3. Timestamp index keys include timestamp and commit version so equal
   timestamps tie-break by greatest version.
4. Timeline values duplicate key facts, allowing validators to reject
   key/value/row-fact mismatches.
5. `CommitTimelineView` builds branch-local retained timeline facts and skips
   non-timeline rows without scanning user row histories.
6. Timeline helpers are pure L7 row/fact helpers and do not call L6 mutation,
   L4 WAL, backend, filesystem, or clock APIs.

### Deferred By Owner Slice

1. `L7H`: install timeline rows atomically with user rows in cache/no-WAL mode.
2. `L7I`: include timeline rows in durable `WalRecord` payloads.
3. `L7J`: classify durable-but-not-visible failures involving timeline rows.
4. `L7K`: replay durable timeline rows without allocating new versions.
5. `L7M`: fuzz target registration and expanded timeline corpora.
6. `L8`: process-open recovery orchestration.
7. `L9`: public timestamp selectors and branch-from-time APIs.

### Tests And Guards Added

1. `crates/storage-next/src/commit/timeline.rs`
2. Direct timeline tests in `crates/storage-next/src/commit/tests/timeline.rs`
3. Generated timeline checks in
   `crates/storage-next/src/testkit/commit_runtime_timeline.rs`
4. Generated counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
5. Commit-runtime source guards include a timeline-specific boundary check for
   `commit/timeline.rs` and reject L6, L4/WAL, backend, table, service,
   filesystem, public API, and product-vocabulary leakage.

### Sensitivity Probes

Planned L7G probes:

1. Omit timestamp-index row; missing-index direct/generated tests must fail.
2. Omit version-index row; missing-index direct/generated tests must fail.
3. Write timeline rows into an engine-owned storage-space id; malformed-row
   tests must fail.
4. Stamp timeline rows with a different commit version than the entry;
   mismatched-row tests must fail.
5. Stamp timeline rows with a different timestamp than the entry;
   mismatched-row tests must fail.
6. Sort duplicate timestamps by lowest version; duplicate-timestamp direct and
   generated tests must fail.
7. Resolve timestamp lookup across branch boundaries; branch-isolation tests
   must fail.
8. Trust timestamp-index value without checking key facts; key/value mismatch
   tests must fail.
9. Trust version-index key without checking row timestamp; mismatched-row tests
   must fail.
10. Accept tombstone timeline rows; malformed-row tests must fail.
11. Let caller mutations target timeline storage space; caller-boundary tests
    must fail.
12. Import L6, WAL, backend, table, filesystem, or product APIs from
    `commit/timeline.rs`; source guard tests must fail.

### Command Evidence

Verified for L7G:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --locked --lib commit::tests::timeline`
3. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
5. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

## L7H: Cache Commit Runtime

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7h-cache-commit-runtime-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7h-cache-commit-runtime-test-plan.md`
6. `crates/storage-next/src/commit/cache.rs`
7. `crates/storage-next/src/commit/batch.rs`
8. `crates/storage-next/src/commit/conflict.rs`
9. `crates/storage-next/src/commit/visibility.rs`
10. `crates/storage-next/src/branch/state.rs`

### Preserved As Behavior

1. Cache commits remain no-WAL, non-durable commits.
2. Cache commits still use the same commit fact allocator, branch admission, and
   conflict validation as durable commits.
3. Cache commits install user rows and timeline rows in one L6 atomic append.
4. Visible publication remains the final step.

### Intentionally Changed Or Added

1. Added `CommitCacheRuntime` as the V1 no-WAL executor.
2. Cache commits reject durable modes, read-only batches, branch mismatches,
   stale branch generations, unresolved global gates, and target branches with
   unpublished rows before allocation.
3. Cache visible-publication failure now records a global not-durable
   applied-not-visible gate so a later cross-branch visible-version advance
   cannot expose unpublished cache rows by side effect.
4. Cache apply failure remains a pre-visible lower-layer failure: no gate is
   recorded because rows were not installed.

### Deferred By Owner Slice

1. `L8`: process-open recovery and checkpoint coordination.
2. `L9`: public cache-mode API decisions and user-facing error surfaces.
3. `post-V1`: public rollback/repair workflow for not-durable
   applied-not-visible cache state.

### Tests And Guards Added

1. Direct cache tests in `crates/storage-next/src/commit/tests/cache.rs`
2. Generated cache checks in
   `crates/storage-next/src/testkit/commit_runtime_cache.rs`
3. Closeout inventory now points to direct cache fault-window tests.

### Command Evidence

Verified for L7H:

1. `cargo test -p strata-storage-next --locked --lib commit::tests::cache`
2. `cargo test -p strata-storage-next --locked --lib commit`
3. `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit`
5. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
6. `cargo fmt --package strata-storage-next --check`
7. `git diff --check`

## L7I: Durable WAL Commit Runtime

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7i-durable-wal-commit-runtime-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7i-durable-wal-commit-runtime-test-plan.md`
7. `crates/storage-next/src/commit/durable.rs`
8. `crates/storage-next/src/commit/durable_gate.rs`
9. `crates/storage-next/src/format/wal.rs`
10. `crates/storage-next/src/service/wal.rs`

### Preserved As Behavior

1. Durable commits append a row-native `WalRecord` before L6 apply.
2. WAL append facts determine whether the requested durability policy was met.
3. Clean WAL failures do not install rows, publish visibility, or record a gate.
4. Uncertain WAL failures remain `DurabilityUncertain` and do not record a gate.
5. Successful durable commits apply rows and publish visibility only after the
   WAL boundary is crossed.

### Intentionally Changed Or Added

1. Added `CommitDurableRuntime` as the WAL-backed executor.
2. Durable commits now build WAL records through the L3 row-native payload and
   L4 WAL append surface instead of constructing private commit payload bytes.
3. Durable apply and visible-publication failures record unresolved durable
   facts for replay/reconciliation.
4. Active durable admissions are globally serialized. This prevents two
   cross-branch post-WAL failures from racing to record different unresolved
   facts and misclassifying the loser.
5. Always-mode append success without a forced durable append remains
   `DurabilityUncertain`; L6 apply does not run, no gate is recorded, and replay
   ownership is left to L8 if the WAL record survives process restart.

### Deferred By Owner Slice

1. `L7J`: richer durable-but-not-visible classification and unresolved-gate
   integration.
2. `L7K`: replay entrypoints and allocator catch-up.
3. `L8`: process-open WAL discovery, durability-uncertain replay decisions, and
   crash/reopen orchestration.
4. `post-V1`: distributed fencing or multi-writer durable admission.

### Tests And Guards Added

1. Direct durable tests in `crates/storage-next/src/commit/tests/durable.rs`
2. Direct gate tests in `crates/storage-next/src/commit/tests/durable_gate.rs`
3. Generated durable checks in
   `crates/storage-next/src/testkit/commit_runtime_durable.rs`
4. Closeout inventory now points to direct durable and replay fault-window tests.

### Command Evidence

Verified for L7I:

1. `cargo test -p strata-storage-next --locked --lib commit::tests::durable`
2. `cargo test -p strata-storage-next --locked --lib commit::tests::durable_gate`
3. `cargo test -p strata-storage-next --locked --lib commit`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit`
5. `cargo test -p strata-storage-next --locked --test commit_runtime_faults --features 'testkit fault-injection'`
6. `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit`
7. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

## L7J: Durable-But-Not-Visible Classification

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l4-log-manifest-snapshot-services.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-test-plan.md`
7. `crates/storage/src/durability/commit_adapter.rs`
8. `crates/storage/src/txn/manager.rs`
9. `crates/storage-next/src/commit/durable.rs`
10. `crates/storage-next/src/commit/cache.rs`
11. `crates/storage-next/src/commit/outcome.rs`
12. `crates/storage-next/src/commit/visibility.rs`
13. `crates/storage-next/src/branch/state.rs`

### Preserved As Behavior

1. A commit that has not crossed the WAL durability boundary remains a clean
   pre-visible failure or a durability-uncertain failure.
2. A commit that has crossed the WAL durability boundary but fails before
   normal visibility is classified distinctly as durable-but-not-visible.
3. Forward mutating progress halts while durable commit state is unresolved.
4. Lower-layer failure details remain attached to the returned
   durable-but-not-visible error.
5. Successful L7I durable commit ordering remains unchanged:
   WAL append, L6 apply, then visible publication.

### Intentionally Changed Or Added

1. Added `CommitUnresolvedDurable` and `CommitUnresolvedDurableKind` as bounded
   crate-private handoff facts for L7K/L8.
2. Added `CommitUnresolvedDurableGate` as an in-process write gate. It is not
   process-global and can only be cleared by an exact fact match.
3. Added `CommitRuntimeError::UnresolvedDurableCommit` for later mutating
   commits blocked by the gate.
4. Durable runtime now records `DurableNotApplied` when WAL append succeeds but
   L6 apply fails.
5. Durable runtime now records `AppliedNotVisible` when WAL append and L6 apply
   succeed but visible publication fails.
6. Cache runtime checks the same gate before allocation or mutation.
7. Durable runtime now uses narrow apply/visible traits so tests can inject
   post-WAL L6 and visible-publish fault windows without mocking the entire L6
   branch runtime.

### Deferred By Owner Slice

1. `L7K`: WAL replay and clearing the gate after exact replay/reconcile.
2. `L8`: process-open recovery orchestration and durable gate reconstruction.
3. `L9`: public error mapping and user-facing recovery messaging.
4. Checkpoint/manifest interaction remains outside L7J.

### Tests And Guards Added

1. `crates/storage-next/src/commit/durable_gate.rs`
2. Direct gate tests in
   `crates/storage-next/src/commit/tests/durable_gate.rs`
3. Durable post-WAL fault-window tests in
   `crates/storage-next/src/commit/tests/durable.rs`
4. Cache gate-blocking test in `crates/storage-next/src/commit/tests/cache.rs`
5. Generated durable contract checks in
   `crates/storage-next/src/testkit/commit_runtime_durable.rs`
6. Generated counter assertions in
   `crates/storage-next/tests/commit_runtime_properties.rs`
7. Source-guard allowance for durable runtime's narrow L6 read-view boundary.

### Sensitivity Probes

Planned L7J probes:

1. Collapse post-WAL L6 apply failure into a clean lower-layer error;
   durable-not-applied direct and generated tests must fail.
2. Return visible success after visible publication failure; applied-not-visible
   direct and generated tests must fail.
3. Record applied/visible facts for `DurableNotApplied`; gate validation tests
   must fail.
4. Omit applied/timeline facts for `AppliedNotVisible`; gate validation and
   visible-failure tests must fail.
5. Allow `NotDurable` or `Uncertain` unresolved durable facts; gate validation
   tests must fail.
6. Do not record the gate before returning durable-but-not-visible; direct and
   generated post-WAL fault tests must fail.
7. Allow cache commit through a set gate; cache gate-blocking test must fail.
8. Allow durable commit through a set gate; durable gate-blocking test must
   fail.
9. Overwrite one unresolved fact with a different fact; gate-state tests must
   fail.
10. Add table/backend/layout/filesystem/product imports to `commit/durable.rs`
    or the new gate module; source guard tests must fail.

### Command Evidence

Verified for L7J during implementation:

1. `cargo check -p strata-storage-next --lib --locked`
2. `cargo test -p strata-storage-next --lib commit::tests::durable_gate --locked`
3. `cargo test -p strata-storage-next --lib commit::tests::cache --locked`
4. `cargo test -p strata-storage-next --lib commit::tests::durable --locked`
5. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
6. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
7. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
8. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
9. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
10. `cargo fmt --package strata-storage-next --check`
11. `git diff --check`

## L7K: Recovery Replay And Allocator Catch-Up

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7k-recovery-replay-allocator-catch-up-test-plan.md`
7. `crates/storage/src/segmented/mod.rs`
8. `crates/storage/src/durability/recovery.rs`
9. `crates/storage-next/src/format/wal.rs`
10. `crates/storage-next/src/commit/allocator.rs`
11. `crates/storage-next/src/commit/durable.rs`
12. `crates/storage-next/src/commit/durable_gate.rs`
13. `crates/storage-next/src/commit/outcome.rs`
14. `crates/storage-next/src/commit/visibility.rs`
15. `crates/storage-next/src/branch/state.rs`

### Preserved As Behavior

1. Replay uses the durable WAL record's original branch id, commit version, and
   timestamp.
2. Replay does not allocate a new commit version.
3. Replay does not request a new timestamp.
4. Replay does not run read-set or CAS conflict validation.
5. Replay installs storage-owned timeline rows with user rows.
6. Exact duplicate replay is idempotent.
7. Duplicate mismatch and partial replay state fail closed.
8. Matching unresolved durable gates clear only after visible publication.

### Intentionally Changed Or Added

1. Added `CommitReplayRuntime`, `CommitReplayRequest`,
   `CommitReplayAction`, and `CommitReplayReport`.
2. Added replay validation for durable class, target branch, duplicate internal
   rows, and required commit-timeline row pairs.
3. Added own-row duplicate classification using L6 read views while ignoring
   inherited-only rows as installed replay state.
4. Added allocator version/timestamp catch-up only after L6 install or exact
   duplicate confirmation.
5. Added replay visible publication through the same visible-publisher boundary
   used by the durable path.
6. Added `CommitUnresolvedDurableGate::replace_exact` so replay can advance a
   stale `DurableNotApplied` gate to `AppliedNotVisible` if rows apply but
   visible publication fails.
7. Updated commit-runtime source guards to allow replay's narrow decoded-WAL
   and L6 read-view boundaries without allowing WAL scanning, service imports,
   table internals, backend, layout, object, or IO APIs.
8. Relaxed the row module's stale dead-code expectation to an allow because L7
   now consumes enough row helpers that the previous expectation became
   unfulfilled under clippy.
9. Replay outcome counts now bypass live batch admission limits for
   already-durable WAL rows while still validating the runtime config itself.
10. Direct replay coverage now includes fresh `Always` replay, mixed
    put/delete replay, second-replay idempotency, timeline-present/user-missing
    partial state, timestamp/expiry/tombstone mismatch dimensions, lower
    allocator-floor replay, stale-conflict bypass, WAL outer-fact rejection,
    empty-payload rejection, matching-gate apply clear, matching-gate apply
    failure preservation, gate-clear failure after visible publication, value
    byte non-leakage, and L6 read-view/apply source preservation.

### Deferred By Owner Slice

1. `L7M`: generated replay scripts, fuzz inputs, and replay counters.
2. `L8`: WAL scanning, recovery ordering, checkpoint selection, process-open
   recovery health, and replay orchestration.
3. `L9`: public recovery commands and user-facing error mapping.

### Tests And Guards Added

1. `crates/storage-next/src/commit/replay.rs`
2. Direct replay tests in `crates/storage-next/src/commit/tests/replay.rs`
3. `CommitUnresolvedDurableGate::replace_exact` test in
   `crates/storage-next/src/commit/tests/durable_gate.rs`
4. Source-guard updates in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probes

Planned L7K probes:

1. Allocate a fresh version during replay; replay direct tests must fail.
2. Generate a fresh timestamp during replay; replay direct tests must fail.
3. Omit timeline row validation; missing-timeline replay test must fail.
4. Publish visibility before L6 install; visible-failure replay tests must
   fail.
5. Treat partial replay rows as success; partial replay test must fail.
6. Treat duplicate row mismatch as idempotent; mismatch replay test must fail.
7. Skip allocator catch-up after exact duplicate replay; idempotent replay test
   must fail.
8. Clear an unresolved gate before visible publication succeeds; visible
   failure replay tests must fail.
9. Leave a stale `DurableNotApplied` gate after rows apply and visible publish
   fails; gate-advance replay test must fail.
10. Add table/backend/layout/filesystem/product imports to `commit/replay.rs`;
    source guard tests must fail.
11. Re-admit replayed durable rows through current batch-size caps; replay
    config-limit test must fail.
12. Drop L6 read-view or apply source chains; replay source-chain tests must
    fail.
13. Accept WAL payload rows whose outer branch/version/timestamp disagree with
    the record envelope; replay outer-fact construction tests must fail.
14. Clear an unresolved durable gate that changed after visible publication;
    gate-clear failure replay test must fail.
15. Print replay row value bytes in mismatch errors; replay value-non-leakage
    assertion must fail.

### Command Evidence

Verified for L7K during implementation:

1. `cargo test -p strata-storage-next --locked --lib commit::tests::replay`
2. `cargo test -p strata-storage-next --locked --lib commit::tests::durable_gate`
3. `cargo test -p strata-storage-next --locked --lib commit`
4. `cargo test -p strata-storage-next --no-default-features --locked --lib commit`
5. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
6. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties`
7. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults`
8. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
9. `cargo fmt --package strata-storage-next --check`
10. `git diff --check`

## L7L: Concurrency And Quiesce Hardening

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7l-concurrency-quiesce-hardening-test-plan.md`
7. `crates/storage/src/txn/manager.rs`
8. `crates/storage/src/txn/lock_ordering.rs`
9. `crates/storage-next/src/commit/guard.rs`
10. `crates/storage-next/src/commit/branch_registry.rs`
11. `crates/storage-next/src/commit/conflict.rs`
12. `crates/storage-next/src/commit/cache.rs`
13. `crates/storage-next/src/commit/durable.rs`
14. `crates/storage-next/src/commit/durable_gate.rs`
15. `crates/storage-next/src/commit/replay.rs`
16. `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs`

### Preserved As Behavior

1. Same-branch mutating commit admission is serialized by a branch guard.
2. Different branches can hold branch guards concurrently.
3. Mutating admission validates branch lifecycle and generation before
   allocation.
4. Read-only diagnostics do not require a mutating branch guard.
5. Cache commits hold the branch guard through conflict validation, L6 apply,
   and visible publication.
6. Durable commits hold the branch guard through WAL append, L6 apply, gate
   recording, and visible publication.
7. Replay remains a separate already-durable path. L8 owns quiescing normal
   writes before replay when process-wide exclusion is required.

### Intentionally Changed Or Added

1. Documented V1 quiesce as nonblocking: active branch guards return
   `CommitQuiesceUnavailable`, and L8 owns retry/deadline policy.
2. Added guard/quiesce contract comments in `commit/guard.rs`.
3. Added admission and runtime-order comments in `branch_registry.rs`,
   `cache.rs`, and `durable.rs`.
4. Added a replay handoff comment documenting why replay bypasses normal
   mutating admission.
5. Added a direct scripted guard interleaving test that proves quiesce and
   branch guards stay mutually exclusive across a fixed operation sequence.
6. Added a deterministic guard/quiesce interleaving contract to the generated
   commit-runtime scaffold outcome.
7. Reused the L7 durable apply/visible traits for cache commits so tests can
   inject L6 apply and visible-publication failure windows without fake global
   state.
8. Added cache L6 apply failure and cache visible-publication failure tests that
   assert branch-guard release, value-free error output, and same-branch
   follow-on rejection after an injected applied-not-visible state.
9. Added durable guard-release assertions for conflict, clean WAL failure, and
   writer-halted failure windows.
10. Added durable target-branch applied-above-visible rejection coverage before
   allocation or WAL append.
11. Extended commit-runtime source guards to reject direct sleeps and async
   runtime dependencies in `src/commit`.
12. Updated the parent L7 plans to remove stale loom/blocking-wait language for
   L7L and point to the dedicated L7L plan files.

### Deferred By Owner Slice

1. `L7M`: broad generated multi-branch commit scripts, fuzz targets, and
   richer fault corpora.
2. `L7N`: closeout inventory and sensitivity ledger enforcement.
3. `L8`: checkpoint/recovery retry loops and any caller-level deadline policy
   around nonblocking quiesce attempts.
4. `L9`: public branch clear/delete orchestration and user-facing maintenance
   commands.

### Tests And Guards Added

1. Direct guard script test in `crates/storage-next/src/commit/tests/guard.rs`
2. Cache L6 apply and visible-publication failure tests in
   `crates/storage-next/src/commit/tests/cache.rs`
3. Durable guard-release and applied-above-visible pre-WAL tests in
   `crates/storage-next/src/commit/tests/durable.rs`
4. Deterministic guard interleaving helper in
   `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs`
5. New generated scaffold counter and property assertion in
   `crates/storage-next/src/testkit/commit_runtime.rs` and
   `crates/storage-next/tests/commit_runtime_properties.rs`
6. Source-guard checks in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probes

Planned L7L probes:

1. Allow same-branch double guard acquisition; direct guard and generated
   guard-contention tests must fail.
2. Reject different-branch guard acquisition; direct and generated
   different-branch guard tests must fail.
3. Allow quiesce while branch guards are active; direct scripted and generated
   quiesce-active-guard tests must fail.
4. Allow branch guard acquisition while quiesce is active; direct scripted and
   generated quiesce-blocking tests must fail.
5. Forget to clear quiesce on token drop; direct quiesce release tests must
   fail.
6. Move allocation before branch admission; cache/durable no-allocation tests
   must fail.
7. Drop the branch guard before conflict validation completes; cache guarded
   conflict-window tests must fail.
8. Ignore unresolved durable gate for cache commit; cache gate-blocking tests
   must fail.
9. Ignore unresolved durable gate for durable commit; durable gate-blocking
   tests must fail.
10. Accept target-branch applied rows above the visible version; cache and
    durable applied-above-visible tests must fail.
11. Add direct sleeps, async runtime dependencies, or public commit APIs to
    `src/commit`; source guard tests must fail.

### Command Evidence

Verified for L7L during implementation:

1. `cargo test -p strata-storage-next --locked --lib commit::tests::guard`
2. `cargo test -p strata-storage-next --locked --lib commit::tests::branch_registry`
3. `cargo test -p strata-storage-next --locked --lib commit::tests::cache`
4. `cargo test -p strata-storage-next --locked --lib commit::tests::durable`
5. `cargo test -p strata-storage-next --locked --lib commit`
6. `cargo test -p strata-storage-next --no-default-features --locked --lib commit`
7. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties`
8. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_faults`
9. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
10. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
11. `cargo fmt --package strata-storage-next --check`
12. `git diff --check`

## L7M: Generated Fuzz And Fault Assurance

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L7/l7m-generated-fuzz-fault-assurance-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7m-generated-fuzz-fault-assurance-test-plan.md`
6. `crates/storage-next/src/commit/cache.rs`
7. `crates/storage-next/src/commit/durable.rs`
8. `crates/storage-next/src/commit/conflict.rs`
9. `crates/storage-next/src/commit/timeline.rs`
10. `crates/storage-next/src/commit/replay.rs`
11. `crates/storage-next/src/testkit/commit_runtime.rs`
12. `crates/storage-next/tests/commit_runtime_properties.rs`
13. `crates/storage-next/tests/commit_runtime_faults.rs`

### Preserved As Behavior

1. The existing scaffold contract remains as a broad fixed-scenario smoke
   route for allocator, batch, branch-guard, outcome, conflict, timeline,
   cache, durable, and replay helpers.
2. L7M does not add a production commit mode and does not change the commit
   protocol.
3. Cache commits remain no-WAL commits that install L6 rows and publish the
   visible version atomically from the caller's perspective.
4. Durable commits still append WAL records before L6 apply and record
   unresolved durable facts for post-WAL failure windows.
5. Replay remains the only path that consumes already-durable WAL records and
   clears matching unresolved durable facts.

### Intentionally Changed Or Added

1. Added a bounded generated script decoder in
   `crates/storage-next/src/testkit/commit_runtime_script.rs`, including
   generated branch lifecycle operations. The generated script is capped at
   the L7M-planned 64 operations.
2. Added an independent in-memory oracle in
   `crates/storage-next/src/testkit/commit_runtime_model.rs` for visible
   rows, timeline facts, unresolved durable facts, guard/quiesce state, and
   branch lifecycle/generation facts.
3. Added a production runner in
   `crates/storage-next/src/testkit/commit_runtime_runner.rs` that compares the
   oracle to the real cache, durable, conflict, timeline, guard/quiesce, and
   replay surfaces after every generated operation. The public fuzz contracts
   now validate their own focus category instead of only routing to the broad
   scaffold. Generated timestamps deliberately include adjacent equal values
   so timeline timestamp/version tiebreak parity is exercised.
4. Added generated property coverage in
   `crates/storage-next/tests/commit_runtime_properties.rs` that requires all
   L7M categories to be exercised.
5. Expanded `commit_runtime_faults.rs` so post-WAL apply, post-WAL visibility,
   WAL clean/uncertain/writer-halted/segment-overflow, replay, and
   guard/quiesce failures run through the generated model route.
6. Added four fuzz targets:
   `commit_runtime_batch`, `commit_runtime_conflict`,
   `commit_runtime_durable`, and `commit_runtime_timeline`.
7. Added checked-in seed corpora for all four commit-runtime fuzz targets.

### Deferred By Owner Slice

1. `L7N`: final closeout inventory, sensitivity-ledger rows, and any remaining
   documentation-only consistency checks.
2. `L8`: lifecycle/recovery orchestration around replay discovery,
   checkpointing, quiesce retry policy, and process restart.
3. `L9`: public database APIs and user-facing branch maintenance commands.

### Tests And Guards Added

1. `CommitRuntimeAssuranceOutcome` and script contract exports in
   `crates/storage-next/src/testkit/mod.rs`
2. Model-vs-runtime property test in
   `crates/storage-next/tests/commit_runtime_properties.rs`
3. Generated fault-script assertions in
   `crates/storage-next/tests/commit_runtime_faults.rs`
4. Fuzz target routing and seed corpus checks in
   `crates/storage-next/tests/commit_runtime_fuzz_inventory.rs`
5. Fuzz targets under `crates/storage-next/fuzz/fuzz_targets/`
6. Seed corpora under `crates/storage-next/fuzz/corpus/commit_runtime_*`
7. Script decoder and independent-model unit tests under
   `crates/storage-next/src/testkit/commit_runtime_script.rs` and
   `crates/storage-next/src/testkit/commit_runtime_model.rs`

### Sensitivity Probes

Planned L7M probes for L7N to run and record:

1. Remove generated cache commits from the script; the L7M property category
   assertion must fail.
2. Remove generated durable success from the script; the L7M property category
   assertion must fail.
3. Remove WAL clean/uncertain/writer-halted/segment-overflow faults; generated
   fault assertions must fail.
4. Do not record unresolved durable facts after post-WAL apply/visible
   failures; model parity or replay assertions must fail.
5. Allow unresolved durable facts to admit new cache/durable commits; model
   parity must fail.
6. Publish visibility without applying model rows; visible-value parity must
   fail.
7. Skip timeline rows during cache/durable/replay apply; timeline parity must
   fail.
8. Ignore read-set conflict validation; generated conflict operations must
   diverge from the model.
9. Allow branch guard acquisition during quiesce or quiesce with active branch
   guards; guard/quiesce parity must fail.
10. Forget allocator catch-up during replay; allocator parity must fail.
11. Route any commit-runtime fuzz target to the wrong contract function; L7N
    source/fuzz inventory should fail.
12. Drop generated branch lifecycle transitions or lifecycle rejection; L7M
    property and focused batch-contract assertions must fail.

### Command Evidence

Verified for L7M during implementation:

1. `cargo check -p strata-storage-next --tests --locked`
2. `cargo test -p strata-storage-next --test commit_runtime_properties --features testkit --locked -- --nocapture`
3. `cargo test -p strata-storage-next --test commit_runtime_faults --features 'testkit fault-injection' --locked -- --nocapture`
4. `cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins`
5. `cargo test -p strata-storage-next --test commit_runtime_source_guard --locked`
6. `cargo test -p strata-storage-next --locked --lib commit`
7. `cargo test -p strata-storage-next --locked --lib commit_runtime --features testkit`
8. `cargo test -p strata-storage-next --test commit_runtime_fuzz_inventory --locked`
9. `cargo test -p strata-storage-next --test commit_runtime_fuzz_inventory --features testkit --locked`
10. `cargo check -p strata-storage-next --no-default-features --locked --tests`
11. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties --test commit_runtime_faults`
12. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
13. `cargo fmt --package strata-storage-next --check`
14. `git diff --check`

## L7N: L7 Conformance Closeout

### Source Evidence Read

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7n-l7-conformance-closeout-test-plan.md`
7. `crates/storage-next/src/commit/`
8. `crates/storage-next/src/testkit/commit_runtime*.rs`
9. `crates/storage-next/tests/commit_runtime_properties.rs`
10. `crates/storage-next/tests/commit_runtime_faults.rs`
11. `crates/storage-next/tests/commit_runtime_fuzz_inventory.rs`
12. `crates/storage-next/tests/commit_runtime_source_guard.rs`
13. `crates/storage-next/fuzz/Cargo.toml`
14. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_*.rs`
15. `crates/storage-next/fuzz/corpus/commit_runtime_*`

### Preserved As Behavior

1. L7 remains a crate-private storage runtime, not a public transaction API.
2. Cache commits remain no-WAL commits with non-durable visible outcomes.
3. Durable commits remain WAL-before-L6-apply and classify post-WAL failures as
   unresolved durable state.
4. Replay remains the only path that consumes already-durable records and
   catches up commit-version allocation.
5. Timeline facts remain generated by storage and installed with user rows.
6. Read-set and CAS validation remain optional internal validation modes; blind
   writes remain accepted.

### Intentionally Changed Or Added

1. Added `crates/storage-next/tests/commit_runtime_closeout.rs` as the L7N
   implementation inventory test. It checks generated counters, fault phase
   assertions, fuzz target routing, seed corpus presence, and source-guard
   inventory.
2. Extended `commit_runtime_source_guard.rs` with
   `lower_storage_layers_do_not_import_commit_runtime_upward`, so branch,
   format, and service production sources cannot import the L7 runtime.
3. Kept closeout tests focused on implementation artifacts. L7N does not add
   tests that pass because planning documents exist or are linked.
4. The L7N closeout inventory executes generated commit-runtime contracts when
   the `testkit` feature is enabled and otherwise still checks source/fuzz
   implementation artifacts.

### Deferred By Owner

1. `L8`: process-open WAL discovery, replay orchestration, recovery quiesce,
   checkpoint/recovery coordination, and real crash/reopen classification.
2. `L9`: public database APIs, user-visible transaction/session decisions,
   branch lifecycle commands, and storage API response DTOs.
3. `engine-next`: observer side effects, product branch operations, and any
   product-level merge/cherry-pick/revert behavior.
4. `post-V1`: durable transaction ids, cross-branch atomic commits,
   serializable isolation claims, distributed fencing beyond current L4
   capabilities, and query/index/search side effects.

### Tests And Guards Added

1. `crates/storage-next/tests/commit_runtime_closeout.rs`
2. `lower_storage_layers_do_not_import_commit_runtime_upward` in
   `crates/storage-next/tests/commit_runtime_source_guard.rs`

### Sensitivity Probe Ledger

L7N records the mutation target, the implemented test/guard evidence, and the
command that executes that evidence for each closeout probe. These are
coverage-backed sensitivity probes, not live mutation-test runs; no mutation
edits were made or left in the committed tree.

| Probe | Mutation target | Mutation | Implemented evidence | Verification command | Status |
|---|---|---|---|---|---|
| S1 | `src/commit/outcome.rs` / read-only constructors | Allocate a version for a read-only batch. | `commit_fact_allocator_read_only_path_does_not_touch_source_or_clocks`, `read_only_diagnostic_returns_snapshot_without_commit_facts`, and generated `read_only_diagnostics` counters. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S2 | `src/commit/batch.rs` / stamping helpers | Stamp two rows in one batch with different versions. | `cache_commit_row_preparation_uses_one_stamp_for_user_and_timeline_rows` and scaffold stamping counters asserted by `commit_runtime_properties`. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S3 | `src/commit/batch.rs` / stamping helpers | Stamp two rows in one batch with different timestamps. | `cache_commit_preserves_explicit_timestamp_for_user_and_timeline_rows`, `cache_commit_row_preparation_uses_one_stamp_for_user_and_timeline_rows`, and WAL payload parity counters. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S4 | `src/commit/durable.rs` / durable execution order | Apply to L6 before WAL append in durable mode. | Durable WAL-before-visible tests plus generated model parity in `commit_runtime_closeout_generated_contracts_exercise_required_categories`. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit` | Covered-by-test; live mutation not run |
| S5 | `src/commit/durable.rs` / WAL failure handling | Treat WAL append failure or unforced Always append as visible success. | `commit_runtime_fault_harness_exercises_protocol_boundaries` asserts clean/uncertain WAL failures and no-gate behavior; `durable_always_commit_rejects_unforced_success_before_apply_phase` pins the orphaned-WAL/no-apply path. | `cargo test -p strata-storage-next --locked --test commit_runtime_faults --features 'testkit fault-injection'`; `cargo test -p strata-storage-next --locked --lib commit` | Covered-by-test; live mutation not run |
| S6 | `src/commit/durable.rs` / post-WAL failure mapping | Collapse durable-but-not-visible into clean failure. | L7J durable gate tests and `commit_runtime_fault_harness_exercises_protocol_boundaries` assert post-WAL gate cases. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_faults --features 'testkit fault-injection'` | Covered-by-test; live mutation not run |
| S7 | `src/commit/conflict.rs` or cache/durable call order | Validate conflicts after version allocation. | `cache_commit_conflict_rejects_before_allocation_or_mutation` and generated conflict rejection counters. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S8 | `src/commit/conflict.rs` / blind write handling | Reject blind writes as conflicts. | `blind_put_no_conflict_cases` and `blind_delete_no_conflict_cases` are required by `commit_runtime_properties`. | `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S9 | `src/commit/batch.rs` / branch validation | Allow branch-mismatched rows in a batch. | `branch_mismatch_cases` are required by `commit_runtime_properties`. | `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S10 | `src/commit/timeline.rs` and cache/durable/replay callers | Omit timeline row generation. | Timeline row counters and generated `timeline_checks` are required by `commit_runtime_properties` and L7N closeout. | `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit`; `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit` | Covered-by-test; live mutation not run |
| S11 | `src/commit/cache.rs` or `src/commit/durable.rs` | Publish visible version before full L6 apply. | Cache/durable visibility failure tests and generated model parity assert visible publication sequencing. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit` | Covered-by-test; live mutation not run |
| S12 | `src/commit/branch_registry.rs` / branch admission | Ignore branch deleting marker. | `mutating_admission_rejects_deleting_and_deleted_before_guard` and generated branch lifecycle rejection counters. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S13 | `src/commit/guard.rs` / quiesce guard rules | Allow quiesce and mutating commit guard overlap. | `scripted_guard_interleaving_keeps_quiesce_and_branch_guards_exclusive` and generated guard/quiesce rejection counters. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit` | Covered-by-test; live mutation not run |
| S14 | `src/commit/replay.rs` / duplicate handling | Replay duplicate mismatch as success. | `replay_rejects_duplicate_mismatch_without_mutating_state`, `replay_duplicate_mismatch_checks_timestamp_expiry_and_tombstone`, and L7N replay closeout inventory. | `cargo test -p strata-storage-next --locked --lib commit`; `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit` | Covered-by-test; live mutation not run |
| S15 | `src/commit/*.rs` imports | Import backend or layout directly from `commit/`. | `commit_runtime_source_does_not_use_table_backend_layout_or_io_apis` and source-guard fixture assertions. | `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | Covered-by-guard; live mutation not run |
| S16 | `src/commit/*.rs` public surface | Expose public `begin`/`commit`/`rollback` API. | `commit_runtime_stays_crate_private`, public-surface source-guard fixtures, and L7N source inventory. | `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`; `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit` | Covered-by-guard; live mutation not run |

### Command Evidence

Verified for L7N during implementation:

1. `cargo fmt --package strata-storage-next --check`
2. `cargo test -p strata-storage-next --locked --lib commit`
3. `cargo test -p strata-storage-next --locked --lib commit_runtime --features testkit`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo test -p strata-storage-next --locked --test commit_runtime_fuzz_inventory --features testkit`
6. `cargo test -p strata-storage-next --locked --test commit_runtime_closeout --features testkit`
7. `cargo test -p strata-storage-next --locked --test commit_runtime_closeout`
8. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_closeout`
9. `cargo test -p strata-storage-next --locked --test commit_runtime_properties --features testkit`
10. `cargo test -p strata-storage-next --locked --test commit_runtime_faults --features 'testkit fault-injection'`
11. `cargo test -p strata-storage-next --all-features --locked --test commit_runtime_properties --test commit_runtime_faults --test commit_runtime_fuzz_inventory --test commit_runtime_closeout`
12. `cargo test -p strata-storage-next --no-default-features --locked commit`
13. `cargo check -p strata-storage-next --no-default-features --locked --tests`
14. `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked`
15. `cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins`
16. `cargo hack check -p strata-storage-next --feature-powerset --depth 2`
17. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
18. `git diff --check`
