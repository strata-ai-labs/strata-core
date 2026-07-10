# L7C Implementation Plan: Version And Timestamp Clocks

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L7/l7c-version-and-timestamp-clocks-test-plan.md`

## Objective

Implement the storage-owned commit fact allocator for L7-Core.

L7C takes validated commit-batch metadata from L7B and produces one
`CommitStamp` for a mutating commit: one nonzero commit version and one commit
timestamp. It also defines the recovery catch-up shape that later L7K/L8 replay
uses after durable rows have been recovered.

This slice must stay narrow. It must not validate conflicts against L6, acquire
branch guards, append WAL, mutate branch state, publish visibility, write
timeline rows, run read-only diagnostics, or replay durable rows.

The slice should make one thing true: L7 can allocate ordered commit facts in a
storage-owned way, with explicit timestamp policy and typed overflow/failure
behavior, without reintroducing durable transaction ids or public transaction
sessions.

## Inputs

1. `docs/architecture/storage/l7-commit-runtime.md`
2. `docs/architecture/storage/commit-timeline-substrate.md`
3. `docs/architecture/implementation-plans/m4-l7-commit-runtime-implementation-plan.md`
4. `docs/architecture/implementation-plans/m4-l7-commit-runtime-test-plan.md`
5. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L7/l7b-commit-batch-mutation-model-test-plan.md`
7. `crates/storage-next/src/commit/`
8. `crates/storage-next/src/commit/batch.rs`
9. `crates/storage-next/src/commit/facts.rs`
10. `crates/storage-next/src/commit/error.rs`
11. `crates/storage-next/src/testkit/commit_runtime.rs`
12. `crates/core-next/src/version.rs`
13. `crates/core-next/src/time.rs`
14. `crates/storage/src/txn/manager.rs`
15. `crates/storage/src/segmented/mod.rs`

## Existing-Code Source Map

| Current file | L7C evidence | L7C action |
|---|---|---|
| `crates/storage/src/txn/manager.rs` | Global monotonic version counter, overflow detection, visible-version separation, and recovery floor bumping. | Port the monotonic commit-version allocator and catch-up idea. Do not port durable transaction IDs. |
| `crates/storage/src/segmented/mod.rs` | Historical `next_version` behavior and old overflow bug coverage. | Preserve nonzero version allocation and typed overflow. Keep apply mechanics out of L7C. |
| `crates/storage/src/durability/commit_adapter.rs` | Durable path uses an already allocated commit version before WAL/apply. | Reserve the fact allocation boundary. L7I owns WAL use of the facts. |
| `crates/storage-next/src/commit/batch.rs` | `CommitTimestampPolicy` and `CommitStamp` already exist from L7B. | L7C produces `CommitStamp`; it does not change stamping semantics. |
| `crates/core-next/src/version.rs` | `CommitVersion::ZERO`, `CommitVersion::MAX`, and `checked_next`. | Use `checked_next` for overflow-safe allocation and catch-up. |
| `crates/core-next/src/time.rs` | `Timestamp` is representation only and does not read clocks. | Add a commit-runtime timestamp source abstraction rather than putting clock reads into `Timestamp`. |

## Scope

L7C implements:

1. monotonic storage-owned commit-version allocator;
2. allocator initialization from a recovered or configured version floor;
3. allocator catch-up to a recovered maximum commit version;
4. version-gap policy facts;
5. timestamp source abstraction for runtime-generated timestamps;
6. deterministic/manual timestamp source for tests;
7. optional system-clock source when the crate target supports it;
8. monotonic timestamp guard that prevents one runtime from moving timestamp
   facts backward;
9. explicit timestamp policy handling;
10. commit fact allocation request and outcome types;
11. typed errors for version overflow, timestamp source failure, and invalid
    timestamp policy;
12. direct and generated tests for allocator behavior.

L7C does not implement:

1. public begin/commit/rollback transaction sessions;
2. durable storage transaction ids;
3. branch registry or branch-generation checks;
4. read-only diagnostic execution;
5. read-set or CAS validation against L6;
6. row stamping beyond returning an existing L7B `CommitStamp`;
7. visible-version publication;
8. timeline row construction or lookup;
9. WAL record construction or envelope append;
10. cache/no-WAL apply into L6;
11. durable-but-not-visible classification;
12. recovery replay of rows.

## Module Layout

Expected production layout after L7C:

```text
crates/storage-next/src/commit/
  allocator.rs          # version and timestamp fact allocation
  batch.rs
  config.rs
  error.rs
  facts.rs
  result.rs
  tests/
    allocator.rs
    batch.rs
    scaffold.rs
```

