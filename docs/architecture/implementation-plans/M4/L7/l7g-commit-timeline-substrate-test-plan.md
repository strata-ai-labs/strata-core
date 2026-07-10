# L7G Test Plan: Commit Timeline Substrate

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7g-commit-timeline-substrate-implementation-plan.md`

## Goal

Prove that L7G constructs and validates storage-owned commit timeline facts
without depending on L6 mutation, WAL append, product APIs, or row-history
scans.

The suite must fail if L7G:

1. omits either timeline index row;
2. writes timeline rows outside `StorageSpaceId::COMMIT_TIMELINE`;
3. gives timeline rows different commit facts than the user commit;
4. resolves equal timestamps without commit-version tiebreaking;
5. leaks rows across branches;
6. accepts malformed timeline row bytes or mismatched key/value facts;
7. scans user rows to answer timeline lookup;
8. imports L6, L4, backend, table, filesystem, clock, or product APIs;
9. exposes product `as_of` APIs from L7.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/timeline.rs` for direct timeline
   entry, row construction, decode, validation, and lookup tests.
2. `crates/storage-next/src/testkit/commit_runtime_timeline.rs` or
   `crates/storage-next/src/testkit/commit_runtime/timeline.rs` for generated
   L7G contracts.
3. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7G
   counter assertions.
4. `crates/storage-next/tests/commit_runtime_source_guard.rs` for boundary and
   forbidden-vocabulary checks.
5. `crates/storage-next/src/commit/tests/batch.rs` only for proving caller
   mutations into storage-owned timeline space remain rejected by L7B.

Do not add tests that only prove planning documents exist or link to each
other. L7G automated tests should exercise implementation behavior, generated
coverage, or source boundaries.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. at least two branches;
3. at least three commit versions;
4. repeated timestamps;
5. timestamp queries before, at, between, and after retained entries;
6. opaque value bytes only for timeline values;
7. no engine value types, JSON, graph, vector, search, product branch commands,
   or public transaction/session vocabulary.

## Direct Test Matrix

### 1. Entry Validation

Required cases:

1. nonzero commit version with epoch timestamp is accepted;
2. nonzero commit version with non-epoch timestamp is accepted;
3. `CommitVersion::ZERO` is rejected;
4. entry preserves branch id;
5. entry preserves commit version;
6. entry preserves commit timestamp;
7. entry display/debug is bounded and storage-shaped.

Assertions:

1. entry construction does not allocate rows until row construction is called;
2. entry construction does not call clocks;
3. entry construction does not inspect user mutations.

### 2. Physical Key Construction

Required cases:

1. timestamp-index key uses the entry branch id;
2. timestamp-index key uses timeline space constant;
3. timestamp-index key uses `StorageSpaceId::COMMIT_TIMELINE`;
4. timestamp-index user key includes timestamp then version in big-endian
   order;
5. version-index key uses the entry branch id;
6. version-index key uses timeline space constant;
7. version-index key uses `StorageSpaceId::COMMIT_TIMELINE`;
8. version-index user key includes version in big-endian order;
9. two branches with the same timestamp/version produce different physical
   keys.

Assertions:

1. timeline keys are storage-owned, not engine-owned;
2. timeline key construction does not use product spaces or product names;
3. timestamp-index key ordering places lower timestamps before higher
   timestamps and lower versions before higher versions for equal timestamps.

### 3. Timeline Row Construction

Required cases:

1. one entry produces exactly two rows;
2. timestamp-index row is a put row;
3. version-index row is a put row;
4. neither row is a tombstone;
5. both rows use `Timestamp::EPOCH` expiry;
6. both rows carry the entry commit version;
7. both rows carry the entry commit timestamp;
8. timestamp-index row value encodes the commit version;
9. version-index row value encodes the commit timestamp;
10. repeated construction is byte-for-byte deterministic through physical key
    and value facts.

Assertions:

1. row construction does not mutate L6;
2. row construction does not append WAL;
3. row construction does not allocate a new version or timestamp.

### 4. Row Decode And Validation

Required cases:

1. valid timestamp-index row decodes to the original entry;
2. valid version-index row decodes to the original entry;
3. unknown user-key prefix rejects;
4. wrong storage space rejects;
5. wrong named space rejects;
6. timestamp-index value length other than 8 rejects;
7. version-index value length other than 8 rejects;
8. timestamp-index key version mismatching value version rejects;
9. version-index key version mismatching row commit version rejects;
10. row commit timestamp mismatching decoded timestamp rejects;
11. non-epoch expiry rejects;
12. tombstone timeline row rejects.

Assertions:

1. validation fails closed;
2. validation does not trust only the key or only the value;
3. validation errors are typed commit-runtime errors.

### 5. Timestamp Lookup

Required cases:

1. empty timeline reports empty miss;
2. query before earliest retained timestamp reports before-history miss;
3. query at earliest timestamp returns earliest version;
4. query between two timestamps returns the previous retained version;
5. query at latest timestamp returns latest matching version;
6. query after latest timestamp returns latest retained version with the
   documented after-latest fact if the result type exposes one;
7. duplicate timestamp returns greatest version at that timestamp;
8. duplicate timestamp with lower version is not selected over higher version;
9. lookup is branch-local.
10. view construction skips non-timeline retained branch rows before lookup.

Assertions:

1. lookup uses timestamp-index rows, not user rows;
2. lookup does not require table internals;
3. lookup does not depend on insertion order.

### 6. Version Lookup

Required cases:

1. existing version returns original timestamp;
2. missing version returns `None`;
3. version lookup is branch-local;
4. duplicate identical version-index rows are idempotent;
5. duplicate version-index rows with different timestamps reject;
6. version-index fact must agree with timestamp-index fact for the same version.

Assertions:

1. version lookup does not infer timestamp from row order;
2. version lookup does not scan user rows;
3. version lookup returns storage timestamp facts only.

### 7. Bounds And Retained History Facts

Required cases:

1. empty timeline bounds are empty;
2. single-entry bounds report the same min and max version/timestamp;
3. multi-entry bounds report min timestamp, max timestamp, min version, and
   max version;
4. duplicate timestamps do not corrupt bounds;
5. branch A bounds ignore branch B rows.
6. non-timeline retained rows do not affect timeline bounds.

Assertions:

1. bounds are retained-history facts, not product explanations;
2. bounds do not claim completeness outside retained rows;
3. bounds do not mutate visibility facts.

### 8. Caller Boundary

Required cases:

1. L7B still rejects caller put into `StorageSpaceId::COMMIT_TIMELINE`;
2. L7B still rejects caller delete from `StorageSpaceId::COMMIT_TIMELINE`;
3. L7G-generated timeline rows are accepted by timeline validators;
4. caller storage-owned row rejection remains separate from L7G system-row
   construction.

Assertions:

1. engine-owned commit mutations cannot forge timeline rows;
2. storage-owned rows are generated only by commit runtime helpers;
3. no public API accepts raw timeline rows.

### 9. Error Vocabulary

Required cases:

1. invalid prefix error is storage-shaped;
2. mismatched key/value error is storage-shaped;
3. duplicate conflicting timeline facts are storage-shaped;
4. errors do not mention `as_of`, documents, JSON, graph, vector, search,
   datasets, remotes, public transactions, rollback, or sessions.

Assertions:

1. timeline errors do not expose user row values;
2. timeline errors do not expose product keys;
3. timeline errors preserve branch/version/timestamp facts when useful.

## Generated Testkit Matrix

Extend the commit-runtime property harness with counters for:

