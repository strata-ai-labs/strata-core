# L7E Test Plan: Branch Registry And Commit Guards

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7e-branch-registry-commit-guards-implementation-plan.md`

## Goal

Prove that L7E admits or rejects target-branch mutating commits before any
allocation, timestamping, WAL append, L6 mutation, timeline install, or visible
publication.

The suite must fail if L7E:

1. commits to a missing branch;
2. commits to a deleting or deleted branch;
3. ignores a supplied stale branch generation;
4. treats branch-id reuse as safe when an exact stale generation is supplied;
5. allows two same-branch mutating guards at once;
6. leaks a branch guard after an error path;
7. allows new mutating commits while quiesce is active;
8. starts quiesce while mutating guards are active;
9. blocks read-only diagnostics through the mutating guard path;
10. allocates versions or timestamps before branch admission succeeds;
11. imports L6, WAL, backend, layout, filesystem, table internals, or product
    transaction APIs for branch admission.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/branch_registry.rs` for direct
   registry, descriptor, lifecycle, generation, and admission tests.
2. `crates/storage-next/src/commit/tests/guard.rs` for direct guard and quiesce
   skeleton tests.
3. `crates/storage-next/src/commit/tests/scaffold.rs` only for shared shell
   assertions that remain relevant.
4. `crates/storage-next/src/testkit/commit_runtime_branch_guards.rs` or
   `crates/storage-next/src/testkit/commit_runtime/branch_guards.rs` for
   generated L7E contracts.
5. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7E
   counter assertions.
6. `crates/storage-next/tests/commit_runtime_source_guard.rs` for production
   boundary and forbidden-vocabulary checks.

Do not add tests that only prove planning documents exist or link to each
other. L7E automated tests should exercise implementation behavior, generated
coverage, or source boundaries.

## Direct Test Matrix

### 1. Branch Generation Facts

Required cases:

1. valid generation constructs successfully;
2. invalid sentinel generation is rejected if the type reserves one;
3. exact generation guard preserves the supplied generation;
4. `NotSupplied` generation guard is represented explicitly;
5. exact mismatch returns a typed commit-runtime error;
6. exact match admits the generation check;
7. generation comparison does not inspect commit version or timestamp;
8. debug/display output is bounded and product-free.

Assertions:

1. generation is a branch-admission fact, not a commit version;
2. generation mismatch happens before allocation;
3. generation validation does not import L6 or WAL.

### 2. Descriptor Validation

Required cases:

1. active descriptor validates;
2. descriptor branch id must match its registry key;
3. deleting descriptor is visible as deleting;
4. deleted descriptor is visible as deleted;
5. invalid generation rejects the descriptor;
6. descriptor contains no product branch metadata.

Assertions:

1. descriptor validation is deterministic;
2. descriptor validation does not allocate;
3. descriptor validation does not mutate the registry.

### 3. Registry Registration And Lookup

Required cases:

1. empty registry lookup returns missing branch;
2. registering one branch makes it visible to lookup;
3. duplicate active registration rejects;
4. registering multiple branches keeps descriptors isolated;
5. branch A lifecycle changes do not mutate branch B;
6. lookup returns descriptor facts without acquiring commit guards;
7. lookup does not allocate a commit version.

Assertions:

1. missing branch uses commit-runtime error vocabulary;
2. duplicate branch create is tested as internal registry behavior, not as a
   public user API;
3. registry order does not affect lookup results.

### 4. Branch Deleting And Deleted Rejection

Required cases:

1. marking active branch deleting succeeds;
2. marking missing branch deleting rejects;
3. mutating admission to deleting branch rejects;
4. marking deleted or removing descriptor prevents mutating admission;
5. branch can be recreated only with a valid newer generation if L7E supports
   recreate;
6. recreating with the same generation rejects;
7. stale generation from before deletion rejects after recreate.

Assertions:

1. deleting/deleted rejection happens before allocation;
2. deletion marker does not release L6 rows in L7E;
3. deletion marker does not publish visibility.

### 5. Mutating Admission

Required cases:

1. active branch with matching supplied generation admits;
2. active branch with no supplied generation follows V1 optional-generation
   policy;
3. active branch with mismatched supplied generation rejects;
4. missing branch rejects;
5. deleting branch rejects;
6. deleted branch rejects;
7. read-only batch rejects through mutating-admission helper;
8. target branch in admission equals the validated batch branch.

Assertions:

1. rejection does not call `CommitFactAllocator`;
2. rejection does not call timestamp source;
3. rejection does not call L6;
4. rejection does not append WAL;
5. rejection does not mutate visible-version tracker.

### 6. Same-Branch Guard Serialization

Required cases:

1. first mutating guard for branch A succeeds;
2. second mutating guard for branch A rejects while first token is live;
3. guard for branch A releases when token drops;
4. guard for branch A can be reacquired after release;
5. guard release is idempotent from caller perspective, even though token is
   not cloneable;
6. guard token debug output does not expose internal state dumps.

Assertions:

1. guard release happens through RAII;
2. guard release happens after validation failure when the admission helper
   owns the token;
3. guard state cannot go negative or double-release.

### 7. Cross-Branch Guard Independence

Required cases:

1. guard for branch A and guard for branch B can both be live;
2. releasing branch A does not release branch B;
3. rejecting a second branch A guard does not affect branch B;
4. branch id ordering does not matter;
5. generated cases cover at least two branch ids.

