# L8C Implementation Plan: Storage Mode Capability Validation

Status: implemented

Parent plan:
`docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-test-plan.md`

## Objective

Add lifecycle-owned storage-mode capability validation before any open/create
side effects.

L8C is the first L8 slice that is allowed to look at backend capability facts.
It must still avoid durable service assembly, manifest creation, WAL open,
writer-lock acquisition, recovery, maintenance, checkpointing, retention,
quarantine, repair, and close side effects.

L8C establishes:

1. a lifecycle capability-validation module;
2. a single mapping from `StorageOpenPlan` storage modes to existing L1/L4
   capability requirements;
3. typed accepted/rejected capability outcomes;
4. a side-effect-free preflight that only reads backend capability facts;
5. testkit/generated coverage for capability matrices and missing-capability
   cases;
6. source-boundary protection against accidentally assembling services in the
   capability slice.

## Inputs

1. `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
2. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
5. `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
6. `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-test-plan.md`
7. `crates/storage-next/src/lifecycle/mod.rs`
8. `crates/storage-next/src/lifecycle/facts.rs`
9. `crates/storage-next/src/lifecycle/outcome.rs`
10. `crates/storage-next/src/lifecycle/error.rs`
11. `crates/storage-next/src/lifecycle/state.rs`
12. `crates/storage-next/src/backend/mod.rs`
13. `crates/storage-next/src/backend/memory.rs`
14. `crates/storage-next/src/backend/local_fs.rs`
15. `crates/storage-next/src/config/mode.rs`
16. `crates/storage-next/src/service/cache_mode_absence_tests.rs`
17. `crates/storage-next/src/backend/conformance.rs`
18. `crates/storage-next/tests/lifecycle_source_guard.rs`

## Existing-Code Source Map

| Current file | L8C evidence | L8C action |
|---|---|---|
| `crates/storage-next/src/backend/mod.rs` | Defines `BackendCapability`, `BackendCapabilities`, `CACHE_MODE_REQUIREMENTS`, `DURABLE_LOCAL_MODE_REQUIREMENTS`, and `OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS`. | Treat this as the capability vocabulary authority. Do not duplicate string lists in lifecycle. |
| `crates/storage-next/src/config/mode.rs` | Defines `StorageModeRequest`, `DurabilityPolicy`, missing-capability computation, and object-durable fence alternatives. | Reuse or bridge this logic from lifecycle. L8C should not fork a second capability matrix. |
| `crates/storage-next/src/backend/memory.rs` | Memory backend satisfies cache-mode requirements but not durable publish/sync/writer-lock requirements. | Use as the concrete cache-mode positive and durable-mode negative evidence. |
| `crates/storage-next/src/backend/local_fs.rs` | Local filesystem reports durable capabilities on supported platforms and already has mode-validation tests. | Use as optional all-features/localfs evidence; do not make L8C depend on localfs for default verification. |
| `crates/storage-next/src/service/cache_mode_absence_tests.rs` | Existing L4 tests prove cache backends reject durable service paths before mutation. | Keep those tests as lower-layer evidence. L8C should add lifecycle preflight tests so these service paths are not reached. |
| `crates/storage-next/src/lifecycle/facts.rs` | Defines lifecycle `StorageMode` and `StorageOpenPlan`. | Add the lifecycle-to-backend request mapping here or in a new capability module. |
| `crates/storage-next/src/lifecycle/state.rs` | Defines operation admission after L8B. | Capability validation should happen during `Opening` before any transition to durable service assembly. |
| `crates/engine/src/database/open.rs` | Old open path validates and assembles many resources in one flow. | Port only the ordering rule: capability validation first. Product access mode and engine wiring remain out of L8. |

## Scope

L8C implements:

1. side-effect-free capability validation for every L8 storage mode;
2. lifecycle mapping from `StorageMode::Cache` to `StorageModeRequest::cache()`;
3. lifecycle mapping from `StorageMode::DurableLocalStandard` to
   `StorageModeRequest::durable_local(DurabilityPolicy::Standard)`;
4. lifecycle mapping from `StorageMode::DurableLocalAlways` to
   `StorageModeRequest::durable_local(DurabilityPolicy::Always)`;
5. lifecycle mapping from `StorageMode::ObjectDurableCandidate` to
   `StorageModeRequest::object_durable_candidate()`;
