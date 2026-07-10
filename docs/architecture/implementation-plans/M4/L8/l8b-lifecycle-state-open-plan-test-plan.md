# L8B Test Plan: Lifecycle State And Open Plan

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`

## Goal

Prove that L8B has a side-effect-free lifecycle state machine with explicit
transition validation, operation admission rules, close retry/idempotence facts,
and stronger storage open-plan/open-outcome validation.

The tests must fail if L8B:

1. permits undocumented lifecycle transitions;
2. rejects documented lifecycle transitions;
3. allows commits, ordinary reads, or ordinary maintenance outside `Open`;
4. exposes partial recovery state through ordinary reads;
5. treats close timeout retry as clean close;
6. makes `Closed -> Closed` close retry non-idempotent;
7. loses failed-phase/reason facts;
8. accepts impossible open-plan or open-outcome combinations;
9. adds backend/service side effects before L8C-L8G;
10. imports product, engine, raw IO, follower, or StrataHub vocabulary.

## Test Locations

Use:

1. `crates/storage-next/src/lifecycle/tests/mod.rs` for direct fact/open-plan tests.
2. `crates/storage-next/src/lifecycle/tests/state.rs` for direct state-machine tests.
3. `crates/storage-next/src/testkit/lifecycle/mod.rs` for generated state-script
   contracts.
4. `crates/storage-next/tests/lifecycle_properties.rs` for generated property
   route checks.
5. `crates/storage-next/tests/lifecycle_source_guard.rs` for source-boundary
   checks.
6. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for the
   L8B verification and sensitivity-probe entry.

Do not add tests that require localfs, durable L4 services, L6 mutation, L7
commit execution, WAL replay, snapshot publication, maintenance execution,
retention, quarantine, repair, or close side effects.

## Direct Unit Tests

### 1. Initial State

1. `LifecycleStateMachine::new()` starts in `New`.
2. Initial machine has no failure fact.
3. Initial machine has no close fact.
4. Initial state admits open.
5. Initial state rejects commit, ordinary read, ordinary maintenance, recovery
   step, close drain, and close.
6. Initial state admits health query.

### 2. Valid Transition Matrix

Each valid transition should have a named test or table-driven case:

1. `New + OpenRequested -> Opening`.
2. `Opening + CacheOpenReady -> Open`.
3. `Opening + DurableRecoveryRequired -> Recovering`.
4. `Opening + PhaseFailed -> Failed`.
5. `Recovering + RecoveryAccepted -> Open`.
6. `Recovering + PhaseFailed -> Failed`.
7. `Open + CloseRequested -> Closing`.
8. `Closing + CloseCompleted -> Closed`.
9. `Closing + CloseRetried -> Closing`.
10. `Closing + PhaseFailed -> Failed`.
11. `Closed + CloseRetried -> Closed`, marked idempotent.

Assertions:

1. outcome records `from`;
2. outcome records `to`;
3. outcome records trigger;
4. machine state is updated exactly once;
5. failure facts are present only for failure transitions;
6. idempotence flag is true only for `Closed + CloseRetried`.

### 3. Invalid Transition Matrix

Required cases:

1. `New + CacheOpenReady`;
2. `New + RecoveryAccepted`;
3. `Opening + CloseCompleted`;
4. `Recovering + CloseRequested`;
5. `Open + DurableRecoveryRequired`;
6. `Closed + OpenRequested`;
7. `Closed + RecoveryAccepted`;
8. `Failed + OpenRequested`;
9. `Failed + CloseCompleted`;
10. `Failed + CloseRetried`.

Assertions:

1. returns `LifecycleError::InvalidLifecycleState`;
2. error display is bounded and storage-shaped;
3. state is unchanged after rejection;
4. no failure fact is added by a rejected invalid transition unless the trigger
   itself is a valid failure transition.

### 4. Operation Admission Matrix

For every state, assert admission for:

1. `Open`;
2. `OrdinaryRead`;
3. `Commit`;
4. `RecoveryStep`;
5. `OrdinaryMaintenance`;
6. `CloseRequiredDrain`;
7. `HealthQuery`;
8. `Close`;
9. `CloseRetry`.

Expected matrix:

| State | Open | Read | Commit | Recovery step | Maintenance | Drain | Health | Close | Close retry |
|---|---|---|---|---|---|---|---|---|---|
| `New` | allow | reject | reject | reject | reject | reject | allow | reject | reject |
| `Opening` | reject | reject | reject | reject | reject | reject | allow | reject | reject |
| `Recovering` | reject | reject | reject | allow | reject | reject | allow | reject | reject |
| `Open` | reject | allow | allow | reject | allow | reject | allow | allow | reject |
| `Closing` | reject | reject | reject | reject | reject | allow | allow | reject | allow |
| `Closed` | reject | reject | reject | reject | reject | reject | allow | allow-idempotent | allow-idempotent |
| `Failed` | reject | reject | reject | reject | reject | reject | allow | reject | reject |

The exact `Close` vs `CloseRetry` split may be represented differently in code,
but tests must prove closed close is idempotent and closing close retry does not
claim completion.

### 5. Failure Fact Tests

1. `Opening + PhaseFailed` preserves failed phase `Opening`.
2. `Recovering + PhaseFailed` preserves failed phase `Recovering`.
3. `Closing + PhaseFailed` preserves failed phase `Closing`.
4. Failure reason must be nonempty.
5. Failure reason display must not include product vocabulary.
6. `Failed` state is sticky.
7. Failed-state health query remains allowed.

### 6. Close Retry And Idempotence

1. `Open -> Closing` records close requested.
2. `Closing + CloseRetried` remains `Closing`.
3. `Closing + CloseRetried` records retryable close fact.
4. `Closing + CloseCompleted -> Closed`.
5. `Closed + CloseRetried -> Closed` returns idempotent outcome.
6. Repeating `Closed + CloseRetried` remains idempotent.
7. Close retry before close request is rejected.

### 7. Open Plan Validation

Extend L8A cases with:

1. cache strict plan accepted;
2. cache lossy fallback rejected;
3. durable standard strict plan accepted;
4. durable always strict plan accepted;
5. durable standard lossy fallback accepted only when config explicitly allows
   lossy recovery;
6. durable always lossy fallback accepted only when config explicitly allows
   lossy recovery;
7. object durable candidate strict plan accepted as candidate fact;
8. object durable candidate lossy fallback follows the same explicit lossy rule;
9. empty codec id rejected;
10. null codec id rejected;
11. oversized codec id rejected;
12. config validation runs before future side effects.

If L8B replaces bools with explicit enums, add tests for every enum variant.

### 8. Open Outcome Validation

1. cache created outcome with no recovered durable version is accepted;
2. cache opened-existing outcome is rejected if it claims recovered durable
   visibility unless a future persistent-cache mode exists;
3. cache outcome cannot report durable degraded recovery facts;
4. durable standard outcome can report recovered visible version;
5. durable always outcome can report recovered visible version;
6. object candidate outcome cannot claim production durable recovery unless
   explicitly marked candidate;
7. maintenance readiness false is accepted as raw storage fact;
8. created/opened-existing fact is preserved;
9. outcome debug/display remains storage-shaped;
10. outcome does not encode product acceptance of degraded recovery.

### 9. Source Boundary Regression

Run and extend `lifecycle_source_guard` if new files require it:

1. production `lifecycle/` remains crate-private;
2. no `pub mod lifecycle`;
3. no bare `pub` production lifecycle items;
4. no engine imports;
5. no public API imports;
6. no `crate::testkit` production imports;
7. no raw `std::fs`, `std::path`, `PathBuf`, `File`, mmap, or environment
   access;
8. no product DTO vocabulary;
9. no follower vocabulary;
10. no StrataHub vocabulary;
11. lower layers do not import `crate::lifecycle`.

### 10. Non-Behavior Assertions

The L8B suite should prove by absence and source guards:

1. no backend calls;
2. no service calls;
3. no manifest create/load/publish;
4. no WAL append/replay/truncate;
5. no snapshot write/load;
6. no L6 branch mutation;
7. no L7 commit/replay execution;
8. no maintenance queue execution;
9. no object deletion or quarantine;
10. no durable close/sync/guard release.

Do not overfit this into permanent source guards that block later L8C-L8N.
Prefer L8B-local tests and comments that are removed or replaced by positive
behavior tests in later slices.

## Generated Property Harness

Extend the L8A generated route.

### Required Counters

Add counters for:

1. valid lifecycle transitions;
2. invalid lifecycle transitions;
3. operation admission accepts;
4. operation admission rejects;
5. close retry cases;
6. closed idempotence cases;
7. failed-state sticky cases;
8. open-plan validation cases;
9. open-outcome validation cases.

### Script Shape

Use bounded scripts decoded from input bytes:

```text
byte 0: max maintenance queue depth selector
byte 1: max recovery faults selector
byte 2: codec-id selector
byte 3: recovered visible-version selector
bytes 4-8: lifecycle stats selectors
byte 9: transition initial-state selector
byte 10: transition trigger selector
byte 11: admission state selector
byte 12: admission operation selector
```

The contract may run a deterministic canonical sequence first, but it must also
exercise at least one input-derived transition/admission route and count it
separately once L8B is implemented.

### Generated Assertions

1. every generated case returns typed errors, not panics;
2. every state appears in either a transition route or admission route;
3. every trigger appears in either a valid or invalid transition route;
4. every operation kind appears in admission checks;
5. input-derived route counters are nonzero.

## Sensitivity Probes

Record in the L8B porting-log entry after implementation:

| Probe | Mutation | Expected failing test |
|---|---|---|
| Transition skip | Allow `New -> Open` directly | invalid transition test |
| Recovery exposure | Allow ordinary read in `Recovering` | admission matrix test |
| Commit outside open | Allow commit in `Opening` or `Closing` | admission matrix test |
| Close false success | Treat `Closing + CloseRetried` as `Closed` | close retry test |
| Closed non-idempotent | Reject `Closed + CloseRetried` | idempotent close test |
| Failed reset | Allow `Failed + OpenRequested` | failed sticky test |
| Failure fact loss | Drop failed phase/reason | failure fact test |
| Cache durable claim | Allow cache outcome recovered version | open outcome validation |
| Product vocabulary | Add product open/follower term | source guard |
| Raw IO | Add raw path/fs/env usage | source guard |

## Verification Commands

Required:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Optional targeted checks:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle_state
cargo test -p strata-storage-next --features testkit --locked lifecycle_state
```

## Exit Criteria

L8B can close when:

1. direct tests cover every state, trigger, operation kind, and open-plan mode;
2. generated property route covers state/admission scripts;
3. invalid transition tests prove state is unchanged after rejection;
4. close retry and closed idempotence are pinned;
5. failed-state stickiness is pinned;
6. open-plan/outcome validation is stronger than L8A and still
   side-effect-free;
7. all source guards pass;
8. all verification commands pass;
9. porting log records delivered behavior, deferrals, and sensitivity probes.

## Deferred

1. Backend capability matrix: L8C.
2. Cache runtime open/close: L8D.
3. Durable service assembly and writer guard: L8E.
4. Recovery orchestration: L8F.
5. Commit bootstrap and recovery-health finalization: L8G.
6. Maintenance executor: L8H.
7. Durable close side effects: L8N.
8. Generated/fault/crash closeout beyond state scripts: L8O-L8P.
