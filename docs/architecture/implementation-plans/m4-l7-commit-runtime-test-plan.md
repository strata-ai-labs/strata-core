# M4-L7 Test Plan: Commit Runtime

Status: test-suite plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

## Goal

Prove that storage-next L7 implements the internal commit runtime over L6
branch state and L4 WAL services without exposing public transaction sessions
or importing engine/product semantics.

The suite must fail if L7:

1. allocates a version for a read-only batch;
2. allocates more than one version for one mutating batch;
3. stamps rows in one commit with different versions or timestamps;
4. makes a durable commit visible before L4 accepts the WAL record;
5. loses a durable WAL commit that failed before visibility;
6. publishes visibility before every row in the batch is installed into L6;
7. validates conflicts after version allocation;
8. treats blind writes as conflicts under the preserved V1 conflict model;
9. accepts cross-branch rows in a single-branch commit;
10. writes user rows without matching timeline facts;
11. lets branch-deleting or generation-mismatched commits proceed;
12. deadlocks or violates lock-order rules under generated interleavings;
13. imports engine/product/backend/filesystem concepts directly.

This plan is stricter than current storage transaction tests because it tests
the L7 contract as a lower-layer storage runtime, not as a public transaction
feature.

M4-L7 is tested in the same three parts used by the implementation plan:

1. **L7-Core: Commit Semantics**
   Proves batch validation, clocks, branch guards, conflict validation,
   timeline rows, read-only behavior, and cache/no-WAL commits into L6.
2. **L7-Durable: WAL-Before-Visible**
   Proves durable local commit ordering, standard/always policy facts, clean
   WAL failures, uncertain WAL failures, and durable-but-not-visible windows.
3. **L7-Replay + Closeout: L8 Handoff**
   Proves replay hooks, commit-version allocator catch-up, quiesce hardening,
   generated/fuzz assurance, source guards, sensitivity probes, and closeout
   inventory.

Each part should be independently closeable. Later parts may add stronger
generated coverage over earlier parts, but they must not weaken earlier exit
gates.

## Testing Principles

1. Test internal storage commits, not product transactions.
2. Valid mutations are storage-shaped row mutations over physical keys and
   opaque value bytes.
3. Every committed row in one batch must share the same version and timestamp.
4. Every mutating durable commit must prove WAL-before-visible ordering.
5. Every failure must be classified by commit phase.
6. Conflict validation must happen before version allocation.
7. Read-only paths must not mutate clocks, L6 state, WAL, or timeline.
8. Generated tests must compare production results to an independent commit
   model.
9. Fault tests must exercise every protocol boundary: validation, allocation,
   WAL append, WAL writer halted, segment-roll failure, L6 apply, timeline
   install, and visibility publication.
10. Source guards are part of the suite, not advisory documentation.

## Test Harness Layout

Recommended locations:

1. `crates/storage-next/src/commit/` for small module-local tests.
2. `crates/storage-next/src/commit/tests/` for larger direct suites.
3. `crates/storage-next/src/testkit/commit_runtime/` for generated model and
   script helpers.
4. `crates/storage-next/tests/commit_runtime_properties.rs` for generated L7
   conformance properties.
5. `crates/storage-next/tests/commit_runtime_faults.rs` for backend/L4/L6 fault
   window tests.
6. `crates/storage-next/tests/commit_runtime_source_guard.rs` for production
   boundary scans.
7. `crates/storage-next/tests/commit_runtime_closeout.rs` for closeout
   inventory and command evidence.
8. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_batch.rs` for batch
   validation and stamping.
9. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_conflict.rs` for
   conflict scripts.
10. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_durable.rs` for
    WAL-before-visible and phase-failure scripts.
11. `crates/storage-next/fuzz/fuzz_targets/commit_runtime_timeline.rs` for
    timeline substrate scripts.

Required regression files:

1. `crates/storage-next/proptest-regressions/commit_runtime.txt`, created only
   when a failing generated case is captured.
2. `crates/storage-next/fuzz/corpus/commit_runtime_*` seed directories for
   each fuzz target.

## Part Gates

### Part 1: L7-Core

L7-Core closes when direct and generated tests prove:

1. module/source boundaries;
2. batch validation;
3. row stamping;
4. version/timestamp allocation;
5. read-only no-allocation behavior;
6. branch guard rejection paths;
7. read-set and CAS conflict validation;
8. blind-write no-conflict behavior;
9. timeline row construction and branch-isolated lookup;
10. cache/no-WAL atomic commit into L6;
11. visible-version publication after full L6 apply.

Core tests must not require WAL services or recovery replay.

### Part 2: L7-Durable

L7-Durable closes when direct and fault tests prove:

1. durable `WalRecord` rows match the stamped Core rows;
2. `WalRecordEnvelope` append happens before L6 apply;
3. `standard` and `always` outcomes are distinct;
4. clean WAL failure leaves no visible rows;
5. uncertain WAL failure is not collapsed into clean failure;
6. durable-but-not-visible is distinct from all non-durable failure modes;
7. unresolved durable-but-not-visible facts block unsafe later commits.

Durable tests may use fake L4/L6 fault surfaces. Full process open/recovery is
still L8 work.

### Part 3: L7-Replay + Closeout

L7-Replay + Closeout closes when direct, generated, fuzz, and closeout tests
prove:

1. replay applies already-durable rows with original version and timestamp;
2. replay bypasses normal conflict validation;
3. replay exact duplicates are idempotent;
4. replay fact mismatches fail closed;
5. the commit-version allocator catches up above recovered versions;
6. visible version is published only after replay install;
7. quiesce and lock-order rules are hardened;
8. source guards enforce layer boundaries;
9. fuzz targets are distinct and seeded;
10. sensitivity probes are recorded;
11. the closeout command matrix passes.

## Reference Model

Use an independent model. Do not derive expected results by reading production
commit runtime state.

Suggested shape:

```text
ModelCommitRuntime {
  branches: BTreeMap<BranchId, ModelBranch>
  allocated_version: CommitVersion
  durable_version: Option<CommitVersion>
  applied_version: Option<CommitVersion>
  visible_version: Option<CommitVersion>
  timeline: ModelTimeline
  branch_guards: BTreeMap<BranchId, ModelBranchGuard>
}

ModelBranch {
  rows: BTreeMap<PhysicalKey, Vec<ModelRow newest-first>>
  generation: u64
  deleting: bool
}