6. accepted capability facts that can be carried into L8D/L8E open assembly;
7. rejected capability facts with the requested mode and missing capabilities;
8. a validation path that can accept raw `BackendCapabilities` directly;
9. a convenience path that can inspect a `Backend` by calling only
   `backend.capabilities()`;
10. generated/testkit counters for accepted modes, rejected modes,
    missing-capability categories, object-candidate fence variants, and
    side-effect-free preflight;
11. porting-log update requirements for when L8C is implemented.

L8C does not implement:

1. cache runtime open or close;
2. durable local service assembly;
3. writer lock acquisition;
4. database manifest load/create/publish;
5. WAL service open, append, replay, or repair;
6. snapshot/checkpoint load or publish;
7. L6 branch state construction;
8. L7 commit runtime construction or replay;
9. maintenance task execution;
10. retention, quarantine, purge, repair, or close side effects;
11. object-durable production durability claims;
12. public L9 storage API exposure.

## Design Decisions

### Reuse The Existing Capability Matrix

L8C should not invent a second list of required capabilities.

The backend layer already owns the capability vocabulary and requirement
constants:

1. `CACHE_MODE_REQUIREMENTS`;
2. `DURABLE_LOCAL_MODE_REQUIREMENTS`;
3. `OBJECT_DURABLE_CANDIDATE_BASE_REQUIREMENTS`;
4. object-candidate fence alternatives through either `ConditionalPublish` or
   `ConditionalCreate + ConditionalUpdate`.

The lifecycle layer should call or wrap `StorageModeRequest` rather than
copying the capability matrix into L8.

### Capability Validation Is A Preflight

Capability validation must run before:

1. writer-lock acquisition;
2. manifest object creation;
3. WAL object creation or open;
4. snapshot/checkpoint object reads or writes;
5. table object publication;
6. quarantine inventory reads or writes;
7. temporary object creation;
8. L6/L7 runtime mutation.

The safest L8C implementation takes `BackendCapabilities` as input and returns a
typed decision. A convenience helper may accept `&dyn Backend`, but it must call
only `capabilities()`.

### Durable Standard And Durable Always Share Backend Requirements

For L8C, durable standard and durable always require the same backend capability
set. Their behavioral difference is later:

1. durable standard can append without forcing every commit durable;
2. durable always requires L7/L4 commit paths to force durability for each
   commit.

L8C should preserve the requested `DurabilityPolicy` in the accepted facts so
L8E/L8G can select the correct durable service and L7 policy later.

### Object Durable Candidate Is Explicitly Experimental

`StorageMode::ObjectDurableCandidate` can validate an object-store capability
shape, but L8C must not promote it to production durable local behavior.

Accepted object-candidate facts should record:

1. base object capabilities are present;
2. consistent list and monotonic metadata are present;
3. fenced publication is available through either conditional publish or the
   conditional create/update pair;
4. the mode remains `ObjectDurableCandidate`.

L8E/L8F may still defer production object-durable open/recovery until the
conditional-publish and object-retention contracts are complete.

## Module Layout

Add one lifecycle-owned capability module:

```text
crates/storage-next/src/lifecycle/
  capability.rs
```

Update `mod.rs` to crate-private re-export the L8C surface.

Tests should stay split:

```text
crates/storage-next/src/lifecycle/tests/
  capability.rs
```

Expected ownership after L8C:

1. `facts.rs`: storage fact atoms and open-plan shell.
2. `outcome.rs`: open/maintenance/close outcome shells.
3. `state.rs`: transition and operation-admission logic.
4. `capability.rs`: storage-mode capability preflight and accepted/rejected
   capability facts.
5. `tests/mod.rs`: config, facts, outcomes, and open-plan validation.
6. `tests/state.rs`: lifecycle transition and admission tests.
7. `tests/capability.rs`: storage-mode capability validation tests.

L8C splits the lifecycle testkit into `src/testkit/lifecycle/mod.rs` plus
focused `capability.rs` and `outcome.rs` submodules so capability counters do
not push the scaffold file past the local maintainability threshold.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production items stay
`pub(crate)`.

### `LifecycleCapabilityValidator`

Suggested shape:

```text
LifecycleCapabilityValidator
  validate_open_plan(plan, capabilities) -> LifecycleResult<LifecycleCapabilityOutcome>
  validate_backend_for_open(plan, backend) -> LifecycleResult<LifecycleCapabilityOutcome>
```

Rules:

1. `validate_open_plan` is pure over `StorageOpenPlan` plus
   `BackendCapabilities`;
