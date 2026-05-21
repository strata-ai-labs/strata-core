# L7I Test Plan: WAL Record And Envelope Integration

Status: implemented test plan

Implementation plan:
`docs/architecture/implementation-plans/M4/L7/l7i-wal-record-envelope-integration-implementation-plan.md`

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`

## Goal

Prove that L7I durable local commits write the exact committed row set to L4
WAL before any L6 visibility, and that `standard` and `always` durability
policies are classified correctly.

The suite must fail if L7I:

1. applies rows to L6 before WAL append succeeds;
2. publishes visible version before L6 apply completes;
3. writes different rows to WAL than it applies to L6;
4. bypasses `WalRecord::new` outer-fact validation;
5. manually writes backend objects instead of using L4 WAL service;
6. reports `Always` success without a forced durable append fact;
7. treats an uncertain `Always` sync failure as a clean non-durable failure;
8. turns a WAL append failure into visible L6 state;
9. routes cache/no-WAL mode through durable code;
10. loses the guard, generation, or conflict ordering already proven by L7H.

Do not add tests that only prove planning documents exist or link to each
other. L7I automated tests should exercise commit behavior, WAL record parity,
fault windows, generated model parity, or source boundaries.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests/durable.rs` for direct durable commit
   protocol tests.
2. `crates/storage-next/src/testkit/commit_runtime_durable.rs` for generated
   durable commit contracts.
3. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   counter assertions.
4. `crates/storage-next/tests/commit_runtime_faults.rs` for behavioral fault
   windows that are awkward as module-local tests.
5. `crates/storage-next/tests/commit_runtime_source_guard.rs` for source
   boundary checks.
6. Existing L4 WAL service tests remain L4-owned; L7I tests should use them as
   confidence, not duplicate the full WAL service matrix.

## Fixture Rules

Direct tests should use:

1. deterministic branch ids;
2. deterministic manual timestamp source;
3. one target `BranchLocalState`;
4. real `CommitBranchRegistry`;
5. real `CommitBranchGuardSet`;
6. real `CommitFactAllocator`;
7. real `VisibleVersionTracker`;
8. real L6 read-view assertions after success and failure;
9. a fake `CommitWalAppender` for precise ordering/fault tests;
10. at least one integration-style test using real `WalService` when the
    required backend feature is available;
11. opaque value bytes only;
12. no engine DTOs, JSON, graph, vector, search, public transaction-session,
    product `as_of`, remote, hub, or dataset vocabulary.

The fake WAL appender should record:

1. every appended `WalRecord`;
2. requested/effective durability policy;
3. append order relative to L6 apply and visibility publication;
4. returned segment id, offset, bytes, and forced-durable flag;
5. injected clean append failure;
6. injected uncertain sync failure;
7. injected segment/rotation/record-size failure.

## Direct Test Matrix

### 1. Durable Happy Path

Required cases:

1. single put with `Standard` appends WAL, applies L6, publishes visible;
2. single delete with `Standard` appends tombstone WAL row and hides latest;
3. mixed put/delete with `Standard` writes user rows plus timeline rows;
4. single put with `Always` appends WAL, forces durability, applies L6,
   publishes visible;
5. `Standard` outcome reports `CommitDurabilityClass::Standard`;
6. `Always` outcome reports `CommitDurabilityClass::Always`;
7. both success paths report `CommitOutcomeKind::Visible`;
8. both success paths report `CommitPhase::Visible`;
9. visibility facts contain allocated, durable, applied, timeline, and visible
   versions equal to the commit version;
10. mutation counts include puts, deletes, and two timeline rows.

Assertions:

1. the same `CommitStamp` is used for user rows, timeline rows, and WAL record;
2. branch guard remains held through WAL append, L6 apply, and visibility;
3. no L6 row exists before the fake appender reports append success;
4. no visible version is published before L6 apply completes.

### 2. WAL Record Parity

Required cases:

1. WAL payload rows equal the combined L6 rows;
2. WAL payload row order is deterministic: user rows first, timeline rows
   after;
3. `WalRecord` branch id equals the target branch;
4. `WalRecord` commit version equals the allocated version;
5. `WalRecord` timestamp equals the allocated timestamp;
6. `WalCommitPayload::new` rejects empty row sets;
7. `WalRecord::new` rejects row branch mismatch;
8. `WalRecord::new` rejects row version mismatch;
9. `WalRecord::new` rejects row timestamp mismatch;
10. `WalRecordEnvelope` framing is owned by L4 append, not hand-written by L7.

