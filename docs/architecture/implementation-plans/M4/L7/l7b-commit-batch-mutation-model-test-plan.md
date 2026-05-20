# L7B Test Plan: Commit Batch And Mutation Model

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`

## Goal

Prove that L7B defines a storage-owned commit batch model that can validate
batch structure and stamp user mutations into `StorageRow` values without
allocating versions, allocating timestamps, touching WAL, mutating L6, writing
timeline rows, or exposing public transaction APIs.

The suite must fail if L7B:

1. accepts a mutating batch with no mutations;
2. accepts branch-mismatched mutation keys;
3. accepts branch-mismatched validation facts;
4. accepts caller-supplied storage-owned key spaces;
5. accepts duplicate physical keys in one mutation batch;
6. accepts duplicate read or CAS facts without a documented policy;
7. accepts expiry-at-epoch as a real TTL despite `StorageRow` using epoch as
   the no-expiry sentinel;
8. stamps rows with mixed branch/version/timestamp facts;
9. changes opaque value bytes during stamping;
10. produces tombstones with value bytes or expiry;
11. allocates a commit version or timestamp during validation/stamping;
12. imports L6 branch state, WAL services, table internals, backend/layout
    mechanics, filesystem APIs, engine DTOs, or public transaction vocabulary.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests.rs` while the direct suite remains
   small.
2. `crates/storage-next/src/commit/tests/batch.rs` and
   `crates/storage-next/src/commit/tests/stamping.rs` if the direct tests need
   to be split.
3. `crates/storage-next/src/testkit/commit_runtime.rs` or
   `crates/storage-next/src/testkit/commit_runtime/` if the generated helpers
   need to be split before they exceed a readable size.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7B
   scaffold/model checks.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary checks.

Do not add closeout tests that merely assert planning documents exist or link
to each other. L7B automated tests should exercise implementation behavior or
source boundaries.

## Direct Test Matrix

### 1. Valid Batch Construction

Required cases:

1. single put over an engine-owned `PhysicalKey`;
2. single delete over an engine-owned `PhysicalKey`;
3. mixed put and delete over distinct physical keys;
4. empty value bytes are preserved for a put;
5. high-bit and embedded-zero user-key bytes are accepted when `PhysicalKey`
   accepts them;
6. multiple engine-owned storage spaces are accepted in one branch;
7. read-only diagnostic batch shape is constructible with no mutations;
8. options round-trip through accessors:
   - cache durability;
   - standard durability;
   - always durability;
   - validate conflicts;
   - skip conflicts;
   - reject duplicate keys;
   - runtime-generated timestamp;
   - explicit timestamp.

Assertions:

1. no row stamping happens during construction;
2. batch accessors return the target branch and original mutation order;
3. options are explicit enums, not boolean flags;
4. debug/display output does not include value bytes.

### 2. Limit Enforcement

Required cases:

1. mutation count exactly at `max_mutations_per_batch` succeeds;
2. mutation count over `max_mutations_per_batch` fails;
3. validation fact count exactly at `max_validation_facts_per_batch` succeeds;
4. validation fact count over `max_validation_facts_per_batch` fails;
5. stamped user-row count remains bounded by `max_commit_rows_per_batch`;
6. with the current config invariant, user mutation rows cannot exceed
   `max_commit_rows_per_batch` without first exceeding
   `max_mutations_per_batch`; direct row-cap overflow becomes separately
   testable when L7G adds storage-owned timeline rows;
7. durable-mode row count above the V1 durable payload cap is not enforced by
   L7B; L7B tests should assert only the commit-runtime config row cap and the
   porting log should mark the WAL payload cap as L7I-owned.

Assertions:

1. failures return typed commit errors;
2. failures happen before stamping;
3. failures do not allocate version or timestamp facts.

### 3. Branch Validation

Required cases:

1. put key from another branch is rejected;
2. delete key from another branch is rejected;
3. read-set fact key from another branch is rejected;
4. CAS fact key from another branch is rejected;
5. `CommitStamp` branch different from `CommitBatch` branch is rejected;
6. batch containing mixed valid and invalid branches returns no partial stamped
   rows.

Assertions:

1. error identifies branch mismatch without dumping value bytes;
2. target branch remains unchanged in the original batch object;
3. branch validation happens before duplicate-key or stamping work when both
   could apply.