2. `validate_backend_for_open` calls only `backend.capabilities()`;
3. validation delegates missing-capability computation to
   `StorageModeRequest`;
4. validation returns accepted facts or a typed lifecycle capability error;
5. validation must not allocate lower-layer services.

An associated-free function is also acceptable if it keeps the same ownership:

```text
validate_storage_mode_capabilities(plan, capabilities)
validate_backend_capabilities_for_open(plan, backend)
```

### `LifecycleCapabilityOutcome`

Suggested fields:

```text
LifecycleCapabilityOutcome {
  storage_mode: StorageMode,
  request: StorageModeRequest,
  capabilities: BackendCapabilities,
  required: Vec<BackendCapability>,
  missing: Vec<BackendCapability>, // empty when accepted
  durability_policy: Option<DurabilityPolicy>,
  object_candidate_fence: Option<ObjectDurableFenceMode>,
}
```

Rules:

1. accepted outcomes have empty `missing`;
2. rejected outcomes should expose the missing set through a typed error or a
   report object;
3. the outcome preserves the requested mode, not only the backend facts;
4. durable standard and durable always preserve their policy;
5. cache outcomes must not include durable policy;
6. object-candidate outcomes must remain explicitly candidate-tagged.

### `ObjectDurableFenceMode`

Suggested variants:

```text
ConditionalPublish
ConditionalCreateUpdate
```

Rules:

1. `ConditionalPublish` is selected when the backend reports
   `BackendCapability::ConditionalPublish`;
2. `ConditionalCreateUpdate` is selected when the backend reports both
   `BackendCapability::ConditionalCreate` and
   `BackendCapability::ConditionalUpdate`;
3. if both are present, choose a deterministic preferred variant and document it
   in tests;
4. if only one side of the create/update pair is present, reject with the
   missing counterpart and the conditional-publish alternative still absent.

### Capability Mismatch Error

`LifecycleError::CapabilityMismatch` carries typed facts; tests must not parse
display text for capability validation.

Acceptable shapes:

1. extend `LifecycleError::CapabilityMismatch` to include requested mode,
   required capabilities, and missing capabilities;
2. add `LifecycleCapabilityMismatch` as an owned source/details type;
3. return a `LifecycleCapabilityReport` from validation failures while keeping
   display text storage-shaped.

The final implementation must make tests able to assert the exact missing
capabilities without parsing an error string.

## Capability Matrix

### Cache Mode

Required capabilities:

1. `ReadObject`;
2. `ReadRange`;
3. `WriteObject`;
4. `DeleteObject`;
5. `ListPrefix`.

Cache mode must not require:

1. `ObjectMetadata`;
2. `AppendObject`;
3. `DurablePublish`;
4. `DurableSync`;
5. `SingleWriterLock`;
6. `ConditionalPublish`;
7. `ConsistentList`;
8. `MonotonicMetadata`.

### Durable Local Standard

Required capabilities:

1. `ReadObject`;
2. `ReadRange`;
3. `WriteObject`;
4. `DeleteObject`;
5. `ListPrefix`;
6. `ObjectMetadata`;
7. `AppendObject`;
8. `DurablePublish`;
9. `DurableSync`;
10. `SingleWriterLock`.

### Durable Local Always

Required capabilities are the same as durable local standard. The accepted
outcome must preserve `DurabilityPolicy::Always`.

L8C should not try to prove per-commit fsync here. That is L7/L8E behavior over
the WAL service.

### Object Durable Candidate

Required base capabilities:

1. `ReadObject`;
2. `ReadRange`;
3. `WriteObject`;
4. `DeleteObject`;
5. `ListPrefix`;
6. `ObjectMetadata`;
7. `ConsistentList`;
8. `MonotonicMetadata`.

Required fencing alternative:

1. `ConditionalPublish`; or
2. `ConditionalCreate + ConditionalUpdate`.

L8C may accept the candidate shape, but it must not claim production durable
local semantics.

## Implementation Steps

### L8C-A: Capability Module

1. Add `crates/storage-next/src/lifecycle/capability.rs`.
2. Add crate-private exports from `lifecycle/mod.rs`.
3. Keep the module free of service, layout-constructor, WAL, manifest,
   snapshot, table, branch, and commit imports except for type facts explicitly
   needed by the validation surface.
4. Import `Backend`, `BackendCapabilities`, and `BackendCapability` only as
   capability facts.
