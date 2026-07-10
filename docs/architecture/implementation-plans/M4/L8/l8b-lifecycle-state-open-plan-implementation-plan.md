# L8B Implementation Plan: Lifecycle State And Open Plan

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-test-plan.md`

## Objective

Turn the L8A lifecycle vocabulary into a small, explicit state machine and
tighten the storage open-plan/open-outcome facts.

L8B should make lifecycle state transitions and operation admission testable
without opening storage, assembling services, replaying WAL, scheduling
maintenance, or closing durable resources. It is still a side-effect-free slice.

L8B establishes:

1. valid lifecycle state transitions;
2. typed invalid-transition errors;
3. operation admission rules by lifecycle state;
4. close retry and closed-idempotence facts;
5. failed-state stickiness facts;
6. open-plan/open-outcome validation strong enough for L8C-L8G to rely on;
7. generated scaffold coverage for state transition scripts.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`
6. `crates/storage-next/src/lifecycle/mod.rs`
7. `crates/storage-next/src/lifecycle/config.rs`
8. `crates/storage-next/src/lifecycle/error.rs`
9. `crates/storage-next/src/lifecycle/facts.rs`
10. `crates/storage-next/src/lifecycle/health.rs`
11. `crates/storage-next/src/lifecycle/outcome.rs`
12. `crates/storage-next/src/lifecycle/tests/mod.rs`
13. `crates/storage-next/src/lifecycle/tests/state.rs`
14. `crates/storage-next/src/testkit/lifecycle/mod.rs`
15. `crates/storage-next/tests/lifecycle_properties.rs`
16. `crates/storage-next/tests/lifecycle_source_guard.rs`
17. `crates/engine/src/database/lifecycle.rs`
18. `crates/engine/src/database/open.rs`
19. `crates/storage/src/durability/recovery_bootstrap.rs`

## Existing-Code Source Map

| Current file | L8B evidence | L8B action |
|---|---|---|
| `crates/engine/src/database/lifecycle.rs` | Shutdown/open state gates, close retry shape, writer health state. | Port only storage lifecycle state/admission facts. Product handle state and public error wording stay above L8. |
| `crates/engine/src/database/open.rs` | Open/create phase ordering and post-open readiness facts. | Use as transition evidence: new/opening/recovering/open. No service assembly in L8B. |
| `crates/engine/src/background.rs` | Task acceptance/drain gates during close. | Reserve operation-admission categories. Deterministic executor lands in L8H. |
| `crates/storage/src/durability/recovery_bootstrap.rs` | Strict vs lossy recovery choices and failed recovery facts. | Preserve as open-plan/recovery-policy validation only. Recovery lands in L8F/L8G. |
| `crates/storage-next/src/lifecycle/*` | L8A scaffold. | Add state machine, transition result, operation admission, and stronger validation. |

## Scope

L8B implements:

1. a crate-private lifecycle state-machine type;
2. transition triggers and transition outcomes;
3. state policy/admission checks for reads, commits, maintenance, recovery
   steps, health queries, open, and close;
4. invalid transition classification with previous/current/requested state;
5. failed-state facts that preserve the failed phase and reason;
6. close retry and closed-idempotence facts;
7. open-plan validation refinements that are independent of backend
   capabilities;
8. open-outcome validation refinements that are independent of actual recovery;
9. testkit/generated state-transition script coverage;
10. porting-log update for L8B.

L8B does not implement:

1. backend capability validation;
2. cache-mode runtime open or close;
3. durable service assembly;
4. writer guard acquisition;
5. manifest create/load/publish;
6. snapshot load/install/write;
7. WAL replay, tail repair, or L7 replay;
8. maintenance queue execution;
9. flush, checkpoint, compaction, materialization, retention, quarantine, purge,
   repair, or close side effects;
10. public storage API exposure.

## Module Layout

Add one small state module rather than growing `facts.rs`:

```text
crates/storage-next/src/lifecycle/
  state.rs
```

Update `mod.rs` to crate-private re-export the L8B surface.

Expected ownership after L8B:

1. `facts.rs`: storage fact atoms and open-plan shell.
2. `outcome.rs`: open/maintenance/close outcome shells.
3. `state.rs`: transition and operation-admission logic.
4. `tests/mod.rs`: direct tests for config, facts, outcomes, and open-plan
   validation.
5. `tests/state.rs`: direct tests for lifecycle transitions and operation
   admission.

Direct tests are intentionally split under `src/lifecycle/tests/` so later L8
slices do not grow one oversized module.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production items stay
`pub(crate)`.

### `LifecycleStateMachine`

Suggested shape:

```text
LifecycleStateMachine {
  state: LifecycleState,
  failed: Option<LifecycleFailureFact>,
  close_fact: Option<LifecycleCloseFact>,
}
```

