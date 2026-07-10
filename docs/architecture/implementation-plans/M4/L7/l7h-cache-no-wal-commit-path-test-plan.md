# L7H Test Plan: Cache/No-WAL Commit Path

Status: implemented for direct cache runtime tests and generated cache contract

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7h-cache-no-wal-commit-path-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that L7H can execute an internal cache/no-WAL commit into L6 without
claiming durability and without making partial batches visible.

The suite must fail if L7H:

1. allocates a version before validation, branch admission, or conflict
   validation succeeds;
2. allocates more than one version for one mutating batch;
3. stamps user rows and timeline rows with different commit facts;
4. applies only part of a batch into L6;
5. publishes visible version before every user and timeline row is installed;
6. returns a durable outcome for cache mode;
7. builds or appends any WAL record;
8. skips L7E branch admission or generation checks;
9. skips L7F conflict validation;
10. lets caller-supplied timeline rows bypass L7B rejection.

Do not add tests that only prove planning documents exist or link to each
other. L7H automated tests should exercise commit behavior, model parity,
fault windows, or source boundaries.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/cache.rs` for direct cache commit
   protocol tests.
2. `crates/storage-next/src/branch/tests/` only if L7H adds a branch-level
   atomic append helper that needs direct L6 tests.
3. `crates/storage-next/src/testkit/commit_runtime_cache.rs` or
   `crates/storage-next/src/testkit/commit_runtime/cache.rs` for generated
   cache-commit contracts.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   counter assertions.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary checks.
6. `crates/storage-next/tests/commit_runtime_faults.rs` only if fault helpers
   are easier to express as integration tests than module-local tests.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. deterministic manual timestamp source;
3. one target `BranchLocalState`;
4. a real `CommitBranchRegistry`;
5. a real `CommitBranchGuardSet`;
6. a real `CommitFactAllocator`;
7. a real `VisibleVersionTracker`;
8. real L6 `BranchReadView` assertions after commit;
9. opaque value bytes only;
10. no engine DTOs, JSON, graph, vector, search, public transaction-session,
    or product `as_of` vocabulary.

## Direct Test Matrix

### 1. Happy Path

Required cases:

1. single put commits to L6 and is visible after publication;
2. single delete commits as a tombstone and hides the key from latest reads;
3. mixed put/delete batch commits atomically;
4. multiple storage spaces in one branch commit together;
5. every user row carries one commit version;
6. every user row carries one commit timestamp;
7. both timeline rows carry the same commit version and timestamp;
8. outcome kind is visible;
9. outcome phase is visible;
10. outcome durability is not durable;
11. outcome counts puts, deletes, and two timeline rows;
12. visible-version tracker advances to the commit version.

Assertions:

1. no WAL record is constructed;
2. no WAL service is imported or called;
3. no branch mutation happens before allocation;
4. no visible publication happens before apply.

### 2. Timeline Installation

Required cases:

1. mutating cache commit installs timestamp-to-version timeline row;
2. mutating cache commit installs version-to-timestamp timeline row;
3. timeline lookup after commit resolves the commit timestamp to the commit
   version;
4. version lookup after commit resolves the commit version to the commit
   timestamp;
5. user mutations into `StorageSpaceId::COMMIT_TIMELINE` still reject before
   L7H execution;
6. timeline row count contributes to configured commit-row limits.

Assertions:

1. timeline rows are installed in the same L6 atomic apply unit as user rows;
2. timeline rows are not visible if user-row apply fails;
3. user rows are not visible if timeline-row apply fails.

### 3. Branch Admission And Guards

Required cases:

1. missing branch rejects before allocation;
2. deleting branch rejects before allocation;
3. deleted branch rejects before allocation;
4. stale branch generation rejects before allocation;
5. branch-state branch id mismatch rejects before allocation;
6. same-branch guard contention rejects before allocation;
7. guard releases after success;
8. guard releases after validation failure;
9. guard releases after conflict failure;
10. guard releases after apply failure.

Assertions:

1. no rejected admission path mutates L6;
2. no rejected admission path advances visible version;
3. rejected admission paths preserve allocator state.

### 4. Conflict Integration

Required cases:

1. read-set match permits commit;
2. read-set mismatch rejects before allocation;
3. observed missing key remains missing and permits commit;
4. observed missing key becomes present and rejects before allocation;
5. CAS present match permits commit;
6. CAS present mismatch rejects before allocation;
7. CAS missing match permits commit;
8. CAS missing mismatch rejects before allocation;
9. blind put over changed key permits commit;
10. blind delete over changed key permits commit;
11. skip mode performs no conflict-source reads.
12. conflict validation is capped at the current L7 visible version, not raw
    L6 latest.

Assertions:

1. conflict rejection leaves allocator unchanged;
2. conflict rejection leaves L6 unchanged;
3. lower-layer read failure preserves source chain.

### 5. Allocation And Stamping

Required cases:

1. one mutating batch allocates exactly one version;
2. one mutating batch allocates exactly one timestamp;
3. all rows in one commit share the allocated version;
4. all rows in one commit share the allocated timestamp;
5. explicit timestamp policy is preserved;
6. generated timestamp policy uses the timestamp source once;
7. timestamp source failure rejects without version allocation;
8. version overflow rejects before L6 mutation;
9. stamping failure after allocation leaves a version gap.

Assertions:

1. version gaps are accepted by later successful commits;
2. timestamp guard is not advanced by failed explicit timestamps;
3. read-only diagnostics remain outside L7H.

### 6. Atomic L6 Apply

Required cases:

1. successful batch installs all user rows and both timeline rows;
2. duplicate internal key inside the staged batch rejects the whole batch;
3. duplicate internal key against existing active state rejects the whole
   batch;
4. wrong-branch row inside staged rows rejects the whole batch;
5. timeline-row failure leaves user rows absent;
6. user-row failure leaves timeline rows absent;
7. branch facts update only after complete staged install;
8. active row count increases by full row count only on success.

Assertions:

1. original branch state is unchanged after staged apply failure;
2. post-failure read view matches the pre-failure read view;
3. post-success read view contains all committed rows.

### 7. Visibility Publication

Required cases:

1. visible version starts at zero or configured recovered value;
2. successful cache commit publishes allocated version visible;
3. visible publication happens after L6 apply;
4. visible-version regression failure returns a not-visible cache failure;
5. applied-not-visible cache failure does not claim durability;
6. applied-not-visible cache failure does not return visible success;
7. next safe behavior after applied-not-visible is fail-closed or explicitly
   documented.

Assertions:

1. `CommitVisibilityFacts` has allocated, applied, timeline, and visible equal
   to the commit version on success;
2. `durable_version` is `None` on cache success;
3. visible facts validate.

### 8. Non-Cache Mode Rejection

Required cases:

1. `CommitDurabilityMode::Standard` rejects in L7H before allocation;
2. `CommitDurabilityMode::Always` rejects in L7H before allocation;
3. unsupported mode error is storage-shaped;
4. rejection leaves branch guard unheld;
5. rejection leaves allocator, L6, and visible tracker unchanged.

Assertions:

1. L7H does not accidentally route durable modes through cache semantics;
2. durable mode implementation remains owned by L7I.

### 9. Error Vocabulary And Source Chains

Required cases:

1. invalid batch error has storage vocabulary;
2. branch admission error has storage vocabulary;
3. conflict error has storage vocabulary;
4. L6 apply error is wrapped as lower-layer branch-runtime error;
5. visible publication failure is typed;
6. errors do not mention transaction sessions, rollback, JSON, graph, vector,
   search, datasets, remotes, documents, or `as_of`.

Assertions:

1. value bytes are not dumped in error display;
2. physical key diagnostics remain bounded;
3. lower-layer source chains are preserved where the lower layer supplies one.

## Generated Testkit Matrix

Extend the commit-runtime property harness with cache-commit counters for:

1. cache put commits;
2. cache delete commits;
3. mixed put/delete commits;
4. one-version-per-batch invariant;
5. one-timestamp-per-batch invariant;
6. timeline rows installed;
7. visible-version publication;
8. not-durable outcome;
9. branch admission rejection before allocation;
10. conflict rejection before allocation;
11. non-cache mode rejection before allocation;
12. apply failure atomicity;
13. version gap after post-allocation failure;
14. source-guard fixture coverage.

Generated scripts should vary:

1. branch id;
2. branch generation;
3. existing branch rows;
4. mutation count;
5. put/delete mix;
6. storage space id;
7. conflict validation mode;
8. timestamp policy;
9. timestamp source behavior;
10. durability mode;
11. apply fault point;
12. visible tracker starting version.

Each generated case should compare production output to an independent model:

```text
ModelCacheRuntime {
  branches
  allocated_version
  visible_version
  timeline
}
```

The model must not call production `BranchLocalState`, `CommitTimelineView`, or
`VisibleVersionTracker` to compute expected results.

## Fault Windows

Direct or generated fault tests must cover:

1. invalid batch before allocation;
2. missing/deleting/deleted branch before allocation;
3. generation mismatch before allocation;
4. conflict before allocation;
5. timestamp source failure before version allocation;
6. version allocation overflow before L6 mutation;
7. branch already contains applied rows above current visible version;
8. row stamping failure after allocation;
9. timeline row construction failure after allocation;
10. combined row-count overflow after allocation;
11. allocated version not greater than current visible version;
12. L6 staged apply failure before mutation;
13. L6 staged apply failure after some staged rows have been attempted;
14. visible publication failure after L6 apply;
15. branch guard contention;
16. guard release after each failure class.

WAL failures are not L7H fault windows. They belong to L7I/L7J.

## Source Guard Matrix

`commit_runtime_source_guard.rs` should enforce:

1. `commit/cache.rs` may import approved L6 branch state/read-view symbols;
2. `commit/cache.rs` may not import `crate::format::wal`;
3. `commit/cache.rs` may not import `crate::service::wal`;
4. `commit/cache.rs` may not import backend, layout, object, or table
   internals;
5. `commit/cache.rs` may not use filesystem, path, environment, process clock,
   or product APIs;
6. `src/branch/` still must not import `crate::commit`;
7. `src/format/` and `src/service/` still must not import `crate::commit`;
8. `commit/cache.rs` remains `pub(crate)` only.

Add fixture assertions for:

1. allowed `BranchLocalState` import in `commit/cache.rs`;
2. rejected `BranchLocalState` import from any other commit module unless
   explicitly allowed;
3. rejected WAL import;
4. rejected table import;
5. rejected backend call;
6. rejected public `pub fn commit`.

## Sensitivity Probes

Before closing L7H, record these probes in the porting log:

1. Allocate before conflict validation.
2. Allocate two versions for one batch.
3. Stamp timeline rows with a different version than user rows.
4. Omit timeline rows from the L6 apply set.
5. Apply only user rows when timeline apply fails.
6. Apply only timeline rows when user apply fails.
7. Publish visible version before L6 apply.
8. Return durable success for cache mode.
9. Route `Standard` mode through cache path.
10. Ignore branch generation mismatch.
11. Ignore branch guard contention.
12. Treat conflict mismatch as success.
13. Leave branch state partially mutated after apply failure.
14. Import WAL APIs from `commit/cache.rs`.
15. Validate conflicts against raw L6 latest instead of current visible
    version.
16. Continue committing when branch state contains applied rows above current
    visible version.

Each probe should name the mutation site and the direct or generated test that
failed.

## Required Verification

Run at minimum:

1. `cargo test -p strata-storage-next --locked --lib commit::tests::cache`
2. `cargo test -p strata-storage-next --locked --lib branch`
3. `cargo test -p strata-storage-next --locked --lib commit`
4. `cargo test -p strata-storage-next --no-default-features --locked --lib commit`
5. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
6. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
7. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
8. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
9. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
10. `cargo fmt --package strata-storage-next --check`
11. `git diff --check`

## Exit Gate

L7H is complete when:

1. cache-mode commits apply put/delete rows to L6;
2. both timeline rows are installed with the user rows;
3. all rows in one commit share one version and timestamp;
4. the L6 apply step is atomic under failure;
5. visible version publishes only after full apply;
6. success outcome is visible and not durable;
7. rejected-before-allocation paths preserve allocator state;
8. post-allocation failures may leave version gaps but not partial visibility;
9. non-cache durability modes reject before allocation;
10. generated properties compare production behavior to an independent model;
11. source guards prove L7H does not import WAL/backend/table/filesystem or
    product APIs.

## Deferred

1. WAL-before-visible durable mode: `L7I`.
2. Durable-but-not-visible classification: `L7J`.
3. Replay repair and allocator catch-up: `L7K`.
4. Concurrent quiesce hardening: `L7L`.
5. Fuzz targets and expanded fault scripts: `L7M`.
6. Public storage API mapping: L9.