5. Import `StorageModeRequest` and `DurabilityPolicy` from `config::mode` to
   avoid duplicating the matrix.

### L8C-B: Mode Mapping

1. Map `StorageMode::Cache` to `StorageModeRequest::cache()`.
2. Map `StorageMode::DurableLocalStandard` to durable local standard.
3. Map `StorageMode::DurableLocalAlways` to durable local always.
4. Map `StorageMode::ObjectDurableCandidate` to object durable candidate.
5. Add an exhaustive-match guard so future storage modes cannot skip capability
   validation.

### L8C-C: Accepted And Rejected Facts

1. Return accepted capability facts that include mode, policy, capabilities, and
   object-candidate fence mode when applicable.
2. Return rejected facts or errors that include the missing capabilities.
3. Keep display/debug storage-shaped and bounded.
4. Do not use product open wording such as "database cannot open" in production
   lifecycle errors.

### L8C-D: Side-Effect-Free Backend Preflight

1. Add a helper that accepts `&dyn Backend` and calls only `capabilities()`.
2. Do not call backend metadata, list, read, write, publish, append, delete,
   conditional update, or writer-lock methods.
3. Tests should use a counting backend that panics or records if any method
   other than `capabilities()` is called.

### L8C-E: Testkit And Generated Counters

Extend the lifecycle testkit with counters for:

1. accepted capability validations;
2. rejected capability validations;
3. cache-mode validations;
4. durable-standard validations;
5. durable-always validations;
6. object-candidate validations;
7. missing-capability categories;
8. object-candidate fence variants;
9. side-effect-free backend preflight cases;
10. input-derived capability cases.

The generated route should decode input bytes into mode and capability masks,
run validation, and count typed accepted/rejected outcomes.

### L8C-F: Porting Log

When implementing L8C, add a porting-log entry with:

1. implemented files;
2. reused lower-layer capability constants;
3. exact mode mapping;
4. accepted/rejected test cases;
5. side-effect-free preflight evidence;
6. sensitivity-probe results;
7. command output.

## Source Guard Policy

L8C production lifecycle code may import:

1. `crate::backend::{Backend, BackendCapabilities, BackendCapability}`;
2. backend capability requirement constants;
3. `crate::config::mode::{StorageModeRequest, DurabilityPolicy}`;
4. lifecycle-local facts, errors, and results.

L8C production lifecycle code must not import:

1. `crate::service::*`;
2. `crate::layout::ObjectLayout`;
3. `crate::format::*`;
4. `crate::table::*`;
5. `crate::branch::*`;
6. `crate::commit::*`;
7. engine crates;
8. raw filesystem/path/environment APIs;
9. product or StrataHub vocabulary.

Later L8 slices will need service, branch, and commit imports. L8C-specific
source checks should be local to this slice or written so they can evolve when
L8D-L8G deliberately add those dependencies.

## Deferred To Later Slices

1. Cache-mode runtime open and close: L8D.
2. Durable service assembly and writer-lock acquisition: L8E.
3. Manifest load/create/publish: L8E.
4. WAL, snapshot, table, timeline, and quarantine recovery: L8F.
5. L7 replay/bootstrap and recovery health finalization: L8G.
6. Maintenance executor and task execution: L8H-L8K.
7. Retention, quarantine, purge, repair, close, and crash assurance: L8L-L8P.

## Minimum Verification

Run at minimum:

```text
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

If localfs-specific capability evidence is added, also run:

```text
cargo test -p strata-storage-next --all-features --locked lifecycle
```

Do not add closeout tests that only prove plan documents exist. Tests should
assert lifecycle capability behavior, source boundaries, and generated coverage.

## Close Criteria

L8C is complete when:

1. every storage mode maps to exactly one backend capability request;
2. missing capabilities are reported as typed facts, not parsed display text;
3. cache mode does not require durable capabilities;
4. durable local standard and always require durable local capabilities;
5. durable always preserves the always policy for later L7/L8E wiring;
6. object-durable candidate accepts both documented fence alternatives;
7. object-durable candidate remains explicitly experimental/candidate-tagged;
8. validation can run from raw capability facts without a backend object;
9. backend preflight calls only `capabilities()`;
10. capability mismatch fails before any durable object/service side effects;
11. generated lifecycle properties exercise accepted and rejected capability
    cases;
12. source guards still prevent product, engine, raw IO, and accidental service
    assembly drift.