Rules:

1. starts in `LifecycleState::New`;
2. transitions only through documented triggers;
3. never skips `Opening` for durable paths;
4. can go `Opening -> Open` only for cache/no-recovery open;
5. can go `Opening -> Recovering` for durable recovery;
6. can go `Recovering -> Open` only through an accepted recovery fact;
7. can go `Open -> Closing`;
8. can go `Closing -> Closed`;
9. `Opening`, `Recovering`, and `Closing` may transition to `Failed`;
10. `Closed -> Closed` close retry is idempotent;
11. `Failed` is sticky until a later slice adds an explicit drop/rebuild/reset
    operation.

### `LifecycleTransitionTrigger`

Suggested variants:

```text
OpenRequested
CacheOpenReady
DurableRecoveryRequired
RecoveryAccepted
CloseRequested
CloseCompleted
CloseRetried
PhaseFailed
```

Rules:

1. triggers are storage lifecycle facts, not public commands;
2. no trigger should mention product open, public maintenance, IPC, follower
   refresh, or engine handles;
3. failure trigger carries a storage phase/reason fact;
4. transition validation should use exhaustive matches.

### `LifecycleTransitionOutcome`

Suggested fields:

```text
from
to
trigger
idempotent
failure
```

Rules:

1. invalid transitions return `LifecycleError::InvalidLifecycleState`;
2. successful idempotent `Closed -> Closed` is explicit;
3. close retry after timeout remains `Closing` and is explicit;
4. failed transition outcome preserves the phase and reason.

### `LifecycleOperationKind`

Suggested variants:

```text
Open
OrdinaryRead
Commit
RecoveryStep
OrdinaryMaintenance
CloseRequiredDrain
HealthQuery
Close
CloseRetry
```

### `LifecycleOperationAdmission`

Suggested shape:

```text
Allowed
Rejected { reason }
```

Rules:

1. `Commit` is allowed only in `Open`;
2. ordinary reads are allowed only in `Open`;
3. recovery steps are allowed only in `Recovering`;
4. ordinary maintenance is allowed only in `Open`;
5. close-required drain is allowed in `Closing`;
6. health queries are allowed in every state;
7. `Close` is accepted in `Open`, `CloseRetry` is accepted in `Closing`, and
   both `Close` and `CloseRetry` are idempotent in `Closed`;
8. `New`, `Opening`, `Recovering`, `Closing`, `Closed`, and `Failed` reject
   commits.

### Open Plan Validation Refinements

L8A already introduced `StorageOpenPlan`.

L8B should refine it without adding backend capabilities:

1. represent created-vs-existing intent explicitly if the current bool shape is
   not precise enough;
2. keep cache-mode durable recovery fallback rejected;
3. keep lossy fallback rejected unless explicit;
4. reject impossible combinations before L8C capability validation runs;
5. validate codec ID before any future durable-service assembly;
6. keep product access mode, IPC, primitive registry, and StrataHub fields
   absent.

If changing `opened_existing: bool` or similar bool fields, prefer explicit
fact enums:

```text
StorageOpenDisposition::Created
StorageOpenDisposition::OpenedExisting
```

This is a fact enum, not a user-facing open policy.

### Open Outcome Validation Refinements

L8B should refine `StorageOpenOutcome` enough for later slices:

1. cache mode cannot report recovered durable visible version;
2. cache mode cannot report durable recovery health degradation;
3. durable modes can report recovered visible version;
4. maintenance readiness can be false without implying product failure;
5. open outcome should distinguish created/opened-existing as raw storage fact;
6. outcome validation must not claim product acceptance of degraded recovery.

## Transition Matrix

Required valid transitions:

| From | Trigger | To | Notes |
|---|---|---|---|
| `New` | `OpenRequested` | `Opening` | No side effects, state only. |
| `Opening` | `CacheOpenReady` | `Open` | Cache path skips recovery. |
| `Opening` | `DurableRecoveryRequired` | `Recovering` | Durable path enters recovery. |
| `Opening` | `PhaseFailed` | `Failed` | Preserve failed phase. |
| `Recovering` | `RecoveryAccepted` | `Open` | Healthy or accepted degraded fact. |
| `Recovering` | `PhaseFailed` | `Failed` | Preserve recovery failure fact. |
| `Open` | `CloseRequested` | `Closing` | No close side effects yet. |
| `Closing` | `CloseCompleted` | `Closed` | Final state fact only. |
| `Closing` | `CloseRetried` | `Closing` | Retryable after timeout. |
| `Closing` | `PhaseFailed` | `Failed` | Preserve close failure fact. |
| `Closed` | `CloseRetried` | `Closed` | Idempotent. |

Required invalid examples:

