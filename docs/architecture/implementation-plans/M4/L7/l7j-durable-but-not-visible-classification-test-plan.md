# L7J Test Plan: Durable-But-Not-Visible Classification

Status: draft test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7j-durable-but-not-visible-classification-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that once a durable commit has crossed the WAL success boundary, every
failure before normal visibility becomes an explicit unresolved durable fact and
blocks later mutating commits.

The suite must fail if L7J:

1. reports post-WAL L6 apply failure as a clean retryable failure;
2. reports post-WAL visible publish failure as visible success;
3. loses the branch/version/timestamp for an unresolved durable commit;
4. fails to record an unresolved durable gate before returning an error;
5. allows a later cache commit while the gate is set;
6. allows a later durable commit while the gate is set;
7. blocks read-only diagnostics unnecessarily;
8. overwrites one unresolved durable fact with a different fact;
9. stores user value bytes in the gate;
10. regresses the successful L7I durable path.

Do not add tests that only prove planning documents exist or link to each other.
L7J automated tests should exercise commit behavior, gate state, fault windows,
generated model parity, or source boundaries.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/durable_gate.rs` for direct gate fact
   and gate-state tests.
2. `crates/storage-next/src/commit/tests/durable.rs` for durable executor
   post-WAL failure tests if the fixture remains local to that file.
3. `crates/storage-next/src/commit/tests/cache.rs` for cache commit blocked by
   unresolved durable state.
4. `crates/storage-next/src/testkit/commit_runtime_durable.rs` or a new
   `commit_runtime_durable_gate.rs` for generated gate contracts.
5. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   counter assertions.
6. `crates/storage-next/tests/commit_runtime_faults.rs` for behavioral fault
   windows that are awkward as module-local tests.
7. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary checks.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. deterministic manual timestamp source;
3. real `CommitBranchRegistry`;
4. real `CommitBranchGuardSet`;
5. real `CommitFactAllocator`;
6. fake `CommitWalAppender` that succeeds before the injected failure;
7. fake L6 apply target for post-WAL apply failure;
8. fake visible publisher for post-L6 visible failure;
9. real `BranchLocalState` smoke tests for successful paths;
10. opaque value bytes only;
11. no engine DTOs, JSON, graph, vector, search, public transaction-session,
    product `as_of`, remote, hub, or dataset vocabulary.

Fault fakes must record call order:

```text
admit -> conflict -> allocate -> wal_append -> l6_apply -> publish_visible
```

The order recorder should prove the gate is recorded after the failed stage and
before the branch guard can be reacquired for another mutating commit.

## Direct Test Matrix

### 1. Unresolved Durable Fact Validation

Required cases:

1. `DurableNotApplied` accepts allocated and durable versions only;
2. `DurableNotApplied` rejects applied version;
3. `DurableNotApplied` rejects timeline version;
4. `DurableNotApplied` rejects visible version;
5. `AppliedNotVisible` accepts allocated, durable, applied, and timeline
   versions;
6. `AppliedNotVisible` rejects missing applied version;
7. `AppliedNotVisible` rejects missing timeline version;
8. `AppliedNotVisible` rejects visible version equal to the commit version;
9. every unresolved fact rejects `CommitDurabilityClass::NotDurable`;
10. zero commit version is rejected through existing commit-stamp validation;
11. display/debug output includes branch and version but not value bytes.

Assertions:

1. validation uses `CommitVisibilityFacts::validate`;
2. no duplicate phase vocabulary is introduced;
3. invalid facts return `CommitRuntimeError`, not panics.

### 2. Gate State

Required cases:

1. empty gate allows mutating commit admission;
2. gate with unresolved fact rejects mutating admission;
3. rejection error includes unresolved branch and version;
4. recording the first valid fact succeeds;
5. recording the exact same fact twice is idempotent;
6. recording a different fact fails closed;
7. exact clear succeeds;
8. clearing with a different fact fails closed;
9. clear on empty gate is either idempotent or explicitly rejected; document the
   chosen behavior in the test name;
10. gate never stores user row values.

Assertions:

1. `unresolved()` returns a copy of bounded facts;
2. internal lock poisoning, if a lock is used, returns typed error or recovers
   deterministically according to local pattern;
3. gate state is not process-global.

### 3. Durable Apply Failure After WAL Success

Required cases:

1. fake WAL append succeeds;
2. fake L6 apply fails before installing rows;
3. result is `CommitRuntimeError::DurableButNotVisible`;
4. recorded gate kind is `DurableNotApplied`;
5. gate fact contains allocated and durable versions equal to the commit
   version;
6. gate fact does not contain applied, timeline, or visible version;
7. lower-layer apply error is preserved as the error source;
8. visible publisher is not called;
9. branch guard is released after gate recording;
10. retrying a normal mutating commit is blocked by the gate.

Assertions:

1. the WAL record payload rows equal the rows that would have been applied;
2. no visible outcome is returned;
3. allocator may have advanced; this is acceptable because durability exists.

### 4. Visible Publish Failure After L6 Apply

Required cases:

1. fake WAL append succeeds;
2. fake L6 apply succeeds;
3. fake visible publisher fails;
4. result is `CommitRuntimeError::DurableButNotVisible`;
5. recorded gate kind is `AppliedNotVisible`;
6. gate fact contains allocated, durable, applied, and timeline versions equal
   to the commit version;
7. gate fact does not contain visible version;
8. lower-layer visible error is preserved as the error source;
9. rows are readable through the branch target when using a real L6 target, but
   the visible tracker remains below the commit version;
10. retrying a normal mutating commit is blocked by the gate.

Assertions:

1. the result is not `AppliedButNotVisible` with `NotDurable`;
2. the result is not a visible durable success;
3. the gate reason distinguishes visible-publication failure from L6 apply
   failure.

### 5. Normal-Write Gate Coverage

Required cases:

1. cache mutating commit is blocked by unresolved durable state before
   allocation;
2. durable mutating commit is blocked by unresolved durable state before
   allocation;
3. blocked commit does not append WAL;
4. blocked commit does not apply L6 rows;
5. blocked commit does not publish visible version;
6. branch guard is not retained after blocked rejection;
7. read-only diagnostic is allowed while the gate is set;
8. read-only diagnostic reports the current visible version, not the unresolved
   durable version.

Assertions:

1. gate check runs before version allocation;
2. gate check runs before WAL append;
3. gate blocks all branches, not only the branch recorded in the fact.

### 6. Successful Path Regression

Required cases:

1. standard durable success still appends WAL, applies L6, and publishes
   visible;
2. always durable success still requires forced durable append fact;
3. cache success still applies and publishes non-durable state when gate is
   empty;
4. clean WAL append failure still leaves no gate because no durable fact exists;
5. uncertain WAL append failure still returns durability-uncertain and leaves no
   durable-but-not-visible gate.

Assertions:

1. L7J must not turn pre-durable failures into unresolved durable facts;
2. L7J must not make successful commits slower by adding unnecessary value-byte
   clones into gate state.

### 7. Source Chains And Vocabulary

Required cases:

1. L6 apply failure source is reachable via `Error::source`;
2. visible publisher failure source is reachable via `Error::source`;
3. blocked-write error display uses storage vocabulary;
4. debug/display output does not contain product transaction/session terms;
5. debug/display output does not contain user value bytes.

Assertions:

1. no source chain is replaced with a static string when a lower-layer error
   exists;
2. `PartialEq` ignores source identity consistently with existing
   `CommitRuntimeError` behavior.

## Generated Testkit Matrix

Extend the generated commit-runtime harness with counters for:

1. valid unresolved durable facts;
2. invalid unresolved durable fact rejection;
3. first gate record;
4. idempotent gate record;
5. different-fact rejection;
6. exact gate clear hook;
7. durable apply failure after WAL success;
8. visible publish failure after L6 apply;
9. cache commit blocked by gate;
10. durable commit blocked by gate;
11. read-only diagnostic allowed by gate;
12. clean WAL failure does not set gate;
13. uncertain WAL failure does not set durable-but-not-visible gate.

Generated scripts should vary:

1. branch id;
2. commit version floor;
3. timestamp;
4. durability class;
5. post-WAL failure kind;
6. blocked follow-up mode: cache or durable;
7. target branch for the blocked follow-up;
8. exact-vs-different duplicate gate fact;
9. visible tracker starting version;
10. mutation count and put/delete mix.

The independent model should track:

```text
ModelDurableGate {
  unresolved: Option<ModelUnresolvedDurable>,
  allocated_version
  durable_version
  applied_version
  timeline_version
  visible_version
  blocked_mutations
}
```

The model must not call production gate, branch, WAL, or visible tracker code to
compute expected outcomes.

## Fault Windows

Direct or generated tests must cover:

1. invalid batch before gate check;
2. gate blocks before allocation;
3. clean WAL append failure before durable fact exists;
4. uncertain WAL append failure before L6 apply;
5. WAL success then L6 apply failure;
6. WAL success then L6 apply success then visible publish failure;
7. gate record exact duplicate;
8. gate record different fact;
9. cache commit after gate;
10. durable commit after gate;
11. read-only diagnostic after gate;
12. gate clear exact fact hook.

## Source Guards

Update `commit_runtime_source_guard.rs` if L7J adds files.

Production `commit/` code may continue to import:

1. the narrow L6 branch boundary already allowed for `cache.rs`/`durable.rs`;
2. the narrow L4 WAL boundary already allowed for `durable.rs`;
3. `crate::config::mode::DurabilityPolicy`;
4. `std::sync` primitives if the gate uses interior mutability.

The guard must continue to reject:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::object`;
4. direct filesystem/path/environment APIs;
5. engine/product modules;
6. public transaction-session vocabulary;
7. JSON, graph, vector, search, event, embedding, remote, hub, or dataset
   terms;