Assertions:

1. tests should decode the appended record using L3 helpers when real bytes are
   available;
2. tests should not compare hand-written byte literals for WAL internals unless
   they are already L3 golden vectors;
3. L7I must not duplicate row-native payload validation logic.

### 3. Durability Mode Admission

Required cases:

1. `CommitDurabilityMode::Cache` rejects before allocation;
2. `CommitDurabilityMode::Standard` accepts a standard WAL policy;
3. `CommitDurabilityMode::Always` accepts an always WAL policy;
4. `Always` request with non-forced append result rejects;
5. policy mismatch rejects before allocation, if exact policy matching is the
   V1 choice;
6. unsupported durable backend or missing WAL appender rejects before
   allocation.

Assertions:

1. rejection leaves allocator unchanged;
2. rejection leaves L6 unchanged;
3. rejection leaves visible tracker unchanged;
4. rejection does not acquire or retain a branch guard.

### 4. Ordering And Guard Lifetime

Required cases:

1. branch admission happens before conflict validation;
2. conflict validation happens before allocation;
3. allocation happens before WAL record construction;
4. WAL append happens before L6 apply;
5. L6 apply happens before visible publication;
6. guard contention rejects before allocation and WAL append;
7. guard releases after WAL append success and visible success;
8. guard releases after clean WAL failure;
9. guard releases after uncertain WAL failure;
10. guard releases after post-WAL L6/visibility failure.

Assertions:

1. order recorder sequence is stable and exact for success;
2. order recorder sequence stops at the correct failure point;
3. no failure path leaves a guard active.

### 5. Clean Pre-WAL Failures

Required cases:

1. invalid batch rejects before allocation and WAL append;
2. missing branch rejects before allocation and WAL append;
3. deleting branch rejects before allocation and WAL append;
4. deleted branch rejects before allocation and WAL append;
5. generation mismatch rejects before allocation and WAL append;
6. conflict rejects before allocation and WAL append;
7. timestamp source failure rejects before version allocation and WAL append;
8. version overflow rejects before WAL append;
9. row stamping failure after allocation leaves a version gap but no WAL row;
10. WAL record construction failure after allocation leaves a version gap but
    no L6 mutation.

Assertions:

1. no clean pre-WAL failure mutates L6;
2. no clean pre-WAL failure publishes visible version;
3. version gaps after allocation are accepted by the next successful commit.

### 6. WAL Append Failures

Required cases:

1. backend append failure before bytes accepted leaves no L6 rows;
2. record-too-large failure leaves no L6 rows;
3. segment id overflow leaves no L6 rows;
4. segment rotation failure leaves no L6 rows;
5. unexpected append offset leaves no L6 rows;
6. unexpected append length leaves no L6 rows;
7. unexpected object size leaves no L6 rows;
8. repair-uncertain WAL service rejects append and leaves no L6 rows;
9. source chain is preserved for L4 backend/service errors;
10. error vocabulary remains storage-shaped.

Assertions:

1. visible version remains at its pre-commit value;
2. branch active row count remains unchanged;
3. retry after clean WAL failure may allocate a higher version and succeed;
4. clean WAL failures are not reported as durable.

### 7. Always Durability Uncertain

Required cases:

1. `Always` sync failure after append bytes may exist is classified as
   uncertain;
2. uncertain failure leaves no L6 rows;
3. uncertain failure leaves visible version unchanged;
4. uncertain failure does not return visible success;
5. uncertain failure is distinguishable from clean append failure;
6. uncertain failure preserves enough WAL facts or source chain for L7J/L8
   diagnosis.

Assertions:

1. the result reports `CommitDurabilityClass::Uncertain` or an equivalent typed
   uncertainty error;
2. caller cannot treat the commit as safe to blindly retry as if no WAL bytes
   existed.

### 8. Post-WAL Failures

L7I adds the `DurableButNotVisible` error vocabulary and keeps the durable
commit protocol structured so L7J can install the normal-write gate. Concrete
branch-apply and visible-publication fault injection is deferred to L7J because
the L7I executor currently depends on concrete L6 branch state and visible
tracker types rather than injectable fault traits.

Required cases:

1. WAL append success followed by L6 apply failure returns durable-not-visible
   handoff;