1. `New -> Open`;
2. `New -> Recovering`;
3. `Opening -> Closed`;
4. `Recovering -> Closing`;
5. `Open -> Recovering`;
6. `Closed -> Open`;
7. `Failed -> Open`;
8. `Failed -> Closed` without explicit future reset/drop operation.

## Operation Admission Matrix

| State | Open | Read | Commit | Recovery step | Ordinary maintenance | Close drain | Health | Close | Close retry |
|---|---|---|---|---|---|---|---|---|---|
| `New` | yes | no | no | no | no | no | yes | no | no |
| `Opening` | no | no | no | no | no | no | yes | no | no |
| `Recovering` | no | no | no | yes | no | no | yes | no | no |
| `Open` | no | yes | yes | no | yes | no | yes | yes | no |
| `Closing` | no | no | no | no | no | yes | yes | no | yes retry |
| `Closed` | no | no | no | no | no | no | yes | yes idempotent | yes idempotent |
| `Failed` | no | no | no | no | no | no | yes | no | no |

This matrix is a storage-runtime admission fact. L9 may map it into public API
behavior later.

## Implementation Steps

### L8B-A: State Module

1. Add `src/lifecycle/state.rs`.
2. Define transition trigger, transition outcome, operation kind, operation
   admission, close retry fact, and failure fact.
3. Add `LifecycleStateMachine::new`.
4. Add `LifecycleStateMachine::state`.
5. Add `LifecycleStateMachine::transition`.
6. Add `LifecycleStateMachine::admit`.
7. Add exhaustive match helpers for transition and admission matrices.

### L8B-B: Error And Fact Refinement

1. Add enough error detail to classify invalid transitions without lower-layer
   strings.
2. Preserve lower-layer source-chain behavior from L8A.
3. Add failure fact type if needed so `Failed` can preserve failed phase and
   reason.
4. Keep displays bounded and product-neutral.

### L8B-C: Open Plan And Outcome Refinement

1. Review bool fields introduced in L8A.
2. Replace any bool that controls behavior with explicit fact enums.
3. Keep simple predicate getters where they are derived facts, not control
   inputs.
4. Strengthen `StorageOpenPlan::validate`.
5. Strengthen `StorageOpenOutcome::new`.
6. Keep capability validation out of L8B.

### L8B-D: Testkit Route

1. Extend `check_lifecycle_scaffold_contract` or add
   `check_lifecycle_state_contract`.
2. Exercise transition scripts generated from input bytes.
3. Count valid transitions, invalid transitions, admission accepts, admission
   rejects, idempotent close retry, and sticky failed-state cases.
4. Keep scripts side-effect free.

### L8B-E: Porting Log

Add `L8B - Lifecycle State And Open Plan` to
`docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` after
implementation.

Record:

1. source evidence read;
2. state/admission matrix delivered;
3. open-plan/outcome validation delivered;
4. behavior deferred to L8C-L8N;
5. tests and commands run;
6. sensitivity probes.

## Source Guard Expectations

Reuse the L8A source guard and add only if needed.

L8B production code must not introduce:

1. engine imports;
2. public API imports;
3. `crate::testkit`;
4. raw filesystem/path/env APIs;
5. product DTO vocabulary;
6. follower mode;
7. StrataHub behavior;
8. public lifecycle exports.

L8B may import lower storage layers only when needed for type facts. It should
not call lower-layer behavior yet.

## Acceptance Criteria

L8B is complete when:

1. lifecycle state transitions are explicit and exhaustive;
2. invalid transitions return typed lifecycle errors;
3. operation admission follows the matrix above;
4. close after `Closed` is idempotent;
5. close retry while `Closing` is represented without pretending close
   completed;
6. failure transitions preserve failed phase/reason;
7. open-plan and open-outcome validation rejects impossible side-effect-free
   combinations;
8. tests cover every state, trigger, and operation admission category;
9. generated lifecycle property coverage exercises state scripts;
10. no product/API/raw-IO boundary guard regresses;
11. no actual open, recovery, maintenance, retention, repair, or close side
    effects are implemented.

## Verification Commands

Required for L8B:

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

Optional after implementation if the generated route becomes nontrivial:

```bash
cargo test -p strata-storage-next --features testkit --locked lifecycle_state
```

## Deferred

1. Backend capability validation: L8C.
2. Cache open/close runtime behavior: L8D.
3. Durable service assembly: L8E.
4. Recovery orchestration: L8F.
5. L7 bootstrap/recovery health finalization: L8G.
6. Maintenance executor: L8H.
7. Flush/checkpoint/compaction/materialization: L8I-L8K.
8. Retention/quarantine/repair: L8L-L8M.
9. Close side effects and guard release: L8N.
10. Crash/fault/fuzz closeout: L8O-L8P.