8. user value bytes in debug/display fixtures.

## Sensitivity Probes

The L7J suite should fail under these semantic mutations:

1. skip gate recording on post-WAL L6 apply failure;
2. skip gate recording on post-WAL visible failure;
3. record `DurableNotApplied` with applied version set;
4. record `AppliedNotVisible` without applied version;
5. allow cache commit while gate is set;
6. allow durable commit while gate is set;
7. block read-only diagnostic while gate is set;
8. overwrite unresolved fact with a different fact;
9. classify clean WAL append failure as durable-but-not-visible;
10. classify uncertain WAL append failure as clean durable-but-not-visible;
11. drop lower-layer source chain;
12. store user value bytes in gate facts.

Record any probes that are run in
`docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`.

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
cargo test -p strata-storage-next --all-features --locked --lib commit::tests::durable
cargo test -p strata-storage-next --all-features --locked --lib commit::tests::durable_gate
cargo test -p strata-storage-next --all-features --locked --lib commit::tests::cache
```

## Exit Criteria

L7J is complete when:

1. post-WAL L6 apply failure records and returns `DurableNotApplied`;
2. post-WAL visible publish failure records and returns `AppliedNotVisible`;
3. the unresolved fact blocks later cache and durable mutating commits;
4. read-only diagnostics remain allowed;
5. exact duplicate gate recording is idempotent;
6. different unresolved fact recording fails closed;
7. clean and uncertain WAL failures do not set the durable-but-not-visible gate;
8. generated properties include durable-gate counters;
9. source guards remain green;
10. L7H and L7I successful paths do not regress.