### 4. Storage-Space Validation

Required cases:

1. put to `StorageSpaceId::COMMIT_TIMELINE` is rejected;
2. delete from `StorageSpaceId::COMMIT_TIMELINE` is rejected;
3. read fact over storage-owned timeline key is rejected for caller-supplied
   validation facts;
4. CAS fact over storage-owned timeline key is rejected;
5. invalid storage-space ids remain rejected by `PhysicalKey` construction and
   are not worked around by L7B;
6. engine-owned ids at the lower boundary are accepted.

Assertions:

1. tests prove storage-owned timeline rows are deferred to L7G;
2. tests do not import timeline row encoding helpers into L7B.

### 5. Duplicate Physical-Key Policy

Under V1 `CommitDuplicateKeyPolicy::Reject`, required rejected cases:

1. put then put on same physical key;
2. put then delete on same physical key;
3. delete then put on same physical key;
4. delete then delete on same physical key;
5. duplicate key appears non-adjacent in the mutation list;
6. same user bytes in different storage spaces are not duplicates;
7. same user bytes in different branches are rejected as branch mismatch, not
   accepted as distinct duplicates in a single-branch batch.

Validation-fact duplicate cases:

1. duplicate read fact for one physical key is rejected;
2. duplicate CAS fact for one physical key is rejected;
3. read fact and CAS fact may both mention the same key if the implementation
   intentionally treats them as independent L7F requirements;
4. if the implementation rejects read+CAS overlap, the test name and plan note
   must say so explicitly.

### 6. Expiry And Tombstone Semantics

Required cases:

1. put with `CommitExpiry::None` stamps `expires_at = Timestamp::EPOCH`;
2. put with `CommitExpiry::At(t)` stamps `expires_at = t`;
3. put with `CommitExpiry::At(Timestamp::EPOCH)` is rejected;
4. delete stamps `StorageRow::tombstone`;
5. delete stamps `expires_at = Timestamp::EPOCH`;
6. delete stamps empty value bytes;
7. put with empty value remains a non-tombstone row.

Assertions:

1. no wall-clock time is read during expiry handling;
2. no TTL cleanup or retention behavior runs in L7B.

### 7. Validation Fact Shape

Required cases:

1. empty validation facts are accepted;
2. read fact with `CommitObservedVersion::Missing` is accepted;
3. read fact with `CommitObservedVersion::Present(v)` is accepted for real
   committed versions;
4. CAS fact with `Missing` is accepted;
5. CAS fact with `Present(v)` is accepted for real committed versions;
6. `Present(CommitVersion::ZERO)` is either rejected or documented as
   equivalent to `Missing`, with tests pinning the chosen behavior;
7. validation facts are retained in caller order if later diagnostics need that
   order.

Assertions:

1. L7B does not query L6 to decide whether a fact is true;
2. conflict success/failure is not tested here because it belongs to L7F.

### 8. Stamping Invariants

Required cases:

1. all stamped rows carry the supplied branch;
2. all stamped rows carry the supplied commit version;
3. all stamped rows carry the supplied commit timestamp;
4. stamping preserves mutation order;
5. value bytes round-trip exactly, including empty, high-bit, and long values;
6. put/delete mixed batch stamps one row per mutation;
7. stamped rows contain only user mutation rows, not timeline rows;
8. stamped row metadata preserves put retention hints and carries no retention
   hint for deletes;
9. stamping a read-only diagnostic batch is rejected;
10. stamping a structurally invalid batch is impossible through the validated
   wrapper or returns an error before rows are emitted.

Assertions:

1. no version allocator API is called;
2. no timestamp source API is called;
3. no WAL or branch state API is called.

### 9. Error Vocabulary

Required cases:

1. invalid batch errors are constructible;
2. invalid mutation errors are constructible;
3. invalid validation-fact errors are constructible;
4. duplicate-key errors are constructible;
5. branch-mismatch errors are constructible;
6. storage-owned-space errors are constructible;
7. displays include the category and omit raw value bytes;
8. displays do not use product transaction vocabulary:
   - `TransactionContext`;
   - `TransactionManager`;
   - `rollback`;
   - `VersionedValue`;
   - `Value`;
   - `Key`;
   - `EntityRef`;
   - `JsonValue`;
   - `Graph`;
   - `Vector`;
   - `Search`.

