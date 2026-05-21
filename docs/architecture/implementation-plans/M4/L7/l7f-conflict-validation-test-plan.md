# L7F Test Plan: Conflict Validation

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7f-conflict-validation-implementation-plan.md`

## Goal

Prove that L7F validates optional read-set and CAS facts against the target
branch read view before version allocation, timestamp allocation, WAL append,
L6 mutation, timeline install, or visible publication.

The suite must fail if L7F:

1. accepts a changed read-set fact;
2. accepts a mismatched CAS fact;
3. rejects a blind write as a conflict;
4. reads L6 when conflict validation mode is `Skip`;
5. allocates a commit version before conflict validation;
6. loses lower-layer read errors or their source chain;
7. validates against the wrong branch read view;
8. treats product transaction concepts as part of the error vocabulary;
9. imports table/backend/WAL/filesystem internals for conflict checking.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/conflict.rs` for direct conflict
   validation tests.
2. `crates/storage-next/src/commit/tests/batch.rs` only for structural
   validation fact tests that already belong to L7B.
3. `crates/storage-next/src/testkit/commit_runtime_conflicts.rs` or
   `crates/storage-next/src/testkit/commit_runtime/conflicts.rs` for generated
   L7F contracts.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7F
   counter assertions.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for boundary and
   forbidden-vocabulary checks.

Do not add tests that only prove planning documents exist or link to each
other. L7F automated tests should exercise implementation behavior, generated
coverage, or source boundaries.

## Fixture Rules

Direct tests should use two source styles:

1. a real L6 `BranchReadView` fixture for branch integration behavior;
2. a fake `CommitConflictReadSource` fixture for no-read, read-count, and
   injected-error behavior.

Fixture rows should use:

1. one target branch;
2. at least one non-target branch for wrong-view checks;
3. one key currently present at version `V`;
4. one key currently missing;
5. one key deleted by a tombstone;
6. one key changed from version `V` to version `V+1`;
7. opaque value bytes only.

Tests must not use engine value types, JSON, public transactions, or product
branch commands.

## Direct Test Matrix

### 1. Current Observed Version

Required cases:

1. present visible row maps to `CommitObservedVersion::Present(version)`;
2. missing key maps to `CommitObservedVersion::Missing`;
3. latest tombstone maps to `Missing` under the V1 visible-row conflict model;
4. branch mismatch from the read view maps to a commit-runtime lower-layer
   error;
5. lower-layer error preserves source chain;
6. observed-version conversion does not clone row values.

Assertions:

1. current observation uses only L6 read-view APIs;
2. current observation does not inspect table internals;
3. current observation does not allocate commit facts.

### 2. Read-Set Present Facts

Required cases:

1. observed present version equals current present version and passes;
2. observed present version differs from current present version and rejects;
3. observed present version becomes missing after delete and rejects;
4. read-set conflict reports `ReadSet` kind;
5. read-set conflict includes expected and actual observed versions;
6. read-set success increments checked-read-fact count.

Assertions:

1. rejection happens before version allocation;
2. rejection does not acquire WAL or L6 mutation handles;
3. display output is storage-shaped and bounded.

### 3. Read-Set Missing Facts

Required cases:

1. observed missing remains missing and passes;
2. observed missing becomes present and rejects;
3. observed missing remains missing through a visible tombstone and passes;
4. missing-vs-present conflict reports actual `Present(version)`;
5. multiple read facts are checked in deterministic order.

Assertions:

1. missing facts do not use `CommitVersion::ZERO` internally as a present
   version;
2. missing facts do not dump user-key bytes in display output;
3. missing facts remain branch-scoped.

### 4. CAS Present Facts

Required cases:

1. expected present version equals current present version and passes;
2. expected present version differs from current present version and rejects;
3. expected present version becomes missing after delete and rejects;
4. CAS conflict reports `Cas` kind;
5. CAS conflict includes expected and actual observed versions.

Assertions:

1. CAS validation is separate from read-set validation;
2. CAS facts do not implicitly add read-set facts;
3. CAS rejection happens before allocation.

### 5. CAS Missing Facts

Required cases:

1. expected missing remains missing and passes;
2. expected missing becomes present and rejects;
3. expected missing remains missing through a visible tombstone and passes;
4. CAS missing conflict reports actual `Present(version)`;
5. CAS missing match does not read value bytes.

Assertions:

1. expected missing means no visible current row;
2. expected missing is not represented as present version zero;
3. the behavior matches L7B validation-fact semantics.

### 6. Combined Read-Set And CAS

Required cases:

1. read-set facts are checked before CAS facts;
2. first read-set conflict is returned before later CAS facts are read;
3. if all read-set facts pass, CAS facts are checked;
4. all passing facts return a report with both checked counts;
5. lower-layer read failure after some successful checks returns a lower-layer
   error rather than a conflict.

Assertions:

1. deterministic order does not depend on map iteration;
2. reports count facts, not source read attempts after failure;
3. source failures preserve source chains.

### 7. Blind Writes

Required cases:

1. blind put over a changed key passes conflict validation;
2. blind delete over a changed key passes conflict validation;
3. blind put over a missing key passes;
4. blind delete over a missing key passes;
5. blind writes do not read the conflict source when validation facts are empty.

Assertions:

1. L7F preserves snapshot-isolation style behavior;
2. L7F does not claim serializable transactions;
3. blind-write success does not imply commit success in later slices.

### 8. Skip Mode

Required cases:

1. `CommitConflictValidationMode::Skip` passes with empty facts;
2. skip mode passes even when read-set facts would mismatch;
3. skip mode passes even when CAS facts would mismatch;
4. skip mode performs zero source reads;
5. skip report is distinguishable from validate-with-empty-facts report.

Assertions:

1. skip mode is available for replay and internal trusted paths;
2. skip mode does not bypass structural batch validation from L7B;
3. skip mode does not allocate or mutate anything.

### 9. Branch Scope

Required cases:

1. L7B still rejects validation facts whose keys are outside the batch branch;
2. L7F validates against the supplied target branch read view;
3. wrong branch read view produces a lower-layer branch mismatch error;
4. branch A conflicts do not inspect branch B state;
5. generated cases vary at least two branch ids.

Assertions:

1. branch mismatch remains storage-shaped;
2. L7F does not rewrite physical keys;
3. L7F does not reach into inherited-layer internals.

### 10. Ordering Before Allocation

Required cases:

1. read-set mismatch rejects before `CommitFactAllocator::allocate`;
2. CAS mismatch rejects before `CommitFactAllocator::allocate`;
3. lower-layer read failure rejects before allocation;
4. skip mode with mismatching facts can proceed to later allocation in future
   integrated tests;
5. successful validation itself returns no version or timestamp.

Assertions:

1. use spy allocators or a pipeline harness when helpful;
2. conflict validation returns only validation facts/reports;
3. no visible-version tracker is touched.

### 11. Error Vocabulary

Required cases:

1. read-set conflict display is bounded and storage-shaped;
2. CAS conflict display is bounded and storage-shaped;
3. lower-layer branch-read error preserves source;
4. conflict errors are comparable by typed facts;
5. errors do not mention sessions, public transactions, rollback, datasets,
   remotes, documents, entities, JSON, graph, vector, or search.
6. same-length user keys in the same branch and storage space produce distinct
   conflict facts through a stable key fingerprint without displaying the key
   bytes.

Assertions:

1. error variants include enough facts for diagnostics;
2. error variants do not clone row value bytes;
3. source-chain behavior is tested separately for conflicts vs lower-layer
   failures.

## Generated Testkit Matrix

Extend the commit-runtime property harness with counters for:

1. read-set present match;
2. read-set present mismatch;
3. read-set present becoming missing;
4. read-set missing match;
5. read-set missing becoming present;
6. CAS present match;
7. CAS present mismatch;
8. CAS present becoming missing;
9. CAS missing match;
10. CAS missing becoming present;
11. combined read-set-before-CAS ordering;
12. blind put no-conflict;
13. blind delete no-conflict;
14. skip mode no-read;
15. lower-layer read failure classification;
16. conflict error vocabulary.

The generated harness should vary:

1. branch id;
2. physical storage space;
3. user-key bytes;
4. current observed version;
5. expected observed version;
6. read-set vs CAS facts;
7. validation mode;
8. source failure position;
9. blind mutation kind;
10. tombstone/missing/current-present state.

Each generated case should compare production output to an independent model
function:

```text
model_current_observed_version(key, model_branch_state)
model_validate_read_set(facts, model_branch_state)
model_validate_cas_set(facts, model_branch_state)
```

Do not derive expected results by calling the production validator twice.

## Source Guard Matrix

`commit_runtime_source_guard.rs` should enforce:

1. `commit/conflict.rs` may import only the approved L6 read-view surface;
2. all other commit modules remain forbidden from importing `crate::branch`;
3. broad branch imports such as `use crate::branch::*` fail;
4. branch state mutation APIs are forbidden in `commit/conflict.rs`;
5. table, backend, object, layout, WAL format/service, filesystem, environment,
   and wall-clock APIs remain forbidden;
6. product transaction/session vocabulary remains forbidden.

Add fixture assertions for:

1. allowed narrow branch read-view import;
2. rejected wildcard branch import;
3. rejected branch mutation import;
4. rejected table import;
5. rejected WAL import.

## Sensitivity Probes

Before closing L7F, record these probes in the porting log:

1. Treat every read-set fact as matched.
2. Treat every CAS fact as matched.
3. Compare only present/missing and ignore version.
4. Treat tombstone-hidden rows as present under V1 visible-row semantics.
5. Reject blind writes.
6. Read the source even when validation mode is `Skip`.
7. Validate CAS before read-set.
8. Drop lower-layer source errors.
9. Validate against a non-target branch view.
10. Include row value bytes in conflict display.

Each probe should name the mutation site and the direct or generated test that
failed.

## Required Verification

Run at minimum:

1. `cargo test -p strata-storage-next --locked --lib commit`
2. `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties`
3. `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties`
4. `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard`
5. `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
6. `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
7. `cargo fmt --package strata-storage-next --check`
8. `git diff --check`

## Exit Gate

L7F is complete when:

1. all read-set match/mismatch cases are covered;
2. all CAS match/mismatch cases are covered;
3. blind put/delete cases are explicitly no-conflict;
4. skip mode proves no source reads;
5. lower-layer source chains are preserved;
6. conflict rejection is proven pre-allocation;
7. generated counters cover every conflict category;
8. source guards allow only the narrow L7-to-L6 dependency;
9. no product transaction vocabulary leaks into code or tests;
10. the porting log records the preserved conflict model and sensitivity probes.

## Deferred

1. Full public transaction semantics remain retired.
2. Serializable isolation is not part of V1.
3. Replay conflict bypass is tested in L7K.
4. Durable/cache integrated commit paths are tested in L7H and L7I.
5. Fuzz targets and larger generated scripts are strengthened in L7M.