Assertions:

1. per-branch guard does not claim cross-branch atomicity;
2. global visible-version ordering remains owned by later commit paths;
3. no branch id is encoded into commit version facts.

### 8. Quiesce Skeleton

Required cases:

1. quiesce can start when no mutating guards are active;
2. quiesce token blocks new mutating guard acquisition;
3. dropping quiesce token allows later mutating guard acquisition;
4. quiesce cannot start while branch A guard is active;
5. quiesce cannot start while multiple branch guards are active;
6. failed quiesce attempt does not block later commits;
7. repeated quiesce start while quiesce is active rejects.

Assertions:

1. L7E quiesce is nonblocking;
2. L7E quiesce does not implement timeout scheduling;
3. L7E quiesce does not mutate L6 or visible-version facts.

### 9. Read-Only During Quiesce

Required cases:

1. read-only diagnostic path does not acquire mutating branch guard;
2. read-only diagnostic path may run while quiesce token is live under L7E
   policy;
3. disabled read-only diagnostics still reject through L7D config;
4. read-only outcome during quiesce does not allocate;
5. read-only outcome during quiesce does not mutate guard state.

Assertions:

1. read-only policy is documented in implementation comments or tests;
2. checkpoint/recovery stronger barriers remain deferred to L7L/L8;
3. read-only diagnostics do not bypass mutating branch rejection because they
   are not mutating commits.

### 10. Admission Ordering

Required cases:

1. missing branch rejects before allocation;
2. deleting branch rejects before allocation;
3. generation mismatch rejects before allocation;
4. quiesce active rejects before allocation;
5. same-branch guard contention rejects before allocation;
6. successful admission still does not allocate by itself;
7. successful admission returns facts later slices can consume.

Assertions:

1. use spy allocators/timestamp sources where helpful;
2. admission returns no commit version;
3. admission returns no commit timestamp;
4. admission returns no durable or visible outcome.

### 11. Error Vocabulary

Required cases:

1. missing branch display is storage-shaped;
2. branch deleting display is storage-shaped;
3. generation mismatch display is storage-shaped;
4. quiesce active display is storage-shaped;
5. same-branch guard contention display is storage-shaped;
6. duplicate registration display is storage-shaped.

Assertions:

1. errors do not mention user sessions, public transactions, datasets, remotes,
   or product branch commands;
2. errors preserve enough branch/generation facts for diagnostics without
   dumping row values;
3. errors remain comparable in tests without relying on full strings when a
   typed variant exists.

## Generated Testkit Matrix

Extend the commit-runtime property harness with counters for:

1. branch registration success;
2. duplicate registration rejection;
3. missing branch rejection;
4. deleting/deleted branch rejection;
5. generation exact match;
6. generation mismatch;
7. generation not supplied under V1 policy;
8. stale generation after recreate;
9. same-branch guard contention;
10. different-branch simultaneous guards;
11. quiesce start success;
12. quiesce rejected due to active guards;
13. mutating guard rejected during quiesce;
14. read-only diagnostic allowed during quiesce;
15. guard release and reacquire.

The generated harness should vary:

1. branch id;
2. generation;
3. branch lifecycle state;
4. operation ordering;
5. guard acquire/drop ordering;
6. quiesce start/drop points;
7. mutating-vs-read-only batch kind;
8. supplied-vs-unsupplied generation facts.

The generated harness must not:

1. call L6 branch mutation;
2. append WAL;
3. construct timeline rows;
4. allocate commit versions;
5. allocate commit timestamps;
6. use engine/product DTOs;
7. assert that documentation files exist.

## Source Guards

The commit-runtime source guard should reject:

1. `pub mod commit`;
2. public `pub` commit-runtime type/function leaks;
3. product transaction vocabulary;
4. `VersionedValue`/product value/key vocabulary;
5. direct `crate::table` internals;
6. `crate::backend`, `crate::layout`, or object-name builders;
7. `std::fs`, `std::path`, `std::env`, `File`, mmap, and process-global
   mutable state;
8. engine crates;
9. product branch command/API DTOs.

L7E may use standard library synchronization/data-structure primitives, but it
must not create process-global guard state. Guard state must be owned by an
explicit runtime value.

## Sensitivity Probes For This Slice

Record these in the L7 porting log when the implementation lands:

| Probe | Mutation | Expected failure |
|---|---|---|
| E1 | Treat missing branch as active. | Missing branch direct/generated tests fail. |
| E2 | Ignore deleting marker. | Deleting branch admission tests fail. |
| E3 | Ignore supplied generation mismatch. | Generation mismatch tests fail. |
| E4 | Allow same-branch double guard. | Same-branch serialization tests fail. |
| E5 | Forget to release guard on drop. | Reacquire-after-drop tests fail. |
| E6 | Allow mutating guard during quiesce. | Quiesce-blocks-mutating tests fail. |
| E7 | Start quiesce while guard is active. | Quiesce-active-guard tests fail. |
| E8 | Route read-only through mutating guard. | Read-only-during-quiesce test fails. |
| E9 | Allocate version before branch rejection. | Spy allocator no-call test fails. |
| E10 | Import backend/layout/filesystem from commit guard code. | Source guard fails. |

## Verification Commands

During implementation:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib commit --quiet
cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties --quiet
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties --quiet
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard --quiet
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

L7E closeout evidence should report implementation tests and generated
counters. It should not rely on tests that only inspect documentation paths.