If the implementation stays small, `allocator.rs` may include timestamp source
types. Split into `timestamp.rs` only if allocator and source tests become hard
to review in one module.

All production items remain `pub(crate)`.

## Proposed Type Surface

Names may change if the responsibilities stay intact.

### `CommitVersionAllocator`

Suggested shape:

```text
CommitVersionAllocator {
    last_allocated: CommitVersion
}
```

Rules:

1. `last_allocated = CommitVersion::ZERO` means no commit version has been
   allocated yet.
2. `allocate_next()` returns `last_allocated.checked_next()`.
3. `CommitVersion::ZERO` is never returned from normal allocation.
4. `CommitVersion::MAX` as `last_allocated` returns a typed overflow error.
5. `catch_up_to(recovered)` sets `last_allocated = max(last_allocated,
   recovered)`.
6. `catch_up_to(CommitVersion::ZERO)` is a no-op.
7. catch-up does not publish visibility or imply rows are applied.
8. allocation may leave version gaps after later slices fail post-allocation.

The old storage manager also allocated transaction IDs. L7C must not add a
transaction-id allocator. Storage-next V1 deliberately has no durable storage
transaction id and no transaction-id recovery catch-up hook.

### `CommitTimestampSource`

Suggested shape:

```text
trait CommitTimestampSource {
    fn now(&mut self) -> CommitRuntimeResult<Timestamp>;
}
```

Implementations:

1. deterministic/manual source for tests and generated scripts;
2. sequence source for edge-case tests;
3. optional system-clock source for production targets that can read time.

Rules:

1. `Timestamp` remains a representation type and does not read clocks.
2. timestamp-source errors are typed commit-runtime errors.
3. a source failure must not consume a commit version.
4. source implementations must not import engine or product types.
5. no process-global mutable clock state is allowed.

### `CommitTimestampGuard`

Suggested shape:

```text
CommitTimestampGuard {
    last_allocated: Option<Timestamp>
}
```

Rules:

1. generated timestamps are monotonic nondecreasing within one runtime;
2. if a runtime-generated source value is less than `last_allocated`, clamp to
   `last_allocated`;
3. if a runtime-generated source value is equal to `last_allocated`, accept it;
4. equal timestamps are valid and timeline lookup later uses commit version as
   the tiebreaker;
5. explicit timestamps are accepted when they are greater than or equal to
   `last_allocated`;
6. explicit timestamps less than `last_allocated` are rejected rather than
   silently rewritten;
7. `Timestamp::EPOCH` is allowed as a commit timestamp unless a later product
   policy forbids it. L7B only reserves epoch for row expiry.

The guard prevents one live runtime from moving backward. It does not prove
retained timestamp-history completeness. L6 timestamp coverage remains a
separate retention proof.

### `CommitFactAllocator`

Suggested shape:

```text
CommitFactAllocator<S> {
    versions: CommitVersionAllocator,
    timestamps: CommitTimestampGuard,
    source: S,
}
```

Primary operation:

```text
allocate_for_batch(batch) -> CommitFactAllocation
```

Rules:

1. read-only diagnostic batches allocate no version and no timestamp;
2. mutating batches allocate exactly one version and one timestamp;
3. timestamp resolution happens before version consumption so timestamp failure
   leaves no version gap;
4. invalid explicit timestamp policy leaves no version gap;
5. once a version is allocated, later slices may leave gaps if WAL/apply fails;
6. returned `CommitStamp` branch matches the validated batch branch;
7. returned stamp version is nonzero;
8. returned stamp timestamp is the guarded timestamp;
9. allocation does not stamp rows, append WAL, apply L6, or publish visibility.

### `CommitFactAllocation`

Suggested shape:

```text
CommitFactAllocation::Mutating {
    stamp: CommitStamp,
    previous_allocated_version: CommitVersion,
    timestamp_source: CommitTimestampAllocationSource,
}

CommitFactAllocation::ReadOnly {
    branch_id: BranchId,
}
```

The exact shape may be simpler if `CommitStamp` plus accessors is enough for
later slices. Tests must still prove read-only no-allocation.

### `CommitTimestampAllocationSource`

Suggested shape:

```text
CommitTimestampAllocationSource::RuntimeGenerated
CommitTimestampAllocationSource::RuntimeGeneratedClamped
CommitTimestampAllocationSource::Explicit
```

This is diagnostic only. It helps tests prove that generated rollback/clamping
and explicit policy are not conflated.

## Allocation Ordering

The L7C allocation order is:

```text
validate batch is already L7B-valid
if read-only:
  return no-allocation outcome
preflight next version availability without consuming it
resolve timestamp policy
preview monotonic timestamp guard without committing allocator state
allocate next commit version
construct CommitStamp
record timestamp guard floor
return fact allocation
```