## Generated Testkit Coverage

Extend `check_commit_runtime_scaffold_contract` or split a new
`check_commit_runtime_batch_contract`.

Generated scripts should vary:

1. target branch byte;
2. mutation count from 0 to at least 32 in normal property cases;
3. puts vs deletes;
4. engine storage-space id;
5. user-key bytes;
6. value bytes;
7. expiry mode;
8. duplicate-key injection;
9. branch-mismatch injection;
10. validation fact count;
11. read vs CAS fact shape;
12. durability mode;
13. conflict-validation mode;
14. timestamp policy;
15. stamp version and timestamp.

The generated outcome should expose nonzero counters for:

1. valid batches;
2. invalid empty mutating batches;
3. limit rejections;
4. branch mismatch rejections;
5. storage-owned-space rejections;
6. duplicate mutation rejections;
7. duplicate validation fact rejections;
8. expiry rejections;
9. stamping successes;
10. stamping rejections.

The property test must assert the counters, not just file or symbol presence.

## Source Guard Additions

Update `commit_runtime_source_guard.rs` so L7B production files:

1. may import `crate::row`;
2. may import `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
3. still may not import `crate::branch`;
4. still may not import `crate::table`;
5. still may not import `crate::backend`;
6. still may not import `crate::layout`;
7. still may not import `crate::object`;
8. still may not import `crate::service::wal`;
9. still may not import `crate::format::wal`;
10. still may not use filesystem/path/env APIs;
11. still may not expose bare `pub`;
12. still may not use product transaction/value vocabulary.

Add source-guard self-tests for:

1. allowed `crate::row::{PhysicalKey, StorageRow, StorageSpaceId}`;
2. rejected `crate::branch::BranchState`;
3. rejected `crate::service::wal::WalService`;
4. rejected `crate::format::wal::WalRecord`;
5. rejected `crate::table::TableRow`;
6. rejected bare `pub enum CommitBatch`.

## Negative Non-Behavior Tests

L7B tests must not prove later behavior by accident.

The suite should fail if production L7B code contains:

1. a version allocator type or function;
2. a timestamp source implementation;
3. a call to `append_committed_row`;
4. a call to a WAL service append method;
5. a timeline row encoder;
6. a visible-version publisher;
7. a replay entrypoint.

Prefer source guards or direct module tests over doc-inventory tests.

## Sensitivity Probes

Before closing L7B, run and record these local mutations:

| Probe | Mutation | Expected failing test |
|---|---|---|
| B1 | Accept empty mutating batch. | Direct invalid-batch test and generated counter route. |
| B2 | Skip branch validation for put. | Branch-mismatch direct/generated tests. |
| B3 | Skip branch validation for validation facts. | Validation-fact branch mismatch tests. |
| B4 | Allow storage-owned timeline key as caller mutation. | Storage-space validation tests. |
| B5 | Allow duplicate mutation keys. | Duplicate mutation policy tests. |
| B6 | Treat put/delete same key as valid. | Duplicate mixed-operation tests. |
| B7 | Stamp delete as non-tombstone. | Tombstone stamping test. |
| B8 | Stamp put with wrong commit timestamp. | Stamping invariant test. |
| B9 | Drop or reorder mutation rows during stamping. | Mutation order preservation test. |
| B10 | Dump value bytes in error display. | Error vocabulary test. |
| B11 | Add `crate::branch` import to `src/commit`. | Source guard. |
| B12 | Add `crate::format::wal` import to L7B code. | Source guard. |

Record probe status in `m4-l7-porting-log.md`.

## Command Matrix

Mandatory L7B commands:

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

Run the broader storage-next test suite if L7B changes `row`, source guards
shared by other tests, testkit exports, or feature gating.

## Exit Gate

L7B test coverage is complete when:

1. every direct test category above has a concrete test name;
2. generated tests exercise both valid and invalid batch scripts;
3. source guards enforce the L7B boundary;
4. mutation stamping has no hidden allocator, WAL, L6, timeline, or visibility
   side effects;
5. sensitivity probes are recorded in the porting log;
6. no-default, wasm, clippy, fmt, and diff checks pass.