ModelCommit {
  branch_id
  version
  timestamp
  mutations
  timeline_rows
  durable
  visible
  phase
}
```

The model must:

1. keep version allocation separate from durability, apply, and visibility;
2. allow version gaps;
3. reject malformed batches before allocation;
4. reject conflicts before allocation;
5. apply a whole batch atomically or not at all;
6. represent WAL durable but not visible as a distinct phase;
7. preserve one timestamp per commit;
8. write timeline facts in the same modeled commit unit;
9. track visible version independently from allocated version;
10. preserve branch isolation.

## Generators

### Batch Generator

Generate batches over 1 to 8 branches and 0 to 256 mutations by default, with
stress cases for larger bounded batches.

Vary:

1. empty/read-only batches;
2. single put;
3. single delete;
4. mixed puts and deletes;
5. repeated physical key in one batch;
6. rows across multiple storage spaces;
7. long shared user-key prefixes;
8. embedded zero and high-bit user-key bytes;
9. empty value bytes;
10. expiry at epoch, before timestamp, at timestamp, and after timestamp;
11. branch-mismatched physical keys;
12. storage-owned timeline keys as user input, which must be rejected unless
    generated by L7 itself.

### Conflict Generator

Generate:

1. observed version matches current latest;
2. observed version differs from current latest;
3. observed missing key remains missing;
4. observed missing key becomes present;
5. CAS expected version matches;
6. CAS expected version mismatches;
7. delete races with put;
8. put races with delete;
9. blind write over changed key;
10. multiple validation facts for one key;
11. validation facts for keys outside the target branch.

### Commit Mode Generator

Generate:

1. cache/no-WAL commit;
2. durable standard commit;
3. durable always commit;
4. unsupported durable mode for configured backend capabilities;
5. read-only diagnostic batch;
6. replay batch from WAL;
7. commit while branch is deleting;
8. commit with stale branch generation;
9. commit while quiesce is pending;
10. commit after unresolved durable-but-not-visible fact.

### Fault Generator

Generate phase failures:

1. validation error;
2. branch lookup error;
3. version allocation overflow;
4. timestamp allocation failure;
5. WAL record construction or envelope encode failure;
6. WAL append failure before durable;
7. WAL writer halted;
8. WAL segment id overflow or segment-roll failure;
9. WAL append uncertain;
10. WAL append durable success followed by L6 apply failure;
11. timeline install failure before visibility;
12. visibility publication failure;
13. quiesce unavailable or caller-level deadline;
14. replay duplicate exact match;
15. replay duplicate mismatch;
16. allocator catch-up failure.

## Required Cases

### 1. Module And Boundary Guards

1. `commit` module compiles under default features.
2. `commit` module compiles under no-default features.
3. `commit` module compiles under all features.
4. Production `commit/` does not expose public begin/commit/rollback session
   APIs through crate root.
5. Production commit runtime items remain `pub(crate)` unless a later L9 public
   API explicitly wraps them.
6. Production `commit/` does not import engine crates.
7. Production `commit/` does not import product DTOs: `Value`, `Key`,
   `Namespace`, `EntityRef`, JSON, graph, vector, search, event, or embedding
   vocabulary.
8. Production `commit/` does not import `crate::table` internals directly; it
   reaches table data only through L6 branch APIs or explicit row facts.
9. Production `commit/` does not import backend or object layout APIs directly.
10. Production `commit/` does not use `std::fs`, `Path`, `File`, mmap,
   environment variables, or process-global mutable singletons.
11. Commit errors are typed and preserve L4/L6 source errors where useful.
12. Commit code files stay within engineering thresholds or split into
    submodules.

### 2. Commit Batch Validation

1. Empty read-only batch is accepted as read-only.
2. Empty mutating batch is rejected or normalized to read-only by explicit
   policy.
3. Batch with target branch id validates all mutation physical keys.
4. Branch-mismatched mutation is rejected before version allocation.
5. Duplicate physical key in one batch follows the documented policy.
6. Oversized mutation count is rejected before allocation.
7. Oversized value bytes are rejected before allocation.
8. Invalid expiry metadata is rejected before allocation.
9. User-supplied storage-owned timeline keys are rejected.
10. Batch validation never touches WAL and never mutates L6 state. Conflict
    validation may read L6 branch views later in the protocol.

### 3. Version And Timestamp Allocation

1. Mutating batch allocates exactly one commit version.
2. All rows in one batch use that version.
3. Commit versions are monotonically increasing.
4. Commit versions are not required to be dense.
5. Failure after allocation but before visibility may leave a gap.
6. `CommitVersion::MAX` overflow returns a typed error.
7. Recovery catch-up advances allocator above recovered versions.
8. Catch-up with lower version is a no-op.
9. One commit timestamp is assigned per mutating commit.
10. Every row and WAL record in one commit carries the same timestamp.
11. Timestamp allocation failure leaves no visible rows.
12. A deterministic/manual timestamp source is supported for tests.
13. A wall-clock-backed source is wrapped by a monotonic guard.
14. Equal timestamps are permitted and rely on commit-version tiebreaking in
    the timeline.
15. V1 does not allocate or recover durable storage transaction ids.

### 4. Read-Only Path

Read-only batches are diagnostic helpers, not the L7 V1 product surface. These
tests pin the no-allocation/no-mutation behavior only because the helper is
useful while porting old storage context behavior.

1. Read-only batch does not allocate a version.
2. Read-only batch does not append WAL.
3. Read-only batch does not mutate L6.
4. Read-only batch reports current visible snapshot facts.
5. Read-only path rejects mutating options that require durability.
6. Read-only path remains available while no branch state exists, if the
   runtime policy allows diagnostics on an empty store.

### 5. Cache / No-WAL Commit Path

1. Cache commit applies put rows to L6.
2. Cache commit applies delete rows as tombstones.
3. Mixed put/delete batch becomes visible atomically.
4. L6 latest/getv/history reads observe committed rows after visibility.
5. No WAL record is appended.
6. Outcome reports durable false and visible true.
7. L6 apply failure leaves no visible rows.
8. Version gap after pre-visible cache failure does not break later reads.
9. Cache mode never claims crash durability.

### 6. Durable WAL-Before-Visible Path

1. Durable commit constructs a `WalRecord` from stamped rows through the
   existing format-layer constructor.
2. The row-native payload row count matches committed mutation row count plus
   timeline rows when timeline rows are included in the WAL record.
3. WAL record outer branch, version, and timestamp match payload rows by using
   the existing `validate_outer_facts` path instead of reimplementing it in L7.
4. The encoded `WalRecordEnvelope` append through L4 happens before L6 apply.
5. WAL append failure leaves no visible rows.
6. WAL append uncertain is classified separately from clean failure.
7. L6 apply is not attempted after clean WAL append failure.
8. L6 apply is attempted only after L4 reports required durable status.
9. Visibility publication happens only after L6 apply succeeds.
10. `standard` outcome records the standard durability policy.
11. `always` outcome records that the per-commit durability barrier completed.
12. Unsupported durable backend capabilities reject before version allocation
    where possible.

Part: L7-Durable.

### 7. Durable But Not Visible

1. WAL durable success followed by L6 apply failure returns
   durable-but-not-visible.
2. In durable mode, WAL durable success followed by timeline install failure
   returns durable-but-not-visible because timeline rows are part of the
   durable commit unit.
3. In cache mode, timeline install failure is an in-memory pre-visible apply
   failure and returns not-durable-not-visible.
4. WAL durable success followed by visibility publication failure returns
   durable-but-not-visible.
5. Normal reads do not observe durable-but-not-visible rows in the current
   process.
6. New mutating commits are blocked while an unresolved durable-but-not-visible
   fact exists.
7. L8 replay hook can install the durable rows and publish visibility.
8. Replaying the same durable commit twice is idempotent when facts match.
9. Replay mismatch fails closed.

Part: L7-Durable owns the durable-but-not-visible classification and write
gate. L7-Replay + Closeout owns replay repair/idempotency over that fact.

### 8. Conflict Validation

1. Read-set match allows commit.
2. Read-set mismatch rejects before allocation.
3. Observed missing key remains missing and allows commit.
4. Observed missing key becoming present rejects.
5. CAS expected present version match allows commit.
6. CAS expected present version mismatch rejects.
7. CAS expected missing match allows commit.
8. CAS expected missing mismatch rejects.
9. Blind write over changed key commits.
10. Blind delete over changed key commits.
11. Conflict validation uses the target branch read view.
12. Validation facts for a different branch reject.
13. Multiple validation facts for one key are deterministic and documented.
14. Conflict errors include storage-shaped key/version facts, not product DTOs.

Part: L7-Core.

### 9. Branch Guards

1. Missing target branch rejects before allocation.
2. Branch-deleting marker rejects before allocation.
3. Branch generation mismatch rejects before allocation or before visibility by
   documented policy.
4. If L9 supplies generation facts, reuse-after-delete/recreate is rejected as
   a stale generation.
5. Same-branch commits are serialized.
6. Different-branch commits preserve global version ordering.
7. Cross-branch rows in one normal batch reject.
8. Commit to a branch with materialization/reachability state uses L6 APIs
   without corrupting reachability facts.
9. Branch guard is released after clean success.
10. Branch guard is released after validation failure.
11. Branch guard is released after durable-but-not-visible classification only
    if the unresolved durable fact is recorded and write gate policy is active.

### 10. Quiesce And Lock Ordering

1. V1 quiesce returns a typed unavailable error while in-flight mutating
   commits hold branch guards; L8 owns retry and deadline policy.
2. Quiesce blocks new mutating commits after the token is acquired.
3. Read-only diagnostic path during quiesce follows documented policy.
4. Quiesce unavailable facts are typed and do not allocate or mutate storage.
5. Quiesce release lets later commits proceed.
6. Lock-order guard catches inverted acquisition order in tests.
7. Deterministic scheduler-style guard interleavings never leave stuck guards.
8. L7 does not add blocking waits, wall-clock sleeps, or async runtime
   dependencies for quiesce.
9. Quiesce does not publish visibility or mutate versions itself.
10. Checkpoint/fork/recovery callers can use quiesce without importing L6
   internals.

### 11. Commit Timeline

1. Mutating commit writes timestamp-to-version timeline fact.
2. Mutating commit writes version-to-timestamp timeline fact.
3. Timeline facts are branch-isolated.
4. Timeline facts share commit version with user rows.
5. Timeline facts share commit timestamp with user rows.
6. Timeline facts are included in WAL durability when durable mode is used.
7. Timestamp index keys include branch id, commit timestamp, and commit
   version.
8. Timestamp lookup returns the greatest retained commit version at or before
   the requested timestamp.
9. Version lookup returns the commit timestamp for that version.
10. Duplicate timestamps in one branch use greatest commit version as the
    deterministic tiebreaker.
11. Timeline install failure before visibility is classified by phase and mode.
12. User mutations cannot write timeline namespace directly.
13. Replay preserves original timeline timestamps.

Part: L7-Core owns timeline construction and lookup. L7-Durable proves timeline
rows participate in WAL durability. L7-Replay + Closeout proves replay
preserves timeline facts.

### 12. Visible Version And Snapshot Safety

1. Visible version starts at zero or documented empty-state value.
2. Allocated version may exceed visible version.
3. Durable version may exceed visible version.
4. Applied version may not be published visible until full batch apply
   succeeds.
5. New read snapshots can only target visible versions.
6. Visible version moves monotonically.
7. Visible version catch-up after replay requires L6 rows installed.
8. Cross-branch visible-version policy is documented and tested.
9. Version gaps do not make snapshots fail.
10. Pinned L6 read views remain stable across later commits.

### 13. Recovery Replay Hooks

1. Replay applies rows with WAL version.
2. Replay applies rows with WAL timestamp.
3. Replay bypasses normal read-set/CAS validation.
4. Replay rejects branch-mismatched durable rows.
5. Replay exact duplicate is idempotent.
6. Replay duplicate with different facts fails closed.
7. Replay advances version allocator above maximum recovered version.
8. Replay updates visible version only after L6 install.
9. Replay installs timeline facts.
10. Replay errors preserve L4/L6 source chains.

Part: L7-Replay + Closeout.

### 14. Outcome And Error Classification

1. Invalid batch reports rejected-before-allocation phase.
2. Conflict reports rejected-before-allocation phase.
3. Allocation overflow reports allocated-none phase.
4. WAL append failure reports not-durable-not-visible phase.
5. WAL writer halted reports not-durable-not-visible unless the halt occurs
   after durable acknowledgment.
6. WAL segment id overflow or segment-roll failure reports a typed
   durable-not-acquired phase failure.
7. WAL uncertain reports ambiguous durability phase.
8. Durable-but-not-visible is distinct from clean WAL failure.
9. Post-visible observer failure is not represented as L7 commit failure.
10. Outcome counts puts, deletes, and timeline rows.
11. Outcome records durability mode and visible status.
12. Error display has no product vocabulary.
13. Error source chains preserve lower-layer causes.

### 15. Generated Property Harness

Create generated commit-runtime contracts that run bounded scripts through both
production and the independent model.

The generated harness should grow by part:

1. Core scripts first cover validation, clocks, timeline, conflict, guards, and
   cache commits.
2. Durable scripts add WAL/fault phases after the Core model is stable.
3. Replay/closeout scripts add replay, catch-up, quiesce, and full phase
   coverage.

Required contracts:

1. batch validation and stamping;
2. cache commit atomicity;
3. durable WAL-before-visible ordering with fake L4 service;
4. conflict validation;
5. branch guard and quiesce interleavings;
6. timeline facts;
7. replay idempotency;
8. version gaps and visible-version monotonicity.

The property harness must assert:

1. every generated script reaches at least one mutating commit;
2. every configured commit mode is exercised over the default case set;
3. every failure phase has at least one generated route or direct test;
4. production visible reads match model visible reads after every successful
   visible commit.

### 16. Fuzz Targets

Required fuzz targets:

1. `commit_runtime_batch`
   - arbitrary bytes decode into commit batches and validation facts;
   - successful validation must produce rows that satisfy branch/version
     invariants;
   - malformed inputs return typed errors, not panics.

2. `commit_runtime_conflict`
   - arbitrary scripts build model state and conflict facts;
   - commit either succeeds with unchanged facts or rejects before allocation.

3. `commit_runtime_durable`
   - arbitrary scripts choose commit mode and fault point;
   - WAL-before-visible and phase classification must hold.

4. `commit_runtime_timeline`
   - arbitrary scripts generate branch/timestamp/version facts;
   - timeline lookups must match the independent model.

Every fuzz target must have checked-in seed corpora and must call a distinct
contract function. Closeout tests must reject targets that only call a shared
scaffold contract.

Part ownership:

1. L7-Core owns `commit_runtime_batch`, `commit_runtime_conflict`, and
   `commit_runtime_timeline` registration and seed corpora.
2. L7-Durable owns `commit_runtime_durable` registration and seed corpora.
3. L7-Replay + Closeout may add replay-focused seeds or a replay target if the
   durable target becomes too broad, but must not require a fourth target unless
   the implementation surface justifies it.

### 17. Fault Windows

Direct fault tests must cover:

1. validation failure before allocation;
2. branch deleting before allocation;
3. conflict before allocation;
4. version allocation succeeds then timestamp allocation fails;
5. timestamp allocation succeeds then WAL record construction or envelope
   encode fails;
6. WAL append returns clean failure;
7. WAL writer is halted before durable acknowledgment;
8. WAL segment roll or segment id overflow occurs before durable
   acknowledgment;
9. WAL append returns uncertain outcome;
10. WAL append returns durable success then L6 apply fails;
11. WAL append returns durable success then timeline install fails;
12. WAL append returns durable success then visibility publish fails;
13. commit visible then later observer/side-effect failure is ignored by L7;
14. replay duplicate exact match;
15. replay duplicate mismatch;
16. unresolved durable-but-not-visible blocks later commits.

### 18. Source Guards

Add `commit_runtime_source_guard.rs`.

It must prove:

1. `src/commit/` has no public transaction-session vocabulary in exported
   crate surface;
2. `src/commit/` exposes no `pub` runtime surface unless the item is explicitly
   marked as an L9-facing wrapper;
3. `src/commit/` does not import engine crates;
4. `src/commit/` does not import product DTO or payload vocabulary;
5. `src/commit/` does not import `crate::table` internals directly;
6. `src/commit/` does not import backend/layout/filesystem APIs directly;
7. `src/commit/` does not use `std::env`, global lazy state, or process-global
   mutable caches;
8. `src/branch/` does not import `crate::commit`;
9. `src/format/` does not import `crate::commit`;
10. `src/service/` does not call upward into `crate::commit`;
11. fuzz and testkit code may import commit test helpers only behind testkit or
   test targets.

### 19. Sensitivity Probes

Record each probe in `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md`.

Minimum probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| S1 | Allocate a version for read-only batch. | Read-only direct test and generated contract fail. |
| S2 | Stamp two rows in one batch with different versions. | Batch stamping invariant fails. |
| S3 | Stamp two rows in one batch with different timestamps. | Timestamp invariant and WAL record parity fail. |
| S4 | Apply to L6 before WAL append in durable mode. | WAL-before-visible ordering test fails. |
| S5 | Treat WAL append failure as visible success. | Durable fault-window test fails. |
| S6 | Collapse durable-but-not-visible into clean failure. | Phase classification test fails. |
| S7 | Validate conflicts after version allocation. | Conflict/no-allocation test fails. |
| S8 | Reject blind writes as conflicts. | Preserved conflict-model test fails. |
| S9 | Allow branch-mismatched row in batch. | Batch validation/source guard tests fail. |
| S10 | Omit timeline row generation. | Timeline direct/property tests fail. |
| S11 | Publish visible version before full L6 apply. | Atomic visibility property fails. |
| S12 | Ignore branch deleting marker. | Branch guard test fails. |
| S13 | Allow quiesce and mutating commit concurrently. | Quiesce interleaving property fails. |
| S14 | Replay duplicate mismatch as success. | Replay mismatch test fails. |
| S15 | Import backend or layout directly from `commit/`. | Source guard fails. |
| S16 | Expose public begin/commit/rollback API. | Source guard/closeout inventory fails. |

### 20. Closeout Inventory

Add `commit_runtime_closeout.rs`.

It must verify:

1. generated harness exposes counters for every required category;
2. property tests assert every required counter;
3. source guard covers boundary categories;
4. fuzz targets exist and call distinct contracts;
5. fuzz corpora contain non-empty seed scenarios;
6. porting log records preserved/changed/retired/deferred behavior;
7. sensitivity probes are recorded with mutation target and failing test;
8. command matrix is recorded.

Closeout inventory should not test that planning documents exist or link to
each other. Documentation consistency is reviewed in the porting log, while
automated closeout tests stay focused on implementation assurance.

### 21. Command Matrix

Mandatory commands before L7 closeout:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked commit
cargo test -p strata-storage-next --locked --test commit_runtime_properties
cargo test -p strata-storage-next --locked --test commit_runtime_faults
cargo test -p strata-storage-next --locked --test commit_runtime_source_guard
cargo test -p strata-storage-next --locked --test commit_runtime_closeout
cargo test -p strata-storage-next --locked --quiet
cargo test -p strata-storage-next --no-default-features --locked commit
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo hack check -p strata-storage-next --feature-powerset --depth 2
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

Optional when nightly/libfuzzer is available:

```bash
cargo +nightly fuzz run commit_runtime_batch -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_conflict -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_durable -- -max_total_time=60
cargo +nightly fuzz run commit_runtime_timeline -- -max_total_time=60
```

If nightly fuzzing is unavailable, closeout inventory must still prove target
registration, distinct contract routing, and checked-in seed corpora.

## Deferred Behavior Map

The canonical deferred behavior map lives in
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`.
This test plan should not duplicate it. L7 closeout tests must verify the
porting log records any test deferral against that canonical map.

## Exit Gate

M4-L7 can close only when:

1. direct tests prove cache, standard, and always commit protocols;
2. model/property tests prove atomic visibility and version/timestamp
   invariants;
3. fault tests prove phase classification and durable-but-not-visible behavior;
4. conflict tests prove read-set/CAS behavior and blind-write behavior;
5. timeline tests prove storage-owned timestamp/version mapping;
6. replay tests prove idempotency and allocator catch-up hooks;
7. source guards prove layer boundaries;
8. fuzz targets are registered and seeded;
9. sensitivity probes are recorded;
10. closeout command matrix passes.