1. valid timeline entry construction;
2. zero-version rejection;
3. timestamp-index key construction;
4. version-index key construction;
5. row construction produces two rows;
6. row construction shares commit facts;
7. valid timestamp-index decode;
8. valid version-index decode;
9. malformed prefix rejection;
10. value-length rejection;
11. key/value mismatch rejection;
12. timestamp lookup exact match;
13. timestamp lookup between retained timestamps;
14. duplicate timestamp greatest-version tiebreak;
15. version lookup;
16. branch isolation;
17. bounds reporting;
18. caller storage-owned mutation rejection.

The generated harness should vary:

1. branch id;
2. commit version;
3. commit timestamp;
4. repeated timestamp count;
5. query timestamp position;
6. branch mixing;
7. malformed row kind;
8. malformed value length;
9. key/value mismatch type;
10. row order.

Each generated case should compare production output to an independent model:

```text
ModelTimelineEntry { branch, version, timestamp }
model_timestamp_lookup(entries, branch, query_timestamp)
model_version_lookup(entries, branch, query_version)
model_bounds(entries, branch)
```

Do not derive expected lookup results by calling the production lookup helper
twice.

## Source Guard Matrix

`commit_runtime_source_guard.rs` should enforce:

1. `commit/timeline.rs` may import `crate::row` and core-next atoms;
2. `commit/timeline.rs` must not import `crate::branch`;
3. `commit/timeline.rs` must not import `crate::table`;
4. `commit/timeline.rs` must not import `crate::backend`;
5. `commit/timeline.rs` must not import `crate::service` or
   `crate::format::wal`;
6. `commit/timeline.rs` must not use filesystem, path, environment, process
   clock, or backend operation vocabulary;
7. commit production code remains crate-private;
8. product vocabulary remains forbidden.

Add fixture assertions for:

1. allowed row import;
2. rejected branch import;
3. rejected table import;
4. rejected WAL import;
5. rejected backend operation call;
6. rejected product `as_of` or transaction vocabulary.

## Sensitivity Probes

Before closing L7G, record these probes in the porting log:

1. Omit timestamp-index row.
2. Omit version-index row.
3. Write timeline rows into an engine-owned storage-space id.
4. Stamp timeline rows with a different commit version than the entry.
5. Stamp timeline rows with a different timestamp than the entry.
6. Sort duplicate timestamps by lowest version instead of greatest version.
7. Resolve timestamp lookup across branch boundaries.
8. Trust timestamp-index value without checking key facts.
9. Trust version-index key without checking row timestamp.
10. Accept tombstone timeline rows.
11. Let caller mutations target timeline storage space.
12. Import L6 or WAL APIs from `commit/timeline.rs`.

Each probe should name the mutation site and the direct or generated test that
failed.

## Required Verification

Run at minimum:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --no-default-features --locked --lib commit`
3. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
5. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
6. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
7. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
8. `cargo fmt --package strata-storage-next --check`
9. `git diff --check`

## Exit Gate

L7G is complete when:

1. timeline row construction is deterministic;
2. every commit entry produces exactly two timeline rows;
3. timestamp-to-version lookup handles exact, between, before-history,
   after-latest, and duplicate timestamp cases;
4. version-to-timestamp lookup handles found, missing, duplicate identical, and
   duplicate conflicting facts;
5. branch isolation is direct-tested and generated-tested;
6. malformed rows fail closed;
7. caller storage-owned mutation rejection remains pinned;
8. generated counters cover all timeline categories;
9. source guards keep `commit/timeline.rs` pure;
10. no product `as_of` API is exposed from L7.

## Deferred

1. Atomic apply of timeline rows with user rows: `L7H`.
2. WAL inclusion of timeline rows: `L7I`.
3. Durable-but-not-visible timeline phase classification: `L7J`.
4. Replay and recovery validation: `L7K` and L8.
5. Timeline retention/compaction policy: later retention slices.
6. Public timestamp selectors: L9/engine-next.
7. Fuzz targets and expanded generated scripts: `L7M`.