2. WAL append success and L6 apply success followed by visible publication
   failure returns applied-not-visible handoff;
3. post-WAL failure does not return visible success;
4. post-WAL failure does not claim cache/non-durable semantics;
5. post-WAL failure preserves the commit version and branch id;
6. L7J follow-up gate can identify that normal writes must pause.

Assertions:

1. if L6 apply failed, the branch read view does not expose partial rows;
2. if L6 apply succeeded but visibility failed, the visible tracker is not
   advanced;
3. the error/outcome is typed enough for L7J to install a write gate.

### 9. Real WAL Service Smoke Tests

Required cases, when local durable features are available:

1. open a real `WalService` in `Standard` mode;
2. execute one durable commit;
3. read the WAL through L4 and verify the record exists;
4. verify the WAL record rows match L6 rows;
5. open a real `WalService` in `Always` mode;
6. execute one durable commit and verify forced durable append fact;
7. ensure cache mode still uses L7H and does not require WAL service.

These tests should be skipped or feature-gated when the backend mode cannot
provide L4 durable WAL capabilities.

## Generated Testkit Matrix

Extend the generated commit-runtime harness with durable counters for:

1. standard durable success;
2. always durable success;
3. WAL payload parity;
4. WAL-before-L6 ordering;
5. L6-before-visible ordering;
6. cache rejected by durable runtime;
7. clean WAL append failure;
8. record-too-large or segment failure;
9. always sync uncertainty;
10. post-WAL L6 failure handoff when L7J fault injection lands;
11. post-WAL visibility failure handoff when L7J fault injection lands;
12. version gap after post-allocation pre-WAL failure;
13. guard release after each durable failure class;
14. source-guard fixture coverage.

Generated scripts should vary:

1. branch id;
2. branch generation;
3. mutation count;
4. put/delete mix;
5. storage space id;
6. validation facts;
7. timestamp policy;
8. durability mode;
9. WAL policy;
10. WAL append fault point;
11. L6 apply fault point;
12. visible tracker starting version.

Each generated case should compare production output to an independent model
that tracks at least:

```text
ModelDurableCommit {
  allocated_version
  durable_version
  applied_version
  visible_version
  wal_rows
  l6_rows
  timeline_rows
  phase
}
```

The model must not call production `WalRecord`, `WalService`, `BranchLocalState`,
or `VisibleVersionTracker` to compute expected outcomes.

## Fault Windows

Direct or generated fault tests must cover:

1. invalid batch before allocation;
2. branch admission failure before allocation;
3. conflict before allocation;
4. timestamp source failure before version allocation;
5. version overflow before WAL;
6. row stamping failure after allocation before WAL;
7. WAL record construction failure after allocation before append;
8. clean append failure before bytes accepted;
9. record too large;
10. segment rotation failure;
11. segment id overflow;
12. unexpected append offset/length/object size;
13. repair-uncertain WAL service;
14. `Always` sync failure after append bytes may exist;
15. L6 staged apply failure after WAL success;
16. visible publication failure after WAL success and L6 apply;
17. guard contention;
18. guard release after every failure class.

## Source Guards

Update `commit_runtime_source_guard.rs` so production `commit/` code may import
only the intended durable dependencies:

1. `crate::format::wal`;
2. `crate::service::wal`;
3. `crate::config::mode::DurabilityPolicy`.

The guard must continue to reject:

1. `crate::backend`;
2. `crate::layout`;
3. `crate::object`;
4. `std::fs`;
5. `std::path::Path`;
6. `std::env`;
7. engine/product modules;
8. public transaction-session vocabulary;
9. JSON, graph, vector, search, event, embedding, remote, hub, or dataset terms.

## Sensitivity Probes

The L7I suite should fail under these semantic mutations:

1. move L6 apply before WAL append;
2. publish visible before L6 apply;
3. omit timeline rows from WAL payload;
4. omit timeline rows from L6 apply;
5. build WAL record from a separately stamped row set;
6. bypass `WalRecord::new`;
7. treat `Always` sync failure as clean non-durable failure;
8. report `Always` success when `forced_durable == false`;
9. treat `Standard` and `Always` outcomes as the same durability class;
10. leave guard held after WAL failure;
11. allow cache mode through durable runtime;
12. mutate L6 after clean WAL failure.

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

If `commit_runtime_faults.rs` is introduced in L7I, it must contain behavioral
fault tests only. It must not assert that documentation files exist.