This deliberately differs from the durable commit protocol pseudocode, which
shows "allocate version, allocate timestamp" at a higher level. The observable
contract is one version and one timestamp per mutating commit. L7C resolves the
timestamp first so a timestamp source failure cannot create an avoidable
version gap. It records the timestamp guard only after version allocation so a
version-overflow failure cannot advance timestamp state for a commit that did
not receive a stamp. The version preflight also avoids reading a mutable
timestamp source when the allocator is already unable to issue a version.

Post-allocation failures in later slices may still create version gaps. Version
gaps are allowed and must not break latest, `getv`, history, timeline, or
recovery.

## Recovery Catch-Up

L7C adds only local allocator catch-up:

```text
catch_up_to_recovered_version(max_recovered_version)
catch_up_to_recovered_timestamp(max_recovered_timestamp)
```

Rules:

1. version catch-up advances the next allocated version above every recovered
   commit version;
2. lower or equal catch-up input is a no-op;
3. catch-up to `CommitVersion::MAX` is allowed, but the next allocation returns
   overflow;
4. timestamp catch-up updates the monotonic guard floor;
5. lower or equal timestamp catch-up input is a no-op;
6. catch-up does not validate WAL rows, replay rows, write timeline rows, or
   publish visible versions;
7. L7K owns replay calls that invoke these helpers after durable facts are
   known.

There is no transaction-id catch-up helper in V1.

## Error Additions

Add or reuse typed variants for:

1. version allocator overflow;
2. timestamp source unavailable;
3. explicit timestamp before monotonic floor;
4. allocation attempted for an invalid batch state;
5. catch-up input that would violate an invariant, if any such invariant
   remains after no-op behavior is defined.

Displays must be bounded and must not include row value bytes, product DTOs, or
public transaction-session vocabulary.

## Source Guard Additions

Extend `commit_runtime_source_guard.rs` so production `src/commit/` still has:

1. no public transaction-session vocabulary;
2. no durable transaction-id vocabulary;
3. no engine imports;
4. no product DTO imports;
5. no direct table internals;
6. no backend/layout/filesystem imports;
7. no process-global mutable clock state;
8. no `std::env` or global lazy cache.

Using `std::time::SystemTime` in the commit timestamp source is allowed only if
the implementation keeps it isolated behind the L7C source abstraction and it
does not appear in wasm-disabled builds.

## Generated Harness Additions

Extend `crates/storage-next/src/testkit/commit_runtime.rs` with allocation
coverage counters:

1. valid version allocations;
2. version catch-up cases;
3. version overflow cases;
4. runtime-generated timestamp cases;
5. clamped timestamp cases;
6. explicit timestamp cases;
7. invalid explicit timestamp cases;
8. timestamp source failure cases;
9. read-only no-allocation cases;
10. no transaction-id allocation cases.

The existing `commit_runtime_properties.rs` route should assert every L7C
counter is nonzero.

## Sensitivity Probes

Record these in the L7 porting log when L7C is implemented:

1. allocate `CommitVersion::ZERO`; allocator direct tests fail;
2. wrap from `CommitVersion::MAX` to zero; overflow tests fail;
3. ignore recovery catch-up; catch-up direct and generated tests fail;
4. let timestamp source failure consume a version; no-gap-on-timestamp-failure
   test fails;
5. rewrite an explicit timestamp that is below the guard floor instead of
   rejecting; explicit-policy tests fail;
6. move generated timestamps backward; monotonic guard tests fail;
7. reject equal timestamps; duplicate-timestamp policy tests fail;
8. allocate facts for a read-only diagnostic batch; read-only no-allocation
   tests fail;
9. add a transaction-id allocator or transaction-id catch-up helper; source
   guard fails;
10. import L6, WAL service, backend, or engine code into `allocator.rs`; source
    guard fails.

## Exit Gate

L7C is complete when:

1. mutating validated batches allocate exactly one nonzero commit version;
2. mutating validated batches allocate exactly one guarded commit timestamp;
3. read-only diagnostic batches allocate no version and no timestamp;
4. version overflow is typed and does not wrap;
5. timestamp source failure is typed and does not consume a version;
6. catch-up advances the version allocator above recovered versions;
7. catch-up advances the timestamp guard to recovered timestamps;
8. equal timestamps are permitted and documented for timeline tiebreaking;
9. V1 has no storage transaction-id allocator;
10. allocator code does not touch L6, WAL, timeline rows, backend, layout, or
    engine code;
11. direct tests, generated tests, source guards, wasm check, clippy, fmt, and
    diff check pass.

## Verification Commands

Run after implementation:

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
