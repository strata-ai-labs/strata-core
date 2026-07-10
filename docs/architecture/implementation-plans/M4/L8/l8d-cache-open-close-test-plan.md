# L8D Test Plan: Cache Open And Close

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`

## Goal

Prove that L8D opens and closes cache-mode storage as a volatile
storage-internal runtime by composing existing L6/L7 surfaces, while making no
durable recovery claim and creating no durable services or objects.

The tests must fail if L8D:

1. accepts a non-cache open plan through the cache runtime;
2. opens without L8C capability preflight;
3. calls backend methods other than `capabilities()` during cache open;
4. creates or imports WAL, manifest, snapshot, table-object, quarantine, or
   writer-lock services;
5. reports recovered durable visibility or degraded recovery health;
6. starts with non-empty branch state or nonzero visible version;
7. reimplements L7 commit stamping instead of using `CommitCacheRuntime`;
8. allows commits or ordinary reads before open or after close;
9. makes cache reopen recover prior rows;
10. uses product open/close, follower, IPC, primitive, or StrataHub vocabulary.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/cache.rs` for direct cache open,
   commit/read, and close tests.
2. `crates/storage-next/src/lifecycle/tests/mod.rs` only for shared lifecycle
   test helpers.
3. `crates/storage-next/src/testkit/lifecycle/` for generated cache lifecycle
   scripts and counters.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated property
   assertions.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary and
   durable-service absence checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8D verification and sensitivity-probe entry after implementation.

Do not add tests whose only assertion is that plan documents exist or link to
each other. L8D tests should assert executable storage behavior.

Do not add integration tests that require durable local service assembly,
manifest creation, WAL replay, snapshot publication, checkpointing,
maintenance execution, retention, quarantine, repair, or public L9 APIs.

## Direct Unit Tests

### 1. Cache Open Request Validation

Required cases:

1. valid cache plan plus initial branch generation accepts;
2. durable standard plan rejects;
3. durable always plan rejects;
4. object-durable candidate plan rejects;
5. zero branch generation rejects through the L7 generation type;
6. invalid lifecycle config rejects before runtime construction;
7. cache plan with lossy durable recovery fallback rejects through existing
   open-plan validation.

Assertions:

1. errors are typed lifecycle errors or lower-layer typed errors;
2. errors do not mention product open policy;
3. rejection returns no opened runtime.

### 2. Capability Preflight Ordering

Use a counting fake backend that records every backend call.

Required cases:

1. accepted cache open calls `capabilities()` exactly once;
2. rejected cache open calls `capabilities()` exactly once;
3. rejected non-cache open does not call durable backend methods;
4. capability rejection happens before L6 branch state construction;
5. capability rejection happens before L7 allocator/registry construction.

Forbidden backend calls:

1. read object;
2. read range;
3. write object;
4. delete object;
5. list prefix;
6. metadata;
7. append;
8. sync;
9. publish;
10. conditional create/update;
11. writer-lock acquire.

Assertions:

1. no temporary object is left behind;
2. no writer guard is acquired;
3. no durable service constructor is reached.

### 3. Opened Runtime Baseline

Required cases:

1. cache open reaches `LifecycleState::Open`;
2. open outcome mode is `StorageMode::Cache`;
3. open disposition is `StorageOpenDisposition::Created`;
4. recovered visible version is `None`;
5. recovery health is healthy;
6. maintenance readiness is false for the L8D baseline;
7. visible tracker starts at `CommitVersion::ZERO`;
8. version allocator starts at `CommitVersion::ZERO`;
9. timestamp guard starts empty;
10. unresolved durable gate starts empty;
11. branch registry contains the initial branch at generation `1`;
12. initial branch state is empty.

Assertions:

1. no durable objects are listed or created;
2. no product default branch name is hardcoded;
3. runtime stores only storage facts.

### 4. Durable-Service Absence

Source and behavior tests must prove cache runtime production code does not
import, instantiate, or call:

1. database manifest service;
2. table manifest service;
3. WAL service;
4. WAL sidecar service;
5. snapshot service;
6. table object service;
7. quarantine service;
8. object layout constructors;
9. writer-lock acquire/release;
10. durable publish/sync helpers.

Assertions:

1. `lifecycle/cache.rs` imports `branch` and `commit` but not `service`;
2. source guard fixtures catch a fake WAL or manifest import;
3. behavior tests with a counting backend observe no durable method calls.

### 5. Cache Commit Smoke

Use the opened cache runtime and existing L7 cache executor.

Required cases:

1. one put batch commits successfully;
2. committed row is stamped with branch id, commit version, and timestamp;
3. visible version advances to the committed version;
4. branch state max commit version advances;
5. timeline rows are added by L7;
6. read view captured after commit can read the user row through L6;
7. mutation counts include the user mutation and timeline rows;
8. durable class is `NotDurable`.

Assertions:

1. no WAL bytes are produced;
2. no durable commit runtime is constructed;
3. no L8D code manually stamps rows outside L7.

### 6. Cache Commit Rejection

Required cases:

1. durable standard batch sent through cache runtime rejects;
2. durable always batch sent through cache runtime rejects;
3. read-only diagnostic batch sent through the mutating cache executor rejects;
4. wrong branch id rejects;
5. stale generation guard rejects;
6. conflicting read-set/CAS facts reject through L7;
7. commit after close rejects through lifecycle admission before allocation.

Assertions:

1. allocator does not advance on pre-allocation rejection;
2. branch state remains unchanged on rejection;
3. visible version remains unchanged on rejection;
4. branch guard is released after rejection.

### 7. Read Admission

Required cases:

1. read view before close succeeds while state is `Open`;
2. read view after close rejects;
3. read view before open is impossible through the owned runtime API;
4. health/open-outcome facts remain inspectable after close.

Assertions:

1. ordinary reads are admitted only in `Open`;
2. health facts are still queryable after close if the runtime keeps them for
   diagnostics;
3. read rejection is lifecycle-state shaped.

### 8. Cache Close

Required cases:

1. close from `Open` transitions to `Closing` then `Closed`;
2. close outcome reports `ClosePhase::Closed`;
3. first close outcome reports `CloseOutcomeStatus::Complete`;
4. first close outcome reports `LifecycleCloseFact::Complete`;
5. first close effects report commits quiesced, maintenance drained, guards
   released, and no durable sync;
6. second close reports `CloseOutcomeStatus::Idempotent`;
7. second close reports `LifecycleCloseFact::AlreadyClosed` and prior-final
   close effect;
8. close after a committed row does not attempt durable flush;
9. close after no commits is still complete;
10. close failure paths, if any, leave the runtime retryable.

Assertions:

1. no WAL flush is attempted;
2. no manifest sync is attempted;
3. no writer guard release is attempted;
4. no engine freeze hooks or product close callbacks exist;
5. commits are rejected after close.

### 9. Reopen Empty Semantics

Required cases:

1. open cache runtime A;
2. commit a row;
3. close runtime A;
4. open cache runtime B with the same backend and initial branch;
5. assert runtime B has empty branch state;
6. assert runtime B visible version is `CommitVersion::ZERO`;
7. assert runtime B reports no recovered visible version.

Assertions:

1. cache reopen does not inspect durable object inventory;
2. cache reopen does not recover rows from runtime A;
3. cache reopen does not report degraded recovery for the discarded volatile
   state.

### 10. Lower-Layer Error Mapping

Required cases:

1. L6 append failure from a bad row maps to `LifecycleLowerLayer::BranchRuntime`
   if exposed through an L8D boundary;
2. L7 cache execution failure maps to `LifecycleLowerLayer::CommitRuntime` if
   exposed through an L8D boundary;
3. lifecycle-state rejection remains `LifecycleError::InvalidLifecycleState`;
4. source chains are preserved where the lower layer exposes a source error.

Assertions:

1. displays are bounded;
2. displays use storage lifecycle terms;
3. row payload bytes are not printed.

## Generated Property Coverage

Extend `check_lifecycle_scaffold_contract` or split a focused cache lifecycle
contract.

Generated scripts should exercise:

1. cache open success;
2. cache open capability rejection;
3. one or more cache commits;
4. read after commit;
5. close success;
6. close idempotence;
7. commit-after-close rejection;
8. reopen-empty semantics.

Required counters:

1. cache open accepted;
2. cache open rejected;
3. cache baseline facts checked;
4. cache durable-absence checked;
5. cache commit/read smoke checked;
6. cache close checked;
7. cache close idempotence checked;
8. cache commit-after-close rejection checked;
9. cache reopen empty checked;
10. input-derived cache operation checked.

The generated property test must require each counter to be nonzero.

## Source Guards

Extend `lifecycle_source_guard.rs` to pin the L8D boundary.

Required checks:

1. lifecycle production code remains crate-private;
2. lower layers do not import lifecycle upward;
3. `lifecycle/cache.rs` does not import `crate::service`;
4. `lifecycle/cache.rs` does not import `crate::layout`;
5. `lifecycle/cache.rs` does not import `crate::format`;
6. `lifecycle/cache.rs` does not import raw filesystem/path/env APIs;
7. lifecycle code does not import product/engine modules;
8. lifecycle code does not mention follower mode, IPC, StrataHub, primitive
   registries, or `VersionedValue`;
9. source guard fixture strings prove each forbidden import is caught.

## Sensitivity Probes

Record each implemented probe in
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`.

Minimum L8D probes:

| Probe | Mutation | Expected failure |
|---|---|---|
| D1 | Skip capability preflight during cache open. | Capability-order test fails. |
| D2 | Let cache open call `list_prefix`. | Backend side-effect counter test fails. |
| D3 | Report cache open with recovered visible version. | Open outcome test fails. |
| D4 | Construct a WAL or manifest service in cache open. | Durable-service absence/source-guard test fails. |
| D5 | Start visible version at a nonzero value. | Opened baseline test fails. |
| D6 | Recover previous cache rows on reopen. | Reopen-empty test fails. |
| D7 | Allow commit after close. | Close/admission test fails. |
| D8 | Manually stamp commit rows in L8D instead of L7. | Commit smoke/source-shape test fails. |

## Verification Commands

Minimum commands after implementation:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```
